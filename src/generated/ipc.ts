// Generated from Rust IPC signatures and DTOs. Run pnpm commands:generate; do not edit.

export type AnswerInput = { operationId: string, questionId: string, answer: string, unable: boolean, };

export type AppCapabilities = { platform: string, offlineCore: boolean, independentRuntime: boolean, synchronization: boolean, nativePlayback: boolean, screenCapture: boolean, systemSpeech: boolean, knownTargetPreferences: boolean, };

export type AppError = { code: string, message: string, retryable: boolean, };

export type AppInfo = { version: string, dataDirectory: string, schemaVersion: number, entryCount: number, sourceCount: number, networkUnavailable: boolean, dictionaryWarnings: Array<string>, };

export type AssessmentAnswer = { id: string, sessionId: string, sessionItemId: string, originalAction: string, optionId: string | null, text: string, result: string, unscoredReason: string | null, uncertain: boolean, hinted: boolean, audioStatus: string, scorePolicy: string, createdAt: number, };

export type AssessmentAnswerInput = { sessionId: string, itemId: string, expectedRevision: number, operationId: string, action: string, optionId: string | null, text: string, uncertain: boolean, };

export type AssessmentAudio = { kind: string, relativePath: string, sha256: string, text: string, voiceId: string, assetId: string | null, };

export type AssessmentAudioEvent = { sessionId: string, itemId: string, playbackId: string, operationId: string, event: string, seconds: number, error: string, };

export type AssessmentAudioReply = { playbackId: string, played: boolean, seconds: number, error: string | null, };

export type AssessmentAudioStart = { sessionId: string, itemId: string, expectedRevision: number, operationId: string, };

export type AssessmentBankAudioInput = { content: QuestionDraft, operationId: string, };

export type AssessmentCollectionDraft = { draftId: string, sessionId: string, reportRevision: number, items: Array<CollectionCandidate>, };

export type AssessmentCollectionReceipt = { sessionItemId: string, saved: boolean, result?: CollectionResult, error?: AppError, };

export type AssessmentCollectionResult = { items: Array<AssessmentCollectionReceipt>, };

export type AssessmentComparison = { first: AssessmentReportSummary, second: AssessmentReportSummary, comparable: boolean, explanation: string, correctRateDelta: number | null, independentRateDelta: number | null, };

export type AssessmentConfig = { mode: string, wordTopN: number, phraseTopN: number, questionCount: number, content: string, questionTypes: Array<string>, bandSize: number, avoidRecentDays: number, minimumSample: number, };

export type AssessmentConflictSide = { state: string, position: number, planned: number, answers: number, corrections: number, reports: number, };

export type AssessmentCorrection = { id: string, answerId: string, revision: number, result: string, addHint: boolean, reason: string, createdAt: number, };

export type AssessmentCorrectionReply = { answerId: string, revision: number, sessionId: string, sessionRevision: number, correction: AssessmentCorrection, };

export type AssessmentCoverage = { catalogId: string, topN: number, rawTargets: number, lexemeTargets: number, preparedTargets: number, missingTargets: number, };

export type AssessmentFeedback = { answerId: string, answerRevision: number, sessionItemId: string, questionType: string, target: string, sentence: string, meaning: string, translation: string, explanation: string, acceptedAnswers: Array<string>, result: string, unscoredReason: string | null, uncertain: boolean, hinted: boolean, };

export type AssessmentHistoryItem = { id: string, state: string, revision: number, createdAt: number, config: AssessmentConfig, counts: Counts, status: string, comparisonKey: string, repeatedTargets: number, preservedCopy: boolean, };

export type AssessmentKeepBothReply = { sessionId: string, copySessionId: string, archiveId: string, };

export type AssessmentPendingReport = { sessionId: string, state: string, position: number, planned: number, };

export type AssessmentPlayback = { playbackId: string, path: string | null, kind: string, error: AppError | null, };

export type AssessmentPreview = { inventoryRevision: string, canStart: boolean, policyVersion: string, catalogs: Array<FrequencyCatalog>, quotas: Array<AssessmentQuota>, coverage: Array<AssessmentCoverage>, warnings: Array<string>, repeatCount: number, missingSlots: number, };

export type AssessmentQuestion = { id: string, position: number, questionType: string, prompt: string, choices: Array<Choice>, audioKind: string | null, audioPlayed: boolean, hinted: boolean, hint: string | null, draftText: string, draftOption: string | null, };

export type AssessmentQuota = { catalogId: string, content: string, fromRank: number, toRank: number, questionType: string, planned: number, available: number, unfilled: number, };

export type AssessmentReportReply = { report: AssessmentReportSummary, revisions: Array<number>, } | AssessmentPendingReport;

export type AssessmentReportSummary = { sessionId: string, revision: number, state: string, status: string, createdAt: number, generatedAt: number, scorePolicy: string, config: AssessmentConfig, comparisonKey: string, counts: Counts, byContent: Array<ReportGroup>, byBand: Array<ReportGroup>, byType: Array<ReportGroup>, byDimension: Array<ReportGroup>, details: Array<ReportGroup>, coverage: Array<AssessmentCoverage>, repeatedTargets: number, warnings: Array<string>, itemTotal: number, bankVersions: Array<string>, };

