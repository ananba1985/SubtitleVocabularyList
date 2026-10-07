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
  systemVoice: string;
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
  error: string | null;
}
