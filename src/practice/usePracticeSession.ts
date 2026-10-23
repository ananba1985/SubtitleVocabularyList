import { useRuntimeFeatures } from "../runtimeFeatures";
import type { ClipAudioState } from "../generated/ipc";
import { useEffect, useRef, useState } from "react";
import { audioUrl, call, message, uid, waitTask } from "../api";
import type {
  PracticeDocument,
  PracticeProgress,
  PracticeState,
} from "../practice";
import {
  adjacentSentence,
  compareCopy,
  pastedDocument,
  practiceProgress,
  readPracticeFile,
} from "../practice";
import type { CollectionSeed, Explanation, SystemVoice } from "../types";

export type PracticePanel =
  "import" | "contents" | "settings" | "language" | null;

export function usePracticeSession({
  dataDirectory,
  panel,
  onPanelChange,
  collect,
}: {
  dataDirectory: string;
  collect: (seed: CollectionSeed) => void;
  panel: PracticePanel;
  onPanelChange: (panel: PracticePanel) => void;
}) {
  const { mobile } = useRuntimeFeatures();
  const nativeAudio = useRef("");
  const nativeTimer = useRef<ReturnType<typeof setInterval> | null>(null);
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
        call("practice_progress_save", {
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
    if(nativeTimer.current){clearInterval(nativeTimer.current);nativeTimer.current=null;}
    if(nativeAudio.current){void call("audio_control",{sessionId:nativeAudio.current,action:"close"}).catch(()=>{});nativeAudio.current="";}
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
        setStatus("正在准备 本机离线英语语音…");
        const task = await call("speech_start", {
          text: line.text,
          operationId: uid(),
          voiceId: value.voiceId,
        });
        if (request !== generation.current) {
          call("task_cancel", { taskId: task.id }).catch(() => {});
          return;
        }
        speechTask.current = task.id;
        const result = await waitTask(task);
        path = result.path;
      }
      if (!mounted.current || request !== generation.current || !audio.current)
        return;
      speechTask.current = null;
      if (mobile) {
        const original=value.audioMode === "original" && !!value.document.audioFile;
        const native=await call("audio_open",{path,startMs:original?line.startMs ?? 0:0,endMs:original?line.endMs:null,speed:value.speed});
        if(request!==generation.current){void call("audio_control",{sessionId:native.sessionId,action:"close"});return;}
        nativeAudio.current=native.sessionId;
        let polling=false;
        nativeTimer.current=setInterval(async()=>{
          if(polling || request!==generation.current)return;polling=true;
          try {
            const next:ClipAudioState=await call("audio_state",{sessionId:native.sessionId});
            if(request!==generation.current)return;
            if(next.error)throw Error(next.error);
            if(next.active && next.ended && next.played)finishPlayback();
            else if(!next.active || (!next.playing && next.played)){stopWatching(false,"播放已暂停，可继续当前句。");}
          }catch(e){if(request===generation.current){stopPlayback();setError(message(e));}}
          finally{polling=false;}
        },100);
        setStatus(original?"正在播放课程原声。":"正在播放本机离线英语语音。");
        return;
      }
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
            : "正在播放 本机离线英语语音。",
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
    call("practice_get")
      .then(
        async (saved) =>
          saved ??
          call("practice_import", {
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
    call("speech_voices")
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
    if (bytes) document.audioFile = await call("practice_audio_save", {bytes:Array.from(bytes)});
    const value = await call("practice_import", { document });
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
      const task = await call("explain_start", {
        text: target,
        context: sentence.text,
        operationId: uid(),
      });
      const result = await waitTask(task);
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
          ? await call("practice_example", {
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

  return {
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
  };
}
