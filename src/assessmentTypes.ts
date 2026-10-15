export type AssessmentQuestionType = "recognition" | "cloze" | "listening";
export interface AssessmentConfig {
  mode: "quick" | "specified";
  wordTopN: number;
  phraseTopN: number;
  questionCount: number;
  content: "word" | "phrase" | "mixed";
  questionTypes: AssessmentQuestionType[];
  bandSize: number;
  avoidRecentDays: number;
  minimumSample: number;
}
export interface AssessmentSettings {
  revision: number;
  config: AssessmentConfig;
}
export interface FrequencyCatalog {
  id: string;
  kind: "word" | "phrase";
  title: string;
  sourceUrl: string;
  sourceVersion: string;
  sourceSha256: string;
  rankUnit: string;
  rawRows: number;
  maxRank: number;
  notice: string;
  eligibleTargets: number;
}
export interface AssessmentBank {
  id: string;
  state: "draft" | "published";
  revision: number;
  createdAt: number;
  publishedAt: number | null;
  itemCounts: Record<string, number>;
  targetCount: number;
  sources: FrequencyCatalog[];
  policyVersion: string;
}
export interface AssessmentInventory {
  policyVersion: string;
  catalogs: FrequencyCatalog[];
  banks: AssessmentBank[];
  indexReady: boolean;
}
export interface AssessmentChoice {
  id: string;
  text: string;
}
export interface AssessmentAudio {
  kind: "system" | "original";
  relativePath: string;
  sha256: string;
  text: string;
  voiceId: string;
  assetId: string | null;
}
export interface AssessmentDraft {
  target: string;
  meaning: string;
  sentence: string;
  translation: string;
  prompt: string;
  choices: AssessmentChoice[];
  correctOptionId: string | null;
  acceptedAnswers: string[];
  explanation: string;
  audio: AssessmentAudio | null;
  origin: string;
}
export interface AssessmentBankItem {
  id: string;
  bankId: string;
  catalogId: string;
  targetRank: number;
  targetKey: string;
  lexemeKey: string;
  kind: string;
  senseKey: string;
  questionType: AssessmentQuestionType;
  state: string;
  revision: number;
  content: AssessmentDraft;
  reviewNote: string;
  reviewedAt: number | null;
}
export interface AssessmentBankPage {
  items: AssessmentBankItem[];
  total: number;
}
export interface AssessmentQuota {
  catalogId: string;
  content: string;
  fromRank: number;
  toRank: number;
  questionType: AssessmentQuestionType;
  planned: number;
  available: number;
  unfilled: number;
}
export interface AssessmentPreview {
  inventoryRevision: string;
  canStart: boolean;
  policyVersion: string;
  catalogs: FrequencyCatalog[];
  quotas: AssessmentQuota[];
  coverage: {
    catalogId: string;
    topN: number;
    rawTargets: number;
    lexemeTargets: number;
    preparedTargets: number;
    missingTargets: number;
  }[];
  warnings: string[];
  repeatCount: number;
  missingSlots: number;
}
export interface AssessmentQuestion {
  id: string;
  position: number;
  questionType: AssessmentQuestionType;
  prompt: string;
  choices: AssessmentChoice[];
  audioKind: string | null;
  audioPlayed: boolean;
  hinted: boolean;
  hint: string | null;
  draftText: string;
  draftOption: string | null;
}
export interface AssessmentFeedback {
  answerId: string;
  answerRevision: number;
  sessionItemId: string;
  questionType: string;
  target: string;
  sentence: string;
  meaning: string;
  translation: string;
  explanation: string;
  acceptedAnswers: string[];
  result: string;
  unscoredReason: string | null;
  uncertain: boolean;
  hinted: boolean;
}
export interface AssessmentSession {
  id: string;
  state: "active" | "paused" | "completed" | "ended_early";
  revision: number;
  position: number;
  planned: number;
  processed: number;
  config: AssessmentConfig;
  createdAt: number;
  updatedAt: number;
  question: AssessmentQuestion | null;
  lastFeedback: AssessmentFeedback | null;
  warnings: string[];
  policyVersion: string;
}
export interface AssessmentSessionSummary {
  id: string;
  state: AssessmentSession["state"];
  revision: number;
  position: number;
  config: AssessmentConfig;
  createdAt: number;
  updatedAt: number;
}
export interface AssessmentPlayback {
  playbackId: string;
  path: string | null;
  kind: string;
  error: { code: string; message: string; retryable: boolean } | null;
}
export const assessmentTypeNames: Record<AssessmentQuestionType, string> = {
  recognition: "词义辨识",
  cloze: "语境填空",
  listening: "听音识义",
};
export const assessmentStateNames: Record<AssessmentSession["state"], string> =
  {
    active: "进行中",
    paused: "已暂停",
    completed: "已完成",
    ended_early: "提前结束",
  };
