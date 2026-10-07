CREATE TABLE entries (
    id TEXT PRIMARY KEY NOT NULL,
    kind TEXT NOT NULL CHECK (kind IN ('word', 'phrase', 'sentence')),
    text TEXT NOT NULL,
    match_key TEXT NOT NULL,
    revision INTEGER NOT NULL DEFAULT 1,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);
CREATE INDEX entries_match ON entries(kind, match_key);
CREATE TABLE meanings (
    id TEXT PRIMARY KEY NOT NULL,
    entry_id TEXT NOT NULL REFERENCES entries(id),
    text TEXT NOT NULL,
    origin TEXT NOT NULL,
    revision INTEGER NOT NULL DEFAULT 1,
    created_at INTEGER NOT NULL,
    UNIQUE(entry_id, text),
    UNIQUE(id, entry_id)
);
CREATE TABLE sources (
    id TEXT PRIMARY KEY NOT NULL,
    kind TEXT NOT NULL,
    title TEXT NOT NULL,
    fingerprint TEXT NOT NULL,
    path_hint TEXT,
    duration_ms INTEGER,
    created_at INTEGER NOT NULL,
    UNIQUE(kind, fingerprint)
);
CREATE TABLE examples (
    id TEXT PRIMARY KEY NOT NULL,
    source_id TEXT REFERENCES sources(id),
    location_key TEXT NOT NULL,
    identity_key TEXT NOT NULL UNIQUE,
    text TEXT NOT NULL,
    start_ms INTEGER,
    end_ms INTEGER,
    revision INTEGER NOT NULL DEFAULT 1,
    created_at INTEGER NOT NULL,
    CHECK ((start_ms IS NULL AND end_ms IS NULL) OR (start_ms IS NOT NULL AND end_ms IS NOT NULL AND start_ms >= 0 AND end_ms > start_ms))
);
CREATE TABLE entry_examples (
    entry_id TEXT NOT NULL REFERENCES entries(id),
    example_id TEXT NOT NULL REFERENCES examples(id),
    meaning_id TEXT,
    context_meaning TEXT NOT NULL DEFAULT '',
    PRIMARY KEY(entry_id, example_id),
    FOREIGN KEY(meaning_id, entry_id) REFERENCES meanings(id, entry_id)
);
CREATE TABLE occurrences (
    id TEXT PRIMARY KEY NOT NULL,
    source_id TEXT NOT NULL REFERENCES sources(id),
    example_id TEXT NOT NULL REFERENCES examples(id),
    candidate_key TEXT NOT NULL,
    raw_text TEXT NOT NULL,
    token_start INTEGER NOT NULL,
    token_end INTEGER NOT NULL,
    entry_id TEXT REFERENCES entries(id),
    UNIQUE(source_id, example_id, candidate_key, token_start, token_end)
);
CREATE INDEX occurrences_candidates ON occurrences(source_id, candidate_key);
CREATE TABLE candidate_decisions (
    source_id TEXT NOT NULL REFERENCES sources(id),
    candidate_key TEXT NOT NULL,
    scope_key TEXT NOT NULL,
    decision TEXT NOT NULL CHECK (decision IN ('familiar', 'uncertain', 'collected')),
    updated_at INTEGER NOT NULL,
    PRIMARY KEY(source_id, candidate_key, scope_key)
);
CREATE TABLE media_assets (
    id TEXT PRIMARY KEY NOT NULL,
    recipe_key TEXT NOT NULL UNIQUE,
    digest TEXT NOT NULL,
    relative_path TEXT NOT NULL,
    kind TEXT NOT NULL DEFAULT 'original',
    format TEXT NOT NULL,
    duration_ms INTEGER NOT NULL,
    state TEXT NOT NULL CHECK(state IN ('ready', 'missing')),
    created_at INTEGER NOT NULL
);
CREATE INDEX media_digest ON media_assets(digest);
CREATE TABLE example_media (
    example_id TEXT NOT NULL REFERENCES examples(id),
    asset_id TEXT NOT NULL REFERENCES media_assets(id),
    PRIMARY KEY(example_id, asset_id)
);
CREATE TABLE collection_actions (
    operation_id TEXT PRIMARY KEY NOT NULL,
    request_hash TEXT NOT NULL,
    entry_id TEXT NOT NULL REFERENCES entries(id),
    source_id TEXT REFERENCES sources(id),
    result_json TEXT NOT NULL,
    created_at INTEGER NOT NULL
);
CREATE INDEX collection_entry ON collection_actions(entry_id, created_at);
CREATE TABLE learning_units (
    id TEXT PRIMARY KEY NOT NULL,
    entry_id TEXT NOT NULL REFERENCES entries(id),
    scope_key TEXT NOT NULL,
    dimension TEXT NOT NULL CHECK (dimension IN ('meaning', 'listening')),
    UNIQUE(entry_id, scope_key, dimension)
);
CREATE TABLE review_attempts (
    id TEXT PRIMARY KEY NOT NULL,
    operation_id TEXT NOT NULL UNIQUE,
    request_hash TEXT NOT NULL,
    unit_id TEXT NOT NULL REFERENCES learning_units(id),
    question_json TEXT NOT NULL,
    answer TEXT NOT NULL,
    hinted INTEGER NOT NULL CHECK (hinted IN (0, 1)),
    outcome TEXT NOT NULL,
    grader TEXT NOT NULL,
    policy_version TEXT NOT NULL,
    state_before_json TEXT NOT NULL,
    revision INTEGER NOT NULL DEFAULT 1,
    created_at INTEGER NOT NULL
);
CREATE INDEX review_unit ON review_attempts(unit_id, created_at);
CREATE TABLE review_corrections (
    id TEXT PRIMARY KEY NOT NULL,
    operation_id TEXT NOT NULL UNIQUE,
    attempt_id TEXT NOT NULL REFERENCES review_attempts(id),
    expected_revision INTEGER NOT NULL,
    outcome TEXT NOT NULL,
    reason TEXT NOT NULL,
    created_at INTEGER NOT NULL
);
CREATE TABLE review_states (
    unit_id TEXT PRIMARY KEY NOT NULL REFERENCES learning_units(id),
    revision INTEGER NOT NULL DEFAULT 1,
    state_json TEXT NOT NULL,
    due_at INTEGER NOT NULL,
    relearn_at INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX reviews_due ON review_states(due_at);
CREATE TABLE tasks (
    id TEXT PRIMARY KEY NOT NULL,
    operation_id TEXT NOT NULL,
    kind TEXT NOT NULL,
    stage TEXT NOT NULL,
    state TEXT NOT NULL,
    snapshot_json TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);
CREATE INDEX tasks_operation ON tasks(operation_id, kind);
CREATE TABLE settings (key TEXT PRIMARY KEY NOT NULL, value_json TEXT NOT NULL);
CREATE TABLE sync_outbox (
    change_id TEXT PRIMARY KEY NOT NULL,
    entity_kind TEXT NOT NULL,
    entity_id TEXT NOT NULL,
    base_revision INTEGER NOT NULL,
    payload_json TEXT NOT NULL,
    state TEXT NOT NULL DEFAULT 'pending',
    created_at INTEGER NOT NULL
);
CREATE INDEX sync_pending ON sync_outbox(state, created_at);
CREATE TABLE sync_links (
    account_scope TEXT NOT NULL,
    remote_id TEXT NOT NULL,
    entity_kind TEXT NOT NULL,
    local_id TEXT NOT NULL,
    PRIMARY KEY(account_scope, remote_id, entity_kind)
);
CREATE TABLE sync_state (account_scope TEXT PRIMARY KEY NOT NULL, state_json TEXT NOT NULL);
