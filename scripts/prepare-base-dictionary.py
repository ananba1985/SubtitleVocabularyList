"""Prepare the downloaded Chinese Google frequency dictionary and audit coverage.

Run: python -X utf8 scripts/prepare-base-dictionary.py --analyze
Only base.sqlite3 and analysis outputs are written; the vocabulary DB is read-only.
"""

import argparse
import csv
import hashlib
import json
import re
import sqlite3
import unicodedata
from collections import Counter, defaultdict
from datetime import datetime, timezone
from pathlib import Path
from urllib.request import Request, urlopen


SOURCES = [
    ("google-frequency", "chenny/english-word-10000", "a3bfa2903a2cbbbeb8cd3df5a3f6003578b08f8b", "english-word-10000.md", "7af875be3cd4907bb42b2d860be7658470f03058152ae3a003707b80554f276e"),
    ("google-frequency", "chenny/english-word-10000", "a3bfa2903a2cbbbeb8cd3df5a3f6003578b08f8b", "README.md", "5ddc099564428d3bbcd914b025693c318880aafd425ebb40fa1dcbaf651705d3"),
    ("ecdict-lemma", "skywind3000/ECDICT", "bc015ed2e24a7abef49fc6dbbb7fe32c1dadaf8b", "lemma.en.txt", "e255b097404e3e0052060e2ddf6e15a1414f577071d63d51d2ca0ce9dacee0fc"),
    ("ecdict-lemma", "skywind3000/ECDICT", "bc015ed2e24a7abef49fc6dbbb7fe32c1dadaf8b", "LICENSE", "f8552dd246f61a4e064569eae6194a01c6b3d63b03bf27c6ca863593c549ed0f"),
]
CANDIDATES_SQL = """SELECT o.source_id,s.title,o.kind,o.candidate_key,
       MAX(CASE WHEN k.key IS NULL THEN 0 ELSE 1 END) known,
       MAX(CASE WHEN x.target IS NULL THEN 0 ELSE 1 END) cached
FROM occurrences o JOIN sources s ON s.id=o.source_id
JOIN examples e ON e.id=o.example_id
LEFT JOIN settings k ON k.key='known_target:'||o.kind||':'||o.candidate_key
LEFT JOIN explanations x ON x.target=o.candidate_key AND x.context=''
WHERE s.corpus_path IS NOT NULL AND e.archived=0
GROUP BY o.source_id,s.title,o.kind,o.candidate_key"""


def normalize(text):
    return " ".join(unicodedata.normalize("NFKC", text).split()).lower()


def download(root):
    manifest = []
    for folder, repo, commit, name, expected in SOURCES:
        path = root / "sources" / folder / name
        url = f"https://raw.githubusercontent.com/{repo}/{commit}/{name}"
        if not path.exists() or hashlib.sha256(path.read_bytes()).hexdigest() != expected:
            path.parent.mkdir(parents=True, exist_ok=True)
            with urlopen(Request(url, headers={"User-Agent": "SubtitleVocabularyList"}), timeout=60) as response:
                data = response.read()
            if hashlib.sha256(data).hexdigest() != expected:
                raise ValueError(f"Source hash mismatch: {name}")
            path.write_bytes(data)
        manifest.append({"file": str(path), "url": url, "commit": commit,
                         "sha256": expected, "bytes": path.stat().st_size})
    (root / "sources" / "download-manifest.json").write_text(
        json.dumps({"recordedAt": datetime.now(timezone.utc).isoformat(), "files": manifest},
                   ensure_ascii=False, indent=2), encoding="utf-8")


def parse_sources(root):
    entries, raw_rows, missing_chinese = {}, 0, 0
    text = (root / "sources/google-frequency/english-word-10000.md").read_text(encoding="utf-8-sig")
    for line in text.splitlines():
        columns = [value.strip() for value in line.strip().strip("|").split("|")]
        if len(columns) != 9 or not columns[0].isdigit():
            continue
        raw_rows += 1
        rank, word, meaning, uk, us, pos = columns[:6]
        if not re.search(r"[\u3400-\u9fff]", meaning):
            missing_chinese += 1
            continue
        key = normalize(word)
        entries.setdefault(key, (key, meaning, uk, us, pos, int(rank)))
    lemma_candidates = defaultdict(set)
    text = (root / "sources/ecdict-lemma/lemma.en.txt").read_text(encoding="utf-8-sig")
    for line in text.splitlines():
        if line.startswith(";") or " -> " not in line:
            continue
        head, forms = line.split(" -> ", 1)
        lemma = normalize(head.rsplit("/", 1)[0])
        # Count ambiguity before restricting to our wordlist, to avoid false matches.
        for form in forms.split(","):
            form = normalize(form)
            if form and form != lemma:
                lemma_candidates[form].add(lemma)
    aliases = {form: next(iter(lemmas)) for form, lemmas in lemma_candidates.items()
               if len(lemmas) == 1 and next(iter(lemmas)) in entries and form not in entries}
    return entries, aliases, {"sourceRows": raw_rows, "validChineseEntries": len(entries),
                              "rowsWithoutChinese": missing_chinese,
                              "duplicateValidWords": raw_rows - missing_chinese - len(entries),
                              "unambiguousAliases": len(aliases)}


