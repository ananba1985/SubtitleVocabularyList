import { useEffect, useState } from "react";
import { call, time } from "../api";
import type { AppInfo, AudioAsset, CollectionSeed, Entry } from "../types";
import { AudioPlayer } from "./AudioPlayer";
import { EntryEditor } from "./EntryEditor";
import { SystemSpeech } from "./SystemSpeech";
export function LibraryView({
  info,
  refreshKey,
  collect,
  report,
}: {
  info: AppInfo | null;
  refreshKey: number;
  collect: (seed: CollectionSeed) => void;
  report: (error: unknown) => void;
}) {
  const [search, setSearch] = useState(""),
    [offset, setOffset] = useState(0),
    [entries, setEntries] = useState<Entry[]>([]),
    [entry, setEntry] = useState<Entry | null>(null),
    [audioPath, setAudioPath] = useState("");
  const [editing, setEditing] = useState(false);
  const [playbackKey, setPlaybackKey] = useState(0);
  useEffect(() => {
    let cancelled = false;
    call<Entry[]>("entries_list", { search, offset, limit: 40 })
      .then((value) => {
        if (!cancelled) {
          setEntries(value);
          if (entry) {
            const updated = value.find((value) => value.id === entry.id);
            if (updated) setEntry(updated);
          }
        }
      })
      .catch(report);
    return () => {
      cancelled = true;
    };
  }, [search, offset, refreshKey]);
  async function play(asset: AudioAsset) {
    try {
      setAudioPath(await call<string>("media_path", { assetId: asset.id }));
      setPlaybackKey((value) => value + 1);
    } catch (error) {
      report(error);
    }
  }
  return (
    <>
      {entry && (
        <div className="library-actions">
          <button className="secondary" onClick={() => setEditing(true)}>
            修改选中词条
          </button>
        </div>
      )}
      {editing && entry && (
        <EntryEditor
          entry={entry}
          onClose={() => setEditing(false)}
          onSaved={(updated) => {
            setEntry(updated);
            setEntries((values) =>
              values.map((value) =>
                value.id === updated.id ? updated : value,
              ),
            );
            setEditing(false);
          }}
        />
      )}
      <div className="stats">
        <div>
          <strong>{info?.entryCount ?? 0}</strong>
          <span>已收录词条</span>
        </div>
        <div>
          <strong>{info?.sourceCount ?? 0}</strong>
          <span>剧集学习资料</span>
        </div>
        <div>
          <strong>原声与语境</strong>
          <span>保存每次需要记住的内容</span>
        </div>
      </div>
      <div className="toolbar">
        <input
          aria-label="搜索词条"
          placeholder="搜索单词、短语或整句"
          value={search}
          onChange={(event) => {
            setSearch(event.target.value);
            setOffset(0);
          }}
        />
        <span className="muted">自由浏览不会自动记为测验通过</span>
      </div>
      <div className="split-view">
        <section className="list-panel">
          {entries.map((value) => (
            <button
              className={`entry-row ${entry?.id === value.id ? "selected" : ""}`}
              key={value.id}
              onClick={() => {
                setEntry(value);
                setAudioPath("");
              }}
            >
              <span className="word">{value.text}</span>
              <span className="definition">
                {value.meanings.map((meaning) => meaning.text).join("；") ||
                  "释义待补充"}
              </span>
              <small>
                {value.examples.length} 条例句 · 主动收录{" "}
                {value.collectionCount} 次
              </small>
            </button>
          ))}
          {!entries.length && (
            <div className="empty">
              <h3>从第一次收录开始</h3>
              <p>保存一个词、短语或整句，之后再逐渐补充语境。</p>
              <button
                className="secondary"
                onClick={() => collect({ text: "", kind: "word", context: "" })}
              >
                收录词条
              </button>
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
              disabled={entries.length < 40}
              onClick={() => setOffset(offset + 40)}
            >
              下一页
            </button>
          </div>
        </section>
        <section className="detail-panel">
          {entry ? (
            <>
              <span className="badge">
                {{ word: "单词", phrase: "短语", sentence: "整句" }[entry.kind]}
              </span>
              <h2 className="entry-title">{entry.text}</h2>
              <SystemSpeech
                key={`word-${entry.id}`}
                text={entry.text}
                label="朗读词条（系统语音）"
              />
              {entry.meanings.map((value) => (
                <p className="meaning" key={value.id}>
                  {value.text}
                </p>
              ))}
              <p className="muted">
                来源出现 {entry.occurrenceCount} 次 · 主动收录{" "}
                {entry.collectionCount} 次
              </p>
              <h3>例句与原声</h3>
              {entry.examples.map((example) => (
                <article className="example-card" key={example.id}>
                  <p className="quote">{example.text}</p>
                  <p>{example.contextMeaning}</p>
                  <small>
                    {example.sourceTitle ?? "手动收录"}
                    {example.startMs !== null && ` · ${time(example.startMs)}`}
                  </small>
                  <div className="actions">
                    {example.audio.map((asset) => (
                      <button
                        className="secondary"
                        key={asset.id}
                        onClick={() => play(asset)}
                      >
                        播放原声 · {time(asset.durationMs)}
                      </button>
                    ))}
                  </div>
                  {!example.audio.length && (
                    <SystemSpeech text={example.text} />
                  )}
                </article>
              ))}
              {audioPath && (
                <AudioPlayer path={audioPath} playbackKey={playbackKey} />
              )}
            </>
          ) : (
            <div className="empty">
              <h3>保留单词出现的地方</h3>
              <p>选择词条，查看不同语境和原声。</p>
            </div>
          )}
        </section>
      </div>
    </>
  );
}
