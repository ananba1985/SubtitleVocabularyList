import { invoke, convertFileSrc, type InvokeArgs } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type { TaskSnapshot } from "./types";
import type { TaskHandle } from "./generated/ipc";
import { waitForTask } from "./taskUpdates";
import type {
  CommandArgs,
  CommandName,
  CommandResult,
  OptionalArgsCommand,
} from "./commands";
export { terminal } from "./taskUpdates";
export function call<K extends OptionalArgsCommand>(
  command: K,
): Promise<CommandResult<K>>;
export function call<K extends CommandName>(
  command: K,
  args: CommandArgs<K>,
): Promise<CommandResult<K>>;
export function call(
  command: CommandName,
  args?: InvokeArgs,
): Promise<unknown> {
  return invoke(command, args);
}
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
export function waitTask<T>(
  task: TaskHandle<T>,
  onProgress?: (task: TaskSnapshot) => void,
): Promise<T>;
export function waitTask(
  task: TaskSnapshot,
  onProgress?: (task: TaskSnapshot) => void,
): Promise<unknown>;
export async function waitTask(
  task: TaskSnapshot,
  onProgress?: (task: TaskSnapshot) => void,
): Promise<unknown> {
  return waitForTask<unknown>(
    task,
    () => call("task_get", { taskId: task.id }),
    (receive) =>
      listen<TaskSnapshot>("task_updated", (event) => receive(event.payload)),
    onProgress,
  );
}
