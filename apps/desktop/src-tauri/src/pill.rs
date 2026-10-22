//! Riga della pillola, scelta dagli stati già tradotti dal demone.
//!
//! Chi aspetta l'utente viene prima di chi lavora; altrimenti resta
//! l'aggiornamento più recente. Una sessione archiviata non compare.

use bitbot_protocol::{AgentState, SessionEventParams, SESSION_EVENT_METHOD};
use serde_json::Value;

/// Testo della pillola quando non c'è una sessione da mostrare.
pub(crate) const REST_LABEL: &str = "BitBot";

/// Evento Tauri con la riga corrente. Lo stesso nome è in `ui/island.js`.
pub(crate) const PILL_EVENT: &str = "pill-label";

/// `id` della richiesta `version` che il guscio manda appena collegato.
const VERSION_ID: i64 = 1;

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Tier {
    Latest,
    Working,
    Waiting,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PillSession {
    pub(crate) id: String,
    line: String,
    state: AgentState,
    revision: u64,
}

#[derive(Debug, Default)]
pub(crate) struct PillBoard {
    sessions: Vec<PillSession>,
    next_revision: u64,
}

impl PillBoard {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// Registra l'evento e restituisce la riga da mostrare.
    pub(crate) fn apply(&mut self, event: &SessionEventParams) -> String {
        self.next_revision = self.next_revision.saturating_add(1);
        let revision = self.next_revision;
        let line = if event.line.is_empty() {
            event.status.line()
        } else {
            event.line.clone()
        };
        if let Some(session) = self
            .sessions
            .iter_mut()
            .find(|session| session.id == event.id)
        {
            session.line = line;
            session.state = event.status.state;
            session.revision = revision;
        } else {
            self.sessions.push(PillSession {
                id: event.id.clone(),
                line,
                state: event.status.state,
                revision,
            });
        }
        self.label().to_string()
    }

    pub(crate) fn label(&self) -> &str {
        pill_label(&self.sessions)
    }
}

pub(crate) fn pill_label(sessions: &[PillSession]) -> &str {
    sessions
        .iter()
        .filter_map(|session| tier(session.state).map(|tier| (tier, session.revision, session)))
        .max_by_key(|(tier, revision, _)| (*tier, *revision))
        .map(|(_, _, session)| session.line.as_str())
        .unwrap_or(REST_LABEL)
}

fn tier(state: AgentState) -> Option<Tier> {
    match state {
        AgentState::AwaitingPermission | AgentState::AwaitingReply => Some(Tier::Waiting),
        AgentState::Working => Some(Tier::Working),
        AgentState::Starting | AgentState::Finished | AgentState::Failed => Some(Tier::Latest),
        AgentState::Archived => None,
    }
}

/// Riga letta dal demone. `session/request` non si risponde: è ignorata.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum DaemonLine {
    Event(SessionEventParams),
    Version,
    UnexpectedVersion,
    Ignore,
}

pub(crate) fn classify_line(line: &str) -> DaemonLine {
    let Ok(value) = serde_json::from_str::<Value>(line) else {
        return DaemonLine::Ignore;
    };
    let Some(object) = value.as_object() else {
        return DaemonLine::Ignore;
    };
    if object.get("jsonrpc").and_then(Value::as_str) != Some("2.0") {
        return DaemonLine::Ignore;
    }
    if is_version_id(object.get("id")) && object.get("method").is_none() {
        let name_ok = object
            .get("result")
            .and_then(|result| result.get("name"))
            .and_then(Value::as_str)
            == Some("bitbotd");
        return if name_ok {
            DaemonLine::Version
        } else {
            DaemonLine::UnexpectedVersion
        };
    }
    if object.contains_key("id") {
        return DaemonLine::Ignore;
    }
    if object.get("method").and_then(Value::as_str) != Some(SESSION_EVENT_METHOD) {
        return DaemonLine::Ignore;
    }
    match object.get("params").cloned() {
        Some(params) => serde_json::from_value(params)
            .map(DaemonLine::Event)
            .unwrap_or(DaemonLine::Ignore),
        None => DaemonLine::Ignore,
    }
}

fn is_version_id(id: Option<&Value>) -> bool {
    id.and_then(Value::as_i64) == Some(VERSION_ID)
}

#[cfg(test)]
mod tests {
    use bitbot_protocol::{Action, AgentState, SessionEventParams, Status};

    use super::*;

    fn at(id: &str, state: AgentState, line: &str, revision: u64) -> PillSession {
        PillSession {
            id: id.to_string(),
            line: line.to_string(),
            state,
            revision,
        }
    }

    fn params(id: &str, state: AgentState, line: &str) -> SessionEventParams {
        let mut event = SessionEventParams::from_status(id, Status::new("opencode", state));
        event.line = line.to_string();
        event
    }

    #[test]
    fn an_empty_board_rests() {
        assert_eq!(pill_label(&[]), REST_LABEL);
        assert_eq!(PillBoard::new().label(), REST_LABEL);
    }

