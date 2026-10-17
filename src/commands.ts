import type * as T from "./types";
import type * as A from "./assessmentTypes";
import type * as Q from "./queryTypes";
import type {
  PracticeDocument,
  PracticeProgress,
  PracticeState,
} from "./practice";

type Command<Args, Result> = { args: Args; result: Result };
type Read<Result> = Command<undefined, Result>;
type Page<T> = { items: T[]; total: number };
type Pagination = { offset?: number; limit?: number };
type Operation = { operationId: string };
type SessionVersion = { sessionId: string; expectedRevision: number };
type InputCommand<Input, Result> = Command<
  { input: Input & Operation },
  Result
>;

export type AssessmentReportReply =
  | { report: A.AssessmentReport; revisions: number[] }
  | {
      sessionId: string;
      state: A.AssessmentSession["state"];
      position: number;
      planned: number;
    };

export type TrayAction =
  | "library"
  | "collect"
  | "import"
  | "episodes"
  | "practice"
  | "reviews"
  | "leech"
  | "sync"
  | "tasks"
  | "settings";

// The public IPC catalog. Results are selected by the command, never by the caller.
// Keep names and top-level arguments aligned with desktop/commands/mod.rs (commands:check).
export interface CommandMap {
  // Application settings, task receipts and lifecycle.
  app_info: Read<T.AppInfo>;
  settings_get: Read<T.Settings>;
  settings_update: Command<{ settings: T.Settings }, T.Settings>;
  task_get: Command<{ taskId: string }, T.TaskSnapshot>;
  tasks_list: Read<T.TaskSnapshot[]>;
  tasks_history: Command<Pagination, T.TaskHistoryPage>;
  task_cancel: Command<{ taskId: string }, T.TaskSnapshot>;
  tray_action_get: Read<TrayAction | null>;
  app_quit: Read<void>;

  // Sources, candidates and known targets.
  sources_list: Read<T.SourceSummary[]>;
  import_start: Command<
    Operation & { paths: string[]; options?: T.ImportOptions | null },
    T.TaskSnapshot
  >;
  media_inspect: Command<{ path: string }, T.MediaInspection>;
  candidates_list: Command<
    Pagination & {
      sourceId: string;
      search?: string;
      kind?: string;
      onlyPending?: boolean;
      showKnown?: boolean;
    },
    T.Candidate[]
  >;
  candidate_examples: Command<
    { sourceId: string; key: string },
    T.CandidateExample[]
  >;
  candidate_decide: Command<
    { sourceId: string; key: string; exampleId: string; decision: string },
    void
  >;
  source_example_update: Command<
    {
      sourceId: string;
      exampleId: string;
      revision: number;
      text: string;
      startMs: number;
      endMs: number;
    },
    T.CandidateExample
  >;
  known_target_get: Command<{ kind: string; text: string }, boolean>;
  known_target_set: Command<
    { kind: string; text: string; known: boolean },
    void
  >;
  known_targets_list: Command<
    Pagination & { search?: string },
    T.KnownTargetPage
  >;

  // The unified vocabulary collection and persistence boundary.
  entries_list: Command<Pagination & { search?: string }, T.Entry[]>;
  entry_get: Command<{ entryId: string }, T.Entry>;
  entry_update: Command<
    {
      input: {
        id: string;
        expectedRevision: number;
        text: string;
        meanings: { id: string | null; text: string }[];
      };
    },
    T.Entry
  >;
  collection_prepare: Command<
    { input: T.CollectionInput },
    T.PreparedCollection
  >;
  collection_from_example: Command<
    Operation & {
      sourceId: string;
      exampleId: string;
      text: string;
      kind: string;
      meaning: string;
    },
    T.PreparedCollection
  >;
  collection_commit: Command<
    {
      draftId: string;
      targetEntryId?: string | null;
      expectedRevision?: number | null;
      saveAudio: boolean;
    },
    T.TaskSnapshot
  >;
  preview_start: Command<
    Operation & { sourceId: string; exampleId: string },
    T.TaskSnapshot
  >;
  media_path: Command<{ assetId: string }, string>;

