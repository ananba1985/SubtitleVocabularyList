export interface AppInfo {
  dataDirectory: string;
  schemaVersion: number;
  entryCount: number;
  sourceCount: number;
}
export interface TaskSnapshot {
  id: string;
  operationId: string;
  kind: string;
  stage: string;
  state: string;
  current: number;
  total: number;
  message: string;
  result: unknown;
  error: { code: string; message: string } | null;
  createdAt: number;
  updatedAt: number;
}
export interface AudioAsset {
  id: string;
  relativePath: string;
  durationMs: number;
  kind: string;
  state: string;
}
export interface Example {
  id: string;
  text: string;
  contextMeaning: string;
  contexts: {
    scopeKey: string;
    meaningId: string | null;
    contextMeaning: string;
  }[];
  sourceId: string | null;
  sourceTitle: string | null;
  startMs: number | null;
  endMs: number | null;
  audio: AudioAsset[];
}
export interface Entry {
  id: string;
  kind: string;
  text: string;
  matchKey: string;
  revision: number;
  collectionCount: number;
  occurrenceCount: number;
  meanings: { id: string; text: string; origin: string }[];
  examples: Example[];
}
export interface SourceSummary {
  id: string;
  title: string;
  durationMs: number;
  textSource: string;
  exampleCount: number;
  candidateCount: number;
  importedAt: number;
}
export interface ImportOptions {
  audioStream: number | null;
  subtitleStream: number | null;
  subtitleMode: "auto" | "embedded" | "external" | "speech";
  externalSubtitle: string | null;
}
export interface MediaInspection {
  audioTracks: {
    index: number;
    codec_name: string;
    tags: Record<string, string>;
  }[];
  subtitleTracks: {
    index: number;
    codec_name: string;
    tags: Record<string, string>;
  }[];
  externalSubtitle: string | null;
}
export interface Candidate {
  key: string;
  text: string;
  kind: string;
  count: number;
  handledCount: number;
  exampleCount: number;
  existingEntryCount: number;
}
export interface CandidateExample {
  id: string;
  text: string;
  startMs: number;
  endMs: number;
  decision: string | null;
  revision: number;
}
export interface ExampleInput {
  text: string;
  contextMeaning: string;
  sourceId: string | null;
  locationKey: string;
  startMs: number | null;
  endMs: number | null;
  mediaAssetIds: string[];
}
export interface CollectionInput {
  operationId: string;
  kind: string;
  text: string;
  meaning: string;
  examples: ExampleInput[];
  targetEntryId: string | null;
  expectedRevision: number | null;
}
export interface PreparedCollection {
  draftId: string;
  matches: Entry[];
  input: CollectionInput;
}
export interface Explanation {
  meaning: string;
  translation: string;
  notes: string;
}
export interface Settings {
  modelUrl: string;
  modelName: string;
  offlineMode: boolean;
  selectionShortcut: string;
  ocrShortcut: string;
  systemVoice: string;
  siteUrl: string;
  tools: {
    ffmpeg: string;
    ffprobe: string;
    tesseract: string;
    whisper: string;
    whisperModel: string;
  };
}
export interface CollectionSeed {
  text: string;
  kind: string;
  context: string;
  sourceId?: string;
  example?: CandidateExample;
  sourceTitle?: string;
  locationKey?: string;
}
export interface SystemVoice {
  id: string;
  name: string;
}
export interface NativeStatus {
  selectionRegistered: boolean;
  ocrRegistered: boolean;
  error: string | null;
}
export interface ScreenSession {
  id: string;
  bounds: { x: number; y: number; width: number; height: number };
  imagePath: string;
}
export type ReviewOutcome =
  "correct" | "correct_with_hint" | "incorrect" | "needs_confirmation";
export interface ReviewState {
  level: string;
  streak: number;
  lapses: number;
  intervalMs: number;
  dueAt: number;
  lastReviewedAt: number;
  recent: ReviewOutcome[];
  leechActive: boolean;
  leechStartedAt: number;
  leechClearedAt: number;
}
export interface ReviewUnit {
  id: string;
  entryId: string;
  text: string;
  kind: string;
  scope: string;
  dimension: "meaning" | "listening";
  state: ReviewState;
  revision: number;
  reasons: string[];
  available: boolean;
}
export interface ReviewQuestion {
  id: string;
  unitId: string;
  dimension: "meaning" | "listening";
  prompt: string;
  context: string;
  audioKind: string;
}
export interface ReviewAttempt {
  id: string;
  unitId: string;
  question: {
    entryId: string;
    target: string;
    dimension: "meaning" | "listening";
    expected: string[];
    context: string;
    assetId: string | null;
    audioKind: string;
  };
  answer: string;
  hinted: boolean;
  originalOutcome: ReviewOutcome;
  outcome: ReviewOutcome;
  grader: string;
  revision: number;
  createdAt: number;
  state: ReviewState;
  corrections: { outcome: ReviewOutcome; reason: string; createdAt: number }[];
}
export interface ConnectionStatus {
  siteUrl: string;
  state: string;
  offline: boolean;
  deviceName: string;
  requestId: string | null;
  displayCode: string | null;
  authorizationUrl: string | null;
  accountScope: string | null;
  expiresAt: number;
}
export interface SyncStatus {
  cursor: number;
  pending: number;
  conflicts: number;
}
export interface SyncResult {
  pulled: number;
  pushed: number;
  conflicts: number;
  cursor: number;
}
export interface SyncBundle {
  schemaVersion: number;
  deleted: boolean;
  entry: { id: string; kind: string; text: string; revision: number };
  tables: Record<string, Record<string, unknown>[]>;
}
export interface SyncConflict {
  id: string;
  remoteId: string;
  localId: string | null;
  text: string;
  reason: string;
  remoteRevision: number;
  local: SyncBundle | null;
  remote: SyncBundle;
  candidates: Entry[];
}
