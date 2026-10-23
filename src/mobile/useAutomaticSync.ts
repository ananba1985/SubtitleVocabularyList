import { useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { call, message, terminal, uid, waitTask } from "../api";
import type { TaskUpdate } from "../taskUpdates";

/** Foreground only, opt in, coalesced saves. The Rust engine owns all exchanges. */
export function useAutomaticSync(ready: boolean, changed: () => void) {
  const trigger = useRef(() => {});
  const onChanged = useRef(changed);
  onChanged.current = changed;
  const [notice, setNotice] = useState("");
  useEffect(() => {
    if (!ready) return;
    let disposed = false, running = false, requested = false;
    let timer: ReturnType<typeof setTimeout> | undefined;
    const unlisten: Array<() => void> = [];
    function request() {
      requested = true;
      clearTimeout(timer);
      if (!running && !disposed) timer = setTimeout(() => void drain(), 300);
    }
    async function drain() {
      if (disposed || running || !requested || document.hidden) return;
      requested = false;
      running = true;
      let started = false;
      try {
        const [device, settings, tasks] = await Promise.all([
          call("device_sync_get"), call("settings_get"), call("tasks_list"),
        ]);
        if (disposed || !device.preferences.autoSync || settings.offlineMode || !device.currentAccount ||
          device.bindingRequired || !device.network.connected || !device.network.foreground) return;
        if (tasks.some((task) => ["sync", "sync_resolve"].includes(task.kind) && !terminal(task))) {
          requested = true;
          return; // The terminal event retries only when a save/foreground request is pending.
        }
        started = true;
        const task = await call("sync_start", { operationId: uid() });
        await waitTask(task);
        if (!disposed) { setNotice(""); onChanged.current(); }
      } catch (error) {
        if (!disposed) setNotice(`自动同步暂未完成：${message(error)}`);
      } finally {
        running = false;
        if (started && requested && !disposed) request();
      }
    }
    trigger.current = request;
    const subscribe = <T,>(name: string, receive: (payload: T) => void) => {
      listen<T>(name, ({ payload }) => { if (!disposed) receive(payload); })
        .then((stop) => { if (disposed) stop(); else unlisten.push(stop); })
        .catch((error) => { if (!disposed) setNotice(message(error)); });
    };
    subscribe("mobile_resumed", request);
    subscribe<TaskUpdate>("task_updated", (task) => {
      if (task.kind === "collection" && task.state === "succeeded") request();
      else if (["sync", "sync_resolve"].includes(task.kind) && terminal(task) && requested) request();
    });
    const visible = () => { if (!document.hidden) request(); };
    document.addEventListener("visibilitychange", visible);
    window.addEventListener("online", request);
    request();
    return () => {
      disposed = true; clearTimeout(timer); unlisten.forEach((stop) => stop());
      document.removeEventListener("visibilitychange", visible);
      window.removeEventListener("online", request);
      trigger.current = () => {};
    };
  }, [ready]);
  return { request: () => trigger.current(), notice };
}