  // Review, listening/reading and local speech.
  review_units: Command<
    { mode: string; dimension: string; offset: number; limit: number },
    T.ReviewUnit[]
  >;
  review_question: Command<
    { unitId: string; expectedRevision: number },
    T.ReviewQuestion
  >;
  review_hint: Command<{ questionId: string }, string>;
  review_submit: InputCommand<
    { questionId: string; answer: string; unable: boolean },
    T.ReviewAttempt
  >;
  review_correct: InputCommand<
    {
      attemptId: string;
      expectedRevision: number;
      outcome: T.ReviewOutcome;
      reason: string;
    },
    T.ReviewAttempt
  >;
  review_history: Command<
    { unitId?: string | null; offset: number; limit: number },
    T.ReviewAttempt[]
  >;
  review_audio_start: Command<
    Operation & { questionId: string },
    T.TaskSnapshot
  >;
  practice_get: Read<PracticeState | null>;
  practice_import: Command<{ document: PracticeDocument }, PracticeState>;
  practice_progress_save: Command<
    { documentId: string; progress: PracticeProgress },
    void
  >;
  practice_audio_save: Command<Uint8Array, string>;
  practice_example: Command<
    { documentId: string; chapter: number; sentence: number },
    T.CandidateExample
  >;
  speech_voices: Read<T.SystemVoice[]>;
  speech_start: Command<
    Operation & { text: string; voiceId?: string | null },
    T.TaskSnapshot
  >;

  // Query sessions, providers and local explanations.
  translation_open: Read<void>;
  translation_session_open: Command<
    { input: Q.QueryInput; sessionId?: string | null },
    Q.QuerySession
  >;
  translation_session_get: Command<
    { sessionId?: string | null },
    Q.QuerySession | null
  >;
  translation_query_start: Command<
    {
      sessionId: string;
      expectedInputRevision: number;
      providerIds: string[];
      automatic: boolean;
      persist: boolean;
    },
    Q.QuerySession
  >;
  translation_query_cancel: Command<
    { sessionId: string; queryId?: string | null },
    void
  >;
  translation_window_hide: Read<void>;
  translation_window_pin: Command<{ pinned: boolean }, void>;
  translation_collect: Command<
    { sessionId: string; inputRevision: number; providerId: string },
    void
  >;
  query_copy: Command<{ text: string }, void>;
  clipboard_listener_get: Read<Q.ClipboardStatus>;
  clipboard_listener_set: Command<
    {
      enabled: boolean;
      preferences: Pick<Q.ClipboardStatus, "debounceMs" | "persistResults">;
    },
    Q.ClipboardStatus
  >;
  providers_list: Read<Q.Provider[]>;
  providers_update: Command<
    {
      update: Pick<
        Q.Provider,
        | "id"
        | "enabled"
        | "autoQuery"
        | "sendContext"
        | "order"
        | "maxConcurrency"
        | "values"
      > & { secrets: Record<string, string> };
    },
    Q.Provider[]
  >;
  plugin_install: Command<{ path: string }, Q.Provider>;
  plugin_remove: Command<{ pluginId: string }, Q.Provider[]>;
  explanations_status: Read<{ state: string }>;
  explanations_prepare: Command<
    { sourceId?: string | null },
    { task: T.TaskSnapshot | null }
  >;
  explanation_get: Command<
    { text: string; context: string },
    T.Explanation | null
  >;
  explanation_import: Command<
    {
      text: string;
      context: string;
      value: T.Explanation;
      modelUrl: string;
      modelName: string;
    },
    T.Explanation
  >;
  explain_start: Command<
    Operation & { text: string; context: string },
    T.TaskSnapshot
  >;
  online_query_start: Command<
    Operation & { text: string; provider?: string | null },
    T.TaskSnapshot
  >;

  // Connection and complete vocabulary synchronization.
  connection_status: Read<T.ConnectionStatus>;
  connection_start: Command<Operation & { replace: boolean }, T.TaskSnapshot>;
  connection_check: Command<Operation, T.TaskSnapshot>;
  connection_open: Read<void>;
  sync_status: Read<T.SyncStatus>;
  sync_conflicts: Read<T.SyncConflict[]>;
  sync_start: Command<Operation, T.TaskSnapshot>;
  sync_resolve: Command<
    Operation & {
      input: {
        conflictId: string;
        choice: string;
        targetEntryId?: string | null;
        expectedRemoteRevision: number;
        expectedLocalRevision?: number | null;
        expectedTargetRevision?: number | null;
      };
    },
    T.TaskSnapshot
  >;

  // Native capture, OCR and selection replacement.
  native_status: Read<T.NativeStatus>;
  replacement_status_get: Read<Q.ReplacementStatus>;
  replacement_cancel: Command<Operation, void>;
  replacement_result_open: Read<void>;
  replacement_clipboard_restore: Read<void>;
  capture_selection: Read<T.TaskSnapshot>;
  capture_ocr: Read<T.TaskSnapshot>;
  capture_session_get: Command<{ sessionId: string }, T.ScreenSession>;
  capture_ocr_cancel: Command<{ sessionId: string }, void>;
  capture_ocr_submit: Command<
    {
      sessionId: string;
      rect: { x: number; y: number; width: number; height: number };
    },
    T.TaskSnapshot
  >;

