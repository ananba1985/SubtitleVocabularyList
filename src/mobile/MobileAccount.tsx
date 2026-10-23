import { useEffect, useState } from "react";
import { call, message } from "../api";
import type { DeviceSyncState } from "../generated/ipc";
import type { Settings } from "../types";
import { SyncView } from "../components/SyncView";

export function MobileAccount({ settings, refreshKey, changed, requestSync, autoNotice }: {
  settings: Settings; refreshKey: number; changed: () => void; requestSync: () => void; autoNotice: string;
}) {
  const [state, setState] = useState<DeviceSyncState | null>(null);
  const [site, setSite] = useState(settings.siteUrl);
  const [deviceName, setDeviceName] = useState("");
  const [confirmed, setConfirmed] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  useEffect(() => { setSite(settings.siteUrl); }, [settings.siteUrl]);
  useEffect(() => {
    let disposed = false;
    call("device_sync_get").then((next) => {
      if (disposed) return;
      setState(next); setDeviceName(next.preferences.deviceName); setConfirmed(false);
    }).catch((e) => { if (!disposed) setError(message(e)); });
    return () => { disposed = true; };
  }, [refreshKey]);
  async function run(action: () => Promise<unknown>) {
    setBusy(true); setError("");
    try { await action(); changed(); }
    catch (e) { setError(message(e)); }
    finally { setBusy(false); }
  }
  return <>
    <section className="mobile-card">
      <h2>账号与同步设置</h2>
      {error && <p role="alert" className="error">{error}</p>}
      <label>站点地址<input type="url" value={site} onChange={(e) => setSite(e.target.value)} /></label>
      <label>手机设备名<input value={deviceName} onChange={(e) => setDeviceName(e.target.value)} maxLength={80} /></label>
      <button disabled={busy || !state} onClick={() => run(async () => {
        if (!state) return;
        await call("settings_update", { settings: { ...settings, siteUrl: site } });
        await call("device_sync_update", { input: { ...state.preferences, deviceName } });
      })}>保存连接设置</button>
      <p className="muted">设备名用于下一次授权。手机和电脑各自连接此站点。</p>
      {state && <>
        <label className="check-label"><input type="checkbox" checked={state.preferences.autoSync} disabled={busy}
          onChange={(e) => { const autoSync = e.target.checked; void run(async () => {
            const preferences = await call("device_sync_update", { input: { ...state.preferences, autoSync } });
            setState({ ...state, preferences }); if (autoSync) requestSync();
          }); }} />自动同步（回到前台、保存后）</label>
        <p className="muted">原声等待 Wi-Fi 等不计费网络。移动网络遇到需要传输的原声时，会保留待同步进度。</p>
        {state.network.connected && state.network.metered && <p className="notice">当前网络计费，原声传输等待 Wi-Fi。</p>}
        {!state.network.connected && <p className="notice">当前无可用网络，本地学习照常保存。</p>}
        <p>上次完整交换：{state.lastCompletedAt ? new Date(state.lastCompletedAt).toLocaleString() : "尚未完成"}</p>
        <p>本机原声：{state.readyAudio} 份已准备 · {state.missingAudio} 份待准备</p>
        {state.bindingRequired && <div className="suggestion">
          <h3>确认本机资料归属</h3>
          {state.boundAccount && <p className="muted">本机资料此前关联：{state.boundAccount}</p>}
          <p>当前获批账号：{state.currentAccount}</p>
          <p>同步会将手机现有的全部词条、原声和学习历史加入此账号，也会下载此账号资料。</p>
          <label className="check-label"><input type="checkbox" checked={confirmed} onChange={(e) => setConfirmed(e.target.checked)} />我确认将本机全部学习资料用于此账号</label>
          <button disabled={!confirmed || busy} onClick={() => run(async () => {
            if (!state.currentAccount) return;
            await call("device_sync_bind", { expectedAccount: state.currentAccount });
            requestSync();
          })}>确认资料归属</button>
        </div>}
        <button className="secondary" disabled={busy} onClick={() => run(() => call("connection_disconnect"))}>断开本机账号连接</button>
        <p className="muted">断开后保留手机资料；如需撤销已授权设备，请在站点连接页操作。</p>
      </>}
      {autoNotice && <p role="status" className="notice">{autoNotice}</p>}
    </section>
    <SyncView refreshKey={refreshKey} onChanged={changed} />
  </>;
}
