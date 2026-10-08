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
type Runtime = {
  entries: Map<string, Entry>;
  saved: Map<string, Explanation>;
  unavailable: Set<string>;
  queue: Entry[];
  loaded: boolean;
  working: boolean;
};
const runtime: Runtime = import.meta.hot?.data.localExplanations ?? {
  entries: new Map(),
  saved: new Map(),
  unavailable: new Set(),
  queue: [],
  loaded: false,
  working: false,
};
const { entries, saved, unavailable, queue } = runtime;
if (import.meta.hot)
  import.meta.hot.dispose((data) => {
    data.localExplanations = runtime;
  });
const chinese = /[\u3400-\u9fff]/;

function valid(value: Explanation, context: string) {
  return (
    value &&
    typeof value.meaning === "string" &&
    typeof value.translation === "string" &&
    typeof value.notes === "string" &&
    chinese.test(value.meaning) &&
    (!context.trim() || chinese.test(value.translation))
  );
}
function load() {
  if (runtime.loaded) return;
  runtime.loaded = true;
  try {
    const values = JSON.parse(localStorage.getItem(storageKey) ?? "[]");
    if (Array.isArray(values))
      for (const [key, value] of values.slice(-500)) {
        const parts = JSON.parse(key);
        if (Array.isArray(parts) && valid(value, parts[2] ?? ""))
          saved.set(key, value);
      }
  } catch {
    /* Preview caching must not prevent local learning. */
  }
}
function publish(entry: Entry, snapshot: Snapshot) {
  entry.snapshot = snapshot;
  entry.listeners.forEach((listener) => listener());
}
function save(entry: Entry, value: Explanation) {
  saved.delete(entry.key);
  saved.set(entry.key, value);
  while (saved.size > 500) {
    const key = saved.keys().next().value!;
    saved.delete(key);
    const previous = entries.get(key);
    if (previous?.snapshot.state === "ready" && !previous.listeners.size)
      entries.delete(key);
  }
  try {
    localStorage.setItem(storageKey, JSON.stringify([...saved]));
  } catch {
    /* Keep the in-memory result. */
  }
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
      if (unavailable.has(entry.model)) {
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
        save(entry, value);
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
  if (entry.priority) queue.unshift(entry);
  else queue.push(entry);
  void pump();
}

export function useLocalExplanation(
  text: string,
  context: string,
  model: string,
  active = true,
  priority = false,
) {
  load();
  const key = JSON.stringify([model, text.trim(), context]);
  let entry = entries.get(key);
  if (!entry) {
    entry = {
      key,
      model,
      text: text.trim(),
      context,
      priority,
      listeners: new Set(),
      snapshot: saved.has(key)
        ? { state: "ready", value: saved.get(key)! }
        : idle,
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
    if (
      active &&
      current.text &&
      model &&
      (snapshot.state === "idle" || (snapshot.state === "loading" && priority))
    )
      request(current, false, priority);
  }, [current, active, model, priority, snapshot.state]);
  return { ...snapshot, retry: () => request(current, true, priority) };
}
