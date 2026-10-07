import { useEffect, useRef, useState } from "react";
import { audioUrl } from "../api";
export function AudioPlayer({
  path,
  label = "原声",
}: {
  path: string;
  label?: string;
}) {
  const audio = useRef<HTMLAudioElement>(null);
  const [notice, setNotice] = useState("");
  useEffect(() => {
    setNotice("");
    if (path)
      audio.current
        ?.play()
        .catch((error) =>
          setNotice(
            error?.name === "NotAllowedError"
              ? "请点击播放器开始播放。"
              : "原声暂时无法播放，请检查文件或重新截取。",
          ),
        );
  }, [path]);
  return (
    <div className="audio-player">
      <span className="badge">{label}</span>
      <audio
        ref={audio}
        controls
        src={audioUrl(path)}
        preload="metadata"
        onPlay={() => setNotice("")}
        onError={() => setNotice("原声无法加载，请检查文件或重新截取。")}
      />
      {notice && (
        <span className="muted" role="status">
          {notice}
        </span>
      )}
    </div>
  );
}
