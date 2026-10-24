import { useCallback, useEffect, useReducer, useRef, useState } from "react";
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
import { EpisodesView, type EpisodePanel } from "./components/EpisodesView";
import { ReviewView } from "./components/ReviewView";
import { AssessmentView } from "./components/AssessmentView";
import { SyncView } from "./components/SyncView";
import { TasksView } from "./components/TasksView";
import { KnownTargetsManager } from "./components/KnownTargetsManager";
import { PracticeView, type PracticePanel } from "./components/PracticeView";
import { collectionQueue } from "./collectionQueue";
import { ReplacementSettings } from "./components/ReplacementSettings";
import { StartupSettings } from "./components/StartupSettings";
import { WatchView } from "./components/WatchView";
import { SubtitleServiceSettings } from "./components/OnlineSubtitlePanel";
import { applyTaskUpdate, type TaskUpdate } from "./taskUpdates";

type Tab =
  | "library"
  | "episodes"
  | "watch"
  | "practice"
  | "reviews"
  | "assessment"
  | "leech"
  | "sync"
  | "tasks"
  | "settings";
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
  const [watchFullscreen, setWatchFullscreen] = useState(false);
  const [voices, setVoices] = useState<SystemVoice[]>([]),
    [native, setNative] = useState<NativeStatus | null>(null);
  const [skippedCapture, setSkippedCapture] = useState<CollectionSeed | null>(
    null,
  );
  const [episodePanel, setEpisodePanel] = useState<EpisodePanel>(null);
  const [practicePanel, setPracticePanel] = useState<PracticePanel>(null);
  const taskBuffer = useRef<Map<string, TaskUpdate> | null>(null);
  const startupReported = useRef(false);
  useEffect(() => {
    if (!info || !settings || startupReported.current) return;
    startupReported.current = true;
    void call("app_ready").catch((error) => console.warn("启动后维护未开始", error));
  }, [info, settings]);
  useEffect(() => {
    let disposed = false;
    const receive = () => {
      if (!disposed) setTab("watch");
    };
    const stop = listen("watch_files_pending", receive)
      .then(async (unlisten) => {
        if (disposed) {
          unlisten();
          return;
        }
        if (await call("watch_files_pending")) receive();
        return unlisten;
      })
      .catch((e) => {
        if (!disposed) setError(message(e));
      });
    return () => {
      disposed = true;
      void stop.then((unlisten) => unlisten?.());
    };
  }, []);
  const refresh = useCallback(async () => {
    const updates = new Map<string, TaskUpdate>();
    taskBuffer.current = updates;
    try {
      const [nextInfo, nextTasks] = await Promise.all([
        call("app_info"),
        call("tasks_list"),
      ]);
      if (taskBuffer.current !== updates) return;
      setInfo(nextInfo);
      setTasks([...updates.values()].reduce(applyTaskUpdate, nextTasks));
      for (const task of nextTasks)
        window.dispatchEvent(new CustomEvent("svl_task_updated", { detail: task }));
    } finally {
      if (taskBuffer.current === updates) taskBuffer.current = null;
    }
  }, []);
  useEffect(() => {
    setNotice("");
    call("settings_get")
      .then(setSettings)
      .catch((error) => setError(message(error)));
    call("native_status")
      .then(setNative)
      .catch((error) => setError(message(error)));
  }, [refresh]);
  useEffect(() => {
    if (tab !== "settings") return;
    let disposed = false;
    call("speech_voices")
      .then((value) => {
        if (!disposed) setVoices(value);
      })
      .catch(() => {
        if (!disposed) setVoices([]);
      });
    return () => {
      disposed = true;
    };
  }, [tab]);
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
    const receiveTrayAction = async () => {
      try {
        const action = await call("tray_action_get");
        if (disposed || !action) return;
        setError("");
        setNotice("");
        setEpisodePanel(action === "import" ? "import" : null);
        if (action === "collect") {
          setTab("library");
          dispatchCollection({
            type: "open",
            seed: { text: "", kind: "word", context: "" },
          });
        } else {
          setTab(action === "import" ? "episodes" : action);
        }
      } catch (error) {
        if (!disposed) setError(message(error));
      }
    };
    listen("tray_action", receiveTrayAction)
      .then((stop) => {
        keep(stop);
        if (!disposed) void receiveTrayAction();
      })
      .catch((error) => setError(message(error)));
    return () => {
      disposed = true;
      stops.forEach((stop) => stop());
    };
  }, []);
  useEffect(() => {
    let disposed = false;
    let stop: (() => void) | undefined;
    let infoTimer: ReturnType<typeof setTimeout> | undefined;
    const resume = () => {
      if (!document.hidden) refresh().catch((error) => setError(message(error)));
    };
    listen<TaskUpdate>("task_updated", ({ payload: task }) => {
      if (disposed) return;
      taskBuffer.current?.set(task.id, task);
      setTasks((current) => applyTaskUpdate(current, task));
      window.dispatchEvent(new CustomEvent("svl_task_updated", { detail: task }));
      if (terminal(task) && !infoTimer) {
        infoTimer = setTimeout(() => {
          infoTimer = undefined;
          call("app_info").then((value) => {
            if (!disposed) setInfo(value);
          }).catch(() => {});
        }, 300);
      }
    }).then((unlisten) => {
      if (disposed) unlisten();
      else stop = unlisten;
    }).catch((error) => {
      if (!disposed) setError(message(error));
    }).finally(() => { if (!disposed) resume(); });
    window.addEventListener("focus", resume);
    document.addEventListener("visibilitychange", resume);
    return () => {
      disposed = true;
      stop?.();
      clearTimeout(infoTimer);
      taskBuffer.current = null;
      window.removeEventListener("focus", resume);
      document.removeEventListener("visibilitychange", resume);
    };
  }, [refresh]);
  useEffect(() => {
    if (document.hidden || !tasks.some((task) => !terminal(task))) return;
    const timer = setTimeout(() => {
      if (!document.hidden) refresh().catch(() => {});
    }, 10_000);
    return () => clearTimeout(timer);
  }, [tasks, refresh]);
  const active = tasks.filter((task) => !terminal(task));
  const report = (error: unknown) => setError(message(error));
  useEffect(() => {
    const stop = listen("watch_collected", () => {
      setRefreshKey((value) => value + 1);
      refresh().catch(() => {});
    });
    return () => {
      void stop.then((unlisten) => unlisten());
    };
  }, [refresh]);
  const saved = () => {
    setSeed(null);
    setNotice("收录成功，例句与原声已按选择保存。");
    setRefreshKey((value) => value + 1);
    refresh().catch(report);
  };
  return (
    <div
      className={`app-shell${tab === "watch" && watchFullscreen ? " watch-fullscreen" : ""}`}
    >
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
              ["watch", "边看边学"],
              ["episodes", "观看前预习"],
              ["practice", "听读练习"],
              ["reviews", "主动回忆测验"],
              ["assessment", "水平测试"],
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
                setEpisodePanel(null);
                setPracticePanel(null);
                setError("");
                setNotice("");
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
          <span
            className={`offline-dot${settings?.offlineMode || info?.networkUnavailable ? " is-offline" : ""}`}
          />
          <span title="在线查询和同步仍由你主动发起；联网失败不影响本地学习，重试成功后恢复在线状态。">
            {!settings
              ? "正在读取设置"
              : settings.offlineMode
                ? "手动离线"
                : info?.networkUnavailable
                  ? "联网失败，离线使用"
                  : "在线模式"}
          </span>
          <small>{info?.version ?? "开发中"}</small>
        </div>
      </aside>
      <main
        className={`workspace${tab === "watch" ? " workspace-watch" : tab === "practice" ? " workspace-practice" : tab === "assessment" ? " workspace-assessment" : ""}`}
      >
        <header className="workspace-header">
          <div>
            {tab !== "episodes" &&
              tab !== "practice" &&
              tab !== "assessment" && (
                <p className="eyebrow">英语学习，从真实语境开始</p>
              )}
            <h1>
              {
                {
                  library: "我的单词本",
                  watch: "边看边学",
                  episodes: "观看前预习",
                  practice: "听读练习",
                  reviews: "主动回忆测验",
                  assessment: "水平测试",
                  leech: "易忘词专项",
                  sync: "站点同步",
                  tasks: "后台任务",
                  settings: "本地设置",
                }[tab]
              }
            </h1>
          </div>
          {(tab === "library" || tab === "episodes" || tab === "practice") && (
            <div className="actions">
              {tab === "practice" && (
                <button
                  className="secondary"
                  aria-expanded={practicePanel === "import"}
                  aria-controls="practice-panel"
                  onClick={() =>
                    setPracticePanel((current) =>
                      current === "import" ? null : "import",
                    )
                  }
                >
                  导入阅读或课程
                </button>
              )}
              {tab === "episodes" && (
                <button
                  className="secondary"
                  aria-expanded={episodePanel === "import"}
                  aria-controls="episode-import"
                  onClick={() =>
                    setEpisodePanel((current) =>
                      current === "import" ? null : "import",
                    )
                  }
                >
                  导入剧集
                </button>
              )}
              {tab === "library" && (
                <>
                  <button
                    className="secondary"
                    onClick={() => call("capture_ocr").catch(report)}
                  >
                    截图收录
                  </button>
                  <button
                    className="primary"
                    onClick={() =>
                      setSeed({ text: "", kind: "word", context: "" })
                    }
                  >
                    ＋ 收录词条
                  </button>
                </>
              )}
            </div>
          )}
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
        {!!info?.dictionaryWarnings?.length && (
          <div className="notice banner" role="status">
            <span>{info.dictionaryWarnings.join(" ")}</span>
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
        {tab === "library" && (
          <LibraryView
            info={info}
            refreshKey={refreshKey}
            collect={setSeed}
            report={report}
          />
        )}
        {tab === "watch" && (
          <WatchView
            settings={settings}
            onSettings={setSettings}
            collect={setSeed}
            collectionOpen={!!seed}
            onFullscreenChange={setWatchFullscreen}
          />
        )}
        {tab === "episodes" && (
          <EpisodesView
            sourceCount={info?.sourceCount ?? 0}
            refreshKey={refreshKey}
            collect={setSeed}
            report={report}
            notify={setNotice}
            panel={episodePanel}
            onPanelChange={setEpisodePanel}
          />
        )}
        {(tab === "reviews" || tab === "leech") && (
          <ReviewView
            key={tab}
            special={tab === "leech"}
            refreshKey={refreshKey}
          />
        )}
        {tab === "assessment" && <AssessmentView dataDirectory={info?.dataDirectory ?? ""} tasks={tasks} showTasks={() => setTab("tasks")} />}
        {tab === "practice" && info && (
          <PracticeView
            dataDirectory={info.dataDirectory}
            collect={setSeed}
            review={() => setTab("library")}
            panel={practicePanel}
            onPanelChange={setPracticePanel}
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
            refreshKey={refreshKey}
            sourceCount={info?.sourceCount ?? 0}
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
            <h2>启动与关闭行为</h2>
            <StartupSettings />
            <label>
              点击关闭按钮时
              <select
                value={settings.closeToTray ? "tray" : "exit"}
                onChange={(event) =>
                  setSettings({
                    ...settings,
                    closeToTray: event.target.value === "tray",
                  })
                }
              >
                <option value="exit">完全退出程序</option>
                <option value="tray">常驻托盘，保留取词快捷键</option>
              </select>
            </label>
            <p className="muted">
              保存设置后生效。完全退出会停止后台任务并释放快捷键；托盘菜单始终可以完全退出。
            </p>
            <h2>取词、输入与系统语音</h2>
            <p className="muted">
              在原应用选择文字后按快捷键，本次文字将在收录窗口中确认。选择常驻托盘时，关闭主窗口后快捷键仍可使用。
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
              输入翻译 / 收录快捷键
              <input
                value={settings.inputShortcut}
                onChange={(event) =>
                  setSettings({
                    ...settings,
                    inputShortcut: event.target.value,
                  })
                }
              />
            </label>
            <p className="muted">
              {native?.inputRegistered
                ? "输入快捷键已注册"
                : "输入快捷键未启用"}
              。打开输入窗口后，可查询释义、朗读并按需收录；留空可禁用。
            </p>
            <ReplacementSettings settings={settings} native={native} onChange={setSettings} />
            <button
              className="secondary"
              onClick={() =>
                setSettings({
                  ...settings,
                  selectionShortcut: "Alt+C",
                  ocrShortcut: "Alt+E",
                  inputShortcut: "Alt+D",
                })
              }
            >
              使用 Pot 快捷键
            </button>
            <p className="muted">
              划词 Alt+C、截图 Alt+E、输入 Alt+D。采用后点“保存设置”；
              切换前先退出 Pot，释放相同快捷键。
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
            <label>
              模型并发请求数
              <input
                type="number"
                min={1}
                max={16}
                step={1}
                value={settings.modelConcurrency}
                onChange={(event) =>
                  setSettings({
                    ...settings,
                    modelConcurrency: Number(event.target.value),
                  })
                }
              />
            </label>
            <p className="muted">
              1 表示串行，最多可设为
              16。保存后应用新上限，已开始的请求会继续完成；实际速度取决于模型服务的并行能力。
            </p>
            <label>
              每次请求最多词条或句子数
              <input
                type="number"
                min={1}
                max={64}
                step={1}
                value={settings.modelBatchSize}
                onChange={(event) =>
                  setSettings({
                    ...settings,
                    modelBatchSize: Number(event.target.value),
                  })
                }
              />
            </label>
            <p className="muted">
              默认 8，可设为
              1～64。只合并同一句对白中的待解释词，不足时按实际数量请求； 1
              表示逐词处理。保存后调整待执行请求，正在执行的请求继续完成。
            </p>
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
              手动离线，禁止外网请求；本机模型仍可使用
            </label>
            <p className="muted">
              默认允许在线查询和同步，由你主动发起。联网失败时继续使用本地词库、原声和测验，可随时重试；重试成功后恢复在线状态。
            </p>
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
                call("settings_update", { settings })
                  .then((value) => {
                    setSettings(value);
                    setNotice("本地设置已保存。");
                    call("native_status")
                      .then(setNative)
                      .catch(report);
                  })
                  .catch(report)
              }
            >
              保存设置
            </button>
            <SubtitleServiceSettings />
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
