//! Stato unificato degli agenti, condiviso da driver e interfacce,
//! e tipi dell'API JSON-RPC del demone.

use serde::{Deserialize, Serialize};

/// Nome del metodo JSON-RPC che restituisce [`DaemonVersion`].
pub const VERSION_METHOD: &str = "version";

/// Avvia una sessione ACP in memoria.
pub const SESSION_START_METHOD: &str = "session/start";

/// Risponde a un permesso, una domanda o un piano in attesa.
pub const SESSION_RESPOND_METHOD: &str = "session/respond";

/// Notifica: lo stato unificato di una sessione è cambiato.
pub const SESSION_EVENT_METHOD: &str = "session/event";

/// Notifica: l'agente aspetta un permesso, una domanda o un piano.
pub const SESSION_REQUEST_METHOD: &str = "session/request";

/// Risposta del metodo `version`: chi parla con il demone e in che versione.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DaemonVersion {
    pub name: String,
    pub version: String,
}

/// Parametri di `session/start`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SessionStartParams {
    pub agent: String,
    pub cwd: String,
    pub prompt: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
}

/// Risposta di `session/start`: id locale del demone, non quello ACP.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SessionStartResult {
    pub id: String,
}

/// Parametri della notifica `session/event`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SessionEventParams {
    pub id: String,
    /// Riga della vista ridotta, la stessa che lo spike stampa.
    pub line: String,
    pub status: Status,
}

/// Parametri della notifica `session/request`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SessionRequestParams {
    pub id: String,
    pub request_id: String,
    pub body: SessionRequestBody,
}

/// Cosa l'agente sta chiedendo.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SessionRequestBody {
    Permission {
        options: Vec<PermissionOptionView>,
    },
    Question {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        call_id: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        title: Option<String>,
        questions: Vec<QuestionView>,
    },
    Plan {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        call_id: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        name: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        overview: Option<String>,
        plan: String,
        #[serde(default, skip_serializing_if = "is_false")]
        is_project: bool,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        todos: Vec<PlanTodoView>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        phases: Vec<PlanPhaseView>,
    },
}

/// Opzione di permesso dichiarata dall'agente.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PermissionOptionView {
    pub id: String,
    pub name: String,
    /// `allow_once`, `allow_always`, `reject_once`, `reject_always` o `other`.
    pub kind: String,
}

/// Una domanda a scelta, come `cursor/ask_question`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct QuestionView {
    pub id: String,
    pub prompt: String,
    pub options: Vec<QuestionOptionView>,
    #[serde(default)]
    pub allow_multiple: bool,
}

/// Una voce di una domanda.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct QuestionOptionView {
    pub id: String,
    pub label: String,
}

/// Voce di un piano.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PlanTodoView {
    pub id: String,
    pub content: String,
    pub status: String,
}

/// Fase di un piano.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PlanPhaseView {
    pub name: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub todos: Vec<PlanTodoView>,
}

/// Parametri di `session/respond`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SessionRespondParams {
    pub id: String,
    pub request_id: String,
    pub outcome: RespondOutcome,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub answers: Vec<QuestionAnswer>,
}

/// Scelta del client. `cancelled` sblocca l'agente anche senza una UI.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RespondOutcome {
    AllowOnce,
    AllowAlways,
    Reject,
    Cancelled,
    Accepted,
    Rejected,
    Answered,
    Skipped,
}

/// Risposta a una o più domande.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct QuestionAnswer {
    pub question_id: String,
    pub selected_option_ids: Vec<String>,
}

/// Risposta pronta da inoltrare all'agente.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SessionReply {
    pub outcome: RespondOutcome,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub answers: Vec<QuestionAnswer>,
}

fn is_false(value: &bool) -> bool {
    !*value
}

impl SessionReply {
    /// Risposta che sblocca l'agente senza scegliere un'opzione.
    pub fn cancelled() -> Self {
        Self {
            outcome: RespondOutcome::Cancelled,
            answers: Vec::new(),
        }
    }
}

impl SessionEventParams {
    /// Evento a partire dallo stato unificato.
    pub fn from_status(id: impl Into<String>, status: Status) -> Self {
        let line = status.line();
        Self {
            id: id.into(),
            line,
            status,
        }
    }
}

