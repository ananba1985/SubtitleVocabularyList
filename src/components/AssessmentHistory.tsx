import { useEffect, useState } from "react";
import { call, message } from "../api";
import type {
  AssessmentComparison,
  AssessmentHistoryItem,
} from "../assessmentTypes";
import { assessmentStateNames, assessmentScopeLabel } from "../assessmentTypes";
const rate = (n: number | null) =>
  n === null ? "暂无有效结果" : `${(n * 100).toFixed(1)}%`;
export function AssessmentHistory({
  onOpen,
}: {
  onOpen: (id: string) => void;
}) {
  const [open, setOpen] = useState(false),
    [page, setPage] = useState(0),
    [items, setItems] = useState<AssessmentHistoryItem[]>([]),
    [total, setTotal] = useState(0),
    [selected, setSelected] = useState<string[]>([]),
    [comparison, setComparison] = useState<AssessmentComparison | null>(null),
    [error, setError] = useState("");
  useEffect(() => {
    if (!open) return;
    let disposed = false;
    setError("");
    void call<{ items: AssessmentHistoryItem[]; total: number }>(
      "assessment_history_list",
      { offset: page * 20, limit: 20, comparisonKey: null },
    )
      .then((v) => {
        if (!disposed) {
          setItems(v.items);
          setTotal(v.total);
        }
      })
      .catch((e) => {
        if (!disposed) setError(message(e));
      });
    return () => {
      disposed = true;
    };
  }, [open, page]);
  async function compare() {
    try {
      setComparison(
        await call<AssessmentComparison>("assessment_compare", {
          first: selected[0],
          second: selected[1],
        }),
      );
      setError("");
    } catch (e) {
      setError(message(e));
    }
  }
  return (
    <details
      className="assessment-history"
      onToggle={(e) => setOpen(e.currentTarget.open)}
    >
      <summary>测试历史与比较</summary>
      {error && (
        <p role="alert" className="error">
          {error}
        </p>
      )}
      {items.map((i) => (
        <div className="assessment-history-row" key={i.id}>
          <label className="checkbox">
            <input
              aria-label={`选择比较 ${new Date(i.createdAt).toLocaleString()}`}
              type="checkbox"
              checked={selected.includes(i.id)}
              disabled={selected.length === 2 && !selected.includes(i.id)}
              onChange={(e) =>
                setSelected((old) =>
                  e.target.checked
                    ? [...old, i.id]
                    : old.filter((id) => id !== i.id),
                )
              }
            />
          </label>
          <button className="text-button" onClick={() => onOpen(i.id)}>
            {new Date(i.createdAt).toLocaleString()} ·{" "}
            {assessmentStateNames[i.state]} · {i.counts.processed}/
            {i.counts.planned} 题{i.preservedCopy ? " · 保留副本" : ""}
          </button>
          <span>
            正确 {rate(i.counts.correctRate)} · 无提示且确定{" "}
            {rate(i.counts.independentRate)}
          </span>
        </div>
      ))}
      {!total && open && <p className="muted">尚无已结束的测试。</p>}
      <div className="pagination">
        <button disabled={page === 0} onClick={() => setPage((p) => p - 1)}>
          上一页
        </button>
        <span>
          {page + 1} / {Math.max(1, Math.ceil(total / 20))} · 共 {total} 次
        </span>
        <button
          disabled={(page + 1) * 20 >= total}
          onClick={() => setPage((p) => p + 1)}
        >
          下一页
        </button>
        <button disabled={selected.length !== 2} onClick={() => void compare()}>
          比较所选两次
        </button>
      </div>
      {comparison && (
        <section className="assessment-compare">
          <h3>
            {comparison.comparable ? "相同配置下的表现" : "不同配置，并列查看"}
          </h3>
          <p>{comparison.explanation}</p>
          <table>
            <thead>
              <tr>
                <th>测试</th>
                <th>有效／计划</th>
                <th>正确率</th>
                <th>无提示且确定</th>
                <th>近期重复</th>
              </tr>
            </thead>
            <tbody>
              {[comparison.first, comparison.second].map((r) => (
                <tr key={r.sessionId}>
                  <td>{new Date(r.createdAt).toLocaleString()}</td>
                  <td>
                    {r.counts.valid}/{r.counts.planned}
                  </td>
                  <td>{rate(r.counts.correctRate)}</td>
                  <td>{rate(r.counts.independentRate)}</td>
                  <td>{r.repeatedTargets}</td>
                </tr>
              ))}
            </tbody>
          </table>
          {comparison.comparable && comparison.correctRateDelta !== null && (
            <p>
              正确率差值：{(comparison.correctRateDelta * 100).toFixed(1)}{" "}
              个百分点；无提示且确定差值：
              {comparison.independentRateDelta === null
                ? "暂无"
                : (comparison.independentRateDelta * 100).toFixed(1)}{" "}
              个百分点。
            </p>
          )}
          <details>
            <summary>配置与样本限制</summary>
            {[comparison.first, comparison.second].map((r) => (
              <section key={r.sessionId}>
                <h4>{new Date(r.createdAt).toLocaleString()}</h4>
                <p>
                  {assessmentScopeLabel(r.config)} · 每段 {r.config.bandSize}
                  <br />
                  题库版本：{r.bankVersions.join("、")}
                </p>
                {r.warnings.map((w, n) => (
                  <p className="muted" key={n}>
                    {w}
                  </p>
                ))}
              </section>
            ))}
          </details>
        </section>
      )}
    </details>
  );
}
