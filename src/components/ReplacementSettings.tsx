import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { call, message } from "../api";
import type { NativeStatus, Settings } from "../types";
import type { Provider, ReplacementStatus } from "../queryTypes";

const labels: Record<string, string> = { capturing: "读取选句", translating: "正在翻译", validating: "核对选区", applying: "正在替换", replaced: "已替换", result_only: "译文可查看", unverified: "请核对原输入", cancelled: "已取消", failed: "失败" };
export function ReplacementSettings({ settings, native, onChange }: { settings: Settings; native: NativeStatus | null; onChange: (settings: Settings) => void }) {
  const [providers, setProviders] = useState<Provider[]>([]);
  const [status, setStatus] = useState<ReplacementStatus | null>(null);
  const [error, setError] = useState("");
  useEffect(() => {
    let disposed = false;
    const refresh = () => call("replacement_status_get").then(v => { if (!disposed) setStatus(v); }).catch(e => { if (!disposed) setError(message(e)); });
    call("providers_list").then(v => { if (!disposed) setProviders(v); }).catch(e => { if (!disposed) setError(message(e)); });
    refresh();
    const stop = listen("replacement_updated", refresh);
    return () => { disposed = true; stop.then(f => f()); };
  }, []);
  const available = providers.filter(p => p.enabled && (p.kind === "qwen" || p.kind === "translation" || p.manifest?.capabilities.includes("translation")));
  const perform = async (
    command:
      | "replacement_cancel"
      | "replacement_result_open"
      | "replacement_clipboard_restore",
  ) => {
    try {
      if (command === "replacement_cancel") {
        if (!status?.operationId) return;
        await call(command, { operationId: status.operationId });
      } else {
        await call(command);
      }
      setStatus(await call("replacement_status_get"));
      setError("");
    } catch (e) {
      setError(message(e));
    }
  };
  return <>
    <label>选句翻译并替换快捷键
      <input value={settings.replacementShortcut} onChange={e => onChange({ ...settings, replacementShortcut: e.target.value })} />
    </label>
    <p className="muted">{native?.replacementRegistered ? "替换快捷键已注册" : "替换快捷键未启用"}。在可编辑输入框选中英文后触发；再次按下可取消等待。留空可禁用。</p>
    <label>翻译替换服务
      <select value={settings.replacementProviderId} onChange={e => onChange({ ...settings, replacementProviderId: e.target.value })}>
        {!available.some(p => p.id === settings.replacementProviderId) && <option value={settings.replacementProviderId}>当前服务未启用：{settings.replacementProviderId}</option>}
        {available.map(p => <option key={p.id} value={p.id}>{p.name}{p.networkScope === "external" ? "（联网）" : ""}</option>)}
      </select>
    </label>
    <p className="muted">默认使用本地模型。译文仅替换选句，不会发送消息或自动收录；窗口或选区变化时只保留译文。进度可在托盘查看。</p>
    {status?.operationId && <div>
      <p>上次翻译替换：{labels[status.state] ?? status.state}</p>
      {status.error && <p role="status">{status.error}</p>}
      <button type="button" className="secondary" onClick={() => perform("replacement_result_open")}>查看译文</button>
      {["capturing", "translating", "validating"].includes(status.state) && <button type="button" className="secondary" onClick={() => perform("replacement_cancel")}>取消替换</button>}
      {status.clipboardPending && <button type="button" className="secondary" onClick={() => perform("replacement_clipboard_restore")}>已核对输入，恢复剪贴板</button>}
    </div>}
    {error && <p role="alert">{error}</p>}
  </>;
}
