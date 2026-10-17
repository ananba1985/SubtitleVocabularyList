import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { audioUrl, call, message, uid, waitTask } from "../api";
import {
  adjacentSentence,
  compareCopy,
  highlightCopy,
  pastedDocument,
  practiceProgress,
  readPracticeFile,
} from "../practice";
import type {
  PracticeDocument,
  PracticeProgress,
  PracticeState,
} from "../practice";
import type {
  CandidateExample,
  CollectionSeed,
  Explanation,
  SystemVoice,
  TaskSnapshot,
} from "../types";
import { OnlineLookup } from "./OnlineLookup";

export type PracticePanel =
  "import" | "contents" | "settings" | "language" | null;
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
  const [state, setState] = useState<PracticeState | null>(null);
  const current = useRef<PracticeState | null>(null);
  const [error, setError] = useState("");
  const [status, setStatus] = useState("");
  const [importing, setImporting] = useState(false);
  const [title, setTitle] = useState("");
  const [text, setText] = useState("");
  const [voices, setVoices] = useState<SystemVoice[]>([]);
  const [playing, setPlaying] = useState(false);
  const [watching, setWatching] = useState(false);
  const watchReturn = useRef<PracticeProgress | null>(null);
  const [selected, setSelected] = useState("");
  const [explanation, setExplanation] = useState<Explanation | null>(null);
  const [explaining, setExplaining] = useState(false);
  const audio = useRef<HTMLAudioElement>(null);
  const reading = useRef<HTMLParagraphElement>(null);
  const editor = useRef<HTMLTextAreaElement>(null);
  const highlights = useRef<HTMLDivElement>(null);
  const generation = useRef(0);
  const speechTask = useRef<string | null>(null);
  const saveTimer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const saveQueue = useRef(Promise.resolve());
  const mounted = useRef(true);
  const activeSentence = useRef<HTMLButtonElement>(null);
  const panelDialog = useRef<HTMLDialogElement>(null);
  const explanationGeneration = useRef(0);
  const sentence =
    state?.document.chapters[state.chapterIndex].sentences[state.sentenceIndex];
  const chapter = state?.document.chapters[state.chapterIndex];
  const feedback = compareCopy(sentence?.text ?? "", state?.copy ?? "");

  function save(value: PracticeState) {
    const documentId = value.document.id;
    const progress = practiceProgress(value);
    saveQueue.current = saveQueue.current
      .then(() =>
        call<void>("practice_progress_save", {
          documentId,
          progress,
        }),
      )
      .catch((error) => {
        if (mounted.current) setError(message(error));
      });
  }
  function update(value: PracticeState, persist = true) {
    current.current = value;
    setState(value);
    if (persist && !watchReturn.current) {
      if (saveTimer.current) clearTimeout(saveTimer.current);
      save(value);
    }
  }
  function stopPlayback() {
    generation.current++;
    audio.current?.pause();
    if (speechTask.current) {
      call("task_cancel", { taskId: speechTask.current }).catch(() => {});
      speechTask.current = null;
    }
    setPlaying(false);
  }
  function stopWatching(restore: boolean, notice = "已停止连续听读。") {
    const origin = watchReturn.current;
    stopPlayback();
    watchReturn.current = null;
    setWatching(false);
    if (current.current && origin) {
      const same =
        current.current.chapterIndex === origin.chapterIndex &&
        current.current.sentenceIndex === origin.sentenceIndex;
      update(
        restore
          ? { ...current.current, ...origin }
          : { ...current.current, copy: same ? origin.copy : "" },
      );
    }
    setStatus(notice);
  }
  async function play(value: PracticeState) {
    stopPlayback();
    const request = generation.current;
    setError("");
    setPlaying(true);
    const line =
      value.document.chapters[value.chapterIndex].sentences[
        value.sentenceIndex
      ];
    try {
      let path: string;
      if (
        value.audioMode === "original" &&
        value.document.audioFile &&
        line.startMs !== null
      ) {
        path = dataDirectory + "/" + value.document.audioFile;
      } else {
        setStatus("正在准备 Windows 本地英语语音…");
        const task = await call<TaskSnapshot>("speech_start", {
          text: line.text,
          operationId: uid(),
          voiceId: value.voiceId,
        });
        if (request !== generation.current) {
          call("task_cancel", { taskId: task.id }).catch(() => {});
          return;
        }
        speechTask.current = task.id;
        const result = await waitTask<{ path: string }>(task);
        path = result.path;
      }
      if (!mounted.current || request !== generation.current || !audio.current)
        return;
      speechTask.current = null;
      audio.current.src = audioUrl(path);
      audio.current.playbackRate = value.speed;
      audio.current.currentTime =
        value.audioMode === "original" && value.document.audioFile
          ? (line.startMs ?? 0) / 1000
          : 0;
      await audio.current.play();
      if (request !== generation.current) return;
      setStatus(
        watchReturn.current
          ? "连续听读中，当前句会随播放前进。"
          : value.audioMode === "original"
            ? "正在播放课程原声。"
            : "正在播放 Windows 本地英语语音。",
      );
    } catch (error) {
      if (mounted.current && request === generation.current) {
        if (watchReturn.current) stopWatching(true);
        else stopPlayback();
        setError(message(error));
      }
    }
  }
  function finishPlayback() {
    const value = current.current;
    if (!value) return;
    if (watchReturn.current) {
      const next = adjacentSentence(value, 1);
      if (next) {
        const target = { ...value, ...next };
        update(target, false);
        void play(target);
      } else
        stopWatching(true, "已听到结尾，已返回开始听读时的位置和输入草稿。");
    } else {
      stopPlayback();
      setStatus("播放完成，可以重听或继续输入。");
    }
  }
  function audioTick() {
    const value = current.current;
    const player = audio.current;
    if (!value || !player || player.paused || value.audioMode !== "original")
      return;
    let line =
      value.document.chapters[value.chapterIndex].sentences[
        value.sentenceIndex
      ];
    if (line.endMs === null) return;
    if (!watchReturn.current) {
      if (player.currentTime * 1000 >= line.endMs) finishPlayback();
      return;
    }
    let cursor = value;
    while (true) {
      const next = adjacentSentence(cursor, 1);
      const nextLine =
        next &&
        cursor.document.chapters[next.chapterIndex].sentences[
          next.sentenceIndex
        ];
      const boundary =
        nextLine && nextLine.startMs !== null
          ? Math.max(
              (line.startMs ?? 0) + 10,
              Math.min(line.endMs!, nextLine.startMs),
            )
          : line.endMs!;
      if (player.currentTime * 1000 < boundary) break;
      if (!next || !nextLine) {
        stopWatching(true, "已听到结尾，已返回开始听读时的位置和输入草稿。");
        return;
      }
      cursor = { ...cursor, ...next };
      update(cursor, false);
      line = nextLine;
    }
  }

  useEffect(() => {
    mounted.current = true;
    let disposed = false;
    call<PracticeState | null>("practice_get")
      .then(
        async (saved) =>
          saved ??
          call<PracticeState>("practice_import", {
            document: pastedDocument(
              "从一句话开始",
              "I can practice one sentence at a time. Every small step helps me improve.",
            ),
          }),
      )
      .then((value) => {
        if (!disposed) {
          current.current = value;
          setState(value);
        }
      })
      .catch((error) => {
        if (!disposed) setError(message(error));
      });
    call<SystemVoice[]>("speech_voices")
      .then((value) => {
        if (!disposed) setVoices(value);
      })
      .catch((error) => {
        if (!disposed) setStatus(message(error));
      });
    return () => {
      disposed = true;
      mounted.current = false;
      stopPlayback();
      if (saveTimer.current) clearTimeout(saveTimer.current);
      if (current.current)
        save(
          watchReturn.current
            ? { ...current.current, ...watchReturn.current }
            : current.current,
        );
    };
  }, []);
  useEffect(() => {
    const dialog = panelDialog.current;
    if (!dialog) return;
    if (panel && !dialog.open) dialog.showModal();
    else if (!panel && dialog.open) dialog.close();
    if (panel === "import" && watchReturn.current) stopWatching(true);
  }, [panel]);
  useEffect(() => {
    setSelected("");
    setExplanation(null);
  }, [state?.document.id, state?.chapterIndex, state?.sentenceIndex]);
  useEffect(() => {
    activeSentence.current?.scrollIntoView({ block: "nearest" });
  }, [panel, state?.document.id, state?.chapterIndex, state?.sentenceIndex]);
  useEffect(() => {
    explanationGeneration.current++;
  }, [selected, state?.document.id, state?.chapterIndex, state?.sentenceIndex]);
  useEffect(() => {
    if (!playing) return;
    const timer = setInterval(audioTick, 25);
    return () => clearInterval(timer);
  }, [playing]);
  useEffect(() => {
    const stopOthers = (event: Event) => {
      if ((event as CustomEvent<HTMLAudioElement>).detail !== audio.current) {
        if (watchReturn.current) stopWatching(true);
        else stopPlayback();
      }
    };
    window.addEventListener("svl_audio_started", stopOthers);
    return () => window.removeEventListener("svl_audio_started", stopOthers);
  }, []);

  async function importDocument(
    document: PracticeDocument,
    bytes?: Uint8Array,
  ) {
    if (watchReturn.current) stopWatching(true);
    else stopPlayback();
    if (saveTimer.current) clearTimeout(saveTimer.current);
    if (current.current) save(current.current);
    await saveQueue.current;
    if (bytes)
      document.audioFile = await invoke<string>("practice_audio_save", bytes);
    const value = await call<PracticeState>("practice_import", { document });
    update(value, false);
    onPanelChange(null);
    setStatus("已保存到本机，可离线练习并在下次打开时继续。");
    editor.current?.focus();
  }
  async function importFile(file: File) {
    setImporting(true);
    setError("");
    try {
      const result = await readPracticeFile(file);
      await importDocument(result.document, result.audio);
    } catch (error) {
      setError(message(error));
    } finally {
      setImporting(false);
    }
  }
  function move(target: PracticeProgress) {
    if (!current.current) return;
    if (watchReturn.current) stopWatching(false);
    else stopPlayback();
    update({ ...current.current, ...target });
    setStatus("");
    editor.current?.focus();
  }
  function inputCopy(copy: string, composing: boolean) {
    const value = current.current;
    if (!value) return;
    const changed = { ...value, copy };
    current.current = changed;
    setState(changed);
    if (saveTimer.current) clearTimeout(saveTimer.current);
    saveTimer.current = setTimeout(() => save(changed), 250);
    if (!composing && compareCopy(sentence!.text, copy).complete) {
      const next = adjacentSentence(changed, 1);
      if (next) {
        move(next);
        void play(current.current!);
      } else setStatus("已完成本篇练习，可以前往单词本复习。");
    }
  }
  async function explain() {
    if (!sentence) return;
    const request = explanationGeneration.current;
    const target = selected || sentence.text;
    setExplaining(true);
    setError("");
    try {
      const task = await call<TaskSnapshot>("explain_start", {
        text: target,
        context: sentence.text,
        operationId: uid(),
      });
      const result = await waitTask<Explanation>(task);
      if (mounted.current && request === explanationGeneration.current)
        setExplanation(result);
    } catch (error) {
      if (mounted.current) setError(message(error));
    } finally {
      if (mounted.current) setExplaining(false);
    }
  }
  async function collectSentence() {
    if (!state || !sentence) return;
    if (watchReturn.current) stopWatching(false);
    else stopPlayback();
    try {
      const example =
        state.document.audioFile && sentence.startMs !== null
          ? await call<CandidateExample>("practice_example", {
              documentId: state.document.id,
              chapter: state.chapterIndex,
              sentence: state.sentenceIndex,
            })
          : undefined;
      collect({
        text: selected || sentence.text,
        kind: selected
          ? /\s/.test(selected.trim())
            ? "phrase"
            : "word"
          : "sentence",
        context: sentence.text,
        meaning: explanation?.meaning,
        sourceId: state.document.id,
        sourceTitle: state.document.title,
        locationKey: `practice:${state.chapterIndex}:${state.sentenceIndex}`,
        example,
      });
    } catch (error) {
      setError(message(error));
    }
  }

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
