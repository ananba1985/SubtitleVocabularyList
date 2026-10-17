import type { CommandMap } from "./generated/ipc";
export type {
  CommandMap,
  AssessmentReportReply,
  TrayNavigation as TrayAction,
} from "./generated/ipc";

export type CommandName = keyof CommandMap;
export type CommandArgs<K extends CommandName> = CommandMap[K]["args"];
export type CommandResult<K extends CommandName> = CommandMap[K]["result"];
export type OptionalArgsCommand = {
  [K in CommandName]: undefined extends CommandArgs<K>
    ? K
    : {} extends CommandArgs<K>
      ? K
      : never;
}[CommandName];

export type AssessmentMutationCommand = {
  [K in Extract<CommandName, `assessment_${string}`>]: CommandArgs<K> extends {
    input: { operationId: string };
  }
    ? K
    : never;
}[Extract<CommandName, `assessment_${string}`>];
export type AssessmentMutationInput<K extends AssessmentMutationCommand> = Omit<
  CommandArgs<K>["input"],
  "operationId"
>;
