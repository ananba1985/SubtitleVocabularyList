import { useEffect, useState } from "react";
import { call, message } from "../api";
import type { KnownTarget, KnownTargetPage } from "../types";

const PAGE_SIZE = 20;
export function KnownTargetsManager({
  onChanged,
  refreshKey,
}: {
  onChanged: () => void;
  refreshKey: number;
}) {
  const [opened, setOpened] = useState(false);
  const [search, setSearch] = useState("");
  const [page, setPage] = useState(0);
  const [data, setData] = useState<
    (KnownTargetPage & { page: number; search: string }) | null
  >(null);
  const [error, setError] = useState("");
  const [loading, setLoading] = useState(false);
  const [revision, setRevision] = useState(0);
  const [busy, setBusy] = useState(false);
  useEffect(() => {
    if (!opened) return;
    let disposed = false;
    setLoading(true);
    setError("");
    setData(null);
    call<KnownTargetPage>("known_targets_list", {
      search,
      offset: page * PAGE_SIZE,
      limit: PAGE_SIZE,
    })
      .then((value) => {
        if (disposed) return;
        const last = Math.max(0, Math.ceil(value.total / PAGE_SIZE) - 1);
        if (page > last) setPage(last);
        else setData({ ...value, page, search });
      })
      .catch((value) => {
        if (!disposed) setError(message(value));
      })
      .finally(() => {
        if (!disposed) setLoading(false);
      });
    return () => {
      disposed = true;
    };
  }, [opened, search, page, revision, refreshKey]);
  async function restore(item: KnownTarget) {
    setBusy(true);
    setError("");
    try {
      await call("known_target_set", {
        kind: item.kind,
        text: item.text,
        known: false,
      });
      setRevision((value) => value + 1);
      onChanged();
    } catch (value) {
      setError(message(value));
    } finally {
      setBusy(false);
    }
  }
  return (
    <details
      className="task-history known-manager"
      onToggle={(event) => setOpened(event.currentTarget.open)}
    >
      <summary>管理已掌握内容</summary>
      {opened && (
        <div aria-busy={loading}>
          <p className="muted">
            在本机所有剧集、划词和 OCR
            中默认跳过。可随时恢复；原有词条和测验历史保留。不同词形分别标记。
          </p>
          <input
            aria-label="搜索已掌握内容"
            placeholder="搜索已掌握的单词、短语或句子"
            value={search}
            onChange={(event) => {
              setSearch(event.target.value);
              setPage(0);
            }}
          />
          {loading && <p role="status">正在读取…</p>}
          {error && (
            <div className="error" role="alert">
              <p>{error}</p>
              <button onClick={() => setRevision((value) => value + 1)}>
                重试
              </button>
            </div>
          )}
          {data && data.page === page && data.search === search && (
            <>
              <div className="known-list">
                {data.items.map((item) => (
                  <article key={`${item.kind}:${item.matchKey}`}>
                    <div>
                      <strong>{item.text}</strong>
                      <small>
                        {{ word: "单词", phrase: "短语", sentence: "整句" }[
                          item.kind
                        ] ?? item.kind}{" "}
                        · {new Date(item.markedAt).toLocaleDateString("zh-CN")}
                      </small>
                    </div>
                    <button disabled={busy} onClick={() => restore(item)}>
                      恢复展示
                    </button>
                  </article>
                ))}
                {!data.total && <p className="muted">没有匹配的已掌握内容。</p>}
              </div>
              {data.total > 0 && (
                <nav className="task-pagination" aria-label="已掌握内容分页">
                  <button
                    disabled={page === 0}
                    onClick={() => setPage((value) => value - 1)}
                  >
                    上一页
                  </button>
                  <span>
                    第 {page + 1} / {Math.ceil(data.total / PAGE_SIZE)} 页 · 共{" "}
                    {data.total} 条
                  </span>
                  <button
                    disabled={(page + 1) * PAGE_SIZE >= data.total}
                    onClick={() => setPage((value) => value + 1)}
                  >
                    下一页
                  </button>
                </nav>
              )}
            </>
          )}
        </div>
      )}
    </details>
  );
}
