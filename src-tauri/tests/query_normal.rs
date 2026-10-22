use serde_json::json;
use std::{
    io::{Read, Write},
    net::TcpListener,
    sync::Arc,
    time::{Duration, Instant},
};
use subtitle_vocabulary_list_core::{
    application::{Application, Settings},
    providers::{PluginManifest, ProviderUpdate},
    queries::{QueryInput, QueryOrigin},
    store::Store,
    tasks::TaskManager,
    vocabulary::{CollectionInput, ExampleInput},
};

fn finished(app: &Application, id: &str) -> subtitle_vocabulary_list_core::queries::QuerySession {
    let start = Instant::now();
    loop {
        let session = app.queries.get(Some(id)).unwrap().unwrap();
        if session.results.iter().all(|r| r.state == "ready") {
            return session;
        }
        assert!(
            start.elapsed() < Duration::from_secs(8),
            "{:?}",
            session.results
        );
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn automatic_query_translates_chinese_and_explains_english() {
    let directory = tempfile::tempdir().unwrap();
    let store = Arc::new(Store::open(directory.path()).unwrap());
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let settings = Settings {
        model_url: format!("http://{}", listener.local_addr().unwrap()),
        ..Default::default()
    };
    let server = std::thread::spawn(move || {
        for (target, source, destination, value) in [
            ("最终", "zh-CN", "en", json!({"meaning":"","translation":"finally","notes":""})),
            ("eventually", "en", "zh-CN", json!({"meaning":"最终；终于","translation":"","notes":""})),
        ] {
            let (mut stream, _) = listener.accept().unwrap();
            stream.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
            let mut data = Vec::new();
            let request: serde_json::Value = loop {
                let mut buffer = [0; 4096];
                let size = stream.read(&mut buffer).unwrap();
                assert!(size > 0);
                data.extend_from_slice(&buffer[..size]);
                if let Some(end) = data.windows(4).position(|p| p == b"\r\n\r\n") {
                    let length: usize = String::from_utf8_lossy(&data[..end]).lines()
                        .find_map(|line| line.to_lowercase().strip_prefix("content-length:")
                            .map(|n| n.trim().parse().unwrap())).unwrap();
                    if data.len() >= end + 4 + length {
                        break serde_json::from_slice(&data[end + 4..end + 4 + length]).unwrap();
                    }
                }
            };
            let input: serde_json::Value = serde_json::from_str(request["messages"][1]["content"].as_str().unwrap()).unwrap();
            assert_eq!(input["target"], target);
            assert_eq!(input["sourceLanguage"], source);
            assert_eq!(input["targetLanguage"], destination);
            let system = request["messages"][0]["content"].as_str().unwrap();
            if source == "zh-CN" {
                assert!(system.contains("将 target 完整译成 targetLanguage"));
                assert_eq!(request["max_tokens"], 2048);
            } else {
                assert!(system.contains("常用的简短中文解释"));
            }
            let response = json!({"choices":[{"finish_reason":"stop","message":{"content":value.to_string()}}]}).to_string();
            write!(stream, "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}", response.len(), response).unwrap();
        }
    });
    let app = Arc::new(Application::new(
        Arc::clone(&store),
        TaskManager::new(Arc::clone(&store), Arc::new(|_| {})),
        settings,
    ));
    // Omitted languages use the same automatic defaults as the query window.
    let mut input: QueryInput = serde_json::from_value(json!({"target":"最终","context":"","mode":"auto"})).unwrap();
    let mut session_id: Option<String> = None;
    for (target, expected) in [("最终", "finally"), ("eventually", "最终；终于")] {
        input.target = target.into();
        let session = app.queries.open(input.clone(), session_id.as_deref()).unwrap();
        app.query_start(session.session_id.clone(), session.input_revision, vec!["qwen".into()], false, true).unwrap();
        let completed = finished(&app, &session.session_id);
        assert_eq!(completed.input.source_language, "auto");
        assert_eq!(completed.input.target_language, "auto");
        assert_eq!(completed.results[0].value.as_ref().unwrap().text(), expected);
        session_id = Some(session.session_id);
    }
    server.join().unwrap();
    assert!(store.list_entries("", 0, 40).unwrap().is_empty());
}

#[test]
fn named_project_explanation_in_notes_is_displayed_and_saved() {
    let directory = tempfile::tempdir().unwrap();
    let store = Arc::new(Store::open(directory.path()).unwrap());
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let settings = Settings {
        model_url: format!("http://{}", listener.local_addr().unwrap()),
        ..Default::default()
    };
    let definition = "HeadScale 是一个开源的 Tailscale 控制服务器，用于管理 Tailscale 网络。";
    // Replay the actual local model's response to the user's headscale query.
    let server = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        stream.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
        let mut data = Vec::new();
        let request = loop {
            let mut buffer = [0; 4096];
            let size = stream.read(&mut buffer).unwrap();
            assert!(size > 0);
            data.extend_from_slice(&buffer[..size]);
            if let Some(end) = data.windows(4).position(|p| p == b"\r\n\r\n") {
                let length: usize = String::from_utf8_lossy(&data[..end]).lines()
                    .find_map(|line| line.to_lowercase().strip_prefix("content-length:")
                        .map(|n| n.trim().parse().unwrap())).unwrap();
                if data.len() >= end + 4 + length {
                    break serde_json::from_slice::<serde_json::Value>(&data[end + 4..end + 4 + length]).unwrap();
                }
            }
        };
        assert!(request["messages"][0]["content"].as_str().unwrap().contains("专名保留原拼写"));
        let value = json!({"meaning":"headscale","translation":"","notes":definition});
        let response = json!({"choices":[{"finish_reason":"stop","message":{"content":value.to_string()}}]}).to_string();
        write!(stream, "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}", response.len(), response).unwrap();
    });
    let app = Arc::new(Application::new(Arc::clone(&store), TaskManager::new(Arc::clone(&store), Arc::new(|_| {})), settings));
    let input = QueryInput {
        target: "headscale".into(), context: String::new(), mode: "explain".into(),
        source_language: "en".into(), target_language: "zh-CN".into(), origin: QueryOrigin::default(),
    };
    let session = app.queries.open(input.clone(), None).unwrap();
    app.query_start(session.session_id.clone(), session.input_revision, vec!["qwen".into()], false, true).unwrap();
    let result = finished(&app, &session.session_id);
    server.join().unwrap();
    let value = result.results[0].value.as_ref().unwrap();
    assert_eq!(value.meaning, definition);
    assert!(value.notes.is_empty());
    assert_eq!(store.explanation("headscale", "").unwrap().unwrap().meaning, definition);
    let session = app.queries.open(QueryInput { target: "Headscale".into(), ..input }, None).unwrap();
    app.query_start(session.session_id.clone(), session.input_revision, vec!["qwen".into()], false, true).unwrap();
    let reused = finished(&app, &session.session_id);
    assert_eq!(reused.results[0].cache_source, "stored");
    assert_eq!(reused.results[0].value.as_ref().unwrap().meaning, definition);
    assert!(store.list_entries("", 0, 40).unwrap().is_empty());
}

