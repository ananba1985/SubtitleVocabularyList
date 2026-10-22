import { useEffect, useRef, useState } from "react";
import { audioUrl, time } from "../api";
import type { PixelRect, WatchFrame } from "../generated/ipc";

export function WatchOcr({
  frame,
  control,
}: {
  frame: WatchFrame;
  control: (action: string, text: string) => Promise<void>;
}) {
  const [text, setText] = useState(frame.text),
    [crop, setCrop] = useState<PixelRect | null>(null);
  const origin = useRef<{ x: number; y: number } | null>(null);
  useEffect(() => {
    setText(frame.text);
  }, [frame.text]);
  return (
    <div className="watch-ocr">
      <p>暂停帧 OCR · 采集位置 {time(frame.positionMs)} · 请核对英文后使用</p>
      <div className="watch-frame">
        <img
          src={audioUrl(frame.path)}
          alt="当前暂停帧，可拖动框选字幕"
          draggable={false}
          onPointerDown={(e) => {
            const bounds = e.currentTarget.getBoundingClientRect();
            origin.current = {
              x: ((e.clientX - bounds.left) * frame.width) / bounds.width,
              y: ((e.clientY - bounds.top) * frame.height) / bounds.height,
            };
            setCrop(null);
            e.currentTarget.setPointerCapture(e.pointerId);
          }}
          onPointerMove={(e) => {
            if (!origin.current) return;
            const bounds = e.currentTarget.getBoundingClientRect();
            const x = Math.max(
              0,
              Math.min(
                frame.width,
                ((e.clientX - bounds.left) * frame.width) / bounds.width,
              ),
            );
            const y = Math.max(
              0,
              Math.min(
                frame.height,
                ((e.clientY - bounds.top) * frame.height) / bounds.height,
              ),
            );
            const left = Math.round(Math.min(x, origin.current.x)),
              top = Math.round(Math.min(y, origin.current.y));
            setCrop({
              x: left,
              y: top,
              width: Math.min(
                frame.width - left,
                Math.round(Math.abs(x - origin.current.x)),
              ),
              height: Math.min(
                frame.height - top,
                Math.round(Math.abs(y - origin.current.y)),
              ),
            });
          }}
          onPointerUp={() => {
            origin.current = null;
          }}
          onPointerCancel={() => {
            origin.current = null;
          }}
        />
        {crop && (
          <div
            className="watch-frame-selection"
            style={{
              left: `${(crop.x / frame.width) * 100}%`,
              top: `${(crop.y / frame.height) * 100}%`,
              width: `${(crop.width / frame.width) * 100}%`,
              height: `${(crop.height / frame.height) * 100}%`,
            }}
          />
        )}
      </div>
      <div className="watch-caption-actions">
        <button
          disabled={frame.busy || !crop || crop.width < 4 || crop.height < 4}
          onClick={() => void control("ocr_crop", JSON.stringify(crop))}
        >
          识别选区
        </button>
        <span className="muted">
          {frame.busy ? "正在识别…" : "可在图上重新框选字幕"}
        </span>
      </div>
      {frame.error && (
        <p className="error" role="alert">
          {frame.error}
        </p>
      )}
      <label>
        英文原文
        <textarea
          value={text}
          maxLength={4000}
          disabled={frame.busy}
          onChange={(e) => setText(e.target.value)}
        />
      </label>
      <button
        disabled={frame.busy || !text.trim()}
        onClick={() => void control("confirm_ocr", text)}
      >
        确认原文
      </button>
      <small className="muted">
        只有画面采集位置，自动收录仅存文字；确认原文后可展开“原声区间”，手动选择并试听。
      </small>
    </div>
  );
}
