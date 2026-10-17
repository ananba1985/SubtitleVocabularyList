import type * as Native from "./generated/ipc";
export type AssessmentQuestionType = Native.BankItem["questionType"];
export type AssessmentConfig = Native.AssessmentConfig;
export type AssessmentSettings = Native.AssessmentSettings;
export type FrequencyCatalog = Native.FrequencyCatalog;
export type AssessmentBank = Native.BankSummary;
export type AssessmentInventory = Native.CatalogInventory;
export type AssessmentChoice = Native.Choice;
export type AssessmentAudio = Native.AssessmentAudio;
export type AssessmentDraft = Native.QuestionDraft;
export type AssessmentBankItem = Native.BankItem;
export type AssessmentBankPage = Native.BankPage;
export type AssessmentQuota = Native.AssessmentQuota;
export type AssessmentPreview = Native.AssessmentPreview;
export type AssessmentQuestion = Native.AssessmentQuestion;
export type AssessmentFeedback = Native.AssessmentFeedback;
export type AssessmentSession = Native.AssessmentSession;
export type AssessmentSessionSummary = Native.AssessmentSessionSummary;
export type AssessmentPlayback = Native.AssessmentPlayback;
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

export type AssessmentCounts = Native.Counts;
export type AssessmentReportGroup = Native.ReportGroup;
export type AssessmentReport = Native.AssessmentReportSummary;
export function assessmentScopeLabel(c: AssessmentConfig): string {
  return `${c.content === "word" ? `单词 Top ${c.wordTopN}` : c.content === "phrase" ? `组合 Top ${c.phraseTopN}` : `单词 Top ${c.wordTopN} ＋ 组合 Top ${c.phraseTopN}`} · ${c.questionCount} 题`;
}
export type AssessmentReportItem = Native.ReportItem;
export type AssessmentHistoryItem = Native.AssessmentHistoryItem;
export type AssessmentComparison = Native.AssessmentComparison;
export type AssessmentSyncStatus = Native.AssessmentSyncStatus;
export type AssessmentSyncConflict = Native.AssessmentSyncConflict;
export type AssessmentCollectionCandidate = Native.CollectionCandidate;
export type AssessmentCollectionDraft = Native.AssessmentCollectionDraft;
