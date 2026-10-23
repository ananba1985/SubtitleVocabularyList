import { useEffect, useRef, useState } from "react";
import { call, message } from "../api";
import type { CollectionSeed } from "../types";
import type {
  Provider,
  ProviderResult,
  QuerySession,
  QuerySeed,
} from "../generated/ipc";

export function MobileQuery({
  text,
  context,
  seed,
  apply,
}: {
  text: string;
  context: string;
  seed?: CollectionSeed;
  apply: (value: QuerySeed) => void;
}) {
  const [providers, setProviders] = useState<Provider[]>([]),
    [provider, setProvider] = useState("qwen");
  const [result, setResult] = useState<ProviderResult | null>(null),
    [busy, setBusy] = useState(false),
    [notice, setNotice] = useState("");
  const [compare, setCompare] = useState(false);
  const [results, setResults] = useState<ProviderResult[]>([]);
  const [sendContext, setSendContext] = useState(false);
  const generation = useRef(0),
    session = useRef<QuerySession | null>(null);
  const reverse = /[\u3400-\u9fff]/.test(text);
  useEffect(() => {
    call("providers_list")
      .then(setProviders)
      .catch((e) => setNotice(message(e)));
  }, []);
  useEffect(() => {
    generation.current++;
    setResult(null);setResults([]);
    setNotice("");
    setBusy(false);
    const old = session.current;
    session.current = null;
    if (old)
      void call("translation_query_cancel", {
        sessionId: old.sessionId,
        queryId: old.queryId,
      }).catch(() => {});
    return () => {
      generation.current++;
      const old = session.current;
      if (old)
        void call("translation_query_cancel", {
          sessionId: old.sessionId,
          queryId: old.queryId,
        }).catch(() => {});
    };
  }, [text, context, provider, sendContext, compare]);
  async function query() {
    const epoch = ++generation.current;
    setBusy(true);
    setResult(null);
    setResults([]);
    setNotice("");
    try {
      const opened = await call("translation_session_open", {
        input: {
          target: text,
          context: sendContext ? context : "",
          mode: "auto",
          sourceLanguage: "auto",
          targetLanguage: "auto",
          origin: {
            kind: seed?.sourceId ? "selection" : "input",
            sourceId: seed?.sourceId ?? null,
            sourceTitle: seed?.sourceTitle ?? "手机查询",
            locationKey: seed?.locationKey ?? "",
          },
        },
      });
      if (epoch !== generation.current) return;
      session.current = opened;
      let current = await call("translation_query_start", {
        sessionId: opened.sessionId,
        expectedInputRevision: opened.inputRevision,
        providerIds: compare ? providers.filter(p=>p.enabled && p.kind!=="plugin").map(p=>p.id) : [provider],
        automatic: false,
        persist: true,
      });
      if (epoch !== generation.current) {
        await call("translation_query_cancel", {
          sessionId: current.sessionId,
          queryId: current.queryId,
        });
        return;
      }
      while (epoch === generation.current) {
        session.current = current;
        setResult(current.results[0] ?? null); setResults(current.results);
        if (
          !current.results.some(
            (r) => r.state === "queued" || r.state === "running",
          )
        )
          break;
        await new Promise((resolve) => setTimeout(resolve, 250));
        const next = await call("translation_session_get", {
          sessionId: opened.sessionId,
        });
        if (!next || next.inputRevision !== opened.inputRevision) return;
        current = next;
      }
    } catch (e) {
      if (epoch === generation.current) setNotice(message(e));
    } finally {
      if (epoch === generation.current) setBusy(false);
    }
  }
  async function useResult(chosen: ProviderResult | null = result) {
    const current = session.current;
    if (
      !current ||
      chosen?.state !== "ready" ||
      current.input.target !== text ||
      current.input.context !== (sendContext ? context : "")
    )
      return;
    const epoch = generation.current;
    try {
      const draft = await call("mobile_query_seed", {
        sessionId: current.sessionId,
        revision: current.inputRevision,
        providerId: chosen.providerId,
      });
      if (epoch === generation.current)
        apply({
          ...draft,
          context: reverse ? draft.context : draft.context || context,
        });
    } catch (e) {
      setNotice(message(e));
    }
  }
  return (
    <section className="mobile-card mobile-query">
      <h3>按需查询／翻译</h3>
      <label>
        服务
        <select
          aria-label="手机查询服务"
          value={provider}
          disabled={busy}
          onChange={(e) => setProvider(e.target.value)}
        >
          {providers
            .filter((p) => p.kind !== "plugin")
            .map((p) => (
              <option key={p.id} value={p.id} disabled={!p.enabled}>
                {p.kind === "qwen" ? "已配置模型" : p.name}
                {!p.enabled ? "（未启用）" : ""}
              </option>
            ))}
        </select>
      </label>
      <label className="check-label"><input type="checkbox" checked={compare} onChange={e=>setCompare(e.target.checked)}/>对比所有已启用服务（将查询发送给这些服务）</label>
      <p className="muted">
        只在点击时发送查询文字。中文输入将译成英文供核对；可在“我的”配置服务。手动离线禁止外部请求。
      </p>
      {context.trim() && context.trim() !== text.trim() && (
        <label className="check-label">
          <input
            type="checkbox"
            checked={sendContext}
            onChange={(e) => setSendContext(e.target.checked)}
          />
          本次同时发送上方原句语境
        </label>
      )}
      <button
        disabled={
          busy ||
          !text.trim() ||
          !providers.some((p) => p.enabled && p.kind !== "plugin" && (compare || p.id === provider))
        }
        onClick={query}
      >
        {reverse ? "查询英文译文" : "明确调用所选服务"}
      </button>
      {busy && (
        <button
          onClick={() => {
            generation.current++;
            setBusy(false);
            setResult(null);
            setResults([]);
            const current = session.current;
            if (current)
              void call("translation_query_cancel", {
                sessionId: current.sessionId,
                queryId: current.queryId,
              });
          }}
        >
          取消查询
        </button>
      )}
      {compare && results.map(r=><article key={r.providerId} className="mobile-query-result"><h4>{r.name} · {r.state}</h4><p>{r.value?.meaning}</p><p>{r.value?.translation}</p>{r.error && <p className="notice">{r.error}</p>}<button disabled={busy||r.state!=="ready"} onClick={()=>useResult(r)}>采用 {r.name} 的结果</button></article>)}
      {!compare && result && (
        <>
          <p role="status">
            {result.state === "ready"
              ? "完整结果"
              : result.state === "running"
                ? "正在接收，完成前不可采用"
                : result.state}
          </p>
          {result.value && (
            <>
              <p>{result.value.meaning}</p>
              <p>{result.value.translation}</p>
            </>
          )}
          {result.error && <p className="notice">{result.error}</p>}
          <div className="actions">
            <button
              disabled={result.state !== "ready" || busy}
              onClick={()=>useResult()}
            >
              {reverse ? "采用英文目标并继续核对" : "采用查询结果"}
            </button>
            <button
              disabled={result.state !== "ready" || busy}
              onClick={() =>
                call("mobile_copy", {
                  text:
                    result.value?.translation || result.value?.meaning || "",
                })
                  .then(() => setNotice("已复制结果，未收录词条。"))
                  .catch((e) => setNotice(message(e)))
              }
            >
              复制结果
            </button>
          </div>
        </>
      )}
      {notice && <p role="status">{notice}</p>}
    </section>
  );
}
