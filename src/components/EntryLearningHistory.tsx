import { useEffect, useState } from "react";
import { call, message } from "../api";
import type { Attempt } from "../generated/ipc";
const outcomes: Record<string, string> = {
  correct: "正确",
  correct_with_hint: "有提示正确",
  incorrect: "错误",
  uncertain: "待确认",
};
export function EntryLearningHistory({ entryId }: { entryId: string }) {
  const [open, setOpen] = useState(false),
    [page, setPage] = useState(0),
    [history, setHistory] = useState<Attempt[]>([]),
    [error, setError] = useState("");
  useEffect(() => {
    if (!open) return;
    let disposed = false;
    setError("");
    call("entry_review_history", { entryId, offset: page * 20, limit: 20 })
      .then((rows) => {
        if (!disposed) setHistory(rows);
      })
      .catch((e) => {
        if (!disposed) setError(message(e));
      });
    return () => {
      disposed = true;
    };
  }, [entryId, open, page]);
  return (
    <details
      className="entry-learning-history"
      onToggle={(e) => setOpen(e.currentTarget.open)}
    >
      <summary>查看这个词条的学习记录</summary>
      {error && <p className="error">{error}</p>}
      {open && (
        <>
          {history.map((a) => (
            <article key={a.id} className="example-card">
              <p>
                {new Date(a.createdAt).toLocaleString()} ·{" "}
                {a.question.dimension === "meaning" ? "词义回忆" : "听力识别"} ·{" "}
                {outcomes[a.outcome] ?? a.outcome}
              </p>
              <p>
                回答：{a.answer || "暂时想不起来"}
                {a.hinted ? "（使用提示）" : ""}
              </p>
              <p>当时参考：{a.question.expected.join("；")}</p>
              <p className="muted">
                初次判分：{outcomes[a.originalOutcome] ?? a.originalOutcome} ·{" "}
                {a.grader}
              </p>
              {a.corrections.map((c, i) => (
                <p key={i}>
                  修正：{outcomes[c.outcome] ?? c.outcome} · {c.reason}
                </p>
              ))}
            </article>
          ))}
          {!history.length && (
            <p>此词条尚无作答记录。浏览与收录不会自动计为测验通过。</p>
          )}
          <div className="actions">
            <button disabled={!page} onClick={() => setPage(page - 1)}>
              较新记录
            </button>
            <button
              disabled={history.length < 20}
              onClick={() => setPage(page + 1)}
            >
              更早记录
            </button>
          </div>
        </>
      )}
    </details>
  );
}
