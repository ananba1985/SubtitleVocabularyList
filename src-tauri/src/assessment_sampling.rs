use crate::{
    assessment::{FrozenItem, SamplingSnapshot},
    assessment_bank::{
        AssessmentConfig, BankItem, FrequencyCatalog, POLICY, QuestionDraft, TYPES, safe_audio_path,
    },
    error::AppError,
    store::Store,
    vocabulary::digest,
};
use chrono::Utc;
use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeSet, HashMap, HashSet};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "ipc-types", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct AssessmentQuota {
    pub catalog_id: String,
    pub content: String,
    pub from_rank: usize,
    pub to_rank: usize,
    pub question_type: String,
    pub planned: usize,
    pub available: usize,
    pub unfilled: usize,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "ipc-types", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct AssessmentCoverage {
    pub catalog_id: String,
    pub top_n: usize,
    pub raw_targets: usize,
    pub lexeme_targets: usize,
    pub prepared_targets: usize,
    pub missing_targets: usize,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "ipc-types", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct AssessmentPreview {
    pub inventory_revision: String,
    pub can_start: bool,
    pub policy_version: String,
    pub catalogs: Vec<FrequencyCatalog>,
    pub quotas: Vec<AssessmentQuota>,
    pub coverage: Vec<AssessmentCoverage>,
    pub warnings: Vec<String>,
    pub repeat_count: usize,
    pub missing_slots: usize,
}
pub(crate) struct Inventory {
    pub preview: AssessmentPreview,
    items: Vec<BankItem>,
    recent: HashSet<String>,
}
fn identity(item: &BankItem) -> String {
    format!("{}:{}", item.kind, item.lexeme_key)
}

pub(crate) fn load_inventory(
    store: &Store,
    c: &Connection,
    config: &AssessmentConfig,
) -> Result<Inventory, AppError> {
    config.validate()?;
    if config.question_count < config.question_types.len() {
        return Err(AppError::new(
            "invalid_input",
            "题量至少应覆盖每种已选题型一次。",
        ));
    }
    let catalogs = store
        .assessment_catalogs()?
        .into_iter()
        .filter(|cat| config.content == "mixed" || config.content == cat.kind)
        .collect::<Vec<_>>();
    let requested = if config.content == "mixed" { 2 } else { 1 };
    if catalogs.len() != requested {
        return Err(AppError::new(
            "assessment_not_ready",
            "所选内容没有唯一可用的词频目录，请先准备来源。",
        ));
    }
    let types = TYPES
        .into_iter()
        .filter(|t| config.question_types.iter().any(|v| v == t))
        .collect::<Vec<_>>();
    let mut quotas = vec![];
    let mut coverage = vec![];
    let mut items = vec![];
    let mut warnings = vec![];
    let mut remainder_rotation = 0;
    for (cat_index, cat) in catalogs.iter().enumerate() {
        let n = if cat.kind == "word" {
            config.word_top_n
        } else {
            config.phrase_top_n
        };
        if n > cat.max_rank {
            return Err(AppError::new(
                "invalid_input",
                format!("{} 的最大原排名为 {}。", cat.title, cat.max_rank),
            ));
        }
        let targets = store.assessment_targets(&cat.id, n)?;
        let identities = targets
            .iter()
            .map(|t| (t.lexeme_key.clone(), t.rank))
            .collect::<HashMap<_, _>>();
        let mut q=c.prepare("SELECT i.id,i.bank_id,i.catalog_id,i.target_rank,i.target_key,i.lexeme_key,i.kind,i.sense_key,i.question_type,i.revision,i.content_json,b.sources_json FROM assessment_items i JOIN assessment_banks b ON b.id=i.bank_id WHERE b.state='published' AND i.state='approved' AND i.catalog_id=? AND i.target_rank<=? ORDER BY b.published_at DESC,b.id DESC,i.id")?;
        let rows = q
            .query_map(params![cat.id, n], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, usize>(3)?,
                    r.get::<_, String>(4)?,
                    r.get::<_, String>(5)?,
                    r.get::<_, String>(6)?,
                    r.get::<_, String>(7)?,
                    r.get::<_, String>(8)?,
                    r.get::<_, i64>(9)?,
                    r.get::<_, String>(10)?,
                    r.get::<_, String>(11)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        let mut seen = HashSet::new();
        let start = items.len();
        for row in rows {
            let sources: Vec<FrequencyCatalog> = serde_json::from_str(&row.11)?;
            if !types.contains(&row.8.as_str())
                || identities.get(&row.5) != Some(&row.3)
                || !sources.iter().any(|s| {
                    s.id == cat.id
                        && s.source_sha256 == cat.source_sha256
                        && s.rank_unit == cat.rank_unit
                })
            {
                continue;
            }
            let content: QuestionDraft = serde_json::from_str(&row.10)?;
            if content.validate(&row.8, false, store.root()).is_err() {
                continue;
            }
            if row.8 == "listening"
                && !content.audio.as_ref().is_some_and(|a| {
                    safe_audio_path(store.root(), a).is_ok_and(|p| {
                        std::fs::metadata(p).is_ok_and(|m| m.is_file() && m.len() > 44)
                    })
                })
            {
                continue;
            }
            if !seen.insert((row.5.clone(), row.8.clone())) {
                continue;
            }
            items.push(BankItem {
                id: row.0,
                bank_id: row.1,
                catalog_id: row.2,
                target_rank: row.3,
                target_key: row.4,
                lexeme_key: row.5,
                kind: row.6,
                sense_key: row.7,
                question_type: row.8,
                state: "approved".into(),
                revision: row.9,
                content,
                review_note: String::new(),
                reviewed_at: None,
            });
        }
        let ready = items[start..]
            .iter()
            .map(|i| &i.lexeme_key)
            .collect::<HashSet<_>>()
            .len();
        let raw_targets = {
            let path = store.root().join("dictionaries/base.sqlite3");
            let reference =
                Connection::open_with_flags(path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)?;
            reference.query_row("SELECT COUNT(*) FROM frequency_targets WHERE catalog_id=? AND rank<=? AND eligible=1",params![cat.id,n],|r|r.get::<_,usize>(0))?
        };
        coverage.push(AssessmentCoverage {
            catalog_id: cat.id.clone(),
            top_n: n,
            raw_targets,
            lexeme_targets: targets.len(),
            prepared_targets: ready,
            missing_targets: targets.len().saturating_sub(ready),
        });
        if ready < targets.len() {
            warnings.push(format!(
                "{}：在所选 Top {} 范围内已准备题目抽样；可用目标 {}/{}，覆盖未齐。",
                cat.title,
                n,
                ready,
                targets.len()
            ));
        }
        if cat.kind == "phrase" {
            warnings.push(
                "组合范围采用原双词频次排名及自然短语筛选；未筛选的相邻词组不代表可测试短语。"
                    .into(),
            );
        }
        let count = config.question_count / catalogs.len()
            + usize::from(cat_index < config.question_count % catalogs.len());
        let band_count = n.div_ceil(config.band_size);
        for band in 0..band_count {
            let band_questions = count / band_count + usize::from(band < count % band_count);
            let from_rank = band * config.band_size + 1;
            let to_rank = ((band + 1) * config.band_size).min(n);
            for (type_index, question_type) in types.iter().enumerate() {
                let extra = (type_index + types.len() - remainder_rotation) % types.len()
                    < band_questions % types.len();
                let planned = band_questions / types.len() + usize::from(extra);
                let available = items[start..]
                    .iter()
                    .filter(|item| {
                        item.question_type == *question_type
                            && item.target_rank >= from_rank
                            && item.target_rank <= to_rank
                    })
                    .count();
                quotas.push(AssessmentQuota {
                    catalog_id: cat.id.clone(),
                    content: cat.kind.clone(),
                    from_rank,
                    to_rank,
                    question_type: (*question_type).into(),
                    planned,
                    available,
                    unfilled: 0,
                });
            }
            remainder_rotation = (remainder_rotation + band_questions % types.len()) % types.len();
        }
    }
    let recent = if config.avoid_recent_days == 0 {
        HashSet::new()
    } else {
        let cutoff = Utc::now().timestamp_millis() - (config.avoid_recent_days as i64) * 86_400_000;
        let mut q=c.prepare("SELECT DISTINCT json_extract(i.snapshot_json,'$.kind')||':'||json_extract(i.snapshot_json,'$.lexemeKey') FROM assessment_session_items i JOIN assessment_sessions s ON s.id=i.session_id WHERE s.created_at>=?")?;
        q.query_map([cutoff], |r| r.get::<_, String>(0))?
            .collect::<Result<HashSet<_>, _>>()?
    };
    let recent_sorted = recent.iter().collect::<BTreeSet<_>>();
    let revision = digest(&serde_json::to_vec(&(
        POLICY,
        config,
        &catalogs,
        items
            .iter()
            .map(|i| (&i.id, i.revision))
            .collect::<Vec<_>>(),
        recent_sorted,
    ))?);
    let mut inventory = Inventory {
        preview: AssessmentPreview {
            inventory_revision: revision,
            can_start: false,
            policy_version: POLICY.into(),
            catalogs,
            quotas,
            coverage,
            warnings,
            repeat_count: 0,
            missing_slots: 0,
        },
        items,
        recent,
    };
    let (selection, missing) = inventory.select("preview", true);
    inventory.preview.missing_slots = missing;
    for quota in &mut inventory.preview.quotas {
        let filled = selection
            .iter()
            .filter(|index| {
                let i = &inventory.items[**index];
                i.catalog_id == quota.catalog_id
                    && i.target_rank >= quota.from_rank
                    && i.target_rank <= quota.to_rank
                    && i.question_type == quota.question_type
            })
            .count();
        quota.unfilled = quota.planned.saturating_sub(filled);
    }
    inventory.preview.can_start = missing == 0;
    inventory.preview.repeat_count = selection
        .iter()
        .filter(|i| inventory.recent.contains(&identity(&inventory.items[**i])))
        .count();
    if missing > 0 {
        inventory.preview.warnings.push(format!(
            "当前缺少 {} 个不重复的配额位置，请调整范围／题量／题型，或补充并发布题库。",
            missing
        ));
    }
    if inventory.preview.repeat_count > 0 {
        inventory.preview.warnings.push(format!(
            "题库有限，预计复用近期目标 {} 个；仍在原频段内补足。",
            inventory.preview.repeat_count
        ));
    }
    Ok(inventory)
}

impl Inventory {
    fn select(&self, seed: &str, with_recent: bool) -> (Vec<usize>, usize) {
        let mut slots = vec![];
        for quota in &self.preview.quotas {
            for _ in 0..quota.planned {
                let mut candidates = self
                    .items
                    .iter()
                    .enumerate()
                    .filter(|(_, i)| {
                        i.catalog_id == quota.catalog_id
                            && i.question_type == quota.question_type
                            && i.target_rank >= quota.from_rank
                            && i.target_rank <= quota.to_rank
                    })
                    .map(|(index, _)| index)
                    .collect::<Vec<_>>();
                candidates.sort_by_cached_key(|index| {
                    let i = &self.items[*index];
                    (
                        self.recent.contains(&identity(i)),
                        digest(format!("{seed}:{}:{}", slots.len(), i.id).as_bytes()),
                    )
                });
                slots.push(candidates);
            }
        }
        let mut chosen = vec![None; slots.len()];
        let mut owner = HashMap::new();
        let mut order = (0..slots.len()).collect::<Vec<_>>();
        order.sort_by_key(|i| slots[*i].len());
        for allow_recent in [false, true] {
            if allow_recent && !with_recent {
                break;
            }
            for slot in &order {
                if chosen[*slot].is_none() {
                    self.assign(
                        *slot,
                        &slots,
                        &mut owner,
                        &mut chosen,
                        &mut HashSet::new(),
                        allow_recent,
                    );
                }
            }
        }
        let missing = chosen.iter().filter(|v| v.is_none()).count();
        (chosen.into_iter().flatten().collect(), missing)
    }
    fn assign(
        &self,
        slot: usize,
        slots: &[Vec<usize>],
        owners: &mut HashMap<String, usize>,
        chosen: &mut [Option<usize>],
        seen: &mut HashSet<String>,
        allow_recent: bool,
    ) -> bool {
        for index in &slots[slot] {
            let key = identity(&self.items[*index]);
            if (!allow_recent && self.recent.contains(&key)) || !seen.insert(key.clone()) {
                continue;
            }
            let previous = owners.get(&key).copied();
            if previous
                .is_none_or(|old| self.assign(old, slots, owners, chosen, seen, allow_recent))
            {
                owners.insert(key, slot);
                chosen[slot] = Some(*index);
                return true;
            }
        }
        false
    }
    pub fn freeze(
        &self,
        config: &AssessmentConfig,
        seed: &str,
    ) -> Result<(SamplingSnapshot, Vec<FrozenItem>), AppError> {
        let (selected, missing) = self.select(seed, true);
        if missing > 0 {
            return Err(AppError::new(
                "assessment_insufficient_items",
                format!("配额缺少 {} 个不重复目标，请重新预览。", missing),
            ));
        }
        let mut snapshots = vec![];
        let mut repeated = 0;
        for index in selected {
            let item = &self.items[index];
            let mut content = item.content.clone();
            // Original option ids (e.g. o0) must not encode the answer in the public projection.
            let correct = content.correct_option_id.clone();
            for choice in &mut content.choices {
                let old = choice.id.clone();
                choice.id = digest(format!("{seed}:{}:{old}", item.id).as_bytes())[..24].into();
                if correct.as_ref() == Some(&old) {
                    content.correct_option_id = Some(choice.id.clone());
                }
            }
            content.choices.sort_by_cached_key(|choice| {
                digest(format!("{seed}:choice:{}", choice.id).as_bytes())
            });
            let repeat_recent = self.recent.contains(&identity(item));
            repeated += usize::from(repeat_recent);
            let n = if item.kind == "word" {
                config.word_top_n
            } else {
                config.phrase_top_n
            };
            let band = (item.target_rank - 1) / config.band_size;
            snapshots.push(FrozenItem {
                bank_id: item.bank_id.clone(),
                item_id: item.id.clone(),
                catalog: self
                    .preview
                    .catalogs
                    .iter()
                    .find(|c| c.id == item.catalog_id)
                    .unwrap()
                    .clone(),
                target_rank: item.target_rank,
                lexeme_key: item.lexeme_key.clone(),
                kind: item.kind.clone(),
                question_type: item.question_type.clone(),
                from_rank: band * config.band_size + 1,
                to_rank: ((band + 1) * config.band_size).min(n),
                content,
                repeat_recent,
            });
        }
        // Question order is random as well as target allocation and choices; the saved sequence is authoritative.
        snapshots
            .sort_by_cached_key(|i| digest(format!("{seed}:position:{}", i.item_id).as_bytes()));
        let mut warnings = self
            .preview
            .warnings
            .iter()
            .filter(|w| !w.starts_with("题库有限，预计"))
            .cloned()
            .collect::<Vec<_>>();
        if repeated > 0 {
            warnings.push(format!(
                "本次包含 {} 个近期已测目标；均在原频段补入。",
                repeated
            ));
        }
        Ok((
            SamplingSnapshot {
                seed: seed.into(),
                inventory_revision: self.preview.inventory_revision.clone(),
                policy_version: POLICY.into(),
                score_policy: crate::assessment::SCORE_POLICY.into(),
                quotas: self.preview.quotas.clone(),
                coverage: self.preview.coverage.clone(),
                catalogs: self.preview.catalogs.clone(),
                warnings,
                repeated_targets: repeated,
            },
            snapshots,
        ))
    }
}
