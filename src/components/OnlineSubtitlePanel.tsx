import { useEffect, useRef, useState } from "react";
import { call, message, time, uid, waitTask } from "../api";
import type {
  SubtitleDownload,
  SubtitleDownloadPreview,
  SubtitleInfo,
  SubtitleQuery,
  SubtitleSearch,
  SubtitleService,
} from "../generated/ipc";

export function SubtitleServiceSettings() {
  const [config, setConfig] = useState<SubtitleService | null>(null),
    [key, setKey] = useState("");
  const [busy, setBusy] = useState(false),
    [notice, setNotice] = useState("");
  useEffect(() => {
    let active = true;
    void call("subtitle_service_get")
      .then((v) => active && setConfig(v))
      .catch((e) => {
        if (active) {
          setNotice(message(e));
          setConfig({
            enabled: false,
            searchOnImport: false,
            hasApiKey: false,
            englishCode: "EN",
            chineseCode: "ZH",
          });
        }
      });
    return () => {
      active = false;
    };
  }, []);
  async function save(clear = false) {
    if (!config) return;
    setBusy(true);
    setNotice("");
    try {
      setConfig(
        await call("subtitle_service_set", {
          config: clear
            ? { ...config, enabled: false, searchOnImport: false }
            : config,
          apiKey: clear ? "" : key.trim() || null,
        }),
      );
      setKey("");
      setNotice(
        clear ? "字幕服务已停用，密钥已清除。" : "字幕服务配置已保存。",
      );
    } catch (e) {
      setNotice(message(e));
    } finally {
      setBusy(false);
    }
  }
  return (
    <details className="subtitle-service">
      <summary>在线字幕服务 · SubDL</summary>
      <p className="muted">
        用于查找已有中英字幕，不使用云端翻译。先在 SubDL 账户取得 API
        key，再在此保存；凭据加密保存在本机。只使用免费基础检索与普通下载，不提供付费 AI 翻译或订阅操作。免费额度及限制以服务方说明为准。
      </p>
      {config && (
        <fieldset disabled={busy}>
          <label className="check-label">
            <input
              type="checkbox"
              checked={config.enabled}
              onChange={(e) =>
                setConfig({ ...config, enabled: e.target.checked })
              }
            />
            启用在线字幕检索与下载
          </label>
          <label>
            API key
            <input
              type="password"
              autoComplete="off"
              value={key}
              placeholder={
                config.hasApiKey ? "已保存；留空保持原密钥" : "尚未配置"
              }
              onChange={(e) => setKey(e.target.value)}
            />
          </label>
          <label className="check-label">
            <input
              type="checkbox"
              checked={config.searchOnImport}
              onChange={(e) =>
                setConfig({ ...config, searchOnImport: e.target.checked })
              }
            />
            导入时自动检索缺失语言的候选字幕
          </label>
          <p className="muted">
            自动检索仅发送剧名、年份和季集，不上传视频或个人词库；下载和采用候选仍需核对。
          </p>
          <details>
            <summary>服务语言代码</summary>
            <label>
              英语
              <input
                value={config.englishCode}
                onChange={(e) =>
                  setConfig({ ...config, englishCode: e.target.value })
                }
              />
            </label>
            <label>
              中文
              <input
                value={config.chineseCode}
                onChange={(e) =>
                  setConfig({ ...config, chineseCode: e.target.value })
                }
              />
            </label>
            <small className="muted">
              按服务支持的代码填写；服务更改语言代码时可在这里调整。
            </small>
          </details>
          <button onClick={() => void save()}>保存字幕服务</button>
          {config.hasApiKey && (
            <button className="text-button" onClick={() => void save(true)}>
              停用并清除密钥
            </button>
          )}
        </fieldset>
      )}
      {notice && <p role="status">{notice}</p>}
    </details>
  );
}

