//! Pure three-way merging and preserved conflict copies.
use super::*;

pub(super) fn preserved_copy(
    packet: &AssessmentPacket,
    archive: &str,
) -> Result<AssessmentPacket, AppError> {
    let mut copy = packet.clone();
    let parent = copy.session.id.clone();
    let new_session = Uuid::new_v4().to_string();
    let items = copy
        .items
        .iter()
        .map(|i| (i.id.clone(), Uuid::new_v4().to_string()))
        .collect::<BTreeMap<_, _>>();
    let answers = copy
        .answers
        .iter()
        .map(|a| (a.answer.id.clone(), Uuid::new_v4().to_string()))
        .collect::<BTreeMap<_, _>>();
    let corrections = copy
        .corrections
        .iter()
        .map(|a| (a.id.clone(), Uuid::new_v4().to_string()))
        .collect::<BTreeMap<_, _>>();
    let notice = format!(
        "保留副本：来自并发会话 {parent}，原始两份资料存档 {archive}；此副本用于保留记录，不代表一次新测试或独立复测。"
    );
    let answer = |a: &mut AssessmentAnswer| {
        a.id = answers[&a.id].clone();
        a.session_id = new_session.clone();
        a.session_item_id = items[&a.session_item_id].clone();
    };
    let correction = |c: &mut AssessmentCorrection| {
        c.id = corrections[&c.id].clone();
        c.answer_id = answers[&c.answer_id].clone();
    };
    copy.session.id = new_session.clone();
    copy.session.sampling.warnings.push(notice.clone());
    for i in &mut copy.items {
        i.id = items[&i.id].clone();
    }
    for a in &mut copy.answers {
        answer(&mut a.answer);
    }
    for c in &mut copy.corrections {
        correction(c);
    }
    for link in &mut copy.collection_links {
        link.session_item_id = items[&link.session_item_id].clone();
        if let Some(details) = link.details.as_object_mut() {
            details.insert("sessionId".into(), json!(new_session));
        }
    }
    for value in &mut copy.reports {
        let mut r: AssessmentReport = serde_json::from_str(&value.report_json)?;
        r.session_id = new_session.clone();
        r.warnings.push(notice.clone());
        for row in &mut r.items {
            row.id = items[&row.id].clone();
            if let Some(a) = &mut row.answer {
                answer(a);
            }
            if let Some(a) = &mut row.original_answer {
                answer(a);
            }
            for c in &mut row.corrections {
                correction(c);
            }
        }
        value.report_json = canonical(&r)?;
        value.digest = digest(value.report_json.as_bytes());
    }
    copy.reports.sort_by(|a, b| a.digest.cmp(&b.digest));
    copy.validate()?;
    Ok(copy)
}
pub(super) fn equal<T: Serialize>(a: &T, b: &T) -> Result<bool, AppError> {
    Ok(canonical(a)? == canonical(b)?)
}
fn choose<T: Clone + Serialize>(base: &T, local: &T, remote: &T) -> Result<T, AppError> {
    if equal(local, remote)? || equal(base, remote)? {
        Ok(local.clone())
    } else if equal(base, local)? {
        Ok(remote.clone())
    } else {
        Err(AppError::new(
            "assessment_conflict",
            "同一测试状态或输入被两端修改，完整资料已保留。",
        ))
    }
}
fn union<T: Clone + Serialize>(
    local: &[T],
    remote: &[T],
    key: impl Fn(&T) -> String,
) -> Result<Vec<T>, AppError> {
    let mut out = BTreeMap::new();
    for row in local.iter().chain(remote) {
        let id = key(row);
        if let Some(old) = out.get(&id) {
            if !equal(old, row)? {
                return Err(AppError::new(
                    "assessment_conflict",
                    "同一测试事件的内容不同，保留两端完整版本。",
                ));
            }
        } else {
            out.insert(id, row.clone());
        }
    }
    Ok(out.into_values().collect())
}
pub(crate) fn merge(
    base: &AssessmentPacket,
    local: &AssessmentPacket,
    remote: &AssessmentPacket,
) -> Result<AssessmentPacket, AppError> {
    if !equal(&base.session.config, &local.session.config)?
        || !equal(&base.session.config, &remote.session.config)?
        || !equal(&base.session.sampling, &local.session.sampling)?
        || !equal(&base.session.sampling, &remote.session.sampling)?
        || base.origin_device_id != local.origin_device_id
        || base.origin_device_id != remote.origin_device_id
    {
        return Err(AppError::new(
            "assessment_conflict",
            "同一会话的冻结配置或抽样依据不一致。",
        ));
    }
    let mut merged = local.clone();
    for item in &mut merged.items {
        let b = base
            .items
            .iter()
            .find(|i| i.id == item.id)
            .ok_or_else(|| invalid("测试共同基准缺少题目。"))?;
        let r = remote
            .items
            .iter()
            .find(|i| i.id == item.id)
            .ok_or_else(|| invalid("远端测试缺少冻结题目。"))?;
        if item.position != r.position || !equal(&item.snapshot, &r.snapshot)? {
            return Err(AppError::new(
                "assessment_conflict",
                "已冻结的测试题目被修改。",
            ));
        }
        let b: Interaction = serde_json::from_str(&b.interaction_json)?;
        let r: Interaction = serde_json::from_str(&r.interaction_json)?;
        let mut interaction: Interaction = serde_json::from_str(&item.interaction_json)?;
        interaction.draft_text = choose(&b.draft_text, &interaction.draft_text, &r.draft_text)?;
        interaction.draft_option =
            choose(&b.draft_option, &interaction.draft_option, &r.draft_option)?;
        interaction.hinted |= r.hinted;
        interaction.hint_count = interaction.hint_count.max(r.hint_count);
        interaction.audio_played |= r.audio_played;
        interaction.audio_started |= r.audio_started;
        interaction.audio_seconds = interaction.audio_seconds.max(r.audio_seconds);
        item.interaction_json = canonical(&interaction)?;
    }
    // Revisions are derived from immutable correction events, not concurrent counters.
    let originals = union(
        &local
            .answers
            .iter()
            .map(|a| a.answer.clone())
            .collect::<Vec<_>>(),
        &remote
            .answers
            .iter()
            .map(|a| a.answer.clone())
            .collect::<Vec<_>>(),
        |a| a.id.clone(),
    )?;
    let corrections = union(&local.corrections, &remote.corrections, |c| c.id.clone())?;
    merged.answers = originals
        .into_iter()
        .map(|answer| {
            let revision = corrections
                .iter()
                .filter(|r| r.answer_id == answer.id)
                .map(|r| r.revision)
                .max()
                .unwrap_or(1);
            AnswerFacts { answer, revision }
        })
        .collect();
    merged.corrections = corrections;
    merged.reports = union(&local.reports, &remote.reports, |r| r.digest.clone())?;
    merged.collection_links = union(&local.collection_links, &remote.collection_links, |r| {
        format!("{}:{}", r.session_item_id, r.operation_id)
    })?;
    merged.media = local
        .media
        .iter()
        .chain(&remote.media)
        .cloned()
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect();
    merged.session.position = merged.answers.len();
    merged.session.state = if merged.session.position == merged.items.len() {
        "completed".into()
    } else {
        choose(
            &base.session.state,
            &local.session.state,
            &remote.session.state,
        )?
    };
    merged.session.revision = local.session.revision.max(remote.session.revision) + 1;
    merged.session.updated_at = local.session.updated_at.max(remote.session.updated_at);
    merged.validate()?;
    Ok(merged)
}
