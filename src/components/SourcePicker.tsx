import { useEffect, useMemo, useState, type ReactNode } from "react";
import type { SourceSummary } from "../types";
import { filterSourceGroups, groupSources, sourceCount } from "../sourceGroups";

export function SourcePicker({
  sources,
  sourceId,
  onSelect,
  action,
}: {
  sources: SourceSummary[];
  sourceId: string;
  onSelect: (id: string) => void;
  action?: ReactNode;
}) {
  const [query, setQuery] = useState("");
  const groups = useMemo(() => groupSources(sources), [sources]);
  const filtered = useMemo(
    () => filterSourceGroups(groups, query),
    [groups, query],
  );
  const series = filtered.find((group) =>
    group.seasons.some((season) =>
      season.choices.some(({ source }) => source.id === sourceId),
    ),
  );
  const season = series?.seasons.find((value) =>
    value.choices.some(({ source }) => source.id === sourceId),
  );
  const selected = sources.find((source) => source.id === sourceId);
  // Imports and refreshes retain a valid selection; only an unavailable choice needs a fallback.
  useEffect(() => {
    if (!series) {
      const next = filtered[0]?.seasons[0]?.choices[0]?.source.id ?? "";
      if (next !== sourceId) onSelect(next);
    }
  }, [filtered, series, sourceId, onSelect]);
  function search(value: string) {
    setQuery(value);
    const matches = filterSourceGroups(groups, value);
    const current = matches.some((group) =>
      group.seasons.some((season) =>
        season.choices.some(({ source }) => source.id === sourceId),
      ),
    );
    if (!current) onSelect(matches[0]?.seasons[0]?.choices[0]?.source.id ?? "");
  }
  return (
    <section className="source-picker" aria-label="剧集资料选择">
      <div className="source-search">
        <input
          aria-label="搜索剧名或集名"
          placeholder="搜索剧名、集名或 S01E24"
          value={query}
          onChange={(event) => search(event.target.value)}
        />
        {query && (
          <button className="text-button" onClick={() => search("")}>
            清除搜索
          </button>
        )}
        {action}
      </div>
      <div className="source-filters">
        <label>
          剧名
          <select
            aria-label="选择剧名"
            value={series?.key ?? ""}
            disabled={!filtered.length}
            onChange={(event) => {
              const next = filtered.find(
                (value) => value.key === event.target.value,
              );
              if (next) onSelect(next.seasons[0].choices[0].source.id);
            }}
          >
            {!series && <option value="">请选择剧名</option>}
            {filtered.map((group) => (
              <option key={group.key} value={group.key}>
                {group.name} · {sourceCount(group)} 份资料
              </option>
            ))}
          </select>
        </label>
        <label>
          季
          <select
            aria-label="选择季"
            value={season?.key ?? ""}
            disabled={!series || series.seasons.length < 2}
            onChange={(event) => {
              const next = series?.seasons.find(
                (value) => value.key === event.target.value,
              );
              if (next) onSelect(next.choices[0].source.id);
            }}
          >
            {!season && <option value="">请选择季</option>}
            {series?.seasons.map((value) => (
              <option key={value.key} value={value.key}>
                {value.number === null
                  ? "未标注季集"
                  : value.number === 0
                    ? "特别篇（第 0 季）"
                    : `第 ${value.number} 季`}{" "}
                · {value.choices.length} 份资料
              </option>
            ))}
          </select>
        </label>
        <label>
          集 / 资料版本
          <select
            aria-label="选择剧集"
            value={selected && season ? sourceId : ""}
            disabled={!season}
            title={selected?.title}
            onChange={(event) => onSelect(event.target.value)}
          >
            {(!selected || !season) && <option value="">请选择剧集</option>}
            {season?.choices.map(({ source, label }) => (
              <option key={source.id} value={source.id}>
                {label} · {source.candidateCount} 个候选
              </option>
            ))}
          </select>
        </label>
      </div>
      {!filtered.length && (
        <p className="muted" role="status">
          没有匹配的剧集资料，请调整或清除搜索。
        </p>
      )}
      {selected && (
        <p className="source-current" title={selected.title}>
          当前预习：{selected.title}
        </p>
      )}
    </section>
  );
}
