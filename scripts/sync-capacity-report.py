"""Read-only size audit of existing sync snapshots and persisted audio.

Use a closed, checkpointed Data directory or copy. Does not export current
business facts, freeze a request, hash/read audio contents, open credentials or
connect to a site. Output contains aggregate sizes only.
"""
from __future__ import annotations

import argparse
import json
from pathlib import Path
import re
import sqlite3


def limits() -> dict[str, int]:
    source = (Path(__file__).resolve().parents[1] / "src-tauri/src/sync_limits.rs").read_text(encoding="utf-8")
    result = {}
    for name in ("MAX_PACKET_BYTES", "MAX_BUNDLE_BYTES", "MAX_AUDIO_BYTES"):
        match = re.search(r"\bconst\s+" + name + r"\s*:\s*usize\s*=\s*([\d_]+)\s*;", source)
        if match is None:
            raise ValueError(f"Cannot read {name} from the runtime limit definition")
        result[name] = int(match[1].replace("_", ""))
    return result


def size_group(connection: sqlite3.Connection, sql: str, limit: int) -> dict:
    count, total, maximum = connection.execute(
        "SELECT COUNT(*), COALESCE(SUM(bytes),0), COALESCE(MAX(bytes),0) FROM (" + sql + ")"
    ).fetchone()
    ratio = maximum / limit
    return {"count": count, "totalBytes": total, "maxBytes": maximum, "limitBytes": limit,
            "maxLimitRatio": round(ratio, 6), "level": "not_sampled" if not count else
            "over_limit" if ratio > 1 else "near_limit" if ratio >= 0.8 else "within_limit"}


def report(data: Path) -> dict:
    data = data.resolve(strict=True)
    database = data / "vocabulary.sqlite3"
    budget = limits()
    wal = database.with_name(database.name + "-wal")
    if wal.exists() and wal.stat().st_size:
        raise ValueError("Close the application and use a checkpointed Data directory before this offline audit")
    # Immutable mode prevents SQLite from creating WAL access sidecars in the
    # inspected directory; it is valid only for this closed/checkpointed input.
    connection = sqlite3.connect(database.as_uri() + "?mode=ro&immutable=1", uri=True)
    try:
        connection.execute("BEGIN")
        schema = connection.execute("PRAGMA user_version").fetchone()[0]
        if schema < 10:
            raise ValueError("This audit requires the schema 10 sync-growth metadata")
        groups = {
            "confirmedEntries": size_group(connection,
                "SELECT length(CAST(base_json AS BLOB)) AS bytes FROM sync_documents WHERE base_json IS NOT NULL", budget["MAX_BUNDLE_BYTES"]),
            "confirmedAssessments": size_group(connection,
                "SELECT length(CAST(json_extract(base_json,'$.bundle') AS BLOB)) AS bytes FROM assessment_sync_state WHERE base_json IS NOT NULL", budget["MAX_BUNDLE_BYTES"]),
            "frozenEntryRequests": size_group(connection,
                "SELECT length(CAST(payload_json AS BLOB)) AS bytes FROM sync_outbox WHERE state IN ('pending','conflict') AND payload_json<>'{}'", budget["MAX_PACKET_BYTES"]),
            "frozenAssessmentRequests": size_group(connection,
                "SELECT length(CAST(pending_json AS BLOB)) AS bytes FROM assessment_sync_state WHERE pending_json IS NOT NULL", budget["MAX_PACKET_BYTES"]),
        }
        facts = {name: connection.execute(f"SELECT COUNT(*) FROM {table}").fetchone()[0] for name, table in (
            ("entries", "entries"), ("reviewAttempts", "review_attempts"),
            ("assessmentSessions", "assessment_sessions"), ("assessmentAnswers", "assessment_answers"),
            ("assessmentCorrections", "assessment_corrections"), ("assessmentReports", "assessment_report_versions"))}
        facts["reportJsonBytes"] = connection.execute("SELECT COALESCE(SUM(length(CAST(report_json AS BLOB))),0) FROM assessment_report_versions").fetchone()[0]
        relative_paths = {row[0] for row in connection.execute("SELECT relative_path FROM media_assets WHERE state='ready'")}
        relative_paths.update(row[0] for row in connection.execute(
            "SELECT json_extract(snapshot_json,'$.content.audio.relativePath') FROM assessment_session_items WHERE json_extract(snapshot_json,'$.content.audio.kind')='original'"))
        connection.rollback()
    finally:
        connection.close()

    audio = {"files": 0, "totalBytes": 0, "maxBytes": 0, "missing": 0, "outsideDataDirectory": 0,
             "limitBytes": budget["MAX_AUDIO_BYTES"]}
    for relative in relative_paths:
        if not relative:
            continue
        path = (data / relative).resolve()
        if not path.is_relative_to(data):
            audio["outsideDataDirectory"] += 1
            continue
        if not path.is_file():
            audio["missing"] += 1
            continue
        size = path.stat().st_size
        audio["files"] += 1
        audio["totalBytes"] += size
        audio["maxBytes"] = max(audio["maxBytes"], size)
    ratio = audio["maxBytes"] / audio["limitBytes"]
    audio["maxLimitRatio"] = round(ratio, 6)
    audio["level"] = "not_sampled" if not audio["files"] else "over_limit" if ratio > 1 else "near_limit" if ratio >= 0.8 else "within_limit"
    return {"schemaVersion": schema, "limits": budget, "nearLimitRatio": 0.8,
            "snapshotScope": "Confirmed bundles and already frozen requests only; unfrozen changes are not projected. Counts include all locally stored account scopes.",
            "audioScope": "File metadata of persisted ready assets and original assessment snapshots; no audio contents or system voice caches are read.",
            "snapshots": groups, "persistedAudio": audio, "facts": facts,
            "readOnly": True, "networkRequests": 0,
            "limitations": "No lifetime/capacity forecast, new full export, boundary/stress test or guarantee of unlimited history."}


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--data-dir", type=Path, required=True, help="Existing Data directory")
    parser.add_argument("--output", type=Path, help="Optional private JSON report; outside the inspected Data directory")
    args = parser.parse_args()
    if args.output and args.output.resolve().is_relative_to(args.data_dir.resolve()):
        parser.error("Keep report output outside the inspected Data directory")
    result = report(args.data_dir)
    text = json.dumps(result, ensure_ascii=False, indent=2) + "\n"
    if args.output:
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(text, encoding="utf-8")
    print(text, end="")


if __name__ == "__main__":
    main()