  // Assessment inventory, sessions, reports and independent synchronization.
  assessment_preview: Command<
    { config: A.AssessmentConfig },
    A.AssessmentPreview
  >;
  assessment_start: InputCommand<
    { config: A.AssessmentConfig; expectedInventoryRevision: string },
    A.AssessmentSession
  >;
  assessment_session_get: Command<{ sessionId: string }, A.AssessmentSession>;
  assessment_session_list: Command<
    { unfinishedOnly: boolean; offset: number; limit: number },
    Page<A.AssessmentSessionSummary>
  >;
  assessment_answer_submit: InputCommand<
    SessionVersion & {
      itemId: string;
      action: string;
      optionId?: string | null;
      text: string;
      uncertain: boolean;
    },
    A.AssessmentSession
  >;
  assessment_session_transition: InputCommand<
    SessionVersion & {
      action: string;
      itemId?: string | null;
      draftText?: string;
      draftOption?: string | null;
    },
    A.AssessmentSession
  >;
  assessment_audio_start: InputCommand<
    SessionVersion & { itemId: string },
    A.AssessmentPlayback
  >;
  assessment_audio_event: InputCommand<
    {
      sessionId: string;
      itemId: string;
      playbackId: string;
      event: string;
      seconds: number;
      error?: string;
    },
    { playbackId: string; played: boolean; seconds: number; error: string }
  >;
  assessment_result_correct: InputCommand<
    {
      answerId: string;
      expectedRevision: number;
      result: string;
      addHint: boolean;
      reason: string;
    },
    {
      answerId: string;
      revision: number;
      sessionId: string;
      sessionRevision: number;
      correction: {
        id: string;
        answerId: string;
        result: string;
        addHint: boolean;
        reason: string;
        createdAt: number;
        revision: number;
      };
    }
  >;
  assessment_report_get: Command<
    { sessionId: string; reportRevision?: number | null },
    AssessmentReportReply
  >;
  assessment_report_items: Command<
    {
      sessionId: string;
      reportRevision: number;
      offset: number;
      limit: number;
    },
    Page<A.AssessmentReportItem>
  >;
  assessment_history_list: Command<
    { offset: number; limit: number; comparisonKey?: string | null },
    Page<A.AssessmentHistoryItem>
  >;
  assessment_compare: Command<
    { first: string; second: string },
    A.AssessmentComparison
  >;
  assessment_collection_prepare: InputCommand<
    { sessionId: string; itemIds: string[] },
    A.AssessmentCollectionDraft
  >;
  assessment_collection_commit: InputCommand<
    {
      draftId: string;
      items: {
        sessionItemId: string;
        targetEntryId?: string | null;
        expectedRevision?: number | null;
        meaning: string;
        translation: string;
        saveAudio: boolean;
      }[];
    },
    T.TaskSnapshot
  >;
  assessment_catalog_get: Read<A.AssessmentInventory>;
  assessment_settings_get: Read<A.AssessmentSettings>;
  assessment_settings_update: Command<
    Operation & { settings: A.AssessmentSettings },
    A.AssessmentSettings
  >;
  assessment_bank_prepare: InputCommand<
    {
      bankId?: string | null;
      ranges: Record<string, number>;
      questionTypes: A.AssessmentQuestionType[];
      limit: number;
    },
    T.TaskSnapshot
  >;
  assessment_bank_review: InputCommand<
    {
      bankId: string;
      expectedRevision: number;
      action: string;
      itemId?: string | null;
      expectedItemRevision?: number | null;
      content?: A.AssessmentDraft | null;
      note: string;
    },
    {
      bankId: string;
      state: string;
      revision: number;
      publishedItems?: number;
      itemId?: string;
      itemRevision?: number;
    }
  >;
  assessment_bank_items: Command<
    { bankId: string; state?: string | null; offset: number; limit: number },
    A.AssessmentBankPage
  >;
  assessment_bank_audio_prepare: InputCommand<
    { content: A.AssessmentDraft },
    T.TaskSnapshot
  >;
  assessment_sync_status: Read<A.AssessmentSyncStatus>;
  assessment_sync_conflicts: Read<A.AssessmentSyncConflict[]>;
  assessment_sync_keep_both: InputCommand<
    {
      sessionId: string;
      expectedLocalRevision: number;
      expectedRemoteRevision: number;
    },
    { sessionId: string; copySessionId: string; archiveId: string }
  >;
  assessment_audio_prepare: Command<
    Operation & { sessionId: string; itemId: string },
    T.TaskSnapshot
  >;
}

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
    input: Operation;
  }
    ? K
    : never;
}[Extract<CommandName, `assessment_${string}`>];
export type AssessmentMutationInput<K extends AssessmentMutationCommand> = Omit<
  CommandArgs<K>["input"],
  "operationId"
>;
