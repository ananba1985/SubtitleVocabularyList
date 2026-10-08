import type { SourceSummary } from "./types";

export type SourceChoice = {
  source: SourceSummary;
  episode: number | null;
  label: string;
};
export type SourceSeason = {
  key: string;
  number: number | null;
  choices: SourceChoice[];
};
export type SourceSeries = {
  key: string;
  name: string;
  seasons: SourceSeason[];
};
const compare = new Intl.Collator("zh-CN", {
  numeric: true,
  sensitivity: "base",
});
const clean = (text: string) =>
  text.replace(/[._]+/g, " ").replace(/\s+/g, " ").trim();
const patterns = [
  /\bS(\d{1,2})[ ._-]*E(\d{1,3})(?:[ ._-]*E(\d{1,3}))?\b/i,
  /\b(\d{1,2})x(\d{2,3})\b/i,
  /第(\d{1,2})季\s*第?(\d{1,3})[集话]/,
];

export function sourceIdentity(source: SourceSummary) {
  const parts = source.title.split(/\s+·\s+(?=音轨\s+\d)/);
  const title = parts[0]
    .normalize("NFKC")
    .replace(/_/g, " ")
    .replace(/\.(mkv|mp4|mov|webm|avi|m4v)$/i, "");
  const match = patterns.map((pattern) => title.match(pattern)).find(Boolean);
  const prefix = match
    ? clean(
        title
          .slice(0, match.index)
          .replace(/[\s\-[(]+$/g, "")
          .replace(/^\[([^\[\]]+)\]$/, "$1"),
      )
    : "";
  if (!match || !prefix)
    return {
      seriesKey: "other",
      seriesName: "其他视频",
      season: null,
      episode: null,
      label: source.title,
    };
  const season = Number(match[1]);
  const episode = Number(match[2]);
  const end = match[3] ? Number(match[3]) : null;
  const name = clean(
    title
      .slice((match.index ?? 0) + match[0].length)
      .replace(/^[\s\-[\]()]+/g, ""),
  );
  const range = end !== null ? `–${String(end).padStart(2, "0")}` : "";
  const variant = parts[1] ? ` · ${parts[1]}` : "";
  return {
    seriesKey: `series:${prefix.normalize("NFKC").toLocaleLowerCase()}`,
    seriesName: prefix,
    season,
    episode,
    label: `第 ${String(episode).padStart(2, "0")}${range} 集${name ? ` · ${name}` : ""}${variant}`,
  };
}

export function groupSources(sources: SourceSummary[]): SourceSeries[] {
  const groups = new Map<string, SourceSeries>();
  for (const source of sources) {
    const identity = sourceIdentity(source);
    let series = groups.get(identity.seriesKey);
    if (!series) {
      series = {
        key: identity.seriesKey,
        name: identity.seriesName,
        seasons: [],
      };
      groups.set(series.key, series);
    }
    const key = identity.season === null ? "unmarked" : String(identity.season);
    let season = series.seasons.find((value) => value.key === key);
    if (!season) {
      season = { key, number: identity.season, choices: [] };
      series.seasons.push(season);
    }
    season.choices.push({
      source,
      episode: identity.episode,
      label: identity.label,
    });
  }
  for (const series of groups.values()) {
    series.seasons.sort(
      (a, b) => (a.number ?? Infinity) - (b.number ?? Infinity),
    );
    for (const season of series.seasons)
      season.choices.sort(
        (a, b) =>
          (a.episode ?? Infinity) - (b.episode ?? Infinity) ||
          compare.compare(a.label, b.label) ||
          b.source.importedAt - a.source.importedAt ||
          compare.compare(a.source.id, b.source.id),
      );
    for (const season of series.seasons) {
      const totals = new Map<string, number>();
      const versions = new Map<string, number>();
      for (const choice of season.choices)
        totals.set(choice.label, (totals.get(choice.label) ?? 0) + 1);
      for (const choice of season.choices) {
        const label = choice.label;
        if ((totals.get(label) ?? 0) > 1) {
          const version = (versions.get(label) ?? 0) + 1;
          versions.set(label, version);
          choice.label = `${label} · 资料版本 ${version}`;
        }
      }
    }
  }
  return [...groups.values()].sort(
    (a, b) =>
      Number(a.key === "other") - Number(b.key === "other") ||
      compare.compare(a.name, b.name),
  );
}

export function filterSourceGroups(
  groups: SourceSeries[],
  query: string,
): SourceSeries[] {
  const words = clean(query)
    .normalize("NFKC")
    .toLocaleLowerCase()
    .split(" ")
    .filter(Boolean);
  if (!words.length) return groups;
  return groups
    .map((series) => ({
      ...series,
      seasons: series.seasons
        .map((season) => ({
          ...season,
          choices: season.choices.filter(({ source, label, episode }) => {
            const code =
              season.number === null || episode === null
                ? ""
                : `S${String(season.number).padStart(2, "0")}E${String(episode).padStart(2, "0")} 第${season.number}季`;
            const text = clean(
              `${series.name} ${source.title} ${label} ${code}`,
            )
              .normalize("NFKC")
              .toLocaleLowerCase();
            return words.every((word) => text.includes(word));
          }),
        }))
        .filter((season) => season.choices.length),
    }))
    .filter((series) => series.seasons.length);
}

export const sourceCount = (series: SourceSeries) =>
  series.seasons.reduce((count, season) => count + season.choices.length, 0);
