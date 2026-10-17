import { useState } from "react";
import { call, message, uid, waitTask } from "../api";

import { AudioPlayer } from "./AudioPlayer";
export function AssessmentHistoryAudio({
  sessionId,
  itemId,
  kind,
}: {
  sessionId: string;
  itemId: string;
  kind: string;
}) {
  const [path, setPath] = useState<string | null>(null),
    [busy, setBusy] = useState(false),
    [error, setError] = useState("");
  async function play() {
    setBusy(true);
    setError("");
    try {
      const task = await call("assessment_audio_prepare", {
        sessionId,
        itemId,
        operationId: uid(),
      });
      const result = await waitTask<{ path: string }>(task);
      setPath(result.path);
    } catch (e) {
      setError(message(e));
    } finally {
      setBusy(false);
    }
  }
  return (
    <div>
      <button disabled={busy} onClick={() => void play()}>
        {busy
          ? "准备英语录音…"
          : kind === "original"
            ? "播放真实原声"
            : "播放本地英语语音"}
      </button>
      {path && (
        <AudioPlayer
          path={path}
          label={kind === "original" ? "真实原声" : "Windows 本地英语语音"}
        />
      )}{" "}
      {error && (
        <p className="error" role="alert">
          {error}
        </p>
      )}
    </div>
  );
}
