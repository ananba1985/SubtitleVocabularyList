import { useEffect, useState } from "react";
import { call, message, terminal } from "../api";
import type { SourceSummary, TaskHistoryPage, TaskSnapshot } from "../types";

const PAGE_SIZE = 10;
const kinds: Record<string, string> = {
  import: "剧集导入",
  collection: "词条收录",
  preview: "原声准备",
  explanation: "本地解释",
  explanation_batch: "剧集中文资料准备",
  speech: "系统英语语音",
  selection: "划词采集",
  screen_capture: "截图选区准备",
  ocr: "截图文字识别",
  review_audio: "听力题音频准备",
  site_connection: "站点连接请求",
  site_connection_check: "站点认证检查",
  online_query: "在线查询",
  sync: "站点同步",
  sync_resolve: "同步冲突处理",
};
const states: Record<string, string> = {
  queued: "等待",
  running: "处理中",
  cancel_requested: "正在取消",
  succeeded: "已完成",
  failed: "失败",
  cancelled: "已取消",
};

type Result = {
  text?: string;
  query?: string;
  meaning?: string;
  translation?: string;
  kind?: string;
  asset?: { durationMs: number };
  voice?: { name: string };
  sources?: {
    id: string;
    title: string;
    exampleCount: number;
    candidateCount: number;
  }[];
  failures?: { file: string; message: string }[];
  pushed?: number;
  pulled?: number;
  conflicts?: number;
  preparation?: {
    generated: number;
    reused: number;
    skipped: number;
    failed: number;
    sources: {
      sourceId: string;
      title: string;
      current: number;
      total: number;
    }[];
    failures: { source: string; text: string; message: string }[];
  };
};

function date(value: number) {
  return new Date(value).toLocaleString("zh-CN", { hour12: false });
}

function duration(value: number) {
  const seconds = Math.max(0, Math.round(value / 1000));
  if (seconds < 60) return `${seconds} 秒`;
  if (seconds < 3600)
    return `${Math.floor(seconds / 60)} 分 ${seconds % 60} 秒`;
  return `${Math.floor(seconds / 3600)} 小时 ${Math.floor((seconds % 3600) / 60)} 分`;
}

