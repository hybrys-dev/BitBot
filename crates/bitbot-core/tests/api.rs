//! Prova l'API locale end-to-end: listener, permessi del socket e
//! risposte JSON-RPC su una vera connessione.

use bitbot_core::{listen, serve, socket::SocketEndpoint};
use interprocess::local_socket::tokio::prelude::*;
use interprocess::local_socket::tokio::Stream;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

fn scratch_endpoint(label: &str) -> SocketEndpoint {
    let dir = std::env::temp_dir().join(format!("bitbot-core-{label}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    SocketEndpoint::in_dir(dir, "test.sock")
}

async fn roundtrip(endpoint: &SocketEndpoint, request: &str) -> String {
    let stream = Stream::connect(endpoint.name()).await.unwrap();
    let (reader, mut writer) = stream.split();
    writer.write_all(request.as_bytes()).await.unwrap();
    writer.write_all(b"\n").await.unwrap();
    writer.flush().await.unwrap();
    let mut line = String::new();
    BufReader::new(reader).read_line(&mut line).await.unwrap();
    line.trim_end().to_string()
}

#[tokio::test]
async fn serves_version_and_unknown_method_over_the_socket() {
    let _lock = test_lock().await;
    let endpoint = scratch_endpoint("api");
    let listener = listen(&endpoint).unwrap();

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(endpoint.unix_path())
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o600, "il socket deve essere del solo utente");
    }

    let server = tokio::spawn(async move { serve(listener, bitbot_core::Daemon::new()).await });
    let response = roundtrip(&endpoint, r#"{"jsonrpc":"2.0","id":1,"method":"version"}"#).await;
    assert!(response.contains(r#""name":"bitbotd""#), "{response}");
    assert!(response.contains(r#""id":1"#), "{response}");

    let response = roundtrip(&endpoint, r#"{"jsonrpc":"2.0","id":2,"method":"nope"}"#).await;
    assert!(response.contains(r#""code":-32601"#), "{response}");

    server.abort();
    let _ = server.await;
    let _ = std::fs::remove_dir_all(
        std::env::temp_dir().join(format!("bitbot-core-api-{}", std::process::id())),
    );
}

async fn test_lock() -> tokio::sync::MutexGuard<'static, ()> {
    static LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());
    LOCK.lock().await
}

#[tokio::test]
async fn session_start_rejects_a_missing_directory() {
    let _lock = test_lock().await;
    let endpoint = scratch_endpoint("missing-cwd");
    let listener = listen(&endpoint).unwrap();
    let server = tokio::spawn(async move { serve(listener, bitbot_core::Daemon::new()).await });
    let response = roundtrip(
        &endpoint,
        r#"{"jsonrpc":"2.0","id":1,"method":"session/start","params":{"agent":"opencode","cwd":"/bitbot-cartella-che-non-c-e","prompt":"x"}}"#,
    )
    .await;
    assert!(response.contains("cartella non valida"), "{response}");
    assert!(response.contains("-32602"), "{response}");
    server.abort();
    let _ = server.await;
}

/// Agente ACP finto: parla il protocollo v1 quel tanto che basta a far
/// girare `session/start` senza un agente reale.
#[cfg(unix)]
mod fake_agent {
    use std::path::{Path, PathBuf};
    use std::time::Duration;

    use bitbot_core::{listen, serve, socket::SocketEndpoint};
    use interprocess::local_socket::tokio::prelude::*;
    use interprocess::local_socket::tokio::Stream;
    use serde_json::{json, Value};
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
    use tokio::time::timeout;

    const FAKE_AGENT: &str = r#"#!/usr/bin/python3
import json, os, sys

mode = os.environ.get("BITBOT_FAKE_MODE", "quiet")
log_path = os.environ.get("BITBOT_FAKE_LOG", "")
model = "alpha"

def emit(obj):
    sys.stdout.write(json.dumps(obj, separators=(",", ":")) + "\n")
    sys.stdout.flush()

def note(text):
    if not log_path:
        return
    with open(log_path, "a", encoding="utf-8") as handle:
        handle.write(text + "\n")

def options():
    return [{
        "id": "model",
        "name": "Modello",
        "category": "model",
        "type": "select",
        "currentValue": model,
        "options": [
            {"value": "alpha", "name": "Alpha"},
            {"value": "beta", "name": "Beta"},
        ],
    }]

def read_msg():
    while True:
        line = sys.stdin.readline()
        if line == "":
            return None
        line = line.strip()
        if line:
            return json.loads(line)

while True:
    msg = read_msg()
    if msg is None:
        break
    method = msg.get("method")
    mid = msg.get("id")
    params = msg.get("params") or {}
    if method == "initialize":
        emit({"jsonrpc": "2.0", "id": mid, "result": {"protocolVersion": 1}})
    elif method == "session/new":
        emit({"jsonrpc": "2.0", "id": mid, "result": {"sessionId": "sess-1", "configOptions": options()}})
    elif method == "session/set_config_option":
        model = params.get("value", model)
        note("model=" + str(model))
        emit({"jsonrpc": "2.0", "id": mid, "result": {"configOptions": options()}})
    elif method == "session/prompt":
        sid = params.get("sessionId", "sess-1")
        emit({"jsonrpc": "2.0", "method": "session/update", "params": {
            "sessionId": sid,
            "update": {"sessionUpdate": "agent_message_chunk", "content": {"type": "text", "text": "fatto"}},
        }})
        if mode == "permission":
            emit({"jsonrpc": "2.0", "id": 50, "method": "session/request_permission", "params": {
                "sessionId": sid,
                "toolCall": {"toolCallId": "call-1", "kind": "execute", "title": "echo hi"},
                "options": [
                    {"optionId": "once", "name": "Allow once", "kind": "allow_once"},
                    {"optionId": "always", "name": "Always allow", "kind": "allow_always"},
                    {"optionId": "reject", "name": "Reject", "kind": "reject_once"},
                ],
            }})
            reply = read_msg() or {}
            note("permission=" + json.dumps(reply.get("result"), separators=(",", ":")))
        elif mode == "question":
            emit({"jsonrpc": "2.0", "id": 51, "method": "cursor/ask_question", "params": {
                "toolCallId": "call-q",
                "title": "Need input",
                "questions": [{
                    "id": "q1",
                    "prompt": "Which?",
                    "options": [{"id": "a", "label": "Alpha"}, {"id": "b", "label": "Beta"}],
                    "allowMultiple": False,
                }],
            }})
            reply = read_msg() or {}
            note("question=" + json.dumps(reply.get("result"), separators=(",", ":")))
        elif mode == "plan":
            emit({"jsonrpc": "2.0", "id": 52, "method": "cursor/create_plan", "params": {
                "toolCallId": "call-p",
                "name": "Piccolo",
                "overview": "Fallo",
                "plan": "1. Uno",
                "todos": [{"id": "t1", "content": "Uno", "status": "pending"}],
            }})
            reply = read_msg() or {}
            note("plan=" + json.dumps(reply.get("result"), separators=(",", ":")))
        emit({"jsonrpc": "2.0", "id": mid, "result": {"stopReason": "end_turn"}})
"#;

    struct EnvVar {
        key: &'static str,
        previous: Option<std::ffi::OsString>,
    }

    impl EnvVar {
        fn set(key: &'static str, value: impl AsRef<std::ffi::OsStr>) -> Self {
            let previous = std::env::var_os(key);
            // SAFETY: i test di questo file prendono `test_lock` prima di
            // mutare l'ambiente, così un solo task chiama `set_var`. Il
            // demone risolve il profilo dal PATH del processo.
            unsafe { std::env::set_var(key, value) };
            Self { key, previous }
        }
    }

    impl Drop for EnvVar {
        fn drop(&mut self) {
            // SAFETY: stesso lock dei test, vedi `EnvVar::set`.
            match &self.previous {
                Some(value) => unsafe { std::env::set_var(self.key, value) },
                None => unsafe { std::env::remove_var(self.key) },
            }
        }
    }

    struct Fake {
        _dir: PathBuf,
        log: PathBuf,
        _env: Vec<EnvVar>,
    }

    impl Fake {
        fn install(label: &str, mode: &str) -> Self {
            let dir =
                std::env::temp_dir().join(format!("bitbot-fake-{label}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).unwrap();
            for name in ["opencode", "cursor-agent"] {
                let path = dir.join(name);
                std::fs::write(&path, FAKE_AGENT).unwrap();
                use std::os::unix::fs::PermissionsExt;
                let mut permissions = std::fs::metadata(&path).unwrap().permissions();
                permissions.set_mode(0o755);
                std::fs::set_permissions(&path, permissions).unwrap();
            }
            let log = dir.join("log.txt");
            let path = {
                let current = std::env::var_os("PATH");
                let mut joined = dir.as_os_str().to_owned();
                if let Some(current) = current {
                    joined.push(":");
                    joined.push(current);
                }
                joined
            };
            let env = vec![
                EnvVar::set("PATH", path),
                EnvVar::set("BITBOT_FAKE_MODE", mode),
                EnvVar::set("BITBOT_FAKE_LOG", &log),
            ];
            Self {
                _dir: dir,
                log,
                _env: env,
            }
        }
    }

    struct Client {
        writer: tokio::io::WriteHalf<Stream>,
        lines: tokio::io::Lines<BufReader<tokio::io::ReadHalf<Stream>>>,
    }

    impl Client {
        async fn connect(endpoint: &SocketEndpoint) -> Self {
            let stream = Stream::connect(endpoint.name()).await.unwrap();
            let (reader, writer) = tokio::io::split(stream);
            Self {
                writer,
                lines: BufReader::new(reader).lines(),
            }
        }

        async fn call(&mut self, id: i64, method: &str, params: Value) -> Value {
            let request = json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params});
            self.send(&request.to_string()).await;
            loop {
                let message = self.next().await;
                if message.get("id").is_some() && message["id"] == id {
                    return message;
                }
            }
        }

        async fn next(&mut self) -> Value {
            let line = timeout(Duration::from_secs(5), self.lines.next_line())
                .await
                .expect("tempo scaduto in attesa di una riga")
                .unwrap()
                .expect("connessione chiusa");
            serde_json::from_str(&line).unwrap_or_else(|error| panic!("{error}: {line}"))
        }

        async fn send(&mut self, line: &str) {
            self.writer.write_all(line.as_bytes()).await.unwrap();
            self.writer.write_all(b"\n").await.unwrap();
            self.writer.flush().await.unwrap();
        }
    }

    fn socket_at(label: &str) -> SocketEndpoint {
        let dir = std::env::temp_dir().join(format!("bitbot-core-{label}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        SocketEndpoint::in_dir(dir, "test.sock")
    }

    async fn until_state(client: &mut Client, state: &str) -> Value {
        let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
        loop {
            if tokio::time::Instant::now() > deadline {
                panic!("stato {state} non arrivato");
            }
            let message = client.next().await;
            if message["method"] == "session/event" && message["params"]["status"]["state"] == state
            {
                return message;
            }
        }
    }

    async fn log_contains(log: &Path, needle: &str) {
        let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
        loop {
            if let Ok(text) = std::fs::read_to_string(log) {
                if text.contains(needle) {
                    return;
                }
            }
            if tokio::time::Instant::now() > deadline {
                let text = std::fs::read_to_string(log).unwrap_or_default();
                panic!("manca {needle} nel log: {text}");
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    }

    #[tokio::test]
    async fn streams_status_until_the_turn_finishes_and_sets_the_model() {
        let _lock = super::test_lock().await;
        let fake = Fake::install("quiet", "quiet");
        let endpoint = socket_at("quiet");
        let listener = listen(&endpoint).unwrap();
        let server = tokio::spawn(async move { serve(listener, bitbot_core::Daemon::new()).await });
        let mut client = Client::connect(&endpoint).await;
        let started = client
            .call(
                1,
                "session/start",
                json!({"agent": "opencode", "cwd": ".", "prompt": "aggiungi un test", "model": "beta"}),
            )
            .await;
        assert_eq!(started["result"]["id"], "s-1", "{started}");
        let finished = until_state(&mut client, "finished").await;
        assert!(
            finished["params"]["line"]
                .as_str()
                .unwrap()
                .contains("[opencode] finito"),
            "{finished}"
        );
        log_contains(&fake.log, "model=beta").await;
        server.abort();
        let _ = server.await;
    }

    #[tokio::test]
    async fn permission_can_be_rejected_then_allowed() {
        let _lock = super::test_lock().await;
        let fake = Fake::install("permission", "permission");
        let endpoint = socket_at("permission");
        let listener = listen(&endpoint).unwrap();
        let server = tokio::spawn(async move { serve(listener, bitbot_core::Daemon::new()).await });
        let mut client = Client::connect(&endpoint).await;
        let started = client
            .call(
                1,
                "session/start",
                json!({"agent": "opencode", "cwd": ".", "prompt": "esegui echo"}),
            )
            .await;
        let session = started["result"]["id"].as_str().unwrap().to_string();
        let request = loop {
            let message = client.next().await;
            if message["method"] == "session/request" {
                break message;
            }
        };
        assert_eq!(request["params"]["body"]["kind"], "permission", "{request}");
        let request_id = request["params"]["requestId"].as_str().unwrap().to_string();
        let rejected = client
            .call(
                2,
                "session/respond",
                json!({"id": session, "requestId": request_id, "outcome": "accepted"}),
            )
            .await;
        assert!(
            rejected["error"]["message"]
                .as_str()
                .unwrap()
                .contains("permesso"),
            "{rejected}"
        );
        let accepted = client
            .call(
                3,
                "session/respond",
                json!({"id": session, "requestId": request_id, "outcome": "allow_once"}),
            )
            .await;
        assert!(accepted.get("error").is_none(), "{accepted}");
        assert!(accepted["result"].is_null(), "{accepted}");
        let _ = until_state(&mut client, "finished").await;
        log_contains(&fake.log, r#""optionId":"once""#).await;
        server.abort();
        let _ = server.await;
    }

    #[tokio::test]
    async fn disconnect_cancels_a_pending_permission() {
        let _lock = super::test_lock().await;
        let fake = Fake::install("cancel", "permission");
        let endpoint = socket_at("cancel");
        let listener = listen(&endpoint).unwrap();
        let server = tokio::spawn(async move { serve(listener, bitbot_core::Daemon::new()).await });
        let mut client = Client::connect(&endpoint).await;
        let _ = client
            .call(
                1,
                "session/start",
                json!({"agent": "opencode", "cwd": ".", "prompt": "esegui echo"}),
            )
            .await;
        loop {
            let message = client.next().await;
            if message["method"] == "session/request" {
                break;
            }
        }
        drop(client);
        log_contains(&fake.log, r#""outcome":"cancelled""#).await;
        server.abort();
        let _ = server.await;
    }

    #[tokio::test]
    async fn question_and_plan_round_trip() {
        let _lock = super::test_lock().await;
        let fake = Fake::install("cursor", "question");
        let endpoint = socket_at("question");
        let listener = listen(&endpoint).unwrap();
        let server = tokio::spawn(async move { serve(listener, bitbot_core::Daemon::new()).await });
        let mut client = Client::connect(&endpoint).await;
        let started = client
            .call(
                1,
                "session/start",
                json!({"agent": "cursor", "cwd": ".", "prompt": "chiedi"}),
            )
            .await;
        let session = started["result"]["id"].as_str().unwrap().to_string();
        let request = loop {
            let message = client.next().await;
            if message["method"] == "session/request" {
                break message;
            }
        };
        assert_eq!(request["params"]["body"]["kind"], "question", "{request}");
        let request_id = request["params"]["requestId"].as_str().unwrap().to_string();
        let answered = client
            .call(
                2,
                "session/respond",
                json!({
                    "id": session,
                    "requestId": request_id,
                    "outcome": "answered",
                    "answers": [{"questionId": "q1", "selectedOptionIds": ["b"]}]
                }),
            )
            .await;
        assert!(answered.get("error").is_none(), "{answered}");
        let _ = until_state(&mut client, "finished").await;
        log_contains(&fake.log, r#""selectedOptionIds":["b"]"#).await;
        server.abort();
        let _ = server.await;

        let fake = Fake::install("plan", "plan");
        let endpoint = socket_at("plan");
        let listener = listen(&endpoint).unwrap();
        let server = tokio::spawn(async move { serve(listener, bitbot_core::Daemon::new()).await });
        let mut client = Client::connect(&endpoint).await;
        let started = client
            .call(
                1,
                "session/start",
                json!({"agent": "cursor", "cwd": ".", "prompt": "pianifica"}),
            )
            .await;
        let session = started["result"]["id"].as_str().unwrap().to_string();
        let request = loop {
            let message = client.next().await;
            if message["method"] == "session/request" {
                break message;
            }
        };
        assert_eq!(request["params"]["body"]["kind"], "plan", "{request}");
        assert_eq!(request["params"]["body"]["name"], "Piccolo");
        let request_id = request["params"]["requestId"].as_str().unwrap().to_string();
        let accepted = client
            .call(
                2,
                "session/respond",
                json!({"id": session, "requestId": request_id, "outcome": "accepted"}),
            )
            .await;
        assert!(accepted.get("error").is_none(), "{accepted}");
        let _ = until_state(&mut client, "finished").await;
        log_contains(&fake.log, r#""outcome":"accepted""#).await;
        server.abort();
        let _ = server.await;
    }
}