export type AssessmentSession = { id: string, state: string, revision: number, position: number, planned: number, processed: number, config: AssessmentConfig, createdAt: number, updatedAt: number, question: AssessmentQuestion | null, lastFeedback: AssessmentFeedback | null, warnings: Array<string>, policyVersion: string, };

export type AssessmentSessionSummary = { id: string, state: string, revision: number, position: number, config: AssessmentConfig, createdAt: number, updatedAt: number, };

export type AssessmentSettings = { revision: number, config: AssessmentConfig, };

export type AssessmentSyncConflict = { sessionId: string, reason: string, localRevision: number, remoteRevision: number, local: AssessmentConflictSide, remote: AssessmentConflictSide, };

export type AssessmentSyncStatus = { capability: string | null, pending: number, conflicts: number, pendingReferences: number, cursor: number, };

export type Attempt = { id: string, unitId: string, question: QuestionSnapshot, answer: string, hinted: boolean, originalOutcome: Outcome, outcome: Outcome, grader: string, revision: number, createdAt: number, state: ReviewState, corrections: Array<Correction>, };

export type AudioAsset = { id: string, relativePath: string, durationMs: number, kind: string, state: string, };

export type AudioPath = { path: string, };

export type BankItem = { id: string, bankId: string, catalogId: string, targetRank: number, targetKey: string, lexemeKey: string, kind: string, senseKey: string, questionType: string, state: string, revision: number, content: QuestionDraft, reviewNote: string, reviewedAt: number | null, };

export type BankPage = { items: Array<BankItem>, total: number, };

export type BankReviewReply = { bankId: string, state: string, revision: number, publishedItems: number, } | { bankId: string, itemId: string, state: string, revision: number, itemRevision: number, };

export type BankSummary = { id: string, state: string, revision: number, createdAt: number, publishedAt: number | null, itemCounts: { [key in string]: number }, targetCount: number, sources: Array<FrequencyCatalog>, policyVersion: string, };

export type Candidate = { key: string, text: string, kind: string, count: number, handledCount: number, exampleCount: number, existingEntryCount: number, isKnown: boolean, };

export type CandidateExample = { id: string, text: string, startMs: number, endMs: number, decision: string | null, revision: number, };

export type CatalogInventory = { policyVersion: string, catalogs: Array<FrequencyCatalog>, banks: Array<BankSummary>, indexReady: boolean, };

export type Choice = { id: string, text: string, };

export type ClipAudioAction = "play" | "pause" | "seek" | "close";

export type ClipAudioState = { sessionId: string, active: boolean, positionMs: number, durationMs: number, playing: boolean, played: boolean, ended: boolean, error: string | null, };

export type ClipboardPreferences = { debounceMs: number, persistResults: boolean, };

export type ClipboardStatus = { enabled: boolean, debounceMs: number, persistResults: boolean, error: string | null, };

export type CollectionCandidate = { sessionItemId: string, operationId: string, kind: string, text: string, meaning: string, sentence: string, translation: string, audioKind: string | null, matches: Array<Entry>, collectedEntryIds: Array<string>, };

export type CollectionInput = { operationId: string, kind: string, text: string, meaning: string, examples: Array<ExampleInput>, targetEntryId: string | null, expectedRevision: number | null, };

export type CollectionResult = { entryId: string, operationId: string, revision: number, created: boolean, exampleIds: Array<string>, };

export type CollectionSelection = { sessionItemId: string, targetEntryId: string | null, expectedRevision: number | null, meaning: string, translation: string, saveAudio: boolean, };

export type CommitAssessmentCollection = { draftId: string, items: Array<CollectionSelection>, operationId: string, };

export type ConnectionStatus = { siteUrl: string, state: string, offline: boolean, deviceName: string, requestId: string | null, displayCode: string | null, authorizationUrl: string | null, accountScope: string | null, expiresAt: number, };

export type CorrectAssessment = { answerId: string, expectedRevision: number, operationId: string, result: string, addHint: boolean, reason: string, };

export type Correction = { outcome: Outcome, reason: string, createdAt: number, };

export type CorrectionInput = { operationId: string, attemptId: string, expectedRevision: number, outcome: Outcome, reason: string, };

export type Counts = { planned: number, processed: number, unscored: number, valid: number, unanswered: number, correct: number, independentCorrect: number, incorrect: number, unknown: number, needsConfirmation: number, uncertain: number, hinted: number, correctRate: number | null, independentRate: number | null, reasons: { [key in string]: number }, };

export type Definition = { text: string, partOfSpeech: string, example: string, };

export type DeviceNetwork = { connected: boolean, metered: boolean, foreground: boolean, };

export type DeviceSyncPreferences = { deviceName: string, autoSync: boolean, };

export type DeviceSyncState = { preferences: DeviceSyncPreferences, boundAccount: string | null, currentAccount: string | null, bindingRequired: boolean, network: DeviceNetwork, lastCompletedAt: number | null, readyAudio: number, missingAudio: number, };

