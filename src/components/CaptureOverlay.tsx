import { useEffect, useRef, useState } from "react";
import { convertFileSrc } from "@tauri-apps/api/core";
import { call, message } from "../api";
import type { ScreenSession, TaskSnapshot } from "../types";
import { pixelRect, type Point } from "../selection";

export function CaptureOverlay({ sessionId }: { sessionId: string }) {
  const [session, setSession] = useState<ScreenSession | null>(null),
    [ready, setReady] = useState(false),
    [start, setStart] = useState<Point | null>(null),
    [end, setEnd] = useState<Point | null>(null),
    [busy, setBusy] = useState(false),
    [error, setError] = useState("");
  const startRef = useRef<Point | null>(null);
  async function cancel() {
    try {
      await call("capture_ocr_cancel", { sessionId });
    } catch (error) {
      setError(message(error));
    }
  }
  useEffect(() => {
    call<ScreenSession>("capture_session_get", { sessionId })
      .then(setSession)
      .catch((error) => setError(message(error)));
  }, [sessionId]);
  useEffect(() => {
    const key = (event: KeyboardEvent) => {
      if (event.key === "Escape" && !busy) {
        event.preventDefault();
        void cancel();
      }
    };
    window.addEventListener("keydown", key);
    return () => window.removeEventListener("keydown", key);
  }, [busy, sessionId]);
  const selection =
    start && end
      ? {
          left: Math.min(start.x, end.x),
          top: Math.min(start.y, end.y),
          width: Math.abs(end.x - start.x),
          height: Math.abs(end.y - start.y),
        }
      : null;
  async function submit(origin: Point, point: Point) {
    if (!session || busy) return;
    const rect = pixelRect(
      origin,
      point,
      { width: window.innerWidth, height: window.innerHeight },
      session.bounds,
    );
    if (!rect) {
      setError("选区过小，请重新拖选英语文字。");
      setStart(null);
      setEnd(null);
      return;
    }
    setBusy(true);
    setError("");
    try {
      await call<TaskSnapshot>("capture_ocr_submit", { sessionId, rect });
    } catch (error) {
      setError(message(error));
      setBusy(false);
    }
  }
  return (
    <div
      className="capture-overlay"
      role="region"
      aria-label="截图选区"
      onPointerDown={(event) => {
        if (!ready || busy || event.button !== 0) return;
        event.currentTarget.setPointerCapture(event.pointerId);
        setError("");
        startRef.current = { x: event.clientX, y: event.clientY };
        setStart(startRef.current);
        setEnd({ x: event.clientX, y: event.clientY });
      }}
      onPointerMove={(event) => {
        if (startRef.current && !busy)
          setEnd({ x: event.clientX, y: event.clientY });
      }}
      onPointerUp={(event) => {
        const origin = startRef.current;
        startRef.current = null;
        if (origin && !busy) {
          setEnd({ x: event.clientX, y: event.clientY });
          void submit(origin, { x: event.clientX, y: event.clientY });
        }
      }}
    >
      {session && (
        <img
          className="capture-background"
          alt="本次屏幕快照"
          src={convertFileSrc(session.imagePath)}
          draggable={false}
          onLoad={() => setReady(true)}
          onError={() => {
            setReady(false);
            setError("本次快照无法加载，请取消后重新截图。");
          }}
        />
      )}
      {!selection && <div className="capture-dim" />}
      {selection && <div className="capture-selection" style={selection} />}
      <div
        className="capture-toolbar"
        onPointerDown={(event) => event.stopPropagation()}
        onPointerUp={(event) => event.stopPropagation()}
      >
        <strong>
          {busy
            ? "正在提交选区…"
            : ready
              ? "拖选英语文字，松开后识别"
              : "正在加载本次快照…"}
        </strong>
        <span>Esc 取消</span>
        <button onClick={cancel} disabled={busy}>
          取消截图
        </button>
        {error && (
          <p className="error" role="alert">
            {error}
          </p>
        )}
      </div>
    </div>
  );
}
