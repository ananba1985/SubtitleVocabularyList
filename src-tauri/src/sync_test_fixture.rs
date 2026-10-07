//! Synthetic histories for transport tests; runtime user data is never embedded.
use crate::{
    reviews::AnswerInput,
    store::Store,
    vocabulary::{CollectionInput, ExampleInput, digest},
};
use rusqlite::{params, types::Value as SqlValue};
use uuid::Uuid;

pub(crate) fn large_entry(store: &Store, audio: Option<&[u8]>, attempts: usize) -> String {
    let mut asset_ids = vec![];
    if let Some(bytes) = audio {
        let hash = digest(bytes);
        let asset = Uuid::new_v4().to_string();
        std::fs::write(store.root().join("media/fixture-original.m4a"), bytes).unwrap();
        store.connection().unwrap().execute("INSERT INTO media_assets(id,recipe_key,digest,relative_path,kind,format,duration_ms,state,created_at) VALUES (?, ?,?,'media/fixture-original.m4a','original','m4a',1710,'ready',?)",params![asset,hash,hash,chrono::Utc::now().timestamp_millis()]).unwrap();
        asset_ids.push(asset);
    }
    let entry = store
        .collect(&CollectionInput {
            operation_id: Uuid::new_v4().to_string(),
            kind: "word".into(),
            text: format!("svl-large-history-fixture-{}", Uuid::new_v4()),
            meaning: "大词条同步验证".into(),
            examples: vec![ExampleInput {
                text: if audio.is_some() {
                    "Kids, breakfast!".into()
                } else {
                    "A synthetic transport context. ".repeat(30)
                },
                media_asset_ids: asset_ids,
                ..Default::default()
            }],
            target_entry_id: None,
            expected_revision: None,
        })
        .unwrap()
        .entry_id;
    let unit = store
        .review_units("all", "meaning", 0, 100)
        .unwrap()
        .into_iter()
        .find(|u| u.entry_id == entry)
        .unwrap();
    let question = store.review_question(&unit.id, unit.revision).unwrap();
    store
        .review_submit(&AnswerInput {
            operation_id: Uuid::new_v4().to_string(),
            question_id: question.id,
            answer: "大词条同步验证".into(),
            unable: false,
        })
        .unwrap();
    let bundle = store.sync_export(&entry).unwrap();
    let base = &bundle.tables["review_attempts"][0];
    let fields = base
        .as_object()
        .unwrap()
        .keys()
        .cloned()
        .collect::<Vec<_>>();
    let sql = format!(
        "INSERT INTO review_attempts({}) VALUES ({})",
        fields.join(","),
        vec!["?"; fields.len()].join(",")
    );
    let mut connection = store.connection().unwrap();
    let transaction = connection.transaction().unwrap();
    {
        let mut statement = transaction.prepare(&sql).unwrap();
        for index in 1..attempts {
            let mut row = base.clone();
            row["id"] = Uuid::new_v4().to_string().into();
            row["operation_id"] = Uuid::new_v4().to_string().into();
            row["request_hash"] = digest(format!("synthetic-history-{index}").as_bytes()).into();
            row["created_at"] = (base["created_at"].as_i64().unwrap() + index as i64).into();
            let values = fields
                .iter()
                .map(|key| match &row[key] {
                    serde_json::Value::String(value) => SqlValue::Text(value.clone()),
                    serde_json::Value::Number(value) => SqlValue::Integer(value.as_i64().unwrap()),
                    _ => panic!("Fixed history columns are strings or integers"),
                })
                .collect::<Vec<_>>();
            statement
                .execute(rusqlite::params_from_iter(values))
                .unwrap();
        }
    }
    transaction.commit().unwrap();
    drop(connection);
    crate::sync_merge::validate(&store.sync_export(&entry).unwrap()).unwrap();
    entry
}
