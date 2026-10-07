import { useCallback, useEffect, useState } from "react";
import { call, message, terminal } from "./api";
import type { AppInfo, CollectionSeed, Settings, TaskSnapshot } from "./types";
import { CollectionModal } from "./components/CollectionModal";
import { LibraryView } from "./components/LibraryView";
import { EpisodesView } from "./components/EpisodesView";

type Tab = "library" | "episodes" | "tasks" | "settings";
export default function App() {
  const [tab, setTab] = useState<Tab>("library"),
    [info, setInfo] = useState<AppInfo | null>(null),
    [tasks, setTasks] = useState<TaskSnapshot[]>([]),
    [settings, setSettings] = useState<Settings | null>(null);
  const [seed, setSeed] = useState<CollectionSeed | null>(null),
    [error, setError] = useState(""),
    [notice, setNotice] = useState(""),
    [refreshKey, setRefreshKey] = useState(0);
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
  }, [refresh]);
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
          <small>0.1 · 开发中</small>
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
                  tasks: "后台任务",
                  settings: "本地设置",
                }[tab]
              }
            </h1>
          </div>
          <button
            className="primary"
            onClick={() => setSeed({ text: "", kind: "word", context: "" })}
          >
            ＋ 收录词条
          </button>
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
        {tab === "tasks" && (
          <section className="task-list">
            {tasks.map((task) => (
              <article className="task-card" key={task.id}>
                <header>
                  <strong>
                    {{
                      import: "剧集导入",
                      collection: "词条收录",
                      preview: "原声准备",
                      explanation: "本地解释",
                    }[task.kind] ?? task.kind}
                  </strong>
                  <span className={`badge ${task.state}`}>
                    {
                      {
                        queued: "等待",
                        running: "处理中",
                        cancel_requested: "正在取消",
                        succeeded: "已完成",
                        failed: "失败",
                        cancelled: "已取消",
                      }[task.state]
                    }
                  </span>
                </header>
                <p>{task.message}</p>
                {task.total > 0 && (
                  <>
                    <progress value={task.current} max={task.total} />
                    <small>
                      {task.stage} · {task.current}/{task.total}
                    </small>
                  </>
                )}
                {!terminal(task) && (
                  <button
                    className="secondary"
                    onClick={() =>
                      call("task_cancel", { taskId: task.id })
                        .then(refresh)
                        .catch(report)
                    }
                  >
                    取消任务
                  </button>
                )}
                {task.kind === "import" && Boolean(task.result) && (
                  <ImportResult result={task.result} />
                )}
                {task.error && <p className="error">{task.error.message}</p>}
              </article>
            ))}
            {!tasks.length && (
              <div className="empty">
                <p>还没有后台任务。</p>
              </div>
            )}
          </section>
        )}
        {tab === "settings" && settings && (
          <section className="settings-panel">
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
            <button
              className="primary"
              onClick={() =>
                call<Settings>("settings_update", { settings })
                  .then((value) => {
                    setSettings(value);
                    setNotice("本地设置已保存。");
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
        />
      )}
    </div>
  );
}
function ImportResult({ result }: { result: unknown }) {
  const value = result as {
    sources?: {
      id: string;
      title: string;
      exampleCount: number;
      candidateCount: number;
    }[];
    failures?: { file: string; message: string }[];
  };
  return (
    <div className="import-result">
      {value.sources?.map((source) => (
        <p key={source.id}>
          {source.title} · {source.exampleCount} 条对白 ·{" "}
          {source.candidateCount} 个候选
        </p>
      ))}
      {value.failures?.map((failure, index) => (
        <p className="error" key={index}>
          {failure.file}：{failure.message}
        </p>
      ))}
    </div>
  );
}
