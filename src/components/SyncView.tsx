import { useCallback, useEffect, useState } from "react";
import { call, message, uid, waitTask } from "../api";
import type { ConnectionStatus, TaskSnapshot } from "../types";

export function SyncView() {
  const [connection, setConnection] = useState<ConnectionStatus | null>(null),
    [busy, setBusy] = useState(false),
    [error, setError] = useState("");
  const refresh = useCallback(
    async () =>
      setConnection(await call<ConnectionStatus>("connection_status")),
    [],
  );
  useEffect(() => {
    refresh().catch((e) => setError(message(e)));
  }, [refresh]);
  async function run(command: string, replace = false) {
    setBusy(true);
    setError("");
    try {
      const task = await call<TaskSnapshot>(command, {
        operationId: uid(),
        replace,
      });
      setConnection(await waitTask<ConnectionStatus>(task));
    } catch (e) {
      setError(message(e));
    } finally {
      setBusy(false);
    }
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
        连接后可交换词条、释义、例句、原声和学习记录。剧集文件、未收录对白、截图和系统语音缓存不会上传。
      </p>
      {error && (
        <p className="error" role="alert">
          {error}
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
              当前为离线模式。请在“本地设置”取消离线模式并保存，再发起连接；已有词库和测验仍可使用。
            </p>
          )}
          {connection.accountScope && (
            <p className="muted">
              账号范围：{connection.accountScope} ·
              本页状态来自本机保存，线上认证可单独检查。
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