/// Stato di una sessione, indipendente dall'agente che lo produce.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentState {
    /// Processo in partenza, inizializzazione o login in corso.
    Starting,
    /// L'agente sta eseguendo il turno.
    Working,
    /// Una richiesta di permesso aspetta una risposta.
    AwaitingPermission,
    /// Una domanda o un piano aspettano una risposta.
    AwaitingReply,
    /// Il turno è concluso.
    Finished,
    /// Processo caduto, login scaduto o limite del piano raggiunto.
    Failed,
    /// Fuori dalla vista ridotta.
    Archived,
}

impl AgentState {
    /// Etichetta usata nella vista ridotta e nello spike.
    pub fn label(self) -> &'static str {
        match self {
            Self::Starting => "avvio",
            Self::Working => "al lavoro",
            Self::AwaitingPermission => "in attesa di permesso",
            Self::AwaitingReply => "in attesa di risposta",
            Self::Finished => "finito",
            Self::Failed => "errore",
            Self::Archived => "archiviata",
        }
    }
}

/// Tipo di azione mostrato accanto allo stato.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActionKind {
    /// Lettura di file o dati.
    Read,
    /// Modifica, spostamento o cancellazione.
    Edit,
    /// Esecuzione di un comando.
    Command,
    /// Azione senza una delle categorie precedenti.
    Other,
}

/// Ultima azione dell'agente, accanto allo stato.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Action {
    pub kind: ActionKind,
    pub target: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub added: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub removed: Option<u32>,
}

impl Action {
    /// Lettura di un percorso.
    pub fn read(target: impl Into<String>) -> Self {
        Self {
            kind: ActionKind::Read,
            target: target.into(),
            added: None,
            removed: None,
        }
    }

    /// Modifica di un file, con righe aggiunte e rimosse.
    pub fn edit(target: impl Into<String>, added: u32, removed: u32) -> Self {
        Self {
            kind: ActionKind::Edit,
            target: target.into(),
            added: Some(added),
            removed: Some(removed),
        }
    }

    /// Esecuzione di un comando.
    pub fn command(target: impl Into<String>) -> Self {
        Self {
            kind: ActionKind::Command,
            target: target.into(),
            added: None,
            removed: None,
        }
    }

    /// Azione descritta solo dal testo.
    pub fn other(target: impl Into<String>) -> Self {
        Self {
            kind: ActionKind::Other,
            target: target.into(),
            added: None,
            removed: None,
        }
    }
}

/// Stato unificato pronto da stampare su una riga.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Status {
    pub agent: String,
    pub state: AgentState,
    pub action: Option<Action>,
}

impl Status {
    /// Stato senza un'azione accanto.
    pub fn new(agent: impl Into<String>, state: AgentState) -> Self {
        Self {
            agent: agent.into(),
            state,
            action: None,
        }
    }

    /// Aggiunge l'ultima azione.
    #[must_use]
    pub fn with_action(mut self, action: Action) -> Self {
        self.action = Some(action);
        self
    }

    /// Riga della vista ridotta, per esempio `[opencode] al lavoro · modifica src/lib.rs +8 −2`.
    pub fn line(&self) -> String {
        match &self.action {
            Some(action) => {
                let action = format_action(action);
                if action.is_empty() {
                    format!("[{}] {}", self.agent, self.state.label())
                } else {
                    format!("[{}] {} · {action}", self.agent, self.state.label())
                }
            }
            None => format!("[{}] {}", self.agent, self.state.label()),
        }
    }
}

