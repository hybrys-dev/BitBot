//! Stato unificato degli agenti, condiviso da driver e interfacce.

/// Stato di una sessione, indipendente dall'agente che lo produce.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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
#[derive(Debug, Clone, PartialEq, Eq)]
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
}
