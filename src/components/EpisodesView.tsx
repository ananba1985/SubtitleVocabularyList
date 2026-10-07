import { useEffect, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { call, time, uid, waitTask } from "../api";
import type {
  AudioAsset,
  Candidate,
  CandidateExample,
  CollectionSeed,
  SourceSummary,
  TaskSnapshot,
} from "../types";
import { AudioPlayer } from "./AudioPlayer";
export function EpisodesView({
  sourceCount,
  refreshKey,
  collect,
  report,
  notify,
}: {
  sourceCount: number;
  refreshKey: number;
  collect: (seed: CollectionSeed) => void;
  report: (error: unknown) => void;
  notify: (text: string) => void;
}) {
  const [sources, setSources] = useState<SourceSummary[]>([]),
    [sourceId, setSourceId] = useState(""),
    [search, setSearch] = useState(""),
    [kind, setKind] = useState(""),
    [pendingOnly, setPendingOnly] = useState(true),
    [offset, setOffset] = useState(0);
  const [candidates, setCandidates] = useState<Candidate[]>([]),
    [candidate, setCandidate] = useState<Candidate | null>(null),
    [examples, setExamples] = useState<CandidateExample[]>([]),
    [index, setIndex] = useState(0),
    [audioPath, setAudioPath] = useState(""),
    [inputPath, setInputPath] = useState(""),
    [busy, setBusy] = useState(false);
  const [editing, setEditing] = useState(false),
    [editText, setEditText] = useState(""),
    [editStart, setEditStart] = useState(0),
    [editEnd, setEditEnd] = useState(0);
  const example = examples[index];
  const reloadCandidates = () =>
    call<Candidate[]>("candidates_list", {
      sourceId,
      search,
      kind,
      onlyPending: pendingOnly,
      offset,
      limit: 40,
    }).then(setCandidates);
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
  }, [sourceId, search, kind, pendingOnly, offset, refreshKey]);
  useEffect(() => {
    setExamples([]);
    setIndex(0);
    setEditing(false);
    setAudioPath("");
  }, [candidate?.key, sourceId]);
  useEffect(() => {
    let cancelled = false;
    if (candidate && sourceId)
      call<CandidateExample[]>("candidate_examples", {
        sourceId,
        key: candidate.key,
      })
        .then((value) => {
          if (!cancelled) {
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
  }, [candidate?.key, sourceId, refreshKey]);
  async function importPaths(paths: string[]) {
    setBusy(true);
    try {
      await call<TaskSnapshot>("import_start", { paths, operationId: uid() });
      notify("导入已开始，可以继续浏览或查看任务进度。");
    } catch (error) {
      report(error);
    } finally {
      setBusy(false);
    }
  }
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
      if (selected)
        await importPaths(Array.isArray(selected) ? selected : [selected]);
    } catch (error) {
      report(error);
    }
  }
  async function play() {
    if (!example) return;
    setBusy(true);
    try {
      const task = await call<TaskSnapshot>("preview_start", {
        sourceId,
        exampleId: example.id,
        operationId: uid(),
      });
      const result = await waitTask<{ path: string; asset: AudioAsset }>(task);
      setAudioPath(result.path);
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
      <div className="import-box">
        <div>
          <h3>准备下一集</h3>
          <p>提取对白和原声，按词确认需要学习的内容。</p>
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
            onChange={(event) => setInputPath(event.target.value)}
          />
          <button
            disabled={busy || !inputPath.trim()}
            onClick={() => importPaths([inputPath.trim()])}
          >
            导入路径
          </button>
        </div>
      </div>
      {sources.length > 0 ? (
        <>
          <div className="toolbar">
            <select
              aria-label="选择剧集"
              value={sourceId}
              onChange={(event) => {
                setSourceId(event.target.value);
                setCandidate(null);
                setOffset(0);
              }}
            >
              {sources.map((source) => (
                <option key={source.id} value={source.id}>
                  {source.title} · {source.candidateCount} 个候选
                </option>
              ))}
            </select>
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
          </div>
          <p className="muted">
            较长词优先；已收录仍可再次判断。文本来源：
            {sources.find((source) => source.id === sourceId)?.textSource ===
            "pgs_ocr"
              ? "图片字幕 OCR，可纠错"
              : sources.find((source) => source.id === sourceId)?.textSource ===
                  "local_speech"
                ? "本地语音转写，可纠错"
                : "原始文本字幕"}
          </p>
          <div className="split-view">
            <section className="list-panel">
              {candidates.map((value) => (
                <button
                  key={`${value.kind}:${value.key}`}
                  className={`entry-row ${candidate?.key === value.key ? "selected" : ""}`}
                  onClick={() => setCandidate(value)}
                >
                  <span className="word">{value.text}</span>
                  <small>
                    {value.kind === "phrase" ? "短语" : "单词"} · 出现{" "}
                    {value.count} 次 · {value.exampleCount} 个语境
                    {value.existingEntryCount > 0 &&
                      ` · 已收录 ${value.existingEntryCount} 项`}
                  </small>
                  <span className="definition">
                    已处理 {value.handledCount}/{value.count} 处
                  </span>
                </button>
              ))}
              {!candidates.length && (
                <div className="empty">
                  <p>当前条件下没有待确认的候选。</p>
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
            <section className="detail-panel">
              {candidate && example ? (
                <>
                  <span className="badge">
                    {candidate.kind === "phrase" ? "短语" : "单词"}
                  </span>
                  <h2 className="entry-title">{candidate.text}</h2>
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
                  {audioPath && <AudioPlayer path={audioPath} />}
                  <div className="decision-actions">
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
                      })
                    }
                  >
                    将整句收录
                  </button>
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