    #[test]
    fn archived_sessions_leave_the_pill_at_rest() {
        let sessions = vec![at("s-1", AgentState::Archived, "[opencode] archiviata", 4)];
        assert_eq!(pill_label(&sessions), REST_LABEL);
    }

    #[test]
    fn a_wait_beats_newer_work() {
        let sessions = vec![
            at(
                "s-1",
                AgentState::AwaitingPermission,
                "[opencode] in attesa di permesso",
                1,
            ),
            at("s-2", AgentState::Working, "[cursor] al lavoro", 8),
        ];
        assert_eq!(pill_label(&sessions), "[opencode] in attesa di permesso");
    }

    #[test]
    fn a_reply_wait_beats_newer_work() {
        let sessions = vec![
            at("s-1", AgentState::Working, "[cursor] al lavoro", 9),
            at(
                "s-2",
                AgentState::AwaitingReply,
                "[cursor] in attesa di risposta",
                2,
            ),
        ];
        assert_eq!(pill_label(&sessions), "[cursor] in attesa di risposta");
    }

    #[test]
    fn the_newer_wait_wins() {
        let sessions = vec![
            at(
                "s-1",
                AgentState::AwaitingPermission,
                "[opencode] in attesa di permesso",
                3,
            ),
            at(
                "s-2",
                AgentState::AwaitingReply,
                "[cursor] in attesa di risposta",
                4,
            ),
        ];
        assert_eq!(pill_label(&sessions), "[cursor] in attesa di risposta");
    }

    #[test]
    fn working_beats_a_newer_startup_or_finish() {
        let sessions = vec![
            at("s-1", AgentState::Working, "[opencode] al lavoro", 1),
            at("s-2", AgentState::Starting, "[cursor] avvio", 6),
            at("s-3", AgentState::Finished, "[codex] finito", 7),
        ];
        assert_eq!(pill_label(&sessions), "[opencode] al lavoro");
    }

    #[test]
    fn the_newer_worker_wins() {
        let sessions = vec![
            at("s-1", AgentState::Working, "[opencode] al lavoro", 2),
            at("s-2", AgentState::Working, "[cursor] al lavoro", 5),
        ];
        assert_eq!(pill_label(&sessions), "[cursor] al lavoro");
    }

    #[test]
    fn without_priority_the_latest_update_wins() {
        let sessions = vec![
            at("s-1", AgentState::Finished, "[opencode] finito", 1),
            at("s-2", AgentState::Failed, "[codex] errore", 3),
            at("s-3", AgentState::Starting, "[cursor] avvio", 2),
            at("s-4", AgentState::Archived, "[opencode] archiviata", 9),
        ];
        assert_eq!(pill_label(&sessions), "[codex] errore");
    }

    #[test]
    fn a_later_event_replaces_the_same_session() {
        let mut board = PillBoard::new();
        board.apply(&params("s-1", AgentState::Working, "[opencode] al lavoro"));
        let label = board.apply(&params("s-1", AgentState::Finished, "[opencode] finito"));
        assert_eq!(label, "[opencode] finito");
    }

    #[test]
    fn an_empty_line_falls_back_to_the_status() {
        let status = Status::new("opencode", AgentState::Working).with_action(Action::read("a.rs"));
        let mut event = SessionEventParams::from_status("s-1", status.clone());
        event.line.clear();
        let mut board = PillBoard::new();
        assert_eq!(board.apply(&event), status.line());
    }

    #[test]
    fn classify_keeps_events_and_drops_requests() {
        let event = params("s-1", AgentState::Working, "[opencode] al lavoro");
        let line = serde_json::json!({
            "jsonrpc": "2.0",
            "method": SESSION_EVENT_METHOD,
            "params": event,
        })
        .to_string();
        assert_eq!(classify_line(&line), DaemonLine::Event(event));

        let request = r#"{"jsonrpc":"2.0","method":"session/request","params":{"id":"s-1","requestId":"r-1","body":{"kind":"permission","options":[]}}}"#;
        assert_eq!(classify_line(request), DaemonLine::Ignore);
        assert_eq!(
            classify_line(r#"{"jsonrpc":"2.0","id":2,"result":null}"#),
            DaemonLine::Ignore
        );
        assert_eq!(classify_line("non è json"), DaemonLine::Ignore);
        assert_eq!(
            classify_line(r#"{"jsonrpc":"2.0","method":"session/event"}"#),
            DaemonLine::Ignore
        );
    }

    #[test]
    fn classify_recognizes_the_version_reply() {
        assert_eq!(
            classify_line(
                r#"{"jsonrpc":"2.0","id":1,"result":{"name":"bitbotd","version":"0.1.0"}}"#
            ),
            DaemonLine::Version
        );
        assert_eq!(
            classify_line(r#"{"jsonrpc":"2.0","id":1,"result":{"name":"altro"}}"#),
            DaemonLine::UnexpectedVersion
        );
        assert_eq!(
            classify_line(r#"{"jsonrpc":"2.0","id":1,"error":{"code":-32600,"message":"no"}}"#),
            DaemonLine::UnexpectedVersion
        );
    }
}
