CREATE TABLE entry_examples_next (
    entry_id TEXT NOT NULL REFERENCES entries(id),
    example_id TEXT NOT NULL REFERENCES examples(id),
    scope_key TEXT NOT NULL DEFAULT 'entry',
    meaning_id TEXT,
    context_meaning TEXT NOT NULL DEFAULT '',
    created_at INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY(entry_id,example_id,scope_key),
    FOREIGN KEY(meaning_id,entry_id) REFERENCES meanings(id,entry_id)
);
INSERT INTO entry_examples_next(entry_id,example_id,scope_key,meaning_id,context_meaning,created_at)
SELECT ee.entry_id,ee.example_id,COALESCE(ee.meaning_id,'entry'),ee.meaning_id,ee.context_meaning,e.created_at
FROM entry_examples ee JOIN examples e ON e.id=ee.example_id;
DROP TABLE entry_examples;
ALTER TABLE entry_examples_next RENAME TO entry_examples;
CREATE INDEX entry_examples_by_example ON entry_examples(example_id);
