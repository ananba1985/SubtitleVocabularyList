use rusqlite::{Connection, params};
use serde_json::json;
use std::{
    io::{Read, Write},
    net::TcpListener,
    sync::Arc,
    time::{Duration, Instant},
};
use subtitle_vocabulary_list_core::{
    application::{Application, Settings},
    explanations::Explanation,
    queries::{QueryInput, QueryOrigin},
    store::Store,
    tasks::{TaskManager, TaskSnapshot},
};
use uuid::Uuid;

fn finish(app: &Application, task: TaskSnapshot) -> TaskSnapshot {
    let started = Instant::now();
    loop {
        let snapshot = app.tasks.get(&task.id).unwrap();
        if snapshot.terminal() {
            assert_eq!(snapshot.state, "succeeded", "{:?}", snapshot.error);
            return snapshot;
        }
        assert!(started.elapsed() < Duration::from_secs(5));
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn local_dictionary_serves_words_and_only_missing_meanings_reach_the_model() {
    let directory = tempfile::tempdir().unwrap();
    std::fs::create_dir(directory.path().join("dictionaries")).unwrap();
    let dictionary = Connection::open(directory.path().join("dictionaries/base.sqlite3")).unwrap();
    dictionary.execute_batch("CREATE TABLE dictionary_entries(word TEXT PRIMARY KEY,meaning TEXT);
CREATE TABLE dictionary_aliases(form TEXT PRIMARY KEY,lemma TEXT);
CREATE TABLE metadata(key TEXT PRIMARY KEY,value TEXT);
INSERT INTO dictionary_entries VALUES ('patient','病人'),('apple','苹果');
INSERT INTO dictionary_aliases VALUES ('apples','apple');
INSERT INTO metadata VALUES ('source','合成基础词库');").unwrap();
    drop(dictionary);
    let ecdict = Connection::open(directory.path().join("dictionaries/ecdict.sqlite3")).unwrap();
    ecdict.execute_batch("CREATE TABLE dictionary_entries(word TEXT PRIMARY KEY,meaning TEXT);
CREATE TABLE dictionary_aliases(form TEXT PRIMARY KEY,lemma TEXT);
INSERT INTO dictionary_entries VALUES ('patient','ECDICT 后备病人'),('apples','ECDICT 直接词头'),('harvest','收获'),('murmur','低声说'),('can''t','不能'),('fishing','钓鱼'),('they''d','厚皮的');
INSERT INTO dictionary_aliases VALUES ('murmured','murmur');").unwrap();
    drop(ecdict);

    let store = Arc::new(Store::open(directory.path()).unwrap());
    let sentence = "The patient murmured and was reluctant to harvest apples.";
    store.connection().unwrap().execute("INSERT INTO sources(id,kind,title,fingerprint,duration_ms,corpus_path,imported_at,created_at) VALUES ('episode','video','Synthetic S01E01','fingerprint',1000,'fixture.json',1,1)", []).unwrap();
    store.connection().unwrap().execute("INSERT INTO examples(id,source_id,location_key,identity_key,text,created_at) VALUES ('sentence','episode','sentence','sentence',?,1)", [sentence]).unwrap();
    for word in ["patient", "apples", "harvest", "murmured", "can’t", "fishin", "hasn’t", "wouldnt", "they’d", "reluctant"] {
        store.connection().unwrap().execute("INSERT INTO occurrences(id,source_id,example_id,candidate_key,raw_text,token_start,token_end,kind) VALUES (?,'episode','sentence',?,?,0,1,'word')", params![word,word,word]).unwrap();
    }
    store.save_sentence_translation(sentence, "这位病人低声嘟囔，不情愿采摘苹果。", "fixture", "fixture").unwrap();
    store.save_explanation("patient", "Please be patient.", &Explanation {
        meaning: "耐心的".into(), translation: "请耐心一点。".into(), notes: "保留原语境解释".into(),
    }, "fixture", "fixture").unwrap();
    let direct = store.prepared_explanation("PATIENT", sentence).unwrap().unwrap();
    assert_eq!(direct.meaning_scope, "dictionary");
    assert_eq!(direct.value.meaning, "病人");
    assert_eq!(direct.value.translation, "这位病人低声嘟囔，不情愿采摘苹果。");
    assert!(direct.value.notes.contains("合成基础词库"));
    assert_eq!(store.prepared_explanation("patient", "Please be patient.").unwrap().unwrap().value.meaning, "耐心的");
    let lemma = store.prepared_explanation("apples", sentence).unwrap().unwrap();
    assert_eq!(lemma.value.meaning, "苹果");
    assert!(lemma.value.notes.contains("apples → apple"));
    let fallback = store.prepared_explanation("harvest", sentence).unwrap().unwrap();
    assert_eq!(fallback.value.meaning, "收获");
    assert!(fallback.value.notes.contains("ECDICT"));
    let fallback_lemma = store.prepared_explanation("murmured", sentence).unwrap().unwrap();
    assert_eq!(fallback_lemma.value.meaning, "低声说");
    assert!(fallback_lemma.value.notes.contains("murmured → murmur"));
    let apostrophe = store.prepared_explanation("can’t", sentence).unwrap().unwrap();
    assert_eq!(apostrophe.value.meaning, "不能");
    assert!(apostrophe.value.notes.contains("撇号字形规范化"));
    assert_eq!(store.prepared_explanation("fishin", sentence).unwrap().unwrap().value.meaning, "钓鱼");
    assert!(store.prepared_explanation("hasn’t", sentence).unwrap().unwrap().value.meaning.contains("has not"));
    assert!(store.prepared_explanation("wouldnt", sentence).unwrap().unwrap().value.meaning.contains("would not"));
    let corrected = store.prepared_explanation("they’d", sentence).unwrap().unwrap();
    assert!(corrected.value.meaning.contains("they had"));
    assert!(corrected.value.notes.contains("校订"));
    let work = store.explanation_work(None).unwrap();
    assert_eq!(work.len(), 1);
    assert_eq!(work[0].text, "reluctant");

    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let settings = Settings { model_url: format!("http://{}", listener.local_addr().unwrap()), ..Settings::default() };
    let server = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        stream.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
        let mut bytes = Vec::new();
        loop {
            let mut buffer = [0; 4096];
            let read = stream.read(&mut buffer).unwrap();
            assert!(read > 0);
            bytes.extend_from_slice(&buffer[..read]);
            if let Some(end) = bytes.windows(4).position(|v| v == b"\r\n\r\n") {
                let header = String::from_utf8_lossy(&bytes[..end]);
                let size: usize = header.lines().find_map(|line| line.to_lowercase().strip_prefix("content-length:").map(|size| size.trim().parse().unwrap())).unwrap();
                if bytes.len() >= end + 4 + size { break; }
            }
        }
        let request = String::from_utf8(bytes).unwrap();
        assert!(request.contains("reluctant"));
        assert!(!request.contains("patient"));
        assert!(!request.contains("apples"));
        assert!(!request.contains("harvest"));
        assert!(!request.contains("murmured"));
        let content = json!({"meaning":"不情愿的","translation":"","notes":"通用释义"}).to_string();
        let body = json!({"choices":[{"message":{"content":content}}]}).to_string();
        write!(stream,"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",body.len(),body).unwrap();
        stream.flush().unwrap();
    });
    let tasks = TaskManager::new(Arc::clone(&store), Arc::new(|_| {}));
    let app = Arc::new(Application::new(Arc::clone(&store), tasks, settings));
    let explained = finish(&app, app.explain_start("fishin".into(), sentence.into(), Uuid::new_v4().to_string()).unwrap());
    assert_eq!(explained.result.unwrap()["meaning"], "钓鱼");

    let session = app.queries.open(QueryInput {
        target: "can’t".into(), context: sentence.into(), mode: "explain".into(),
        source_language: "en".into(), target_language: "zh-CN".into(), origin: QueryOrigin::default(),
    }, None).unwrap();
    app.query_start(session.session_id.clone(), session.input_revision, vec!["qwen".into()], false, true).unwrap();
    let started = Instant::now();
    loop {
        let snapshot = app.queries.get(Some(&session.session_id)).unwrap().unwrap();
        if snapshot.results[0].state == "ready" {
            assert_eq!(snapshot.results[0].cache_source, "dictionary");
            assert_eq!(snapshot.results[0].value.as_ref().unwrap().meaning, "不能");
            break;
        }
        assert!(started.elapsed() < Duration::from_secs(5));
        std::thread::sleep(Duration::from_millis(10));
    }
    finish(&app, app.prepare_explanations(None, true).unwrap().task.unwrap());
    server.join().unwrap();
    assert!(store.explanation_work(None).unwrap().is_empty());
    assert!(store.explanation("apples", "").unwrap().is_none());
    assert_eq!(store.connection().unwrap().query_row("SELECT count(*) FROM entries", [], |row| row.get::<_, i64>(0)).unwrap(), 0);
    let reopened = Store::open(directory.path()).unwrap();
    assert_eq!(reopened.prepared_explanation("apples", sentence).unwrap().unwrap().value.meaning, "苹果");
}
