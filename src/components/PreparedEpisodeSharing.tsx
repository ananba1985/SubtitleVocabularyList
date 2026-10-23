import { useEffect, useRef, useState } from "react";
import { call, message, uid, waitTask } from "../api";
import { useRuntimeFeatures } from "../runtimeFeatures";
import type {
  PreparedEpisodeLocal,
  PreparedEpisodeMeta,
  TaskSnapshot,
} from "../generated/ipc";

export function PreparedEpisodeSharing({
  sourceId,
  onApplied,
}: {
  sourceId: string;
  onApplied?: () => Promise<void>;
}) {
  const { mobile } = useRuntimeFeatures();
  const [local, setLocal] = useState<PreparedEpisodeLocal | null>(null),
    [remote, setRemote] = useState<PreparedEpisodeMeta | null>(null);
  const [checked, setChecked] = useState(false),
    [busy, setBusy] = useState(false),
    [notice, setNotice] = useState(""),
    [error, setError] = useState("");
  const [task, setTask] = useState<TaskSnapshot | null>(null);
  const generation = useRef(0);
  useEffect(() => {
    const epoch = ++generation.current;
    setLocal(null);
    setRemote(null);
    setChecked(false);
    setError("");
    if (sourceId)
      call("prepared_episode_local", { sourceId })
        .then((value) => {
          if (generation.current === epoch) setLocal(value);
        })
        .catch((e) => {
          if (generation.current === epoch) setError(message(e));
        });
    return () => {
      generation.current++;
    };
  }, [sourceId]);
  async function run(action: "check" | "publish" | "download") {
    if (!local) return;
    const epoch = generation.current;
    setBusy(true);
    setError("");
    setNotice("");
    try {
      const operationId = uid();
      const progress = (value: TaskSnapshot) => {
        if (generation.current === epoch) setTask(value);
      };
      if (action === "check") {
        const started = await call("prepared_episode_check", {
          sourceId,
          operationId,
        });
        progress(started);
        const value = await waitTask(started, progress);
        if (generation.current === epoch) {
          setRemote(value);
          setChecked(true);
          setNotice(
            value
              ? "已取得当前账号的服务器版本。"
              : "当前账号尚无此视频的准备资料。",
          );
        }
      } else if (action === "publish") {
        const started = await call("prepared_episode_publish", {
          sourceId,
          expectedRevision: remote?.revision ?? 0,
          operationId,
        });
        progress(started);
        const value = await waitTask(started, progress);
        if (generation.current === epoch) {
          setRemote(value);
          setChecked(true);
          setNotice("已发布完整准备资料。电脑退出后，同账号手机仍可下载。");
          setLocal(await call("prepared_episode_local", { sourceId }));
        }
      } else {
        if (!remote) return;
        const started = await call("prepared_episode_download", {
          sourceId,
          expectedRevision: remote.revision,
          operationId,
        });
        progress(started);
        const value = await waitTask(started, progress);
        if (generation.current === epoch) {
          setRemote(value.metadata);
          setNotice("已匹配本地视频并保存准备资料，可离线使用。");
          await onApplied?.();
        }
      }
    } catch (e) {
      if (generation.current === epoch) setError(message(e));
    } finally {
      if (generation.current === epoch) {
        setBusy(false);
        setTask(null);
      }
    }
  }
  if (!sourceId) return null;
  return (
    <details className="mobile-learning-options prepared-episode-sharing">
      <summary>同账号剧集准备资料</summary>
      <p className="muted">
        共享当前视频的字幕、已准备译文和词语解释；视频需在手机单独选择。按视频内容匹配，手机不直连电脑。
      </p>
      {local && (
        <p>
          本地视频：{local.title}
          {local.applied
            ? ` · ${mobile ? "已使用" : "最近发布"} v${local.applied.revision}`
            : ""}
        </p>
      )}
      {remote && (
        <p>
          服务器 v{remote.revision} · 已译 {remote.translatedCount}/
          {remote.cueCount} 句 · {remote.explanationCount} 项词语解释 · 约{" "}
          {(remote.bytes / 1024).toFixed(1)} KiB
        </p>
      )}
      <div className="actions">
        <button disabled={busy || !local} onClick={() => run("check")}>
          检查同账号准备资料
        </button>
        {mobile ? (
          <button
            disabled={busy || !checked || !remote}
            onClick={() => run("download")}
          >
            下载并使用此版本
          </button>
        ) : (
          <button
            disabled={busy || !checked || !local?.cueCount}
            onClick={() => run("publish")}
          >
            {remote ? "用本机准备资料发布新版本" : "发布本机准备资料"}
          </button>
        )}
        {busy && task && (
          <button
            onClick={() =>
              void call("task_cancel", { taskId: task.id }).catch((e) =>
                setError(message(e)),
              )
            }
          >
            取消资料任务
          </button>
        )}
      </div>
      {!mobile && remote && (
        <p className="muted">
          发布将以本机现有准备资料更新服务器 v{remote.revision}
          ；不会重新调用模型，也不会修改个人学习记录。
        </p>
      )}
      {busy && <p role="status">{task?.message ?? "正在处理准备资料…"}</p>}
      {notice && (
        <p role="status" className="notice">
          {notice}
        </p>
      )}
      {error && (
        <p role="alert" className="error">
          {error}
        </p>
      )}
    </details>
  );
}