export type Entry = { id: string, kind: string, text: string, matchKey: string, revision: number, createdAt: number, updatedAt: number, collectionCount: number, occurrenceCount: number, meanings: Array<Meaning>, examples: Array<Example>, };

export type EntryUpdate = { id: string, expectedRevision: number, text: string, meanings: Array<MeaningUpdate>, };

export type Example = { id: string, text: string, contextMeaning: string, contexts: Array<ExampleContext>, sourceId: string | null, sourceTitle: string | null, startMs: number | null, endMs: number | null, audio: Array<AudioAsset>, };

export type ExampleContext = { scopeKey: string, meaningId: string | null, contextMeaning: string, };

export type ExampleInput = { text: string, contextMeaning: string, sourceId: string | null, locationKey: string, startMs: number | null, endMs: number | null, mediaAssetIds: Array<string>, };

export type Explanation = { meaning: string, translation: string, notes: string, };

export type FrequencyCatalog = { id: string, kind: string, title: string, sourceUrl: string, sourceVersion: string, sourceSha256: string, rankUnit: string, rawRows: number, maxRank: number, notice: string, eligibleTargets: number, };

export type FrozenItem = { bankId: string, itemId: string, catalog: FrequencyCatalog, targetRank: number, lexemeKey: string, kind: string, questionType: string, fromRank: number, toRank: number, content: QuestionDraft, repeatRecent: boolean, };

export type ImportOptions = { audioStream: number | null, subtitleStream: number | null, subtitleMode: string, externalSubtitle: string | null, };

export type JsonValue = number | string | boolean | Array<JsonValue> | { [key in string]: JsonValue } | null;

export type KeepAssessmentBoth = { sessionId: string, expectedLocalRevision: number, expectedRemoteRevision: number, operationId: string, };

export type KnownTarget = { kind: string, text: string, matchKey: string, markedAt: number, };

export type KnownTargetPage = { items: Array<KnownTarget>, total: number, };

export type LookupResult = { query: string, source: string, sourceUrl: string, definitions: Array<Definition>, };

export type Meaning = { id: string, text: string, origin: string, };

export type MeaningUpdate = { id: string | null, text: string, };

export type MediaInspection = { audioTracks: Array<Stream>, subtitleTracks: Array<Stream>, externalSubtitle: string | null, };

export type MediaTools = { ffmpeg: string, ffprobe: string, tesseract: string, pgsTessdata: string | null, whisper: string, whisperModel: string, };

export type NativeStatus = { selectionRegistered: boolean, ocrRegistered: boolean, inputRegistered: boolean, replacementRegistered: boolean, error: string | null, };

export type Outcome = "correct" | "correct_with_hint" | "incorrect" | "needs_confirmation";

export type Page<T> = { items: Array<T>, total: number, };

export type PixelRect = { x: number, y: number, width: number, height: number, };

export type PlaybackPreferences = { speed: number, volume: number, muted: boolean, autoNext: boolean, subtitleScale: number, subtitlePosition: number, subtitleVisible: boolean, secondarySubtitleVisible: boolean, loopMode: string, };

export type PlaybackState = { preferences: PlaybackPreferences, playlist: Array<PlaylistItem>, currentPath: string | null, };

export type PlayerSnapshot = { sessionId: string, revision: number, title: string, path: string, sourceId: string | null, importStatus: WatchImportStatus | null, playing: boolean, loaded: boolean, position: number, duration: number, volume: number, subtitleDelay: number, tracks: Array<WatchTrack>, context: WatchContext | null, status: string, remainingSeconds: number | null, collectedEntryId: string | null, error: string | null, renderer: string, translationVisibleAt: number | null, collectionStartedAt: number | null, frame: WatchFrame | null, playback: PlaybackState, playlistIndex: number | null, chapters: Array<WatchChapter>, chapterIndex: number | null, loopA: number | null, loopB: number | null, };

export type PlaylistChange = { action: string, paths: Array<string>, index: number | null, destination: number | null, };

export type PlaylistItem = { path: string, title: string, sourceId: string | null, };

export type PluginField = { id: string, label: string, secret: boolean, required: boolean, };

export type PluginManifest = { id: string, name: string, version: string, protocolVersion: number, minHostVersion: string, license: string, capabilities: Array<string>, allowedHosts: Array<string>, endpoint: string, method: string, fields: Array<PluginField>, headers: { [key in string]: string }, params: { [key in string]: string }, body: JsonValue, meaningPointer: string, translationPointer: string, notesPointer: string, };

export type PracticeChapter = { title: string, sentences: Array<PracticeSentence>, };

export type PracticeDocument = { id: string, title: string, kind: string, chapters: Array<PracticeChapter>, audioFile: string | null, };

export type PracticeProgress = { chapterIndex: number, sentenceIndex: number, copy: string, speed: number, audioMode: string, voiceId: string, };

export type PracticeSentence = { text: string, startMs: number | null, endMs: number | null, };

