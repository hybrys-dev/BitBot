//! Stranezze note per agente, ricavate dalle corse dello spike.
//!
//! Corregge solo nomi, valori predefiniti e alias. Non elenca modelli
//! e non entra nel flusso ACP: la ripresa e i permessi restano quelli
//! dichiarati dall'agente. Finché il client non legge questi dati, fuori
//! dai test non ci sono riferimenti.

#![cfg_attr(not(test), allow(dead_code))]

/// Manifest di un profilo: stranezze osservate, oppure il motivo per cui manca.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct AgentManifest {
    pub name: &'static str,
    pub body: ManifestBody,
}

/// Corpo del manifest. `Unobserved` non porta resume, effort né permessi.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ManifestBody {
    Observed(Observed),
    Unobserved {
        label: &'static str,
        reason: &'static str,
    },
}

/// Solo ciò che una corsa ha mostrato. I campi assenti non sono stati una stranezza.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Observed {
    pub resume: Option<Resume>,
    pub effort: Effort,
    pub permissions: Permissions,
    pub cursor_extensions: Option<CursorExtensions>,
}

/// Metodo di ripresa osservato.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Resume {
    /// `session/resume`: il turno continua la sessione.
    SessionResume,
    /// Solo `session/load`. `replay` è vero se gli aggiornamenti sono già in coda.
    SessionLoad { replay: bool },
}

/// Dove vive l'effort osservato.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Effort {
    /// Opzione ACP di categoria `thought_level`.
    ///
    /// `observed_change` è il valore corrente e quello applicato nella corsa,
    /// non la scala intera.
    ThoughtLevel {
        option_id: &'static str,
        observed_change: (&'static str, &'static str),
        /// Se il processo di ripresa rilegge il valore da qui, non dalla sessione.
        resume_source: Option<EffortSource>,
    },
    /// Nessuna opzione `thought_level`: il valore sta nella stringa del modello.
    InModelString { field: &'static str },
}

/// Da dove un processo nuovo rilegge l'effort.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum EffortSource {
    /// File di configurazione dell'agente, non lo stato della sessione.
    Config,
}

/// Permessi osservati, o il fatto che in quella corsa non sono arrivati.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Permissions {
    /// Id delle opzioni osservate.
    Ids {
        once: &'static str,
        always: &'static str,
        reject: &'static str,
    },
    /// `session/request_permission` è scattata. Gli id non sono una mappa completa.
    Observed,
    /// Nessuna richiesta. `mode` è la modalità dichiarata in quella corsa.
    NotObserved { mode: &'static str },
}

/// Estensioni `cursor/*` di cui si sa se sono scattate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct CursorExtensions {
    pub create_plan: bool,
    pub ask_question: bool,
}

const OPENCODE: AgentManifest = AgentManifest {
    name: "opencode",
    body: ManifestBody::Observed(Observed {
        resume: Some(Resume::SessionResume),
        effort: Effort::ThoughtLevel {
            option_id: "effort",
            observed_change: ("default", "none"),
            resume_source: None,
        },
        permissions: Permissions::Ids {
            once: "once",
            always: "always",
            reject: "reject",
        },
        cursor_extensions: None,
    }),
};

const CURSOR: AgentManifest = AgentManifest {
    name: "cursor",
    body: ManifestBody::Observed(Observed {
        resume: Some(Resume::SessionLoad { replay: true }),
        effort: Effort::InModelString {
            field: "reasoning_effort",
        },
        permissions: Permissions::Observed,
        cursor_extensions: Some(CursorExtensions {
            create_plan: true,
            ask_question: false,
        }),
    }),
};

const CODEX: AgentManifest = AgentManifest {
    name: "codex",
    body: ManifestBody::Observed(Observed {
        resume: None,
        effort: Effort::ThoughtLevel {
            option_id: "reasoning_effort",
            observed_change: ("high", "xhigh"),
            resume_source: Some(EffortSource::Config),
        },
        permissions: Permissions::NotObserved {
            mode: "full-access",
        },
        cursor_extensions: None,
    }),
};

const CLAUDE: AgentManifest = AgentManifest {
    name: "claude",
    body: ManifestBody::Unobserved {
        label: "non osservato",
        reason: "via ACP bloccata dal vincolo API key (nessuna API a consumo)",
    },
};

const MANIFESTS: &[AgentManifest] = &[OPENCODE, CURSOR, CODEX, CLAUDE];

/// Manifest del profilo, se il nome è uno dei quattro dello spike.
pub(crate) fn manifest(name: &str) -> Option<&'static AgentManifest> {
    MANIFESTS.iter().find(|item| item.name == name)
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::AgentProfile;

    fn assert_profile_name(name: &str) {
        let profile = AgentProfile::resolve_with_path(name, None).unwrap();
        assert_eq!(profile.name, name);
    }

    #[test]
    fn opencode_manifest_uses_session_resume_thought_level_and_permission_ids() {
        assert_profile_name("opencode");
        assert_eq!(
            manifest("opencode"),
            Some(&AgentManifest {
                name: "opencode",
                body: ManifestBody::Observed(Observed {
                    resume: Some(Resume::SessionResume),
                    effort: Effort::ThoughtLevel {
                        option_id: "effort",
                        observed_change: ("default", "none"),
                        resume_source: None,
                    },
                    permissions: Permissions::Ids {
                        once: "once",
                        always: "always",
                        reject: "reject",
                    },
                    cursor_extensions: None,
                }),
            })
        );
    }

    #[test]
    fn cursor_manifest_loads_with_replay_and_keeps_effort_in_the_model_string() {
        assert_profile_name("cursor");
        assert_eq!(
            manifest("cursor"),
            Some(&AgentManifest {
                name: "cursor",
                body: ManifestBody::Observed(Observed {
                    resume: Some(Resume::SessionLoad { replay: true }),
                    effort: Effort::InModelString {
                        field: "reasoning_effort",
                    },
                    permissions: Permissions::Observed,
                    cursor_extensions: Some(CursorExtensions {
                        create_plan: true,
                        ask_question: false,
                    }),
                }),
            })
        );
    }

    #[test]
    fn codex_manifest_uses_thought_level_and_rereads_effort_from_config() {
        assert_profile_name("codex");
        assert_eq!(
            manifest("codex"),
            Some(&AgentManifest {
                name: "codex",
                body: ManifestBody::Observed(Observed {
                    resume: None,
                    effort: Effort::ThoughtLevel {
                        option_id: "reasoning_effort",
                        observed_change: ("high", "xhigh"),
                        resume_source: Some(EffortSource::Config),
                    },
                    permissions: Permissions::NotObserved {
                        mode: "full-access",
                    },
                    cursor_extensions: None,
                }),
            })
        );
    }

    #[test]
    fn claude_manifest_is_unobserved_because_acp_needs_an_api_key() {
        assert_profile_name("claude");
        assert_eq!(
            manifest("claude"),
            Some(&AgentManifest {
                name: "claude",
                body: ManifestBody::Unobserved {
                    label: "non osservato",
                    reason: "via ACP bloccata dal vincolo API key (nessuna API a consumo)",
                },
            })
        );
    }
}
