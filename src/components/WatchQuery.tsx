import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { call, message } from "../api";
import type { QuerySession, WatchContext } from "../generated/ipc";

export type WatchRequest = {
  target: string;
  mode: "explain" | "translate";
  nonce: number;
};

/** A separate query owner: never replaces the global capture/clipboard session. */
export function WatchQuery({
  request,
  context,
  playerSession,
  collect,
}: {
  request: WatchRequest;
  context: WatchContext;
  playerSession: string;
  collect: (target: string, meaning: string) => void;
}) {
  const [session, setSession] = useState<QuerySession | null>(null);
  const [error, setError] = useState("");
  useEffect(() => {
    let active = true,
      id = "",
      queryId: string | null = null,
      complete = false;
    setSession(null);
    setError("");
    const refresh = async () => {
      const value = await call("translation_session_get", { sessionId: id });
      if (!active || !value || value.sessionId !== id) return;
      setSession(value);
      if (request.mode === "translate" && !complete) {
        const result = value.results.find(
          (r) => r.value && ["ready", "running"].includes(r.state),
        );
        if (
          !result &&
          value.results.length &&
          value.results.every((r) => ["failed", "cancelled"].includes(r.state))
        ) {
          complete = true;
          await call("watch_control", {
            control: {
              action: "query_translation",
              text: id,
              value: null,
              expectedRevision: context.revision,
            },
          });
        }
        if (result) {
          if (result.state === "ready") complete = true;
          await call("watch_control", {
            control: {
              action: "query_translation",
              text: id,
              value: null,
              expectedRevision: context.revision,
            },
          });
        }
      }
    };
    const stop = listen<{ sessionId: string }>("watch_query_updated", (e) => {
      if (active && e.payload.sessionId === id)
        void refresh().catch((e) => active && setError(message(e)));
    });
    void (async () => {
      await stop;
      const providers = (await call("providers_list")).filter(
        (p) => p.kind === "qwen" && p.enabled,
      );
      if (!providers.length)
        throw new Error("请先在本地设置中启用本地模型查询服务。");
      if (!active) return;
      const opened = await call("translation_session_open", {
        sessionId: null,
        input: {
          target: request.target,
          context: request.mode === "translate" ? "" : context.text,
          mode: request.mode,
          sourceLanguage: "en",
          targetLanguage: "zh-CN",
          origin: {
            kind: "watch",
            sourceId: context.sourceId || null,
            sourceTitle: "边看边学",
            locationKey: `${playerSession}:${context.revision}`,
          },
        },
      });
      id = opened.sessionId;
      if (!active) {
        return;
      }
      const started = await call("translation_query_start", {
        sessionId: id,
        expectedInputRevision: opened.inputRevision,
        providerIds: [providers[0].id],
        automatic: false,
        persist: true,
      });
      queryId = started.queryId;
      if (!active) {
        if (queryId)
          await call("translation_query_cancel", { sessionId: id, queryId });
        return;
      }
      await refresh();
    })().catch((e) => active && setError(message(e)));
    return () => {
      active = false;
      void stop.then((fn) => fn());
      if (id && queryId)
        void call("translation_query_cancel", {
          sessionId: id,
          queryId,
        }).catch(() => {});
    };
  }, [request, context.revision, playerSession]);
  return (
    <div className="watch-query" aria-live="polite">
      <strong>{request.target}</strong>
      {error && (
        <p className="error" role="alert">
          {error}
        </p>
      )}
      {!session?.results.length && !error && <p className="muted">正在查询…</p>}
      {session?.results.map((r) => (
        <div key={r.providerId}>
          {r.state === "queued" && <p className="muted">排队等待本地模型…</p>}
          {r.state === "running" && <small className="muted">正在生成…</small>}
          {r.value && (
            <p className="watch-query-result">
              {r.value.meaning}
              {r.value.meaning && r.value.translation ? "\n" : ""}
              {r.value.translation}
            </p>
          )}
          {r.error && <p className="error">{r.error}</p>}
          {r.state === "ready" && r.value && (
            <button
              className="text-button"
              onClick={() =>
                collect(
                  request.target,
                  request.mode === "translate"
                    ? r.value!.translation
                    : r.value!.meaning,
                )
              }
            >
              收录{request.mode === "translate" ? "整句" : "所查内容"}
            </button>
          )}
        </div>
      ))}
    </div>
  );
}