export type PracticeState = { document: PracticeDocument, chapterIndex: number, sentenceIndex: number, copy: string, speed: number, audioMode: string, voiceId: string, };

export type PreparationFailure = { source: string, text: string, message: string, };

export type PreparationProgress = { unit: string, wordsTotal: number, wordsCurrent: number, sentencesTotal: number, sentencesCurrent: number, generated: number, reused: number, skipped: number, failed: number, sources: Array<SourceProgress>, failures: Array<PreparationFailure>, concurrency: number, inFlight: number, waiting: number, batchesInFlight: number, batchSize: number, };

export type PreparationReply = { task: TaskSnapshot | null, };

export type PreparationStatus = { state: string, };

export type PrepareAssessmentCollection = { sessionId: string, itemIds: Array<string>, operationId: string, };

export type PrepareBank = { bankId: string | null, ranges: { [key in string]: number }, questionTypes: Array<string>, limit: number, operationId: string, };

export type PreparedCollection = { draftId: string, matches: Array<Entry>, input: CollectionInput, };

export type PreparedExplanation = { meaningScope: string, meaning: string, translation: string, notes: string, };

export type PreviewAudio = { path: string, asset: AudioAsset, };

export type Provider = { id: string, name: string, kind: string, enabled: boolean, autoQuery: boolean, sendContext: boolean, order: number, maxConcurrency: number, networkScope: string, values: { [key in string]: string }, credentialFields: Array<string>, manifest: PluginManifest | null, };

export type ProviderResult = { providerId: string, name: string, state: string, cacheSource: string, value: QueryValue | null, error: string | null, };

export type ProviderUpdate = { id: string, enabled: boolean, autoQuery: boolean, sendContext: boolean, order: number, maxConcurrency: number, values: { [key in string]: string }, secrets: { [key in string]: string }, };

export type QueryInput = { target: string, context: string, mode: string, sourceLanguage: string, targetLanguage: string, origin: QueryOrigin, };

export type QueryOrigin = { kind: string, sourceId: string | null, sourceTitle: string, locationKey: string, };

export type QuerySession = { sessionId: string, inputRevision: number, queryId: string | null, input: QueryInput, automatic: boolean, results: Array<ProviderResult>, };

export type QueryValue = { meaning: string, translation: string, notes: string, sourceUrl: string, };

export type Question = { id: string, unitId: string, dimension: string, prompt: string, context: string, audioKind: string, };

export type QuestionDraft = { target: string, meaning: string, sentence: string, translation: string, prompt: string, choices: Array<Choice>, correctOptionId: string | null, acceptedAnswers: Array<string>, explanation: string, audio: AssessmentAudio | null, origin: string, };

export type QuestionSnapshot = { entryId: string, target: string, dimension: string, expected: Array<string>, context: string, assetId: string | null, audioKind: string, };

export type RelatedVideos = { key: string, series: string, directory: string, total: number, paths: Array<string>, };

export type ReplacementStatus = { operationId: string, state: string, originalText: string, translation: string, providerId: string, error: string | null, clipboardPending: boolean, };

export type ReportGroup = { key: string, label: string, counts: Counts, insufficientSample: boolean, };

export type ReportItem = { id: string, position: number, item: FrozenItem, answer: AssessmentAnswer | null, originalAnswer: AssessmentAnswer | null, answerRevision: number | null, corrections: Array<AssessmentCorrection>, collectedEntryIds: Array<string>, };

export type ResolutionInput = { conflictId: string, choice: string, targetEntryId: string | null, expectedRemoteRevision: number, expectedLocalRevision: number | null, expectedTargetRevision: number | null, };

export type ReviewAudio = { path: string, kind: string, };

export type ReviewBank = { operationId: string, bankId: string, expectedRevision: number, action: string, itemId: string | null, expectedItemRevision: number | null, content: QuestionDraft | null, note: string, };

export type ReviewState = { level: string, streak: number, lapses: number, intervalMs: number, dueAt: number, lastReviewedAt: number, recent: Array<Outcome>, leechActive: boolean, leechStartedAt: number, leechClearedAt: number, };

export type ReviewUnit = { id: string, entryId: string, text: string, kind: string, scope: string, dimension: string, state: ReviewState, revision: number, reasons: Array<string>, available: boolean, };

export type ScreenBounds = { x: number, y: number, width: number, height: number, };

export type ScreenSessionView = { id: string, bounds: ScreenBounds, imagePath: string, };

export type Settings = { modelUrl: string, modelName: string, modelConcurrency: number, modelBatchSize: number, offlineMode: boolean, selectionShortcut: string, ocrShortcut: string, inputShortcut: string, replacementShortcut: string, replacementProviderId: string, systemVoice: string, closeToTray: boolean, watchAutoCollect: boolean, watchCollectDelaySeconds: number, siteUrl: string, tools: MediaTools, };

export type SourceProgress = { sourceId: string, title: string, current: number, total: number, };

export type SourceSummary = { id: string, title: string, durationMs: number, textSource: string, exampleCount: number, candidateCount: number, importedAt: number, };