#[test]
fn captured_word_is_explained_without_a_sentence_and_saved_for_reuse() {
    let directory = tempfile::tempdir().unwrap();
    let store = Arc::new(Store::open(directory.path()).unwrap());
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let settings = Settings {
        model_url: format!("http://{}", listener.local_addr().unwrap()),
        ..Default::default()
    };
    let server = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        stream.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
        let mut data = Vec::new();
        let request = loop {
            let mut buffer = [0; 4096];
            let size = stream.read(&mut buffer).unwrap();
            assert!(size > 0);
            data.extend_from_slice(&buffer[..size]);
            if let Some(end) = data.windows(4).position(|p| p == b"\r\n\r\n") {
                let length: usize = String::from_utf8_lossy(&data[..end])
                    .lines()
                    .find_map(|line| line.to_lowercase().strip_prefix("content-length:")
                        .map(|n| n.trim().parse().unwrap()))
                    .unwrap();
                if data.len() >= end + 4 + length {
                    break serde_json::from_slice::<serde_json::Value>(&data[end + 4..end + 4 + length]).unwrap();
                }
            }
        };
        let input: serde_json::Value = serde_json::from_str(request["messages"][1]["content"].as_str().unwrap()).unwrap();
        assert_eq!(input["target"], "Certified");
        assert_eq!(input["context"], "");
        let explanation = json!({"meaning":"经认证的；合格的","translation":"","notes":"certify 的过去分词，也用作形容词。"});
        let response = json!({"choices":[{"finish_reason":"stop","message":{"content":explanation.to_string()}}]}).to_string();
        write!(stream, "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}", response.len(), response).unwrap();
    });
    let app = Arc::new(Application::new(
        Arc::clone(&store),
        TaskManager::new(Arc::clone(&store), Arc::new(|_| {})),
        settings.clone(),
    ));
    let mut input = QueryInput {
        target: "Certified".into(),
        context: "Certified".into(),
        mode: "auto".into(),
        source_language: "en".into(),
        target_language: "zh-CN".into(),
        origin: QueryOrigin { kind: "selection".into(), ..Default::default() },
    };
    let session = app.queries.open(input.clone(), None).unwrap();
    app.query_start(session.session_id.clone(), session.input_revision, vec!["qwen".into()], true, true).unwrap();
    let session = finished(&app, &session.session_id);
    server.join().unwrap();
    assert_eq!(session.input.context, "Certified");
    assert_eq!(session.input.origin.kind, "selection");
    assert_eq!(session.results[0].value.as_ref().unwrap().meaning, "经认证的；合格的");
    assert!(session.results[0].value.as_ref().unwrap().translation.is_empty());
    assert_eq!(store.explanation("certified", "").unwrap().unwrap().meaning, "经认证的；合格的");
    assert!(store.list_entries("", 0, 40).unwrap().is_empty());
    drop(app);
    drop(store);

    let store = Arc::new(Store::open(directory.path()).unwrap());
    let app = Arc::new(Application::new(
        Arc::clone(&store),
        TaskManager::new(Arc::clone(&store), Arc::new(|_| {})),
        settings,
    ));
    input.target = "certified".into();
    input.context.clear();
    let session = app.queries.open(input, None).unwrap();
    app.query_start(session.session_id.clone(), session.input_revision, vec!["qwen".into()], false, true).unwrap();
    let session = finished(&app, &session.session_id);
    assert_eq!(session.results[0].cache_source, "stored");
    assert_eq!(session.results[0].value.as_ref().unwrap().meaning, "经认证的；合格的");
}

