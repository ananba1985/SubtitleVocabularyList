import type * as Native from "./generated/ipc";
export type * from "./assessmentTypes";

export type AppInfo = Native.AppInfo;
export type TaskSnapshot = Native.TaskSnapshot;
export type TaskHistoryPage = Native.TaskHistoryPage;
export type AudioAsset = Native.AudioAsset;
export type Example = Native.Example;
export type Entry = Native.Entry;
export type SourceSummary = Native.SourceSummary;
export type ImportOptions = Native.ImportOptions;
export type MediaInspection = Native.MediaInspection;
export type Candidate = Native.Candidate;
export type KnownTarget = Native.KnownTarget;
export type KnownTargetPage = Native.KnownTargetPage;
export type CandidateExample = Native.CandidateExample;
export type ExampleInput = Native.ExampleInput;
export type CollectionInput = Native.CollectionInput;
export type PreparedCollection = Native.PreparedCollection;
export type Explanation = Native.Explanation &
  Partial<Pick<Native.PreparedExplanation, "meaningScope">>;
export type Settings = Native.Settings;
export interface CollectionSeed {
  draftId?: string;
  inputId?: string;
  operationId?: string;
  saveAudio?: boolean;
  imageSource?: { id: string; uri: string };
  image?: Native.CaptureImage;
  imageCrop?: Native.PixelRect;
  ocrText?: string;
  frameRange?: Native.WatchClipRange;
  saveFrameAudio?: boolean;
  videoUri?: string;
  alreadyKnown?: boolean;
  meaning?: string;
  text: string;
  kind: string;
  context: string;
  sourceId?: string;
  example?: Pick<CandidateExample, "id" | "text" | "startMs" | "endMs"> &
    Partial<CandidateExample>;
  sourceTitle?: string;
  locationKey?: string;
}
export type SystemVoice = Native.SystemVoice;
export type NativeStatus = Native.NativeStatus;
export type ScreenSession = Native.ScreenSessionView;
export type ReviewOutcome = Native.Outcome;
export type ReviewState = Native.ReviewState;
export type ReviewUnit = Native.ReviewUnit;
export type ReviewQuestion = Native.Question;
export type ReviewAttempt = Native.Attempt;
export type ConnectionStatus = Native.ConnectionStatus;
export type SyncStatus = Native.SyncStatus;
export type SyncResult = Native.SyncResult & {
  assessment?: import("./assessmentTypes").AssessmentSyncStatus;
};
export type SyncBundle = Native.SyncBundleView;
export type SyncConflict = Native.SyncConflictView;
