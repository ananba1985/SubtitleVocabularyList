import { useCallback, useEffect, useState } from "react";
import { call, message } from "../api";
import { useAssessmentMutation } from "../assessmentApi";
import type {
  AssessmentCollectionDraft,
  AssessmentConfig,
  AssessmentFeedback,
  AssessmentReport,
  AssessmentReportGroup,
  AssessmentReportItem,
} from "../assessmentTypes";
import { assessmentStateNames, assessmentTypeNames } from "../assessmentTypes";
import { AssessmentFeedbackCard } from "./AssessmentQuestionPanel";
import { AssessmentCollectionPanel } from "./AssessmentCollectionPanel";
import { AudioPlayer } from "./AudioPlayer";
const rate = (value: number | null) =>
  value === null ? "暂无有效结果" : `${(value * 100).toFixed(1)}%`;
const resultNames: Record<string, string> = {
  correct: "正确",
  incorrect: "需巩固",
  unknown: "不知道",
  needs_confirmation: "待确认",
};
function GroupTable({ groups }: { groups: AssessmentReportGroup[] }) {
  return (
    <div className="assessment-table">
      <table>
        <thead>
          <tr>
            <th>范围</th>
            <th>有效／计划</th>
            <th>正确率</th>
            <th>无提示且确定</th>
            <th>未计分／未答</th>
          </tr>
        </thead>
        <tbody>
          {groups.map((g) => (
            <tr key={g.key}>
              <td>
                {g.label}
                {g.insufficientSample && <small>样本不足</small>}
              </td>
              <td>
                {g.counts.valid}/{g.counts.planned}
              </td>
              <td>{rate(g.counts.correctRate)}</td>
              <td>{rate(g.counts.independentRate)}</td>
              <td>
                {g.counts.unscored}/{g.counts.unanswered}
              </td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}
export function AssessmentReportPanel({
  sessionId,
  dataDirectory,
  onExit,
  onRetest,
}: {
  sessionId: string;
  dataDirectory: string;
  onExit: () => void;
  onRetest: (config: AssessmentConfig) => void;
}) {
  const [report, setReport] = useState<AssessmentReport | null>(null),
    [revisions, setRevisions] = useState<number[]>([]),
    [revision, setRevision] = useState<number | null>(null),
    [error, setError] = useState("");
  const [dimension, setDimension] = useState<
      "byBand" | "byContent" | "byDimension" | "byType"
    >("byBand"),
    [detailsOpen, setDetailsOpen] = useState(false),
    [page, setPage] = useState(0),
    [items, setItems] = useState<AssessmentReportItem[]>([]),
    [selected, setSelected] = useState<string[]>([]),
    [draft, setDraft] = useState<AssessmentCollectionDraft | null>(null),
    [busy, setBusy] = useState(false),
    [itemRefresh, setItemRefresh] = useState(0);
  const mutate = useAssessmentMutation();
  const refresh = useCallback(
    async (requested: number | null = null) => {
      const value = await call<{
        report: AssessmentReport;
        revisions: number[];
      }>("assessment_report_get", { sessionId, reportRevision: requested });
      setReport(value.report);
      setRevisions(value.revisions);
      setItemRefresh((v) => v + 1);
    },
    [sessionId],
  );
  useEffect(() => {
    let disposed = false;
    setError("");
    void call<{ report: AssessmentReport; revisions: number[] }>(
      "assessment_report_get",
      { sessionId, reportRevision: revision },
    )
      .then((v) => {
        if (!disposed) {
          setReport(v.report);
          setRevisions(v.revisions);
          setItemRefresh((n) => n + 1);
        }
      })
      .catch((e) => {
        if (!disposed) setError(message(e));
      });
    return () => {
      disposed = true;
    };
  }, [sessionId, revision]);
  useEffect(() => {
    if (!detailsOpen || !report) return;
    let disposed = false;
    setItems([]);
    void call<{ items: AssessmentReportItem[] }>("assessment_report_items", {
      sessionId,
      reportRevision: report.revision,
      offset: page * 20,
      limit: 20,
    })
      .then((v) => {
        if (!disposed) setItems(v.items);
      })
      .catch((e) => {
        if (!disposed) setError(message(e));
      });
    return () => {
      disposed = true;
    };
  }, [detailsOpen, report?.revision, page, sessionId, itemRefresh]);
  async function prepare() {
    setBusy(true);
    setError("");
    try {
      setDraft(
        await mutate<AssessmentCollectionDraft>(
          "assessment_collection_prepare",
          { sessionId, itemIds: selected },
        ),
      );
    } catch (e) {
      setError(message(e));
    } finally {
      setBusy(false);
    }
  }
  if (!report) return <p role="status">{error || "正在读取测试报告…"}</p>;
  const c = report.counts,
    latest = revision === null || revision === revisions[0];
  return (
    <div className="assessment-report">
      <div className="assessment-line">
        <div>
          <h2>本次抽样表现</h2>
          <p className="muted">
            {new Date(report.createdAt).toLocaleString()} ·{" "}
            {assessmentStateNames[report.state]} ·{" "}
            {report.status === "provisional" ? "暂定判分" : "判分已确定"}
          </p>
        </div>
        <div className="actions">
          <button onClick={onExit}>返回水平测试</button>
          <button onClick={() => onRetest(report.config)}>按此配置复测</button>
        </div>
      </div>
      {error && (
        <p role="alert" className="error">
          {error}
        </p>
      )}
      <div className="assessment-report-stats">
        <div>
          <strong>{rate(c.correctRate)}</strong>
          <span>
            答对 {c.correct} / 有效 {c.valid}
          </span>
        </div>
        <div>
          <strong>{rate(c.independentRate)}</strong>
          <span>
            无提示且确定 {c.independentCorrect} / 有效 {c.valid}
          </span>
        </div>
        <div>
          <strong>
            {c.processed} / {c.planned}
          </strong>
          <span>已处理／计划 · 未答 {c.unanswered}</span>
        </div>
      </div>
      <p>
        不知道 {c.unknown} · 不正确 {c.incorrect} · 不确定 {c.uncertain} ·
        使用提示 {c.hinted} · 未计分 {c.unscored}
        {c.needsConfirmation > 0 ? `（待确认 ${c.needsConfirmation}）` : ""}
      </p>
      <details
        className="assessment-report-notes"
        open={
          c.valid < report.config.minimumSample ||
          report.state === "ended_early" ||
          report.status === "provisional"
        }
      >
        <summary>样本与计分说明</summary>
        {report.warnings.map((w, n) => (
          <p key={n} className="muted">
            {w}
          </p>
        ))}
        <p className="muted">
          已处理 = 有效 +
          未计分；不知道和错误计入有效分母。无提示且确定的正确率使用相同分母。不确定和提示数可重叠，不能相加当成题数。
        </p>
        <p className="muted">
          评分依据：{report.scorePolicy} · 近期重复 {report.repeatedTargets}
        </p>
        {report.coverage.map((v) => (
          <p key={v.catalogId} className="muted">
            Top {v.topN}：原始目标 {v.rawTargets}，词元 {v.lexemeTargets}
            ，已准备 {v.preparedTargets}，缺题 {v.missingTargets}
          </p>
        ))}
      </details>
      <div className="assessment-line">
        <label>
          查看汇总
          <select
            value={dimension}
            onChange={(e) => setDimension(e.target.value as typeof dimension)}
          >
            <option value="byBand">按频段</option>
            <option value="byContent">按单词／组合</option>
            <option value="byDimension">按能力维度</option>
            <option value="byType">按题型</option>
          </select>
        </label>
        {revisions.length > 1 && (
          <label>
            报告修订
            <select
              value={revision ?? ""}
              onChange={(e) => {
                setRevision(e.target.value ? Number(e.target.value) : null);
                setSelected([]);
                setDraft(null);
              }}
            >
              <option value="">最新修订</option>
              {revisions.map((r) => (
                <option key={r} value={r}>
                  修订 {r}
                </option>
              ))}
            </select>
          </label>
        )}
      </div>
      <GroupTable groups={report[dimension]} />
      <details>
        <summary>频段与题型交叉明细</summary>
        <GroupTable groups={report.details} />
      </details>
      <details
        className="assessment-report-items"
        onToggle={(e) => setDetailsOpen(e.currentTarget.open)}
      >
        <summary>逐题讲评与确认收录（{report.itemTotal}）</summary>
        {!latest && (
          <p className="muted">
            正在查看旧修订，原作答及当时结论保持；需要修正请切换最新报告。
          </p>
        )}
        <div className="actions">
          <button
            onClick={() =>
              setSelected((old) =>
                Array.from(
                  new Set([
                    ...old,
                    ...items
                      .filter(
                        (i) =>
                          !i.collectedEntryIds.length &&
                          (!i.answer ||
                            i.answer.result !== "correct" ||
                            i.answer.hinted ||
                            i.answer.uncertain ||
                            !!i.answer.unscoredReason),
                      )
                      .map((i) => i.id),
                  ]),
                ),
              )
            }
          >
            选择本页待巩固项
          </button>
          <button
            disabled={!selected.length || busy}
            onClick={() => void prepare()}
          >
            预览收录（{selected.length}）
          </button>
          <button disabled={!selected.length} onClick={() => setSelected([])}>
            清空选择
          </button>
        </div>
        {items.map((row) => {
          const a = row.answer,
            d = row.item.content;
          const feedback: AssessmentFeedback | null = a
            ? {
                answerId: a.id,
                answerRevision: row.answerRevision!,
                sessionItemId: row.id,
                questionType: row.item.questionType,
                target: d.target,
                sentence: d.sentence,
                meaning: d.meaning,
                translation: d.translation,
                explanation: d.explanation,
                acceptedAnswers: d.acceptedAnswers,
                result: a.result,
                unscoredReason: a.unscoredReason,
                uncertain: a.uncertain,
                hinted: a.hinted,
              }
            : null;
          return (
            <article
              className="assessment-report-item"
              key={`${row.id}:${row.answerRevision}`}
            >
              <label className="checkbox">
                <input
                  type="checkbox"
                  checked={selected.includes(row.id)}
                  onChange={(e) =>
                    setSelected((old) =>
                      e.target.checked
                        ? [...old, row.id]
                        : old.filter((id) => id !== row.id),
                    )
                  }
                />
                <strong>
                  {row.position + 1}. {d.target}
                </strong>
                <span>
                  {assessmentTypeNames[row.item.questionType]} ·{" "}
                  {a
                    ? a.unscoredReason
                      ? "未计分"
                      : (resultNames[a.result] ?? a.result)
                    : "未作答"}
                </span>
              </label>
              {!!row.collectedEntryIds.length && (
                <p className="muted">
                  已关联 {row.collectedEntryIds.length} 个单词本词条
                </p>
              )}
              <details>
                <summary>查看题目、原回答和修正</summary>
                <p>{d.prompt}</p>
                <p>
                  原回答：
                  {a
                    ? a.originalAction !== "answer"
                      ? "不知道／跳过"
                      : a.text ||
                        d.choices.find((c) => c.id === a.optionId)?.text ||
                        "—"
                    : "未作答"}
                </p>
                {feedback && latest ? (
                  <AssessmentFeedbackCard
                    value={feedback}
                    onChanged={async () => {
                      await refresh(null);
                      setRevision(null);
                    }}
                  />
                ) : (
                  <>
                    <p>{d.sentence}</p>
                    <p>
                      {d.meaning} · {d.translation}
                    </p>
                    <p>{d.explanation}</p>
                  </>
                )}
                {d.audio && (
                  <AudioPlayer
                    path={`${dataDirectory}/${d.audio.relativePath}`}
                    label={
                      d.audio.kind === "original"
                        ? "真实原声"
                        : "Windows 本地英语语音"
                    }
                  />
                )}
                {row.originalAnswer && (
                  <p className="muted">
                    原判分：
                    {row.originalAnswer.unscoredReason
                      ? "未计分"
                      : (resultNames[row.originalAnswer.result] ??
                        row.originalAnswer.result)}
                  </p>
                )}
                {row.corrections.map((v) => (
                  <p key={v.id} className="muted">
                    修正 {v.revision} · {new Date(v.createdAt).toLocaleString()}{" "}
                    · {v.result}：{v.reason}
                    {v.addHint ? "（补充提示事实）" : ""}
                  </p>
                ))}
              </details>
            </article>
          );
        })}
        <div className="pagination">
          <button disabled={page === 0} onClick={() => setPage((p) => p - 1)}>
            上一页
          </button>
          <span>
            {page + 1} / {Math.max(1, Math.ceil(report.itemTotal / 20))}
          </span>
          <button
            disabled={(page + 1) * 20 >= report.itemTotal}
            onClick={() => setPage((p) => p + 1)}
          >
            下一页
          </button>
        </div>
      </details>
      {draft && (
        <AssessmentCollectionPanel
          key={draft.draftId}
          draft={draft}
          onClose={() => setDraft(null)}
          onSaved={async () => {
            setRevision(null);
            setSelected([]);
            await refresh(null);
          }}
        />
      )}
    </div>
  );
}
