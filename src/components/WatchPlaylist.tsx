import type { PlayerSnapshot } from "../generated/ipc";

export function WatchPlaylist({
  state,
  busy,
  change,
  add,
}: {
  state: PlayerSnapshot;
  busy: boolean;
  change: (
    action: string,
    index?: number,
    destination?: number,
  ) => Promise<void>;
  add: (directory: boolean) => Promise<void>;
}) {
  const items = state.playback.playlist;
  return (
    <details className="watch-playlist">
      <summary>
        播放列表（{items.length}）{state.title ? ` · ${state.title}` : ""}
      </summary>
      <div className="watch-playlist-actions">
        <button disabled={busy} onClick={() => void add(false)}>
          添加视频
        </button>
        <button disabled={busy} onClick={() => void add(true)}>
          添加目录
        </button>
        <button
          disabled={busy || !items.length}
          onClick={() => void change("clear")}
        >
          清空列表
        </button>
      </div>
      {items.length ? (
        <ol>
          {items.map((item, index) => (
            <li
              key={item.path}
              className={item.path === state.path ? "current" : ""}
            >
              <button
                className="watch-playlist-title"
                title={item.path}
                aria-current={item.path === state.path ? "true" : undefined}
                disabled={busy}
                onClick={() => void change("select", index)}
              >
                {index + 1}. {item.title}
                {item.path === state.path ? " · 当前" : ""}
              </button>
              <button
                aria-label={`上移 ${item.title}`}
                disabled={busy || index === 0}
                onClick={() => void change("move", index, index - 1)}
              >
                ↑
              </button>
              <button
                aria-label={`下移 ${item.title}`}
                disabled={busy || index === items.length - 1}
                onClick={() => void change("move", index, index + 1)}
              >
                ↓
              </button>
              <button
                aria-label={`移除 ${item.title}`}
                disabled={busy}
                onClick={() => void change("remove", index)}
              >
                移除
              </button>
            </li>
          ))}
        </ol>
      ) : (
        <p>添加多个视频或一个目录，按文件名中的集数排序。</p>
      )}
    </details>
  );
}