#[test]
fn automatic_paragraph_translates_complete_text_and_reuses_translation_cache() {
    let directory = tempfile::tempdir().unwrap();
    let store = Arc::new(Store::open(directory.path()).unwrap());
    let target = "The visitors have left the museum and are asking me for directions outside. I will print a few cards to hand out";
    let translated = "参观者已经离开博物馆，正在外面向我问路。我会打印几张卡片发给他们。";
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let settings = Settings {
        model_url: format!("http://{}", listener.local_addr().unwrap()),
        ..Default::default()
    };
    let previous = subtitle_vocabulary_list_core::explanations::Explanation {
        meaning: "方向".into(), translation: String::new(), notes: "已有词义资料。".into(),
    };
    store.save_explanation(target, "", &previous, &settings.model_url, &settings.model_name).unwrap();
    let server = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        stream.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
        let mut data = Vec::new();
        let request = loop {
            let mut buffer = [0; 4096];
            let size = stream.read(&mut buffer).unwrap();
            assert!(size > 0);
            data.extend_from_slice(&buffer[..size]);
            if let Some(end) = data.windows(4).position(|p| p == b"\r\n\r\n") {
                let length: usize = String::from_utf8_lossy(&data[..end]).lines()
                    .find_map(|line| line.to_lowercase().strip_prefix("content-length:")
                        .map(|n| n.trim().parse().unwrap())).unwrap();
                if data.len() >= end + 4 + length {
                    break serde_json::from_slice::<serde_json::Value>(&data[end + 4..end + 4 + length]).unwrap();
                }
            }
        };
        let input: serde_json::Value = serde_json::from_str(request["messages"][1]["content"].as_str().unwrap()).unwrap();
        assert_eq!(input["target"], target);
        assert_eq!(input["context"], target);
        assert!(request["messages"][0]["content"].as_str().unwrap().contains("将 target 完整译成"));
        assert_eq!(request["max_tokens"], 2048);
        let value = json!({"meaning":"","translation":translated,"notes":""});
        let response = json!({"choices":[{"finish_reason":"stop","message":{"content":value.to_string()}}]}).to_string();
        write!(stream, "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}", response.len(), response).unwrap();
    });
    let app = Arc::new(Application::new(Arc::clone(&store), TaskManager::new(Arc::clone(&store), Arc::new(|_| {})), settings));
    let input = QueryInput {
        target: target.into(), context: String::new(), mode: "auto".into(),
        source_language: "en".into(), target_language: "zh-CN".into(), origin: QueryOrigin::default(),
    };
    let session = app.queries.open(input.clone(), None).unwrap();
    app.query_start(session.session_id.clone(), session.input_revision, vec!["qwen".into()], false, true).unwrap();
    let completed = finished(&app, &session.session_id);
    server.join().unwrap();
    assert_eq!(completed.input.mode, "auto");
    assert_eq!(completed.results[0].cache_source, "provider");
    assert_eq!(completed.results[0].value.as_ref().unwrap().translation, translated);
    assert!(completed.results[0].value.as_ref().unwrap().meaning.is_empty());
    assert_eq!(store.explanation(target, "").unwrap().unwrap(), previous);
    // Explicit translation and automatic translation share the valid translation
    // cache. Neither can accidentally reuse a stored word explanation.
    let session = app.queries.open(QueryInput { mode: "translate".into(), ..input }, None).unwrap();
    app.query_start(session.session_id.clone(), session.input_revision, vec!["qwen".into()], false, true).unwrap();
    let reused = finished(&app, &session.session_id);
    assert_eq!(reused.results[0].cache_source, "cache");
    assert_eq!(reused.results[0].value.as_ref().unwrap().translation, translated);
    assert!(store.list_entries("", 0, 40).unwrap().is_empty());
}