def build(root, entries, aliases, profile):
    target = root / "base.sqlite3"
    temporary = root / "base.build.sqlite3"
    temporary.unlink(missing_ok=True)
    with sqlite3.connect(temporary) as connection:
        connection.executescript("""PRAGMA user_version=1;
CREATE TABLE dictionary_entries(word TEXT PRIMARY KEY,meaning TEXT NOT NULL,
 phonetic_uk TEXT NOT NULL,phonetic_us TEXT NOT NULL,pos TEXT NOT NULL,rank INTEGER NOT NULL);
CREATE TABLE dictionary_aliases(form TEXT PRIMARY KEY,lemma TEXT NOT NULL REFERENCES dictionary_entries(word));
CREATE TABLE metadata(key TEXT PRIMARY KEY,value TEXT NOT NULL);""")
        connection.executemany("INSERT INTO dictionary_entries VALUES (?,?,?,?,?,?)", entries.values())
        connection.executemany("INSERT INTO dictionary_aliases VALUES (?,?)", sorted(aliases.items()))
        connection.executemany("INSERT INTO metadata VALUES (?,?)", [
            ("source", "Google 高频词中文表"),
            ("source_url", "https://github.com/chenny/english-word-10000"),
            ("source_commit", SOURCES[0][2]),
            ("lemma_commit", SOURCES[2][2]),
            ("profile", json.dumps(profile, ensure_ascii=False)),
        ])
    connection.close()
    temporary.replace(target)
    return target


def analyze(data_dir, output, entries, aliases, profile):
    # Snapshot includes committed WAL rows without upgrading or writing the source.
    source = sqlite3.connect((data_dir / "vocabulary.sqlite3").resolve().as_uri() + "?mode=ro", uri=True)
    snapshot = sqlite3.connect(":memory:")
    source.backup(snapshot)
    source.close()
    rows = snapshot.execute(CANDIDATES_SQL).fetchall()
    grouped, episodes = {}, defaultdict(Counter)
    for source_id, title, kind, key, known, cached in rows:
        grouped[(kind, key)] = (known, cached)
        category = "direct" if key in entries else "lemma" if key in aliases else "unmatched"
        episodes[(source_id, title)][category] += 1
    counts, by_kind, details = Counter(), defaultdict(Counter), []
    ai_pending = unmatched_cached = unmatched_known = 0
    for (kind, key), (known, cached) in sorted(grouped.items()):
        category = "direct" if key in entries else "lemma" if key in aliases else "unmatched"
        counts[category] += 1
        by_kind[kind][category] += 1
        matched = key if category == "direct" else aliases.get(key, "")
        meaning = entries[matched][1] if matched else ""
        if category == "unmatched":
            unmatched_known += bool(known)
            unmatched_cached += bool(cached and not known)
            ai_pending += not (known or cached)
        details.append({"kind": kind, "candidate": key, "category": category,
                        "matchedWord": matched, "meaning": meaning,
                        "known": bool(known), "cached": bool(cached)})
    assert sum(counts.values()) == len(grouped)
    summary = {"analyzedAt": datetime.now(timezone.utc).isoformat(), "sourceDatabase": str(data_dir / "vocabulary.sqlite3"),
               "dictionary": profile, "episodeCount": len(episodes), "uniqueCandidates": len(grouped),
               "sourceCandidateSum": len(rows), "coverage": dict(counts),
               "byKind": {kind: dict(values) for kind, values in by_kind.items()},
               "unmatchedAlreadyKnown": unmatched_known, "unmatchedAlreadyCached": unmatched_cached,
               "pendingAiWordMeanings": ai_pending,
               "episodes": [{"title": title, **dict(values), "total": sum(values.values())}
                            for (_, title), values in sorted(episodes.items(), key=lambda item: item[0][1])]}
    output.mkdir(parents=True, exist_ok=True)
    (output / "coverage.json").write_text(json.dumps(summary, ensure_ascii=False, indent=2), encoding="utf-8")
    with (output / "candidates.csv").open("w", encoding="utf-8-sig", newline="") as stream:
        writer = csv.DictWriter(stream, fieldnames=list(details[0]) if details else ["candidate"])
        writer.writeheader()
        writer.writerows(details)
    snapshot.close()
    return summary


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--data-dir", type=Path, default=Path(r"D:\SubtitleVocabularyList\Data"))
    parser.add_argument("--analyze", action="store_true")
    parser.add_argument("--output", type=Path, default=Path(".local/dictionary-coverage"))
    options = parser.parse_args()
    root = options.data_dir / "dictionaries"
    root.mkdir(parents=True, exist_ok=True)
    download(root)
    entries, aliases, profile = parse_sources(root)
    target = build(root, entries, aliases, profile)
    summary = analyze(options.data_dir, options.output, entries, aliases, profile) if options.analyze else profile
    print(json.dumps({"baseDictionary": str(target), "result": summary}, ensure_ascii=False, indent=2))


if __name__ == "__main__":
    main()
