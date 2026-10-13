use rusqlite::{Connection, params};
use serde_json::{Value, json};
use std::{
    collections::HashSet,
    io::{Read, Write},
    net::TcpListener,
    sync::Arc,
    time::{Duration, Instant},
};
use subtitle_vocabulary_list_core::{
    application::{Application, Settings},
    store::Store,
    tasks::TaskManager,
};

#[test]
fn words_and_sentences_are_prepared_once_and_reused_across_episodes() {
    let directory = tempfile::tempdir().unwrap();
    let database = directory.path().join("vocabulary.sqlite3");
    let old = Connection::open(&database).unwrap();
    for migration in [
        include_str!("../migrations/001_initial.sql"),
        include_str!("../migrations/002_imports.sql"),
        include_str!("../migrations/003_reviews.sql"),
        include_str!("../migrations/004_example_contexts.sql"),
        include_str!("../migrations/005_sync.sql"),
        include_str!("../migrations/006_explanations.sql"),
        include_str!("../migrations/007_query_results.sql"),
    ] {
        old.execute_batch(migration).unwrap();
    }
    old.pragma_update(None, "user_version", 7).unwrap();
    let first = "The fresh patient arrived.";
    let second = "Please be patient.";
    for (source, rows) in [
        ("a", vec![("patient", first), ("fresh", first)]),
        (
            "b",
            vec![("Patient", second), ("patient", first), ("fresh", first)],
        ),
    ] {
        old.execute("INSERT INTO sources(id,kind,title,fingerprint,duration_ms,corpus_path,audio_path,text_source,imported_at,created_at) VALUES (?,'video',?,?,1000,'fixture.json','fixture.wav','embedded_text',1,1)",params![source,source,source]).unwrap();
        for (index, (word, sentence)) in rows.into_iter().enumerate() {
            let id = format!("{source}-{index}");
            old.execute("INSERT INTO examples(id,source_id,location_key,identity_key,text,created_at) VALUES (?,?,?,?,?,1)",params![id,source,id,id,sentence]).unwrap();
            old.execute("INSERT INTO occurrences(id,source_id,example_id,candidate_key,raw_text,token_start,token_end,kind) VALUES (?,?,?,?,?,0,1,'word')",params![id,source,id,word.to_lowercase(),word]).unwrap();
        }
    }
    old.execute("INSERT INTO explanations VALUES ('patient',?,'病人','新来的病人到了。','原有语境解释','old','old',1)",[first]).unwrap();
    drop(old);
    let store = Arc::new(Store::open(directory.path()).unwrap());
    assert_eq!(store.schema_version().unwrap(), 8);
    assert_eq!(
        store.sentence_translation(first).unwrap().as_deref(),
        Some("新来的病人到了。")
    );
    let work = store.explanation_work(None).unwrap();
    let partial = store.prepared_explanation("fresh", first).unwrap().unwrap();
    assert!(partial.value.meaning.is_empty());
    assert_eq!(partial.value.translation, "新来的病人到了。");
    assert_eq!(work.len(), 3); // two shared words and one missing sentence
    assert_eq!(
        work.iter()
            .filter(|v| v.kind != "sentence")
            .map(|v| v.text.as_str())
            .collect::<HashSet<_>>(),
        HashSet::from(["patient", "fresh"])
    );

    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let server = std::thread::spawn(move || {
        let mut words = Vec::new();
        let mut sentences = Vec::new();
        for _ in 0..2 {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(5)))
                .unwrap();
            let mut bytes = Vec::new();
            let request = loop {
                let mut buffer = [0; 4096];
                let n = stream.read(&mut buffer).unwrap();
                assert!(n > 0);
                bytes.extend_from_slice(&buffer[..n]);
                if let Some(end) = bytes.windows(4).position(|v| v == b"\r\n\r\n") {
                    let header = String::from_utf8_lossy(&bytes[..end]);
                    let len: usize = header
                        .lines()
                        .find_map(|line| {
                            line.to_lowercase()
                                .strip_prefix("content-length:")
                                .map(|v| v.trim().parse().unwrap())
                        })
                        .unwrap();
                    if bytes.len() >= end + 4 + len {
                        break serde_json::from_slice::<Value>(&bytes[end + 4..end + 4 + len])
                            .unwrap();
                    }
                }
            };
            let input: Value =
                serde_json::from_str(request["messages"][1]["content"].as_str().unwrap()).unwrap();
            let content = if let Some(targets) = input["targets"].as_array() {
                assert_eq!(input["context"], "");
                json!({"translation":"","items":targets.iter().map(|v|{
                    let text=v["text"].as_str().unwrap(); words.push(text.to_owned());
                    json!({"id":v["id"],"meaning":if text=="patient" {"病人；有耐心的"}else{"新鲜的；新的"},"notes":"通用用法"})
                }).collect::<Vec<_>>()})
            } else {
                let values = input["sentences"].as_array().unwrap();
                json!({"items":values.iter().map(|v|{
                    sentences.push(v["text"].as_str().unwrap().to_owned()); json!({"id":v["id"],"translation":"请耐心一些。"})
                }).collect::<Vec<_>>()})
            };
            let body = json!({"choices":[{"message":{"content":content.to_string()}}]}).to_string();
            write!(stream,"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",body.len(),body).unwrap();
        }
        (words, sentences)
    });
    let app = Arc::new(Application::new(
        Arc::clone(&store),
        TaskManager::new(Arc::clone(&store), Arc::new(|_| {})),
        Settings {
            model_url: url,
            model_batch_size: 10,
            ..Default::default()
        },
    ));
    let task = app
        .prepare_explanations(Some("b".into()), true)
        .unwrap()
        .task
        .unwrap();
    let started = Instant::now();
    let done = loop {
        let value = app.tasks.get(&task.id).unwrap();
        if value.terminal() {
            break value;
        }
        assert!(started.elapsed() < Duration::from_secs(8));
        std::thread::sleep(Duration::from_millis(10));
    };
    assert_eq!(done.state, "succeeded", "{:?}", done.error);
    assert_eq!(done.total, 3);
    let (words, sentences) = server.join().unwrap();
    assert_eq!(words.len(), 2);
    assert_eq!(
        words.into_iter().collect::<HashSet<_>>(),
        HashSet::from(["patient".to_owned(), "fresh".to_owned()])
    );
    assert_eq!(sentences, vec![second.to_owned()]);
    assert!(store.explanation_work(None).unwrap().is_empty());
    assert!(
        app.prepare_explanations(None, false)
            .unwrap()
            .task
            .is_none()
    );
    let contextual = store
        .prepared_explanation("patient", first)
        .unwrap()
        .unwrap();
    assert_eq!(contextual.meaning_scope, "context");
    assert_eq!(contextual.value.meaning, "病人");
    let general = store
        .prepared_explanation("Patient", second)
        .unwrap()
        .unwrap();
    assert_eq!(general.meaning_scope, "general");
    assert_eq!(general.value.meaning, "病人；有耐心的");
    assert_eq!(general.value.translation, "请耐心一些。");
    drop(app);
    drop(store);
    let reopened = Store::open(directory.path()).unwrap();
    assert!(reopened.explanation_work(None).unwrap().is_empty());
    assert_eq!(
        reopened
            .prepared_explanation("patient", second)
            .unwrap()
            .unwrap()
            .value,
        general.value
    );
}
