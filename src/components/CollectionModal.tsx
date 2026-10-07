import { useEffect, useRef, useState } from "react";
import { call, message, uid, waitTask } from "../api";
import type {
  CollectionInput,
  CollectionSeed,
  Explanation,
  PreparedCollection,
  TaskSnapshot,
} from "../types";
import { SystemSpeech } from "./SystemSpeech";
import { OnlineLookup } from "./OnlineLookup";
export function CollectionModal({
  seed,
  onClose,
  onSaved,
}: {
  seed: CollectionSeed;
  onClose: () => void;
  onSaved: () => void;
}) {
  const [text, setText] = useState(seed.text),
    [kind, setKind] = useState(seed.kind),
    [meaning, setMeaning] = useState(""),
    [context, setContext] = useState(seed.context);
  const [explanation, setExplanation] = useState<Explanation | null>(null),
    [prepared, setPrepared] = useState<PreparedCollection | null>(null);
  const [target, setTarget] = useState("create"),
    [saveAudio, setSaveAudio] = useState(Boolean(seed.example)),
    [busy, setBusy] = useState(false),
    [error, setError] = useState("");
  const [task, setTask] = useState<TaskSnapshot | null>(null),
    [operationId, setOperationId] = useState(uid);
  useEffect(() => {
    setExplanation(null);
  }, [text, context]);
  const submitted = useRef<{
    signature: string;
    prepared: PreparedCollection;
  } | null>(null);
  const change = () => {
    setPrepared(null);
    setTarget("create");
    setError("");
    setOperationId(uid());
  };
  async function explain() {
    setBusy(true);
    setError("");
    try {
      const started = await call<TaskSnapshot>("explain_start", {
        text,
        context,
        operationId: uid(),
      });
      setExplanation(await waitTask<Explanation>(started, setTask));
    } catch (error) {
      setError(message(error));
    } finally {
      setBusy(false);
    }
  }
  async function prepare() {
    setBusy(true);
    setError("");
    try {
      let result: PreparedCollection;
      if (seed.sourceId && seed.example)
        result = await call<PreparedCollection>("collection_from_example", {
          sourceId: seed.sourceId,
          exampleId: seed.example.id,
          text,
          kind,
          meaning,
          operationId,
        });
      else {
        const input: CollectionInput = {
          operationId,
          kind,
          text,
          meaning,
          examples: context.trim()
            ? [
                {
                  text: context,
                  contextMeaning: meaning,
                  sourceId: seed.sourceId ?? null,
                  locationKey: seed.locationKey ?? "",
                  startMs: null,
                  endMs: null,
                  mediaAssetIds: [],
                },
              ]
            : [],
          targetEntryId: null,
          expectedRevision: null,
        };
        result = await call<PreparedCollection>("collection_prepare", {
          input,
        });
      }
      setPrepared(result);
      if (result.matches.length === 1 && result.matches[0].text === text.trim())
        setTarget(result.matches[0].id);
    } catch (error) {
      setError(message(error));
    } finally {
      setBusy(false);
    }
  }
  async function commit() {
    if (!prepared) return;
    setBusy(true);
    setError("");
    try {
      const match = prepared.matches.find((entry) => entry.id === target);
      const signature = JSON.stringify({
        kind: prepared.input.kind,
        text: prepared.input.text,
        meaning: prepared.input.meaning,
        examples: prepared.input.examples,
        target,
        revision: match?.revision,
        saveAudio,
      });
      let current = prepared;
      if (submitted.current?.signature === signature)
        current = submitted.current.prepared;
      else if (submitted.current)
        current = await call<PreparedCollection>("collection_prepare", {
          input: { ...prepared.input, operationId: uid() },
        });
      submitted.current = { signature, prepared: current };
      const started = await call<TaskSnapshot>("collection_commit", {
        draftId: current.draftId,
        targetEntryId: match?.id ?? null,
        expectedRevision: match?.revision ?? null,
        saveAudio,
      });
      await waitTask(started, setTask);
      onSaved();
    } catch (error) {
      setError(message(error));
    } finally {
      setBusy(false);
    }
  }
  async function cancelTask() {
    if (task && !["succeeded", "failed", "cancelled"].includes(task.state))
      await call("task_cancel", { taskId: task.id });
  }
  return (
    <div className="modal-backdrop">
      <section
        className="modal"
        role="dialog"
        aria-modal="true"
        aria-label="收录确认"
      >
        <header>
          <div>
            <p className="eyebrow">保存这次相遇</p>
            <h2>收录确认</h2>
          </div>
          <button
            className="icon-button"
            disabled={busy}
            onClick={onClose}
            aria-label="关闭收录窗口"
          >
            ×
          </button>
        </header>
        <div className="field-row">
          <label>
            类型
            <select
              value={kind}
              disabled={busy}
              onChange={(event) => {
                setKind(event.target.value);
                change();
              }}
            >
              <option value="word">单词</option>
              <option value="phrase">短语</option>
              <option value="sentence">整句</option>
            </select>
          </label>
          <label className="grow">
            原文
            <input
              value={text}
              disabled={busy}
              onChange={(event) => {
                setText(event.target.value);
                change();
              }}
              autoFocus
            />
          </label>
        </div>
        <label>
          语境释义
          <textarea
            value={meaning}
            disabled={busy}
            rows={2}
            onChange={(event) => {
              setMeaning(event.target.value);
              change();
            }}
            placeholder="可以先保存原文，之后再补充"
          />
        </label>
        <label>
          原句
          <textarea
            value={context}
            readOnly={Boolean(seed.example)}
            disabled={busy}
            rows={3}
            onChange={(event) => {
              setContext(event.target.value);
              change();
            }}
            placeholder="没有自动上下文时可手动补充"
          />
        </label>
        {seed.example && (
          <p className="muted">
            原句来自已选对白；需要纠正文字或时间时，可先在预习面板中修改。
          </p>
        )}
        {seed.sourceTitle && (
          <p className="muted">
            来源：{seed.sourceTitle}。自动语境可在上方补充或修订。
          </p>
        )}
        {!seed.example && text.trim() && <SystemSpeech text={text} />}
        <OnlineLookup
          key={text}
          text={text}
          onMeaning={(value) => {
            setMeaning(value);
            change();
          }}
        />
        <button
          className="secondary"
          onClick={explain}
          disabled={busy || !text.trim()}
        >
          本地解释
        </button>
        {explanation && (
          <div className="suggestion">
            <strong>{explanation.meaning}</strong>
            <p>{explanation.translation}</p>
            <p className="muted">{explanation.notes}</p>
            <button
              className="text-button"
              onClick={() => {
                setMeaning(
                  kind === "sentence"
                    ? explanation.translation
                    : explanation.meaning,
                );
                change();
              }}
              disabled={busy}
            >
              采用这条释义
            </button>
          </div>
        )}
        {seed.example && (
          <label className="check-label">
            <input
              type="checkbox"
              checked={saveAudio}
              disabled={busy}
              onChange={(event) => setSaveAudio(event.target.checked)}
            />
            保存对应原声，之后复习时使用
          </label>
        )}
        {prepared && (
          <div className="merge-choice">
            <h3>保存到哪里</h3>
            <label className="check-label">
              <input
                type="radio"
                checked={target === "create"}
                onChange={() => setTarget("create")}
                disabled={busy}
              />
              新建词条
            </label>
            {prepared.matches.map((entry) => (
              <label className="check-label" key={entry.id}>
                <input
                  type="radio"
                  checked={target === entry.id}
                  onChange={() => setTarget(entry.id)}
                  disabled={busy}
                />
                <span>
                  合并到 <strong>{entry.text}</strong> ·{" "}
                  {entry.meanings.map((value) => value.text).join("；") ||
                    "暂无释义"}
                  <small>
                    保留已有 {entry.examples.length}{" "}
                    条例句和学习历史，追加本次资料
                  </small>
                </span>
              </label>
            ))}
          </div>
        )}
        {busy && (
          <div className="inline-status">
            {task?.message ?? "正在处理…"}
            {task && (
              <button className="text-button" onClick={cancelTask}>
                取消本次任务
              </button>
            )}
          </div>
        )}
        {error && (
          <p className="error" role="alert">
            {error}
          </p>
        )}
        <footer>
          <button className="secondary" onClick={onClose} disabled={busy}>
            暂不收录
          </button>
          {prepared ? (
            <>
              <button className="secondary" disabled={busy} onClick={prepare}>
                重新检查匹配
              </button>
              <button className="primary" disabled={busy} onClick={commit}>
                确认{target === "create" ? "收录" : "合并"}
              </button>
            </>
          ) : (
            <button
              className="primary"
              disabled={busy || !text.trim()}
              onClick={prepare}
            >
              检查已有词条
            </button>
          )}
        </footer>
      </section>
    </div>
  );
}
