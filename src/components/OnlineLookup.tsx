import { useEffect, useRef, useState } from "react";
import { call, message, uid, waitTask } from "../api";
import type { LookupResult } from "../generated/ipc";

export function OnlineLookup({
  text,
  onMeaning,
}: {
  text: string;
  onMeaning?: (meaning: string) => void;
}) {
  const [result, setResult] = useState<LookupResult | null>(null),
    [busy, setBusy] = useState(false),
    [error, setError] = useState(""),
    [taskId, setTaskId] = useState<string | null>(null);
  const generation = useRef(0),
    active = useRef<string | null>(null);
  useEffect(() => {
    generation.current++;
    setResult(null);
    setBusy(false);
    setTaskId(null);
    const task = active.current;
    if (task) void call("task_cancel", { taskId: task }).catch(() => {});
    return () => {
      generation.current++;
      if (active.current)
        void call("task_cancel", { taskId: active.current }).catch(() => {});
    };
  }, [text]);
  async function lookup(provider: "dictionary" | "translation") {
    const epoch = ++generation.current;
    let ownTaskId: string | null = null;
    setBusy(true);
    setError("");
    setResult(null);
    try {
      const task = await call("online_query_start", {
        text,
        provider,
        operationId: uid(),
      });
      active.current = task.id;
      ownTaskId = task.id;
      if (epoch !== generation.current) {
        await call("task_cancel", { taskId: task.id });
        return;
      }
      setTaskId(task.id);
      const value = await waitTask(task);
      if (epoch === generation.current) setResult(value);
    } catch (e) {
      if (epoch === generation.current) setError(message(e));
    } finally {
      if (epoch === generation.current) {
        setBusy(false);
        setTaskId(null);
      }
      if (active.current === ownTaskId) active.current = null;
    }
  }
  return (
    <section className="online-lookup">
      <button
        type="button"
        className="secondary"
        disabled={busy || !text.trim() || text.trim().length > 120}
        onClick={() => lookup("dictionary")}
      >
        在线英语词典
      </button>
      <button
        type="button"
        className="secondary"
        disabled={
          busy ||
          !text.trim() ||
          new TextEncoder().encode(text.trim()).length > 500
        }
        onClick={() => lookup("translation")}
      >
        在线中文翻译
      </button>
      <p className="muted">
        手动查询时只发送上方查询文字：英语词典使用 Wiktionary，中文翻译使用
        MyMemory。不会附带原句、截图或词库；手动离线时禁止联网，联网失败可稍后重试。
      </p>
      {taskId && (
        <button
          type="button"
          onClick={() =>
            call("task_cancel", { taskId }).catch((e) => setError(message(e)))
          }
        >
          取消查询
        </button>
      )}
      {error && (
        <p className="error" role="alert">
          {error}
        </p>
      )}
      {result && (
        <div className="suggestion">
          <strong>
            {result.query} · {result.source}
          </strong>
          <p className="directory">来源：{result.sourceUrl}</p>
          {result.definitions.map((definition, index) => (
            <div key={index} className="lookup-definition">
              <p>
                {definition.partOfSpeech && (
                  <span className="badge">{definition.partOfSpeech}</span>
                )}{" "}
                {definition.text}
              </p>
              {definition.example && (
                <p className="muted">{definition.example}</p>
              )}
              {onMeaning && (
                <button
                  type="button"
                  className="text-button"
                  onClick={() => onMeaning(definition.text)}
                >
                  采用这条释义
                </button>
              )}
            </div>
          ))}
        </div>
      )}
    </section>
  );
}
