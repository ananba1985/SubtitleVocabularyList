CREATE TABLE explanations (
    target TEXT NOT NULL,
    context TEXT NOT NULL,
    meaning TEXT NOT NULL,
    translation TEXT NOT NULL,
    notes TEXT NOT NULL,
    model_url TEXT NOT NULL,
    model_name TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    PRIMARY KEY(target, context)
);
