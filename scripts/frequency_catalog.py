"""Versioned frequency indexes, independent of dictionary translation coverage."""

import hashlib
import json
from pathlib import Path
from urllib.request import Request, urlopen

WORD_CATALOG = "google-words-a3bfa290"
PHRASE_CATALOG = "google-bigrams-781c0596"
BIGRAM_URL = "https://norvig.com/ngrams/count_2w.txt"
BIGRAM_SHA = "781c0596c3eea532d30bef9f3dba1d5137d652f00376260822c761a7584dfb8c"
SOURCE_NOTICE = (
    "Frequency counts: Peter Norvig's public subset derived from Google Web Trillion Word Corpus. "
    "https://norvig.com/ngrams/ describes the code as MIT; it does not expressly apply that license to the corpus. "
    "Keep source attribution; this index is not a claim to redistribute the full Google/LDC corpus. "
    "Original frequency rank is descending count, ties sorted by source text. "
    "Natural phrase meanings and examples are separately authored in assessment-phrases.json; "
    "unreviewed bigrams are not eligible phrases."
)


def prepare_bigram(root):
    path = root / "sources/google-bigrams/count_2w.txt"
    if not path.exists() or hashlib.sha256(path.read_bytes()).hexdigest() != BIGRAM_SHA:
        path.parent.mkdir(parents=True, exist_ok=True)
        with urlopen(Request(BIGRAM_URL, headers={"User-Agent": "SubtitleVocabularyList"}), timeout=60) as r:
            data = r.read()
        if hashlib.sha256(data).hexdigest() != BIGRAM_SHA:
            raise ValueError("Google bigram subset changed: review source version before importing")
        path.write_bytes(data)
    return path


def build_frequency(connection, sources, normalize, lemma_candidates):
    connection.executescript("""
CREATE TABLE frequency_catalogs(
 id TEXT PRIMARY KEY,kind TEXT NOT NULL,title TEXT NOT NULL,source_url TEXT NOT NULL,
 source_version TEXT NOT NULL,source_sha256 TEXT NOT NULL,rank_unit TEXT NOT NULL,
 raw_rows INTEGER NOT NULL,max_rank INTEGER NOT NULL,notice TEXT NOT NULL);
CREATE TABLE frequency_targets(
 catalog_id TEXT NOT NULL REFERENCES frequency_catalogs(id),rank INTEGER NOT NULL,
 text TEXT NOT NULL,target_key TEXT NOT NULL,lexeme_key TEXT NOT NULL,mapping_basis TEXT NOT NULL,
 frequency INTEGER,eligible INTEGER NOT NULL,seed_json TEXT,
 PRIMARY KEY(catalog_id,rank));
CREATE INDEX frequency_lexeme ON frequency_targets(catalog_id,lexeme_key,rank);
CREATE INDEX frequency_eligible ON frequency_targets(catalog_id,eligible,rank);
PRAGMA user_version=2;
""")
    word_path = sources / "sources/google-frequency/english-word-10000.md"
    words = []
    for line in word_path.read_text(encoding="utf-8-sig").splitlines():
        row = [c.strip() for c in line.strip().strip("|").split("|")]
        if len(row) == 9 and row[0].isdigit():
            words.append((int(row[0]), row[1]))
    keys = {normalize(text) for _, text in words}
    aliases = {form: next(iter(lemmas)) for form, lemmas in lemma_candidates(sources).items()
               if len(lemmas) == 1 and next(iter(lemmas)) in keys}
    connection.execute("INSERT INTO frequency_catalogs VALUES (?,?,?,?,?,?,?,?,?,?)", (
        WORD_CATALOG, "word", "Google 高频单词原排名", "https://github.com/chenny/english-word-10000",
        "a3bfa2903a2cbbbeb8cd3df5a3f6003578b08f8b", hashlib.sha256(word_path.read_bytes()).hexdigest(),
        "upstream_word_rank", len(words), max(r for r, _ in words),
        "原始词表排名保留，包括中文缺项；词形归并仅用 ECDICT lemma.en.txt 的明确单一映射。"))
    connection.executemany("INSERT INTO frequency_targets VALUES (?,?,?,?,?,?,?,?,?)", (
        (WORD_CATALOG, rank, text, normalize(text), aliases.get(normalize(text), normalize(text)),
         "ecdict-lemma-bc015ed2" if normalize(text) in aliases else "identity", None, 1, None)
        for rank, text in words))
    path = prepare_bigram(sources)
    rows = sorted((line.split("\t") for line in path.read_text(encoding="utf-8").splitlines()),
                  key=lambda row: (-int(row[1]), row[0]))
    seeds = json.loads((Path(__file__).resolve().parents[1] /
                        "src-tauri/resources/assessment-phrases.json").read_text(encoding="utf-8"))
    missing = set(seeds) - {text for text, _ in rows}
    if missing:
        raise ValueError(f"Curated phrase not in frequency source: {sorted(missing)}")
    connection.execute("INSERT INTO frequency_catalogs VALUES (?,?,?,?,?,?,?,?,?,?)", (
        PHRASE_CATALOG, "phrase", "Google 双词频次／自然短语筛选", BIGRAM_URL, BIGRAM_SHA[:16],
        BIGRAM_SHA, "descending_bigram_count_tie_text", len(rows), len(rows), SOURCE_NOTICE))
    connection.executemany("INSERT INTO frequency_targets VALUES (?,?,?,?,?,?,?,?,?)", (
        (PHRASE_CATALOG, rank, text, normalize(text), normalize(text), "identity", int(count),
         int(text in seeds), json.dumps(seeds[text], ensure_ascii=False) if text in seeds else None)
        for rank, (text, count) in enumerate(rows, 1)))
    return {"wordRows": len(words), "wordMaxRank": max(r for r, _ in words),
            "bigramRows": len(rows), "naturalPhrases": len(seeds), "bigramSha256": BIGRAM_SHA}
