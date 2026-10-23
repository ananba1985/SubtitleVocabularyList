import { useState } from "react";
import { message, waitTask } from "../api";
import { useAssessmentMutation } from "../assessmentApi";
import type { AssessmentCollectionDraft } from "../assessmentTypes";
import type { AssessmentCollectionReceipt as Receipt } from "../generated/ipc";

interface Selection {
  sessionItemId: string;
  targetEntryId: string | null;
  expectedRevision: number | null;
  meaning: string;
  translation: string;
  saveAudio: boolean;
}
export function AssessmentCollectionPanel({
  draft,
  onSaved,
  onClose,
}: {
  draft: AssessmentCollectionDraft;
  onSaved: () => Promise<void>;
  onClose: () => void;
}) {
  const [selections, setSelections] = useState<Selection[]>(() =>
    draft.items.map((i) => ({
      sessionItemId: i.sessionItemId,
      targetEntryId: i.matches[0]?.id ?? null,
      expectedRevision: i.matches[0]?.revision ?? null,
      meaning: i.meaning,
      translation: i.translation,
      saveAudio: !!i.audioKind,
    })),
  );
  const [receipts, setReceipts] = useState<Receipt[]>([]),
    [busy, setBusy] = useState(false),
    [error, setError] = useState("");
  const mutate = useAssessmentMutation();
  const done = new Set(
    receipts.filter((r) => r.saved).map((r) => r.sessionItemId),
  );
  function change(index: number, patch: Partial<Selection>) {
    setSelections((old) =>
      old.map((s, i) => (i === index ? { ...s, ...patch } : s)),
    );
  }
  async function save() {
    setBusy(true);
    setError("");
    try {
      const task = await mutate("assessment_collection_commit", {
        draftId: draft.draftId,
        items: selections.filter((s) => !done.has(s.sessionItemId)),
      });
      const result = await waitTask(task);
      setReceipts((old) => [
        ...old.filter(
          (r) => !result.items.some((n) => n.sessionItemId === r.sessionItemId),
        ),
        ...result.items,
      ]);
      await onSaved();
    } catch (e) {
      setError(message(e));
    } finally {
      setBusy(false);
    }
  }
  return (
    <section className="assessment-collection" aria-label="确认收录测试内容">
      <div className="assessment-line">
        <h3>确认收录（{draft.items.length}）</h3>
        <button disabled={busy} onClick={onClose}>
          {receipts.length ? "关闭收录结果" : "取消"}
        </button>
      </div>
      <p className="muted">
        核对释义和合并目标后再保存。新增或合并均沿用单词本规则；测试原答案保持。
      </p>
      {draft.items.map((item, index) => {
        const value = selections[index],
          receipt = receipts.find(
            (r) => r.sessionItemId === item.sessionItemId,
          );
        return (
          <fieldset
            disabled={busy || done.has(item.sessionItemId)}
            key={item.sessionItemId}
          >
            <legend>
              {item.text}
              {done.has(item.sessionItemId) ? " · 已保存" : ""}
            </legend>
            <p>{item.sentence}</p>
            {!!item.collectedEntryIds.length && (
              <p className="muted">
                此题已有收录关联，重复收录会按原规则增加一次收录记录。
              </p>
            )}
            <label>
              保存到
              <select
                value={value.targetEntryId ?? ""}
                onChange={(e) => {
                  const match = item.matches.find(
                    (m) => m.id === e.target.value,
                  );
                  change(index, {
                    targetEntryId: match?.id ?? null,
                    expectedRevision: match?.revision ?? null,
                  });
                }}
              >
                <option value="">新建词条</option>
                {item.matches.map((m) => (
                  <option key={m.id} value={m.id}>
                    合并：{m.text} · {m.meanings.map((v) => v.text).join("；")}
                  </option>
                ))}
              </select>
            </label>
            <label>
              中文释义
              <input
                value={value.meaning}
                onChange={(e) => change(index, { meaning: e.target.value })}
              />
            </label>
            <label>
              例句中文
              <textarea
                rows={2}
                value={value.translation}
                onChange={(e) => change(index, { translation: e.target.value })}
              />
            </label>
            {item.audioKind && (
              <label className="checkbox">
                <input
                  type="checkbox"
                  checked={value.saveAudio}
                  onChange={(e) =>
                    change(index, { saveAudio: e.target.checked })
                  }
                />
                保留
                {item.audioKind === "original"
                  ? "真实原声"
                  : "本机离线英语语音"}
              </label>
            )}
            {receipt && (
              <p role="status" className={receipt.saved ? "" : "error"}>
                {receipt.saved
                  ? receipt.result?.created
                    ? "已新建词条"
                    : "已合并并保留原资料"
                  : receipt.error?.message}
              </p>
            )}
          </fieldset>
        );
      })}
      {error && (
        <p role="alert" className="error">
          {error}
        </p>
      )}
      <button
        className="primary"
        disabled={busy || done.size === draft.items.length}
        onClick={() => void save()}
      >
        {busy
          ? "正在保存…"
          : receipts.length
            ? "重试未保存项目"
            : "确认保存选中内容"}
      </button>
    </section>
  );
}
