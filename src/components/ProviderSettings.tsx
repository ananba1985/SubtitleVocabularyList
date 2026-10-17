import { useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { call } from "../api";
import type { Provider } from "../queryTypes";

export function ProviderSettings({
  providers,
  onChange,
  report,
}: {
  providers: Provider[];
  onChange: (providers: Provider[]) => void;
  report: (error: unknown) => void;
}) {
  const [busy, setBusy] = useState(false);
  async function install() {
    try {
      const path = await open({
        multiple: false,
        filters: [{ name: "声明式插件", extensions: ["json"] }],
      });
      if (typeof path !== "string") return;
      setBusy(true);
      await call("plugin_install", { path });
      onChange(await call("providers_list"));
    } catch (error) {
      report(error);
    } finally {
      setBusy(false);
    }
  }
  return (
    <section className="query-services" aria-label="查询服务设置">
      <div className="query-service-title">
        <strong>查询服务</strong>
        <button disabled={busy} onClick={install}>
          安装插件
        </button>
      </div>
      <p className="muted">
        自动查询默认只用本地模型。外部服务启用自动查询后会发送当前文字；原句需单独允许。
      </p>
      {providers.map((provider) => (
        <ProviderEditor
          key={`${provider.id}:${provider.manifest?.version ?? "builtin"}`}
          provider={provider}
          onChange={onChange}
          report={report}
        />
      ))}
    </section>
  );
}
function ProviderEditor({
  provider,
  onChange,
  report,
}: {
  provider: Provider;
  onChange: (providers: Provider[]) => void;
  report: (error: unknown) => void;
}) {
  const [draft, setDraft] = useState(provider),
    [secrets, setSecrets] = useState<Record<string, string>>({}),
    [busy, setBusy] = useState(false),
    [notice, setNotice] = useState("");
  async function save() {
    try {
      setBusy(true);
      setNotice("");
      const updated = await call("providers_update", {
        update: {
          id: draft.id,
          enabled: draft.enabled,
          autoQuery: draft.autoQuery,
          sendContext: draft.sendContext,
          order: draft.order,
          maxConcurrency: draft.maxConcurrency,
          values: draft.values,
          secrets,
        },
      });
      onChange(updated);
      setDraft(updated.find((p) => p.id === draft.id)!);
      setSecrets({});
      setNotice("已保存");
    } catch (error) {
      report(error);
    } finally {
      setBusy(false);
    }
  }
  async function remove() {
    try {
      setBusy(true);
      onChange(await call("plugin_remove", { pluginId: provider.id }));
    } catch (error) {
      report(error);
    } finally {
      setBusy(false);
    }
  }
  return (
    <details className="query-service-editor">
      <summary>
        {provider.name}
        <span>
          {provider.networkScope === "loopback" ? "本机" : "联网"}
          {provider.enabled ? " · 已启用" : " · 未启用"}
        </span>
      </summary>
      {provider.manifest && (
        <p className="query-provider-address">
          {provider.manifest.endpoint}
          <br />
          版本 {provider.manifest.version} · {provider.manifest.license}
        </p>
      )}
      {provider.kind === "qwen" && (
        <p className="muted">模型地址和并发数沿用主窗口“本地设置”。</p>
      )}
      <label className="check-label">
        <input
          type="checkbox"
          checked={draft.enabled}
          onChange={(e) => setDraft({ ...draft, enabled: e.target.checked })}
        />
        启用服务
      </label>
      <label className="check-label">
        <input
          type="checkbox"
          checked={draft.autoQuery}
          onChange={(e) => setDraft({ ...draft, autoQuery: e.target.checked })}
        />
        允许自动查询
        {draft.networkScope === "external" ? "（发送当前文字）" : ""}
      </label>
      {provider.kind === "plugin" && (
        <label className="check-label">
          <input
            type="checkbox"
            checked={draft.sendContext}
            onChange={(e) =>
              setDraft({ ...draft, sendContext: e.target.checked })
            }
          />
          允许发送原句语境
        </label>
      )}
      <div className="query-service-numbers">
        <label>
          顺序
          <input
            type="number"
            min={0}
            max={1000}
            value={draft.order}
            onChange={(e) =>
              setDraft({ ...draft, order: Number(e.target.value) })
            }
          />
        </label>
        {provider.kind !== "qwen" && (
          <label>
            请求并发
            <input
              type="number"
              min={1}
              max={16}
              value={draft.maxConcurrency}
              onChange={(e) =>
                setDraft({ ...draft, maxConcurrency: Number(e.target.value) })
              }
            />
          </label>
        )}
      </div>
      {provider.manifest?.fields.map((field) => (
        <label key={field.id}>
          {field.label}
          {field.required ? " *" : ""}
          <input
            type={field.secret ? "password" : "text"}
            autoComplete="off"
            value={
              field.secret
                ? (secrets[field.id] ?? "")
                : (draft.values[field.id] ?? "")
            }
            placeholder={
              field.secret && provider.credentialFields.includes(field.id)
                ? "已配置；留空保持，输入新值替换"
                : ""
            }
            onChange={(e) =>
              field.secret
                ? setSecrets({ ...secrets, [field.id]: e.target.value })
                : setDraft({
                    ...draft,
                    values: { ...draft.values, [field.id]: e.target.value },
                  })
            }
          />
          {field.secret && provider.credentialFields.includes(field.id) && (
            <button onClick={() => setSecrets({ ...secrets, [field.id]: "" })}>
              清除该凭据
            </button>
          )}
        </label>
      ))}
      <div className="query-row">
        <button disabled={busy} onClick={save}>
          {busy ? "处理中…" : "保存服务"}
        </button>
        {provider.kind === "plugin" && (
          <button disabled={busy} onClick={remove}>
            卸载插件
          </button>
        )}
        <small role="status">{notice}</small>
      </div>
    </details>
  );
}
