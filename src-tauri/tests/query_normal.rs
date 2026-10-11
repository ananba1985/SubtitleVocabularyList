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
