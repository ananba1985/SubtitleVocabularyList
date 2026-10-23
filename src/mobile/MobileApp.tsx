import { useCallback, useEffect, useReducer, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { call, message, terminal, uid } from "../api";
import type { CommandResult } from "../commands";
import type { AppInfo, CollectionSeed, Settings, TaskSnapshot } from "../types";
import { collectionQueue } from "../collectionQueue";
import { CollectionModal } from "../components/CollectionModal";
import { LibraryView } from "../components/LibraryView";
import { TasksView } from "../components/TasksView";
import { RuntimeFeatures } from "../runtimeFeatures";
import { MobileAccount } from "./MobileAccount";
import { ReviewView } from "../components/ReviewView";
import { MobileSpeechSettings } from "./MobileSpeechSettings";
import { MobileWatch } from "./MobileWatch";
import { useAutomaticSync } from "./useAutomaticSync";
import { useMobileInputs } from "./useMobileInputs";
import { MobileOcrModal } from "./MobileOcrModal";
import "./mobile.css";

export default function MobileApp() {
  const [page, setPage] = useState<"library" | "watch" | "reviews" | "account">(
    "library",
  );
  const [special, setSpecial] = useState(false);
  const [info, setInfo] = useState<AppInfo | null>(null);
  const [settings, setSettings] = useState<Settings | null>(null);
  const [capabilities, setCapabilities] =
    useState<CommandResult<"app_capabilities"> | null>(null);
  const [tasks, setTasks] = useState<TaskSnapshot[]>([]);
  const [refreshKey, setRefreshKey] = useState(0);
  const [error, setError] = useState("");
  const [queue, dispatch] = useReducer(collectionQueue, {
    current: null,
    pending: [],
  });
  const inputs = useMobileInputs(queue, dispatch, !!info);
  const draftChanged = useCallback(
    (seed: CollectionSeed) => dispatch({ type: "replace", seed }),
    [],
  );
  const [openRequest, setOpenRequest] = useState<{
    uri: string;
    nonce: string;
  }>();
  const refresh = useCallback(async () => {
    await call("app_ready");
    const [nextInfo, nextSettings, nextCapabilities, nextTasks] =
      await Promise.all([
        call("app_info"),
        call("settings_get"),
        call("app_capabilities"),
        call("tasks_list"),
      ]);
    setInfo(nextInfo);
    setSettings(nextSettings);
    setCapabilities(nextCapabilities);
    setTasks(nextTasks);
  }, []);
  useEffect(() => {
    refresh().catch((e) => setError(message(e)));
  }, [refresh]);
  const changed = useCallback(() => {
    setRefreshKey((value) => value + 1);
    refresh().catch((e) => setError(message(e)));
  }, [refresh]);
  const automaticSync = useAutomaticSync(!!info, changed);
  useEffect(() => {
    let disposed = false;
    const stop = listen<TaskSnapshot>("task_updated", ({ payload }) => {
      if (disposed) return;
      refresh().catch((e) => setError(message(e)));
      if (terminal(payload)) setRefreshKey((value) => value + 1);
    });
    const receive = listen<CollectionSeed>(
      "capture_completed",
      ({ payload }) => {
        if (!disposed)
          dispatch({ type: "receive", seed: { ...payload, draftId: uid() } });
      },
    );
    return () => {
      disposed = true;
      stop.then((fn) => fn());
      receive.then((fn) => fn());
    };
  }, [refresh]);
  const collect = (seed: CollectionSeed) =>
    dispatch({
      type: "open",
      seed: { ...seed, draftId: seed.draftId ?? uid() },
    });
  async function pickImage() {
    try {
      const image = await call("mobile_image_pick");
      collect({ text: "", context: "", kind: "word", image });
    } catch (e) {
      setError(message(e));
    }
  }
  async function openSharedVideo(uri: string) {
    try {
      await call("mobile_shared_video", { uri });
      dispatch({ type: "close" });
      setOpenRequest({ uri, nonce: uid() });
      setPage("watch");
    } catch (e) {
      setError(message(e));
    }
  }
  async function setOffline(offlineMode: boolean) {
    if (!settings) return;
    try {
      setSettings(
        await call("settings_update", {
          settings: { ...settings, offlineMode },
        }),
      );
      setRefreshKey((value) => value + 1);
      if (!offlineMode) automaticSync.request();
    } catch (e) {
      setError(message(e));
    }
  }
  return (
    <RuntimeFeatures.Provider
      value={{
        mobile: true,
        systemSpeech: capabilities?.systemSpeech ?? false,
        knownTargetPreferences: capabilities?.knownTargetPreferences ?? false,
      }}
    >
      <div className="mobile-app">
        <header className="mobile-header">
          <div>
            <span className="eyebrow">SubtitleVocabularyList</span>
            <h1>
              {page === "library"
                ? "我的单词本"
                : page === "watch"
                  ? "边看边学"
                  : page === "reviews"
                    ? "复习"
                    : "我的"}
            </h1>
          </div>
          <div className="actions">
            <button disabled={!inputs.loaded} onClick={pickImage}>
              图片 OCR
            </button>
            <button
              disabled={!inputs.loaded}
              onClick={() => collect({ text: "", context: "", kind: "word" })}
            >
              ＋ 收录
            </button>
          </div>
        </header>
        <main>
          {error && (
            <div className="error banner" role="alert">
              {error}
              <button
                onClick={() => {
                  setError("");
                  refresh().catch((e) => setError(message(e)));
                }}
              >
                重试
              </button>
            </div>
          )}
          {inputs.error && (
            <p className="error" role="alert">
              {inputs.error}
            </p>
          )}
          {!info && !error && <p role="status">正在打开手机本地词库…</p>}
          {page === "library" && info && (
            <LibraryView
              compact
              info={info}
              refreshKey={refreshKey}
              collect={collect}
              onChanged={() => {
                changed();
                automaticSync.request();
              }}
              report={(e) => setError(message(e))}
            />
          )}
          {page === "watch" && info && settings && (
            <MobileWatch
              collect={collect}
              covered={!!queue.current}
              settings={settings}
              saved={setSettings}
              openRequest={openRequest}
            />
          )}
          {page === "reviews" && info && (
            <>
              <div className="toolbar" aria-label="复习方式">
                <button
                  aria-pressed={!special}
                  onClick={() => setSpecial(false)}
                >
                  计划复习
                </button>
                <button aria-pressed={special} onClick={() => setSpecial(true)}>
                  易忘词专项
                </button>
              </div>
              <ReviewView
                key={String(special)}
                special={special}
                refreshKey={refreshKey}
                onChanged={() => {
                  changed();
                  automaticSync.request();
                }}
              />
            </>
          )}
          {page === "account" && (
            <>
              <section className="mobile-card">
                <h2>本机学习</h2>
                <p>
                  {info?.entryCount ?? 0} 个词条 · v{info?.version ?? ""}
                </p>
                <label className="check-label">
                  <input
                    type="checkbox"
                    checked={settings?.offlineMode ?? false}
                    onChange={(e) => setOffline(e.target.checked)}
                  />
                  手动离线
                </label>
                <p className="muted">
                  词库和学习记录保存在手机上，与桌面客户端独立运行。
                </p>
                {info && (
                  <p>
                    离线词库：
                    {info.dictionaryWarnings.length
                      ? "请查看下方资源提示"
                      : "已准备"}{" "}
                    · 英语 OCR 资源：已准备
                  </p>
                )}
                {!!info?.dictionaryWarnings.length && (
                  <p className="notice">{info.dictionaryWarnings.join(" ")}</p>
                )}
              </section>
              {settings && capabilities?.systemSpeech && (
                <MobileSpeechSettings settings={settings} saved={setSettings} />
              )}
              {info && settings && capabilities?.synchronization && (
                <MobileAccount
                  settings={settings}
                  refreshKey={refreshKey}
                  changed={changed}
                  requestSync={automaticSync.request}
                  autoNotice={automaticSync.notice}
                />
              )}
              <TasksView
                tasks={tasks}
                refreshKey={refreshKey}
                sourceCount={info?.sourceCount ?? 0}
                cancel={(taskId) => {
                  call("task_cancel", { taskId })
                    .then(refresh)
                    .catch((e) => setError(message(e)));
                }}
              />
            </>
          )}
          {!queue.current && queue.pending.length > 0 && (
            <button onClick={() => dispatch({ type: "next" })}>
              继续确认待收录内容（{queue.pending.length}）
            </button>
          )}
        </main>
        <nav className="mobile-navigation" aria-label="主导航">
          <button
            aria-current={page === "library" ? "page" : undefined}
            onClick={() => setPage("library")}
          >
            单词本
          </button>
          <button
            aria-current={page === "watch" ? "page" : undefined}
            onClick={() => setPage("watch")}
          >
            观看
          </button>
          <button
            aria-current={page === "reviews" ? "page" : undefined}
            onClick={() => setPage("reviews")}
          >
            复习
          </button>
          <button
            aria-current={page === "account" ? "page" : undefined}
            onClick={() => setPage("account")}
          >
            我的
          </button>
        </nav>
        {queue.current &&
          (queue.current.image || queue.current.imageSource ? (
            <MobileOcrModal
              key={queue.current.draftId}
              seed={queue.current}
              changed={draftChanged}
              close={() => dispatch({ type: "close" })}
            />
          ) : queue.current.videoUri ? (
            <div className="modal-backdrop">
              <section
                className="modal"
                role="dialog"
                aria-modal="true"
                aria-label="打开分享视频"
              >
                <h2>打开分享视频</h2>
                <p>
                  系统分享的文件权限可能是临时的。以后无法重开时，请通过“打开本地视频”重新授权。
                </p>
                <footer>
                  <button onClick={() => dispatch({ type: "close" })}>
                    放弃
                  </button>
                  <button
                    onClick={() => openSharedVideo(queue.current!.videoUri!)}
                  >
                    在播放器打开
                  </button>
                </footer>
              </section>
            </div>
          ) : (
            <CollectionModal
              key={queue.current.draftId}
              seed={queue.current}
              onDraftChanged={draftChanged}
              onClose={() => dispatch({ type: "close" })}
              onMarked={() => dispatch({ type: "close" })}
              onSaved={() => {
                dispatch({ type: "close" });
                setRefreshKey((value) => value + 1);
                refresh().catch((e) => setError(message(e)));
              }}
            />
          ))}
      </div>
    </RuntimeFeatures.Provider>
  );
}