export type SpeechAudio = { path: string, kind: string, voice: SystemVoice, };

export type StartAssessment = { config: AssessmentConfig, expectedInventoryRevision: string, operationId: string, };

export type Stream = { index: number, codec_type: string, codec_name: string, tags: { [key in string]: string }, };

export type SubtitleCandidate = { id: string, title: string, titleId: number, language: string, releaseName: string, name: string, url: string, season: number | null, episode: number | null, fps: string, };

export type SubtitleDownload = { id: string, sourceId: string, mediaSha256: string, candidate: SubtitleCandidate, files: Array<SubtitleDownloadFile>, downloadedAt: number, };

export type SubtitleDownloadFile = { id: string, name: string, relativePath: string, sha256: string, };

export type SubtitleDownloadPreview = { file: SubtitleDownloadFile, language: string, cueCount: number, firstMs: number, lastMs: number, videoDurationMs: number, matchedSentences: number, sentenceCount: number, pairs: Array<SubtitlePair>, warnings: Array<string>, };

export type SubtitleInfo = { mediaSha256: string, selectedChineseId: string | null, assets: Array<SubtitleTrackInfo>, warnings: Array<string>, };

export type SubtitlePair = { startMs: number, endMs: number, english: string, chinese: string, };

export type SubtitleProvenance = { provider: string, sourceUrl: string, fileId: string, releaseName: string, title: string, season: number | null, episode: number | null, fps: string, downloadedAt: number, versionConfirmed: boolean, };

export type SubtitleQuery = { fileName: string, title: string, year: number | null, season: number | null, episode: number | null, sdId: number | null, languages: Array<string>, notice: string, };

export type SubtitleSearch = { query: SubtitleQuery, mediaSha256: string, titles: Array<SubtitleTitle>, candidates: Array<SubtitleCandidate>, searchedAt: number, error: string | null, };

export type SubtitleService = { enabled: boolean, searchOnImport: boolean, englishCode: string, chineseCode: string, hasApiKey: boolean, };

export type SubtitleTitle = { id: number, name: string, year: number | null, };

export type SubtitleTrackInfo = { id: string, language: string, title: string, origin: string, streamIndex: number | null, format: string, relativePath: string, sha256: string, cueCount: number, timeOffsetMs: number, provenance: SubtitleProvenance | null, };

export type SyncBundleView = { schemaVersion: number, deleted: boolean, entry: SyncEntryView, tables: { [key in string]: Array<{ [key in string]: JsonValue }> }, };

export type SyncConflictView = { id: string, remoteId: string, localId: string | null, text: string, reason: string, remoteRevision: number, local: SyncBundleView | null, remote: SyncBundleView, candidates: Array<Entry>, };

export type SyncEntryView = { id: string, kind: string, text: string, revision: number, } & ({ [key in string]: number | string | boolean | Array<JsonValue> | { [key in string]: JsonValue } | null });

export type SyncResolution = { resolved: boolean, };

export type SyncResult = { pulled: number, pushed: number, conflicts: number, cursor: number, };

export type SyncStatus = { cursor: number, pending: number, conflicts: number, };

export type SyncTaskResult = { assessment: AssessmentSyncStatus, pulled: number, pushed: number, conflicts: number, cursor: number, };

export type SystemVoice = { id: string, name: string, };

export type TaskHandle<T> = TaskSnapshot & { readonly __taskResult: T };

export type TaskHistoryPage = { items: Array<TaskSnapshot>, total: number, };

export type TaskSnapshot = { id: string, operationId: string, requestHash: string, kind: string, subject: string | null, stage: string, state: string, current: number, total: number, message: string, result: JsonValue | null, error: AppError | null, createdAt: number, updatedAt: number, };

export type TaskUpdate = { visibleInList: boolean, id: string, operationId: string, requestHash: string, kind: string, subject: string | null, stage: string, state: string, current: number, total: number, message: string, result: JsonValue | null, error: AppError | null, createdAt: number, updatedAt: number, };

export type TransitionAssessment = { sessionId: string, expectedRevision: number, operationId: string, action: string, itemId: string | null, draftText: string, draftOption: string | null, };

export type TrayNavigation = "library" | "collect" | "import" | "episodes" | "watch" | "practice" | "reviews" | "leech" | "sync" | "tasks" | "settings";

export type VideoControl = { sessionId: string, action: string, value: number | null, text: string | null, kind: string | null, };

export type VideoLearning = { sessionId: string, context: WatchContext | null, eligible: boolean, remainingSeconds: number | null, collected: boolean, skipped: boolean, task: TaskSnapshot | null, };

export type VideoState = { sessionId: string, uri: string, positionMs: number, durationMs: number, playing: boolean, paused: boolean, revision: number, userPaused: boolean, foreground: boolean, windowFocused: boolean, fullscreen: boolean, ended: boolean, speed: number, tracks: Array<VideoTrack>, error: string | null, };

export type VideoStudy = { sourceId: string, sha256: string, subtitles: SubtitleInfo, };

