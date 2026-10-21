import { useEffect, useState } from "react";
import { call, message } from "../api";

export function StartupSettings() {
  const [enabled, setEnabled] = useState<boolean | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  useEffect(() => {
    let disposed = false;
    call("autostart_get")
      .then((value) => {
        if (!disposed) setEnabled(value);
      })
      .catch((error) => {
        if (!disposed) setError(message(error));
      });
    return () => {
      disposed = true;
    };
  }, []);
  async function change(value: boolean) {
    setBusy(true);
    setError("");
    try {
      setEnabled(await call("autostart_set", { enabled: value }));
    } catch (error) {
      setError(message(error));
    } finally {
      setBusy(false);
    }
  }
  return (
    <>
      <label className="check-label">
        <input
          type="checkbox"
          checked={enabled === true}
          disabled={busy || enabled === null}
          onChange={(event) => void change(event.target.checked)}
        />
        开机启动（登录 Windows 后）
      </label>
      <p className="muted">
        {busy
          ? "正在更新启动设置…"
          : enabled === null && !error
            ? "正在读取启动设置…"
            : "修改后立即生效，无需点击保存设置。仅对当前 Windows 用户生效，启动后正常显示主窗口。移动程序或资料目录后，请关闭此开关再重新开启。"}
      </p>
      {error && (
        <p className="error" role="alert">
          {error}
        </p>
      )}
    </>
  );
}
