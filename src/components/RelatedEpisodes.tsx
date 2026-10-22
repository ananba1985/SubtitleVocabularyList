import { useEffect, useRef, useState } from "react";
import { emit } from "@tauri-apps/api/event";
import { call, message, uid, waitTask } from "../api";
import type { RelatedVideos } from "../generated/ipc";

export function RelatedEpisodes({
  path,
  loaded,
}: {
  path: string;
  loaded: boolean;
}) {
  const [pending, setPending] = useState<{
    path: string;
    videos: RelatedVideos;
  } | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<{ path: string; message: string } | null>(
    null,
  );
  const decided = useRef(new Set<string>());
  useEffect(() => {
    let disposed = false;
    setPending(null);
    setError(null);
    if (loaded && path) {
      call("watch_related_videos", { path })
        .then((videos) => {
          if (!disposed && videos && !decided.current.has(videos.key)) {
            setPending({ path, videos });
          }
        })
        .catch((e) => {
          if (!disposed)
            setError({ path, message: `同目录剧集检查未完成：${message(e)}` });
        });
    }
    return () => {
      disposed = true;
    };
  }, [path, loaded]);

  async function confirm(value: NonNullable<typeof pending>) {
    setBusy(true);
    setError(null);
    try {
      const task = await call("import_start", {
        paths: value.videos.paths,
        operationId: uid(),
      });
      decided.current.add(value.videos.key);
      setPending((current) => (current === value ? null : current));
      // The existing task page owns progress, cancellation and errors, even after navigation.
      void waitTask(task)
        .then(() => emit("watch_sources_updated"))
        .catch(() => {});
    } catch (e) {
      setError({ path: value.path, message: message(e) });
    } finally {
      setBusy(false);
    }
  }

  if (!loaded || !path) return null;
  const visibleError = error?.path === path ? error.message : "";
  const value = pending?.path === path ? pending : null;
  if (!value)
    return visibleError ? (
      <p className="watch-related muted" role="status">
        {visibleError}
      </p>
    ) : null;
  return (
    <section className="watch-related" aria-label="发现同目录剧集">
      <strong>是否导入同目录完整剧集？</strong>
      <p>
        发现《{value.videos.series}》共 {value.videos.total} 个视频，另有{" "}
        {value.videos.paths.length} 集待导入。当前视频继续播放。
      </p>
      <details>
        <summary>查看待导入文件（{value.videos.paths.length}）</summary>
        <small>{value.videos.directory}</small>
        <ul>
          {value.videos.paths.map((file) => (
            <li key={file}>{file.split(/[\\/]/).pop()}</li>
          ))}
        </ul>
      </details>
      {visibleError && (
        <p role="alert" className="error">
          {visibleError}
        </p>
      )}
      <div className="watch-playlist-actions">
        <button
          className="primary"
          disabled={busy}
          onClick={() => void confirm(value)}
        >
          {busy ? "正在安排…" : `导入其余 ${value.videos.paths.length} 集`}
        </button>
        <button
          disabled={busy}
          onClick={() => {
            decided.current.add(value.videos.key);
            setPending(null);
          }}
        >
          仅导入当前集
        </button>
      </div>
    </section>
  );
}
