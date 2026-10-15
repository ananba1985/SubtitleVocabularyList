import { useCallback, useEffect, useState } from "react";
import { call, message, terminal } from "../api";
import { useAssessmentMutation } from "../assessmentApi";
import { assessmentStateNames, assessmentTypeNames } from "../assessmentTypes";
import type {
  AssessmentConfig,
  AssessmentInventory,
  AssessmentPreview,
  AssessmentQuestionType,
  AssessmentSession,
  AssessmentSessionSummary,
  AssessmentSettings,
} from "../assessmentTypes";
import type { TaskSnapshot } from "../types";
import { AssessmentBankManager } from "./AssessmentBankManager";
import { AssessmentQuestionPanel } from "./AssessmentQuestionPanel";
import { AssessmentReportPanel } from "./AssessmentReportPanel";
import { AssessmentHistory } from "./AssessmentHistory";

export function AssessmentView({
  dataDirectory,
  tasks,
  showTasks,
}: {
  dataDirectory: string;
  tasks: TaskSnapshot[];
  showTasks: () => void;
}) {
  const [inventory, setInventory] = useState<AssessmentInventory | null>(null),
    [settings, setSettings] = useState<AssessmentSettings | null>(null);
  const [config, setConfig] = useState<AssessmentConfig | null>(null),
    [preview, setPreview] = useState<AssessmentPreview | null>(null),
    [session, setSession] = useState<AssessmentSession | null>(null);
  const [unfinished, setUnfinished] = useState<AssessmentSessionSummary[]>([]),
    [error, setError] = useState(""),
    [busy, setBusy] = useState(false),
    [previewBusy, setPreviewBusy] = useState(false),
    [bankOpen, setBankOpen] = useState(false);
  const [previewRefresh, setPreviewRefresh] = useState(0);
  const [reportId, setReportId] = useState<string | null>(null);
  const mutate = useAssessmentMutation();
  const refresh = useCallback(async () => {
    const [catalog, sessions] = await Promise.all([
      call<AssessmentInventory>("assessment_catalog_get"),
      call<{ items: AssessmentSessionSummary[] }>("assessment_session_list", {
        unfinishedOnly: true,
        offset: 0,
        limit: 20,
      }),
    ]);
    setInventory(catalog);
    setUnfinished(sessions.items);
    setPreviewRefresh((v) => v + 1);
  }, []);
  useEffect(() => {
    let disposed = false;
    Promise.all([
      call<AssessmentSettings>("assessment_settings_get"),
      refresh(),
    ])
      .then(([saved]) => {
        if (!disposed) {
          setSettings(saved);
          setConfig(saved.config);
        }
      })
      .catch((e) => {
        if (!disposed) setError(message(e));
      });
    return () => {
      disposed = true;
    };
  }, [refresh]);
  const completedBanks = tasks
    .filter((t) => t.kind === "assessment_bank" && terminal(t))
    .map((t) => `${t.id}:${t.updatedAt}`)
    .join(",");
  useEffect(() => {
    if (completedBanks) void refresh().catch((e) => setError(message(e)));
  }, [completedBanks, refresh]);
  useEffect(() => {
    if (!config || !inventory?.indexReady || session) return;
    let disposed = false;
    setPreviewBusy(true);
    setPreview(null);
    const timer = setTimeout(() => {
      call<AssessmentPreview>("assessment_preview", { config })
        .then((next) => {
          if (!disposed) {
            setPreview(next);
            setError("");
          }
        })
        .catch((e) => {
          if (!disposed) setError(message(e));
        })
        .finally(() => {
          if (!disposed) setPreviewBusy(false);
        });
    }, 250);
    return () => {
      disposed = true;
      clearTimeout(timer);
    };
  }, [config, inventory?.indexReady, previewRefresh, session?.id]);
  function change<K extends keyof AssessmentConfig>(
    key: K,
    value: AssessmentConfig[K],
  ) {
    setConfig((old) =>
      old ? { ...old, mode: "specified", [key]: value } : old,
    );
  }
  async function start() {
    if (!config || !preview) return;
    setBusy(true);
    setError("");
    try {
      setSession(
        await mutate<AssessmentSession>("assessment_start", {
          config,
          expectedInventoryRevision: preview.inventoryRevision,
        }),
      );
    } catch (e) {
      setError(message(e));
      setPreviewRefresh((v) => v + 1);
    } finally {
      setBusy(false);
    }
  }
  async function resume(id: string) {
    setBusy(true);
    setError("");
    try {
      setSession(
        await call<AssessmentSession>("assessment_session_get", {
          sessionId: id,
        }),
      );
    } catch (e) {
      setError(message(e));
    } finally {
      setBusy(false);
    }
  }
  async function save() {
    if (!settings || !config) return;
    setBusy(true);
    setError("");
    try {
      setSettings(
        await call<AssessmentSettings>("assessment_settings_update", {
          settings: { revision: settings.revision, config },
          operationId: crypto.randomUUID(),
        }),
      );
    } catch (e) {
      setError(message(e));
    } finally {
      setBusy(false);
    }
  }
  const terminalSession =
    session && ["completed", "ended_early"].includes(session.state)
      ? session
      : null;
  if (reportId || terminalSession)
    return (
      <AssessmentReportPanel
        key={reportId ?? terminalSession!.id}
        sessionId={reportId ?? terminalSession!.id}
        onExit={() => {
          setReportId(null);
          setSession(null);
          void refresh().catch((e) => setError(message(e)));
        }}
        onRetest={(original) => {
          setConfig(original);
          setReportId(null);
          setSession(null);
          setPreviewRefresh((v) => v + 1);
        }}
      />
    );
  if (session)
    return (
      <div className="assessment-view">
        <AssessmentQuestionPanel
          key={session.id}
          session={session}
          onChanged={setSession}
          onExit={() => {
            setSession(null);
            void refresh().catch((e) => setError(message(e)));
          }}
        />
      </div>
    );
  if (!config || !inventory)
    return <p role="status">{error || "正在读取水平测试资料…"}</p>;
  const types = Object.keys(assessmentTypeNames) as AssessmentQuestionType[];
  return (
    <div className="assessment-view">
      {error && (
        <p className="error" role="alert">
          {error}
        </p>
      )}
      <section className="assessment-home">
        <h2>高频词汇摸底</h2>
        <p>按常用程度抽样，通过实际作答了解本次已测表现。</p>
        <p className="assessment-summary">
          {config.content === "word"
            ? `单词 Top ${config.wordTopN}`
            : config.content === "phrase"
              ? `组合 Top ${config.phraseTopN}`
              : `单词 Top ${config.wordTopN} ＋ 组合 Top ${config.phraseTopN}`}{" "}
          · {config.questionCount} 题 ·{" "}
          {config.questionTypes.map((t) => assessmentTypeNames[t]).join("／")}
        </p>
        <div className="actions">
          <button
            className="primary"
            disabled={busy || previewBusy || !preview?.canStart}
            onClick={() => void start()}
          >
            {previewBusy ? "正在核对题量…" : "开始测试"}
          </button>
          <button
            onClick={() => setBankOpen((v) => !v)}
            aria-expanded={bankOpen}
          >
            题库准备与核对
          </button>
        </div>
        {preview && !preview.canStart && (
          <p className="muted">
            当前配额缺少 {preview.missingSlots}{" "}
            个不重复目标。可减少题量、调整范围／题型，或准备并发布题库。
          </p>
        )}
        {!inventory.indexReady && (
          <p className="muted">完整词频目录尚未就绪，请先准备参考资料。</p>
        )}
        {preview?.canStart && preview.warnings.length > 0 && (
          <p className="muted">
            基于所选范围内已准备题目抽样；覆盖、样本和重复限制将保留在报告中。
          </p>
        )}
        {!!unfinished.length && (
          <div className="assessment-continue">
            <strong>有未完成测试</strong>
            <button
              disabled={busy}
              onClick={() => void resume(unfinished[0].id)}
            >
              继续上次 · {unfinished[0].position}/
              {unfinished[0].config.questionCount}
            </button>
            {unfinished.length > 1 && (
              <details>
                <summary>其他未完成测试（{unfinished.length - 1}）</summary>
                {unfinished.slice(1).map((s) => (
                  <button
                    key={s.id}
                    disabled={busy}
                    onClick={() => void resume(s.id)}
                  >
                    {new Date(s.createdAt).toLocaleString()} ·{" "}
                    {assessmentStateNames[s.state]} · {s.position}/
                    {s.config.questionCount}
                  </button>
                ))}
              </details>
            )}
          </div>
        )}
      </section>
      <AssessmentHistory onOpen={setReportId} />
      <details className="assessment-settings">
        <summary>调整测试范围与题型</summary>
        <div className="assessment-config-grid">
          <label>
            内容类别
            <select
              value={config.content}
              onChange={(e) =>
                change("content", e.target.value as AssessmentConfig["content"])
              }
            >
              <option value="word">单词</option>
              <option value="phrase">短语与组合</option>
              <option value="mixed">单词＋组合</option>
            </select>
          </label>
          <label>
            题目数量
            <input
              type="number"
              min={1}
              max={300}
              value={config.questionCount}
              onChange={(e) => change("questionCount", Number(e.target.value))}
            />
          </label>
        </div>
        <div className="actions">
          {[1000, 3000, 5000].map((n) => (
            <button
              key={n}
              onClick={() =>
                setConfig((old) =>
                  old
                    ? {
                        ...old,
                        mode: "specified",
                        wordTopN: Math.min(
                          n,
                          inventory.catalogs.find((c) => c.kind === "word")
                            ?.maxRank ?? n,
                        ),
                        phraseTopN: Math.min(
                          n,
                          inventory.catalogs.find((c) => c.kind === "phrase")
                            ?.maxRank ?? n,
                        ),
                      }
                    : old,
                )
              }
            >
              Top {n}
            </button>
          ))}
        </div>
        <div className="assessment-config-grid">
          {config.content !== "phrase" && (
            <label>
              单词原排名上限
              <input
                type="number"
                min={1}
                max={inventory.catalogs.find((c) => c.kind === "word")?.maxRank}
                value={config.wordTopN}
                onChange={(e) => change("wordTopN", Number(e.target.value))}
              />
            </label>
          )}
          {config.content !== "word" && (
            <label>
              组合原排名上限
              <input
                type="number"
                min={1}
                max={
                  inventory.catalogs.find((c) => c.kind === "phrase")?.maxRank
                }
                value={config.phraseTopN}
                onChange={(e) => change("phraseTopN", Number(e.target.value))}
              />
            </label>
          )}
        </div>
        <fieldset className="assessment-types">
          <legend>题型</legend>
          {types.map((t) => (
            <label key={t} className="assessment-check">
              <input
                type="checkbox"
                checked={config.questionTypes.includes(t)}
                onChange={(e) =>
                  change(
                    "questionTypes",
                    e.target.checked
                      ? [...config.questionTypes, t]
                      : config.questionTypes.filter((v) => v !== t),
                  )
                }
              />
              {assessmentTypeNames[t]}
            </label>
          ))}
        </fieldset>
        <details>
          <summary>高级抽样参数</summary>
          <div className="assessment-config-grid">
            <label>
              每个频段的排名跨度
              <input
                type="number"
                min={1}
                max={10000}
                value={config.bandSize}
                onChange={(e) => change("bandSize", Number(e.target.value))}
              />
            </label>
            <label>
              优先避开近期目标（天）
              <input
                type="number"
                min={0}
                max={365}
                value={config.avoidRecentDays}
                onChange={(e) =>
                  change("avoidRecentDays", Number(e.target.value))
                }
              />
            </label>
            <label>
              样本不足提示阈值（题）
              <input
                type="number"
                min={1}
                value={config.minimumSample}
                onChange={(e) =>
                  change("minimumSample", Number(e.target.value))
                }
              />
            </label>
          </div>
          <p className="muted">
            初始策略未校准；仅展示本次已测结果，不换算精确词汇量或语言等级。
          </p>
        </details>
        <div className="actions">
          <button disabled={busy} onClick={() => void save()}>
            保存为默认配置
          </button>
          <button
            onClick={() => {
              setConfig(settings?.config ?? config);
            }}
          >
            恢复已保存配置
          </button>
        </div>
      </details>
      {preview && (
        <details className="assessment-preview">
          <summary>范围覆盖与抽样配额</summary>
          {preview.coverage.map((c) => (
            <p key={c.catalogId}>
              {preview.catalogs.find((x) => x.id === c.catalogId)?.title} ·
              原范围 {c.topN} · 词频目标 {c.rawTargets}／归并词元{" "}
              {c.lexemeTargets} · 有已批准题目的目标 {c.preparedTargets} ·
              缺题目标 {c.missingTargets}
            </p>
          ))}
          {preview.warnings.map((w) => (
            <p className="muted" key={w}>
              {w}
            </p>
          ))}
          <div className="assessment-table">
            <table>
              <thead>
                <tr>
                  <th>类别／频段</th>
                  <th>题型</th>
                  <th>计划</th>
                  <th>可用</th>
                  <th>缺额</th>
                </tr>
              </thead>
              <tbody>
                {preview.quotas
                  .filter((q) => q.planned > 0)
                  .map((q) => (
                    <tr key={`${q.catalogId}:${q.fromRank}:${q.questionType}`}>
                      <td>
                        {q.content === "word" ? "单词" : "组合"} · {q.fromRank}–
                        {q.toRank}
                      </td>
                      <td>{assessmentTypeNames[q.questionType]}</td>
                      <td>{q.planned}</td>
                      <td>{q.available}</td>
                      <td>{q.unfilled}</td>
                    </tr>
                  ))}
              </tbody>
            </table>
          </div>
        </details>
      )}
      {bankOpen && (
        <section className="assessment-bank-panel">
          <div className="assessment-line">
            <h2>题库准备与核对</h2>
            <button className="text-button" onClick={() => setBankOpen(false)}>
              收起
            </button>
          </div>
          <AssessmentBankManager
            inventory={inventory}
            config={config}
            dataDirectory={dataDirectory}
            tasks={tasks}
            onChanged={refresh}
            showTasks={showTasks}
          />
        </section>
      )}
    </div>
  );
}