fn format_action(action: &Action) -> String {
    let kind = match action.kind {
        ActionKind::Read => "lettura",
        ActionKind::Edit => "modifica",
        ActionKind::Command => "comando",
        ActionKind::Other => "",
    };
    let mut text = String::new();
    if !kind.is_empty() {
        text.push_str(kind);
    }
    if !action.target.is_empty() {
        if !text.is_empty() {
            text.push(' ');
        }
        text.push_str(&action.target);
    }
    if let (Some(added), Some(removed)) = (action.added, action.removed) {
        if !text.is_empty() {
            text.push(' ');
        }
        text.push_str(&format!("+{added} −{removed}"));
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_edit_work_line() {
        let line = Status::new("opencode", AgentState::Working)
            .with_action(Action::edit("src/lib.rs", 8, 2))
            .line();
        assert_eq!(line, "[opencode] al lavoro · modifica src/lib.rs +8 −2");
    }

    #[test]
    fn formats_states_without_action() {
        assert_eq!(
            Status::new("opencode", AgentState::Starting).line(),
            "[opencode] avvio"
        );
        assert_eq!(
            Status::new("opencode", AgentState::AwaitingPermission).line(),
            "[opencode] in attesa di permesso"
        );
        assert_eq!(
            Status::new("opencode", AgentState::AwaitingReply).line(),
            "[opencode] in attesa di risposta"
        );
        assert_eq!(
            Status::new("opencode", AgentState::Finished).line(),
            "[opencode] finito"
        );
        assert_eq!(
            Status::new("opencode", AgentState::Failed).line(),
            "[opencode] errore"
        );
        assert_eq!(
            Status::new("opencode", AgentState::Archived).line(),
            "[opencode] archiviata"
        );
    }

    #[test]
    fn daemon_version_round_trips_without_extra_fields() {
        let info = super::DaemonVersion {
            name: "bitbotd".to_string(),
            version: "0.1.0".to_string(),
        };
        let json = serde_json::to_string(&info).unwrap();
        assert_eq!(json, r#"{"name":"bitbotd","version":"0.1.0"}"#);
        assert_eq!(
            serde_json::from_str::<super::DaemonVersion>(&json).unwrap(),
            info
        );
        assert!(serde_json::from_str::<super::DaemonVersion>(
            r#"{"name":"bitbotd","version":"0.1.0","extra":1}"#
        )
        .is_err());
    }

    #[test]
    fn session_start_and_event_round_trip() {
        let params = super::SessionStartParams {
            agent: "opencode".to_string(),
            cwd: "/tmp/proj".to_string(),
            prompt: "aggiungi un test".to_string(),
            model: Some("beta".to_string()),
        };
        let json = serde_json::to_string(&params).unwrap();
        assert_eq!(
            serde_json::from_str::<super::SessionStartParams>(&json).unwrap(),
            params
        );
        assert!(serde_json::from_str::<super::SessionStartParams>(
            r#"{"agent":"opencode","cwd":"/tmp","prompt":"x","extra":1}"#
        )
        .is_err());

        let event = super::SessionEventParams::from_status(
            "s-1",
            Status::new("opencode", AgentState::Working).with_action(Action::edit(
                "src/lib.rs",
                8,
                2,
            )),
        );
        assert_eq!(
            event.line,
            "[opencode] al lavoro · modifica src/lib.rs +8 −2"
        );
        let json = serde_json::to_value(&event).unwrap();
        assert_eq!(json["status"]["state"], "working");
        assert_eq!(json["status"]["action"]["kind"], "edit");
        assert_eq!(
            serde_json::from_value::<super::SessionEventParams>(json).unwrap(),
            event
        );
    }

    #[test]
    fn session_request_and_reply_round_trip() {
        let request = super::SessionRequestParams {
            id: "s-1".to_string(),
            request_id: "r-1".to_string(),
            body: super::SessionRequestBody::Permission {
                options: vec![super::PermissionOptionView {
                    id: "once".to_string(),
                    name: "Allow once".to_string(),
                    kind: "allow_once".to_string(),
                }],
            },
        };
        let json = serde_json::to_value(&request).unwrap();
        assert_eq!(json["body"]["kind"], "permission");
        assert_eq!(json["requestId"], "r-1");
        assert_eq!(
            serde_json::from_value::<super::SessionRequestParams>(json).unwrap(),
            request
        );

        let reply = super::SessionRespondParams {
            id: "s-1".to_string(),
            request_id: "r-1".to_string(),
            outcome: super::RespondOutcome::Answered,
            answers: vec![super::QuestionAnswer {
                question_id: "q1".to_string(),
                selected_option_ids: vec!["plan".to_string()],
            }],
        };
        let json = serde_json::to_string(&reply).unwrap();
        assert!(json.contains(r#""outcome":"answered""#));
        assert_eq!(
            serde_json::from_str::<super::SessionRespondParams>(&json).unwrap(),
            reply
        );
        assert_eq!(
            super::SessionReply::cancelled().outcome,
            super::RespondOutcome::Cancelled
        );
    }
}
