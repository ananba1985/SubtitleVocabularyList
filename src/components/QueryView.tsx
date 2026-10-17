import { useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { call, message, uid, waitTask, audioUrl } from "../api";

import type {
  QuerySession,
  QueryInput,
  Provider,
  ClipboardStatus,
  ProviderResult,
} from "../queryTypes";
import { ProviderSettings } from "./ProviderSettings";

const empty: QueryInput = {
  target: "",
  context: "",
  mode: "explain",
  sourceLanguage: "en",
  targetLanguage: "zh-CN",
  origin: { kind: "input", sourceId: null, sourceTitle: "", locationKey: "" },
};
const states: Record<string, string> = {
  queued: "排队等待…",
  running: "等待回复…",
  ready: "",
  failed: "查询失败",
  cancelled: "已取消",
};
export function QueryView() {
  const [session, setSession] = useState<QuerySession | null>(null),
    [draft, setDraft] = useState<QueryInput>(empty),
    [providers, setProviders] = useState<Provider[]>([]),
    [clipboard, setClipboard] = useState<ClipboardStatus>({
      enabled: false,
      debounceMs: 300,
      persistResults: true,
      error: null,
    });
  const [dirty, setDirty] = useState(false),
    [pinned, setPinned] = useState(false),
    [error, setError] = useState(""),
    [notice, setNotice] = useState(""),
    [pending, setPending] = useState<string | null>(null),
    [options, setOptions] = useState(false);
  const [speechBusy, setSpeechBusy] = useState(false),
    [speechPath, setSpeechPath] = useState("");
  const [windowVisible, setWindowVisible] = useState(true);
  const current = useRef<QuerySession | null>(null),
    input = useRef<HTMLTextAreaElement>(null),
    audio = useRef<HTMLAudioElement>(null),
    dirtyRef = useRef(false),
    pinnedRef = useRef(false),
    speechTask = useRef<string | null>(null),
    speechEpoch = useRef(0);
  current.current = session;
  dirtyRef.current = dirty;
  pinnedRef.current = pinned;
  const report = (error: unknown) => setError(message(error));
  const busy = !!session?.results.some(
    (result) => result.state === "queued" || result.state === "running",
  );
  useEffect(() => {
    if (speechPath)
      audio.current?.play().catch(() => setNotice("请点击播放器开始朗读。"));
  }, [speechPath]);
  async function refresh(id?: string) {
    const value = await call("translation_session_get", {
      sessionId: id ?? null,
    });
    if (!value) return;
    if (dirtyRef.current) {
      if (
        value.input.origin.kind === "clipboard" &&
        value.sessionId !== current.current?.sessionId
      )
        setPending(value.sessionId);
      return;
    }
    if (
      !current.current ||
      value.sessionId !== current.current.sessionId ||
      value.inputRevision !== current.current.inputRevision
    ) {
      setDraft(value.input);
      setSpeechPath("");
      setError("");
      setNotice("");
    }
    setSession(value);
  }
  async function stopSpeech() {
    speechEpoch.current += 1;
    audio.current?.pause();
    setSpeechPath("");
    setSpeechBusy(false);
    const id = speechTask.current;
    speechTask.current = null;
    if (id) await call("task_cancel", { taskId: id }).catch(() => {});
  }
  async function hide() {
    await stopSpeech();
    await call("translation_window_hide").catch(report);
  }
  useEffect(() => {
    document.body.classList.add("query-body");
    let disposed = false;
    const stops: (() => void)[] = [];
    const keep = (stop: () => void) => (disposed ? stop() : stops.push(stop));
    const ready = () => refresh().catch(report);
    call("providers_list").then(setProviders).catch(report);
    call("clipboard_listener_get").then(setClipboard).catch(report);
    listen<{ sessionId: string; origin: string; inputRevision: number }>(
      "query_updated",
      (e) => {
        if (e.payload.origin === "clipboard" && dirtyRef.current) {
          setPending(e.payload.sessionId);
          return;
        }
        refresh().catch(report);
      },
    )
      .then((stop) => {
        keep(stop);
        if (!disposed) ready();
      })
      .catch(report);
    listen("translation_hidden", () => {
      setWindowVisible(false);
      void stopSpeech();
      setDirty(false);
      dirtyRef.current = false;
    })
      .then(keep)
      .catch(report);
    listen("clipboard_changed", () =>
      call("clipboard_listener_get").then(setClipboard).catch(report),
    )
      .then(keep)
      .catch(report);
    getCurrentWindow()
      .onFocusChanged((e) => {
        if (e.payload) {
          setWindowVisible(true);
          ready();
          input.current?.focus();
        } else if (
          !pinnedRef.current &&
          !dirtyRef.current &&
          !current.current?.results.some((r) =>
            ["queued", "running"].includes(r.state),
          )
        ) {
          void hide();
        }
      })
      .then(keep)
      .catch(() => {});
    const key = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        event.preventDefault();
        void hide();
      }
    };
    window.addEventListener("keydown", key);
    input.current?.focus();
    return () => {
      disposed = true;
      stops.forEach((stop) => stop());
      window.removeEventListener("keydown", key);
      document.body.classList.remove("query-body");
    };
  }, []);
  useEffect(() => {
    if (!busy || !windowVisible || document.hidden || dirty) return;
    const timer = setTimeout(() => {
      if (!document.hidden && !dirtyRef.current && current.current)
        refresh(current.current.sessionId).catch(report);
    }, 10_000);
    return () => clearTimeout(timer);
  }, [session, busy, windowVisible, dirty]);
  function change(next: QueryInput) {
    setDraft(next);
    setDirty(true);
    dirtyRef.current = true;
    setError("");
    setNotice("");
    void stopSpeech();
    if (current.current)
      void call("translation_query_cancel", {
        sessionId: current.current.sessionId,
        queryId: current.current.queryId,
      }).catch(() => {});
  }
  async function query() {
    try {
      setError("");
      setNotice("");
      await stopSpeech();
      const opened = await call("translation_session_open", {
        input:
          draft.origin.kind === "clipboard"
            ? { ...draft, origin: { ...draft.origin, kind: "input" } }
            : draft,
        sessionId:
          session?.input.origin.kind === "clipboard"
            ? null
            : (session?.sessionId ?? null),
      });
      dirtyRef.current = false;
      setDirty(false);
      setSession(opened);
      current.current = opened;
      const started = await call("translation_query_start", {
        sessionId: opened.sessionId,
        expectedInputRevision: opened.inputRevision,
        providerIds: [],
        automatic: false,
        persist: true,
      });
      setSession(started);
    } catch (error) {
      report(error);
    }
  }
  async function usePending() {
    const id = pending;
    if (!id) return;
    dirtyRef.current = false;
    setDirty(false);
    current.current = null;
    setPending(null);
    await refresh(id).catch(report);
  }
  async function pin() {
    try {
      await call("translation_window_pin", { pinned: !pinned });
      setPinned(!pinned);
    } catch (error) {
      report(error);
    }
  }
  async function listener(enabled: boolean) {
    try {
      const status = await call("clipboard_listener_set", {
        enabled,
        preferences: {
          debounceMs: clipboard.debounceMs,
          persistResults: clipboard.persistResults,
        },
      });
      setClipboard(status);
    } catch (error) {
      report(error);
    }
  }
  async function collect(result: ProviderResult) {
    if (!session) return;
    try {
      await call("translation_collect", {
        sessionId: session.sessionId,
        inputRevision: session.inputRevision,
        providerId: result.providerId,
      });
      setNotice("已打开收录确认，保存前不会加入单词本。");
    } catch (error) {
      report(error);
    }
  }
  async function copy(result: ProviderResult) {
    try {
      const value = result.value!;
      await call("query_copy", { text: value.translation || value.meaning });
      setNotice("已复制译文");
    } catch (error) {
      report(error);
    }
  }
  async function speak() {
    const epoch = ++speechEpoch.current;
    setSpeechBusy(true);
    setError("");
    setSpeechPath("");
    try {
      const task = await call("speech_start", {
        text: draft.target,
        operationId: uid(),
      });
      if (epoch !== speechEpoch.current) {
        await call("task_cancel", { taskId: task.id }).catch(() => {});
        return;
      }
      speechTask.current = task.id;
      const result = await waitTask<{ path: string }>(task);
      if (epoch === speechEpoch.current) {
        setSpeechPath(result.path);
        speechTask.current = null;
      }
    } catch (error) {
      if (epoch === speechEpoch.current) report(error);
    } finally {
      if (epoch === speechEpoch.current) setSpeechBusy(false);
    }
  }
  const visible = !dirty ? (session?.results ?? []) : [];
  return (
    <main className="query-window">
      <header className="query-header">
        <strong>查词与翻译</strong>
        <div>
          <button onClick={pin} aria-pressed={pinned} title="固定窗口">
            {pinned ? "已固定" : "固定"}
          </button>
          <button onClick={() => setOptions(!options)} aria-expanded={options}>
            选项
          </button>
          <button onClick={hide} aria-label="关闭查询窗口">
            ×
          </button>
        </div>
      </header>
      <section className="query-input">
        <textarea
          ref={input}
          rows={2}
          aria-label="查询文字"
          placeholder="输入英文，Enter 查询"
          value={draft.target}
          onChange={(e) => change({ ...draft, target: e.target.value })}
          onKeyDown={(e) => {
            if (
              e.key === "Enter" &&
              !e.shiftKey &&
              !e.nativeEvent.isComposing
            ) {
              e.preventDefault();
              void query();
            }
          }}
        />
        <div className="query-row">
          <select
            aria-label="查询方式"
            value={draft.mode}
            onChange={(e) =>
              change({ ...draft, mode: e.target.value as QueryInput["mode"] })
            }
          >
            <option value="explain">词义解释</option>
            <option value="translate">整段翻译</option>
          </select>
          <button
            className="primary"
            disabled={!draft.target.trim()}
            onClick={query}
          >
            查询
          </button>
          {busy && !dirty && (
            <button
              onClick={() =>
                call("translation_query_cancel", {
                  sessionId: session!.sessionId,
                  queryId: session!.queryId,
                }).catch(report)
              }
            >
              取消
            </button>
          )}
          <button
            disabled={!draft.target.trim()}
            onClick={speechBusy || speechPath ? () => void stopSpeech() : speak}
          >
            {speechBusy ? "取消朗读" : speechPath ? "停止朗读" : "英语朗读"}
          </button>
        </div>
        <details>
          <summary>原句语境</summary>
          <textarea
            aria-label="原句语境"
            rows={2}
            value={draft.context}
            onChange={(e) => change({ ...draft, context: e.target.value })}
          />
        </details>
      </section>
      {pending && (
        <button className="query-pending" onClick={usePending}>
          查看新复制的文字
        </button>
      )}
      {error && (
        <p className="error" role="alert">
          {error}
        </p>
      )}
      {notice && (
        <p className="query-notice" role="status">
          {notice}
        </p>
      )}
      {speechPath && (
        <audio
          ref={audio}
          autoPlay
          controls
          src={audioUrl(speechPath)}
          onError={() => setError("语音播放失败，请重新朗读。")}
        />
      )}
      <section className="query-results" aria-label="查询结果">
        {visible.length ? (
          visible.map((result, index) => (
            <details
              key={`${session?.inputRevision}:${result.providerId}`}
              open={index === 0}
              className="query-result"
            >
              <summary>
                {result.cacheSource === "dictionary"
                  ? "本地基础词库"
                  : result.name}
                <small>
                  {result.cacheSource === "dictionary"
                    ? "已匹配"
                    : result.cacheSource === "stored"
                      ? "已有本地解释"
                      : result.cacheSource === "cache"
                        ? "已保存结果"
                        : result.state === "running" && result.value
                          ? "正在生成…"
                          : states[result.state]}
                </small>
              </summary>
              {result.error && <p className="error">{result.error}</p>}
              {result.value && (
                <>
                  {result.value.meaning && (
                    <p className="query-meaning">{result.value.meaning}</p>
                  )}
                  {result.value.translation && (
                    <p className="query-translation">
                      {result.value.translation}
                    </p>
                  )}
                  {result.value.notes && (
                    <p className="muted">{result.value.notes}</p>
                  )}
                  {result.state === "ready" && (
                    <div className="query-row">
                      <button onClick={() => copy(result)}>复制译文</button>
                      <button
                        className="primary"
                        onClick={() => collect(result)}
                      >
                        收录
                      </button>
                    </div>
                  )}
                </>
              )}
            </details>
          ))
        ) : (
          <p className="query-empty">
            {dirty
              ? "文字已修改，按 Enter 查询。"
              : "输入文字或使用 Alt+C 划词、Alt+E 截图。"}
          </p>
        )}
      </section>
      {options && (
        <section className="query-options">
          <label className="check-label">
            <input
              type="checkbox"
              checked={clipboard.enabled}
              onChange={(e) => listener(e.target.checked)}
            />
            监听剪贴板
          </label>
          <p className="muted">
            开启后查询新复制的纯文本，默认使用本地模型。监听状态重启后关闭。
          </p>
          <label className="check-label">
            <input
              type="checkbox"
              checked={clipboard.persistResults}
              onChange={(e) =>
                setClipboard({ ...clipboard, persistResults: e.target.checked })
              }
            />
            将监听结果保存在本机查询缓存
          </label>
          <label>
            合并间隔（毫秒）
            <input
              type="number"
              min={100}
              max={2000}
              value={clipboard.debounceMs}
              onChange={(e) =>
                setClipboard({
                  ...clipboard,
                  debounceMs: Number(e.target.value),
                })
              }
            />
          </label>
          <button onClick={() => listener(clipboard.enabled)}>
            保存监听选项
          </button>
          {clipboard.error && <p className="error">{clipboard.error}</p>}
          <ProviderSettings
            providers={providers}
            onChange={setProviders}
            report={report}
          />
        </section>
      )}
      <footer>
        {clipboard.enabled ? "剪贴板监听中" : "查询后按需收录"}
        <span>Enter 查询 · Esc 关闭</span>
      </footer>
    </main>
  );
}
