import { useState } from "react";
import { call, message } from "../api";
import type { Settings, SystemVoice } from "../types";

export function MobileSpeechSettings({ settings, saved }: { settings: Settings; saved: (value: Settings) => void }) {
  const [voices, setVoices] = useState<SystemVoice[] | null>(null);
  const [busy, setBusy] = useState(false), [error, setError] = useState("");
  async function check() {
    setBusy(true); setError("");
    try { setVoices(await call("speech_voices")); } catch (e) { setError(message(e)); }
    finally { setBusy(false); }
  }
  async function select(systemVoice: string) {
    setBusy(true); setError("");
    try { saved(await call("settings_update", { settings: { ...settings, systemVoice } })); }
    catch (e) { setError(message(e)); } finally { setBusy(false); }
  }
  return <section className="mobile-card"><h2>离线英语声音</h2>
    <p className="muted">原声优先；没有原声时使用本机已安装的英语声音。生成语音与原声分别标记。</p>
    <button disabled={busy} onClick={check}>{busy ? "正在读取…" : "检查离线英语声音"}</button>
    <button onClick={() => call("speech_settings_open").catch((e) => setError(message(e)))}>打开系统语音设置</button>
    {voices && (voices.length ? <label>朗读声音<select value={settings.systemVoice} disabled={busy} onChange={(e) => select(e.target.value)}>
      <option value="">默认离线英语声音</option>{voices.map((voice) => <option key={voice.id} value={voice.id}>{voice.name}</option>)}
    </select></label> : <p className="notice">未发现已安装的离线英语声音。请在系统“文字转语音”设置中准备英语语音包；词义复习和已有原声仍可使用。</p>)}
    {error && <p className="error" role="alert">{error}</p>}
  </section>;
}
