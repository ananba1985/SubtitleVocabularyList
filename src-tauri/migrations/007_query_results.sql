CREATE TABLE query_results (
    cache_key TEXT PRIMARY KEY,
    provider_id TEXT NOT NULL,
    result_json TEXT NOT NULL,
    created_at INTEGER NOT NULL
);