export function OnlineSubtitlePanel({
  sourceId,
  onImported,
}: {
  sourceId: string;
  onImported: (info: SubtitleInfo) => Promise<void>;
}) {
  const [config, setConfig] = useState<SubtitleService | null>(null),
    [query, setQuery] = useState<SubtitleQuery | null>(null),
    [search, setSearch] = useState<SubtitleSearch | null>(null);
  const [candidateId, setCandidateId] = useState(""),
    [filter, setFilter] = useState(""),
    [download, setDownload] = useState<SubtitleDownload | null>(null),
    [fileId, setFileId] = useState("");
  const [preview, setPreview] = useState<SubtitleDownloadPreview | null>(null),
    [offset, setOffset] = useState(0),
    [confirmed, setConfirmed] = useState(false),
    [selectChinese, setSelectChinese] = useState(true);
  const [busy, setBusy] = useState(false),
    [error, setError] = useState(""),
    [progress, setProgress] = useState(""),
    [notice, setNotice] = useState("");
  const currentSource = useRef(sourceId);
  const alive = useRef(true);
  currentSource.current = sourceId;
  useEffect(() => {
    alive.current = true;
    let active = true;
    setQuery(null);
    setSearch(null);
    setDownload(null);
    setPreview(null);
    setError("");
    setNotice("");
    setBusy(false);
    setProgress("");
    if (sourceId)
      void Promise.all([
        call("subtitle_service_get"),
        call("subtitle_search_context", { sourceId }),
        call("subtitle_search_get", { sourceId }),
      ])
        .then(([c, q, s]) => {
          if (active) {
            setConfig(c);
            setQuery(q);
            setSearch(s);
          }
        })
        .catch((e) => active && setError(message(e)));
    return () => {
      active = false;
      alive.current = false;
    };
  }, [sourceId]);
  const dirty =
    !!query &&
    !!search &&
    JSON.stringify(query) !== JSON.stringify(search.query);
  const run = async <T,>(
    work: () => Promise<T>,
    done: (value: T) => Promise<void> | void,
  ) => {
    const source = sourceId;
    setBusy(true);
    setError("");
    setNotice("");
    try {
      const result = await work();
      if (alive.current && currentSource.current === source) await done(result);
    } catch (e) {
      if (alive.current && currentSource.current === source)
        setError(message(e));
    } finally {
      if (alive.current && currentSource.current === source) {
        setBusy(false);
        setProgress("");
      }
    }
  };
  const clearPreview = () => {
    setPreview(null);
    setConfirmed(false);
  };
  if (!sourceId) return null;
  return (
    <details className="online-subtitles">
      <summary>
        在线补充中英字幕
        {search?.candidates.length
          ? ` · ${search.candidates.length} 个候选`
          : ""}
      </summary>
      {!config?.enabled && (
        <p className="muted">
          先在“本地设置 → 在线字幕服务”配置并启用
          SubDL。本地字幕和正常观看仍可用。
        </p>
      )}
      {query && (
        <fieldset disabled={busy}>
          <p className="directory">{query.fileName}</p>
          <div className="subtitle-query-fields">
            <label>
              剧名／片名
              <input
                value={query.title}
                onChange={(e) =>
                  setQuery({ ...query, title: e.target.value, sdId: null })
                }
              />
            </label>
            <label>
              年份
              <input
                type="number"
                value={query.year ?? ""}
                onChange={(e) =>
                  setQuery({
                    ...query,
                    year: e.target.value ? Number(e.target.value) : null,
                    sdId: null,
                  })
                }
              />
            </label>
            <label>
              季
              <input
                type="number"
                min={0}
                value={query.season ?? ""}
                onChange={(e) =>
                  setQuery({
                    ...query,
                    season: e.target.value ? Number(e.target.value) : null,
                  })
                }
              />
            </label>
            <label>
              集
              <input
                type="number"
                min={0}
                value={query.episode ?? ""}
                onChange={(e) =>
                  setQuery({
                    ...query,
                    episode: e.target.value ? Number(e.target.value) : null,
                  })
                }
              />
            </label>
          </div>
          {query.notice && <p className="muted">{query.notice}</p>}
          <div className="watch-caption-actions">
            {(
              [
                ["en", "英文"],
                ["zh", "中文"],
              ] as const
            ).map(([language, label]) => (
              <label className="check-label" key={language}>
                <input
                  type="checkbox"
                  checked={query.languages.includes(language)}
                  onChange={(e) =>
                    setQuery({
                      ...query,
                      languages: e.target.checked
                        ? [...query.languages, language]
                        : query.languages.filter((l) => l !== language),
                    })
                  }
                />
                {label}
              </label>
            ))}
          </div>
          <button
            disabled={
              !config?.enabled || !query.languages.length || !query.title.trim()
            }
            onClick={() =>
              void run(
                async () =>
                  waitTask(
                    await call("subtitle_search_start", {
                      sourceId,
                      query,
                      operationId: uid(),
                    }),
                    (t) => setProgress(t.message),
                  ),
                (value) => {
                  setSearch(value);
                  setCandidateId("");
                  setDownload(null);
                  clearPreview();
                },
              )
            }
          >
            检索所选语言
          </button>
          {search && (
            <>
              {search.error && <p className="error">{search.error}</p>}
              {!!search.titles.length && (
                <label>
                  核对片名（选择后重新检索）
                  <select
                    value={query.sdId ?? ""}
                    onChange={(e) =>
                      setQuery({
                        ...query,
                        sdId: e.target.value ? Number(e.target.value) : null,
                      })
                    }
                  >
                    <option value="">按名称检索</option>
                    {search.titles.map((title) => (
                      <option key={title.id} value={title.id}>
                        {title.name}
                        {title.year ? ` · ${title.year}` : ""}
                      </option>
                    ))}
                  </select>
                </label>
              )}
              {dirty && (
                <p className="muted">检索条件已修改，请重新检索后下载。</p>
              )}
              {!search.candidates.length && !search.error && (
                <p className="muted">
                  未取得可用候选；可核对片名、季集、语言代码后重试。多个片名匹配时先选择片名。
                </p>
              )}
              {!!search.candidates.length && (
                <>
                  <label>
                    筛选发行版
                    <input
                      value={filter}
                      placeholder="发行版或字幕文件名"
                      onChange={(e) => setFilter(e.target.value)}
                    />
                  </label>
                  <div className="subtitle-candidates">
                    {search.candidates
                      .filter((c) =>
                        `${c.releaseName} ${c.name}`
                          .toLowerCase()
                          .includes(filter.toLowerCase()),
                      )
                      .map((c) => (
                        <label key={c.id} className="subtitle-candidate">
                          <input
                            type="radio"
                            name={`subtitle-${sourceId}`}
                            checked={candidateId === c.id}
                            onChange={() => {
                              setCandidateId(c.id);
                              setSelectChinese(c.language === "zh");
                            }}
                          />
                          <span>
                            <strong>
                              {c.language === "zh" ? "中文" : "英文"} ·{" "}
                              {c.releaseName || c.name}
                            </strong>
                            <small>
                              {c.title} ·{" "}
                              {c.season !== null ? `第 ${c.season} 季 ` : ""}
                              {c.episode !== null
                                ? `第 ${c.episode} 集 · `
                                : ""}
                              {c.fps ? `${c.fps} fps` : "帧率未标注"} · SubDL
                            </small>
                          </span>
                        </label>
                      ))}
                  </div>
                  <button
                    disabled={!candidateId || dirty}
                    onClick={() =>
                      void run(
                        async () =>
                          waitTask(
                            await call("subtitle_download_start", {
                              sourceId,
                              candidateId,
                              operationId: uid(),
                            }),
                            (t) => setProgress(t.message),
                          ),
                        (value) => {
                          setDownload(value);
                          setFileId(
                            value.files.length === 1 ? value.files[0].id : "",
                          );
                          setOffset(0);
                          clearPreview();
                        },
                      )
                    }
                  >
                    下载所选候选
                  </button>
                </>
              )}
            </>
          )}
        </fieldset>
      )}
      {download && (
        <fieldset disabled={busy}>
          <label>
            字幕包中的文件
            <select
              value={fileId}
              onChange={(e) => {
                setFileId(e.target.value);
                clearPreview();
              }}
            >
              <option value="">选择与本集对应的文件</option>
              {download.files.map((f, index) => (
                <option key={`${f.id}:${index}`} value={f.id}>
                  {f.name}
                </option>
              ))}
            </select>
          </label>
          <label>
            字幕偏移（秒）
            <input
              type="number"
              step={0.1}
              value={offset}
              onChange={(e) => {
                setOffset(Number(e.target.value));
                clearPreview();
              }}
            />
          </label>
          <small className="muted">
            正数将所选字幕延后；先核对同一剧集与发行版本，再用时间预览微调。
          </small>
          <button
            disabled={!fileId}
            onClick={() =>
              void run(
                () =>
                  call("subtitle_download_preview", {
                    downloadId: download.id,
                    fileId,
                    offsetMs: Math.round(offset * 1000),
                  }),
                (value) => {
                  setPreview(value);
                  setConfirmed(false);
                },
              )
            }
          >
            预览原文与时间
          </button>
          {preview && (
            <>
              <p>
                {preview.cueCount} 条 · {time(preview.firstMs)}—
                {time(preview.lastMs)}
                {preview.videoDurationMs
                  ? ` · 视频 ${time(preview.videoDurationMs)}`
                  : ""}
              </p>
              {preview.sentenceCount > 0 && preview.language !== "en" && (
                <p className="muted">
                  按当前时间重叠可对应 {preview.matchedSentences}/
                  {preview.sentenceCount} 句；请核对以下样例。
                </p>
              )}
              {preview.warnings.map((warning, index) => (
                <p className="error" key={index}>
                  {warning}
                </p>
              ))}
              <div className="subtitle-preview">
                {preview.pairs.map((pair, index) => (
                  <div key={index}>
                    <small>
                      {time(pair.startMs)}—{time(pair.endMs)}
                    </small>
                    {pair.english && <p>{pair.english}</p>}
                    {pair.chinese && <p>{pair.chinese}</p>}
                  </div>
                ))}
              </div>
              <label className="check-label">
                <input
                  type="checkbox"
                  checked={confirmed}
                  onChange={(e) => setConfirmed(e.target.checked)}
                />
                已核对本集、发行版本和时间预览
              </label>
              {preview.language !== "en" && (
                <label className="check-label">
                  <input
                    type="checkbox"
                    checked={selectChinese}
                    onChange={(e) => setSelectChinese(e.target.checked)}
                  />
                  作为暂停时优先使用的中文
                </label>
              )}
              <button
                disabled={!confirmed || !!preview.warnings.length}
                onClick={() =>
                  void run(
                    async () =>
                      waitTask(
                        await call("subtitle_download_commit_start", {
                          downloadId: download.id,
                          fileId,
                          offsetMs: Math.round(offset * 1000),
                          confirmed,
                          selectChinese,
                          operationId: uid(),
                        }),
                        (t) => setProgress(t.message),
                      ),
                    async (value) => {
                      await onImported(value);
                      setNotice("字幕已导入，个人词库与学习记录保持。");
                      setDownload(null);
                      clearPreview();
                    },
                  )
                }
              >
                确认导入字幕
              </button>
            </>
          )}
        </fieldset>
      )}
      {progress && <p role="status">{progress}</p>}
      {notice && <p role="status">{notice}</p>}
      {error && (
        <p className="error" role="alert">
          {error}
        </p>
      )}
    </details>
  );
}
