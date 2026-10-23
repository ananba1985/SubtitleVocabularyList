import { useEffect, useState } from "react";
import { call, message } from "../api";
import type { Settings } from "../types";
import type { Provider } from "../generated/ipc";

export function MobileServiceSettings({
  settings,
  saved,
}: {
  settings: Settings;
  saved: (s: Settings) => void;
}) {
  const [providers, setProviders] = useState<Provider[]>([]),
    [url, setUrl] = useState(settings.modelUrl),
    [model, setModel] = useState(settings.modelName);
  const [apiKey, setApiKey] = useState(""),
    [clearKey, setClearKey] = useState(false),
    [busy, setBusy] = useState(false),
    [notice, setNotice] = useState("");
  useEffect(() => {
    call("providers_list")
      .then(setProviders)
      .catch((e) => setNotice(message(e)));
  }, []);
  async function update(
    p: Provider,
    enabled: boolean,
    secrets: Record<string, string> = {},
  ) {
    return call("providers_update", {
      update: {
        id: p.id,
        enabled,
        autoQuery: false,
        sendContext: p.sendContext,
        order: p.order,
        maxConcurrency: p.maxConcurrency,
        values: p.values,
        secrets,
      },
    });
  }
  async function save() {
    setBusy(true);
    setNotice("");
    try {
      const next = await call("settings_update", {
        settings: { ...settings, modelUrl: url, modelName: model },
      });
      saved(next);
      const p = providers.find((p) => p.id === "qwen");
      if (p)
        setProviders(
          await update(
            p,
            p.enabled,
            apiKey ? { apiKey } : clearKey ? { apiKey: "" } : {},
          ),
        );
      setApiKey("");
      setClearKey(false);
      setNotice("已保存。此操作不会发起模型请求。");
    } catch (e) {
      setNotice(message(e));
    } finally {
      setBusy(false);
    }
  }
  return (
    <section className="mobile-card">
      <h2>手机查询服务</h2>
      <p className="muted">
        服务由手机直接访问。HTTPS
        地址填写兼容接口的根地址；回环地址只指手机自身。密钥使用 Android
        Keystore 保护。
      </p>
      <label>
        模型根地址
        <input
          aria-label="手机模型根地址"
          type="url"
          value={url}
          onChange={(e) => setUrl(e.target.value)}
        />
      </label>
      <label>
        模型名称
        <input
          aria-label="手机模型名称"
          value={model}
          onChange={(e) => setModel(e.target.value)}
        />
      </label>
      <label>
        新的 API 密钥（留空保留）
        <input
          aria-label="手机模型密钥"
          type="password"
          autoComplete="off"
          value={apiKey}
          onChange={(e) => setApiKey(e.target.value)}
        />
      </label>
      <label className="check-label">
        <input
          type="checkbox"
          checked={clearKey}
          onChange={(e) => setClearKey(e.target.checked)}
        />
        清除已存模型密钥
      </label>
      <button disabled={busy} onClick={save}>
        保存手机模型设置
      </button>
      {providers
        .filter((p) => p.kind !== "plugin")
        .map((p) => (
          <label key={p.id} className="check-label">
            <input
              type="checkbox"
              disabled={busy}
              checked={p.enabled}
              onChange={async (e) => {
                setBusy(true);
                try {
                  setProviders(await update(p, e.target.checked));
                } catch (e) {
                  setNotice(message(e));
                } finally {
                  setBusy(false);
                }
              }}
            />
            启用{p.kind === "qwen" ? "已配置模型" : p.name}
            {p.credentialFields.includes("apiKey") ? " · 密钥已保存" : ""}
          </label>
        ))}
      {notice && <p role="status">{notice}</p>}
    </section>
  );
}
