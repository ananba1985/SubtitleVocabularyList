import { open } from "@tauri-apps/plugin-dialog";
import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { call, uid, waitTask } from "../api";
import { useLocalExplanation } from "../localExplanations";
import type {
  AudioAsset,
  Candidate,
  CandidateExample,
  ImportOptions,
  MediaInspection,
  SourceSummary,
} from "../types";

export type EpisodePanel = "import" | "sources" | "filters" | null;

export function useEpisodePreview({
  report,
  sourceCount,
  refreshKey,
  notify,
}: {
  sourceCount: number;
  refreshKey: number;
  report: (error: unknown) => void;
  notify: (text: string) => void;
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
    call("settings_get")
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
    call("candidates_list", {
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
      const value = await call("explanations_prepare", { sourceId });
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
    call("sources_list")
      .then((value) => {
        setSources(value);
        if (!sourceId && value.length) setSourceId(value[0].id);
      })
      .catch(report);
  }, [sourceCount, refreshKey]);
  useEffect(() => {
    let cancelled = false;
    if (sourceId)
      call("candidates_list", {
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
      call("candidate_examples", {
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
    call("known_target_get", {
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
      await call("import_start", {
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
      const result = await call("media_inspect", { path });
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
      const task = await call("preview_start", {
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
      const updated = await call("source_example_update", {
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
  return {
    sources,
    currentSource,
    filterCount,
    busy,
    choose,
    inputPath,
    setInputPath,
    setInspection,
    setOptions,
    defaultOptions,
    importPaths,
    inspectPath,
    inspection,
    options,
    trackLabel,
    chooseSubtitle,
    importNotice,
    sourceId,
    selectSource,
    prepareCurrent,
    search,
    setSearch,
    setOffset,
    kind,
    setKind,
    pendingOnly,
    setPendingOnly,
    showKnown,
    setShowKnown,
    candidate,
    selectionVersion,
    setCandidate,
    setExamples,
    setAudioPath,
    candidates,
    selectCandidate,
    model,
    explanation,
    offset,
    detailPanel,
    example,
    index,
    setIndex,
    setEditing,
    examples,
    play,
    setEditText,
    setEditStart,
    setEditEnd,
    editing,
    editText,
    editStart,
    editEnd,
    saveEdit,
    audioPath,
    playbackKey,
    knownBusy,
    markKnown,
    decide,
  };
}
