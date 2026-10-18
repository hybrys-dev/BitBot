//! Sessioni ACP in memoria e dispaccio dei metodi del demone.
//!
//! Una sessione vive quanto il processo: se `bitbotd` cade, cade anche
//! lei. Non c'è ancora un indice né una ripresa al riavvio.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use bitbot_drivers::{
    run_new_session, AskNotice, HostEvent, NewSession, ReplyEnvelope, SessionHost, SessionIngress,
};
use bitbot_protocol::{
    DaemonVersion, SessionEventParams, SessionReply, SessionRequestParams, SessionRespondParams,
    SessionStartParams, SessionStartResult, SESSION_EVENT_METHOD, SESSION_REQUEST_METHOD,
    SESSION_RESPOND_METHOD, SESSION_START_METHOD, VERSION_METHOD,
};
use serde_json::Value;
use tokio::sync::{mpsc, oneshot};

use crate::rpc::{self, Incoming};

/// Cosa fare di una riga letta sul socket.
pub(crate) enum LineOutcome {
    Empty,
    Message(String),
    /// La risposta va scritta prima di avviare l'agente, così il client
    /// riceve l'id e poi gli eventi, nello stesso ordine.
    Start {
        message: String,
        launch: Launch,
    },
}

pub(crate) struct Launch {
    id: String,
    spec: NewSession,
}

struct Subscriber {
    id: u64,
    tx: mpsc::UnboundedSender<String>,
}

struct SessionSlot {
    pending: HashMap<String, mpsc::UnboundedSender<ReplyEnvelope>>,
}

struct Inner {
    next_session: AtomicU64,
    next_subscriber: AtomicU64,
    subscribers: Mutex<Vec<Subscriber>>,
    sessions: Mutex<HashMap<String, SessionSlot>>,
}

/// Stato del demone condiviso fra le connessioni.
#[derive(Clone)]
pub struct Daemon {
    inner: Arc<Inner>,
}

impl Default for Daemon {
    fn default() -> Self {
        Self::new()
    }
}

