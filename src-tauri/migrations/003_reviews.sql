ALTER TABLE learning_units ADD COLUMN created_at INTEGER NOT NULL DEFAULT 0;
UPDATE learning_units SET created_at=COALESCE(
    (SELECT created_at FROM meanings WHERE id=learning_units.scope_key),
    (SELECT created_at FROM entries WHERE id=learning_units.entry_id),0);
ALTER TABLE review_attempts ADD COLUMN state_after_json TEXT NOT NULL DEFAULT '{}';
ALTER TABLE review_corrections ADD COLUMN request_hash TEXT NOT NULL DEFAULT '';
CREATE TABLE review_questions (
    id TEXT PRIMARY KEY NOT NULL,
    unit_id TEXT NOT NULL REFERENCES learning_units(id),
    state_revision INTEGER NOT NULL,
    snapshot_json TEXT NOT NULL,
    hinted INTEGER NOT NULL DEFAULT 0 CHECK(hinted IN (0,1)),
    audio_prepared INTEGER NOT NULL DEFAULT 0 CHECK(audio_prepared IN (0,1)),
    attempt_id TEXT REFERENCES review_attempts(id),
    created_at INTEGER NOT NULL
);
CREATE INDEX review_question_unit ON review_questions(unit_id,created_at);
