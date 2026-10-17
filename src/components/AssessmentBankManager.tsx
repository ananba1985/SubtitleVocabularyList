import { useEffect, useState } from "react";
import { call, message, waitTask } from "../api";
import { useAssessmentMutation } from "../assessmentApi";
import { assessmentTypeNames } from "../assessmentTypes";
import type {
  AssessmentBankItem,
  AssessmentBankPage,
  AssessmentConfig,
  AssessmentDraft,
  AssessmentInventory,
} from "../assessmentTypes";
import type { TaskSnapshot } from "../types";
import { AudioPlayer } from "./AudioPlayer";

const PAGE = 12;
const stateNames: Record<string, string> = {
  needs_review: "待核对",
  approved: "已批准",
  rejected: "已拒绝",
  draft: "草稿",
};
export function AssessmentBankManager({
  inventory,
  config,
  dataDirectory,
  tasks,
  onChanged,
  showTasks,
}: {
  inventory: AssessmentInventory;
  config: AssessmentConfig;
  dataDirectory: string;
  tasks: TaskSnapshot[];
  onChanged: () => Promise<void>;
  showTasks: () => void;
}) {
  const [bankId, setBankId] = useState(inventory.banks[0]?.id ?? ""),
    [filter, setFilter] = useState("needs_review"),
    [page, setPage] = useState(0);
  const [items, setItems] = useState<AssessmentBankPage | null>(null),
    [selected, setSelected] = useState<AssessmentBankItem | null>(null),
    [draft, setDraft] = useState<AssessmentDraft | null>(null);
  const [note, setNote] = useState(""),
    [error, setError] = useState(""),
    [notice, setNotice] = useState(""),
    [busy, setBusy] = useState(false),
    [limit, setLimit] = useState(100),
    [reload, setReload] = useState(0),
    [play, setPlay] = useState(false);
  const [prepareTask, setPrepareTask] = useState<string | null>(null);
  const mutate = useAssessmentMutation();
  const bank = inventory.banks.find((b) => b.id === bankId);
  useEffect(() => {
    if (!bankId && inventory.banks.length) setBankId(inventory.banks[0].id);
  }, [bankId, inventory.banks]);
  useEffect(() => {
    if (!prepareTask) return;
    const task = tasks.find((t) => t.id === prepareTask);
    const id = (task?.result as { bankId?: string } | null)?.bankId;
    if (id && id !== bankId) {
      setBankId(id);
      void onChanged().catch((e) => setError(message(e)));
    }
  }, [prepareTask, tasks, bankId, onChanged]);
  useEffect(() => {
    if (!bankId) {
      setItems(null);
      return;
    }
    let disposed = false;
    setError("");
    call("assessment_bank_items", {
      bankId,
      state: filter || null,
      offset: page * PAGE,
      limit: PAGE,
    })
      .then((value) => {
        if (!disposed) setItems(value);
      })
      .catch((e) => {
        if (!disposed) setError(message(e));
      });
    return () => {
      disposed = true;
    };
  }, [bankId, filter, page, reload, bank?.revision]);
  async function prepare(continueDraft: boolean) {
    setBusy(true);
    setError("");
    setNotice("");
    const ranges = Object.fromEntries(
      inventory.catalogs
        .filter((c) => config.content === "mixed" || c.kind === config.content)
        .map((c) => [
          c.id,
          c.kind === "word" ? config.wordTopN : config.phraseTopN,
        ]),
    );
    try {
      const task = await mutate("assessment_bank_prepare", {
        bankId: continueDraft ? bankId : null,
        ranges,
        questionTypes: config.questionTypes,
        limit,
      });
      setPrepareTask(task.id);
      setNotice(
        "题库准备已提交，可在后台任务查看。候选经核对并发布后才可测试。",
      );
      await onChanged();
    } catch (e) {
      setError(message(e));
    } finally {
      setBusy(false);
    }
  }
  function select(item: AssessmentBankItem) {
    setSelected(item);
    setDraft(structuredClone(item.content));
    setNote("");
    setPlay(false);
    setError("");
  }
  async function review(action: string) {
    if (!bank) return;
    setBusy(true);
    setError("");
    setNotice("");
    try {
      await mutate("assessment_bank_review", {
        bankId: bank.id,
        expectedRevision: bank.revision,
        action,
        itemId: action === "publish" ? null : (selected?.id ?? null),
        expectedItemRevision:
          action === "publish" ? null : (selected?.revision ?? null),
        content: action === "publish" ? null : draft,
        note,
      });
      setSelected(null);
      setDraft(null);
      setNote("");
      setReload((v) => v + 1);
      await onChanged();
      setNotice(
        action === "publish"
          ? "已发布新题库版本，已有测试不受影响。"
          : action === "approve"
            ? "题目已批准，发布题库后可参与抽样。"
            : "核对记录已保存。",
      );
    } catch (e) {
      setError(message(e));
      await onChanged().catch(() => {});
    } finally {
      setBusy(false);
    }
  }
  async function regenerateAudio() {
    if (!draft) return;
    setBusy(true);
    setError("");
    setPlay(false);
    try {
      const task = await mutate("assessment_bank_audio_prepare", {
        content: draft,
      });
      setDraft(await waitTask<AssessmentDraft>(task));
      setNotice("本地英语音频已重新生成，请试听并核对后保存或批准。");
    } catch (e) {
      setError(message(e));
    } finally {
      setBusy(false);
    }
  }
  function field<K extends keyof AssessmentDraft>(
    key: K,
    value: AssessmentDraft[K],
  ) {
    setDraft((old) => (old ? { ...old, [key]: value } : old));
  }
  return (
    <div className="assessment-bank">
      {!inventory.indexReady && (
        <p className="error">
          基础词库尚未包含完整词频目录，请先用词库准备脚本更新参考资料。
        </p>
      )}
      <div className="assessment-line">
        <p>按当前测试范围准备题目，已有结果会复用。</p>
        <button
          disabled={busy}
          onClick={() => {
            setReload((v) => v + 1);
            void onChanged().catch((e) => setError(message(e)));
          }}
        >
          刷新题库
        </button>
      </div>
      <div className="assessment-bank-prepare">
        <label>
          本次最多准备目标
          <input
            type="number"
            min={1}
            max={10000}
            value={limit}
            onChange={(e) => setLimit(Number(e.target.value))}
          />
        </label>
        <button
          disabled={busy || !inventory.indexReady || limit < 1}
          onClick={() => void prepare(false)}
        >
          新建题库并准备
        </button>
        {bank?.state === "draft" && (
          <button
            disabled={busy || !inventory.indexReady}
            onClick={() => void prepare(true)}
          >
            继续准备此草稿
          </button>
        )}
      </div>
      {notice && (
        <p role="status">
          {notice}{" "}
          {prepareTask && (
            <button className="text-button" onClick={showTasks}>
              查看后台任务
            </button>
          )}
        </p>
      )}
      {error && (
        <p className="error" role="alert">
          {error}
        </p>
      )}
      {!!inventory.banks.length && (
        <>
          <div className="assessment-bank-filters">
            <label>
              题库版本
              <select
                value={bankId}
                onChange={(e) => {
                  setBankId(e.target.value);
                  setSelected(null);
                  setDraft(null);
                  setPage(0);
                }}
              >
                {inventory.banks.map((b) => (
                  <option key={b.id} value={b.id}>
                    {b.state === "published" ? "已发布" : "草稿"} ·{" "}
                    {new Date(b.createdAt).toLocaleString()} · {b.targetCount}{" "}
                    个目标
                  </option>
                ))}
              </select>
            </label>
            <label>
              题目状态
              <select
                value={filter}
                onChange={(e) => {
                  setFilter(e.target.value);
                  setPage(0);
                  setSelected(null);
                  setDraft(null);
                }}
              >
                <option value="needs_review">待核对</option>
                <option value="approved">已批准</option>
                <option value="rejected">已拒绝</option>
                <option value="">全部</option>
              </select>
            </label>
          </div>
          {bank && (
            <p className="muted">
              目标 {bank.targetCount} 个 · 候选题{" "}
              {Object.values(bank.itemCounts).reduce((a, b) => a + b, 0)} 道 ·
              题库修订 {bank.revision}
            </p>
          )}
          <div className="assessment-bank-grid">
            <div className="assessment-bank-list">
              {items?.items.map((item) => (
                <button
                  className={selected?.id === item.id ? "selected" : ""}
                  key={item.id}
                  onClick={() => select(item)}
                >
                  <strong>{item.content.target}</strong>
                  <small>
                    {assessmentTypeNames[item.questionType]} · 原排名{" "}
                    {item.targetRank} · {stateNames[item.state] ?? item.state}
                  </small>
                </button>
              ))}
              {items && !items.items.length && (
                <p className="muted">此状态下没有题目。</p>
              )}
              <div className="pagination">
                <button
                  disabled={page === 0}
                  onClick={() => setPage((v) => v - 1)}
                >
                  上一页
                </button>
                <span>
                  {page + 1} /{" "}
                  {Math.max(1, Math.ceil((items?.total ?? 0) / PAGE))}
                </span>
                <button
                  disabled={(page + 1) * PAGE >= (items?.total ?? 0)}
                  onClick={() => setPage((v) => v + 1)}
                >
                  下一页
                </button>
              </div>
            </div>
            {selected && draft ? (
              <section className="assessment-bank-review" aria-label="核对题目">
                <h3>
                  {draft.target} · {assessmentTypeNames[selected.questionType]}
                </h3>
                <p className="muted">
                  来源：{draft.origin}
                  ；题干、义项、干扰项、合理答案及声音均需核对。
                </p>
                <label>
                  本题中文义项
                  <input
                    disabled={busy || bank?.state === "published"}
                    value={draft.meaning}
                    onChange={(e) => field("meaning", e.target.value)}
                  />
                </label>
                <label>
                  英文例句
                  <textarea
                    disabled={busy || bank?.state === "published"}
                    rows={2}
                    value={draft.sentence}
                    onChange={(e) => field("sentence", e.target.value)}
                  />
                </label>
                <label>
                  整句中文
                  <textarea
                    disabled={busy || bank?.state === "published"}
                    rows={2}
                    value={draft.translation}
                    onChange={(e) => field("translation", e.target.value)}
                  />
                </label>
                <label>
                  题干
                  <textarea
                    disabled={busy || bank?.state === "published"}
                    rows={2}
                    value={draft.prompt}
                    onChange={(e) => field("prompt", e.target.value)}
                  />
                </label>
                {selected.questionType === "cloze" ? (
                  <label>
                    可接受答案（每行一个）
                    <textarea
                      disabled={busy || bank?.state === "published"}
                      rows={2}
                      value={draft.acceptedAnswers.join("\n")}
                      onChange={(e) =>
                        field("acceptedAnswers", e.target.value.split("\n"))
                      }
                    />
                  </label>
                ) : (
                  <div className="assessment-review-choices">
                    {draft.choices.map((choice, index) => (
                      <label
                        key={choice.id}
                        className="assessment-review-choice"
                      >
                        <input
                          type="radio"
                          aria-label={`选项 ${index + 1} 为正确答案`}
                          disabled={busy || bank?.state === "published"}
                          checked={draft.correctOptionId === choice.id}
                          onChange={() => field("correctOptionId", choice.id)}
                        />
                        <input
                          aria-label={`选项 ${index + 1}`}
                          disabled={busy || bank?.state === "published"}
                          value={choice.text}
                          onChange={(e) =>
                            field(
                              "choices",
                              draft.choices.map((c) =>
                                c.id === choice.id
                                  ? { ...c, text: e.target.value }
                                  : c,
                              ),
                            )
                          }
                        />
                      </label>
                    ))}
                  </div>
                )}
                <label>
                  讲解
                  <textarea
                    disabled={busy || bank?.state === "published"}
                    rows={3}
                    value={draft.explanation}
                    onChange={(e) => field("explanation", e.target.value)}
                  />
                </label>
                {selected.questionType === "listening" &&
                  bank?.state === "draft" && (
                    <button
                      disabled={busy}
                      onClick={() => void regenerateAudio()}
                    >
                      重新生成本地英语音频
                    </button>
                  )}
                {draft.audio && (
                  <div>
                    <button onClick={() => setPlay(true)}>试听审核音频</button>
                    {play && (
                      <AudioPlayer
                        path={`${dataDirectory}/${draft.audio.relativePath}`}
                        label={
                          draft.audio.kind === "original"
                            ? "真实原声"
                            : "Windows 本地英语语音"
                        }
                      />
                    )}
                  </div>
                )}
                {bank?.state === "draft" && (
                  <>
                    <label>
                      核对结论
                      <input
                        value={note}
                        onChange={(e) => setNote(e.target.value)}
                        placeholder="说明目标、语境、选项／答案和声音的核对结论"
                      />
                    </label>
                    <div className="actions">
                      <button
                        className="primary"
                        disabled={
                          busy ||
                          !note.trim() ||
                          selected.state !== "needs_review"
                        }
                        onClick={() => void review("approve")}
                      >
                        确认核对并批准
                      </button>
                      <button
                        disabled={busy || !note.trim()}
                        onClick={() => void review("edit")}
                      >
                        保存为待核对
                      </button>
                      <button
                        disabled={busy || !note.trim()}
                        onClick={() => void review("reject")}
                      >
                        拒绝此题
                      </button>
                    </div>
                  </>
                )}
                {bank?.state === "published" && (
                  <p className="muted">
                    已发布版本固定保存，调整内容请新建题库。
                  </p>
                )}
              </section>
            ) : (
              <p className="muted">
                选择一题，核对后明确批准；模型生成不等于已核对。
              </p>
            )}
          </div>
          {bank?.state === "draft" && !selected && (
            <div className="assessment-publish">
              <label>
                发布说明
                <input
                  value={note}
                  onChange={(e) => setNote(e.target.value)}
                  placeholder="确认本次已批准题目的核对范围"
                />
              </label>
              <button
                className="primary"
                disabled={
                  busy ||
                  !note.trim() ||
                  !Object.entries(bank.itemCounts).some(
                    ([key, n]) => key.startsWith("approved:") && n > 0,
                  )
                }
                onClick={() => void review("publish")}
              >
                发布已批准题目
              </button>
              <p className="muted">
                发布后整个版本固定；未批准题不会进入测试。
              </p>
            </div>
          )}
          {selected && (
            <button
              className="text-button"
              onClick={() => {
                setSelected(null);
                setDraft(null);
                setNote("");
                setPlay(false);
              }}
            >
              返回题库概览
            </button>
          )}
        </>
      )}
      <details className="assessment-sources">
        <summary>来源与排名说明</summary>
        {inventory.catalogs.map((c) => (
          <section key={c.id}>
            <h4>{c.title}</h4>
            <p>
              {c.rawRows.toLocaleString()} 条原始记录 · 最大原排名{" "}
              {c.maxRank.toLocaleString()} · 已筛选{" "}
              {c.eligibleTargets.toLocaleString()} 个目标
            </p>
            <p>
              <a href={c.sourceUrl} target="_blank" rel="noreferrer">
                来源
              </a>{" "}
              · {c.sourceVersion}
            </p>
            <p className="muted">{c.notice}</p>
          </section>
        ))}
      </details>
    </div>
  );
}
