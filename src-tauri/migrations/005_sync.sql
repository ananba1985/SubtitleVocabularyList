ALTER TABLE entries ADD COLUMN archived INTEGER NOT NULL DEFAULT 0 CHECK(archived IN (0,1));
CREATE TABLE sync_dirty_entries (
    entry_id TEXT PRIMARY KEY NOT NULL REFERENCES entries(id),
    epoch INTEGER NOT NULL DEFAULT 1,
    updated_at INTEGER NOT NULL
);
INSERT INTO sync_dirty_entries(entry_id,updated_at) SELECT id,updated_at FROM entries;
CREATE TABLE sync_documents (
    account_scope TEXT NOT NULL,
    remote_id TEXT NOT NULL,
    local_id TEXT NOT NULL REFERENCES entries(id),
    remote_revision INTEGER NOT NULL DEFAULT 0,
    base_json TEXT,
    acknowledged_epoch INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY(account_scope,remote_id)
);
CREATE INDEX sync_documents_local ON sync_documents(account_scope,local_id);
ALTER TABLE sync_outbox ADD COLUMN account_scope TEXT NOT NULL DEFAULT '';
ALTER TABLE sync_outbox ADD COLUMN local_epoch INTEGER NOT NULL DEFAULT 0;
CREATE INDEX sync_outbox_account ON sync_outbox(account_scope,state,created_at);
CREATE TABLE sync_conflicts (
    id TEXT PRIMARY KEY NOT NULL,
    account_scope TEXT NOT NULL,
    remote_id TEXT NOT NULL,
    local_id TEXT REFERENCES entries(id),
    remote_revision INTEGER NOT NULL,
    remote_json TEXT NOT NULL,
    local_json TEXT,
    reason TEXT NOT NULL,
    state TEXT NOT NULL DEFAULT 'pending',
    created_at INTEGER NOT NULL,
    UNIQUE(account_scope,remote_id,state)
);