impl Daemon {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Inner {
                next_session: AtomicU64::new(1),
                next_subscriber: AtomicU64::new(1),
                subscribers: Mutex::new(Vec::new()),
                sessions: Mutex::new(HashMap::new()),
            }),
        }
    }

    pub(crate) fn subscribe(&self, tx: mpsc::UnboundedSender<String>) -> u64 {
        let id = self.inner.next_subscriber.fetch_add(1, Ordering::Relaxed);
        self.subscribers().push(Subscriber { id, tx });
        id
    }

    /// Se non resta nessuno in ascolto, le richieste aperte si annullano:
    /// l'agente non resta fermo ad aspettare una UI che non c'è.
    pub(crate) fn unsubscribe(&self, id: u64) {
        self.subscribers().retain(|subscriber| subscriber.id != id);
        if self.has_subscribers() {
            return;
        }
        for session in self.sessions().values_mut() {
            session.pending.clear();
        }
    }

    pub(crate) async fn handle(&self, line: &str) -> LineOutcome {
        match rpc::parse(line) {
            Incoming::Silent => LineOutcome::Empty,
            Incoming::Respond(line) => LineOutcome::Message(line),
            Incoming::Call { id, method, params } => self.dispatch(id, &method, params).await,
        }
    }

    pub(crate) fn launch(&self, launch: Launch) {
        let (host, ingress) = SessionHost::channel();
        self.sessions().insert(
            launch.id.clone(),
            SessionSlot {
                pending: HashMap::new(),
            },
        );
        let daemon = self.clone();
        tokio::spawn(async move {
            let id = launch.id.clone();
            let forwarder = daemon.clone();
            let session_id = id.clone();
            let forward = tokio::spawn(async move {
                forwarder.forward(session_id, ingress).await;
            });
            let _ = run_new_session(launch.spec, host).await;
            let _ = forward.await;
            daemon.sessions().remove(&id);
        });
    }

    async fn dispatch(&self, id: Value, method: &str, params: Value) -> LineOutcome {
        match method {
            VERSION_METHOD => LineOutcome::Message(rpc::result_response(
                id,
                serde_json::to_value(DaemonVersion {
                    name: "bitbotd".to_string(),
                    version: env!("CARGO_PKG_VERSION").to_string(),
                })
                .unwrap_or(Value::Null),
            )),
            SESSION_START_METHOD => self.start(id, params),
            SESSION_RESPOND_METHOD => self.respond(id, params).await,
            _ => LineOutcome::Message(rpc::error_response(
                id,
                rpc::METHOD_NOT_FOUND,
                &format!("method not found: {method}"),
            )),
        }
    }

    fn start(&self, id: Value, params: Value) -> LineOutcome {
        let spec = match prepare_session(params) {
            Ok(spec) => spec,
            Err(message) => {
                return LineOutcome::Message(rpc::error_response(
                    id,
                    rpc::INVALID_PARAMS,
                    &message,
                ));
            }
        };
        let session_id = format!(
            "s-{}",
            self.inner.next_session.fetch_add(1, Ordering::Relaxed)
        );
        LineOutcome::Start {
            message: rpc::result_response(
                id,
                serde_json::to_value(SessionStartResult {
                    id: session_id.clone(),
                })
                .unwrap_or(Value::Null),
            ),
            launch: Launch {
                id: session_id,
                spec,
            },
        }
    }

    async fn respond(&self, id: Value, params: Value) -> LineOutcome {
        let params = match serde_json::from_value::<SessionRespondParams>(params) {
            Ok(params) => params,
            Err(error) => {
                return LineOutcome::Message(rpc::error_response(
                    id,
                    rpc::INVALID_PARAMS,
                    &error.to_string(),
                ));
            }
        };
        let reply = SessionReply {
            outcome: params.outcome,
            answers: params.answers,
        };
        let sender = {
            let sessions = self.sessions();
            let Some(session) = sessions.get(&params.id) else {
                return LineOutcome::Message(rpc::error_response(
                    id,
                    rpc::INVALID_PARAMS,
                    "sessione sconosciuta",
                ));
            };
            let Some(sender) = session.pending.get(&params.request_id).cloned() else {
                return LineOutcome::Message(rpc::error_response(
                    id,
                    rpc::INVALID_PARAMS,
                    "nessuna richiesta in attesa",
                ));
            };
            sender
        };
        let (ack_tx, ack_rx) = oneshot::channel();
        if sender.send(ReplyEnvelope { reply, ack: ack_tx }).is_err() {
            self.clear_pending(&params.id, &params.request_id);
            return LineOutcome::Message(rpc::error_response(
                id,
                rpc::INVALID_PARAMS,
                "la sessione non aspetta più",
            ));
        }
        match ack_rx.await {
            Ok(Ok(())) => {
                self.clear_pending(&params.id, &params.request_id);
                LineOutcome::Message(rpc::result_response(id, Value::Null))
            }
            Ok(Err(message)) => {
                LineOutcome::Message(rpc::error_response(id, rpc::INVALID_PARAMS, &message))
            }
            Err(_) => {
                self.clear_pending(&params.id, &params.request_id);
                LineOutcome::Message(rpc::error_response(
                    id,
                    rpc::INVALID_PARAMS,
                    "la sessione non aspetta più",
                ))
            }
        }
    }

    async fn forward(&self, id: String, mut ingress: SessionIngress) {
        let mut request_seq = 0u64;
        while let Some(event) = ingress.events.recv().await {
            match event {
                HostEvent::Status(status) => {
                    let params = SessionEventParams::from_status(&id, status);
                    self.broadcast(&rpc::notification(
                        SESSION_EVENT_METHOD,
                        serde_json::to_value(&params).unwrap_or(Value::Null),
                    ));
                }
                HostEvent::Ask(ask) => {
                    if !self.has_subscribers() {
                        drop(ask);
                        continue;
                    }
                    request_seq += 1;
                    self.publish_ask(&id, request_seq, ask);
                }
            }
        }
        if let Some(session) = self.sessions().get_mut(&id) {
            session.pending.clear();
        }
    }

    fn publish_ask(&self, id: &str, request_seq: u64, ask: AskNotice) {
        let request_id = format!("r-{request_seq}");
        let params = SessionRequestParams {
            id: id.to_string(),
            request_id: request_id.clone(),
            body: ask.body,
        };
        let stored = self.sessions().get_mut(id).is_some_and(|session| {
            session.pending.insert(request_id, ask.reply_tx);
            true
        });
        if stored {
            self.broadcast(&rpc::notification(
                SESSION_REQUEST_METHOD,
                serde_json::to_value(&params).unwrap_or(Value::Null),
            ));
        }
    }

    fn clear_pending(&self, id: &str, request_id: &str) {
        if let Some(session) = self.sessions().get_mut(id) {
            session.pending.remove(request_id);
        }
    }

    fn broadcast(&self, line: &str) {
        self.subscribers()
            .retain(|subscriber| subscriber.tx.send(line.to_string()).is_ok());
    }

    fn has_subscribers(&self) -> bool {
        self.subscribers()
            .iter()
            .any(|subscriber| !subscriber.tx.is_closed())
    }

    fn subscribers(&self) -> std::sync::MutexGuard<'_, Vec<Subscriber>> {
        self.inner
            .subscribers
            .lock()
            .unwrap_or_else(|error| error.into_inner())
    }

    fn sessions(&self) -> std::sync::MutexGuard<'_, HashMap<String, SessionSlot>> {
        self.inner
            .sessions
            .lock()
            .unwrap_or_else(|error| error.into_inner())
    }
}

