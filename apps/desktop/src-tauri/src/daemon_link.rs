//! Client del demone: legge il socket, non lo apre e non avvia processi.
//!
//! `session/request` arriva e viene ignorata. Rispondere è il passo delle
//! schede: finché questo processo resta collegato, una richiesta in attesa
//! non viene annullata dal demone.

use std::io;
use std::time::Duration;

use bitbot_core::socket::SocketEndpoint;
use interprocess::local_socket::tokio::prelude::*;
use interprocess::local_socket::tokio::Stream;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::sync::oneshot;

use crate::pill::{classify_line, DaemonLine, PillBoard, REST_LABEL};

/// Richiesta `version`: conferma che il peer è `bitbotd` e che il demone
/// ci ha già iscritto agli eventi, prima di considerare viva la connessione.
const VERSION_REQUEST: &[u8] = br#"{"jsonrpc":"2.0","id":1,"method":"version"}"#;

/// Pausa fra un tentativo e l'altro se il demone non è in ascolto.
const RECONNECT_INTERVAL: Duration = Duration::from_secs(1);

/// Si collega all'endpoint e riprova per sempre. Non blocca chi la avvia.
pub(crate) async fn watch(endpoint: SocketEndpoint, mut on_label: impl FnMut(&str)) {
    let mut shown = String::from(REST_LABEL);
    let mut failure_logged = false;
    loop {
        let outcome = follow(&endpoint, |label| {
            if shown != label {
                shown = label.to_string();
                on_label(label);
            }
        })
        .await;
        if shown != REST_LABEL {
            shown = REST_LABEL.to_string();
            on_label(REST_LABEL);
        }
        match outcome {
            Ok(()) => {
                failure_logged = false;
                eprintln!("bitbot-desktop: connessione col demone chiusa, riprovo");
            }
            Err(error) => {
                if !failure_logged {
                    eprintln!("bitbot-desktop: demone non raggiungibile ({error}), riprovo");
                    failure_logged = true;
                }
            }
        }
        tokio::time::sleep(RECONNECT_INTERVAL).await;
    }
}

pub(crate) async fn follow(
    endpoint: &SocketEndpoint,
    on_label: impl FnMut(&str),
) -> io::Result<()> {
    drive(endpoint, on_label, None).await
}

