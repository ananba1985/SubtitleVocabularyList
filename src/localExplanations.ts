import { useEffect, useSyncExternalStore } from "react";
import { call } from "./api";
import type { Explanation } from "./types";

type Snapshot = {
  state: "idle" | "loading" | "waiting" | "ready" | "error";
  value?: Explanation;
  error?: string;
};
type Entry = {
  key: string;
  text: string;
  context: string;
  snapshot: Snapshot;
  listeners: Set<() => void>;
  reading: boolean;
};
const idle: Snapshot = { state: "idle" };
const storageKey = "svl.local-explanations.v1";
type Legacy = {
  value: Explanation;
  modelUrl: string;
  modelName: string;
  oldKey: string;
};
type Runtime = {
  entries: Map<string, Entry>;
  legacy: Map<string, Legacy>;
  loaded: boolean;
  status?: { state: string; expires: number };
  statusPromise?: Promise<string>;
};
const runtime: Runtime = import.meta.hot?.data.preparedExplanations ?? {
  entries: new Map(),
  legacy: new Map(),
  loaded: false,
};
const { entries, legacy } = runtime;
if (import.meta.hot)
  import.meta.hot.dispose((data) => {
    data.preparedExplanations = runtime;
  });
const chinese = /[\u3400-\u9fff]/;

function valid(value: Explanation, context: string) {
  return (
    value &&
    typeof value.meaning === "string" &&
    typeof value.translation === "string" &&
    typeof value.notes === "string" &&
    [value.meaning, value.translation, value.notes].every(
      (part) => part.length <= 16000,
    ) &&
    chinese.test(value.meaning) &&
    (!context.trim() || chinese.test(value.translation))
  );
}
function loadLegacy() {
  if (runtime.loaded) return;
  runtime.loaded = true;
  try {
    const values = JSON.parse(localStorage.getItem(storageKey) ?? "[]");
    if (Array.isArray(values))
      for (const record of values) {
        try {
          const [oldKey, value] = record;
          const [model, text, context] = JSON.parse(oldKey);
          const [modelUrl, modelName] = JSON.parse(model);
          if (
            [text, context, modelUrl, modelName].every(
              (part) => typeof part === "string",
            ) &&
            text.length <= 4000 &&
            context.length <= 20000 &&
            modelUrl.length <= 4000 &&
            modelName.length <= 1000 &&
            valid(value, context)
          )
            legacy.set(JSON.stringify([text.trim(), context]), {
              value,
              modelUrl,
              modelName,
              oldKey,
            });
        } catch {
          /* Skip malformed legacy records individually. */
        }
      }
  } catch {
    /* A malformed legacy cache must not block database reads. */
  }
}
function publish(entry: Entry, snapshot: Snapshot) {
  entry.snapshot = snapshot;
  entry.listeners.forEach((listener) => listener());
}
function migrated(entry: Entry, record: Legacy) {
  legacy.delete(entry.key);
  try {
    const values = JSON.parse(localStorage.getItem(storageKey) ?? "[]");
    if (Array.isArray(values))
      localStorage.setItem(
        storageKey,
        JSON.stringify(values.filter((item) => item?.[0] !== record.oldKey)),
      );
  } catch {
    /* The database has committed; a leftover legacy copy is harmless. */
  }
}
async function preparationState() {
  if (runtime.status && runtime.status.expires > Date.now())
    return runtime.status.state;
  if (!runtime.statusPromise) {
    runtime.statusPromise = call<{ state: string }>("explanations_status")
      .then((value) => {
        runtime.status = { state: value.state, expires: Date.now() + 1000 };
        return value.state;
      })
      .finally(() => {
        runtime.statusPromise = undefined;
      });
  }
  return runtime.statusPromise;
}
async function read(entry: Entry) {
  if (entry.reading || entry.snapshot.state === "ready") return;
  entry.reading = true;
  if (entry.snapshot.state === "idle") publish(entry, { state: "loading" });
  try {
    let value = await call<Explanation | null>("explanation_get", {
      text: entry.text,
      context: entry.context,
    });
    const record = legacy.get(entry.key);
    if (!value && record)
      value = await call<Explanation>("explanation_import", {
        text: entry.text,
        context: entry.context,
        value: record.value,
        modelUrl: record.modelUrl,
        modelName: record.modelName,
      });
    if (value) {
      if (record) migrated(entry, record);
      publish(entry, { state: "ready", value });
      return;
    }
    const status = await preparationState();
    if (status === "running") publish(entry, { state: "waiting" });
    else
      publish(entry, {
        state: "error",
        error:
          status === "paused"
            ? "后台准备已暂停，可继续准备中文资料。"
            : status === "failed"
              ? "后台准备因错误停止，请查看后台任务并重试。"
              : "这项中文解释尚未生成，可继续后台准备以补充。",
      });
  } catch {
    publish(entry, {
      state: "error",
      error: "本地解释资料暂时无法读取，请重试。",
    });
  } finally {
    entry.reading = false;
  }
}
async function retry(entry: Entry) {
  try {
    await call("explanations_prepare");
    await read(entry);
  } catch {
    publish(entry, {
      state: "error",
      error: "后台准备暂时无法启动，请到后台任务查看或重试。",
    });
  }
}

export function useLocalExplanation(
  text: string,
  context: string,
  _model: string,
  active = true,
  _priority = false,
) {
  loadLegacy();
  const key = JSON.stringify([text.trim(), context]);
  let entry = entries.get(key);
  if (!entry) {
    entry = {
      key,
      text: text.trim(),
      context,
      reading: false,
      listeners: new Set(),
      snapshot: idle,
    };
    entries.set(key, entry);
  }
  const current = entry;
  const snapshot = useSyncExternalStore(
    (listener) => {
      if (!active) return () => {};
      current.listeners.add(listener);
      return () => {
        current.listeners.delete(listener);
      };
    },
    () => current.snapshot,
  );
  useEffect(() => {
    if (!active || !current.text || snapshot.state === "ready") return;
    void read(current);
    const timer = setInterval(() => void read(current), 1500);
    return () => clearInterval(timer);
  }, [current, active, snapshot.state === "ready"]);
  return { ...snapshot, retry: () => void retry(current) };
}
