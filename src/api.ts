import { invoke, convertFileSrc } from "@tauri-apps/api/core";
import type { TaskSnapshot } from "./types";
export const call = <T>(command: string, args?: Record<string, unknown>) =>
  invoke<T>(command, args);
export function message(error: unknown): string {
  if (error && typeof error === "object" && "message" in error)
    return String(error.message);
  return String(error);
}
export const terminal = (task: TaskSnapshot) =>
  ["succeeded", "failed", "cancelled"].includes(task.state);
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
  let current = task;
  while (!terminal(current)) {
    onProgress?.(current);
    await new Promise((resolve) => setTimeout(resolve, 350));
    current = await call<TaskSnapshot>("task_get", { taskId: current.id });
  }
  onProgress?.(current);
  if (current.state !== "succeeded")
    throw new Error(current.error?.message ?? current.message);
  return current.result as T;
}
