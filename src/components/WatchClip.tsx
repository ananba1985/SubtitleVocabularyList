import { useEffect, useRef, useState } from "react";
import { audioUrl, call, message, uid, waitTask } from "../api";
import type { WatchClipRange, WatchContext } from "../generated/ipc";

/** An optional manual interval; never changes the automatic OCR collection snapshot. */
export function WatchClip({
  context,
  playerSession,
  duration,
  onChange,
}: {
  context: WatchContext;
  playerSession: string;
  duration: number;
  onChange: (range: WatchClipRange | null) => void;
}) {
  const [start, setStart] = useState(
      Math.max(0, context.startMs / 1000 - 2).toFixed(1),
    ),
    [end, setEnd] = useState(
      Math.min(duration, context.startMs / 1000 + 2).toFixed(1),
    );
  const [path, setPath] = useState(""),
    [busy, setBusy] = useState(false),
    [error, setError] = useState(""),
    [accepted, setAccepted] = useState(false);
  const alive = useRef(true),
    task = useRef(""),
    cancelled = useRef(false);
  useEffect(() => {
    alive.current = true;
    return () => {
      alive.current = false;
      if (task.current)
        void call("task_cancel", { taskId: task.current }).catch(() => {});
    };
  }, []);
  const range = {
    startMs: Math.round(Number(start) * 1000),
    endMs: Math.round(Number(end) * 1000),
  };
  const valid =
    !!start.trim() &&
    !!end.trim() &&
    Number.isFinite(range.startMs) &&
    Number.isFinite(range.endMs) &&
    range.startMs >= 0 &&
    range.endMs > range.startMs &&
    range.endMs <= Math.round(duration * 1000);
  const reset = () => {
    setPath("");
    setAccepted(false);
    setError("");
    onChange(null);
  };
  async function prepare() {
    reset();
    cancelled.current = false;
    setBusy(true);
    try {
      await call("watch_control", {
        control: {
          action: "cancel_auto",
          value: null,
          text: null,
          expectedRevision: context.revision,
        },
      });
      if (!alive.current || cancelled.current) return;
      const bound = await call("watch_collection_prepare", {
        sessionId: playerSession,
        expectedRevision: context.revision,
        clipRange: range,
      });
      if (!alive.current || cancelled.current) return;
      const started = await call("preview_start", {
        sourceId: bound.sourceId,
        exampleId: bound.exampleId,
        operationId: uid(),
      });
      task.current = started.id;
      if (!alive.current || cancelled.current) {
        await call("task_cancel", { taskId: started.id });
        return;
      }
      const preview = await waitTask(started);
      if (alive.current && !cancelled.current) setPath(preview.path);
    } catch (e) {
      if (alive.current) setError(message(e));
    } finally {
      task.current = "";
      if (alive.current) setBusy(false);
    }
  }
  return (
    <details className="watch-clip">
      <summary>原声区间（可选）{accepted ? " · 已选" : ""}</summary>
      <p className="muted">
        OCR
        只有采集位置。下面是建议范围，请试听核对；采用后仅用于手动收录，自动收录仍只保存文字。
      </p>
      {context.audioStream === null ? (
        <p className="muted">当前没有可提取的音轨，可仅保存文字。</p>
      ) : (
        <>
          <fieldset disabled={busy}>
            <div className="watch-clip-range">
              <label>
                开始（秒）
                <input
                  type="number"
                  min={0}
                  max={duration}
                  step={0.1}
                  value={start}
                  onChange={(e) => {
                    setStart(e.target.value);
                    reset();
                  }}
                />
              </label>
              <label>
                结束（秒）
                <input
                  type="number"
                  min={0}
                  max={duration}
                  step={0.1}
                  value={end}
                  onChange={(e) => {
                    setEnd(e.target.value);
                    reset();
                  }}
                />
              </label>
              <button disabled={!valid} onClick={() => void prepare()}>
                准备所选片段
              </button>
            </div>
            {path && (
              <>
                <audio
                  controls
                  preload="metadata"
                  src={audioUrl(path)}
                  aria-label="试听手动选择的原声片段"
                />
                <div className="watch-caption-actions">
                  <button
                    onClick={() => {
                      setAccepted(true);
                      onChange(range);
                    }}
                  >
                    使用此片段
                  </button>
                  <button className="text-button" onClick={reset}>
                    仅存文字
                  </button>
                  {accepted && (
                    <small role="status">
                      已选；点击手动收录后仍需确认保存。
                    </small>
                  )}
                </div>
              </>
            )}
          </fieldset>
          {busy && (
            <button
              className="text-button"
              onClick={() => {
                cancelled.current = true;
                if (task.current)
                  void call("task_cancel", { taskId: task.current }).catch(
                    (e) => setError(message(e)),
                  );
              }}
            >
              正在准备… 取消
            </button>
          )}
        </>
      )}
      {error && (
        <p className="error" role="alert">
          {error}
        </p>
      )}
    </details>
  );
}
