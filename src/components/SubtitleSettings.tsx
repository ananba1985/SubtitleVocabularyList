import { useEffect, useState } from "react";
import { call, message, uid, waitTask } from "../api";
import type { SubtitleInfo } from "../generated/ipc";

export function SubtitleSettings({
  sourceId,
  onChanged,
}: {
  sourceId: string;
  onChanged: () => Promise<void>;
}) {
  const [info, setInfo] = useState<SubtitleInfo | null>(null),
    [busy, setBusy] = useState(false),
    [error, setError] = useState(""),
    [progress, setProgress] = useState("");
  useEffect(() => {
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
      setInfo(await waitTask(task, (t) => setProgress(t.message)));
      await onChanged();
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
                setInfo(
                  await call("subtitle_chinese_select", {
                    sourceId,
                    assetId: e.target.value || null,
                  }),
                );
                await onChanged();
              } catch (error) {
                setError(message(error));
              } finally {
                setBusy(false);
              }
            }}
          >
            <option value="">使用原有译文缓存</option>
            {info.assets
              .filter((a) => a.language === "zh" || a.language === "en-zh")
              .map((a) => (
                <option key={a.id} value={a.id}>
                  {a.title || a.language} ·{" "}
                  {a.origin === "embedded" ? "内置" : "外置"} · {a.cueCount} 条
                </option>
              ))}
          </select>
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
    </div>
  );
}
