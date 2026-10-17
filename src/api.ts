import { invoke, convertFileSrc } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type { TaskSnapshot } from "./types";
import { waitForTask } from "./taskUpdates";
export { terminal } from "./taskUpdates";
export const call = <T>(command: string, args?: Record<string, unknown>) =>
  invoke<T>(command, args);
export function message(error: unknown): string {
  if (error && typeof error === "object" && "message" in error)
    return String(error.message);
  return String(error);
}
export const uid = () => crypto.randomUUID();
export const audioUrl = (path: string) => convertFileSrc(path);
export function time(ms: number): string {
  const seconds = Math.floor(ms / 1000);
  return `${Math.floor(seconds / 60)}:${String(seconds % 60).padStart(2, "0")}`;
}
export async function waitTask<T>(
  task: TaskSnapshot,
  onProgress?: (task: TaskSnapshot) => void,
): Promise<T> {
  return waitForTask<T>(task,
    () => call<TaskSnapshot>("task_get", { taskId: task.id }),
    (receive) => listen<TaskSnapshot>("task_updated", (event) => receive(event.payload)),
    onProgress);
}
