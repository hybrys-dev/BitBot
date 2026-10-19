//! Stato unificato degli agenti, condiviso da driver e interfacce,
//! e tipi dell'API JSON-RPC del demone.

use serde::{Deserialize, Serialize};

/// Nome del metodo JSON-RPC che restituisce [`DaemonVersion`].
pub const VERSION_METHOD: &str = "version";

/// Nome del metodo JSON-RPC che elenca le sessioni con lo stato unificato.
pub const SESSIONS_METHOD: &str = "sessions";

/// Nome del metodo JSON-RPC che avvia una sessione agente nel demone.
pub const SESSION_START_METHOD: &str = "session/start";

/// Risposta del metodo `version`: chi parla con il demone e in che versione.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DaemonVersion {
    pub name: String,
    pub version: String,
}

/// Parametri di `session/start`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SessionStartParams {
    /// Nome del profilo agente, per esempio `opencode`.
    pub agent: String,
    /// Cartella di lavoro della sessione.
    pub cwd: String,
    /// Primo prompt della sessione.
    pub task: String,
}

/// Risposta di `session/start`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SessionStarted {
    /// Identificativo della sessione per l'agente.
    pub session_id: String,
}

/// Una sessione come la vede la vista compatta.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SessionStatus {
    pub id: String,
    pub agent: String,
    pub state: AgentState,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub action: Option<Action>,
}

impl SessionStatus {
    /// L'ultimo stato osservato, con l'ultima azione accanto.
    pub fn from_status(id: impl Into<String>, status: &Status) -> Self {
        Self {
            id: id.into(),
            agent: status.agent.clone(),
            state: status.state,
            action: status.action.clone(),
        }
    }
}


/// Stato di una sessione, indipendente dall'agente che lo produce.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
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
pub struct Action {
    pub kind: ActionKind,
    pub target: String,
    pub added: Option<u32>,
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
#[derive(Debug, Clone, PartialEq, Eq)]
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
}
