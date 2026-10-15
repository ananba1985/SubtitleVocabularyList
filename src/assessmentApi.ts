import { useRef } from "react";
import { call, uid } from "./api";

// Retain the same receipt after transport failure; a successful explicit action starts a new intent.
export function useAssessmentMutation() {
  const pending = useRef<{ signature: string; id: string } | null>(null);
  return async function mutate<T>(
    command: string,
    input: Record<string, unknown>,
  ): Promise<T> {
    const signature = JSON.stringify({ command, input });
    if (pending.current?.signature !== signature)
      pending.current = { signature, id: uid() };
    const value = await call<T>(command, {
      input: { ...input, operationId: pending.current.id },
    });
    pending.current = null;
    return value;
  };
}
