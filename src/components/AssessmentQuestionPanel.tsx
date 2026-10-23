import { useRuntimeFeatures } from "../runtimeFeatures";
import { MobileAudioPlayer } from "../mobile/MobileAudioPlayer";
import { useEffect, useRef, useState } from "react";
import { audioUrl, call, message, uid, waitTask } from "../api";

import { useAssessmentMutation } from "../assessmentApi";
import { assessmentStateNames, assessmentTypeNames } from "../assessmentTypes";
import type {
  AssessmentFeedback,
  AssessmentPlayback,
  AssessmentSession,
} from "../assessmentTypes";

const results: Record<string, string> = {
  correct: "正确",
  incorrect: "需要巩固",
  unknown: "不知道",
  needs_confirmation: "待核对判分",
};
export function AssessmentFeedbackCard({
  value,
  onChanged,
}: {
  value: AssessmentFeedback;
  onChanged: () => Promise<void>;
}) {
  const [reason, setReason] = useState(""),
    [busy, setBusy] = useState(false),
    [error, setError] = useState("");
  const mutate = useAssessmentMutation();
  async function correct(result: string) {
    setBusy(true);
    setError("");
    try {
      await mutate("assessment_result_correct", {
        answerId: value.answerId,
        expectedRevision: value.answerRevision,
        result,
        addHint: false,
        reason,
      });
      await onChanged();
    } catch (e) {
      setError(message(e));
    } finally {
      setBusy(false);
    }
  }
  return (
    <section className="assessment-feedback" aria-label="上一题讲评">
      <div className="assessment-line">
        <strong>
          {value.target} · {value.meaning}
        </strong>
        <span className="badge">
          {value.unscoredReason
            ? "未计分"
            : (results[value.result] ?? value.result)}
        </span>
      </div>
      {(value.hinted || value.uncertain) && (
        <p className="muted">
          {value.hinted && "使用过提示　"}
          {value.uncertain && "作答时不确定"}
        </p>
      )}
      {value.unscoredReason && (
        <p className="muted">
          {value.unscoredReason === "invalid_item"
            ? "题目已标为无效，保留原作答。"
            : "没有有效播放完成记录，本题不计为听力错误。"}
        </p>
      )}
      <details open={value.result === "needs_confirmation"}>
        <summary>
          答案与讲解
          {value.result === "needs_confirmation" ? " · 请核对合理变体" : ""}
        </summary>
        <p>{value.sentence}</p>
        <p>{value.translation}</p>
        <p>{value.explanation}</p>
        {!!value.acceptedAnswers.length && (
          <p>已核对答案：{value.acceptedAnswers.join(" / ")}</p>
        )}
        <label>
          判分修正依据
          <input
            value={reason}
            onChange={(e) => setReason(e.target.value)}
            placeholder="说明为何可接受、错误或题目无效"
          />
        </label>
        <div className="actions">
          <button
            disabled={busy || !reason.trim() || !!value.unscoredReason}
            onClick={() => void correct("correct")}
          >
            确认可接受
          </button>
          <button
            disabled={busy || !reason.trim() || !!value.unscoredReason}
            onClick={() => void correct("incorrect")}
          >
            判为不正确
          </button>
          <button
            disabled={busy || !reason.trim()}
            onClick={() => void correct("invalid")}
          >
            题目无效
          </button>
        </div>
        {error && (
          <p className="error" role="alert">
            {error}
          </p>
        )}
      </details>
    </section>
  );
}