#[test]
fn query_multi_provider_collection_cache_and_plugin_normal_flow() {
    let directory = tempfile::tempdir().unwrap();
    let store = Arc::new(Store::open(directory.path()).unwrap());
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let server = std::thread::spawn(move || {
        for _ in 0..2 {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(5)))
                .unwrap();
            let mut data = vec![];
            loop {
                let mut buffer = [0; 4096];
                let size = stream.read(&mut buffer).unwrap();
                assert!(size > 0);
                data.extend_from_slice(&buffer[..size]);
                if let Some(end) = data.windows(4).position(|p| p == b"\r\n\r\n") {
                    let length = String::from_utf8_lossy(&data[..end])
                        .lines()
                        .find_map(|line| {
                            line.to_lowercase()
                                .strip_prefix("content-length:")
                                .map(|n| n.trim().parse::<usize>().unwrap())
                        })
                        .unwrap();
                    if data.len() >= end + 4 + length {
                        break;
                    }
                }
            }
            let explanation =
                json!({"meaning":"不情愿的","translation":"她不太情愿。","notes":"描述意愿较低。"})
                    .to_string();
            let response=json!({"meaning":"不愿意的（插件）","translation":"她不太愿意。","notes":"插件解释。","choices":[{"message":{"content":explanation}}]}).to_string();
            write!(stream,"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",response.len(),response).unwrap();
        }
    });
    let manifest:PluginManifest=serde_json::from_value(json!({"id":"fixture.translation","name":"合成翻译插件","version":"1.0.0","protocolVersion":1,"minHostVersion":"0.4.4","license":"CC0-1.0","capabilities":["translation"],"allowedHosts":["127.0.0.1"],"endpoint":format!("{url}/plugin"),"method":"POST","body":{"text":"{{text}}"},"meaningPointer":"/meaning","translationPointer":"/translation","notesPointer":"/notes"})).unwrap();
    store.plugin_install_manifest(manifest).unwrap();
    store
        .provider_update(ProviderUpdate {
            id: "fixture.translation".into(),
            enabled: true,
            auto_query: false,
            send_context: false,
            order: 1,
            max_concurrency: 1,
            values: Default::default(),
            secrets: Default::default(),
        })
        .unwrap();
    let defaults = Settings {
        model_url: url.clone(),
        ..Default::default()
    };
    let app = Arc::new(Application::new(
        Arc::clone(&store),
        TaskManager::new(Arc::clone(&store), Arc::new(|_| {})),
        defaults.clone(),
    ));
    let input = QueryInput {
        target: "reluctant".into(),
        context: "She was reluctant.".into(),
        mode: "explain".into(),
        source_language: "en".into(),
        target_language: "zh-CN".into(),
        origin: QueryOrigin::default(),
    };
    let session = app.queries.open(input.clone(), None).unwrap();
    app.query_start(
        session.session_id.clone(),
        session.input_revision,
        vec![],
        false,
        true,
    )
    .unwrap();
    let session = finished(&app, &session.session_id);
    assert_eq!(session.results.len(), 2);
    assert!(store.list_entries("", 0, 40).unwrap().is_empty());
    let seed = app
        .query_collection(
            &session.session_id,
            session.input_revision,
            "fixture.translation",
        )
        .unwrap();
    assert_eq!(seed["meaning"], "不愿意的（插件）");
    assert!(store.list_entries("", 0, 40).unwrap().is_empty());
    let saved = store
        .collect(&CollectionInput {
            operation_id: uuid::Uuid::new_v4().to_string(),
            kind: "word".into(),
            text: input.target.clone(),
            meaning: seed["meaning"].as_str().unwrap().into(),
            examples: vec![ExampleInput {
                text: input.context.clone(),
                context_meaning: seed["meaning"].as_str().unwrap().into(),
                source_id: None,
                location_key: String::new(),
                start_ms: None,
                end_ms: None,
                media_asset_ids: vec![],
            }],
            target_entry_id: None,
            expected_revision: None,
        })
        .unwrap();
    assert_eq!(app.tasks.history(0, 10).unwrap().total, 0);
    server.join().unwrap();
    drop(app);
    drop(store);
    let store = Arc::new(Store::open(directory.path()).unwrap());
    assert_eq!(store.schema_version().unwrap(), 7);
    let app = Arc::new(Application::new(
        Arc::clone(&store),
        TaskManager::new(Arc::clone(&store), Arc::new(|_| {})),
        defaults,
    ));
    let session = app.queries.open(input, None).unwrap();
    app.query_start(
        session.session_id.clone(),
        session.input_revision,
        vec![],
        false,
        true,
    )
    .unwrap();
    let session = finished(&app, &session.session_id);
    assert!(session.results.iter().all(|r| r.cache_source == "cache"));
    store.plugin_remove("fixture.translation").unwrap();
    assert_eq!(
        store.get_entry(&saved.entry_id).unwrap().meanings[0].text,
        "不愿意的（插件）"
    );
    assert_eq!(
        store
            .connection()
            .unwrap()
            .query_row("SELECT COUNT(*) FROM query_results", [], |r| r
                .get::<_, i64>(0))
            .unwrap(),
        2
    );
    println!(
        "Normal query flow: two services, explicit collection, restart cache, plugin removal preserves vocabulary; no query task history."
    );
}
