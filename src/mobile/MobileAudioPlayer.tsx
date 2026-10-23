import { useEffect, useRef, useState } from "react";
import { call, message } from "../api";
import type { CommandResult } from "../commands";
import type { AudioPlayerProps } from "../components/AudioPlayer";

const time = (ms: number) => `${Math.floor(ms / 60000)}:${String(Math.floor(ms / 1000) % 60).padStart(2, "0")}`;

export function MobileAudioPlayer({ path, label = "原声", playbackKey, onPlayed, onFailed, onPlaybackEvent }: AudioPlayerProps) {
  const [state, setState] = useState<CommandResult<"audio_state"> | null>(null);
  const [error, setError] = useState("");
  const [replay, setReplay] = useState(0);
  const callbacks = useRef({ onPlayed, onFailed, onPlaybackEvent });
  callbacks.current = { onPlayed, onFailed, onPlaybackEvent };
  const id = useRef("");
  useEffect(() => {
    let disposed = false, session = "", polling = false, reported = false, ended = false;
    setError(""); setState(null);
    const update = (value: CommandResult<"audio_state">) => {
      if (disposed) return;
      setState(value);
      if (value.error) { setError(value.error); callbacks.current.onFailed?.(); callbacks.current.onPlaybackEvent?.("error", value.positionMs/1000, value.error); }
      if (value.played && !reported) { reported = true; callbacks.current.onPlayed?.(); callbacks.current.onPlaybackEvent?.("started",value.positionMs/1000); }
      if(value.active && value.ended && value.played && !ended){ended=true;callbacks.current.onPlaybackEvent?.("ended",value.positionMs/1000);}
    };
    call("audio_open", { path }).then((value) => {
      session = value.sessionId;
      if (disposed) { call("audio_control", { sessionId: session, action: "close" }).catch(() => {}); return; }
      id.current = session;
      update(value);
    }).catch((e) => { if (!disposed) { setError(message(e)); callbacks.current.onFailed?.(); } });
    const timer = window.setInterval(async () => {
      if (!session || disposed || polling) return;
      polling = true;
      try { update(await call("audio_state", { sessionId: session })); }
      catch (e) { if (!disposed) { setError(message(e)); callbacks.current.onFailed?.(); } }
      finally { polling = false; }
    }, 400);
    return () => {
      disposed = true; window.clearInterval(timer);
      if (id.current === session) id.current = "";
      if (session) call("audio_control", { sessionId: session, action: "close" }).catch(() => {});
    };
  }, [path, playbackKey, replay]);
  async function control(action: "play" | "pause" | "seek", positionMs?: number) {
    if (action === "play" && !state?.active) { setReplay((value) => value + 1); return; }
    try { setState(await call("audio_control", { sessionId: id.current, action, positionMs })); }
    catch (e) { setError(message(e)); callbacks.current.onFailed?.(); }
  }
  return <div className="audio-player mobile-audio">
    <span className="badge">{label}</span>
    <button aria-label={state?.playing ? "暂停音频" : "播放音频"} disabled={!state || !!error}
      onClick={() => control(state?.playing ? "pause" : "play")}>{state?.playing ? "暂停" : "播放"}</button>
    <input type="range" aria-label="音频进度" min={0} max={Math.max(state?.durationMs ?? 0, 1)}
      value={Math.min(state?.positionMs ?? 0, state?.durationMs ?? 0)} disabled={!state?.durationMs || !!error}
      onChange={(e) => control("seek", Number(e.target.value))} />
    <span className="muted">{time(state?.positionMs ?? 0)} / {time(state?.durationMs ?? 0)}</span>
    {error && <span className="error" role="alert">{error}</span>}
  </div>;
}
