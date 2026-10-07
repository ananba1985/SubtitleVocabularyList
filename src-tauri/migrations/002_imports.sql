ALTER TABLE sources ADD COLUMN corpus_path TEXT;
ALTER TABLE sources ADD COLUMN audio_path TEXT;
ALTER TABLE sources ADD COLUMN text_source TEXT;
ALTER TABLE sources ADD COLUMN imported_at INTEGER;
ALTER TABLE occurrences ADD COLUMN kind TEXT NOT NULL DEFAULT 'word';
ALTER TABLE examples ADD COLUMN archived INTEGER NOT NULL DEFAULT 0;
CREATE UNIQUE INDEX tasks_one_effective_operation ON tasks(operation_id, kind)
WHERE state IN ('queued','running','cancel_requested','succeeded');