export type VideoTrack = { id: string, formatId: string, kind: string, title: string, selected: boolean, supported: boolean, };

export type VideoViewport = { sessionId: string, x: number, y: number, width: number, height: number, ratio: number, };

export type WatchChapter = { index: number, title: string, position: number, };

export type WatchClipRange = { startMs: number, endMs: number, };

export type WatchContext = { revision: number, sourceId: string, exampleId: string, text: string, translation: string, translationRevision: string, startMs: number, endMs: number, textSource: string, previous: boolean, locationKey: string, mediaSha256: string, audioStream: number | null, preciseTiming: boolean, subtitleStartMs: number, subtitleEndMs: number, };

export type WatchControl = { action: string, value: number | null, text: string | null, expectedRevision: number | null, };

export type WatchFrame = { path: string, width: number, height: number, positionMs: number, text: string, busy: boolean, error: string | null, };

export type WatchImportStatus = { taskId: string | null, state: string, message: string, };

export type WatchProgress = { sourceId: string | null, path: string, position: number, subtitleId: number | null, subtitleSelectionManual: boolean | null, audioId: number | null, subtitleDelay: number, secondarySubtitleId: number | null, externalSubtitles: Array<string>, };

export type WatchTrack = { id: number, kind: string, title: string, selected: boolean, secondary: boolean, };

export type WatchViewport = { x: number, y: number, width: number, height: number, visible: boolean, };

