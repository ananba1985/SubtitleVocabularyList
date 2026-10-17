import { message } from "../api";
import {
  adjacentSentence,
  highlightCopy,
  pastedDocument,
  practiceProgress,
} from "../practice";
import {
  usePracticeSession,
  type PracticePanel,
} from "../practice/usePracticeSession";
import type { CollectionSeed } from "../types";
import { OnlineLookup } from "./OnlineLookup";
export type { PracticePanel } from "../practice/usePracticeSession";

const panelTitles = {
  import: "导入阅读或课程",
  contents: "章节与句子",
  settings: "播放设置",
  language: "解释与查询",
};

export function PracticeView({
  dataDirectory,
  collect,
  review,
  panel,
  onPanelChange,
}: {
  dataDirectory: string;
  collect: (seed: CollectionSeed) => void;
  review: () => void;
  panel: PracticePanel;
  onPanelChange: (panel: PracticePanel) => void;
}) {
  const {
    error,
    state,
    sentence,
    watching,
    play,
    watchReturn,
    stopWatching,
    setWatching,
    playing,
    stopPlayback,
    setStatus,
    status,
    voices,
    explaining,
    selected,
    explain,
    collectSentence,
    setSelected,
    setExplanation,
    reading,
    highlights,
    editor,
    inputCopy,
    feedback,
    move,
    chapter,
    panelDialog,
    importing,
    setImporting,
    setError,
    importDocument,
    title,
    text,
    importFile,
    setTitle,
    setText,
    activeSentence,
    update,
    current,
    explanation,
    audio,
    finishPlayback,
    audioTick,
  } = usePracticeSession({
    dataDirectory,
    collect,
    panel,
    onPanelChange,
  });
  return (
    <section className="practice-view">
      {error && (
        <p className="error practice-error" role="alert">
          {error}
        </p>
      )}
      {!state || !sentence ? (
        <p className="muted">正在读取本地练习…</p>
      ) : (
        <>
          <div className="practice-document-bar">
            <div className="practice-document-title">
              <strong title={state.document.title}>
                {state.document.title}
              </strong>
              <span className="muted">{state.document.kind}</span>
            </div>
            <div className="actions">
              <button
                aria-controls="practice-panel"
                aria-expanded={panel === "contents"}
                onClick={() => onPanelChange("contents")}
              >
                章节与句子
              </button>
              <button
                aria-controls="practice-panel"
                aria-expanded={panel === "settings"}
                onClick={() => onPanelChange("settings")}
              >
                播放设置
              </button>
            </div>
          </div>
          <div className="practice-work">
            <div className="practice-playbar">
              <div className="actions">
                <button
                  className="primary"
                  disabled={watching}
                  onClick={() => void play(state)}
                >
                  ▶ 听这一句
                </button>
                <button
                  aria-pressed={watching}
                  onClick={() => {
                    if (watchReturn.current)
                      stopWatching(true, "已返回开始听读时的位置和输入草稿。");
                    else {
                      watchReturn.current = practiceProgress(state);
                      setWatching(true);
                      void play(state);
                    }
                  }}
                >
                  {watching ? "结束并返回" : "连续听读"}
                </button>
                <button
                  disabled={!playing && !watching}
                  onClick={() => {
                    if (watchReturn.current)
                      stopWatching(false, "已停在当前句，可以开始输入。");
                    else {
                      stopPlayback();
                      setStatus("播放已停止。");
                    }
                  }}
                >
                  停止
                </button>
              </div>
              <span
                className="muted practice-audio-summary"
                role="status"
                title={
                  status ||
                  voices.find((voice) => voice.id === state.voiceId)?.name
                }
              >
                {status ||
                  `${state.speed === 0.65 ? "慢速" : state.speed === 0.85 ? "舒缓" : "正常"} · ${state.audioMode === "original" ? "课程原声" : "本地语音"}`}
              </span>
            </div>
            <section className="practice-original-zone" aria-label="当前原句">
              <div className="practice-section-heading">
                <span className="muted">原句</span>
                <div className="actions">
                  <button
                    disabled={explaining}
                    title={selected || "解释当前原句"}
                    onClick={() => {
                      onPanelChange("language");
                      void explain();
                    }}
                  >
                    {explaining
                      ? "解释中…"
                      : selected
                        ? "解释所选"
                        : "解释原句"}
                  </button>
                  <button onClick={collectSentence}>
                    {selected ? "收录所选" : "收录整句"}
                  </button>
                  <button onClick={() => onPanelChange("language")}>
                    查询
                  </button>
                  {selected && (
                    <button
                      className="text-button"
                      onClick={() => {
                        setSelected("");
                        setExplanation(null);
                      }}
                    >
                      取消选词
                    </button>
                  )}
                </div>
              </div>
              <p
                className="practice-original"
                ref={reading}
                onMouseUp={() => {
                  const selection = window.getSelection();
                  if (
                    selection?.anchorNode &&
                    reading.current?.contains(selection.anchorNode) &&
                    selection.focusNode &&
                    reading.current.contains(selection.focusNode)
                  ) {
                    setSelected(selection.toString().trim());
                    setExplanation(null);
                  }
                }}
              >
                {sentence.text}
              </p>
            </section>
            <section className="practice-input-zone" aria-label="逐句输入">
              <div className="practice-section-heading practice-copy-heading">
                <label htmlFor="practice-copy">轮到你输入</label>
                <span className="muted">大小写均可，标点需要一致</span>
              </div>
              <div className="practice-copy-editor">
                <div
                  ref={highlights}
                  className="practice-copy-highlights"
                  aria-hidden="true"
                >
                  {highlightCopy(sentence.text, state.copy).map(
                    (character, index) => (
                      <span key={index} className={character.state}>
                        {character.text}
                      </span>
                    ),
                  )}
                  {state.copy.endsWith("\n") ? "\u200b" : ""}
                </div>
                <textarea
                  id="practice-copy"
                  ref={editor}
                  rows={3}
                  disabled={watching}
                  value={state.copy}
                  spellCheck={false}
                  autoComplete="off"
                  placeholder="输入上方原句，正确后自动进入下一句…"
                  onChange={(event) =>
                    inputCopy(
                      event.target.value,
                      (event.nativeEvent as InputEvent).isComposing,
                    )
                  }
                  onCompositionEnd={(event) =>
                    inputCopy(event.currentTarget.value, false)
                  }
                  onScroll={(event) => {
                    if (highlights.current) {
                      highlights.current.scrollTop =
                        event.currentTarget.scrollTop;
                      highlights.current.scrollLeft =
                        event.currentTarget.scrollLeft;
                    }
                  }}
                />
              </div>
            </section>
            <div className="practice-feedback">
              <p
                className={
                  feedback.complete
                    ? "success"
                    : feedback.wrong >= 0
                      ? "error"
                      : "muted"
                }
                role="status"
              >
                {watching
                  ? "连续听读中，停止后可继续输入。"
                  : feedback.complete
                    ? "✓ 本篇已完成。"
                    : !state.copy
                      ? "正确输入后自动换句并朗读。"
                      : feedback.wrong >= 0
                        ? `请检查第 ${feedback.wrong + 1} 个字符。`
                        : "✓ 输入正确，请继续。"}
              </p>
              <details className="practice-comparison-panel">
                <summary>逐字比对</summary>
                <div className="practice-comparison" aria-label="输入比对">
                  {feedback.characters.map((character, index) => (
                    <span className={character.state} key={index}>
                      {character.text}
                    </span>
                  ))}
                </div>
              </details>
            </div>
            <footer className="practice-footer">
              <button
                disabled={!adjacentSentence(state, -1)}
                onClick={() => move(adjacentSentence(state, -1)!)}
              >
                上一句
              </button>
              <div className="practice-position">
                <span className="muted" title={chapter!.title}>
                  {chapter!.title} · {state.sentenceIndex + 1} /{" "}
                  {chapter!.sentences.length} 句
                </span>
                <progress
                  max={chapter!.sentences.length}
                  value={state.sentenceIndex + (feedback.complete ? 1 : 0)}
                  aria-label="章节进度"
                />
              </div>
              {feedback.complete && !adjacentSentence(state, 1) ? (
                <button className="primary" onClick={review}>
                  复习单词本
                </button>
              ) : (
                <button
                  disabled={!adjacentSentence(state, 1)}
                  onClick={() => move(adjacentSentence(state, 1)!)}
                >
                  下一句
                </button>
              )}
            </footer>
          </div>
        </>
      )}
      <dialog
        id="practice-panel"
        ref={panelDialog}
        className="practice-dialog"
        aria-labelledby="practice-panel-title"
        onCancel={(event) => {
          event.preventDefault();
          onPanelChange(null);
        }}
        onClose={() => {
          onPanelChange(null);
          editor.current?.focus();
        }}
      >
        <header className="practice-dialog-heading">
          <h2 id="practice-panel-title">
            {panel ? panelTitles[panel] : "听读练习"}
          </h2>
          <button
            type="button"
            className="practice-dialog-close"
            disabled={panel === "import" && importing}
            aria-label={`关闭${panel ? panelTitles[panel] : "面板"}`}
            onClick={() => onPanelChange(null)}
          >
            ×
          </button>
        </header>
        {panel === "import" && (
          <form
            className="practice-import"
            onSubmit={async (event) => {
              event.preventDefault();
              setImporting(true);
              setError("");
              try {
                await importDocument(pastedDocument(title, text));
              } catch (error) {
                setError(message(error));
              } finally {
                setImporting(false);
              }
            }}
          >
            <label>
              打开本地文件
              <input
                type="file"
                accept=".txt,.epub,.lesson.zip"
                disabled={importing}
                onChange={(event) => {
                  const file = event.target.files?.[0];
                  if (file) void importFile(file);
                  event.target.value = "";
                }}
              />
            </label>
            <p className="muted">
              支持 TXT、未加密 EPUB 和
              .lesson.zip。导入内容和原声保存在本机，新阅读替换当前练习。
            </p>
            <label>
              阅读标题
              <input
                value={title}
                onChange={(event) => setTitle(event.target.value)}
                placeholder="我的阅读"
              />
            </label>
            <label>
              或粘贴英语文本
              <textarea
                rows={4}
                value={text}
                onChange={(event) => setText(event.target.value)}
              />
            </label>
            {error && (
              <p className="error" role="alert">
                {error}
              </p>
            )}
            <button
              type="submit"
              className="primary"
              disabled={importing || !text.trim()}
            >
              {importing ? "正在导入…" : "使用这段文本"}
            </button>
          </form>
        )}
        {panel === "contents" && state && (
          <div className="practice-contents">
            <p className="muted">
              {state.document.title} · 共{" "}
              {state.document.chapters.reduce(
                (count, value) => count + value.sentences.length,
                0,
              )}{" "}
              句
            </p>
            <label>
              章节
              <select
                value={state.chapterIndex}
                onChange={(event) =>
                  move({
                    ...state,
                    chapterIndex: Number(event.target.value),
                    sentenceIndex: 0,
                    copy: "",
                  })
                }
              >
                {state.document.chapters.map((value, index) => (
                  <option value={index} key={index}>
                    {value.title}
                  </option>
                ))}
              </select>
            </label>
            <div className="practice-sentences" aria-label="练习句子">
              {chapter!.sentences
                .slice(
                  Math.max(0, state.sentenceIndex - 20),
                  state.sentenceIndex + 40,
                )
                .map((line, index) => {
                  const position =
                    Math.max(0, state.sentenceIndex - 20) + index;
                  return (
                    <button
                      key={position}
                      ref={
                        position === state.sentenceIndex
                          ? activeSentence
                          : undefined
                      }
                      aria-current={
                        position === state.sentenceIndex ? "true" : undefined
                      }
                      className={
                        position === state.sentenceIndex ? "active" : ""
                      }
                      onClick={() => {
                        move({ ...state, sentenceIndex: position, copy: "" });
                        onPanelChange(null);
                      }}
                    >
                      {position + 1}. {line.text}
                    </button>
                  );
                })}
            </div>
          </div>
        )}
        {panel === "settings" && state && (
          <div className="practice-settings">
            <label>
              速度
              <select
                value={state.speed}
                onChange={(event) => {
                  if (watchReturn.current) stopWatching(true);
                  else stopPlayback();
                  update({
                    ...current.current!,
                    speed: Number(event.target.value),
                  });
                }}
              >
                <option value={0.65}>慢速</option>
                <option value={0.85}>舒缓</option>
                <option value={1}>正常</option>
              </select>
            </label>
            {state.document.audioFile && (
              <label>
                音频来源
                <select
                  value={state.audioMode}
                  onChange={(event) => {
                    if (watchReturn.current) stopWatching(true);
                    else stopPlayback();
                    update({
                      ...current.current!,
                      audioMode: event.target.value as "original" | "voice",
                    });
                  }}
                >
                  <option value="original">课程原声</option>
                  <option value="voice">Windows 本地语音</option>
                </select>
              </label>
            )}
            {state.audioMode === "voice" && (
              <label>
                本地英语声音
                <select
                  value={state.voiceId}
                  onChange={(event) => {
                    if (watchReturn.current) stopWatching(true);
                    else stopPlayback();
                    update({
                      ...current.current!,
                      voiceId: event.target.value,
                    });
                  }}
                >
                  <option value="">可用的默认声音</option>
                  {voices.map((voice) => (
                    <option key={voice.id} value={voice.id}>
                      {voice.name}
                    </option>
                  ))}
                </select>
              </label>
            )}
            <p className="muted">
              朗读使用本机 Windows 英语声音；有课程原声时可以切换音频来源。
            </p>
          </div>
        )}
        {panel === "language" && sentence && (
          <div className="practice-language">
            <p className="practice-query-text">{selected || sentence.text}</p>
            <button disabled={explaining} onClick={explain}>
              {explaining ? "正在本地解释…" : "本地解释"}
            </button>
            {explanation && (
              <div className="suggestion">
                <strong>{explanation.meaning}</strong>
                <p>{explanation.translation}</p>
                <p className="muted">{explanation.notes}</p>
              </div>
            )}
            {error && (
              <p className="error" role="alert">
                {error}
              </p>
            )}
            <details className="practice-lookup">
              <summary>按需联网查询</summary>
              <OnlineLookup
                text={selected || sentence.text}
                onMeaning={(meaning) =>
                  setExplanation({
                    meaning,
                    translation: "",
                    notes: "联网查询建议，可在收录时修订。",
                  })
                }
              />
            </details>
          </div>
        )}
      </dialog>
      <audio
        ref={audio}
        onEnded={finishPlayback}
        onTimeUpdate={audioTick}
        onPlay={() =>
          window.dispatchEvent(
            new CustomEvent("svl_audio_started", { detail: audio.current }),
          )
        }
        onError={() => {
          if (watchReturn.current) stopWatching(true);
          else stopPlayback();
          setError("音频无法播放，请重新导入课程包或选择本地英语语音。");
        }}
      />
    </section>
  );
}
