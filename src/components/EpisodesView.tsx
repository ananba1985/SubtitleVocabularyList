import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { call, time, uid, waitTask } from "../api";
import type {
  AudioAsset,
  Candidate,
  CandidateExample,
  CollectionSeed,
  SourceSummary,
  TaskSnapshot,
  ImportOptions,
  MediaInspection,
  Settings,
} from "../types";
import { AudioPlayer } from "./AudioPlayer";
import { OnlineLookup } from "./OnlineLookup";
import { CandidateMeaning } from "./CandidateMeaning";
import { useLocalExplanation } from "../localExplanations";
import { SourcePicker } from "./SourcePicker";
export type EpisodePanel = "import" | "sources" | "filters" | null;
export function EpisodesView({
  sourceCount,
  refreshKey,
  collect,
  report,
  notify,
  panel,
  onPanelChange,
}: {
  sourceCount: number;
  refreshKey: number;
  collect: (seed: CollectionSeed) => void;
  report: (error: unknown) => void;
  notify: (text: string) => void;
  panel: EpisodePanel;
  onPanelChange: (panel: EpisodePanel) => void;
}) {
  const [sources, setSources] = useState<SourceSummary[]>([]),
    [sourceId, setSourceId] = useState(""),
    [search, setSearch] = useState(""),
    [kind, setKind] = useState(""),
    [pendingOnly, setPendingOnly] = useState(true),
    [showKnown, setShowKnown] = useState(false),
    [offset, setOffset] = useState(0);
  const [knownRefresh, setKnownRefresh] = useState(0);
  const [knownBusy, setKnownBusy] = useState(false);
  const [model, setModel] = useState("");
  useEffect(() => {
    let disposed = false;
    call<Settings>("settings_get")
      .then((value) => {
        if (!disposed)
          setModel(JSON.stringify([value.modelUrl, value.modelName]));
      })
      .catch(report);
    return () => {
      disposed = true;
    };
  }, []);
  const [candidates, setCandidates] = useState<Candidate[]>([]),
    [candidate, setCandidate] = useState<Candidate | null>(null),
    [examples, setExamples] = useState<CandidateExample[]>([]),
    [index, setIndex] = useState(0),
    [audioPath, setAudioPath] = useState(""),
    [inputPath, setInputPath] = useState(""),
    [busy, setBusy] = useState(false);
  const [playbackKey, setPlaybackKey] = useState(0);
  const [importNotice, setImportNotice] = useState("");
  const defaultOptions: ImportOptions = {
    audioStream: null,
    subtitleStream: null,
    subtitleMode: "auto",
    externalSubtitle: null,
  };
  const [inspection, setInspection] = useState<MediaInspection | null>(null),
    [options, setOptions] = useState<ImportOptions>(defaultOptions);
  const [editing, setEditing] = useState(false),
    [editText, setEditText] = useState(""),
    [editStart, setEditStart] = useState(0),
    [editEnd, setEditEnd] = useState(0);
  const example = examples[index];
  const currentSource = sources.find((source) => source.id === sourceId);
  const filterCount =
    Number(Boolean(search.trim())) +
    Number(Boolean(kind)) +
    Number(!pendingOnly) +
    Number(showKnown);
  const explanation = useLocalExplanation(
    candidate?.text ?? "",
    example?.text ?? "",
    model,
    Boolean(candidate && example),
    true,
  );
  const detailPanel = useRef<HTMLElement>(null);
  const selectionVersion = useRef(0);
  const activeSelection = `${sourceId}:${candidate?.kind}:${candidate?.key}:${example?.id}`;
  const activeSelectionRef = useRef(activeSelection);
  activeSelectionRef.current = activeSelection;
  useLayoutEffect(() => {
    if (detailPanel.current) detailPanel.current.scrollTop = 0;
  }, [candidate?.key, candidate?.kind, sourceId]);
  function selectCandidate(value: Candidate) {
    if (candidate?.key === value.key && candidate.kind === value.kind) {
      setCandidate(value);
      if (detailPanel.current) detailPanel.current.scrollTop = 0;
      return;
    }
    selectionVersion.current++;
    setCandidate(value);
    setExamples([]);
    setIndex(0);
    setEditing(false);
    setAudioPath("");
  }
  const reloadCandidates = () =>
    call<Candidate[]>("candidates_list", {
      sourceId,
      search,
      kind,
      onlyPending: pendingOnly,
      showKnown,
      offset,
      limit: 40,
    }).then(setCandidates);
  function selectSource(id: string) {
    if (id === sourceId) return;
    selectionVersion.current++;
    setSourceId(id);
    setCandidate(null);
    setCandidates([]);
    setExamples([]);
    setIndex(0);
    setEditing(false);
    setAudioPath("");
    setOffset(0);
  }
  async function prepareCurrent() {
    const version = selectionVersion.current;
    setBusy(true);
    try {
      const value = await call<{ task: TaskSnapshot | null }>(
        "explanations_prepare",
        { sourceId },
      );
      if (version === selectionVersion.current)
        setImportNotice(
          value.task
            ? "中文资料已安排到后台，当前集会优先准备。"
            : "当前集中文资料已准备完成。",
        );
    } catch (error) {
      report(error);
    } finally {
      setBusy(false);
    }
  }
  useEffect(() => {
    call<SourceSummary[]>("sources_list")
      .then((value) => {
        setSources(value);
        if (!sourceId && value.length) setSourceId(value[0].id);
      })
      .catch(report);
  }, [sourceCount, refreshKey]);
  useEffect(() => {
    let cancelled = false;
    if (sourceId)
      call<Candidate[]>("candidates_list", {
        sourceId,
        search,
        kind,
        onlyPending: pendingOnly,
        showKnown,
        offset,
        limit: 40,
      })
        .then((value) => {
          if (!cancelled) setCandidates(value);
        })
        .catch(report);
    return () => {
      cancelled = true;
    };
  }, [
    sourceId,
    search,
    kind,
    pendingOnly,
    showKnown,
    offset,
    refreshKey,
    knownRefresh,
  ]);
  useEffect(() => {
    setExamples([]);
    setIndex(0);
    setEditing(false);
    setAudioPath("");
  }, [candidate?.key, candidate?.kind, sourceId]);
  useEffect(() => {
    let cancelled = false;
    const version = selectionVersion.current;
    if (candidate && sourceId)
      call<CandidateExample[]>("candidate_examples", {
        sourceId,
        key: candidate.key,
      })
        .then((value) => {
          if (!cancelled && version === selectionVersion.current) {
            setExamples(value);
            setIndex((current) =>
              Math.min(current, Math.max(0, value.length - 1)),
            );
          }
        })
        .catch(report);
    return () => {
      cancelled = true;
    };
  }, [candidate?.key, candidate?.kind, sourceId, refreshKey]);
  useEffect(() => {
    if (!candidate) return;
    let disposed = false;
    const version = selectionVersion.current;
    call<boolean>("known_target_get", {
      kind: candidate.kind,
      text: candidate.text,
    })
      .then((known) => {
        if (disposed || version !== selectionVersion.current) return;
        if (known && !showKnown) {
          selectionVersion.current++;
          setCandidate(null);
          setExamples([]);
          setAudioPath("");
        } else
          setCandidate((value) =>
            value ? { ...value, isKnown: known } : null,
          );
      })
      .catch(report);
    return () => {
      disposed = true;
    };
  }, [candidate?.key, candidate?.kind, refreshKey, knownRefresh, showKnown]);
  async function importPaths(paths: string[], choice = options) {
    setBusy(true);
    setImportNotice("");
    try {
      await call<TaskSnapshot>("import_start", {
        paths,
        options: choice,
        operationId: uid(),
      });
      setImportNotice("导入已加入后台任务，可继续使用其他页面。");
    } catch (error) {
      report(error);
    } finally {
      setBusy(false);
    }
  }
  async function inspectPath(path: string) {
    setBusy(true);
    setInspection(null);
    setOptions(defaultOptions);
    try {
      const result = await call<MediaInspection>("media_inspect", { path });
      setInspection(result);
      setInputPath(path);
      setOptions({
        ...defaultOptions,
        externalSubtitle: result.externalSubtitle,
      });
    } catch (error) {
      report(error);
    } finally {
      setBusy(false);
    }
  }
  async function chooseSubtitle() {
    const path = await open({
      title: "选择外置字幕",
      filters: [{ name: "字幕", extensions: ["srt", "vtt", "ass", "ssa"] }],
    });
    if (typeof path === "string")
      setOptions({
        ...options,
        subtitleMode: "external",
        externalSubtitle: path,
      });
  }
  const trackLabel = (track: MediaInspection["audioTracks"][number]) =>
    `轨道 ${track.index} · ${track.tags.language ?? "语言未标注"} · ${track.tags.title ?? track.codec_name}`;
  async function choose(directory: boolean) {
    try {
      const selected = await open({
        directory,
        multiple: !directory,
        title: directory ? "选择剧集目录" : "选择剧集文件",
        filters: directory
          ? undefined
          : [
              {
                name: "视频",
                extensions: ["mkv", "mp4", "mov", "webm", "avi", "m4v"],
              },
            ],
      });
      if (selected) {
        const paths = Array.isArray(selected) ? selected : [selected];
        if (!directory && paths.length === 1) await inspectPath(paths[0]);
        else
          await importPaths(paths, {
            ...defaultOptions,
            subtitleMode: options.subtitleMode === "speech" ? "speech" : "auto",
          });
      }
    } catch (error) {
      report(error);
    }
  }
  async function play() {
    if (!example) return;
    const selection = activeSelectionRef.current;
    const version = selectionVersion.current;
    setBusy(true);
    try {
      const task = await call<TaskSnapshot>("preview_start", {
        sourceId,
        exampleId: example.id,
        operationId: uid(),
      });
      const result = await waitTask<{ path: string; asset: AudioAsset }>(task);
      if (
        version !== selectionVersion.current ||
        selection !== activeSelectionRef.current
      )
        return;
      setAudioPath(result.path);
      setPlaybackKey((value) => value + 1);
    } catch (error) {
      report(error);
    } finally {
      setBusy(false);
    }
  }
  async function decide(decision: string) {
    if (!candidate || !example) return;
    try {
      await call("candidate_decide", {
        sourceId,
        key: candidate.key,
        exampleId: example.id,
        decision,
      });
      setExamples(
        await call("candidate_examples", { sourceId, key: candidate.key }),
      );
      await reloadCandidates();
      notify(
        decision === "familiar"
          ? "已记录当前语境熟悉；其他语境仍可继续判断。"
          : "已保留为暂不确定。",
      );
    } catch (error) {
      report(error);
    }
  }
  async function markKnown() {
    if (!candidate) return;
    const target = candidate;
    const version = selectionVersion.current;
    setKnownBusy(true);
    try {
      await call("known_target_set", {
        kind: target.kind,
        text: target.text,
        known: !target.isKnown,
      });
      setKnownRefresh((value) => value + 1);
      if (version === selectionVersion.current) {
        if (!target.isKnown && !showKnown) {
          selectionVersion.current++;
          setCandidate(null);
          setExamples([]);
          setAudioPath("");
        } else setCandidate({ ...target, isKnown: !target.isKnown });
      }
      notify(
        target.isKnown
          ? "已恢复展示，其他剧集和采集场景也会重新提示。"
          : "已标记我已掌握：其他剧集的候选和采集确认默认跳过，可在本地设置撤销。",
      );
    } catch (error) {
      report(error);
    } finally {
      setKnownBusy(false);
    }
  }
  async function saveEdit() {
    if (!example) return;
    try {
      const updated = await call<CandidateExample>("source_example_update", {
        sourceId,
        exampleId: example.id,
        revision: example.revision,
        text: editText,
        startMs: editStart,
        endMs: editEnd,
      });
      setExamples((values) =>
        values.map((value) => (value.id === updated.id ? updated : value)),
      );
      setEditing(false);
      setAudioPath("");
      await reloadCandidates();
      notify("原句已修正，已收录的旧资料仍然保留。");
    } catch (error) {
      report(error);
    }
  }
  return (
    <>
      {sources.length > 0 && (
        <div className="preview-context">
          <div className="preview-source">
            <span className="muted">当前剧集</span>
            <strong title={currentSource?.title}>
              {currentSource?.title ?? "请选择剧集"}
            </strong>
          </div>
          <div className="actions">
            <button
              className="secondary"
              aria-expanded={panel === "sources"}
              aria-controls="episode-sources"
              onClick={() =>
                onPanelChange(panel === "sources" ? null : "sources")
              }
            >
              {panel === "sources" ? "收起选择" : "切换剧集"}
            </button>
            <button
              className="secondary"
              aria-expanded={panel === "filters"}
              aria-controls="episode-filters"
              onClick={() =>
                onPanelChange(panel === "filters" ? null : "filters")
              }
            >
              搜索筛选{filterCount > 0 ? `（${filterCount}）` : ""}
            </button>
          </div>
        </div>
      )}
      <div
        id="episode-import"
        className="import-box preview-options"
        hidden={panel !== "import"}
      >
        <div>
          <h3>导入视频或目录</h3>
        </div>
        <div className="actions">
          <button
            className="primary"
            disabled={busy}
            onClick={() => choose(false)}
          >
            选择视频
          </button>
          <button
            className="secondary"
            disabled={busy}
            onClick={() => choose(true)}
          >
            选择目录
          </button>
        </div>
        <div className="path-input">
          <input
            aria-label="视频或目录路径"
            placeholder="也可以粘贴本机视频或目录路径"
            value={inputPath}
            onChange={(event) => {
              setInputPath(event.target.value);
              setInspection(null);
              setOptions(defaultOptions);
            }}
          />
          <button
            disabled={busy || !inputPath.trim()}
            onClick={() => importPaths([inputPath.trim()])}
          >
            导入路径
          </button>
          <button
            disabled={busy || !inputPath.trim()}
            onClick={() => inspectPath(inputPath.trim())}
          >
            查看轨道与字幕
          </button>
        </div>
        {inspection && (
          <div className="import-options">
            <label>
              对白音轨
              <select
                aria-label="对白音轨"
                value={options.audioStream ?? "auto"}
                disabled={busy}
                onChange={(e) =>
                  setOptions({
                    ...options,
                    audioStream:
                      e.target.value === "auto" ? null : Number(e.target.value),
                  })
                }
              >
                <option value="auto">自动：优先英语音轨</option>
                {inspection.audioTracks.map((track) => (
                  <option key={track.index} value={track.index}>
                    {trackLabel(track)}
                  </option>
                ))}
              </select>
            </label>
            <label>
              对白文字来源
              <select
                aria-label="对白文字来源"
                value={options.subtitleMode}
                disabled={busy}
                onChange={(e) =>
                  setOptions({
                    ...options,
                    subtitleMode: e.target
                      .value as ImportOptions["subtitleMode"],
                  })
                }
              >
                <option value="auto">
                  自动：同名外置字幕、内嵌字幕或本地转写
                </option>
                <option value="embedded">选择内嵌字幕轨</option>
                <option value="external">选择外置字幕</option>
                <option value="speech">本地语音转写</option>
              </select>
            </label>
            {options.subtitleMode === "embedded" && (
              <label>
                字幕轨
                <select
                  aria-label="字幕轨"
                  value={options.subtitleStream ?? "auto"}
                  disabled={busy}
                  onChange={(e) =>
                    setOptions({
                      ...options,
                      subtitleStream:
                        e.target.value === "auto"
                          ? null
                          : Number(e.target.value),
                    })
                  }
                >
                  <option value="auto">优先英语字幕</option>
                  {inspection.subtitleTracks.map((track) => (
                    <option key={track.index} value={track.index}>
                      {trackLabel(track)}
                    </option>
                  ))}
                </select>
              </label>
            )}
            {options.subtitleMode === "external" && (
              <div>
                <p className="directory">
                  {options.externalSubtitle ?? "尚未选择外置字幕"}
                </p>
                <button
                  disabled={busy}
                  onClick={() => chooseSubtitle().catch(report)}
                >
                  选择字幕文件
                </button>
              </div>
            )}
            <p className="muted">
              核对后点击“导入路径”。更换音轨或字幕会保存为独立预习资料，旧收录及原声保留。
            </p>
          </div>
        )}
      </div>
      {importNotice && (
        <p className="muted" role="status">
          {importNotice}
        </p>
      )}
      {sources.length > 0 ? (
        <>
          <div
            id="episode-sources"
            className="preview-options"
            hidden={panel !== "sources"}
          >
            <SourcePicker
              sources={sources}
              sourceId={sourceId}
              onSelect={selectSource}
              action={
                <button
                  className="secondary"
                  disabled={busy || !sourceId}
                  onClick={prepareCurrent}
                  title="优先准备当前集的词义和译文，进度在后台任务中查看"
                >
                  优先准备当前集
                </button>
              }
            />
          </div>
          <div
            id="episode-filters"
            className="preview-options"
            hidden={panel !== "filters"}
          >
            <div className="toolbar">
              <input
                aria-label="搜索候选词"
                placeholder="搜索候选词或短语"
                value={search}
                onChange={(event) => {
                  setSearch(event.target.value);
                  setOffset(0);
                }}
              />
              <select
                aria-label="候选类型"
                value={kind}
                onChange={(event) => {
                  setKind(event.target.value);
                  setOffset(0);
                }}
              >
                <option value="">单词与短语</option>
                <option value="word">单词</option>
                <option value="phrase">短语</option>
              </select>
              <label className="check-label">
                <input
                  type="checkbox"
                  checked={pendingOnly}
                  onChange={(event) => {
                    setPendingOnly(event.target.checked);
                    setOffset(0);
                  }}
                />
                仅待确认语境
              </label>
              <label className="check-label">
                <input
                  type="checkbox"
                  checked={showKnown}
                  onChange={(event) => {
                    setShowKnown(event.target.checked);
                    setOffset(0);
                    if (!event.target.checked && candidate?.isKnown) {
                      selectionVersion.current++;
                      setCandidate(null);
                      setExamples([]);
                      setAudioPath("");
                    }
                  }}
                />
                显示已掌握
              </label>
            </div>
            <p className="muted">
              已掌握默认隐藏；已收录仍可再次判断。文本来源：
              {sources.find((source) => source.id === sourceId)?.textSource ===
              "pgs_ocr"
                ? "图片字幕 OCR，可纠错"
                : sources.find((source) => source.id === sourceId)
                      ?.textSource === "local_speech"
                  ? "本地语音转写，可纠错"
                  : sources.find((source) => source.id === sourceId)
                        ?.textSource === "external_text"
                    ? "外置文本字幕"
                    : "原始文本字幕"}
            </p>
          </div>
          <div className="split-view">
            <section className="list-panel" aria-label="候选词列表">
              {candidates.map((value) => (
                <button
                  key={`${value.kind}:${value.key}`}
                  className={`entry-row ${candidate?.key === value.key && candidate?.kind === value.kind ? "selected" : ""}`}
                  aria-pressed={
                    candidate?.key === value.key &&
                    candidate?.kind === value.kind
                  }
                  title={`${value.kind === "phrase" ? "短语" : "单词"} · 出现 ${value.count} 次 · ${value.exampleCount} 个语境 · 已处理 ${value.handledCount}/${value.count}`}
                  onClick={() => selectCandidate(value)}
                >
                  <span className="word">{value.text}</span>
                  <CandidateMeaning
                    candidate={value}
                    sourceId={sourceId}
                    model={model}
                    selected={
                      candidate?.key === value.key &&
                      candidate.kind === value.kind
                    }
                    meaning={
                      candidate?.key === value.key &&
                      candidate.kind === value.kind
                        ? explanation.value?.meaning
                        : undefined
                    }
                  />
                  {(value.existingEntryCount > 0 || value.isKnown) && (
                    <small>
                      {value.existingEntryCount > 0 &&
                        `已收录 ${value.existingEntryCount} 项`}
                      {value.isKnown && " · 已掌握，默认跳过"}
                    </small>
                  )}
                </button>
              ))}
              {!candidates.length && (
                <div className="empty">
                  <p>
                    {sourceId
                      ? "当前条件下没有待确认的候选。"
                      : "请选择剧集资料。"}
                  </p>
                </div>
              )}
              <div className="pagination">
                <button
                  disabled={offset === 0}
                  onClick={() => setOffset(Math.max(0, offset - 40))}
                >
                  上一页
                </button>
                <span>{Math.floor(offset / 40) + 1}</span>
                <button
                  disabled={candidates.length < 40}
                  onClick={() => setOffset(offset + 40)}
                >
                  下一页
                </button>
              </div>
            </section>
            <section
              className="detail-panel"
              aria-label="候选词详情"
              ref={detailPanel}
            >
              {candidate && example ? (
                <>
                  <span className="badge">
                    {candidate.kind === "phrase" ? "短语" : "单词"}
                  </span>
                  <h2 className="entry-title">{candidate.text}</h2>
                  <section
                    className="local-explanation"
                    aria-label="中文解释"
                    aria-live="polite"
                  >
                    {explanation.value ? (
                      <>
                        <p className="context-meaning">
                          {explanation.value.meaning}
                        </p>
                        {explanation.value.notes && (
                          <p className="muted">{explanation.value.notes}</p>
                        )}
                      </>
                    ) : explanation.state === "error" ? (
                      <div>
                        <p>{explanation.error}</p>
                        <button type="button" onClick={explanation.retry}>
                          重试中文解释
                        </button>
                      </div>
                    ) : (
                      <p className="muted">
                        等待后台准备中文解释，可在后台任务查看进度。
                      </p>
                    )}
                  </section>
                  <div className="context-nav">
                    <button
                      disabled={index === 0}
                      onClick={() => {
                        setIndex(index - 1);
                        setAudioPath("");
                        setEditing(false);
                      }}
                    >
                      上一处
                    </button>
                    <span>
                      语境 {index + 1}/{examples.length} ·{" "}
                      {time(example.startMs)}—{time(example.endMs)}
                    </span>
                    <button
                      disabled={index + 1 >= examples.length}
                      onClick={() => {
                        setIndex(index + 1);
                        setAudioPath("");
                        setEditing(false);
                      }}
                    >
                      下一处
                    </button>
                  </div>
                  <article className="example-card">
                    <p className="quote">{example.text}</p>
                    <p className="sentence-translation">
                      {explanation.value?.translation ??
                        (explanation.state === "error"
                          ? "暂未获得原句翻译"
                          : "等待后台准备原句翻译…")}
                    </p>
                    <span className="muted">
                      {example.decision === "familiar"
                        ? "当前语境已标记熟悉"
                        : example.decision === "collected"
                          ? "当前语境已收录"
                          : example.decision === "uncertain"
                            ? "暂不确定"
                            : "等待判断"}
                    </span>
                    <div className="actions">
                      <button
                        className="secondary"
                        disabled={busy}
                        onClick={play}
                      >
                        播放这段原声
                      </button>
                      <button
                        className="text-button"
                        onClick={() => {
                          setEditText(example.text);
                          setEditStart(example.startMs);
                          setEditEnd(example.endMs);
                          setEditing(true);
                        }}
                      >
                        纠正原句或时间
                      </button>
                    </div>
                  </article>
                  {editing && (
                    <div className="edit-box">
                      <label>
                        原句
                        <textarea
                          value={editText}
                          onChange={(event) => setEditText(event.target.value)}
                          rows={3}
                        />
                      </label>
                      <div className="field-row">
                        <label>
                          开始毫秒
                          <input
                            type="number"
                            value={editStart}
                            onChange={(event) =>
                              setEditStart(Number(event.target.value))
                            }
                          />
                        </label>
                        <label>
                          结束毫秒
                          <input
                            type="number"
                            value={editEnd}
                            onChange={(event) =>
                              setEditEnd(Number(event.target.value))
                            }
                          />
                        </label>
                      </div>
                      <div className="actions">
                        <button className="primary" onClick={saveEdit}>
                          保存修正
                        </button>
                        <button onClick={() => setEditing(false)}>取消</button>
                      </div>
                    </div>
                  )}
                  {audioPath && (
                    <AudioPlayer path={audioPath} playbackKey={playbackKey} />
                  )}
                  <div className="decision-actions">
                    <button
                      className="secondary"
                      disabled={knownBusy}
                      onClick={markKnown}
                    >
                      {candidate.isKnown
                        ? "取消已掌握，恢复展示"
                        : "我已掌握，以后跳过"}
                    </button>
                    <button
                      className="secondary"
                      onClick={() => decide("familiar")}
                    >
                      当前语境熟悉
                    </button>
                    <button onClick={() => decide("uncertain")}>
                      暂不确定
                    </button>
                    <button
                      className="primary"
                      onClick={() =>
                        collect({
                          text: candidate.text,
                          kind: candidate.kind,
                          context: example.text,
                          sourceId,
                          example,
                          meaning: explanation.value?.meaning,
                        })
                      }
                    >
                      收录生词
                    </button>
                  </div>
                  <button
                    className="text-button"
                    onClick={() =>
                      collect({
                        text: example.text,
                        kind: "sentence",
                        context: example.text,
                        sourceId,
                        example,
                        meaning: explanation.value?.translation,
                      })
                    }
                  >
                    将整句收录
                  </button>
                  <details className="supplementary-lookup">
                    <summary>联网查询</summary>
                    <OnlineLookup key={candidate.key} text={candidate.text} />
                  </details>
                </>
              ) : (
                <div className="empty">
                  <h3>按词判断，按需看对白</h3>
                  <p>选择左侧候选，展开它的语境和原声。</p>
                </div>
              )}
            </section>
          </div>
        </>
      ) : (
        <div className="empty">
          <h3>先导入一集剧集</h3>
          <p>资料准备好后，就能按词确认。</p>
        </div>
      )}
    </>
  );
}