export interface CommandMap {
  app_capabilities: { args: undefined; result: AppCapabilities };
  app_info: { args: undefined; result: AppInfo };
  app_quit: { args: undefined; result: void };
  app_ready: { args: undefined; result: void };
  assessment_answer_submit: { args: { input: AssessmentAnswerInput }; result: AssessmentSession };
  assessment_audio_event: { args: { input: AssessmentAudioEvent }; result: AssessmentAudioReply };
  assessment_audio_prepare: { args: { sessionId: string; itemId: string; operationId: string }; result: TaskHandle<AudioPath> };
  assessment_audio_start: { args: { input: AssessmentAudioStart }; result: AssessmentPlayback };
  assessment_bank_audio_prepare: { args: { input: AssessmentBankAudioInput }; result: TaskHandle<QuestionDraft> };
  assessment_bank_items: { args: { bankId: string; state?: string | null; offset: number; limit: number }; result: BankPage };
  assessment_bank_prepare: { args: { input: PrepareBank }; result: TaskSnapshot };
  assessment_bank_review: { args: { input: ReviewBank }; result: BankReviewReply };
  assessment_catalog_get: { args: undefined; result: CatalogInventory };
  assessment_collection_commit: { args: { input: CommitAssessmentCollection }; result: TaskHandle<AssessmentCollectionResult> };
  assessment_collection_prepare: { args: { input: PrepareAssessmentCollection }; result: AssessmentCollectionDraft };
  assessment_compare: { args: { first: string; second: string }; result: AssessmentComparison };
  assessment_history_list: { args: { offset: number; limit: number; comparisonKey?: string | null }; result: Page<AssessmentHistoryItem> };
  assessment_preview: { args: { config: AssessmentConfig }; result: AssessmentPreview };
  assessment_report_get: { args: { sessionId: string; reportRevision?: number | null }; result: AssessmentReportReply };
  assessment_report_items: { args: { sessionId: string; reportRevision: number; offset: number; limit: number }; result: Page<ReportItem> };
  assessment_result_correct: { args: { input: CorrectAssessment }; result: AssessmentCorrectionReply };
  assessment_session_get: { args: { sessionId: string }; result: AssessmentSession };
  assessment_session_list: { args: { unfinishedOnly: boolean; offset: number; limit: number }; result: Page<AssessmentSessionSummary> };
  assessment_session_transition: { args: { input: TransitionAssessment }; result: AssessmentSession };
  assessment_settings_get: { args: undefined; result: AssessmentSettings };
  assessment_settings_update: { args: { settings: AssessmentSettings; operationId: string }; result: AssessmentSettings };
  assessment_start: { args: { input: StartAssessment }; result: AssessmentSession };
  assessment_sync_conflicts: { args: undefined; result: Array<AssessmentSyncConflict> };
  assessment_sync_keep_both: { args: { input: KeepAssessmentBoth }; result: AssessmentKeepBothReply };
  assessment_sync_status: { args: undefined; result: AssessmentSyncStatus };
  audio_control: { args: { sessionId: string; action: ClipAudioAction; positionMs?: number | null }; result: ClipAudioState };
  audio_open: { args: { path: string }; result: ClipAudioState };
  audio_state: { args: { sessionId: string }; result: ClipAudioState };
  autostart_get: { args: undefined; result: boolean };
  autostart_set: { args: { enabled: boolean }; result: boolean };
  candidate_decide: { args: { sourceId: string; key: string; exampleId: string; decision: string }; result: void };
  candidate_examples: { args: { sourceId: string; key: string }; result: Array<CandidateExample> };
  candidates_list: { args: { sourceId: string; search?: string | null; kind?: string | null; onlyPending?: boolean | null; showKnown?: boolean | null; offset?: number | null; limit?: number | null }; result: Array<Candidate> };
  capture_ocr: { args: undefined; result: TaskSnapshot };
  capture_ocr_cancel: { args: { sessionId: string }; result: void };
  capture_ocr_submit: { args: { sessionId: string; rect: PixelRect }; result: TaskSnapshot };
  capture_selection: { args: undefined; result: TaskSnapshot };
  capture_session_get: { args: { sessionId: string }; result: ScreenSessionView };
  clipboard_listener_get: { args: undefined; result: ClipboardStatus };
  clipboard_listener_set: { args: { enabled: boolean; preferences: ClipboardPreferences }; result: ClipboardStatus };
  collection_commit: { args: { draftId: string; targetEntryId?: string | null; expectedRevision?: number | null; saveAudio: boolean }; result: TaskHandle<CollectionResult> };
  collection_from_example: { args: { sourceId: string; exampleId: string; text: string; kind: string; meaning: string; operationId: string }; result: PreparedCollection };
  collection_prepare: { args: { input: CollectionInput }; result: PreparedCollection };
  connection_check: { args: { operationId: string }; result: TaskHandle<ConnectionStatus> };
  connection_disconnect: { args: undefined; result: void };
  connection_open: { args: undefined; result: void };
  connection_start: { args: { operationId: string; replace: boolean }; result: TaskHandle<ConnectionStatus> };
  connection_status: { args: undefined; result: ConnectionStatus };
  device_sync_bind: { args: { expectedAccount: string }; result: DeviceSyncState };
  device_sync_get: { args: undefined; result: DeviceSyncState };
  device_sync_update: { args: { input: DeviceSyncPreferences }; result: DeviceSyncPreferences };
  entries_list: { args: { search?: string | null; offset?: number | null; limit?: number | null }; result: Array<Entry> };
  entry_get: { args: { entryId: string }; result: Entry };
  entry_update: { args: { input: EntryUpdate }; result: Entry };
  explain_start: { args: { text: string; context: string; operationId: string }; result: TaskHandle<Explanation> };
  explanation_get: { args: { text: string; context: string }; result: PreparedExplanation | null };
  explanation_import: { args: { text: string; context: string; value: Explanation; modelUrl: string; modelName: string }; result: Explanation };
  explanations_prepare: { args: { sourceId?: string | null }; result: PreparationReply };
  explanations_status: { args: undefined; result: PreparationStatus };
  import_start: { args: { paths: Array<string>; operationId: string; options?: ImportOptions | null }; result: TaskSnapshot };
  known_target_get: { args: { kind: string; text: string }; result: boolean };
  known_target_set: { args: { kind: string; text: string; known: boolean }; result: void };
  known_targets_list: { args: { search?: string | null; offset?: number | null; limit?: number | null }; result: KnownTargetPage };
  media_inspect: { args: { path: string }; result: MediaInspection };
  media_path: { args: { assetId: string }; result: string };
  native_status: { args: undefined; result: NativeStatus };
  online_query_start: { args: { text: string; provider?: string | null; operationId: string }; result: TaskHandle<LookupResult> };
  plugin_install: { args: { path: string }; result: Provider };
  plugin_remove: { args: { pluginId: string }; result: Array<Provider> };
  practice_audio_save: { args: Uint8Array; result: string };
  practice_example: { args: { documentId: string; chapter: number; sentence: number }; result: CandidateExample };
  practice_get: { args: undefined; result: PracticeState | null };
  practice_import: { args: { document: PracticeDocument }; result: PracticeState };
  practice_progress_save: { args: { documentId: string; progress: PracticeProgress }; result: void };
  preview_start: { args: { sourceId: string; exampleId: string; operationId: string }; result: TaskHandle<PreviewAudio> };
  providers_list: { args: undefined; result: Array<Provider> };
  providers_update: { args: { update: ProviderUpdate }; result: Array<Provider> };
  query_copy: { args: { text: string }; result: void };
  replacement_cancel: { args: { operationId: string }; result: void };
  replacement_clipboard_restore: { args: undefined; result: void };
  replacement_result_open: { args: undefined; result: void };
  replacement_status_get: { args: undefined; result: ReplacementStatus };
  review_audio_start: { args: { questionId: string; operationId: string }; result: TaskHandle<ReviewAudio> };
  review_correct: { args: { input: CorrectionInput }; result: Attempt };
  review_hint: { args: { questionId: string }; result: string };
  review_history: { args: { unitId?: string | null; offset: number; limit: number }; result: Array<Attempt> };
  review_question: { args: { unitId: string; expectedRevision: number }; result: Question };
  review_submit: { args: { input: AnswerInput }; result: Attempt };
  review_units: { args: { mode: string; dimension: string; offset: number; limit: number }; result: Array<ReviewUnit> };
  settings_get: { args: undefined; result: Settings };
  settings_update: { args: { settings: Settings }; result: Settings };
  source_example_update: { args: { sourceId: string; exampleId: string; revision: number; text: string; startMs: number; endMs: number }; result: CandidateExample };
  sources_list: { args: undefined; result: Array<SourceSummary> };
  speech_settings_open: { args: undefined; result: void };
  speech_start: { args: { text: string; operationId: string; voiceId?: string | null }; result: TaskHandle<SpeechAudio> };
  speech_voices: { args: undefined; result: Array<SystemVoice> };
  subtitle_alignment_set: { args: { sourceId: string; assetId: string; offsetMs: number }; result: SubtitleInfo };
  subtitle_chinese_select: { args: { sourceId: string; assetId?: string | null }; result: SubtitleInfo };
  subtitle_download_commit_start: { args: { downloadId: string; fileId: string; offsetMs: number; confirmed: boolean; selectChinese: boolean; operationId: string }; result: TaskHandle<SubtitleInfo> };
  subtitle_download_preview: { args: { downloadId: string; fileId: string; offsetMs: number }; result: SubtitleDownloadPreview };
  subtitle_download_start: { args: { sourceId: string; candidateId: string; operationId: string }; result: TaskHandle<SubtitleDownload> };
  subtitle_info_get: { args: { sourceId: string }; result: SubtitleInfo | null };
  subtitle_prepare_start: { args: { sourceId: string; operationId: string }; result: TaskHandle<SubtitleInfo> };
  subtitle_search_context: { args: { sourceId: string }; result: SubtitleQuery };
  subtitle_search_get: { args: { sourceId: string }; result: SubtitleSearch | null };
  subtitle_search_start: { args: { sourceId: string; query: SubtitleQuery; operationId: string }; result: TaskHandle<SubtitleSearch> };
  subtitle_service_get: { args: undefined; result: SubtitleService };
  subtitle_service_set: { args: { config: SubtitleService; apiKey?: string | null }; result: SubtitleService };
  sync_conflicts: { args: undefined; result: Array<SyncConflictView> };
  sync_resolve: { args: { input: ResolutionInput; operationId: string }; result: TaskHandle<SyncResolution> };
  sync_start: { args: { operationId: string }; result: TaskHandle<SyncTaskResult> };
  sync_status: { args: undefined; result: SyncStatus };
  task_cancel: { args: { taskId: string }; result: TaskSnapshot };
  task_get: { args: { taskId: string }; result: TaskSnapshot };
  tasks_history: { args: { offset?: number | null; limit?: number | null }; result: TaskHistoryPage };
  tasks_list: { args: undefined; result: Array<TaskSnapshot> };
  translation_collect: { args: { sessionId: string; inputRevision: number; providerId: string }; result: void };
  translation_open: { args: undefined; result: void };
  translation_query_cancel: { args: { sessionId: string; queryId?: string | null }; result: void };
  translation_query_start: { args: { sessionId: string; expectedInputRevision: number; providerIds: Array<string>; automatic: boolean; persist: boolean }; result: QuerySession };
  translation_session_get: { args: { sessionId?: string | null }; result: QuerySession | null };
  translation_session_open: { args: { input: QueryInput; sessionId?: string | null }; result: QuerySession };
  translation_window_hide: { args: undefined; result: void };
  translation_window_pin: { args: { pinned: boolean }; result: void };
  tray_action_get: { args: undefined; result: TrayNavigation | null };
  video_context: { args: undefined; result: WatchContext | null };
  video_control: { args: { control: VideoControl }; result: VideoState };
  video_index_start: { args: { uri: string; operationId: string }; result: TaskHandle<VideoStudy> };
  video_learning_get: { args: undefined; result: VideoLearning };
  video_learning_skip: { args: { revision: number }; result: void };
  video_learning_visible: { args: { revision: number; visible: boolean }; result: void };
  video_library: { args: undefined; result: PlaybackState };
  video_open: { args: { uri: string }; result: VideoState };
  video_pick: { args: undefined; result: PlaybackState };
  video_state: { args: undefined; result: VideoState };
  video_study: { args: { uri: string }; result: VideoStudy | null };
  video_subtitle_add: { args: { uri: string; language: string; operationId: string }; result: TaskHandle<VideoStudy> };
  video_viewport: { args: { viewport: VideoViewport }; result: void };
  watch_collection_prepare: { args: { sessionId: string; expectedRevision: number; clipRange?: WatchClipRange | null }; result: WatchContext };
  watch_control: { args: { control: WatchControl }; result: PlayerSnapshot };
  watch_files_open_next: { args: undefined; result: PlayerSnapshot | null };
  watch_files_pending: { args: undefined; result: boolean };
  watch_open: { args: { path?: string | null; sourceId?: string | null }; result: PlayerSnapshot };
  watch_playlist_update: { args: { change: PlaylistChange }; result: PlayerSnapshot };
  watch_progress_get: { args: undefined; result: WatchProgress | null };
  watch_related_videos: { args: { path: string }; result: RelatedVideos | null };
  watch_state_get: { args: undefined; result: PlayerSnapshot };
  watch_viewport_set: { args: { viewport: WatchViewport }; result: void };
}
