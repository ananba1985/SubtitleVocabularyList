import { useEffect, useRef, useState } from "react";
import { audioUrl } from "../api";
export function AudioPlayer({
  path,
  label = "原声",
  playbackKey,
  onPlayed,
  onFailed,
}: {
  path: string;
  label?: string;
  playbackKey?: number;
  onPlayed?: () => void;
  onFailed?: () => void;
}) {
  const audio = useRef<HTMLAudioElement>(null);
  const [notice, setNotice] = useState("");
  useEffect(() => {
    setNotice("");
    if (audio.current) audio.current.currentTime = 0;
    if (path)
      audio.current
        ?.play()
        .catch((error) =>
          setNotice(
            error?.name === "NotAllowedError"
              ? "请点击播放器开始播放。"
              : "音频暂时无法播放，请检查文件或重新准备。",
          ),
        );
  }, [path, playbackKey]);
  useEffect(() => {
    const stopOthers = (event: Event) => {
      if ((event as CustomEvent<HTMLAudioElement>).detail !== audio.current)
        audio.current?.pause();
    };
    window.addEventListener("svl_audio_started", stopOthers);
    return () => window.removeEventListener("svl_audio_started", stopOthers);
  }, []);
  return (
    <div className="audio-player">
      <span className="badge">{label}</span>
      <audio
        ref={audio}
        controls
        src={audioUrl(path)}
        preload="metadata"
        onPlay={() => {
          setNotice("");
          window.dispatchEvent(
            new CustomEvent("svl_audio_started", { detail: audio.current }),
          );
        }}
        onTimeUpdate={() => {
          if (audio.current && audio.current.currentTime > 0) onPlayed?.();
        }}
        onError={() => {
          setNotice("音频无法加载，请检查文件或重新准备。");
          onFailed?.();
        }}
      />
      {notice && (
        <span className="muted" role="status">
          {notice}
        </span>
      )}
    </div>
  );
}