async fn drive(
    endpoint: &SocketEndpoint,
    mut on_label: impl FnMut(&str),
    mut ready: Option<oneshot::Sender<()>>,
) -> io::Result<()> {
    let stream = Stream::connect(endpoint.name()).await?;
    let (reader, mut writer) = stream.split();
    writer.write_all(VERSION_REQUEST).await?;
    writer.write_all(b"\n").await?;
    writer.flush().await?;
    // Resta in scope fino al ritorno: chiuderla chiude il socket e il demone
    // ci toglie dagli iscritti. `let _ = writer` la dropperebbe subito.
    let _writer = writer;

    let mut lines = BufReader::new(reader).lines();
    let mut board = PillBoard::new();
    let mut ready_seen = false;
    while let Some(line) = lines.next_line().await? {
        match classify_line(&line) {
            DaemonLine::Version => {
                ready_seen = true;
                if let Some(tx) = ready.take() {
                    let _ = tx.send(());
                }
            }
            DaemonLine::UnexpectedVersion => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "risposta version inattesa",
                ));
            }
            DaemonLine::Event(event) => {
                let label = board.apply(&event);
                on_label(&label);
            }
            DaemonLine::Ignore => {}
        }
    }
    if !ready_seen {
        return Err(io::Error::new(
            io::ErrorKind::UnexpectedEof,
            "il demone ha chiuso prima di version",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::time::Duration;

    use bitbot_core::socket::SocketEndpoint;
    use bitbot_protocol::{Action, AgentState, SessionEventParams, Status, SESSION_EVENT_METHOD};
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
    #[cfg(unix)]
    use tokio::sync::oneshot;

    use super::*;

    struct Scratch {
        dir: PathBuf,
        endpoint: SocketEndpoint,
    }

    impl Scratch {
        fn new(label: &str) -> Self {
            let dir =
                std::env::temp_dir().join(format!("bitbot-desktop-{label}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).unwrap();
            let endpoint = SocketEndpoint::in_dir(dir.clone(), "test.sock");
            Self { dir, endpoint }
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.dir);
        }
    }

    #[tokio::test]
    async fn follow_reads_a_session_event_and_ignores_a_request() {
        let scratch = Scratch::new("evento");
        let listener = bitbot_core::listen(&scratch.endpoint).unwrap();
        let status = Status::new("opencode", AgentState::Working).with_action(Action::edit(
            "src/lib.rs",
            8,
            2,
        ));
        let params = SessionEventParams::from_status("s-1", status.clone());
        let event_line = serde_json::json!({
            "jsonrpc": "2.0",
            "method": SESSION_EVENT_METHOD,
            "params": params,
        })
        .to_string();
        let server = tokio::spawn(async move {
            let stream = listener.accept().await.unwrap();
            let (reader, mut writer) = stream.split();
            let mut lines = BufReader::new(reader).lines();
            let request = lines.next_line().await.unwrap().unwrap();
            assert!(request.contains(r#""method":"version""#), "{request}");
            let extra = tokio::time::timeout(Duration::from_millis(150), lines.next_line()).await;
            assert!(
                extra.is_err(),
                "il client non risponde alle richieste: {extra:?}"
            );
            let version =
                r#"{"jsonrpc":"2.0","id":1,"result":{"name":"bitbotd","version":"0.1.0"}}"#;
            let noise = r#"{"jsonrpc":"2.0","method":"session/request","params":{"id":"s-1","requestId":"r-1","body":{"kind":"permission","options":[]}}}"#;
            for chunk in [version, noise, event_line.as_str()] {
                writer.write_all(chunk.as_bytes()).await.unwrap();
                writer.write_all(b"\n").await.unwrap();
            }
            writer.flush().await.unwrap();
        });

        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        let endpoint = scratch.endpoint.clone();
        let client = tokio::spawn(async move {
            follow(&endpoint, move |label| {
                let _ = tx.send(label.to_string());
            })
            .await
        });

        let label = tokio::time::timeout(Duration::from_secs(5), rx.recv())
            .await
            .expect("tempo scaduto")
            .expect("etichetta");
        assert_eq!(label, status.line());
        let joined = client.await.unwrap();
        assert!(joined.is_ok(), "{joined:?}");
        server.await.unwrap();
    }

    /// Agente finto minimo: un turno che finisce, senza permesso.
    #[cfg(unix)]
    #[tokio::test]
    async fn follow_shows_a_fake_agent_turn() {
        let _lock = test_lock().await;
        let fake = install_fake("quiet");
        let scratch = Scratch::new("finto");
        let listener = bitbot_core::listen(&scratch.endpoint).unwrap();
        let server = tokio::spawn(async move {
            let _ = bitbot_core::serve(listener, bitbot_core::Daemon::new()).await;
        });

        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        let (ready_tx, ready_rx) = oneshot::channel();
        let endpoint = scratch.endpoint.clone();
        let client = tokio::spawn(async move {
            drive(
                &endpoint,
                move |label| {
                    let _ = tx.send(label.to_string());
                },
                Some(ready_tx),
            )
            .await
        });
        ready_rx.await.expect("handshake col demone");

        let cwd = scratch.dir.to_string_lossy().to_string();
        let request = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 7,
            "method": "session/start",
            "params": {"agent": "opencode", "cwd": cwd, "prompt": "aggiungi un test"}
        });
        let mut starter = connect_and_send(&scratch.endpoint, &request.to_string()).await;
        let started = starter.next_line().await;
        assert!(
            started.contains("\"result\"") && !started.contains("\"error\""),
            "{started}"
        );

        let mut saw_working = false;
        let deadline = tokio::time::Instant::now() + Duration::from_secs(20);
        loop {
            if tokio::time::Instant::now() > deadline {
                panic!("turno non concluso");
            }
            let label = tokio::time::timeout(Duration::from_secs(20), rx.recv())
                .await
                .expect("tempo scaduto in attesa di uno stato")
                .expect("connessione chiusa");
            if label.contains("errore") {
                panic!("sessione in errore: {label}");
            }
            if label.contains("al lavoro") {
                saw_working = true;
            }
            if label.contains("finito") {
                assert!(saw_working, "manca lo stato al lavoro: {label}");
                break;
            }
        }

        drop(starter);
        client.abort();
        server.abort();
        drop(fake);
    }

    #[cfg(unix)]
    struct Held {
        // Tiene il socket aperto: il demone disiscrive chi chiude la scrittura.
        _writer: interprocess::local_socket::tokio::SendHalf,
        lines: tokio::io::Lines<BufReader<interprocess::local_socket::tokio::RecvHalf>>,
    }

    #[cfg(unix)]
    impl Held {
        async fn next_line(&mut self) -> String {
            tokio::time::timeout(Duration::from_secs(20), self.lines.next_line())
                .await
                .expect("tempo scaduto")
                .unwrap()
                .expect("connessione chiusa")
        }
    }

    #[cfg(unix)]
    async fn connect_and_send(endpoint: &SocketEndpoint, body: &str) -> Held {
        let stream = Stream::connect(endpoint.name()).await.unwrap();
        let (reader, mut writer) = stream.split();
        writer.write_all(body.as_bytes()).await.unwrap();
        writer.write_all(b"\n").await.unwrap();
        writer.flush().await.unwrap();
        Held {
            _writer: writer,
            lines: BufReader::new(reader).lines(),
        }
    }

    #[cfg(unix)]
    async fn test_lock() -> tokio::sync::MutexGuard<'static, ()> {
        static LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());
        LOCK.lock().await
    }

    #[cfg(unix)]
    struct Fake {
        _dir: PathBuf,
        _path: EnvVar,
    }

    #[cfg(unix)]
    struct EnvVar {
        key: &'static str,
        previous: Option<std::ffi::OsString>,
    }

    #[cfg(unix)]
    impl EnvVar {
        fn set(key: &'static str, value: impl AsRef<std::ffi::OsStr>) -> Self {
            let previous = std::env::var_os(key);
            // SAFETY: il test tiene `test_lock` per tutta la corsa, così
            // un solo task muta l'ambiente mentre il demone legge `PATH`.
            unsafe { std::env::set_var(key, value) };
            Self { key, previous }
        }
    }

    #[cfg(unix)]
    impl Drop for EnvVar {
        fn drop(&mut self) {
            // SAFETY: stesso lock di `EnvVar::set`.
            match &self.previous {
                Some(value) => unsafe { std::env::set_var(self.key, value) },
                None => unsafe { std::env::remove_var(self.key) },
            }
        }
    }

    #[cfg(unix)]
    impl Drop for Fake {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self._dir);
        }
    }

    #[cfg(unix)]
    fn install_fake(label: &str) -> Fake {
        let dir = std::env::temp_dir().join(format!(
            "bitbot-desktop-fake-{label}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("opencode");
        std::fs::write(&path, FAKE_AGENT).unwrap();
        use std::os::unix::fs::PermissionsExt;
        let mut permissions = std::fs::metadata(&path).unwrap().permissions();
        permissions.set_mode(0o755);
        std::fs::set_permissions(&path, permissions).unwrap();
        let mut joined = dir.as_os_str().to_owned();
        if let Some(current) = std::env::var_os("PATH") {
            joined.push(":");
            joined.push(current);
        }
        Fake {
            _dir: dir,
            _path: EnvVar::set("PATH", joined),
        }
    }

    #[cfg(unix)]
    const FAKE_AGENT: &str = r#"#!/usr/bin/python3
import json, sys

def emit(obj):
    sys.stdout.write(json.dumps(obj, separators=(",", ":")) + "\n")
    sys.stdout.flush()

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
        emit({"jsonrpc": "2.0", "id": mid, "result": {
            "sessionId": "sess-1",
            "configOptions": [{
                "id": "model",
                "name": "Modello",
                "category": "model",
                "type": "select",
                "currentValue": "alpha",
                "options": [{"value": "alpha", "name": "Alpha"}],
            }],
        }})
    elif method == "session/prompt":
        sid = params.get("sessionId", "sess-1")
        emit({"jsonrpc": "2.0", "method": "session/update", "params": {
            "sessionId": sid,
            "update": {"sessionUpdate": "agent_message_chunk", "content": {"type": "text", "text": "fatto"}},
        }})
        emit({"jsonrpc": "2.0", "id": mid, "result": {"stopReason": "end_turn"}})
    elif mid is not None:
        emit({"jsonrpc": "2.0", "id": mid, "error": {"code": -32601, "message": method or ""}})
"#;
}
