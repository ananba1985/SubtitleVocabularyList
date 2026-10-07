import { useState } from "react";
import { call, message } from "../api";
import type { Entry } from "../types";
export function EntryEditor({
  entry,
  onClose,
  onSaved,
}: {
  entry: Entry;
  onClose: () => void;
  onSaved: (entry: Entry) => void;
}) {
  const [text, setText] = useState(entry.text),
    [meanings, setMeanings] = useState<{ id: string | null; text: string }[]>(
      entry.meanings.map((value) => ({ id: value.id, text: value.text })),
    ),
    [error, setError] = useState(""),
    [busy, setBusy] = useState(false);
  async function save() {
    setBusy(true);
    setError("");
    try {
      const updated = await call<Entry>("entry_update", {
        input: {
          id: entry.id,
          expectedRevision: entry.revision,
          text,
          meanings,
        },
      });
      onSaved(updated);
    } catch (error) {
      setError(message(error));
    } finally {
      setBusy(false);
    }
  }
  return (
    <div className="modal-backdrop">
      <section
        className="modal"
        role="dialog"
        aria-modal="true"
        aria-label="修改词条"
      >
        <header>
          <h2>修改词条</h2>
          <button onClick={onClose} disabled={busy} aria-label="关闭修改窗口">
            ×
          </button>
        </header>
        <label>
          原文
          <input
            value={text}
            disabled={busy}
            onChange={(event) => setText(event.target.value)}
          />
        </label>
        {meanings.map((value, index) => (
          <label key={value.id ?? `new-${index}`}>
            释义 {index + 1}
            <textarea
              value={value.text}
              disabled={busy}
              onChange={(event) =>
                setMeanings((values) =>
                  values.map((item, position) =>
                    position === index
                      ? { ...item, text: event.target.value }
                      : item,
                  ),
                )
              }
            />
          </label>
        ))}
        <button
          className="secondary"
          disabled={busy}
          onClick={() =>
            setMeanings((values) => [...values, { id: null, text: "" }])
          }
        >
          增加释义
        </button>
        <p className="muted">已有例句、原声和学习记录保留。</p>
        {error && (
          <p className="error" role="alert">
            {error}
          </p>
        )}
        <footer>
          <button onClick={onClose} disabled={busy}>
            取消
          </button>
          <button
            className="primary"
            onClick={save}
            disabled={
              busy ||
              !text.trim() ||
              meanings.some((value) => !value.text.trim())
            }
          >
            保存修改
          </button>
        </footer>
      </section>
    </div>
  );
}
