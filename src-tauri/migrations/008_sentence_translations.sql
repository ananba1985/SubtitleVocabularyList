CREATE TABLE sentence_translations (
    sentence TEXT PRIMARY KEY,
    translation TEXT NOT NULL,
    model_url TEXT NOT NULL,
    model_name TEXT NOT NULL,
    created_at INTEGER NOT NULL
);

-- Reuse previously saved full-sentence translations; keep every contextual meaning.
INSERT OR IGNORE INTO sentence_translations
    SELECT trim(context),translation,model_url,model_name,created_at
    FROM explanations
    WHERE trim(context)<>'' AND translation GLOB '*[㐀-䶿一-鿿]*'
    ORDER BY created_at,target;
