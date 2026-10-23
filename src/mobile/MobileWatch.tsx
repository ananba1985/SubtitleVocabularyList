import { useEffect, useRef, useState } from "react";
import { call, message, terminal, uid, waitTask } from "../api";
import type { CommandResult } from "../commands";
import type { CollectionSeed, Settings } from "../types";
import { AudioPlayer } from "../components/AudioPlayer";
import { useVideoLearning } from "./useVideoLearning";
import { MobileWordQuery } from "./MobileWordQuery";
import { MobileQuery } from "./MobileQuery";
import { OnlineSubtitlePanel } from "../components/OnlineSubtitlePanel";
import { PreparedEpisodeSharing } from "../components/PreparedEpisodeSharing";

type VideoState = CommandResult<"video_state">;
const time = (ms: number) =>
  `${Math.floor(ms / 60000)}:${String(Math.floor(ms / 1000) % 60).padStart(2, "0")}`;

export function MobileWatch({
  collect,
  covered,
  settings,
  saved,
  openRequest,
}: {
  collect: (seed: CollectionSeed) => void;
  covered: boolean;
  settings: Settings;
  saved: (value: Settings) => void;
  openRequest?: { uri: string; nonce: string };
}) {
  const [folderFiles,setFolderFiles]=useState<CommandResult<"video_folder_pick">>([]);
  const [folderTask,setFolderTask]=useState("");
  async function pickFolder(){try{setFolderFiles(await call("video_folder_pick"));}catch(e){setError(message(e));}}
  async function indexFolder(){setBusy(true);setError("");try{const task=await call("video_folder_index",{files:folderFiles,operationId:uid()});setFolderTask(task.id);await waitTask(task);setFolderFiles([]);setLibrary(await call("video_library"));setNotice("目录字幕已索引，可在学习 → 按词预习中查看。");}catch(e){setError(message(e));}finally{setFolderTask("");setBusy(false);}}
  const [library, setLibrary] = useState<CommandResult<"video_library"> | null>(
    null,
  );
  const [state, setState] = useState<VideoState | null>(null);
  const [study, setStudy] = useState<CommandResult<"video_study">>(null);
  const [queryTarget, setQueryTarget] = useState("");
  const [clip, setClip] = useState<{ path: string } | null>(null);
  const [error, setError] = useState(""),
    [notice, setNotice] = useState("");
  const [busy, setBusy] = useState(false),
    [indexing, setIndexing] = useState(false);
  const {
    learning,
    card,
    error: learningError,
    skip,
  } = useVideoLearning(state, !!study, covered || busy || !!queryTarget);
  const context = learning?.context;
  const surface = useRef<HTMLDivElement>(null),
    session = useRef("");
  const generation = useRef(0);
  const advanced = useRef("");
  const [offset, setOffset] = useState(0);
  const [subtitleDelay, setSubtitleDelay] = useState(0),
    [translationDraft, setTranslationDraft] = useState("");
  useEffect(() => {
    setTranslationDraft("");
  }, [context?.revision]);
  useEffect(() => {
    setSubtitleDelay((state?.subtitleDelayMs ?? 0) / 1000);
  }, [state?.sessionId, state?.subtitleDelayMs]);
  useEffect(() => {
    setOffset(
      (study?.subtitles.assets.find(
        (a) => a.id === study.subtitles.selectedChineseId,
      )?.timeOffsetMs ?? 0) / 1000,
    );
  }, [study]);
  const openedRequest = useRef("");
  useEffect(() => {
    if (openRequest && openedRequest.current !== openRequest.nonce) {
      openedRequest.current = openRequest.nonce;
      void open(openRequest.uri);
    }
  }, [openRequest?.nonce]);
  const selectedTracks = state?.tracks
    .filter((track) => track.selected)
    .map((track) => track.id)
    .join("|");
  const previousIntent = useRef(false);
  useEffect(() => {
    if (!state) return;
    if (
      state.fullscreen &&
      state.paused &&
      state.userPaused &&
      previousIntent.current
    )
      void control("fullscreen");
    previousIntent.current = !state.paused;
  }, [state?.sessionId, state?.paused, state?.fullscreen]);
  useEffect(() => {
    call("video_library")
      .then(setLibrary)
      .catch((e) => setError(message(e)));
    let reading = false,
      disposed = false;
    const timer = window.setInterval(async () => {
      if (!session.current || reading) return;
      reading = true;
      try {
        const next = await call("video_state");
        if (!disposed && next.sessionId === session.current) setState(next);
      } catch (e) {
        if (!disposed) setError(message(e));
      } finally {
        reading = false;
      }
    }, 600);
    return () => {
      disposed = true;
      generation.current++;
      window.clearInterval(timer);
      if (session.current)
        call("video_control", {
          control: {
            sessionId: session.current,
            action: "hide",
            value: null,
            text: null,
            kind: null,
          },
        }).catch(() => {});
      session.current = "";
    };
  }, []);
  useEffect(() => {
    if (!state || !surface.current) return;
    const id = state.sessionId;
    if (covered) {
      call("video_control", {
        control: {
          sessionId: id,
          action: "hide",
          value: null,
          text: null,
          kind: null,
        },
      }).catch(() => {});
      return;
    }
    const viewport = () => {
      const rect = surface.current!.getBoundingClientRect();
      call("video_viewport", {
        viewport: {
          sessionId: id,
          x: rect.x,
          y: rect.y,
          width: rect.top >= 0 && rect.bottom <= innerHeight ? rect.width : 0,
          height: rect.height,
          ratio: devicePixelRatio,
        },
      }).catch(() => {});
    };
    const observer = new ResizeObserver(viewport);
    observer.observe(surface.current);
    viewport();
    window.addEventListener("scroll", viewport, true);
    window.addEventListener("resize", viewport);
    return () => {
      observer.disconnect();
      window.removeEventListener("scroll", viewport, true);
      window.removeEventListener("resize", viewport);
    };
  }, [state?.sessionId, covered]);
  useEffect(() => {
    setQueryTarget("");
  }, [
    context?.sourceId,
    context?.exampleId,
    context?.text,
    context?.translationRevision,
    selectedTracks,
  ]);
  useEffect(() => {
    if (
      covered ||
      busy ||
      queryTarget ||
      !state?.ended ||
      !library?.preferences.autoNext ||
      advanced.current === state.sessionId
    )
      return;
    advanced.current = state.sessionId;
    const index = library.playlist.findIndex((i) => i.path === state.uri);
    if (index >= 0 && index + 1 < library.playlist.length)
      open(library.playlist[index + 1].path);
  }, [state?.ended, state?.sessionId, library, covered, busy, queryTarget]);

  async function open(uri: string) {
    const version = ++generation.current;
    setBusy(true);
    setError("");
    setStudy(null);
    setQueryTarget("");
    setClip(null);
    try {
      const next = await call("video_open", { uri });
      if (version !== generation.current) return;
      session.current = next.sessionId;
      setState(next);
      setIndexing(true);
      setBusy(false);
      setNotice("正在准备本地指纹与字幕，视频可继续播放…");
      const task = await call("video_index_start", { uri, operationId: uid() });
      const result = await waitTask(task);
      if (version === generation.current) {
        setStudy(result);
        setNotice("");
        setLibrary(await call("video_library"));
      }
    } catch (e) {
      if (version === generation.current) {
        setError(message(e));
        setNotice("");
      }
    } finally {
      if (version === generation.current) {
        setBusy(false);
        setIndexing(false);
      }
    }
  }
  async function pick() {
    setError("");
    setBusy(true);
    try {
      const result = await call("video_pick");
      setLibrary(result);
      const next = result.playlist.find(
        (i) => !library?.playlist.some((old) => old.path === i.path),
      );
      if (next) await open(next.path);
    } catch (e) {
      setError(message(e));
    } finally {
      setBusy(false);
    }
  }
  async function control(
    action: string,
    value?: number,
    text?: string,
    kind?: string,
  ) {
    if (!session.current) return;
    try {
      const next = await call("video_control", {
        control: {
          sessionId: session.current,
          action,
          value: value ?? null,
          text: text ?? null,
          kind: kind ?? null,
        },
      });
      if (action === "stop") {
        generation.current++;
        session.current = "";
        setState(null);
        setStudy(null);
        setClip(null);
      } else setState(next);
      if (action === "track" || action === "learning_offset")
        setStudy(await call("video_study", { uri: next.uri }));
      if (["volume", "auto_next", "stop"].includes(action))
        setLibrary(await call("video_library"));
      if (action === "play")
        surface.current?.scrollIntoView({
          block: "center",
          behavior: "smooth",
        });
    } catch (e) {
      setError(message(e));
    }
  }
  async function subtitle(language: "en" | "zh") {
    if (!state) return;
    setBusy(true);
    setError("");
    try {
      setStudy(
        await waitTask(
          await call("video_subtitle_add", {
            uri: state.uri,
            language,
            operationId: uid(),
          }),
        ),
      );
    } catch (e) {
      setError(message(e));
    } finally {
      setBusy(false);
    }
  }
  async function preview() {
    if (!context) return;
    setBusy(true);
    setError("");
    try {
      const draft = await call("video_collection_draft", {
        revision: context.revision,
        text: context.text,
        kind: "sentence",
        meaning: context.translation,
      });
      setClip(
        await waitTask(
          await call("preview_start", {
            sourceId: draft.sourceId,
            exampleId: draft.example!.id,
            operationId: uid(),
          }),
        ),
      );
    } catch (e) {
      setError(message(e));
    } finally {
      setBusy(false);
    }
  }
  async function frame() {
    if (!state || !surface.current) return;
    setBusy(true);
    setError("");
    try {
      surface.current.scrollIntoView({ block: "center", behavior: "instant" });
      await new Promise<void>((resolve) =>
        requestAnimationFrame(() => requestAnimationFrame(() => resolve())),
      );
      const rect = surface.current.getBoundingClientRect();
      await call("video_viewport", {
        viewport: {
          sessionId: state.sessionId,
          x: rect.x,
          y: rect.y,
          width: rect.width,
          height: rect.height,
          ratio: devicePixelRatio,
        },
      });
      const image = await call("mobile_video_frame");
      collect({ text: "", context: "", kind: "sentence", image });
    } catch (e) {
      setError(message(e));
    } finally {
      setBusy(false);
    }
  }
  async function configure(value: Settings) {
    setBusy(true);
    setError("");
    try {
      saved(await call("settings_update", { settings: value }));
    } catch (e) {
      setError(message(e));
      saved(await call("settings_get"));
    } finally {
      setBusy(false);
    }
  }
  async function draft(text: string, kind: string, meaning: string) {
    if (!context) return;
    setBusy(true);
    try {
      const seed = await call("video_collection_draft", {
        revision: context.revision,
        text,
        kind,
        meaning,
      });
      collect({ ...seed, example: seed.example ?? undefined });
    } catch (e) {
      setError(message(e));
    } finally {
      setBusy(false);
    }
  }
  const title = library?.playlist.find((i) => i.path === state?.uri)?.title;
  return (
    <section className="mobile-watch">
      <details className="mobile-learning-options"><summary>同剧目录批量索引</summary><p>仅读取授权目录直接包含的视频，最多 100 个；不递归子目录，不运行整季翻译。</p><button disabled={busy} onClick={pickFolder}>选择视频目录并预览</button>
      {folderFiles.length>0 && <><p>已选择 {folderFiles.length} 个视频，确认后才索引。</p><ul>{folderFiles.map(f=><li key={f.uri}>{f.title}</li>)}</ul><button disabled={busy} onClick={indexFolder}>确认索引这些视频</button><button disabled={busy} onClick={()=>setFolderFiles([])}>取消目录选择</button></>}
      {folderTask && <button onClick={()=>call("task_cancel",{taskId:folderTask})}>取消目录索引</button>}</details>

      <div className="toolbar">
        <button disabled={busy} onClick={pick}>
          打开本地视频
        </button>
        {!state && library?.currentPath && (
          <button disabled={busy} onClick={() => open(library.currentPath!)}>
            继续上次视频
          </button>
        )}
      </div>
      {error && (
        <p className="error" role="alert">
          {error}
        </p>
      )}
      {learningError && (
        <p className="error" role="alert">
          {learningError}
        </p>
      )}
      {!state && (
        <p className="muted">
          选择手机上的视频。字幕、词库和学习记录均可离线使用。
        </p>
      )}
      {!!library?.playlist.length && (
        <label>
          播放列表
          <select
            aria-label="播放列表"
            value={state?.uri ?? ""}
            onChange={(e) => open(e.target.value)}
            disabled={busy}
          >
            <option value="" disabled>
              选择视频
            </option>
            {library.playlist.map((i) => (
              <option key={i.path} value={i.path}>
                {i.title}
              </option>
            ))}
          </select>
        </label>
      )}
      {state && (
        <>
          <h2>{title}</h2>
          <div
            ref={surface}
            className="mobile-video-surface"
            aria-label="视频画面"
          />
          {state.error && (
            <p className="error" role="alert">
              {state.error}
            </p>
          )}
          <div className="toolbar">
            <button onClick={() => control(state.paused ? "play" : "pause")}>
              {state.paused ? "播放视频" : "暂停视频"}
            </button>
            <button onClick={() => control("stop")}>停止播放</button>
            <button
              onClick={() =>
                control("seek", Math.max(0, state.positionMs - 10000))
              }
            >
              后退 10 秒
            </button>
            <button
              onClick={() =>
                control(
                  "seek",
                  Math.min(state.durationMs, state.positionMs + 10000),
                )
              }
            >
              前进 10 秒
            </button>
            <button onClick={() => control("seek", context?.startMs ?? 0)}>
              {context ? "重播当前句" : "从头播放"}
            </button>
            <button onClick={() => control("fullscreen")}>全屏</button>
            <button
              disabled={busy || indexing || !study || !state.paused}
              onClick={frame}
            >
              暂停帧 OCR
            </button>
          </div>
          <label className="video-seek">
            {time(state.positionMs)} / {time(state.durationMs)}
            <input
              aria-label="视频进度"
              type="range"
              min={0}
              max={Math.max(1, state.durationMs)}
              value={Math.min(state.positionMs, state.durationMs)}
              onChange={(e) => control("seek", Number(e.target.value))}
            />
          </label>
          <div className="field-row">
            <label>
              速度
              <select
                aria-label="播放速度"
                value={state.speed}
                onChange={(e) => control("speed", Number(e.target.value))}
              >
                {[0.5, 0.75, 1, 1.25, 1.5, 2].map((speed) => (
                  <option value={speed} key={speed}>
                    {speed}×
                  </option>
                ))}
              </select>
            </label>
            {(["audio", "subtitle"] as const).map((kind) => (
              <label key={kind}>
                {kind === "audio" ? "音轨" : "画面字幕"}
                <select
                  aria-label={kind === "audio" ? "音轨" : "画面字幕"}
                  value={
                    state.tracks.find((t) => t.kind === kind && t.selected)
                      ?.id ?? ""
                  }
                  onChange={(e) =>
                    control("track", undefined, e.target.value, kind)
                  }
                >
                  <option value="">关闭</option>
                  {state.tracks
                    .filter((t) => t.kind === kind)
                    .map((t) => (
                      <option key={t.id} value={t.id} disabled={!t.supported}>
                        {t.title}
                      </option>
                    ))}
                </select>
              </label>
            ))}
          </div>
          {notice && <p role="status">{notice}</p>}
          {!study && !indexing && (
            <button disabled={busy} onClick={() => open(state.uri)}>
              重试字幕准备
            </button>
          )}
          {study && (
            <PreparedEpisodeSharing
              key={`shared:${study.sourceId}`}
              sourceId={study.sourceId}
              onApplied={async () => {
                if (!state) return;
                setStudy(await call("video_study", { uri: state.uri }));
                setState(await call("video_state"));
              }}
            />
          )}
          {study && <details className="mobile-learning-options"><summary>查找在线字幕</summary><OnlineSubtitlePanel key={study.sourceId} sourceId={study.sourceId} onImported={async()=>{if(state){setStudy(await call("video_study",{uri:state.uri}));setState(await call("video_state"));}}}/></details>}
          <details className="mobile-learning-options">
            <summary>播放设置</summary>
            <label className="check-label"><input type="checkbox" checked={state.dualSubtitles} onChange={e=>control("dual_subtitles",e.target.checked?1:0)}/>画面同时显示已选中文字幕</label>
            <label>字幕字号<input type="number" aria-label="字幕字号" min={14} max={48} value={state.subtitleFontSize} onChange={e=>control("subtitle_font",Number(e.target.value))}/></label>
            <label className="check-label"><input type="checkbox" checked={state.subtitleBackground} onChange={e=>control("subtitle_background",e.target.checked?1:0)}/>字幕黑色背景</label>
            <div className="actions"><button onClick={()=>control("loop_a")}>设为循环 A 点</button><button disabled={state.loopStartMs===null || state.positionMs<=state.loopStartMs} onClick={()=>control("loop_b")}>设为循环 B 点</button><button onClick={()=>control("loop_clear")}>清除循环</button></div>
            <p>循环：{state.loopStartMs===null?"未设 A 点":time(state.loopStartMs)} → {state.loopEndMs===null?"未设 B 点":time(state.loopEndMs)}</p>
            <label>章节<select aria-label="视频章节" value="" onChange={e=>control("seek",Number(e.target.value))}><option value="">{state.chapters.length?"选择章节跳转":"当前文件没有可读取的章节"}</option>{state.chapters.map((c,i)=><option key={i} value={c.positionMs}>{c.title} · {time(c.positionMs)}</option>)}</select></label>

            <label>
              画面字幕整体偏移（秒）
              <input
                aria-label="画面字幕整体偏移"
                type="number"
                step="0.1"
                value={subtitleDelay}
                onChange={(e) => setSubtitleDelay(Number(e.target.value))}
              />
            </label>
            <button
              disabled={busy || indexing || !study}
              onClick={() =>
                control("subtitle_delay", Math.round(subtitleDelay * 1000))
              }
            >
              应用画面字幕偏移
            </button>
            <p className="muted">
              正数延后字幕；学习对白和收录原声区间随之校正。使用可靠文本轨，原字幕文件保持不变。
            </p>
            <label>
              播放器音量
              <input
                aria-label="播放器音量"
                type="range"
                min={0}
                max={100}
                value={library?.preferences.volume ?? 100}
                onChange={(e) => control("volume", Number(e.target.value))}
              />
            </label>
            <label className="check-label">
              <input
                type="checkbox"
                checked={library?.preferences.autoNext ?? false}
                onChange={(e) => control("auto_next", e.target.checked ? 1 : 0)}
              />
              本集结束后连续播放下一集
            </label>
            {study?.subtitles.selectedChineseId && (
              <>
                <label>
                  学习中文字幕偏移（秒）
                  <input
                    aria-label="学习中文字幕偏移"
                    type="number"
                    step="0.1"
                    value={offset}
                    onChange={(e) => setOffset(Number(e.target.value))}
                  />
                </label>
                <button
                  disabled={busy}
                  onClick={() =>
                    control("learning_offset", Math.round(offset * 1000))
                  }
                >
                  应用中文偏移
                </button>
                <p className="muted">
                  正数推迟中英文对齐中的中文；画面字幕使用上方的整体偏移。
                </p>
              </>
            )}
          </details>
          <div className="toolbar">
            <button
              disabled={busy || indexing || !study}
              onClick={() => subtitle("en")}
            >
              添加英文字幕
            </button>
            <button
              disabled={busy || indexing || !study}
              onClick={() => subtitle("zh")}
            >
              添加中文字幕
            </button>
          </div>
          <details className="mobile-learning-options">
            <summary>暂停学习设置</summary>
            <label className="check-label">
              <input
                type="checkbox"
                checked={settings.watchAutoCollect}
                disabled={busy}
                onChange={(e) =>
                  configure({ ...settings, watchAutoCollect: e.target.checked })
                }
              />
              译文显示后自动收录整句
            </label>
            <label>
              等待秒数
              <input
                aria-label="自动收录等待秒数"
                type="number"
                min={1}
                value={settings.watchCollectDelaySeconds}
                onChange={(e) =>
                  saved({
                    ...settings,
                    watchCollectDelaySeconds: Number(e.target.value),
                  })
                }
                onBlur={() => configure(settings)}
              />
            </label>
          </details>
          {study?.subtitles.warnings.map((warning) => (
            <p className="notice" key={warning}>
              {warning}
            </p>
          ))}
          {state.paused && context && (
            <article ref={card} className="mobile-card mobile-learning">
              <span className="badge">
                {context.previous ? "上一句" : "当前对白"}
              </span>
              <p className="quote">
                {context.text
                  .split(/([A-Za-z]+(?:['’\-][A-Za-z]+)*)/)
                  .map((part, index) =>
                    /^[A-Za-z]/.test(part) ? (
                      <button
                        key={index}
                        className="watch-word"
                        onClick={() => setQueryTarget(part)}
                      >
                        {part}
                      </button>
                    ) : (
                      part
                    ),
                  )}
              </p>
              <p data-translation>
                {context.translation ||
                  "尚无译文，可添加中文字幕或手动填写释义。"}
              </p>
              {!context.translation && (
                <details className="mobile-learning-options">
                  <summary>补充当前对白译文</summary>
                  <MobileQuery
                    text={context.text}
                    context=""
                    apply={(value) => setTranslationDraft(value.meaning)}
                  />
                  <label>
                    人工中文译文
                    <textarea
                      aria-label="当前对白人工译文"
                      value={translationDraft}
                      onChange={(e) => setTranslationDraft(e.target.value)}
                    />
                  </label>
                  <button
                    disabled={busy || !translationDraft.trim()}
                    onClick={async () => {
                      try {
                        await call("video_translation_set", {
                          revision: context.revision,
                          translation: translationDraft,
                        });
                        setTranslationDraft("");
                        setState(await call("video_state"));
                      } catch (e) {
                        setError(message(e));
                      }
                    }}
                  >
                    保存当前对白译文
                  </button>
                </details>
              )}
              <p className="notice" role="status">
                {learning?.collected
                  ? "该对白已自动收录，重复暂停不会重复追加。"
                  : learning?.task?.state === "succeeded"
                    ? "对白与原声已保存。"
                    : learning?.task && !terminal(learning.task)
                      ? "正在保存对白与原声…"
                      : learning?.skipped
                        ? "本次不自动收录，可继续播放或手动收录。"
                        : learning?.remainingSeconds != null
                          ? `${Math.ceil(learning.remainingSeconds)} 秒后自动收录整句`
                          : !context.translation
                            ? "补充完整译文后才会自动收录。"
                            : !settings.watchAutoCollect
                              ? "自动收录已关闭。"
                              : learning?.eligible
                                ? "完整译文可见后开始计时；查词或打开草稿会暂停计时。"
                                : "主动播放后再次暂停，可开始新的自动收录。"}
              </p>
              {learning?.task?.error && (
                <p className="error">{learning.task.error.message}</p>
              )}
              <div className="actions">
                <button
                  disabled={busy || context.audioStream === null}
                  onClick={preview}
                >
                  试听独立原声
                </button>
                <button
                  disabled={busy}
                  onClick={() =>
                    draft(context.text, "sentence", context.translation)
                  }
                >
                  手动收录整句
                </button>
                <button
                  disabled={!!learning?.collected || !!learning?.skipped}
                  onClick={skip}
                >
                  本次不收录
                </button>
                <button onClick={() => control("play")}>继续播放</button>
              </div>
              {queryTarget && (
                <MobileWordQuery
                  key={`${context.sourceId}:${context.exampleId}:${queryTarget}`}
                  target={queryTarget}
                  context={context.text}
                  close={() => setQueryTarget("")}
                  collect={draft}
                />
              )}
              {clip && <AudioPlayer path={clip.path} label="原声" />}
            </article>
          )}
        </>
      )}
    </section>
  );
}