export function TasksView({
  tasks,
  refreshKey,
  sourceCount,
  cancel,
}: {
  tasks: TaskSnapshot[];
  refreshKey: number;
  sourceCount: number;
  cancel: (id: string) => void;
}) {
  const active = tasks.filter((task) => !terminal(task));
  const latest = tasks
    .filter(terminal)
    .sort((a, b) => b.updatedAt - a.updatedAt || b.id.localeCompare(a.id))[0];
  const [historyOpen, setHistoryOpen] = useState(false);
  const [page, setPage] = useState(0);
  const [history, setHistory] = useState<
    (TaskHistoryPage & { page: number }) | null
  >(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState("");
  const [retry, setRetry] = useState(0);
  const [preparing, setPreparing] = useState(false);
  const [preparationNotice, setPreparationNotice] = useState("");
  const [sources, setSources] = useState<SourceSummary[] | null>(null);
  const [sourceError, setSourceError] = useState("");
  useEffect(() => {
    let disposed = false;
    setSourceError("");
    setSources(null);
    call<SourceSummary[]>("sources_list")
      .then((value) => {
        if (!disposed) setSources(value);
      })
      .catch((error) => {
        if (!disposed) setSourceError(message(error));
      });
    return () => {
      disposed = true;
    };
  }, [refreshKey, sourceCount]);
  const candidateCounts = new Map(
    (sources ?? []).map((source) => [source.id, source.candidateCount]),
  );
  async function prepare() {
    setPreparing(true);
    try {
      const value = await call<{ task: TaskSnapshot | null }>(
        "explanations_prepare",
      );
      setPreparationNotice(
        value.task
          ? value.task.stage === "retry_wait"
            ? "已请求立即重试，继续使用当前后台任务。"
            : "中文资料准备已在后台运行，已保存内容会复用。"
          : "已导入剧集的中文资料已准备完成。",
      );
    } catch (error) {
      setPreparationNotice(message(error));
    } finally {
      setPreparing(false);
    }
  }
  useEffect(() => {
    if (!historyOpen) return;
    let disposed = false;
    setLoading(true);
    setError("");
    setHistory(null);
    call<TaskHistoryPage>("tasks_history", {
      offset: page * PAGE_SIZE,
      limit: PAGE_SIZE,
    })
      .then((value) => {
        if (disposed) return;
        const last = Math.max(0, Math.ceil(value.total / PAGE_SIZE) - 1);
        if (page > last) setPage(last);
        else setHistory({ ...value, page });
      })
      .catch((value) => {
        if (!disposed) setError(message(value));
      })
      .finally(() => {
        if (!disposed) setLoading(false);
      });
    return () => {
      disposed = true;
    };
  }, [historyOpen, page, latest?.id, latest?.updatedAt, retry]);
  return (
    <div className="task-list">
      <div className="preparation-action">
        <button className="secondary" disabled={preparing} onClick={prepare}>
          准备 / 继续中文资料
        </button>
        {preparationNotice && (
          <p role="status" className="muted">
            {preparationNotice}
          </p>
        )}
      </div>
      {sourceError && <p className="error">候选数量读取失败：{sourceError}</p>}
      <section aria-label="进行中的任务" className="task-section">
        <h2>进行中{active.length > 0 ? `（${active.length}）` : ""}</h2>
        {active.map((task) => (
          <TaskCard
            key={task.id}
            task={task}
            cancel={cancel}
            candidateCounts={candidateCounts}
            prepare={prepare}
            preparing={preparing}
          />
        ))}
        {!active.length && <p className="muted">当前没有进行中的任务。</p>}
      </section>
      {latest && (
        <section aria-label="最近结束的任务" className="task-section">
          <h2>最近结束的任务</h2>
          <TaskCard
            key={latest.id}
            task={latest}
            cancel={cancel}
            candidateCounts={candidateCounts}
            prepare={prepare}
            preparing={preparing}
          />
        </section>
      )}
      <details
        className="task-history"
        onToggle={(event) => {
          setHistoryOpen(event.currentTarget.open);
          if (!event.currentTarget.open) setPage(0);
        }}
      >
        <summary>历史记录</summary>
        {historyOpen && (
          <div id="task-history-content" aria-busy={loading}>
            <p className="muted">
              已完成、失败和已取消的任务，按结束时间从新到旧排列。
              日常播放、朗读和本地解释的正常操作不计入历史；失败仍可查看。
            </p>
            {loading && <p role="status">正在读取历史记录…</p>}
            {error && (
              <div role="alert" className="error">
                <p>{error}</p>
                <button onClick={() => setRetry((value) => value + 1)}>
                  重试
                </button>
              </div>
            )}
            {history && history.page === page && (
              <>
                <div className="task-list" aria-label="任务历史列表">
                  {history.items.map((task) => (
                    <TaskCard
                      key={task.id}
                      task={task}
                      cancel={cancel}
                      candidateCounts={candidateCounts}
                      prepare={prepare}
                      preparing={preparing}
                    />
                  ))}
                  {!history.total && (
                    <p className="muted">还没有已结束的任务。</p>
                  )}
                </div>
                {history.total > 0 && (
                  <nav className="task-pagination" aria-label="任务历史分页">
                    <button
                      disabled={page === 0}
                      onClick={() => setPage((value) => value - 1)}
                    >
                      上一页
                    </button>
                    <span>
                      第 {page + 1} / {Math.ceil(history.total / PAGE_SIZE)} 页
                      · 共 {history.total} 条
                    </span>
                    <button
                      disabled={(page + 1) * PAGE_SIZE >= history.total}
                      onClick={() => setPage((value) => value + 1)}
                    >
                      下一页
                    </button>
                  </nav>
                )}
              </>
            )}
          </div>
        )}
      </details>
      {!tasks.length && (
        <p className="muted">
          还没有后台任务。导入剧集或收录词条后可在这里查看。
        </p>
      )}
    </div>
  );
}

function TaskCard({
  task,
  cancel,
  candidateCounts,
  prepare,
  preparing,
}: {
  task: TaskSnapshot;
  cancel: (id: string) => void;
  candidateCounts: Map<string, number>;
  prepare: () => Promise<void>;
  preparing: boolean;
}) {
  const ended = terminal(task);
  const value = (task.result ?? {}) as Result;
  const preparation = value.preparation;
  const retryWaiting =
    task.kind === "explanation_batch" && !ended && task.stage === "retry_wait";
  const preparationCandidates = preparation?.sources.every((source) =>
    candidateCounts.has(source.sourceId),
  )
    ? preparation.sources.reduce(
        (sum, source) => sum + candidateCounts.get(source.sourceId)!,
        0,
      )
    : null;
  const subject =
    (task.kind === "review_audio" ? "听力测验" : task.subject) ||
    value.text ||
    value.query ||
    (value.sources?.length
      ? `${value.sources
          .slice(0, 3)
          .map((source) => source.title)
          .join(
            "、",
          )}${value.sources.length > 3 ? ` 等 ${value.sources.length} 个视频` : ""}`
      : "");
  return (
    <article className="task-card">
      <header>
        <strong>{kinds[task.kind] ?? "后台处理"}</strong>
        <span className={`badge ${task.state}`}>
          {retryWaiting
            ? "等待自动重试"
            : task.kind === "explanation_batch" &&
                task.state === "succeeded" &&
                preparation &&
                preparation.failed > 0
              ? "部分未完成"
              : (states[task.state] ?? task.state)}
        </span>
      </header>
      {subject && (
        <p className="task-subject" title={subject}>
          {Array.from(subject).length > 240
            ? `${Array.from(subject).slice(0, 240).join("")}…`
            : subject}
        </p>
      )}
      {!subject &&
        ["preview", "speech", "explanation", "collection", "import"].includes(
          task.kind,
        ) && <p className="muted">这条旧任务未记录处理对象。</p>}
      <dl className="task-metadata">
        <div>
          <dt>开始</dt>
          <dd>{date(task.createdAt)}</dd>
        </div>
        {ended && (
          <div>
            <dt>结束</dt>
            <dd>{date(task.updatedAt)}</dd>
          </div>
        )}
        <div>
          <dt>用时</dt>
          <dd>
            {duration((ended ? task.updatedAt : Date.now()) - task.createdAt)}
          </dd>
        </div>
      </dl>
      {!ended && (
        <>
          <p>{task.message}</p>
          {task.total > 0 && (
            <div className="task-progress">
              <progress
                aria-label="任务进度"
                value={task.current}
                max={task.total}
              />
              <span>
                {task.kind === "explanation_batch" ? "已处理词语语境 " : ""}
                {task.current.toLocaleString("zh-CN")} /{" "}
                {task.total.toLocaleString("zh-CN")}
              </span>
            </div>
          )}
          <button
            className="secondary"
            disabled={task.state === "cancel_requested"}
            onClick={() => cancel(task.id)}
          >
            {task.state === "cancel_requested" ? "正在取消…" : "取消任务"}
          </button>
        </>
      )}
      {task.kind === "import" && Boolean(task.result) && (
        <ImportResult value={value} />
      )}
      {task.state === "succeeded" && task.kind === "preview" && value.asset && (
        <p className="task-result">
          原声已准备 · {(value.asset.durationMs / 1000).toFixed(1)} 秒
        </p>
      )}
      {task.state === "succeeded" && task.kind === "speech" && (
        <p className="task-result">
          英语朗读已准备{value.voice ? ` · ${value.voice.name}` : ""}
        </p>
      )}
      {task.state === "succeeded" && task.kind === "review_audio" && (
        <p className="task-result">
          听力音频已准备
          {value.kind === "original"
            ? " · 剧集原声"
            : value.kind === "system"
              ? " · Windows 英语朗读"
              : ""}
        </p>
      )}
      {task.state === "succeeded" &&
        task.kind === "explanation" &&
        value.meaning && (
          <div className="task-result">
            <p>{value.meaning}</p>
            {value.translation && <p>{value.translation}</p>}
          </div>
        )}
      {task.state === "succeeded" && task.kind === "collection" && (
        <p className="task-result">词条及本次确认的语境已保存。</p>
      )}
      {task.state === "succeeded" &&
        ["selection", "ocr"].includes(task.kind) && (
          <p className="task-result">文字已提取，仍需在收录窗口确认保存。</p>
        )}
      {task.state === "succeeded" && task.kind === "sync" && (
        <p className="task-result">
          发送 {value.pushed ?? 0} 条 · 接收 {value.pulled ?? 0} 条 · 冲突{" "}
          {value.conflicts ?? 0} 条
        </p>
      )}
      {task.error && <p className="error">{task.error.message}</p>}
      {task.kind === "explanation_batch" && task.state === "failed" && (
        <p className="muted">
          这是本次执行结束时的错误记录，不代表模型当前状态。重试会继续补充缺失资料。
        </p>
      )}
      {task.kind === "explanation_batch" &&
        (retryWaiting ||
          (ended &&
            (task.state !== "succeeded" ||
              (preparation?.failed ?? 0) > 0))) && (
          <button
            className="secondary"
            disabled={preparing || task.state === "cancel_requested"}
            onClick={prepare}
          >
            {preparing
              ? "正在请求…"
              : retryWaiting
                ? "立即重试"
                : "重试未完成资料"}
          </button>
        )}
      {task.kind === "explanation_batch" && value.preparation && (
        <div className="task-result">
          <p>
            涉及 {value.preparation.sources.length} 集
            {preparationCandidates !== null &&
              ` · ${preparationCandidates.toLocaleString("zh-CN")} 个候选词和短语`}
          </p>
          <p className="muted">
            同一候选在不同对白中分别解释。进度按本次待补充的词语语境计数，已保存资料直接复用。
          </p>
          {ended && task.total > 0 && (
            <p>
              已处理词语语境 {task.current.toLocaleString("zh-CN")} /{" "}
              {task.total.toLocaleString("zh-CN")}
            </p>
          )}
          <p>
            新生成 {value.preparation.generated} 条语境解释 · 复用{" "}
            {value.preparation.reused} 条 · 跳过已掌握{" "}
            {value.preparation.skipped} 条 · 未完成 {value.preparation.failed}{" "}
            条
          </p>
          <details className="task-result-details">
            <summary>
              查看剧集准备明细（{value.preparation.sources.length}）
            </summary>
            <div className="import-result">
              {value.preparation.sources.map((source) => (
                <p key={source.sourceId}>
                  {source.title}
                  {candidateCounts.has(source.sourceId) &&
                    ` · ${candidateCounts.get(source.sourceId)!.toLocaleString("zh-CN")} 个候选`}
                  {" · 已处理词语语境 "}
                  {source.current.toLocaleString("zh-CN")}/
                  {source.total.toLocaleString("zh-CN")}
                </p>
              ))}
            </div>
          </details>
          {value.preparation.failed > 0 && (
            <>
              <p className="muted">
                未完成项计入已处理数量，但没有保存为有效解释。继续准备会补充缺失资料。
              </p>
              {value.preparation.failures.length > 0 && (
                <details className="task-result-details">
                  <summary>
                    查看未完成原因（前 {value.preparation.failures.length} 项）
                  </summary>
                  <div className="import-result">
                    {value.preparation.failures.map((failure, index) => (
                      <p key={index}>
                        {failure.source} · {failure.text}：{failure.message}
                      </p>
                    ))}
                  </div>
                </details>
              )}
            </>
          )}
        </div>
      )}
      {task.state === "cancelled" && !task.error && (
        <p className="muted">处理已取消，已保存内容保留。</p>
      )}
    </article>
  );
}

function ImportResult({ value }: { value: Result }) {
  const sources = value.sources ?? [],
    failures = value.failures ?? [];
  return (
    <div className="task-result">
      <p>
        成功 {sources.length} 个视频 · 失败 {failures.length} 个 ·{" "}
        {sources.reduce((sum, source) => sum + source.exampleCount, 0)} 条对白 ·{" "}
        {sources.reduce((sum, source) => sum + source.candidateCount, 0)} 个候选
      </p>
      {sources.length + failures.length > 0 && (
        <details className="task-result-details">
          <summary>
            查看视频处理明细（{sources.length + failures.length}）
          </summary>
          <div className="import-result">
            {sources.map((source) => (
              <p key={source.id}>
                {source.title} · {source.exampleCount} 条对白 ·{" "}
                {source.candidateCount} 个候选
              </p>
            ))}
            {failures.map((failure, index) => (
              <p className="error" key={index}>
                {failure.file}：{failure.message}
              </p>
            ))}
          </div>
        </details>
      )}
    </div>
  );
}
