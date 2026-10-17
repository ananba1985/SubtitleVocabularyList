import { useCallback, useEffect, useState } from "react";
import { call, message, uid, waitTask } from "../api";
import type {
  AssessmentSyncStatus,
  AssessmentSyncConflict,
} from "../assessmentTypes";
import { assessmentStateNames } from "../assessmentTypes";
import type {
  ConnectionStatus,
  SyncStatus,
  SyncConflict,
  SyncResult,
  SyncBundle,
} from "../types";

const conflictFields: Record<string, string> = {
  "entry.text": "词条文字",
  "entry.kind": "词条类型",
  "meanings.text": "释义",
  "examples.text": "原句",
  "examples.start_ms": "原声开始位置",
  "examples.end_ms": "原声结束位置",
  "sources.title": "来源名称",
};

export function SyncView({ onChanged }: { onChanged: () => void }) {
  const [connection, setConnection] = useState<ConnectionStatus | null>(null),
    [busy, setBusy] = useState(false),
    [error, setError] = useState("");
  const [status, setStatus] = useState<SyncStatus | null>(null),
    [conflicts, setConflicts] = useState<SyncConflict[]>([]),
    [targets, setTargets] = useState<Record<string, string>>({}),
    [taskId, setTaskId] = useState<string | null>(null),
    [notice, setNotice] = useState("");
  const [assessment, setAssessment] = useState<AssessmentSyncStatus | null>(
    null,
  );
  const [assessmentConflicts, setAssessmentConflicts] = useState<
    AssessmentSyncConflict[]
  >([]);
  const refresh = useCallback(async () => {
    const [next, progress, pending, assessmentStatus, assessmentPending] =
      await Promise.all([
        call("connection_status"),
        call("sync_status"),
        call("sync_conflicts"),
        call("assessment_sync_status"),
        call("assessment_sync_conflicts"),
      ]);
    setConnection(next);
    setStatus(progress);
    setConflicts(pending);
    setAssessment(assessmentStatus);
    setAssessmentConflicts(assessmentPending);
  }, []);
  useEffect(() => {
    refresh().catch((e) => setError(message(e)));
  }, [refresh]);
  async function run(
    command: "connection_start" | "connection_check",
    replace = false,
  ) {
    setBusy(true);
    setError("");
    try {
      const operationId = uid();
      const task =
        command === "connection_start"
          ? await call(command, { operationId, replace })
          : await call(command, { operationId });
      setTaskId(task.id);
      await waitTask<ConnectionStatus>(task);
      await refresh();
    } catch (e) {
      setError(message(e));
    } finally {
      setBusy(false);
      setTaskId(null);
    }
  }
  async function synchronize() {
    setBusy(true);
    setError("");
    setNotice("");
    try {
      const task = await call("sync_start", {
        operationId: uid(),
      });
      setTaskId(task.id);
      const result = await waitTask<SyncResult>(task);
      const a = result.assessment;
      setNotice(
        `已拉取 ${result.pulled} 次词条变更，发送 ${result.pushed} 份词条资料。${result.conflicts ? `还有 ${result.conflicts} 项词条需要确认。` : ""}${a ? (a.capability !== "assessment-1" && a.pending ? "水平测试记录仍待站点支持。" : `水平测试：${a.pending} 次待发送，${a.conflicts} 次冲突，${a.pendingReferences} 处词条关联待同步。`) : ""}`,
      );
      onChanged();
    } catch (e) {
      setError(message(e));
    } finally {
      setTaskId(null);
      setBusy(false);
      await refresh().catch((e) => setError(message(e)));
      onChanged();
    }
  }
  async function resolve(conflict: SyncConflict, choice: string) {
    setBusy(true);
    setError("");
    try {
      const task = await call("sync_resolve", {
        input: {
          conflictId: conflict.id,
          choice,
          targetEntryId:
            choice === "merge" ? (targets[conflict.id] ?? null) : null,
          expectedRemoteRevision: conflict.remoteRevision,
          expectedLocalRevision: conflict.local?.entry.revision ?? null,
          expectedTargetRevision:
            conflict.candidates.find((c) => c.id === targets[conflict.id])
              ?.revision ?? null,
        },
        operationId: uid(),
      });
      setTaskId(task.id);
      await waitTask(task);
      await refresh();
      setNotice("选择已保存。再次同步即可发送合并后的资料。");
      onChanged();
    } catch (e) {
      setError(message(e));
    } finally {
      setTaskId(null);
      setBusy(false);
    }
  }
  async function keepAssessmentBoth(conflict: AssessmentSyncConflict) {
    setBusy(true);
    setError("");
    try {
      await call("assessment_sync_keep_both", {
        input: {
          sessionId: conflict.sessionId,
          expectedLocalRevision: conflict.localRevision,
          expectedRemoteRevision: conflict.remoteRevision,
          operationId: uid(),
        },
      });
      await refresh();
      onChanged();
      setNotice(
        "已保留两份测试记录，本地副本已标记。再次同步即可发送副本；个人词库和收录次数保持不变。",
      );
    } catch (e) {
      setError(message(e));
    } finally {
      setBusy(false);
    }
  }
  function summary(bundle: SyncBundle | null) {
    if (!bundle) return <p className="muted">尚未建立本地关联</p>;
    return (
      <>
        <strong>{bundle.entry.text}</strong>
        <ul>
          {bundle.tables.meanings.map((m) => (
            <li key={String(m.id)}>{String(m.text)}</li>
          ))}
        </ul>
        <p className="muted">
          {bundle.tables.examples.length} 条例句 ·{" "}
          {bundle.tables.media_assets.length} 份原声 ·{" "}
          {bundle.tables.collection_actions.length} 次收录 ·{" "}
          {bundle.tables.review_attempts.length} 条作答
        </p>
        {bundle.tables.examples.slice(0, 3).map((e) => (
          <p key={String(e.id)}>{String(e.text)}</p>
        ))}
      </>
    );
  }
  const stateNames: Record<string, string> = {
    disconnected: "尚未连接",
    pending: "等待浏览器批准",
    connected: "已保存账号连接",
    expired: "连接已过期",
    rejected: "请求已拒绝",
    revoked: "访问已撤销",
  };
  return (
    <section className="settings-panel">
      <h2>English Practice</h2>
      <p>
        连接后可交换词条、释义、例句、原声、学习记录及完整水平测试历史。剧集文件、未收录对白、截图和系统语音缓存不会上传。
      </p>
      {error && (
        <p className="error" role="alert">
          {error}
        </p>
      )}
      {notice && (
        <p role="status" className="notice banner">
          {notice}
        </p>
      )}
      {connection && (
        <>
          <p className="directory">{connection.siteUrl}</p>
          <p role="status">
            {stateNames[connection.state] ?? connection.state}
          </p>
          {connection.offline && (
            <p className="notice banner">
              已启用手动离线。请在“本地设置”取消手动离线并保存，再发起连接；已有词库和测验仍可使用。
            </p>
          )}
          {connection.deviceName && (
            <p className="muted">
              连接设备：{connection.deviceName}。可检查账号连接是否仍有效。
            </p>
          )}
          {connection.displayCode && (
            <div className="suggestion">
              <h3>核对连接码</h3>
              <p className="connection-code">{connection.displayCode}</p>
              <p>浏览器中登录需要同步的账号，并核对设备名和连接码后批准。</p>
            </div>
          )}
          <div className="actions">
            <button
              className="primary"
              disabled={
                busy || connection.offline || connection.state !== "connected"
              }
              onClick={synchronize}
            >
              立即同步全部资料
            </button>
            {taskId && (
              <button
                onClick={() =>
                  call("task_cancel", { taskId }).catch((e) =>
                    setError(message(e)),
                  )
                }
              >
                取消当前任务
              </button>
            )}
            <button
              className="primary"
              disabled={busy || connection.offline}
              onClick={() =>
                run("connection_start", connection.state === "connected")
              }
            >
              {connection.state === "connected"
                ? "重新连接账号"
                : "发起 / 重试连接"}
            </button>
            {connection.requestId && (
              <button
                disabled={busy || connection.offline}
                onClick={() =>
                  call("connection_open").catch((e) => setError(message(e)))
                }
              >
                打开浏览器连接页
              </button>
            )}
            {connection.requestId && (
              <button
                disabled={busy || connection.offline}
                onClick={() => run("connection_check")}
              >
                已批准，检查连接
              </button>
            )}
            <button
              disabled={busy}
              onClick={() => refresh().catch((e) => setError(message(e)))}
            >
              刷新本机状态
            </button>
          </div>
          {status && (
            <p className="muted">
              {status.pending} 个词条待同步 · {status.conflicts} 项待确认
            </p>
          )}
          {assessment && (
            <p className="muted">
              水平测试：{assessment.pending} 次待发送 · {assessment.conflicts}{" "}
              次冲突 · {assessment.pendingReferences} 处词条关联待完成
              {assessment.pending > 0 &&
              assessment.capability !== "assessment-1"
                ? "；站点能力待检查或尚未支持，测试记录仍保留在本机。"
                : ""}
            </p>
          )}
          {assessmentConflicts.map((conflict) => (
            <article className="suggestion" key={conflict.sessionId}>
              <h3>水平测试记录需要处理</h3>
              <p>{conflict.reason}</p>
              <div className="sync-comparison">
                {(
                  [
                    ["本地记录", conflict.local],
                    ["网站记录", conflict.remote],
                  ] as const
                ).map(([title, side]) => (
                  <section key={title}>
                    <h4>{title}</h4>
                    <p>
                      {assessmentStateNames[side.state] ?? side.state} · 已答{" "}
                      {side.answers}/{side.planned} 题
                    </p>
                    <p className="muted">
                      {side.corrections} 条修正 · {side.reports} 份报告
                    </p>
                  </section>
                ))}
              </div>
              <details>
                <summary>如何保留记录</summary>
                <p>
                  网站记录保留原会话编号，本地内容另存为已标记副本。原始两份资料完整存档；副本不作为新的独立复测，也不会重复收录词条。
                </p>
              </details>
              <button
                disabled={busy}
                onClick={() => void keepAssessmentBoth(conflict)}
              >
                保留两份
              </button>
            </article>
          ))}
          {conflicts.map((conflict) => (
            <article className="suggestion" key={conflict.id}>
              <h3>{conflict.text}</h3>
              <p>
                {conflict.reason.replace(
                  /(entry|meanings|examples|sources)\.[a-z_]+/g,
                  (field) => conflictFields[field] ?? "同一项资料",
                )}
              </p>
              <div className="sync-comparison">
                <section>
                  <h4>本地资料</h4>
                  {summary(conflict.local)}
                </section>
                <section>
                  <h4>网站资料{conflict.remote.deleted ? "（已移除）" : ""}</h4>
                  {summary(conflict.remote)}
                </section>
              </div>
              <div className="actions">
                {conflict.remote.deleted ? (
                  <>
                    <button
                      disabled={busy || connection.offline}
                      onClick={() => resolve(conflict, "archive")}
                    >
                      本地归档，保留全部资料
                    </button>
                    <button
                      disabled={busy || connection.offline}
                      onClick={() => resolve(conflict, "restore")}
                    >
                      恢复并保留本地资料
                    </button>
                  </>
                ) : conflict.localId ? (
                  <>
                    <button
                      disabled={busy || connection.offline}
                      onClick={() => resolve(conflict, "local")}
                    >
                      保留本地文字，合并新增资料
                    </button>
                    <button
                      disabled={busy || connection.offline}
                      onClick={() => resolve(conflict, "remote")}
                    >
                      采用网站文字，合并新增资料
                    </button>
                  </>
                ) : (
                  <>
                    <select
                      aria-label={`为 ${conflict.text} 选择本地词条`}
                      value={targets[conflict.id] ?? ""}
                      onChange={(e) =>
                        setTargets({
                          ...targets,
                          [conflict.id]: e.target.value,
                        })
                      }
                    >
                      <option value="">选择本地词条</option>
                      {conflict.candidates.map((e) => (
                        <option key={e.id} value={e.id}>
                          {e.text} · {e.meanings.map((m) => m.text).join("；")}
                        </option>
                      ))}
                    </select>
                    <button
                      disabled={
                        busy || connection.offline || !targets[conflict.id]
                      }
                      onClick={() => resolve(conflict, "merge")}
                    >
                      合并到选中词条
                    </button>
                    <button
                      disabled={busy || connection.offline}
                      onClick={() => resolve(conflict, "new")}
                    >
                      独立保存
                    </button>
                  </>
                )}
              </div>
            </article>
          ))}
          <p className="muted">
            连接凭据由 Windows
            当前用户保护，保存于独立位置。可以在站点连接页撤销设备访问。
          </p>
          <p className="muted">
            浏览器批准后返回此页检查连接；所有同步都由你主动发起。
          </p>
        </>
      )}
    </section>
  );
}
