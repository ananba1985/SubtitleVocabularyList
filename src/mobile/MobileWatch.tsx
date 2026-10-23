import { useEffect, useRef, useState } from "react";
import { call, message, uid, waitTask } from "../api";
import type { CommandResult } from "../commands";
import type { CollectionSeed } from "../types";
import { AudioPlayer } from "../components/AudioPlayer";

type VideoState = CommandResult<"video_state">;
const time = (ms: number) => `${Math.floor(ms / 60000)}:${String(Math.floor(ms / 1000) % 60).padStart(2, "0")}`;

export function MobileWatch({ collect, covered }: { collect: (seed: CollectionSeed) => void; covered: boolean }) {
  const [library, setLibrary] = useState<CommandResult<"video_library"> | null>(null);
  const [state, setState] = useState<VideoState | null>(null);
  const [study, setStudy] = useState<CommandResult<"video_study">>(null);
  const [context, setContext] = useState<CommandResult<"video_context">>(null);
  const [clip, setClip] = useState<{ path: string } | null>(null);
  const [error, setError] = useState(""), [notice, setNotice] = useState("");
  const [busy, setBusy] = useState(false), [indexing, setIndexing] = useState(false);
  const surface = useRef<HTMLDivElement>(null), session = useRef("");
  const generation = useRef(0);
  const advanced = useRef("");
  const selectedTracks = state?.tracks.filter((track) => track.selected).map((track) => track.id).join("|");
  useEffect(() => {
    call("video_library").then(setLibrary).catch((e) => setError(message(e)));
    let reading = false, disposed = false;
    const timer = window.setInterval(async () => {
      if (!session.current || reading) return;
      reading = true;
      try {
        const next = await call("video_state");
        if (!disposed && next.sessionId === session.current) setState(next);
      } catch (e) { if (!disposed) setError(message(e)); }
      finally { reading = false; }
    }, 600);
    return () => {
      disposed = true; generation.current++; window.clearInterval(timer);
      if (session.current) call("video_control", { control: { sessionId: session.current, action: "hide",value:null,text:null,kind:null } }).catch(() => {});
      session.current = "";
    };
  }, []);
  useEffect(() => {
    if (!state || !surface.current) return;
    const id = state.sessionId;
    if (covered) {
      call("video_control", { control: { sessionId:id,action:"hide",value:null,text:null,kind:null } }).catch(() => {});
      return;
    }
    const viewport = () => {
      const rect = surface.current!.getBoundingClientRect();
      call("video_viewport", { viewport: { sessionId: id, x: rect.x, y: rect.y,
        width: rect.top >= 0 && rect.bottom <= innerHeight ? rect.width : 0, height: rect.height, ratio: devicePixelRatio } }).catch(() => {});
    };
    const observer = new ResizeObserver(viewport); observer.observe(surface.current); viewport();
    window.addEventListener("scroll", viewport, true); window.addEventListener("resize", viewport);
    return () => { observer.disconnect(); window.removeEventListener("scroll", viewport, true); window.removeEventListener("resize", viewport); };
  }, [state?.sessionId, covered]);
  useEffect(() => {
    setContext(null);
    if (!study || state?.playing || !state?.sessionId) return;
    let disposed = false;
    call("video_context").then((next) => { if (!disposed) setContext(next); }).catch((e) => { if (!disposed) setError(message(e)); });
    return () => { disposed = true; };
  }, [study, state?.playing, state?.positionMs, state?.sessionId, selectedTracks]);
  useEffect(() => {
    if (!state?.ended || !library?.preferences.autoNext || advanced.current === state.sessionId) return;
    advanced.current = state.sessionId;
    const index = library.playlist.findIndex((i) => i.path === state.uri);
    if (index >= 0 && index + 1 < library.playlist.length) open(library.playlist[index + 1].path);
  }, [state?.ended, state?.sessionId, library]);

  async function open(uri: string) {
    const version = ++generation.current;
    setBusy(true); setError(""); setStudy(null); setContext(null); setClip(null);
    try {
      const next = await call("video_open", { uri });
      if (version !== generation.current) return;
      session.current = next.sessionId; setState(next); setIndexing(true); setBusy(false);
      setNotice("正在准备本地指纹与字幕，视频可继续播放…");
      const task = await call("video_index_start", { uri, operationId: uid() });
      const result = await waitTask(task);
      if (version === generation.current) { setStudy(result); setNotice(""); setLibrary(await call("video_library")); }
    } catch (e) { if (version === generation.current) { setError(message(e)); setNotice(""); } }
    finally { if (version === generation.current) { setBusy(false); setIndexing(false); } }
  }
  async function pick() {
    setError(""); setBusy(true);
    try {
      const result = await call("video_pick"); setLibrary(result);
      const next = result.playlist.find((i) => !library?.playlist.some((old) => old.path === i.path));
      if (next) await open(next.path);
    } catch (e) { setError(message(e)); } finally { setBusy(false); }
  }
  async function control(action: string, value?: number, text?: string, kind?: string) {
    if (!session.current) return;
    try {
      const next = await call("video_control", { control: { sessionId: session.current, action, value:value ?? null, text:text ?? null, kind:kind ?? null } }); setState(next);
      if (action === "track") setStudy(await call("video_study", { uri: next.uri }));
    }
    catch (e) { setError(message(e)); }
  }
  async function subtitle(language: "en" | "zh") {
    if (!state) return;
    setBusy(true); setError("");
    try { setStudy(await waitTask(await call("video_subtitle_add", { uri: state.uri, language, operationId: uid() }))); }
    catch (e) { setError(message(e)); } finally { setBusy(false); }
  }
  async function preview() {
    if (!context) return;
    setBusy(true); setError("");
    try { setClip(await waitTask(await call("preview_start", { sourceId: context.sourceId, exampleId: context.exampleId, operationId: uid() }))); }
    catch (e) { setError(message(e)); } finally { setBusy(false); }
  }
  const title = library?.playlist.find((i) => i.path === state?.uri)?.title;
  return <section className="mobile-watch">
    <div className="toolbar"><button disabled={busy} onClick={pick}>打开本地视频</button></div>
    {error && <p className="error" role="alert">{error}</p>}
    {!state && <p className="muted">选择手机上的视频。字幕、词库和学习记录均可离线使用。</p>}
    {!!library?.playlist.length && <label>播放列表<select aria-label="播放列表" value={state?.uri ?? ""} onChange={(e) => open(e.target.value)} disabled={busy}>
      <option value="" disabled>选择视频</option>{library.playlist.map((i) => <option key={i.path} value={i.path}>{i.title}</option>)}
    </select></label>}
    {state && <>
      <h2>{title}</h2><div ref={surface} className="mobile-video-surface" aria-label="视频画面" />
      {state.error && <p className="error" role="alert">{state.error}</p>}
      <div className="toolbar">
        <button onClick={() => control(state.playing ? "pause" : "play")}>{state.playing ? "暂停视频" : "播放视频"}</button>
        <button onClick={() => control("seek", Math.max(0, state.positionMs - 10000))}>后退 10 秒</button>
        <button onClick={() => control("seek", Math.min(state.durationMs, state.positionMs + 10000))}>前进 10 秒</button>
        <button onClick={() => control("seek", context?.startMs ?? 0)}>{context ? "重播当前句" : "从头播放"}</button>
        <button onClick={() => control("fullscreen")}>全屏</button>
      </div>
      <label className="video-seek">{time(state.positionMs)} / {time(state.durationMs)}<input aria-label="视频进度" type="range" min={0} max={Math.max(1, state.durationMs)}
        value={Math.min(state.positionMs,state.durationMs)} onChange={(e) => control("seek", Number(e.target.value))} /></label>
      <div className="field-row"><label>速度<select aria-label="播放速度" value={state.speed} onChange={(e) => control("speed", Number(e.target.value))}>
        {[0.5,0.75,1,1.25,1.5,2].map((speed) => <option value={speed} key={speed}>{speed}×</option>)}</select></label>
        {(["audio","subtitle"] as const).map((kind) => <label key={kind}>{kind === "audio" ? "音轨" : "画面字幕"}<select aria-label={kind === "audio" ? "音轨" : "画面字幕"}
          value={state.tracks.find((t) => t.kind === kind && t.selected)?.id ?? ""} onChange={(e) => control("track", undefined, e.target.value, kind)}>
          <option value="">关闭</option>{state.tracks.filter((t) => t.kind === kind).map((t) => <option key={t.id} value={t.id} disabled={!t.supported}>{t.title}</option>)}
        </select></label>)}
      </div>
      {notice && <p role="status">{notice}</p>}
      <div className="toolbar"><button disabled={busy || indexing || !study} onClick={() => subtitle("en")}>添加英文字幕</button>
        <button disabled={busy || indexing || !study} onClick={() => subtitle("zh")}>添加中文字幕</button></div>
      {study?.subtitles.warnings.map((warning) => <p className="notice" key={warning}>{warning}</p>)}
      {!state.playing && context && <article className="mobile-card">
        <span className="badge">{context.previous ? "上一句" : "当前对白"}</span><p className="quote">{context.text}</p>
        <p>{context.translation || "尚无译文，可添加中文字幕或手动填写释义。"}</p>
        <div className="actions"><button disabled={busy || context.audioStream === null} onClick={preview}>试听独立原声</button>
          <button disabled={busy} onClick={() => collect({ text:context.text,kind:"sentence",context:context.text,meaning:context.translation,
            sourceId:context.sourceId,sourceTitle:title,example:{ id:context.exampleId,text:context.text,startMs:context.startMs,endMs:context.endMs } })}>手动收录整句</button></div>
        {clip && <AudioPlayer path={clip.path} label="原声" />}
      </article>}
    </>}
  </section>;
}
