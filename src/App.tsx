import { useCallback, useEffect, useReducer, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { call, message, terminal } from "./api";
import type {
  AppInfo,
  CollectionSeed,
  Settings,
  TaskSnapshot,
  NativeStatus,
  SystemVoice,
} from "./types";
import { CollectionModal } from "./components/CollectionModal";
import { LibraryView } from "./components/LibraryView";
import { EpisodesView } from "./components/EpisodesView";
import { ReviewView } from "./components/ReviewView";
import { SyncView } from "./components/SyncView";
import { TasksView } from "./components/TasksView";
import { KnownTargetsManager } from "./components/KnownTargetsManager";
import { collectionQueue } from "./collectionQueue";

type Tab =
  "library" | "episodes" | "reviews" | "leech" | "sync" | "tasks" | "settings";
export default function App() {
  const [tab, setTab] = useState<Tab>("library"),
    [info, setInfo] = useState<AppInfo | null>(null),
    [tasks, setTasks] = useState<TaskSnapshot[]>([]),
    [settings, setSettings] = useState<Settings | null>(null);
  const [collection, dispatchCollection] = useReducer(collectionQueue, {
    current: null,
    pending: [],
  });
  const seed = collection.current,
    pendingSeed = collection.pending[0];
  const setSeed = (value: CollectionSeed | null) =>
    dispatchCollection(
      value ? { type: "open", seed: value } : { type: "close" },
    );
  const [error, setError] = useState(""),
    [notice, setNotice] = useState(""),
    [refreshKey, setRefreshKey] = useState(0);
  const [voices, setVoices] = useState<SystemVoice[]>([]),
    [native, setNative] = useState<NativeStatus | null>(null);
  const [skippedCapture, setSkippedCapture] = useState<CollectionSeed | null>(
    null,
  );
  const refresh = useCallback(async () => {
    const [nextInfo, nextTasks] = await Promise.all([
      call<AppInfo>("app_info"),
      call<TaskSnapshot[]>("tasks_list"),
    ]);
    setInfo(nextInfo);
    setTasks(nextTasks);
  }, []);
  useEffect(() => {
    refresh().catch((error) => setError(message(error)));
    call<Settings>("settings_get")
      .then(setSettings)
      .catch((error) => setError(message(error)));
    call<SystemVoice[]>("speech_voices")
      .then(setVoices)
      .catch(() => setVoices([]));
    call<NativeStatus>("native_status")
      .then(setNative)
      .catch((error) => setError(message(error)));
  }, [refresh]);
  useEffect(() => {
    let disposed = false;
    const stops: (() => void)[] = [];
    const keep = (stop: () => void) => {
      if (disposed) stop();
      else stops.push(stop);
    };
    listen<CollectionSeed>("capture_completed", (event) => {
      setError("");
      if (event.payload.alreadyKnown) {
        setSkippedCapture(event.payload);
        return;
      }
      dispatchCollection({ type: "receive", seed: event.payload });
    })
      .then(keep)
      .catch((error) => setError(message(error)));
    listen<unknown>("capture_failed", (event) =>
      setError(message(event.payload)),
    )
      .then(keep)
      .catch((error) => setError(message(error)));
    return () => {
      disposed = true;
      stops.forEach((stop) => stop());
    };
  }, []);
  useEffect(() => {
    const timer = setInterval(() => refresh().catch(() => {}), 1000);
    return () => clearInterval(timer);
  }, [refresh]);
  const active = tasks.filter((task) => !terminal(task));
  const report = (error: unknown) => setError(message(error));
  const saved = () => {
    setSeed(null);
    setNotice("收录成功，例句与原声已按选择保存。");
    setRefreshKey((value) => value + 1);
    refresh().catch(report);
  };
  return (
    <div className="app-shell">
      <aside className="sidebar">
        <div className="brand">
          <span className="brand-mark">SV</span>
          <span>
            Subtitle
            <br />
            VocabularyList
          </span>
        </div>
        <p className="sidebar-subtitle">每个词，都有自己的对白。</p>
        <nav aria-label="主导航">
          {(
            [
              ["library", "我的单词本"],
              ["episodes", "观看前预习"],
              ["reviews", "主动回忆测验"],
              ["leech", "易忘词专项"],
              ["sync", "站点同步"],
              ["tasks", "后台任务"],
              ["settings", "本地设置"],
            ] as [Tab, string][]
          ).map(([key, label]) => (
            <button
              className={tab === key ? "active" : ""}
              key={key}
              onClick={() => {
                setTab(key);
                setError("");
              }}
            >
              {label}
              {key === "tasks" && active.length > 0 && (
                <span className="nav-count">{active.length}</span>
              )}
            </button>
          ))}
        </nav>
        <div className="sidebar-footer">
          <span className="offline-dot" />
          {settings?.offlineMode ? "离线模式" : "按需联网"}
          <small>0.1</small>
        </div>
      </aside>
      <main className="workspace">
        <header className="workspace-header">
          <div>
            <p className="eyebrow">英语学习，从真实语境开始</p>
            <h1>
              {
                {
                  library: "我的单词本",
                  episodes: "观看前预习",
                  reviews: "主动回忆测验",
                  leech: "易忘词专项",
                  sync: "站点同步",
                  tasks: "后台任务",
                  settings: "本地设置",
                }[tab]
              }
            </h1>
          </div>
          <div className="actions">
            <button
              className="secondary"
              onClick={() => call("capture_ocr").catch(report)}
            >
              截图收录
            </button>
            <button
              className="primary"
              onClick={() => setSeed({ text: "", kind: "word", context: "" })}
            >
              ＋ 收录词条
            </button>
          </div>
        </header>
        {error && (
          <div className="error banner" role="alert">
            {error}
            <button className="text-button" onClick={() => setError("")}>
              关闭
            </button>
          </div>
        )}
        {notice && (
          <div className="notice banner" role="status">
            {notice}
            <button className="text-button" onClick={() => setNotice("")}>
              关闭
            </button>
          </div>
        )}
        {skippedCapture && (
          <div className="notice banner" role="status">
            <span>已跳过已掌握内容：{skippedCapture.text.slice(0, 80)}</span>
            <button
              onClick={() => {
                setSeed(skippedCapture);
                setSkippedCapture(null);
              }}
            >
              仍要查看
            </button>
            <button
              className="text-button"
              onClick={() => setSkippedCapture(null)}
            >
              关闭
            </button>
          </div>
        )}
        {pendingSeed && !seed && (
          <div className="notice banner">
            <span>
              有 {collection.pending.length} 条采集结果待确认：
              {pendingSeed.text.slice(0, 80)}
            </span>
            <button
              onClick={() => {
                dispatchCollection({ type: "next" });
              }}
            >
              打开确认
            </button>
            <button onClick={() => dispatchCollection({ type: "discard" })}>
              放弃这一条
            </button>
          </div>
        )}
        {active.length > 0 && tab !== "tasks" && (
          <button className="running-summary" onClick={() => setTab("tasks")}>
            {active[0].message}{" "}
            {active[0].total > 0
              ? `${active[0].current}/${active[0].total}`
              : ""}
            <span>查看任务 →</span>
          </button>
        )}
        {tab === "library" && (
          <LibraryView
            info={info}
            refreshKey={refreshKey}
            collect={setSeed}
            report={report}
          />
        )}
        {tab === "episodes" && (
          <EpisodesView
            sourceCount={info?.sourceCount ?? 0}
            refreshKey={refreshKey}
            collect={setSeed}
            report={report}
            notify={setNotice}
          />
        )}
        {(tab === "reviews" || tab === "leech") && (
          <ReviewView
            key={tab}
            special={tab === "leech"}
            refreshKey={refreshKey}
          />
        )}
        {tab === "sync" && (
          <SyncView
            onChanged={() => {
              setRefreshKey((k) => k + 1);
              refresh().catch(report);
            }}
          />
        )}
        {tab === "tasks" && (
          <TasksView
            tasks={tasks}
            cancel={(id) =>
              call("task_cancel", { taskId: id }).then(refresh).catch(report)
            }
          />
        )}
        {tab === "settings" && settings && (
          <section className="settings-panel">
            <KnownTargetsManager
              refreshKey={refreshKey}
              onChanged={() => setRefreshKey((value) => value + 1)}
            />
            <h2>划词与系统语音</h2>
            <p className="muted">
              在原应用选择文字后按快捷键。本次文字将在收录窗口中确认；不读取剪贴板。关闭主窗口后应用留在托盘，托盘菜单可打开或退出。
            </p>
            <label>
              划词快捷键
              <input
                value={settings.selectionShortcut}
                onChange={(event) =>
                  setSettings({
                    ...settings,
                    selectionShortcut: event.target.value,
                  })
                }
              />
            </label>
            <p className={native?.error ? "error" : "muted"}>
              {native?.error ??
                (native?.selectionRegistered
                  ? "划词快捷键已注册"
                  : "划词快捷键未启用")}
              。留空可禁用快捷键。
            </p>
            <label>
              截图快捷键
              <input
                value={settings.ocrShortcut}
                onChange={(event) =>
                  setSettings({ ...settings, ocrShortcut: event.target.value })
                }
              />
            </label>
            <p className="muted">
              {native?.ocrRegistered ? "截图快捷键已注册" : "截图快捷键未启用"}
              。识别在本机执行；拖选区域后进入收录确认。
            </p>
            <label>
              Windows 英语声音
              <select
                value={settings.systemVoice}
                onChange={(event) =>
                  setSettings({ ...settings, systemVoice: event.target.value })
                }
              >
                <option value="">使用可用的本地英语声音</option>
                {voices.map((voice) => (
                  <option key={voice.id} value={voice.id}>
                    {voice.name}
                  </option>
                ))}
              </select>
            </label>
            {!voices.length && (
              <p className="muted">
                未发现可用的系统英语声音。已有原声仍可播放。
              </p>
            )}
            <h2>本地模型</h2>
            <p className="muted">
              解释通过本机服务执行；服务不可用时仍可收录原文和浏览已有资料。
            </p>
            <label>
              服务地址
              <input
                value={settings.modelUrl}
                onChange={(event) =>
                  setSettings({ ...settings, modelUrl: event.target.value })
                }
              />
            </label>
            <label>
              模型标识
              <input
                value={settings.modelName}
                onChange={(event) =>
                  setSettings({ ...settings, modelName: event.target.value })
                }
              />
            </label>
            <label className="check-label">
              <input
                type="checkbox"
                checked={settings.offlineMode}
                onChange={(event) =>
                  setSettings({
                    ...settings,
                    offlineMode: event.target.checked,
                  })
                }
              />
              离线模式，本机模型仍可使用
            </label>
            <label>
              英语练习站点地址
              <input
                value={settings.siteUrl}
                onChange={(event) =>
                  setSettings({ ...settings, siteUrl: event.target.value })
                }
              />
            </label>
            <button
              className="primary"
              onClick={() =>
                call<Settings>("settings_update", { settings })
                  .then((value) => {
                    setSettings(value);
                    setNotice("本地设置已保存。");
                    call<NativeStatus>("native_status")
                      .then(setNative)
                      .catch(report);
                  })
                  .catch(report)
              }
            >
              保存设置
            </button>
            <h3>数据位置</h3>
            <p className="directory">{info?.dataDirectory}</p>
            <p className="muted">
              原声独立保存，移动剧集文件不会删除已经收录的音频。
            </p>
          </section>
        )}
      </main>
      {seed && (
        <CollectionModal
          seed={seed}
          onClose={() => setSeed(null)}
          onSaved={saved}
          onMarked={() => {
            setSeed(null);
            setNotice("已标记我已掌握，后续候选与采集确认默认跳过。");
            setRefreshKey((value) => value + 1);
          }}
        />
      )}
    </div>
  );
}
