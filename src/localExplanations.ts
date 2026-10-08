import { useEffect, useSyncExternalStore } from "react";
import { call, uid, waitTask } from "./api";
import type { Explanation, TaskSnapshot } from "./types";

type Snapshot = {
  state: "idle" | "loading" | "ready" | "error";
  value?: Explanation;
  error?: string;
};
type Entry = {
  key: string;
  model: string;
  text: string;
  context: string;
  snapshot: Snapshot;
  listeners: Set<() => void>;
  priority: boolean;
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
  unavailable: Set<string>;
  queue: Entry[];
  loaded: boolean;
  working: boolean;
};
const runtime: Runtime = import.meta.hot?.data.databaseExplanations ?? {
  entries: new Map(),
  legacy: new Map(),
  unavailable: new Set(),
  queue: [],
  loaded: false,
  working: false,
};
const { entries, legacy, unavailable, queue } = runtime;
if (import.meta.hot)
  import.meta.hot.dispose((data) => {
    data.databaseExplanations = runtime;
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
async function read(entry: Entry) {
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
  } catch {
    publish(entry, {
      state: "error",
      error: "本地解释资料暂时无法读取，请重试。",
    });
    return;
  }
  if (!entry.listeners.size) {
    publish(entry, idle);
    return;
  }
  if (entry.priority) queue.unshift(entry);
  else queue.push(entry);
  void pump();
}
async function pump() {
  if (runtime.working) return;
  runtime.working = true;
  try {
    while (queue.length) {
      const entry = queue.shift()!;
      if (!entry.listeners.size) {
        publish(entry, idle);
        continue;
      }
      if (!entry.model || unavailable.has(entry.model)) {
        publish(entry, {
          state: "error",
          error: "本地模型暂时不可用，可稍后重试。",
        });
        continue;
      }
      let task: TaskSnapshot | undefined;
      try {
        task = await call<TaskSnapshot>("explain_start", {
          text: entry.text,
          context: entry.context,
          operationId: uid(),
        });
        const value = await waitTask<Explanation>(task, (progress) => {
          task = progress;
        });
        if (!valid(value, entry.context))
          throw new Error("Missing Chinese explanation");
        publish(entry, { state: "ready", value });
      } catch {
        if (task?.error?.code === "provider_unavailable")
          unavailable.add(entry.model);
        publish(entry, {
          state: "error",
          error: unavailable.has(entry.model)
            ? "本地模型暂时不可用，可稍后重试。"
            : "暂未获得中文解释，请重试。",
        });
      }
    }
  } finally {
    runtime.working = false;
  }
}
function request(entry: Entry, retry = false, priority = false) {
  if (retry) {
    unavailable.delete(entry.model);
    for (const current of entries.values())
      if (current.model === entry.model && current.snapshot.state === "error")
        publish(current, idle);
  }
  if (entry.snapshot.state === "ready" && !retry) return;
  if (entry.snapshot.state === "loading") {
    if (priority) entry.priority = true;
    const index = queue.indexOf(entry);
    if (priority && index > 0) {
      entry.priority = true;
      queue.splice(index, 1);
      queue.unshift(entry);
    }
    return;
  }
  entry.priority = priority;
  publish(entry, { state: "loading" });
  void read(entry);
}

export function useLocalExplanation(
  text: string,
  context: string,
  model: string,
  active = true,
  priority = false,
) {
  loadLegacy();
  const key = JSON.stringify([text.trim(), context]);
  let entry = entries.get(key);
  if (!entry) {
    entry = {
      key,
      model,
      text: text.trim(),
      context,
      priority,
      listeners: new Set(),
      snapshot: idle,
    };
    entries.set(key, entry);
  }
  entry.model = model;
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
    if (
      active &&
      current.text &&
      (snapshot.state === "idle" || (snapshot.state === "loading" && priority))
    )
      request(current, false, priority);
  }, [current, active, model, priority, snapshot.state]);
  return { ...snapshot, retry: () => request(current, true, priority) };
}
