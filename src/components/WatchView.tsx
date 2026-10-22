import { useCallback, useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { open } from "@tauri-apps/plugin-dialog";
import { call, message, time } from "../api";
import { SubtitleSettings } from "./SubtitleSettings";
import { WatchQuery, type WatchRequest } from "./WatchQuery";
import { WatchOcr } from "./WatchOcr";
import { WatchClip } from "./WatchClip";
import { WatchPlaylist } from "./WatchPlaylist";
import { RelatedEpisodes } from "./RelatedEpisodes";
import type { CollectionSeed, Settings, SourceSummary } from "../types";
import type {
  PlayerSnapshot,
  WatchControl,
  WatchProgress,
  WatchClipRange,
} from "../generated/ipc";

export function WatchView({
  settings,
  onSettings,
  collect,
  collectionOpen,
  onFullscreenChange,
}: {
  settings: Settings | null;
  onSettings: (value: Settings) => void;
  collect: (seed: CollectionSeed) => void;
  collectionOpen: boolean;
  onFullscreenChange: (value: boolean) => void;
}) {
  const [state, setState] = useState<PlayerSnapshot | null>(null);
  const [sources, setSources] = useState<SourceSummary[]>([]);
  const [selected, setSelected] = useState("");
  const [progress, setProgress] = useState<WatchProgress | null>(null);
  const [busy, setBusy] = useState(false),
    [error, setError] = useState("");
  const [options, setOptions] = useState(false);
  const [query, setQuery] = useState<
    (WatchRequest & { revision: number; playerSession: string }) | null
  >(null);
  const [fullscreen, setFullscreen] = useState(false);
  const [manualClip, setManualClip] = useState<{
    session: string;
    revision: number;
    range: WatchClipRange;
  } | null>(null);
  const video = useRef<HTMLDivElement>(null),
    request = useRef(0);
  const refresh = useCallback(async () => {
    const id = ++request.current;
    const next = await call("watch_state_get");
    if (id === request.current) setState(next);
  }, []);
  useEffect(() => {
    let disposed = false,
      running = false,
      wake = false;
    const receive = async () => {
      wake = true;
      if (running) return;
      running = true;
      try {
        do {
          wake = false;
          while (!disposed) {
            try {
              const next = await call("watch_files_open_next");
              if (!next) break;
              if (!disposed) {
                setError("");
                setState(next);
              }
            } catch (e) {
              if (!disposed) setError(message(e));
              break;
            }
          }
        } while (wake && !disposed);
      } finally {
        running = false;
      }
    };
    const stop = listen("watch_files_pending", () => {
      void receive();
    })
      .then((unlisten) => {
        if (disposed) unlisten();
        else void receive();
        return unlisten;
      })
      .catch((e) => {
        if (!disposed) setError(message(e));
      });
    return () => {
      disposed = true;
      void stop.then((unlisten) => unlisten?.());
    };
  }, []);
  const control = async (
    action: string,
    value?: number,
    text?: string,
    expectedRevision?: number,
  ) => {
    setError("");
    try {
      const next = await call("watch_control", {
        control: {
          action,
          value: value ?? null,
          text: text ?? null,
          expectedRevision: expectedRevision ?? null,
        } satisfies WatchControl,
      });
      setState(next);
    } catch (e) {
      setError(message(e));
    }
  };
  useEffect(() => {
    let disposed = false;
    const stop = listen<Pick<PlayerSnapshot, "revision">>(
      "watch_subtitle_file_requested",
      async (event) => {
        try {
          const path = await open({
            filters: [
              { name: "字幕", extensions: ["srt", "ass", "ssa", "vtt", "sup"] },
            ],
          });
          if (!disposed && typeof path === "string")
            await control(
              "subtitle_file",
              undefined,
              path,
              event.payload.revision,
            );
        } catch (e) {
          if (!disposed) setError(message(e));
        } finally {
          await call("watch_control", {
            control: {
              action: "subtitle_dialog_closed",
              value: null,
              text: null,
              expectedRevision: null,
            },
          }).catch((e) => {
            if (!disposed) setError(message(e));
          });
        }
      },
    );
    return () => {
      disposed = true;
      void stop.then((fn) => fn());
    };
  }, []);
  useEffect(() => {
    let disposed = false;
    let sourceRequest = 0;
    const loadSources = () => {
      const id = ++sourceRequest;
      return call("sources_list")
        .then((value) => {
          if (!disposed && id === sourceRequest) {
            setSources(value);
          }
        })
        .catch((e) => {
          if (!disposed) setError(message(e));
        });
    };
    void loadSources();
    const sourcesStop = listen("watch_sources_updated", () => {
      void loadSources();
    });
    call("watch_progress_get")
      .then((value) => {
        if (!disposed) setProgress(value);
      })
      .catch(() => {});
    void refresh().catch((e) => {
      if (!disposed) setError(message(e));
    });
    const stop = listen("watch_updated", () => {
      if (!disposed) void refresh().catch((e) => setError(message(e)));
    });
    return () => {
      disposed = true;
      ++request.current;
      void stop.then((fn) => fn());
      void sourcesStop.then((fn) => fn());
      void call("watch_control", {
        control: {
          action: "hide",
          value: null,
          text: null,
          expectedRevision: null,
        },
      }).catch(() => {});
      void call("watch_viewport_set", {
        viewport: { x: 0, y: 0, width: 1, height: 1, visible: false },
      }).catch(() => {});
    };
  }, [refresh]);
  useEffect(() => {
    // Follow actual file changes, while allowing a different episode to be
    // selected for the explicit Open action between player updates.
    setSelected(state?.path ? (state.sourceId ?? "") : "");
  }, [state?.path, state?.sourceId, state?.sessionId]);
  useEffect(() => {
    const current = getCurrentWindow();
    let disposed = false;
    const update = async () => {
      const value = await current.isFullscreen();
      if (!disposed) {
        setFullscreen(value);
        onFullscreenChange(value);
        if (value && !fullscreen) video.current?.focus({ preventScroll: true });
      }
    };
    void update();
    const stop = current.onResized(() => {
      void update();
    });
    const key = (event: KeyboardEvent) => {
      if (
        event.defaultPrevented ||
        event.isComposing ||
        collectionOpen ||
        event.ctrlKey ||
        event.altKey ||
        event.metaKey ||
        (event.target instanceof Element &&
          event.target.closest(
            'input, textarea, select, [contenteditable]:not([contenteditable="false"]), [role="textbox"]',
          ))
      )
        return;
      if (fullscreen && state?.loaded && event.code === "Space") {
        event.preventDefault();
        event.stopPropagation();
        if (!event.repeat) void control("toggle");
        return;
      }
      if (event.key === "F11" || (event.key === "Escape" && fullscreen)) {
        event.preventDefault();
        void current
          .setFullscreen(event.key === "Escape" ? false : !fullscreen)
          .then(update);
      }
    };
    document.addEventListener("keydown", key, true);
    return () => {
      disposed = true;
      void stop.then((fn) => fn());
      document.removeEventListener("keydown", key, true);
    };
  }, [fullscreen, collectionOpen, onFullscreenChange, state?.loaded]);
  useEffect(
    () => () => {
      onFullscreenChange(false);
      void getCurrentWindow().setFullscreen(false);
    },
    [onFullscreenChange],
  );
  useEffect(() => {
    const element = video.current;
    if (!element || !state?.sessionId) return;
    let disposed = false,
      frame = 0;
    const update = () => {
      cancelAnimationFrame(frame);
      frame = requestAnimationFrame(() => {
        if (disposed) return;
        const rect = element.getBoundingClientRect();
        const style = getComputedStyle(element);
        const left = parseFloat(style.borderLeftWidth) || 0,
          top = parseFloat(style.borderTopWidth) || 0;
        const right = parseFloat(style.borderRightWidth) || 0,
          bottom = parseFloat(style.borderBottomWidth) || 0;
        void call("watch_viewport_set", {
          viewport: {
            x: rect.left + left,
            y: rect.top + top,
            width: rect.width - left - right,
            height: rect.height - top - bottom,
            // Native window visibility is authoritative. WebView2 can report its
            // own surface occluded when the native fullscreen video covers it.
            visible: !collectionOpen && !!state.path,
          },
        }).catch((e) => setError(message(e)));
      });
    };
    const observer = new ResizeObserver(update);
    observer.observe(element);
    window.addEventListener("resize", update);
    window.addEventListener("scroll", update, true);
    document.addEventListener("visibilitychange", update);
    update();
    return () => {
      disposed = true;
      cancelAnimationFrame(frame);
      observer.disconnect();
      window.removeEventListener("resize", update);
      window.removeEventListener("scroll", update, true);
      document.removeEventListener("visibilitychange", update);
    };
  }, [!!state?.path, collectionOpen]);
  const start = async (path?: string, sourceId?: string) => {
    setBusy(true);
    setError("");
    try {
      setState(
        await call("watch_open", {
          path: path ?? null,
          sourceId: sourceId ?? null,
        }),
      );
    } catch (e) {
      setError(message(e));
    } finally {
      setBusy(false);
    }
  };
  const playlistChange = async (
    action: string,
    index?: number,
    destination?: number,
    paths: string[] = [],
  ) => {
    setBusy(true);
    setError("");
    try {
      setState(
        await call("watch_playlist_update", {
          change: {
            action,
            paths,
            index: index ?? null,
            destination: destination ?? null,
          },
        }),
      );
    } catch (e) {
      setError(message(e));
    } finally {
      setBusy(false);
    }
  };
  const chooseVideo = async (directory = false) => {
    try {
      const path = await open({
        multiple: !directory,
        directory,
        filters: directory
          ? undefined
          : [
              {
                name: "本地视频",
                extensions: [
                  "mkv",
                  "mp4",
                  "avi",
                  "mov",
                  "webm",
                  "m4v",
                  "ts",
                  "mts",
                  "m2ts",
                  "wmv",
                  "flv",
                ],
              },
            ],
      });
      if (path)
        await playlistChange(
          "append",
          undefined,
          undefined,
          typeof path === "string" ? [path] : path,
        );
    } catch (e) {
      setError(message(e));
    }
  };
  const context = state?.context;
  const status = state?.status;
  const statusText =
    status === "counting"
      ? `${Math.ceil(state?.remainingSeconds ?? settings?.watchCollectDelaySeconds ?? 5)} 秒后自动收录整句`
      : status === "saving"
        ? "正在保存当前整句…"
        : status === "saved"
          ? "已自动收录"
          : status === "already_saved"
            ? "本句已收录"
            : status === "missing_translation"
              ? "暂无已保存译文，可先在后台准备中文资料。"
              : status === "no_subtitle"
                ? "此处没有已导入的对白。可先导入字幕资料，或继续观看。"
                : status === "subtitle_changed"
                  ? "字幕轨已切换，暂不使用原导入版本的译文。"
                  : status === "reading" || status === "awaiting_display"
                    ? "正在读取当前句…"
                    : status === "cancelled"
                      ? "本次不自动收录"
                      : "";
  const lookup = async (target: string, mode: "explain" | "translate") => {
    if (!context || !state || !target.trim()) return;
    await control("cancel_auto");
    setQuery({
      target: target.trim(),
      mode,
      nonce: Date.now(),
      revision: context.revision,
      playerSession: state.sessionId,
    });
  };
  const manual = async (target: string, kind: string, meaning?: string) => {
    if (!context || !target.trim()) return;
    await control("cancel_auto");
    let bound = context;
    try {
      bound = await call("watch_collection_prepare", {
        sessionId: state!.sessionId,
        expectedRevision: context.revision,
        clipRange:
          manualClip?.session === state!.sessionId &&
          manualClip.revision === context.revision
            ? manualClip.range
            : null,
      });
    } catch (e) {
      setError(message(e));
      return;
    }
    collect({
      kind,
      text: target.trim(),
      context: context.text,
      meaning: meaning ?? (kind === "sentence" ? context.translation : ""),
      sourceId: bound.sourceId,
      locationKey: bound.locationKey,
      example: bound.preciseTiming
        ? {
            id: bound.exampleId,
            text: bound.text,
            startMs: bound.startMs,
            endMs: bound.endMs,
            decision: null,
            revision: 1,
          }
        : undefined,
    });
  };
  return (
    <section className="watch-view">
      <div className="watch-toolbar">
        <button
          className="primary"
          disabled={busy}
          onClick={() => void chooseVideo().catch((e) => setError(message(e)))}
        >
          {busy ? "正在打开…" : state?.loaded ? "添加视频" : "打开视频"}
        </button>
        {(!!sources.length || !!state?.path) && (
          <>
            <select
              aria-label="已导入剧集"
              value={selected}
              onChange={(e) => setSelected(e.target.value)}
            >
              <option value="">
                {state?.path && !state.sourceId
                  ? `${state.title} · 当前视频`
                  : "选择已导入剧集"}
              </option>
              {state?.sourceId &&
                !sources.some((s) => s.id === state.sourceId) && (
                  <option value={state.sourceId}>{state.title}</option>
                )}
              {sources.map((s) => (
                <option key={s.id} value={s.id}>
                  {s.title}
                </option>
              ))}
            </select>
            <button
              disabled={busy || !selected}
              onClick={() => void start(undefined, selected)}
            >
              打开剧集
            </button>
          </>
        )}
        {!state?.loaded && !!state?.playback.playlist.length && (
          <button
            disabled={busy}
            onClick={() => void playlistChange("restore")}
          >
            继续上次列表
          </button>
        )}
        {!state?.loaded && !state?.playback.playlist.length && progress && (
          <button
            disabled={busy}
            onClick={() =>
              void start(progress.path, progress.sourceId ?? undefined)
            }
          >
            继续上次观看 · {time(progress.position * 1000)}
          </button>
        )}
        <div className="watch-playback-controls">
          <button
            disabled={busy || !state?.path}
            onClick={() => void control("close")}
            title="停止并关闭当前视频，保留播放列表和观看进度"
          >
            ■ 停止播放
          </button>
          <button
            aria-label="上一集"
            disabled={busy || !state?.loaded || (state.playlistIndex ?? 0) <= 0}
            onClick={() => void playlistChange("previous")}
          >
            上一集
          </button>
          <button
            aria-label="下一集"
            disabled={
              busy ||
              !state?.loaded ||
              state.playlistIndex === null ||
              state.playlistIndex >= state.playback.playlist.length - 1
            }
            onClick={() => void playlistChange("next")}
          >
            下一集
          </button>
          <label>
            倍速{" "}
            <select
              aria-label="播放倍速"
              value={state?.playback.preferences.speed ?? 1}
              onChange={(e) => void control("speed", Number(e.target.value))}
            >
              {[
                ...new Set([
                  0.5,
                  0.75,
                  1,
                  1.25,
                  1.5,
                  1.75,
                  2,
                  3,
                  4,
                  state?.playback.preferences.speed ?? 1,
                ]),
              ]
                .sort((a, b) => a - b)
                .map((speed) => (
                  <option key={speed} value={speed}>
                    {Number(speed.toFixed(2))}×
                  </option>
                ))}
            </select>
          </label>
          <label className="check-label">
            <input
              type="checkbox"
              checked={state?.playback.preferences.autoNext ?? true}
              onChange={(e) =>
                void control("auto_next", e.target.checked ? 1 : 0)
              }
            />
            自动连播
          </label>
        </div>
        <button disabled={!state?.loaded} onClick={() => void control("menu")}>
          ⚙ 播放器设置
        </button>
        <button
          className="text-button"
          aria-expanded={options}
          onClick={() => setOptions((v) => !v)}
        >
          学习设置
        </button>
        <button
          onClick={async () => {
            const window = getCurrentWindow();
            await window.setFullscreen(!(await window.isFullscreen()));
          }}
        >
          {fullscreen ? "退出全屏" : "全屏"}
        </button>
      </div>
      <RelatedEpisodes path={state?.path ?? ""} loaded={state?.loaded ?? false} />
      {state?.path && state.importStatus?.message && (
        <p role="status" className="muted">
          {state.importStatus.message}
        </p>
      )}
      {state?.loaded &&
        (state.playback.preferences.loopMode !== "none" ||
          state.loopA !== null) && (
          <small className="watch-loop-status" role="status">
            {state.playback.preferences.loopMode === "file"
              ? "单集循环"
              : state.playback.preferences.loopMode === "playlist"
                ? "列表循环"
                : ""}
            {state.loopA !== null
              ? ` · A–B：${time(state.loopA * 1000)}—${state.loopB === null ? "等待设置 B" : time(state.loopB * 1000)}`
              : ""}
          </small>
        )}
      {state && (
        <WatchPlaylist
          state={state}
          busy={busy}
          change={playlistChange}
          add={chooseVideo}
        />
      )}
      {(error || state?.error) && (
        <p role="alert" className="error">
          {error || state?.error}
        </p>
      )}
      {options && (
        <div className="watch-options">
          <SubtitleSettings
            key={state?.loaded ? (state.sourceId ?? "") : selected}
            sourceId={state?.loaded ? (state.sourceId ?? "") : selected}
            onChanged={async () => {
              await control("refresh_subtitles");
            }}
          />
          <label className="check-label">
            <input
              type="checkbox"
              checked={settings?.watchAutoCollect ?? true}
              disabled={!settings}
              onChange={async (e) => {
                if (!settings) return;
                try {
                  const next = await call("settings_update", {
                    settings: {
                      ...settings,
                      watchAutoCollect: e.target.checked,
                    },
                  });
                  onSettings(next);
                  await control("configure");
                } catch (error) {
                  setError(message(error));
                }
              }}
            />
            译文显示后自动收录整句
          </label>
          <label>
            等待秒数{" "}
            <input
              type="number"
              min="1"
              value={settings?.watchCollectDelaySeconds ?? 5}
              disabled={!settings}
              onChange={(e) =>
                onSettings({
                  ...settings!,
                  watchCollectDelaySeconds: Number(e.target.value),
                })
              }
              onBlur={async () => {
                if (!settings) return;
                try {
                  const next = await call("settings_update", { settings });
                  onSettings(next);
                  await control("configure");
                } catch (error) {
                  setError(message(error));
                }
              }}
            />
          </label>
          <button
            disabled={!state?.loaded}
            onClick={() => void control("subtitle_dialog")}
          >
            加载外置字幕
          </button>
        </div>
      )}
      <div
        className="watch-stage"
        ref={video}
        tabIndex={0}
        aria-label="视频画面，空格播放或暂停"
        onKeyDown={(e) => {
          if (e.code === "Space" && e.target === e.currentTarget) {
            e.preventDefault();
            void control("toggle");
          }
        }}
      >
        {!state?.loaded && (
          <div className="watch-empty">
            <strong>
              {status === "loading" ? "正在打开视频…" : "先看剧，不懂时再暂停"}
            </strong>
            <p>
              {status === "loading"
                ? state?.title
                : "打开或拖入视频、目录；字幕可直接拖到当前画面。"}
            </p>
          </div>
        )}
      </div>
      {state?.loaded && !state.playing && (
        <div className="watch-caption" aria-live="polite">
          <div className="watch-caption-heading">
            <span>
              {statusText ||
                (state.playing
                  ? state.title
                  : "空格播放；观看中暂停可显示当前句译文")}
            </span>
            {status === "counting" && (
              <button
                className="text-button"
                onClick={() => void control("cancel_auto")}
              >
                本次不收录
              </button>
            )}
          </div>
          {state.frame && (
            <WatchOcr
              frame={state.frame}
              control={(action, text) =>
                control(action, undefined, text, state.revision)
              }
            />
          )}
          {!state.playing && context && (
            <>
              <div
                className="watch-sentence"
                onMouseUp={() => {
                  const selection = window.getSelection();
                  const target = selection?.toString().trim();
                  if (
                    target &&
                    selection?.anchorNode &&
                    selection.focusNode &&
                    selection.anchorNode.parentElement?.closest(
                      ".watch-sentence",
                    ) &&
                    selection.focusNode.parentElement?.closest(
                      ".watch-sentence",
                    )
                  )
                    void lookup(target, "explain");
                }}
              >
                {context.text
                  .split(/([A-Za-z]+(?:['’\-][A-Za-z]+)*)/)
                  .map((part, index) =>
                    /^[A-Za-z]/.test(part) ? (
                      <button
                        key={index}
                        className="watch-word"
                        onClick={() => {
                          if (!window.getSelection()?.toString().trim())
                            void lookup(part, "explain");
                        }}
                      >
                        {part}
                      </button>
                    ) : (
                      <span key={index}>{part}</span>
                    ),
                  )}
              </div>
              <p className="watch-translation">
                {context.translation || "暂无已保存译文"}
              </p>
              <div className="watch-caption-actions">
                <small>
                  {context.previous ? "上一句 · " : ""}
                  {context.preciseTiming
                    ? `${time(context.startMs)}—${time(context.endMs)}`
                    : `OCR 采集位置 ${time(context.startMs)} · 无精确原声区间`}
                </small>
                <button
                  className="text-button"
                  onClick={() => void manual(context.text, "sentence")}
                >
                  手动收录整句
                </button>
                {!context.translation && (
                  <button
                    className="text-button"
                    onClick={() => void lookup(context.text, "translate")}
                  >
                    翻译当前句
                  </button>
                )}
              </div>
              {!context.preciseTiming && context.textSource === "frame_ocr" && (
                <WatchClip
                  key={`${state.sessionId}:${context.revision}`}
                  context={context}
                  playerSession={state.sessionId}
                  duration={state.duration}
                  onChange={(range) =>
                    setManualClip(
                      range
                        ? {
                            session: state.sessionId,
                            revision: context.revision,
                            range,
                          }
                        : null,
                    )
                  }
                />
              )}
              {query &&
                state.sessionId === query.playerSession &&
                context.revision === query.revision && (
                  <WatchQuery
                    key={`${query.playerSession}:${query.revision}:${query.nonce}`}
                    request={query}
                    context={context}
                    playerSession={state.sessionId}
                    collect={(target, meaning) =>
                      void manual(
                        target,
                        query.mode === "translate"
                          ? "sentence"
                          : target.includes(" ")
                            ? "phrase"
                            : "word",
                        meaning,
                      )
                    }
                  />
                )}
            </>
          )}
        </div>
      )}
    </section>
  );
}
