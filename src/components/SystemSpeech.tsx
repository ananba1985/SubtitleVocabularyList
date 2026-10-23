import { useState } from "react";
import { call, message, uid, waitTask } from "../api";
import type { SystemVoice, TaskSnapshot } from "../types";
import { AudioPlayer } from "./AudioPlayer";
import { useRuntimeFeatures } from "../runtimeFeatures";

export function SystemSpeech({
  text,
  label = "系统英语朗读",
}: {
  text: string;
  label?: string;
}) {
  const features = useRuntimeFeatures();
  const [task, setTask] = useState<TaskSnapshot | null>(null),
    [result, setResult] = useState<{
      path: string;
      voice: SystemVoice;
      text: string;
      requestId: string;
    } | null>(null),
    [busy, setBusy] = useState(false),
    [error, setError] = useState("");
  async function speak() {
    setBusy(true);
    setError("");
    setResult(null);
    setTask(null);
    try {
      const started = await call("speech_start", {
        text,
        operationId: uid(),
      });
      setTask(started);
      const audio = await waitTask(
        started,
        setTask,
      );
      setResult({ ...audio, text, requestId: started.id });
    } catch (error) {
      setError(message(error));
    } finally {
      setBusy(false);
    }
  }
  if (!features.systemSpeech) return null;
  return (
    <div className="system-speech">
      <div className="actions">
        <button
          className="secondary"
          disabled={busy || !text.trim()}
          onClick={speak}
        >
          {busy ? "正在准备本地语音…" : label}
        </button>
        {busy && task && (
          <button
            onClick={() =>
              call("task_cancel", { taskId: task.id }).catch((error) =>
                setError(message(error)),
              )
            }
          >
            取消准备
          </button>
        )}
      </div>
      {error && (
        <p className="error" role="alert">
          {error}
        </p>
      )}
      {result && result.text === text && (
        <AudioPlayer
          key={result.requestId}
          path={result.path}
          label="系统语音"
        />
      )}
    </div>
  );
}
