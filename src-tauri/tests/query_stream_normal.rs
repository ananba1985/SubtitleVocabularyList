use serde_json::{Value, json};
use std::{
    io::{Read, Write},
    net::TcpListener,
    sync::{Arc, mpsc},
    thread,
    time::{Duration, Instant},
};
use subtitle_vocabulary_list_core::{
    application::{Application, Settings},
    queries::{QueryInput, QueryOrigin, QuerySession},
    store::Store,
    tasks::TaskManager,
};

fn wait(app: &Application, id: &str, condition: impl Fn(&QuerySession) -> bool) -> QuerySession {
    let started = Instant::now();
    loop {
        let session = app.queries.get(Some(id)).unwrap().unwrap();
        if condition(&session) {
            return session;
        }
        assert!(started.elapsed() < Duration::from_secs(8), "{session:?}");
        thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn local_query_streams_decoded_text_then_saves_complete_result() {
    let dir = tempfile::tempdir().unwrap();
    let store = Arc::new(Store::open(dir.path()).unwrap());
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let settings = Settings {
        model_url: format!("http://{}", listener.local_addr().unwrap()),
        ..Default::default()
    };
    let (finish_tx, finish_rx) = mpsc::channel();
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        let mut bytes = Vec::new();
        let request: Value = loop {
            let mut buf = [0; 4096];
            let n = stream.read(&mut buf).unwrap();
            assert!(n > 0);
            bytes.extend_from_slice(&buf[..n]);
            if let Some(end) = bytes.windows(4).position(|s| s == b"\r\n\r\n") {
                let length: usize = String::from_utf8_lossy(&bytes[..end])
                    .lines()
                    .find_map(|s| {
                        s.to_ascii_lowercase()
                            .strip_prefix("content-length:")
                            .map(|v| v.trim().parse().unwrap())
                    })
                    .unwrap();
                if bytes.len() >= end + 4 + length {
                    break serde_json::from_slice(&bytes[end + 4..end + 4 + length]).unwrap();
                }
            }
        };
        assert_eq!(request["stream"], true);
        let first = format!(
            "data: {}\r\n\r\n",
            json!({"choices":[{"delta":{"content":"{\"meaning\":\"维"},"finish_reason":null}]})
        );
        let rest = format!(
            "data: {}\n\ndata: {}\n\ndata: [DONE]\n\n",
            json!({"choices":[{"delta":{"content":"护\",\"translation\":\"服务器已迁移以进行维护。\",\"notes\":\"用于系统\\\"维护\\\"。\"}"},"finish_reason":null}]}),
            json!({"choices":[{"delta":{},"finish_reason":"stop"}]})
        );
        write!(stream, "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}", first.len()+rest.len(),first).unwrap();
        stream.flush().unwrap();
        finish_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        stream.write_all(rest.as_bytes()).unwrap();
    });
    let app = Arc::new(Application::new(
        Arc::clone(&store),
        TaskManager::new(Arc::clone(&store), Arc::new(|_| {})),
        settings,
    ));
    let input = QueryInput {
        target: "maintenance".into(),
        context: "The server has been migrated for maintenance.".into(),
        mode: "explain".into(),
        source_language: "en".into(),
        target_language: "zh-CN".into(),
        origin: QueryOrigin::default(),
    };
    let session = app.queries.open(input.clone(), None).unwrap();
    app.query_start(
        session.session_id.clone(),
        session.input_revision,
        vec!["qwen".into()],
        false,
        true,
    )
    .unwrap();
    let preview = wait(&app, &session.session_id, |s| s.results[0].value.is_some());
    assert_eq!(preview.results[0].state, "running");
    assert_eq!(preview.results[0].value.as_ref().unwrap().meaning, "维");
    assert!(
        store
            .explanation(&input.target, &input.context)
            .unwrap()
            .is_none()
    );
    assert_eq!(
        store
            .connection()
            .unwrap()
            .query_row("SELECT COUNT(*) FROM query_results", [], |r| r
                .get::<_, i64>(0))
            .unwrap(),
        0
    );
    finish_tx.send(()).unwrap();
    let completed = wait(&app, &session.session_id, |s| s.results[0].state == "ready");
    server.join().unwrap();
    let value = completed.results[0].value.as_ref().unwrap();
    assert_eq!(value.meaning, "维护");
    assert_eq!(value.translation, "服务器已迁移以进行维护。");
    assert_eq!(value.notes, "用于系统\"维护\"。");
    assert_eq!(
        store
            .explanation(&input.target, &input.context)
            .unwrap()
            .unwrap()
            .meaning,
        "维护"
    );
    let session = app.queries.open(input, None).unwrap();
    app.query_start(
        session.session_id.clone(),
        session.input_revision,
        vec!["qwen".into()],
        false,
        true,
    )
    .unwrap();
    let cached = wait(&app, &session.session_id, |s| s.results[0].state == "ready");
    assert_eq!(cached.results[0].cache_source, "cache");
    assert_eq!(cached.results[0].value.as_ref().unwrap(), value);
    assert!(store.list_entries("", 0, 40).unwrap().is_empty());
}
