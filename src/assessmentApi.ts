import { useRef } from "react";
import { call, uid } from "./api";
import type {
  AssessmentMutationCommand,
  AssessmentMutationInput,
  CommandArgs,
  CommandResult,
} from "./commands";

// Retain the same receipt after transport failure; a successful explicit action starts a new intent.
export function useAssessmentMutation() {
  const pending = useRef<{ signature: string; id: string } | null>(null);
  return async function mutate<K extends AssessmentMutationCommand>(
    command: K,
    input: AssessmentMutationInput<K>,
  ): Promise<CommandResult<K>> {
    const signature = JSON.stringify({ command, input });
    if (pending.current?.signature !== signature)
      pending.current = { signature, id: uid() };
    // Restore the one receipt field omitted from the caller's command-specific input.
    const value = await call(command, {
      input: { ...input, operationId: pending.current.id },
    } as CommandArgs<K>);
    pending.current = null;
    return value;
  };
}
