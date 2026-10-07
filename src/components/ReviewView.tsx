import { useCallback, useEffect, useRef, useState } from "react";
import { call, message, uid, waitTask } from "../api";
import type {
  Explanation,
  ReviewAttempt,
  ReviewOutcome,
  ReviewQuestion,
  ReviewUnit,
  TaskSnapshot,
} from "../types";
import { AudioPlayer } from "./AudioPlayer";

const outcomeNames: Record<ReviewOutcome, string> = {
  correct: "独立回忆正确",
  correct_with_hint: "借助提示正确",
  incorrect: "需要重学",
  needs_confirmation: "请核对答案并确认判分",
};
const dimensionName = (value: string) =>
  value === "meaning" ? "词义回忆" : "听力识别";
const date = (value: number) =>
  value ? new Date(value).toLocaleString() : "现在";

export function ReviewView({
  special,
  refreshKey,
}: {
  special: boolean;
  refreshKey: number;
}) {
  const [mode, setMode] = useState("due"),
    [dimension, setDimension] = useState("");
  const [units, setUnits] = useState<ReviewUnit[]>([]),
    [history, setHistory] = useState<ReviewAttempt[]>([]);
  const [question, setQuestion] = useState<ReviewQuestion | null>(null),
    [result, setResult] = useState<ReviewAttempt | null>(null);
  const [answer, setAnswer] = useState(""),
    [hint, setHint] = useState(""),
    [error, setError] = useState("");
  const [busy, setBusy] = useState(false),
    [audio, setAudio] = useState<{ path: string; kind: string } | null>(null);
  const [played, setPlayed] = useState(false),
    [playKey, setPlayKey] = useState(0),
    [offset, setOffset] = useState(0);
  const [reason, setReason] = useState("我已对照语境与参考答案核对本次回答。"),
    [explanation, setExplanation] = useState<Explanation | null>(null);
  const generation = useRef(0);
  const submitReceipt = useRef<{ signature: string; id: string } | null>(null);
  const correctionReceipt = useRef<{ signature: string; id: string } | null>(
    null,
  );
  const refresh = useCallback(async () => {
    if (mode === "history" && !special)
      setHistory(
        await call<ReviewAttempt[]>("review_history", {
          unitId: null,
          offset,
          limit: 30,
        }),
      );
    else
      setUnits(
        await call<ReviewUnit[]>("review_units", {
          mode: special ? "leech" : mode,
          dimension,
          offset,
          limit: 30,
        }),
      );
  }, [mode, special, dimension, offset]);
  useEffect(() => {
    refresh().catch((e) => setError(message(e)));
  }, [refresh, refreshKey]);
  useEffect(
    () => () => {
      generation.current++;
    },
    [],
  );

  function closeQuestion() {
    generation.current++;
    setQuestion(null);
    setResult(null);
    setAnswer("");
    setHint("");
    setAudio(null);
    setPlayed(false);
    setExplanation(null);
    setError("");
    submitReceipt.current = null;
    correctionReceipt.current = null;
    refresh().catch((e) => setError(message(e)));
  }
  async function start(unit: ReviewUnit) {
    setBusy(true);
    setError("");
    try {
      const q = await call<ReviewQuestion>("review_question", {
        unitId: unit.id,
        expectedRevision: unit.revision,
      });
      setQuestion(q);
    } catch (e) {
      setError(message(e));
      refresh().catch(() => {});
    } finally {
      setBusy(false);
    }
  }
  async function play() {
    if (!question) return;
    if (audio) {
      setPlayKey((k) => k + 1);
      return;
    }
    const version = generation.current;
    setBusy(true);
    setError("");
    try {
      const task = await call<TaskSnapshot>("review_audio_start", {
        questionId: question.id,
        operationId: uid(),
      });
      const value = await waitTask<{ path: string; kind: string }>(task);
      if (version === generation.current) setAudio(value);
    } catch (e) {
      if (version === generation.current) setError(message(e));
    } finally {
      setBusy(false);
    }
  }
  async function submit(unable: boolean) {
    if (!question) return;
    setBusy(true);
    setError("");
    const signature = JSON.stringify({
      questionId: question.id,
      answer,
      unable,
    });
    if (submitReceipt.current?.signature !== signature)
      submitReceipt.current = { signature, id: uid() };
    try {
      setResult(
        await call<ReviewAttempt>("review_submit", {
          input: {
            operationId: submitReceipt.current.id,
            questionId: question.id,
            answer,
            unable,
          },
        }),
      );
    } catch (e) {
      setError(message(e));
    } finally {
      setBusy(false);
    }
  }
  async function correct(attempt: ReviewAttempt, outcome: ReviewOutcome) {
    setBusy(true);
    setError("");
    const signature = JSON.stringify({
      id: attempt.id,
      revision: attempt.revision,
      outcome,
      reason,
    });
    if (correctionReceipt.current?.signature !== signature)
      correctionReceipt.current = { signature, id: uid() };
    try {
      const next = await call<ReviewAttempt>("review_correct", {
        input: {
          operationId: correctionReceipt.current.id,
          attemptId: attempt.id,
          expectedRevision: attempt.revision,
          outcome,
          reason,
        },
      });
      if (result?.id === next.id) setResult(next);
      await refresh();
    } catch (e) {
      setError(message(e));
    } finally {
      setBusy(false);
    }
  }
  async function explain(attempt: ReviewAttempt) {
    setBusy(true);
    setError("");
    try {
      const task = await call<TaskSnapshot>("explain_start", {
        text: attempt.question.target,
        context: attempt.question.context,
        operationId: uid(),
      });
      setExplanation(await waitTask<Explanation>(task));
    } catch (e) {
      setError(message(e));
    } finally {
      setBusy(false);
    }
  }

  return (
    <section className="review-view">
      {error && (
        <p className="error" role="alert">
          {error}
        </p>
      )}
      {question ? (
        <article className="review-card">
          <div className="toolbar">
            <span className="badge">{dimensionName(question.dimension)}</span>
            <button disabled={busy} onClick={closeQuestion}>
              {result ? "返回列表 / 继续下一题" : "退出本题（不计作答）"}
            </button>
          </div>
          <h2>{question.prompt}</h2>
          {question.context && <p className="quote">{question.context}</p>}
          {!result && (
            <>
              {question.dimension === "listening" && (
                <>
                  <button disabled={busy} className="secondary" onClick={play}>
                    {audio
                      ? "再次播放"
                      : `播放${question.audioKind === "original" ? "原声" : "系统语音"}`}
                  </button>
                  {audio && (
                    <AudioPlayer
                      path={audio.path}
                      label={audio.kind === "original" ? "原声" : "系统语音"}
                      playbackKey={playKey}
                      onPlayed={() => setPlayed(true)}
                      onFailed={() => setPlayed(false)}
                    />
                  )}
                  <p className="muted">
                    先播放音频，再提交回答。音频不可用时可退出，本次不计学习失败。
                  </p>
                </>
              )}
              <label className="review-answer">
                {question.dimension === "meaning"
                  ? "用自己的话写出这个语境中的含义"
                  : "写出听到的目标内容"}
                <textarea
                  rows={3}
                  autoFocus
                  value={answer}
                  onChange={(e) => setAnswer(e.target.value)}
                  disabled={busy}
                />
              </label>
              {hint && (
                <p className="notice banner">
                  {hint} 使用提示会保留在学习记录中。
                </p>
              )}
              <div className="actions">
                <button
                  disabled={busy || !!hint}
                  onClick={async () => {
                    setBusy(true);
                    try {
                      setHint(
                        await call<string>("review_hint", {
                          questionId: question.id,
                        }),
                      );
                    } catch (e) {
                      setError(message(e));
                    } finally {
                      setBusy(false);
                    }
                  }}
                >
                  首字提示
                </button>
                <button
                  className="primary"
                  disabled={
                    busy ||
                    !answer.trim() ||
                    (question.dimension === "listening" && !played)
                  }
                  onClick={() => submit(false)}
                >
                  提交回答
                </button>
                <button
                  disabled={
                    busy || (question.dimension === "listening" && !played)
                  }
                  onClick={() => submit(true)}
                >
                  暂时想不起来
                </button>
              </div>
            </>
          )}
          {result && (
            <div className="review-feedback" role="status">
              <h3>{outcomeNames[result.outcome]}</h3>
              <p>
                你的回答：{result.answer || "暂时想不起来"}
                {result.hinted && "（已使用提示）"}
              </p>
              <p>参考答案：{result.question.expected.join("；")}</p>
              {question.dimension === "listening" && (
                <>
                  <p>目标：{result.question.target}</p>
                  <p className="quote">{result.question.context}</p>
                </>
              )}
              <p className="muted">
                {result.outcome === "needs_confirmation"
                  ? "当前回答已保存，等待确认；复习安排暂未改变。"
                  : `下次复习：${date(result.state.dueAt)}。${result.state.leechActive ? "仍在专项中，需要三次到期独立回忆通过。" : ""}`}
              </p>
              <label>
                判分确认或修正理由
                <input
                  value={reason}
                  onChange={(e) => setReason(e.target.value)}
                />
              </label>
              <div className="actions">
                <button
                  disabled={busy || !reason.trim()}
                  onClick={() => correct(result, "correct")}
                >
                  符合语境，回答正确
                </button>
                <button
                  disabled={busy || !reason.trim()}
                  onClick={() => correct(result, "correct_with_hint")}
                >
                  借助提示才想起
                </button>
                <button
                  disabled={busy || !reason.trim()}
                  onClick={() => correct(result, "incorrect")}
                >
                  回答不正确
                </button>
                <button disabled={busy} onClick={() => explain(result)}>
                  请本地模型解释用法
                </button>
              </div>
              {result.corrections.map((c, i) => (
                <p className="muted" key={i}>
                  判分修正：{outcomeNames[c.outcome]} · {c.reason}
                </p>
              ))}
              {explanation && (
                <div className="suggestion">
                  <p>{explanation.meaning}</p>
                  <p>{explanation.translation}</p>
                  <p>{explanation.notes}</p>
                </div>
              )}
            </div>
          )}
        </article>
      ) : (
        <>
          <div className="toolbar">
            {!special && (
              <select
                aria-label="复习列表"
                value={mode}
                disabled={busy}
                onChange={(e) => {
                  setMode(e.target.value);
                  setOffset(0);
                }}
              >
                <option value="due">到期复习</option>
                <option value="all">所有单元（可提前练习）</option>
                <option value="history">作答历史与判分修正</option>
              </select>
            )}
            {mode !== "history" && (
              <select
                aria-label="能力维度"
                value={dimension}
                disabled={busy}
                onChange={(e) => {
                  setDimension(e.target.value);
                  setOffset(0);
                }}
              >
                <option value="">词义与听力</option>
                <option value="meaning">词义回忆</option>
                <option value="listening">听力识别</option>
              </select>
            )}
            <button
              disabled={busy}
              onClick={() => refresh().catch((e) => setError(message(e)))}
            >
              刷新
            </button>
          </div>
          <p className="muted">
            {special
              ? "按薄弱维度练习。提前答对会保存历史；退出专项需要三次到期、无提示的正确回答。"
              : "词义与听力分别安排。自由浏览不计测验；提前答对不会延长间隔。"}
          </p>
          {mode === "history" && !special ? (
            <div className="task-list">
              {history.map((a) => (
                <article className="task-card" key={a.id}>
                  <header>
                    <strong>
                      {a.question.target} ·{" "}
                      {dimensionName(a.question.dimension)}
                    </strong>
                    <span className="badge">{outcomeNames[a.outcome]}</span>
                  </header>
                  <p>
                    {date(a.createdAt)} · 回答：{a.answer || "暂时想不起来"}
                    {a.hinted && "（使用提示）"}
                  </p>
                  <p>当时参考答案：{a.question.expected.join("；")}</p>
                  <p className="muted">
                    初次判分：{outcomeNames[a.originalOutcome]}
                  </p>
                  {a.corrections.map((c, i) => (
                    <p className="muted" key={i}>
                      {date(c.createdAt)} · {outcomeNames[c.outcome]} ·{" "}
                      {c.reason}
                    </p>
                  ))}
                  <label>
                    修正理由
                    <input
                      value={reason}
                      onChange={(e) => setReason(e.target.value)}
                    />
                  </label>
                  <div className="actions">
                    <button
                      disabled={busy || !reason.trim()}
                      onClick={() => correct(a, "correct")}
                    >
                      改为正确
                    </button>
                    <button
                      disabled={busy || !reason.trim()}
                      onClick={() => correct(a, "correct_with_hint")}
                    >
                      改为有提示正确
                    </button>
                    <button
                      disabled={busy || !reason.trim()}
                      onClick={() => correct(a, "incorrect")}
                    >
                      改为错误
                    </button>
                  </div>
                </article>
              ))}
              {!history.length && <p className="empty">暂无作答历史。</p>}
            </div>
          ) : (
            <div className="review-grid">
              {units.map((unit) => (
                <article className="task-card" key={unit.id}>
                  <span className="badge">{dimensionName(unit.dimension)}</span>
                  <h3>{unit.text}</h3>
                  <p className="muted">
                    下次：{date(unit.state.dueAt)} · 独立成功{" "}
                    {unit.state.streak} 次 · 失败 {unit.state.lapses} 次
                  </p>
                  {unit.reasons.map((r) => (
                    <p key={r}>{r}</p>
                  ))}
                  <button
                    className="primary"
                    disabled={busy || !unit.available}
                    onClick={() => start(unit)}
                  >
                    开始练习
                  </button>
                  {!unit.available && (
                    <p className="muted">请在单词本补充释义。</p>
                  )}
                </article>
              ))}
              {!units.length && (
                <p className="empty">
                  {special
                    ? "当前没有易忘词专项。"
                    : "当前列表没有学习单元。可以收录词条，或查看所有单元。"}
                </p>
              )}
            </div>
          )}
          <div className="pagination">
            <button
              disabled={busy || offset === 0}
              onClick={() => setOffset(Math.max(0, offset - 30))}
            >
              上一页
            </button>
            <span>{Math.floor(offset / 30) + 1}</span>
            <button
              disabled={
                busy ||
                (mode === "history" ? history.length : units.length) < 30
              }
              onClick={() => setOffset(offset + 30)}
            >
              下一页
            </button>
          </div>
        </>
      )}
    </section>
  );
}
