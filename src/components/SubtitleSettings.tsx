import { useEffect, useRef, useState } from "react";
import { call, message, uid, waitTask } from "../api";
import type { SubtitleInfo } from "../generated/ipc";
import { OnlineSubtitlePanel } from "./OnlineSubtitlePanel";

export function SubtitleSettings({
  sourceId,
  onChanged,
  playback = true,
}: {
  sourceId: string;
  onChanged: () => Promise<void>;
  playback?: boolean;
}) {
  const [info, setInfo] = useState<SubtitleInfo | null>(null),
    [busy, setBusy] = useState(false),
    [error, setError] = useState(""),
    [progress, setProgress] = useState("");
  const currentSource = useRef(sourceId),
    alive = useRef(true);
  currentSource.current = sourceId;
  const apply = async (value: SubtitleInfo) => {
    if (!alive.current || currentSource.current !== sourceId) return;
    setInfo(value);
    await onChanged();
  };
  useEffect(() => {
    alive.current = true;
    let disposed = false;
    setInfo(null);
    setError("");
    if (sourceId)
      void call("subtitle_info_get", { sourceId })
        .then((value) => {
          if (!disposed) setInfo(value);
        })
        .catch((e) => {
          if (!disposed) setError(message(e));
        });
    return () => {
      disposed = true;
      alive.current = false;
    };
  }, [sourceId]);
  async function prepare() {
    setBusy(true);
    setError("");
    setProgress("正在检查本地字幕…");
    try {
      const task = await call("subtitle_prepare_start", {
        sourceId,
        operationId: uid(),
      });
      await apply(await waitTask(task, (t) => setProgress(t.message)));
    } catch (e) {
      setError(message(e));
    } finally {
      setBusy(false);
      setProgress("");
    }
  }
  return (
    <div className="watch-subtitles">
      <button disabled={!sourceId || busy} onClick={() => void prepare()}>
        {busy ? "正在取得字幕…" : "补充本地中英字幕"}
      </button>
      {info && (
        <label>
          暂停时优先使用的中文
          <select
            value={info.selectedChineseId ?? ""}
            disabled={busy}
            onChange={async (e) => {
              setBusy(true);
              setError("");
              try {
                await apply(
                  await call("subtitle_chinese_select", {
                    sourceId,
                    assetId: e.target.value || null,
                  }),
                );
              } catch (error) {
                setError(message(error));
              } finally {
                setBusy(false);
              }
            }}
          >
            <option value="">使用原有译文缓存</option>
            {info.assets
              .filter(
                (a) =>
                  a.cueCount > 0 &&
                  (a.language === "zh" || a.language === "en-zh"),
              )
              .map((a) => (
                <option key={a.id} value={a.id}>
                  {a.title || a.language} ·{" "}
                  {a.origin === "embedded"
                    ? "内置"
                    : a.origin === "online"
                      ? "在线"
                      : "外置"}{" "}
                  · {a.cueCount} 条
                </option>
              ))}
          </select>
        </label>
      )}
      {info?.selectedChineseId && (
        <label>
          中文字幕偏移（秒）
          <input
            key={`${sourceId}:${info.selectedChineseId}:${info.assets.find((a) => a.id === info.selectedChineseId)?.timeOffsetMs ?? 0}`}
            type="number"
            step={0.1}
            defaultValue={
              (info.assets.find((a) => a.id === info.selectedChineseId)
                ?.timeOffsetMs ?? 0) / 1000
            }
            disabled={busy}
            onBlur={async (e) => {
              const offsetMs = Math.round(Number(e.target.value) * 1000);
              if (!Number.isFinite(offsetMs)) return;
              if (
                offsetMs ===
                (info.assets.find((a) => a.id === info.selectedChineseId)
                  ?.timeOffsetMs ?? 0)
              )
                return;
              setBusy(true);
              setError("");
              try {
                await apply(
                  await call("subtitle_alignment_set", {
                    sourceId,
                    assetId: info.selectedChineseId!,
                    offsetMs,
                  }),
                );
              } catch (error) {
                setError(message(error));
              } finally {
                setBusy(false);
              }
            }}
          />
          <small>正数延后中文字幕，不改变原始字幕文件。</small>
        </label>
      )}
      {progress && <small role="status">{progress}</small>}
      {error && (
        <p role="alert" className="error">
          {error}
        </p>
      )}
      {info && (
        <details>
          <summary>已保存字幕（{info.assets.length}）</summary>
          <ul>
            {info.assets.map((a) => (
              <li key={a.id}>
                {a.title || "未标注语言"} · {a.language} · {a.format} ·{" "}
                {a.cueCount} 条
                {a.provenance && (
                  <small>
                    {" "}
                    · {a.provenance.provider} · {a.provenance.releaseName} ·
                    已核对版本
                  </small>
                )}
                {playback &&
                  a.cueCount > 0 &&
                  (a.language === "en" || a.language === "en-zh") && (
                    <button
                      className="text-button"
                      onClick={async () => {
                        setBusy(true);
                        setError("");
                        try {
                          await call("watch_control", {
                            control: {
                              action: "subtitle_asset",
                              text: a.id,
                              value: null,
                              expectedRevision: null,
                            },
                          });
                        } catch (error) {
                          setError(message(error));
                        } finally {
                          setBusy(false);
                        }
                      }}
                      disabled={busy}
                    >
                      用于播放器
                    </button>
                  )}
              </li>
            ))}
          </ul>
          {!info.selectedChineseId && (
            <p className="muted">
              当前没有优先使用的中文字幕；已有译文和正常观看仍可用。
            </p>
          )}
          {info.warnings.map((warning, index) => (
            <p key={index} className="muted">
              {warning}
            </p>
          ))}
        </details>
      )}
      <OnlineSubtitlePanel
        key={sourceId}
        sourceId={sourceId}
        onImported={apply}
      />
    </div>
  );
}