fn prepare_session(params: Value) -> Result<NewSession, String> {
    let params =
        serde_json::from_value::<SessionStartParams>(params).map_err(|error| error.to_string())?;
    let prompt = params.prompt.trim();
    if prompt.is_empty() {
        return Err("il prompt è vuoto".to_string());
    }
    bitbot_drivers::AgentProfile::resolve(&params.agent).map_err(|error| error.to_string())?;
    let cwd = bitbot_drivers::prepare_cwd(&params.cwd).map_err(|error| error.to_string())?;
    Ok(NewSession {
        agent: params.agent,
        cwd,
        prompt: prompt.to_string(),
        model: params.model,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn version_returns_daemon_identity() {
        let daemon = Daemon::new();
        let LineOutcome::Message(response) = daemon
            .handle(r#"{"jsonrpc":"2.0","id":1,"method":"version"}"#)
            .await
        else {
            panic!("version risponde");
        };
        let value: Value = serde_json::from_str(&response).unwrap();
        assert_eq!(value["id"], 1);
        assert_eq!(value["result"]["name"], "bitbotd");
        assert_eq!(value["result"]["version"], env!("CARGO_PKG_VERSION"));
    }

    #[tokio::test]
    async fn unknown_method_is_reported_with_its_name() {
        let daemon = Daemon::new();
        let LineOutcome::Message(response) = daemon
            .handle(r#"{"jsonrpc":"2.0","id":7,"method":"nope"}"#)
            .await
        else {
            panic!("metodo sconosciuto");
        };
        assert!(response.contains(r#""code":-32601"#));
        assert!(response.contains("method not found: nope"));
    }

    #[tokio::test]
    async fn notifications_and_garbage() {
        let daemon = Daemon::new();
        assert!(matches!(
            daemon
                .handle(r#"{"jsonrpc":"2.0","method":"version"}"#)
                .await,
            LineOutcome::Empty
        ));
        let LineOutcome::Message(parse_error) = daemon
            .handle(r#"{"jsonrpc":"2.0","id":3,"method":"nope"},"#)
            .await
        else {
            panic!("parse error");
        };
        let value: Value = serde_json::from_str(&parse_error).unwrap();
        assert_eq!(value["id"], Value::Null);
        assert_eq!(value["error"]["code"], rpc::PARSE_ERROR);
        for garbage in [
            r#"[{"jsonrpc":"2.0","id":1,"method":"version"}]"#,
            r#"{"jsonrpc":"1.0","id":2,"method":"version"}"#,
            r#"{"id":4,"method":"version"}"#,
        ] {
            let LineOutcome::Message(response) = daemon.handle(garbage).await else {
                panic!("{garbage}");
            };
            let value: Value = serde_json::from_str(&response).unwrap();
            assert_eq!(value["error"]["code"], rpc::INVALID_REQUEST, "{garbage}");
        }
        let LineOutcome::Message(response) = daemon
            .handle(r#"{"jsonrpc":"1.0","id":2,"method":"version"}"#)
            .await
        else {
            panic!("id conservato");
        };
        let value: Value = serde_json::from_str(&response).unwrap();
        assert_eq!(value["id"], 2);
    }

    #[tokio::test]
    async fn session_start_rejects_unknown_agent_and_empty_prompt() {
        let daemon = Daemon::new();
        let LineOutcome::Message(response) = daemon
            .handle(
                r#"{"jsonrpc":"2.0","id":1,"method":"session/start","params":{"agent":"nessuno","cwd":".","prompt":"x"}}"#,
            )
            .await
        else {
            panic!("profilo");
        };
        assert!(response.contains("profilo sconosciuto"), "{response}");
        assert!(response.contains("-32602"), "{response}");

        let LineOutcome::Message(response) = daemon
            .handle(
                r#"{"jsonrpc":"2.0","id":2,"method":"session/start","params":{"agent":"opencode","cwd":".","prompt":"  "}}"#,
            )
            .await
        else {
            panic!("prompt");
        };
        assert!(response.contains("il prompt è vuoto"), "{response}");
    }

    #[tokio::test]
    async fn session_respond_without_a_session_is_invalid_params() {
        let daemon = Daemon::new();
        let LineOutcome::Message(response) = daemon
            .handle(
                r#"{"jsonrpc":"2.0","id":3,"method":"session/respond","params":{"id":"s-9","requestId":"r-1","outcome":"cancelled"}}"#,
            )
            .await
        else {
            panic!("respond");
        };
        assert!(response.contains("sessione sconosciuta"), "{response}");
    }
}