export function AssessmentQuestionPanel({
  session,
  onChanged,
  onExit,
}: {
  session: AssessmentSession;
  onChanged: (value: AssessmentSession) => void;
  onExit: () => void;
}) {
  const { mobile } = useRuntimeFeatures();
  const question = session.question;
  const [text, setText] = useState(question?.draftText ?? ""),
    [option, setOption] = useState<string | null>(
      question?.draftOption ?? null,
    ),
    [uncertain, setUncertain] = useState(false);
  const [busy, setBusy] = useState(false),
    [error, setError] = useState(""),
    [saved, setSaved] = useState(false),
    [confirmEnd, setConfirmEnd] = useState(false);
  const [playback, setPlayback] = useState<AssessmentPlayback | null>(null),
    [audioBusy, setAudioBusy] = useState(false),
    [played, setPlayed] = useState(question?.audioPlayed ?? false);
  const audio = useRef<HTMLAudioElement>(null),
    audioChain = useRef<Promise<unknown>>(Promise.resolve()),
    generation = useRef(0);
  const mutate = useAssessmentMutation();
  useEffect(() => {
    generation.current++;
    setText(question?.draftText ?? "");
    setOption(question?.draftOption ?? null);
    setUncertain(false);
    setPlayback(null);
    setAudioBusy(false);
    setPlayed(question?.audioPlayed ?? false);
    setError("");
    setSaved(false);
  }, [question?.id]);
  useEffect(() => {
    if (playback?.path)
      void audio.current
        ?.play()
        .catch(() => setError("请点击播放器播放；播放完成后可提交听音答案。"));
  }, [playback?.playbackId]);
  useEffect(() => {
    const stop = (e: Event) => {
      if ((e as CustomEvent<HTMLAudioElement>).detail !== audio.current)
        audio.current?.pause();
    };
    window.addEventListener("svl_audio_started", stop);
    return () => {
      generation.current++;
      window.removeEventListener("svl_audio_started", stop);
    };
  }, []);
  async function refresh() {
    onChanged(
      await call("assessment_session_get", {
        sessionId: session.id,
      }),
    );
  }
  async function transition(action: string, source = session) {
    return mutate("assessment_session_transition", {
      sessionId: source.id,
      expectedRevision: source.revision,
      action,
      itemId: source.question?.id ?? null,
      draftText: text,
      draftOption: option,
    });
  }
  async function control(action: string) {
    setBusy(true);
    setError("");
    setSaved(false);
    try {
      let next = session;
      if (action === "pause" && session.state === "active")
        next = await transition("save_draft");
      next = await transition(action, next);
      onChanged(next);
      if (action === "save_draft") setSaved(true);
    } catch (e) {
      setError(message(e));
    } finally {
      setBusy(false);
    }
  }
  async function submit(action: string) {
    if (!question) return;
    setBusy(true);
    setError("");
    try {
      onChanged(
        await mutate("assessment_answer_submit", {
          sessionId: session.id,
          itemId: question.id,
          expectedRevision: session.revision,
          action,
          optionId: option,
          text,
          uncertain,
        }),
      );
    } catch (e) {
      setError(message(e));
    } finally {
      setBusy(false);
    }
  }
  async function play() {
    if (!question) return;
    const version = generation.current;
    setAudioBusy(true);
    setError("");
    try {
      const prepared = await call("assessment_audio_prepare", {
        sessionId: session.id,
        itemId: question.id,
        operationId: uid(),
      });
      await waitTask(prepared);
      if (version !== generation.current) return;
      const next = await mutate("assessment_audio_start", {
        sessionId: session.id,
        itemId: question.id,
        expectedRevision: session.revision,
      });
      if (version !== generation.current) return;
      if (next.error) setError(next.error.message);
      setPlayback(next);
      audioChain.current = Promise.resolve();
    } catch (e) {
      if (version === generation.current) setError(message(e));
    } finally {
      if (version === generation.current) setAudioBusy(false);
    }
  }
  function audioEvent(event: string, seconds: number, errorText = "") {
    if (!question || !playback) return;
    const version = generation.current;
    const input = {
      sessionId: session.id,
      itemId: question.id,
      playbackId: playback.playbackId,
      operationId: uid(),
      event,
      seconds,
      error: errorText,
    };
    audioChain.current = audioChain.current
      .then(() => call("assessment_audio_event", { input }))
      .then((value) => {
        if (version === generation.current && value.played) setPlayed(true);
      })
      .catch((e) => {
        if (version === generation.current) setError(message(e));
      });
  }
  return (
    <div className="assessment-work">
      <div className="assessment-line">
        <strong>
          {assessmentStateNames[session.state]} · 已处理 {session.processed} /{" "}
          {session.planned}
        </strong>
        <div className="actions">
          {session.state === "active" && (
            <button disabled={busy} onClick={() => void control("pause")}>
              暂停
            </button>
          )}
          {session.state !== "active" && (
            <button onClick={onExit}>返回水平测试</button>
          )}
          {(session.state === "active" || session.state === "paused") && (
            <button
              className="text-button"
              disabled={busy}
              onClick={() => setConfirmEnd(true)}
            >
              提前结束
            </button>
          )}
        </div>
      </div>
      <progress
        aria-label="水平测试进度"
        max={session.planned}
        value={session.processed}
      />
      {error && (
        <p className="error" role="alert">
          {error}
        </p>
      )}
      {confirmEnd && (
        <div className="assessment-confirm">
          <p>保留已经作答的内容并结束本次测试，未答部分会明确标注。</p>
          <div className="actions">
            <button
              disabled={busy}
              onClick={() => {
                setConfirmEnd(false);
                void control("end_early");
              }}
            >
              保留已答并结束
            </button>
            <button onClick={() => setConfirmEnd(false)}>继续本次测试</button>
          </div>
        </div>
      )}
      {session.state === "paused" && (
        <section className="assessment-pause">
          <h2>已保存进度</h2>
          <p>继续后仍是原来的题目、选项和位置。</p>
          <div className="actions">
            <button
              className="primary"
              disabled={busy}
              onClick={() => void control("resume")}
            >
              继续测试
            </button>
            <button onClick={onExit}>暂时返回</button>
          </div>
        </section>
      )}
      {question && (
        <form
          className="assessment-question"
          onSubmit={(e) => {
            e.preventDefault();
            void submit("answer");
          }}
        >
          <div className="assessment-line">
            <span className="badge">
              {assessmentTypeNames[question.questionType]}
            </span>
            <span className="muted">第 {question.position + 1} 题</span>
          </div>
          <p className="assessment-prompt">{question.prompt}</p>
          {question.questionType === "listening" && (
            <div className="assessment-listening">
              <div className="actions">
                <button
                  type="button"
                  className="secondary"
                  disabled={audioBusy || busy}
                  onClick={() => void play()}
                >
                  {audioBusy
                    ? "正在打开音频…"
                    : playback
                      ? "重新播放"
                      : "播放英语录音"}
                </button>
                <span className="muted">
                  {question.audioKind === "original"
                    ? "真实原声"
                    : "本机离线英语语音"}
                  {played ? " · 已播放完成" : ""}
                </span>
              </div>
              {playback?.path && (mobile ? <MobileAudioPlayer key={playback.playbackId} path={playback.path} label="英语录音" onPlaybackEvent={audioEvent}/> :
                <audio
                  key={playback.playbackId}
                  ref={audio}
                  controls
                  src={audioUrl(playback.path)}
                  onPlay={() => {
                    window.dispatchEvent(
                      new CustomEvent("svl_audio_started", {
                        detail: audio.current,
                      }),
                    );
                    audioEvent("started", audio.current?.currentTime ?? 0);
                  }}
                  onEnded={() =>
                    audioEvent("ended", audio.current?.currentTime ?? 0)
                  }
                  onError={() =>
                    audioEvent(
                      "error",
                      audio.current?.currentTime ?? 0,
                      "播放器未能加载本地音频。",
                    )
                  }
                />
              )}
              {!played && (
                <p className="muted">
                  先听完录音再提交答案；未能播放可跳过，本题将标为未计分。
                </p>
              )}
            </div>
          )}
          {question.questionType === "cloze" ? (
            <label>
              填入符合语境的英语表达
              <input
                autoComplete="off"
                value={text}
                onChange={(e) => {
                  setText(e.target.value);
                  setSaved(false);
                }}
                placeholder="输入答案，或选择不知道"
              />
            </label>
          ) : (
            <fieldset className="assessment-choices">
              <legend className="sr-only">选择一个含义</legend>
              {question.choices.map((choice, index) => (
                <label
                  key={choice.id}
                  className={option === choice.id ? "selected" : ""}
                >
                  <input
                    type="radio"
                    name="assessment-choice"
                    value={choice.id}
                    checked={option === choice.id}
                    onChange={() => {
                      setOption(choice.id);
                      setSaved(false);
                    }}
                  />
                  <span>
                    {String.fromCharCode(65 + index)}. {choice.text}
                  </span>
                </label>
              ))}
            </fieldset>
          )}
          {question.hint && <p className="assessment-hint">{question.hint}</p>}
          <label className="assessment-check">
            <input
              type="checkbox"
              checked={uncertain}
              onChange={(e) => setUncertain(e.target.checked)}
            />
            我不太确定
          </label>
          <div className="actions assessment-answer-actions">
            <button
              className="primary"
              type="submit"
              disabled={
                busy ||
                (question.questionType === "cloze" ? !text.trim() : !option) ||
                (question.questionType === "listening" && !played)
              }
            >
              确认答案
            </button>
            <button
              type="button"
              disabled={busy}
              onClick={() => void submit("unknown")}
            >
              不知道／想不起来
            </button>
            <button
              type="button"
              className="text-button"
              disabled={busy}
              onClick={() => void control("request_hint")}
            >
              需要提示
            </button>
            <button
              type="button"
              className="text-button"
              disabled={busy}
              onClick={() => void control("save_draft")}
            >
              保存草稿
            </button>
            {saved && (
              <span className="muted" role="status">
                草稿已保存
              </span>
            )}
          </div>
        </form>
      )}
      {(session.state === "completed" || session.state === "ended_early") && (
        <section className="assessment-pause">
          <h2>
            {session.state === "completed"
              ? "本次作答已完成"
              : "本次测试已提前结束"}
          </h2>
          <p>
            已处理 {session.processed} 题，未作答{" "}
            {session.planned - session.processed} 题。
          </p>
        </section>
      )}
      {session.lastFeedback && (
        <AssessmentFeedbackCard
          key={session.lastFeedback.answerId}
          value={session.lastFeedback}
          onChanged={refresh}
        />
      )}
    </div>
  );
}
