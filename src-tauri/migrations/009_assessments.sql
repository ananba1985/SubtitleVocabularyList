-- Assessment facts are independent from entries, regular reviews and reference dictionaries.
CREATE TABLE assessment_banks (
    id TEXT PRIMARY KEY,
    state TEXT NOT NULL CHECK(state IN ('draft','published')),
    revision INTEGER NOT NULL DEFAULT 1,
    sources_json TEXT NOT NULL,
    policy_version TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    published_at INTEGER
);
CREATE TABLE assessment_items (
    id TEXT PRIMARY KEY,
    bank_id TEXT NOT NULL REFERENCES assessment_banks(id),
    catalog_id TEXT NOT NULL,
    target_rank INTEGER NOT NULL CHECK(target_rank>0),
    target_key TEXT NOT NULL,
    lexeme_key TEXT NOT NULL,
    kind TEXT NOT NULL CHECK(kind IN ('word','phrase')),
    sense_key TEXT NOT NULL,
    question_type TEXT NOT NULL CHECK(question_type IN ('recognition','cloze','listening')),
    state TEXT NOT NULL CHECK(state IN ('draft','needs_review','approved','rejected')),
    revision INTEGER NOT NULL DEFAULT 1,
    content_json TEXT NOT NULL,
    review_note TEXT NOT NULL DEFAULT '',
    reviewed_at INTEGER,
    UNIQUE(bank_id,catalog_id,lexeme_key,question_type,sense_key)
);
CREATE INDEX assessment_items_inventory ON assessment_items(state,catalog_id,question_type,target_rank);
CREATE TRIGGER assessment_items_frozen_update BEFORE UPDATE ON assessment_items
 WHEN (SELECT state FROM assessment_banks WHERE id=OLD.bank_id)='published'
 BEGIN SELECT RAISE(ABORT,'published assessment items are immutable'); END;
CREATE TRIGGER assessment_items_frozen_delete BEFORE DELETE ON assessment_items
 WHEN (SELECT state FROM assessment_banks WHERE id=OLD.bank_id)='published'
 BEGIN SELECT RAISE(ABORT,'published assessment items are immutable'); END;
CREATE TRIGGER assessment_items_frozen_insert BEFORE INSERT ON assessment_items
 WHEN (SELECT state FROM assessment_banks WHERE id=NEW.bank_id)='published'
 BEGIN SELECT RAISE(ABORT,'published assessment items are immutable'); END;
CREATE TRIGGER assessment_banks_frozen BEFORE UPDATE ON assessment_banks
 WHEN OLD.state='published'
 BEGIN SELECT RAISE(ABORT,'published assessment banks are immutable'); END;
CREATE TABLE assessment_preparation_targets (
    bank_id TEXT NOT NULL REFERENCES assessment_banks(id),
    catalog_id TEXT NOT NULL,
    lexeme_key TEXT NOT NULL,
    state TEXT NOT NULL,
    error_json TEXT,
    updated_at INTEGER NOT NULL,
    PRIMARY KEY(bank_id,catalog_id,lexeme_key)
);
CREATE TABLE assessment_operations (
    operation_id TEXT PRIMARY KEY,
    kind TEXT NOT NULL,
    request_hash TEXT NOT NULL,
    result_json TEXT NOT NULL,
    created_at INTEGER NOT NULL
);
CREATE TABLE assessment_sessions (
    id TEXT PRIMARY KEY,
    state TEXT NOT NULL CHECK(state IN ('active','paused','completed','ended_early')),
    revision INTEGER NOT NULL,
    config_json TEXT NOT NULL,
    sampling_json TEXT NOT NULL,
    position INTEGER NOT NULL DEFAULT 0,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);
CREATE TABLE assessment_session_items (
    id TEXT PRIMARY KEY,
    session_id TEXT NOT NULL REFERENCES assessment_sessions(id),
    position INTEGER NOT NULL,
    snapshot_json TEXT NOT NULL,
    interaction_json TEXT NOT NULL DEFAULT '{}',
    UNIQUE(session_id,position)
);
CREATE TABLE assessment_answers (
    id TEXT PRIMARY KEY,
    session_id TEXT NOT NULL REFERENCES assessment_sessions(id),
    session_item_id TEXT NOT NULL UNIQUE REFERENCES assessment_session_items(id),
    answer_json TEXT NOT NULL,
    revision INTEGER NOT NULL DEFAULT 1,
    created_at INTEGER NOT NULL
);
CREATE TABLE assessment_corrections (
    id TEXT PRIMARY KEY,
    answer_id TEXT NOT NULL REFERENCES assessment_answers(id),
    correction_json TEXT NOT NULL,
    created_at INTEGER NOT NULL
);
CREATE TABLE assessment_report_versions (
    session_id TEXT NOT NULL REFERENCES assessment_sessions(id),
    revision INTEGER NOT NULL,
    report_json TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    PRIMARY KEY(session_id,revision)
);
CREATE TABLE assessment_collection_links (
    session_item_id TEXT NOT NULL REFERENCES assessment_session_items(id),
    operation_id TEXT NOT NULL,
    entry_id TEXT NOT NULL REFERENCES entries(id),
    link_json TEXT NOT NULL,
    PRIMARY KEY(session_item_id,operation_id)
);
CREATE TABLE assessment_sync_accounts (
    account_key TEXT PRIMARY KEY,
    cursor TEXT,
    capability TEXT,
    updated_at INTEGER NOT NULL
);
CREATE TABLE assessment_sync_state (
    account_key TEXT NOT NULL REFERENCES assessment_sync_accounts(account_key),
    session_id TEXT NOT NULL REFERENCES assessment_sessions(id),
    base_json TEXT,
    pending_json TEXT,
    pending_change_id TEXT,
    pending_digest TEXT,
    conflict_json TEXT,
    acknowledged_digest TEXT,
    PRIMARY KEY(account_key,session_id)
);
