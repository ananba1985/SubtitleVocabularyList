//! Select missing reusable work and group it into provider-sized batches.
use super::*;
const ENABLED_KEY: &str = "explanation_preparation_enabled";

#[derive(Clone)]
pub(super) struct PreparationBatch {
    pub(super) inputs: Vec<PreparationInput>,
}

pub(super) fn group_batches(
    inputs: Vec<PreparationInput>,
    batch_size: usize,
) -> VecDeque<PreparationBatch> {
    let mut batches = Vec::<PreparationBatch>::new();
    let mut indexes = HashMap::<(bool, String, String), usize>::new();
    for input in inputs {
        let key = (
            input.kind == "sentence",
            input.source_id.clone(),
            input.context.clone(),
        );
        let index = match indexes.get(&key).copied() {
            Some(index) if batches[index].inputs.len() < batch_size => index,
            _ => {
                let index = batches.len();
                batches.push(PreparationBatch { inputs: Vec::new() });
                indexes.insert(key, index);
                index
            }
        };
        batches[index].inputs.push(input);
    }
    batches.into()
}

impl Store {
    pub fn preparation_enabled(&self) -> Result<bool, AppError> {
        Ok(self
            .connection()?
            .query_row(
                "SELECT value_json FROM settings WHERE key=?",
                [ENABLED_KEY],
                |row| row.get::<_, String>(0),
            )
            .optional()?
            .as_deref()
            != Some("false"))
    }

    pub fn set_preparation_enabled(&self, enabled: bool) -> Result<(), AppError> {
        self.connection()?.execute("INSERT INTO settings(key,value_json) VALUES (?,?) ON CONFLICT(key) DO UPDATE SET value_json=excluded.value_json", params![ENABLED_KEY, enabled.to_string()])?;
        Ok(())
    }

    pub fn explanation_work(
        &self,
        source_id: Option<&str>,
    ) -> Result<Vec<PreparationInput>, AppError> {
        if let Some(id) = source_id {
            self.source(id)?;
        }
        let connection = self.connection()?;
        let mut query = connection.prepare(
            "WITH material AS (
                SELECT DISTINCT o.source_id,s.title,s.imported_at,o.kind,o.candidate_key,trim(e.text) sentence
                FROM occurrences o JOIN sources s ON s.id=o.source_id JOIN examples e ON e.id=o.example_id
                LEFT JOIN settings k ON k.key=?3 || o.kind || ':' || o.candidate_key
                WHERE (?1 IS NULL OR o.source_id=?2) AND s.corpus_path IS NOT NULL AND e.archived=0 AND k.key IS NULL
             ), words AS (
                SELECT *,ROW_NUMBER() OVER(PARTITION BY kind,candidate_key ORDER BY imported_at DESC,source_id) rank FROM material
             ), sentences AS (
                SELECT *,ROW_NUMBER() OVER(PARTITION BY sentence ORDER BY imported_at DESC,source_id) rank FROM material
             ), work AS (
                SELECT w.source_id,w.title,w.kind,w.candidate_key target,'' context,w.imported_at,0 phase,x.meaning cached_meaning FROM words w
                LEFT JOIN explanations x ON x.target=w.candidate_key AND x.context=''
                WHERE w.rank=1
                UNION ALL
                SELECT source_id,title,'sentence',sentence,'',imported_at,1,NULL FROM sentences
                WHERE rank=1 AND sentence<>'' AND NOT EXISTS(SELECT 1 FROM sentence_translations x WHERE x.sentence=sentences.sentence)
             ) SELECT source_id,title,kind,target,context,cached_meaning FROM work
             ORDER BY imported_at DESC,source_id,phase,length(target) DESC,target")?;
        let inputs = query
            .query_map(
                params![source_id, source_id, crate::known_targets::PREFIX],
                |row| {
                    Ok((
                        PreparationInput {
                            source_id: row.get(0)?,
                            title: row.get(1)?,
                            kind: row.get(2)?,
                            text: row.get(3)?,
                            context: row.get(4)?,
                        },
                        row.get::<_, Option<String>>(5)?,
                    ))
                },
            )?
            .collect::<Result<Vec<_>, _>>()?;
        let mut pending = Vec::new();
        for (input, cached_meaning) in inputs {
            if cached_meaning
                .as_deref()
                .is_some_and(crate::explanations::Explanation::valid_meaning)
            {
                continue;
            }
            if input.kind == "sentence" || self.dictionary.explanation(&input.text)?.is_none() {
                pending.push(input);
            }
        }
        Ok(pending)
    }
}
