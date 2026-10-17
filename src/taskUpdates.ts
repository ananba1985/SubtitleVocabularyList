import type { TaskSnapshot } from "./types";

export const terminal = (task: TaskSnapshot) =>
  ["succeeded", "failed", "cancelled"].includes(task.state);

export type { TaskUpdate } from "./generated/ipc";
import type { TaskUpdate } from "./generated/ipc";

export function applyTaskUpdate(tasks: TaskSnapshot[], update: TaskUpdate) {
  const previous = tasks.find((task) => task.id === update.id);
  if (previous && (previous.updatedAt > update.updatedAt ||
    (terminal(previous) && !terminal(update)))) return tasks;
  const next = tasks.filter((task) => task.id !== update.id);
  if (update.visibleInList) next.push(update);
  next.sort((a, b) => b.updatedAt - a.updatedAt || b.id.localeCompare(a.id));
  let ended = false;
  return next.filter((task) => {
    if (!terminal(task)) return true;
    if (ended) return false;
    ended = true;
    return true;
  });
}

// Subscribe before reading the snapshot so completion during IPC setup is retained.
export function waitForTask<T>(
  initial: TaskSnapshot,
  read: () => Promise<TaskSnapshot>,
  subscribe: (receive: (task: TaskSnapshot) => void) => Promise<() => void>,
  onProgress?: (task: TaskSnapshot) => void,
): Promise<T> {
  return new Promise((resolve, reject) => {
    let current = initial;
    let ended = false;
    let reading = false;
    let stop: (() => void) | undefined;
    let timer: ReturnType<typeof setTimeout> | undefined;
    const visible = () => typeof document === "undefined" || !document.hidden;
    const cleanup = () => {
      ended = true;
      clearTimeout(timer);
      stop?.();
      if (typeof document !== "undefined")
        document.removeEventListener("visibilitychange", resume);
    };
    const fail = (error: unknown) => { cleanup(); reject(error); };
    const schedule = () => {
      clearTimeout(timer);
      if (!ended && visible()) timer = setTimeout(refresh, 10_000);
    };
    const receive = (next: TaskSnapshot) => {
      if (ended || next.id !== current.id || next.updatedAt < current.updatedAt) return;
      current = next;
      try { onProgress?.(current); } catch (error) { fail(error); return; }
      if (terminal(current)) {
        cleanup();
        if (current.state === "succeeded") resolve(current.result as T);
        else reject(new Error(current.error?.message ?? current.message));
      } else schedule();
    };
    async function refresh(force = false) {
      if (ended || reading || (!force && !visible())) return;
      reading = true;
      try { receive(await read()); }
      catch (error) { if (!ended) fail(error); }
      finally { reading = false; }
    }
    function resume() {
      clearTimeout(timer);
      if (visible()) void refresh(true);
    }
    receive(initial);
    if (ended) return;
    if (typeof document !== "undefined")
      document.addEventListener("visibilitychange", resume);
    subscribe(receive).then((unlisten) => {
      if (ended) unlisten();
      else { stop = unlisten; void refresh(true); }
    }).catch(() => { if (!ended) void refresh(true); });
  });
}
