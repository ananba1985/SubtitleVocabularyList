use crate::{error::AppError, store::Store};
use chrono::Utc;
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use unicode_normalization::UnicodeNormalization;
use uuid::Uuid;

pub fn normalize(text: &str) -> String {
    text.nfkc()
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

pub fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn identity() -> String {
    Uuid::new_v4().to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceInput {
    pub kind: String,
    pub title: String,
    pub fingerprint: String,
    pub path_hint: Option<String>,
    pub duration_ms: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ExampleInput {
    pub text: String,
    #[serde(default)]
    pub context_meaning: String,
    pub source_id: Option<String>,
    #[serde(default)]
    pub location_key: String,
    pub start_ms: Option<i64>,
    pub end_ms: Option<i64>,
    #[serde(default)]
    pub media_asset_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CollectionInput {
    pub operation_id: String,
    pub kind: String,
    pub text: String,
    #[serde(default)]
    pub meaning: String,
    #[serde(default)]
    pub examples: Vec<ExampleInput>,
    pub target_entry_id: Option<String>,
    pub expected_revision: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CollectionResult {
    pub entry_id: String,
    pub operation_id: String,
    pub revision: i64,
    pub created: bool,
    pub example_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Meaning {
    pub id: String,
    pub text: String,
    pub origin: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AudioAsset {
    pub id: String,
    pub relative_path: String,
    pub duration_ms: i64,
    pub kind: String,
    pub state: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Example {
    pub id: String,
    pub text: String,
    pub context_meaning: String,
    pub source_id: Option<String>,
    pub source_title: Option<String>,
    pub start_ms: Option<i64>,
    pub end_ms: Option<i64>,
    pub audio: Vec<AudioAsset>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Entry {
    pub id: String,
    pub kind: String,
    pub text: String,
    pub match_key: String,
    pub revision: i64,
    pub created_at: i64,
    pub updated_at: i64,
    pub collection_count: i64,
    pub occurrence_count: i64,
    pub meanings: Vec<Meaning>,
    pub examples: Vec<Example>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MeaningUpdate {
    pub id: Option<String>,
    pub text: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EntryUpdate {
    pub id: String,
    pub expected_revision: i64,
    pub text: String,
    pub meanings: Vec<MeaningUpdate>,
}

pub(crate) fn validate(input: &CollectionInput) -> Result<(), AppError> {
    if Uuid::parse_str(&input.operation_id).is_err() {
        return Err(AppError::new("invalid_input", "收录操作标识无效。"));
    }
    if !matches!(input.kind.as_str(), "word" | "phrase" | "sentence")
        || input.text.trim().is_empty()
        || input.text.chars().count() > 4000
    {
        return Err(AppError::new("invalid_input", "请确认词条类型与非空原文。"));
    }
    if input.meaning.chars().count() > 10000 || input.examples.len() > 100 {
        return Err(AppError::new(
            "invalid_input",
            "本次收录内容过长，请分次处理。",
        ));
    }
    for example in &input.examples {
        if example.text.trim().is_empty() || example.text.chars().count() > 20000 {
            return Err(AppError::new("invalid_input", "例句不能为空或过长。"));
        }
        match (example.start_ms, example.end_ms) {
            (None, None) => (),
            (Some(start), Some(end)) if start >= 0 && end > start => (),
            _ => return Err(AppError::new("invalid_input", "对白时间范围无效。")),
        }
    }
    Ok(())
}

pub(crate) fn insert_example(
    connection: &Connection,
    example: &ExampleInput,
    now: i64,
) -> Result<String, AppError> {
    let location = if example.location_key.is_empty() {
        format!(
            "{}:{}:{}",
            example.start_ms.unwrap_or(-1),
            example.end_ms.unwrap_or(-1),
            normalize(&example.text)
        )
    } else {
        example.location_key.clone()
    };
    let key = digest(
        serde_json::to_string(&(example.source_id.as_deref().unwrap_or("manual"), &location))?
            .as_bytes(),
    );
    let existing: Option<String> = connection
        .query_row(
            "SELECT id FROM examples WHERE identity_key = ?",
            [&key],
            |row| row.get(0),
        )
        .optional()?;
    if let Some(id) = existing {
        return Ok(id);
    }
    let id = identity();
    connection.execute(
        "INSERT INTO examples(id, source_id, location_key, identity_key, text, start_ms, end_ms, created_at) VALUES (?,?,?,?,?,?,?,?)",
        params![id, example.source_id, location, key, example.text.trim(), example.start_ms, example.end_ms, now],
    )?;
    Ok(id)
}

impl Store {
    pub fn update_entry(&self, input: &EntryUpdate) -> Result<Entry, AppError> {
        if input.text.trim().is_empty()
            || input.text.chars().count() > 4000
            || input.meanings.iter().any(|meaning| {
                meaning.text.trim().is_empty() || meaning.text.chars().count() > 10000
            })
        {
            return Err(AppError::new("invalid_input", "请确认非空原文与释义。"));
        }
        let mut connection = self.connection()?;
        let transaction = connection.transaction()?;
        let version: Option<i64> = transaction
            .query_row(
                "SELECT revision FROM entries WHERE id=?",
                [&input.id],
                |row| row.get(0),
            )
            .optional()?;
        if version != Some(input.expected_revision) {
            return Err(AppError::new("conflict", "词条已变化，请刷新后再修改。"));
        }
        for meaning in &input.meanings {
            if let Some(id) = &meaning.id {
                let count=transaction.execute("UPDATE meanings SET text=?,origin='user',revision=revision+1 WHERE id=? AND entry_id=?",params![meaning.text.trim(),id,input.id])?;
                if count == 0 {
                    return Err(AppError::new("not_found", "待修改释义不存在。"));
                }
            } else {
                transaction.execute("INSERT OR IGNORE INTO meanings(id,entry_id,text,origin,created_at) VALUES (?,?,?,'user',?)",params![identity(),input.id,meaning.text.trim(),Utc::now().timestamp_millis()])?;
            }
        }
        transaction.execute(
            "UPDATE entries SET text=?,match_key=?,revision=revision+1,updated_at=? WHERE id=?",
            params![
                input.text.trim(),
                normalize(&input.text),
                Utc::now().timestamp_millis(),
                input.id
            ],
        )?;
        transaction.commit()?;
        get_entry_on(&connection, &input.id)
    }
    pub fn add_source(&self, source: &SourceInput) -> Result<String, AppError> {
        if source.kind.trim().is_empty() || source.fingerprint.trim().is_empty() {
            return Err(AppError::new("invalid_input", "来源类型与标识不能为空。"));
        }
        let connection = self.connection()?;
        let existing = connection
            .query_row(
                "SELECT id FROM sources WHERE kind=? AND fingerprint=?",
                params![source.kind, source.fingerprint],
                |row| row.get::<_, String>(0),
            )
            .optional()?;
        if let Some(id) = existing {
            if source.path_hint.is_some() {
                connection.execute(
                    "UPDATE sources SET path_hint=? WHERE id=?",
                    params![source.path_hint, id],
                )?;
            }
            return Ok(id);
        }
        let id = identity();
        connection.execute("INSERT INTO sources(id,kind,title,fingerprint,path_hint,duration_ms,created_at) VALUES (?,?,?,?,?,?,?)", params![id, source.kind, source.title, source.fingerprint, source.path_hint, source.duration_ms, Utc::now().timestamp_millis()])?;
        Ok(id)
    }

    pub fn collect(&self, input: &CollectionInput) -> Result<CollectionResult, AppError> {
        validate(input)?;
        let request_hash = digest(&serde_json::to_vec(input)?);
        let now = Utc::now().timestamp_millis();
        let mut connection = self.connection()?;
        let transaction = connection.transaction()?;
        let receipt: Option<(String, String)> = transaction
            .query_row(
                "SELECT request_hash,result_json FROM collection_actions WHERE operation_id=?",
                [&input.operation_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?;
        if let Some((saved_hash, json)) = receipt {
            if saved_hash != request_hash {
                return Err(AppError::new(
                    "conflict",
                    "同一操作标识对应了不同的收录内容，请重新确认。",
                ));
            }
            return Ok(serde_json::from_str(&json)?);
        }
        for example in &input.examples {
            for asset_id in &example.media_asset_ids {
                let record: Option<(String, String, String)> = transaction
                    .query_row(
                        "SELECT relative_path,digest,state FROM media_assets WHERE id=?",
                        [asset_id],
                        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
                    )
                    .optional()?;
                let (path, digest, state) = record.ok_or_else(|| {
                    AppError::new(
                        "resource_missing",
                        "原声音频尚未保存成功，草稿已保留，请重试或明确选择先保存文字。",
                    )
                })?;
                crate::corpus::validate_media_file(self.root(), &path, &digest, &state)?;
            }
        }
        let created = input.target_entry_id.is_none();
        let entry_id = if let Some(entry_id) = &input.target_entry_id {
            let current: Option<(i64, String)> = transaction
                .query_row(
                    "SELECT revision,kind FROM entries WHERE id=?",
                    [entry_id],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
                .optional()?;
            let (revision, kind) =
                current.ok_or_else(|| AppError::new("not_found", "待合并词条已不存在。"))?;
            if input.expected_revision != Some(revision) || kind != input.kind {
                return Err(AppError::new(
                    "conflict",
                    "词条版本或类型已经变化，请核对后重新确认。",
                ));
            }
            transaction.execute(
                "UPDATE entries SET revision=revision+1,updated_at=? WHERE id=?",
                params![now, entry_id],
            )?;
            entry_id.clone()
        } else {
            let id = identity();
            transaction.execute("INSERT INTO entries(id,kind,text,match_key,created_at,updated_at) VALUES (?,?,?,?,?,?)", params![id, input.kind, input.text.trim(), normalize(&input.text), now, now])?;
            id
        };
        let meaning_id = if input.meaning.trim().is_empty() {
            None
        } else {
            let id = identity();
            transaction.execute("INSERT OR IGNORE INTO meanings(id,entry_id,text,origin,created_at) VALUES (?,?,?,'user',?)", params![id, entry_id, input.meaning.trim(), now])?;
            Some(transaction.query_row(
                "SELECT id FROM meanings WHERE entry_id=? AND text=?",
                params![entry_id, input.meaning.trim()],
                |row| row.get::<_, String>(0),
            )?)
        };
        let mut example_ids = Vec::new();
        for example in &input.examples {
            let id = insert_example(&transaction, example, now)?;
            transaction.execute("INSERT OR IGNORE INTO entry_examples(entry_id,example_id,meaning_id,context_meaning) VALUES (?,?,?,?)", params![entry_id, id, meaning_id, example.context_meaning.trim()])?;
            for asset in &example.media_asset_ids {
                transaction.execute(
                    "INSERT OR IGNORE INTO example_media(example_id,asset_id) VALUES (?,?)",
                    params![id, asset],
                )?;
            }
            if let Some(source_id) = &example.source_id {
                transaction.execute("UPDATE occurrences SET entry_id=? WHERE source_id=? AND example_id=? AND candidate_key=?", params![entry_id, source_id, id, normalize(&input.text)])?;
                transaction.execute("INSERT INTO candidate_decisions(source_id,candidate_key,scope_key,decision,updated_at) VALUES (?,?,?,'collected',?) ON CONFLICT(source_id,candidate_key,scope_key) DO UPDATE SET decision='collected',updated_at=excluded.updated_at", params![source_id, normalize(&input.text), id, now])?;
            }
            if !example_ids.contains(&id) {
                example_ids.push(id);
            }
        }
        for dimension in ["meaning", "listening"] {
            let unit_id = identity();
            let scope_key = meaning_id.as_deref().unwrap_or("entry");
            transaction.execute("INSERT OR IGNORE INTO learning_units(id,entry_id,scope_key,dimension) VALUES (?,?,?,?)", params![unit_id, entry_id, scope_key, dimension])?;
            let unit_id: String = transaction.query_row(
                "SELECT id FROM learning_units WHERE entry_id=? AND scope_key=? AND dimension=?",
                params![entry_id, scope_key, dimension],
                |row| row.get(0),
            )?;
            transaction.execute("INSERT INTO review_states(unit_id,state_json,due_at,relearn_at) VALUES (?,'{}',?,?) ON CONFLICT(unit_id) DO UPDATE SET due_at=MIN(review_states.due_at,excluded.due_at),relearn_at=excluded.relearn_at,revision=review_states.revision+1", params![unit_id, now, now])?;
        }
        let revision = transaction.query_row(
            "SELECT revision FROM entries WHERE id=?",
            [&entry_id],
            |row| row.get(0),
        )?;
        let result = CollectionResult {
            entry_id: entry_id.clone(),
            operation_id: input.operation_id.clone(),
            revision,
            created,
            example_ids,
        };
        transaction.execute("INSERT INTO collection_actions(operation_id,request_hash,entry_id,source_id,result_json,created_at) VALUES (?,?,?,?,?,?)", params![input.operation_id, request_hash, entry_id, input.examples.first().and_then(|e| e.source_id.as_ref()), serde_json::to_string(&result)?, now])?;
        transaction.commit()?;
        Ok(result)
    }

    pub fn get_entry(&self, id: &str) -> Result<Entry, AppError> {
        let connection = self.connection()?;
        get_entry_on(&connection, id)
    }

    pub fn find_entries(&self, kind: &str, text: &str) -> Result<Vec<Entry>, AppError> {
        let connection = self.connection()?;
        let mut query = connection.prepare(
            "SELECT id FROM entries WHERE kind=? AND match_key=? ORDER BY created_at,id",
        )?;
        let ids = query
            .query_map(params![kind, normalize(text)], |row| {
                row.get::<_, String>(0)
            })?
            .collect::<Result<Vec<_>, _>>()?;
        ids.iter().map(|id| get_entry_on(&connection, id)).collect()
    }

    pub fn list_entries(
        &self,
        search: &str,
        offset: u32,
        limit: u32,
    ) -> Result<Vec<Entry>, AppError> {
        let connection = self.connection()?;
        let pattern = format!(
            "%{}%",
            normalize(search)
                .replace('!', "!!")
                .replace('%', "!%")
                .replace('_', "!_")
        );
        let mut query = connection.prepare("SELECT id FROM entries WHERE match_key LIKE ? ESCAPE '!' ORDER BY updated_at DESC,id LIMIT ? OFFSET ?")?;
        let ids = query
            .query_map(params![pattern, limit.clamp(1, 100), offset], |row| {
                row.get::<_, String>(0)
            })?
            .collect::<Result<Vec<_>, _>>()?;
        ids.iter().map(|id| get_entry_on(&connection, id)).collect()
    }
}

fn get_entry_on(connection: &Connection, id: &str) -> Result<Entry, AppError> {
    let mut entry = connection.query_row(
        "SELECT id,kind,text,match_key,revision,created_at,updated_at,(SELECT COUNT(*) FROM collection_actions WHERE entry_id=entries.id),CASE WHEN kind='sentence' THEN (SELECT COUNT(DISTINCT e.source_id||':'||e.location_key) FROM entry_examples ee JOIN examples e ON e.id=ee.example_id WHERE ee.entry_id=entries.id AND e.source_id IS NOT NULL) ELSE (SELECT COUNT(*) FROM occurrences o WHERE o.entry_id=entries.id OR (o.candidate_key=entries.match_key AND o.source_id IN (SELECT e.source_id FROM entry_examples ee JOIN examples e ON e.id=ee.example_id WHERE ee.entry_id=entries.id))) END FROM entries WHERE id=?",
        [id], |row| Ok(Entry { id: row.get(0)?, kind: row.get(1)?, text: row.get(2)?, match_key: row.get(3)?, revision: row.get(4)?, created_at: row.get(5)?, updated_at: row.get(6)?, collection_count: row.get(7)?, occurrence_count: row.get(8)?, meanings: Vec::new(), examples: Vec::new() }),
    ).optional()?.ok_or_else(|| AppError::new("not_found", "词条不存在。"))?;
    entry.meanings = connection
        .prepare("SELECT id,text,origin FROM meanings WHERE entry_id=? ORDER BY created_at,id")?
        .query_map([id], |row| {
            Ok(Meaning {
                id: row.get(0)?,
                text: row.get(1)?,
                origin: row.get(2)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    entry.examples = connection.prepare("SELECT e.id,e.text,ee.context_meaning,e.source_id,s.title,e.start_ms,e.end_ms FROM entry_examples ee JOIN examples e ON e.id=ee.example_id LEFT JOIN sources s ON s.id=e.source_id WHERE ee.entry_id=? ORDER BY e.created_at,e.id")?.query_map([id], |row| Ok(Example { id: row.get(0)?, text: row.get(1)?, context_meaning: row.get(2)?, source_id: row.get(3)?, source_title: row.get(4)?, start_ms: row.get(5)?, end_ms: row.get(6)?, audio: Vec::new() }))?.collect::<Result<Vec<_>, _>>()?;
    for example in &mut entry.examples {
        example.audio = connection.prepare("SELECT a.id,a.relative_path,a.duration_ms,a.kind,a.state FROM example_media em JOIN media_assets a ON a.id=em.asset_id WHERE em.example_id=? ORDER BY a.created_at,a.id")?.query_map([&example.id], |row| Ok(AudioAsset { id: row.get(0)?, relative_path: row.get(1)?, duration_ms: row.get(2)?, kind: row.get(3)?, state: row.get(4)? }))?.collect::<Result<Vec<_>, _>>()?;
    }
    Ok(entry)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(word: &str) -> CollectionInput {
        CollectionInput {
            operation_id: identity(),
            kind: "word".into(),
            text: word.into(),
            meaning: "不太愿意".into(),
            examples: vec![ExampleInput {
                text: "I was reluctant to ask for help.".into(),
                context_meaning: "不愿开口求助".into(),
                ..Default::default()
            }],
            target_entry_id: None,
            expected_revision: None,
        }
    }

    #[test]
    fn retries_do_not_create_another_word_action_or_example() {
        let directory = tempfile::tempdir().unwrap();
        let store = Store::open(directory.path()).unwrap();
        let request = request("reluctant");
        let first = store.collect(&request).unwrap();
        let retry = store.collect(&request).unwrap();
        assert_eq!(first.entry_id, retry.entry_id);
        let entry = store.get_entry(&first.entry_id).unwrap();
        assert_eq!(entry.collection_count, 1);
        assert_eq!(entry.examples.len(), 1);
        assert_eq!(store.list_entries("", 0, 50).unwrap().len(), 1);
    }

    #[test]
    fn new_user_action_preserves_examples_but_increases_collection_count() {
        let directory = tempfile::tempdir().unwrap();
        let store = Store::open(directory.path()).unwrap();
        let mut input = request("reluctant");
        let first = store.collect(&input).unwrap();
        input.operation_id = identity();
        input.target_entry_id = Some(first.entry_id.clone());
        input.expected_revision = Some(first.revision);
        store.collect(&input).unwrap();
        let entry = store.get_entry(&first.entry_id).unwrap();
        assert_eq!(entry.collection_count, 2);
        assert_eq!(entry.examples.len(), 1);
        assert_eq!(entry.revision, 2);
    }

    #[test]
    fn new_context_adds_meaning_without_overwriting_old_context() {
        let directory = tempfile::tempdir().unwrap();
        let store = Store::open(directory.path()).unwrap();
        let first_input = request("reluctant");
        let first = store.collect(&first_input).unwrap();
        let mut next = request("reluctant");
        next.target_entry_id = Some(first.entry_id.clone());
        next.expected_revision = Some(first.revision);
        next.meaning = "勉强的".into();
        next.examples[0].text = "He gave a reluctant smile.".into();
        next.examples[0].context_meaning = "勉强地笑".into();
        store.collect(&next).unwrap();
        let entry = store.get_entry(&first.entry_id).unwrap();
        assert_eq!(entry.meanings.len(), 2);
        assert_eq!(entry.examples.len(), 2);
        assert!(
            entry
                .examples
                .iter()
                .any(|e| e.context_meaning == "不愿开口求助")
        );
    }

    #[test]
    fn changed_request_under_same_operation_is_a_conflict() {
        let directory = tempfile::tempdir().unwrap();
        let store = Store::open(directory.path()).unwrap();
        let mut input = request("reluctant");
        store.collect(&input).unwrap();
        input.text = "eager".into();
        assert_eq!(store.collect(&input).unwrap_err().code, "conflict");
        assert_eq!(store.list_entries("", 0, 50).unwrap().len(), 1);
    }

    #[test]
    fn stale_merge_does_not_overwrite_or_count_a_failed_action() {
        let directory = tempfile::tempdir().unwrap();
        let store = Store::open(directory.path()).unwrap();
        let mut input = request("reluctant");
        let first = store.collect(&input).unwrap();
        input.operation_id = identity();
        input.target_entry_id = Some(first.entry_id.clone());
        input.expected_revision = Some(0);
        assert_eq!(store.collect(&input).unwrap_err().code, "conflict");
        assert_eq!(
            store.get_entry(&first.entry_id).unwrap().collection_count,
            1
        );
    }

    #[test]
    fn corrupt_audio_rejects_collection_and_valid_audio_survives_entry_edit() {
        let directory = tempfile::tempdir().unwrap();
        let store = Store::open(directory.path()).unwrap();
        let path = directory.path().join("media/test.m4a");
        std::fs::write(&path, b"synthetic audio").unwrap();
        let digest = digest(b"synthetic audio");
        store.connection().unwrap().execute("INSERT INTO media_assets(id,recipe_key,digest,relative_path,kind,format,duration_ms,state,created_at) VALUES ('audio','test',?,'media/test.m4a','original','m4a',1000,'ready',0)", [&digest]).unwrap();
        let mut input = request("reluctant");
        input.examples[0].media_asset_ids.push("audio".into());
        std::fs::write(&path, b"damaged").unwrap();
        assert_eq!(store.collect(&input).unwrap_err().code, "resource_missing");
        assert!(store.list_entries("", 0, 50).unwrap().is_empty());
        std::fs::write(&path, b"synthetic audio").unwrap();
        let result = store.collect(&input).unwrap();
        let entry = store.get_entry(&result.entry_id).unwrap();
        let update = EntryUpdate {
            id: entry.id.clone(),
            expected_revision: entry.revision,
            text: "Reluctant".into(),
            meanings: vec![MeaningUpdate {
                id: Some(entry.meanings[0].id.clone()),
                text: "不情愿的".into(),
            }],
        };
        let updated = store.update_entry(&update).unwrap();
        assert_eq!(updated.collection_count, 1);
        assert_eq!(updated.examples[0].id, entry.examples[0].id);
        assert_eq!(updated.examples[0].audio[0].id, "audio");
        assert_eq!(updated.meanings[0].text, "不情愿的");
        assert_eq!(store.update_entry(&update).unwrap_err().code, "conflict");
        assert_eq!(
            store.media_file("audio").unwrap(),
            path.canonicalize().unwrap()
        );
    }

    #[test]
    fn missing_audio_rolls_back_the_whole_collection() {
        let directory = tempfile::tempdir().unwrap();
        let store = Store::open(directory.path()).unwrap();
        let mut input = request("reluctant");
        input.examples[0].media_asset_ids.push("missing".into());
        assert_eq!(store.collect(&input).unwrap_err().code, "resource_missing");
        assert!(store.list_entries("", 0, 50).unwrap().is_empty());
    }

    #[test]
    fn explicit_new_entries_can_keep_distinct_casing_and_types() {
        let directory = tempfile::tempdir().unwrap();
        let store = Store::open(directory.path()).unwrap();
        for (text, kind) in [
            ("US", "word"),
            ("us", "word"),
            ("look up", "phrase"),
            ("I was reluctant.", "sentence"),
        ] {
            let mut input = request(text);
            input.kind = kind.into();
            store.collect(&input).unwrap();
        }
        assert_eq!(store.find_entries("word", "US").unwrap().len(), 2);
        assert_eq!(store.list_entries("", 0, 50).unwrap().len(), 4);
    }

    #[test]
    fn saved_material_survives_reopening() {
        let directory = tempfile::tempdir().unwrap();
        let store = Store::open(directory.path()).unwrap();
        let result = store.collect(&request("reluctant")).unwrap();
        drop(store);
        let entry = Store::open(directory.path())
            .unwrap()
            .get_entry(&result.entry_id)
            .unwrap();
        assert_eq!(entry.collection_count, 1);
        assert_eq!(entry.examples.len(), 1);
    }

    #[test]
    fn search_treats_percent_and_underscore_as_literal_text() {
        let directory = tempfile::tempdir().unwrap();
        let store = Store::open(directory.path()).unwrap();
        store.collect(&request("100%_sure")).unwrap();
        store.collect(&request("reluctant")).unwrap();
        assert_eq!(store.list_entries("%_", 0, 50).unwrap().len(), 1);
        assert_eq!(store.list_entries("sure", 0, 50).unwrap().len(), 1);
    }
}
