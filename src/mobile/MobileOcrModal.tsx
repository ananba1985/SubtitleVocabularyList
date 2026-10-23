import { useEffect, useRef, useState } from "react";
import type { PointerEvent } from "react";
import { audioUrl, call, message, uid, waitTask } from "../api";
import type { CollectionSeed } from "../types";
import type { PixelRect } from "../generated/ipc";
import { AudioPlayer } from "../components/AudioPlayer";

export function MobileOcrModal({
  seed,
  changed,
  close,
}: {
  seed: CollectionSeed;
  changed: (seed: CollectionSeed) => void;
  close: () => void;
}) {
  const image = seed.image;
  const [crop, setCrop] = useState<PixelRect | undefined>(seed.imageCrop);
  const [text, setText] = useState(seed.ocrText ?? ""),
    [error, setError] = useState("");
  const [busy, setBusy] = useState(false),
    [taskId, setTaskId] = useState("");
  const [audio, setAudio] = useState(seed.saveFrameAudio ?? false);
  const [range, setRange] = useState(
    seed.frameRange ?? {
      startMs: Math.max(0, (image?.frame?.positionMs ?? 1500) - 1500),
      endMs: (image?.frame?.positionMs ?? 1500) + 1500,
    },
  );
  const [clip, setClip] = useState<{ path: string } | null>(null),
    [played, setPlayed] = useState(false);
  const box = useRef<HTMLDivElement>(null),
    drag = useRef<{ x: number; y: number } | null>(null),
    generation = useRef(0),
    active = useRef("");
  const whole = image
    ? { x: 0, y: 0, width: image.width, height: image.height }
    : undefined;
  useEffect(() => {
    if (image || !seed.imageSource) return;
    let disposed = false;
    call("mobile_image_prepare", seed.imageSource)
      .then((next) => {
        if (!disposed) changed({ ...seed, image: next });
      })
      .catch((e) => {
        if (!disposed) setError(message(e));
      });
    return () => {
      disposed = true;
    };
  }, [image?.id, seed.imageSource?.id, changed]);
  useEffect(
    () => () => {
      generation.current++;
      if (active.current)
        void call("task_cancel", { taskId: active.current }).catch(() => {});
    },
    [],
  );
  function persist(patch: Partial<CollectionSeed>) {
    changed({ ...seed, ...patch });
  }
  function invalidate() {
    generation.current++;
    setClip(null);
    setPlayed(false);
  }
  function select(next: PixelRect) {
    invalidate();
    setCrop(next);
    persist({ imageCrop: next });
  }
  function point(event: PointerEvent) {
    const rect = box.current!.getBoundingClientRect();
    return {
      x: Math.max(
        0,
        Math.min(
          image!.width - 1,
          Math.round(((event.clientX - rect.x) * image!.width) / rect.width),
        ),
      ),
      y: Math.max(
        0,
        Math.min(
          image!.height - 1,
          Math.round(((event.clientY - rect.y) * image!.height) / rect.height),
        ),
      ),
    };
  }
  function move(event: PointerEvent) {
    if (!drag.current || !image) return;
    const end = point(event),
      start = drag.current;
    setCrop({
      x: Math.min(start.x, end.x),
      y: Math.min(start.y, end.y),
      width: Math.max(1, Math.abs(end.x - start.x)),
      height: Math.max(1, Math.abs(end.y - start.y)),
    });
  }
  async function recognize() {
    if (!image || !whole) return;
    const version = ++generation.current;
    setBusy(true);
    setError("");
    try {
      const task = await call("mobile_ocr_start", {
        imageId: image.id,
        rect: crop ?? whole,
        operationId: uid(),
      });
      active.current = task.id;
      setTaskId(task.id);
      const result = await waitTask(task);
      if (version === generation.current) {
        setText(result.text);
        persist({ ocrText: result.text, imageCrop: crop ?? whole });
      }
    } catch (e) {
      setError(message(e));
    } finally {
      active.current = "";
      setTaskId("");
      setBusy(false);
    }
  }
  async function preview() {
    if (!image) return;
    setBusy(true);
    setError("");
    setPlayed(false);
    try {
      setClip(
        await waitTask(
          await call("mobile_frame_preview", {
            imageId: image.id,
            text,
            range,
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
  async function confirm() {
    if (!image) return;
    setBusy(true);
    setError("");
    try {
      const draft = image.frame
        ? await call("mobile_frame_draft", {
            imageId: image.id,
            text,
            range: audio ? range : null,
          })
        : await call("mobile_image_draft", { imageId: image.id, text });
      changed({
        ...seed,
        ...draft,
        example: draft.example ?? undefined,
        image: undefined,
        imageSource: undefined,
        saveAudio: audio && !!draft.example,
        imageCrop: undefined,
        ocrText: undefined,
        saveFrameAudio: undefined,
        frameRange: undefined,
      });
    } catch (e) {
      setError(message(e));
    } finally {
      setBusy(false);
    }
  }
  const selected = crop ?? whole;
  return (
    <div className="modal-backdrop">
      <section
        className="modal mobile-ocr"
        role="dialog"
        aria-modal="true"
        aria-label="离线图片识别"
      >
        <h2>{image?.frame ? "暂停帧 OCR" : "图片 OCR"}</h2>
        <p>
          {image?.origin ?? "正在读取授权图片…"}
          。拖动选择英语区域，识别后可修改文字。
        </p>
        {error && (
          <p className="error" role="alert">
            {error}
          </p>
        )}
        {image && (
          <>
            <div className="mobile-ocr-image">
              <div
                ref={box}
                style={{
                  position: "relative",
                  display: "inline-block",
                  touchAction: "none",
                }}
                onPointerDown={(e) => {
                  if (busy) return;
                  e.currentTarget.setPointerCapture(e.pointerId);
                  drag.current = point(e);
                  invalidate();
                }}
                onPointerMove={move}
                onPointerUp={() => {
                  drag.current = null;
                  if (crop) persist({ imageCrop: crop });
                }}
                onPointerCancel={() => {
                  drag.current = null;
                }}
              >
                <img
                  src={audioUrl(image.path)}
                  alt="待识别图片"
                  draggable={false}
                  style={{
                    display: "block",
                    maxWidth: "100%",
                    maxHeight: "38dvh",
                    width: "auto",
                    height: "auto",
                  }}
                />
                {selected && (
                  <div
                    style={{
                      pointerEvents: "none",
                      position: "absolute",
                      border: "2px solid #eea738",
                      background: "#f2c85222",
                      left: `${(100 * selected.x) / image.width}%`,
                      top: `${(100 * selected.y) / image.height}%`,
                      width: `${(100 * selected.width) / image.width}%`,
                      height: `${(100 * selected.height) / image.height}%`,
                    }}
                  />
                )}
              </div>
            </div>
            <div className="actions">
              <button disabled={busy} onClick={() => select(whole!)}>
                全图
              </button>
              <button
                disabled={busy}
                onClick={() =>
                  select({
                    x: 0,
                    y: Math.round(image.height * 0.55),
                    width: image.width,
                    height: Math.floor(image.height * 0.45),
                  })
                }
              >
                字幕区域
              </button>
              <button disabled={busy} onClick={recognize}>
                离线识别英语
              </button>
              {taskId && (
                <button
                  onClick={() =>
                    call("task_cancel", { taskId }).catch((e) =>
                      setError(message(e)),
                    )
                  }
                >
                  取消识别
                </button>
              )}
            </div>
            <label>
              识别文字（可纠错）
              <textarea
                aria-label="识别文字"
                rows={4}
                value={text}
                disabled={busy}
                onChange={(e) => {
                  invalidate();
                  setText(e.target.value);
                  persist({ ocrText: e.target.value });
                }}
              />
            </label>
            {image.frame && (
              <>
                <p className="notice">
                  暂停帧位于 {(image.frame.positionMs / 1000).toFixed(2)}{" "}
                  秒。图片没有可靠对白边界；保存原声需手动核对区间。
                </p>
                <label className="check-label">
                  <input
                    type="checkbox"
                    checked={audio}
                    disabled={busy}
                    onChange={(e) => {
                      invalidate();
                      setAudio(e.target.checked);
                      persist({ saveFrameAudio: e.target.checked });
                    }}
                  />
                  同时保存人工核对的原声
                </label>
                {audio && (
                  <>
                    <div className="field-row">
                      {(["startMs", "endMs"] as const).map((key) => (
                        <label key={key}>
                          {key === "startMs" ? "开始秒数" : "结束秒数"}
                          <input
                            aria-label={
                              key === "startMs"
                                ? "原声开始秒数"
                                : "原声结束秒数"
                            }
                            type="number"
                            step="0.1"
                            min="0"
                            disabled={busy}
                            value={range[key] / 1000}
                            onChange={(e) => {
                              const next = {
                                ...range,
                                [key]: Math.round(
                                  Number(e.target.value) * 1000,
                                ),
                              };
                              invalidate();
                              setRange(next);
                              persist({ frameRange: next });
                            }}
                          />
                        </label>
                      ))}
                    </div>
                    <button disabled={busy || !text.trim()} onClick={preview}>
                      生成原声供核对
                    </button>
                    {clip && (
                      <AudioPlayer
                        key={clip.path}
                        path={clip.path}
                        label="人工区间原声"
                        onPlayed={() => setPlayed(true)}
                      />
                    )}
                    <p className="muted">
                      试听后确认区间；词条仍需在下一步确认保存。
                    </p>
                  </>
                )}
              </>
            )}
          </>
        )}
        <footer>
          <button disabled={busy} onClick={close}>
            放弃草稿
          </button>
          <button
            disabled={busy || !image || !text.trim() || (audio && !played)}
            onClick={confirm}
          >
            核对后继续收录
          </button>
        </footer>
      </section>
    </div>
  );
}
