import { useCallback, useEffect, useReducer, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { call, message, terminal } from "../api";
import type { CommandResult } from "../commands";
import type { AppInfo, CollectionSeed, Settings, TaskSnapshot } from "../types";
import { collectionQueue } from "../collectionQueue";
import { CollectionModal } from "../components/CollectionModal";
import { LibraryView } from "../components/LibraryView";
import { TasksView } from "../components/TasksView";
import { RuntimeFeatures } from "../runtimeFeatures";
import "./mobile.css";

export default function MobileApp() {
  const [page, setPage] = useState<"library" | "account">("library");
  const [info, setInfo] = useState<AppInfo | null>(null);
  const [settings, setSettings] = useState<Settings | null>(null);
  const [capabilities, setCapabilities] = useState<CommandResult<"app_capabilities"> | null>(null);
  const [tasks, setTasks] = useState<TaskSnapshot[]>([]);
  const [refreshKey, setRefreshKey] = useState(0);
  const [error, setError] = useState("");
  const [queue, dispatch] = useReducer(collectionQueue, { current: null, pending: [] });
  const refresh = useCallback(async () => {
    await call("app_ready");
    const [nextInfo, nextSettings, nextCapabilities, nextTasks] = await Promise.all([
      call("app_info"), call("settings_get"), call("app_capabilities"), call("tasks_list"),
    ]);
    setInfo(nextInfo); setSettings(nextSettings); setCapabilities(nextCapabilities); setTasks(nextTasks);
  }, []);
  useEffect(() => { refresh().catch((e) => setError(message(e))); }, [refresh]);
  useEffect(() => {
    let disposed = false;
    const stop = listen<TaskSnapshot>("task_updated", ({ payload }) => {
      if (disposed) return;
      refresh().catch((e) => setError(message(e)));
      if (terminal(payload)) setRefreshKey((value) => value + 1);
    });
    const receive = listen<CollectionSeed>("capture_completed", ({ payload }) => {
      if (!disposed) dispatch({ type: "receive", seed: payload });
    });
    return () => { disposed = true; stop.then((fn) => fn()); receive.then((fn) => fn()); };
  }, [refresh]);
  const collect = (seed: CollectionSeed) => dispatch({ type: "open", seed });
  async function setOffline(offlineMode: boolean) {
    if (!settings) return;
    try { setSettings(await call("settings_update", { settings: { ...settings, offlineMode } })); }
    catch (e) { setError(message(e)); }
  }
  return (
    <RuntimeFeatures.Provider value={{ mobile: true, systemSpeech: capabilities?.systemSpeech ?? false,
      knownTargetPreferences: capabilities?.knownTargetPreferences ?? false }}>
      <div className="mobile-app">
        <header className="mobile-header">
          <div><span className="eyebrow">SubtitleVocabularyList</span><h1>{page === "library" ? "我的单词本" : "我的"}</h1></div>
          <button disabled={!info} onClick={() => collect({ text: "", context: "", kind: "word" })}>＋ 收录</button>
        </header>
        <main>
          {error && <div className="error banner" role="alert">{error}<button onClick={() => { setError(""); refresh().catch((e) => setError(message(e))); }}>重试</button></div>}
          {!info && !error && <p role="status">正在打开手机本地词库…</p>}
          {page === "library" && info && <LibraryView compact info={info} refreshKey={refreshKey} collect={collect} report={(e) => setError(message(e))} />}
          {page === "account" && <>
            <section className="mobile-card"><h2>本机学习</h2><p>{info?.entryCount ?? 0} 个词条 · v{info?.version ?? ""}</p>
              <label className="check-label"><input type="checkbox" checked={settings?.offlineMode ?? false} onChange={(e) => setOffline(e.target.checked)} />手动离线</label>
              <p className="muted">词库和学习记录保存在手机上，与桌面客户端独立运行。</p>
              {!!info?.dictionaryWarnings.length && <p className="notice">{info.dictionaryWarnings.join(" ")}</p>}
            </section>
            <TasksView tasks={tasks} refreshKey={refreshKey} sourceCount={info?.sourceCount ?? 0}
              cancel={(taskId) => { call("task_cancel", { taskId }).then(refresh).catch((e) => setError(message(e))); }} />
          </>}
          {!queue.current && queue.pending.length > 0 && <button onClick={() => dispatch({ type: "next" })}>继续确认待收录内容（{queue.pending.length}）</button>}
        </main>
        <nav className="mobile-navigation" aria-label="主导航">
          <button aria-current={page === "library" ? "page" : undefined} onClick={() => setPage("library")}>单词本</button>
          <button aria-current={page === "account" ? "page" : undefined} onClick={() => setPage("account")}>我的</button>
        </nav>
        {queue.current && <CollectionModal key={queue.current.text + queue.current.locationKey}
          seed={queue.current} onClose={() => dispatch({ type: "close" })}
          onMarked={() => dispatch({ type: "close" })}
          onSaved={() => { dispatch({ type: "close" }); setRefreshKey((value) => value + 1); refresh().catch((e) => setError(message(e))); }} />}
      </div>
    </RuntimeFeatures.Provider>
  );
}
