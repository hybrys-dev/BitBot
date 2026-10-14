//! Client ACP per il primo spike: avvia OpenCode, apre una sessione e segue un compito.

mod status;

use std::path::{Path, PathBuf};

use agent_client_protocol::schema::v1::{
    AuthMethod, ClientCapabilities, Implementation, InitializeRequest, InitializeResponse,
    RequestPermissionRequest, SessionConfigKind, SessionConfigOption, SessionConfigOptionCategory,
    SessionModeState, SessionNotification,
};
use agent_client_protocol::schema::ProtocolVersion;
use agent_client_protocol::{AcpAgent, AcpAgentConfig, Error, LineDirection, SessionMessage};
use anyhow::Context;
use bitbot_protocol::{AgentState, Status};

const CLIENT_NAME: &str = "bitbot-spike";

macro_rules! json_string {
    ($value:expr) => {
        match serde_json::to_value($value) {
            Ok(serde_json::Value::String(text)) => text,
            Ok(other) => other.to_string(),
            Err(err) => err.to_string(),
        }
    };
}

/// Richiesta `initialize` per il protocollo v1, senza capacità di file o terminale.
pub fn initialize_request() -> InitializeRequest {
    InitializeRequest::new(ProtocolVersion::V1)
        .client_capabilities(ClientCapabilities::new())
        .client_info(Implementation::new(CLIENT_NAME, env!("CARGO_PKG_VERSION")))
}

/// Avvia `opencode acp` come processo figlio.
///
/// Stdin e stdout restano il trasporto JSON-RPC. Lo stderr del figlio viene
/// inoltrato sullo stderr di questo processo.
fn opencode_agent() -> AcpAgent {
    AcpAgent::new(AcpAgentConfig::new("opencode").arg("acp")).with_debug(|line, direction| {
        if direction == LineDirection::Stderr {
            eprintln!("{line}");
        }
    })
}

/// Rende assoluta la cartella di lavoro richiesta da `session/new`.
pub fn prepare_cwd(path: impl AsRef<Path>) -> anyhow::Result<PathBuf> {
    let path = path.as_ref();
    let cwd = std::fs::canonicalize(path)
        .with_context(|| format!("cartella non valida: {}", path.display()))?;
    if !cwd.is_dir() {
        anyhow::bail!("non è una cartella: {}", cwd.display());
    }
    Ok(cwd)
}

/// Esegue `initialize`, `session/new` e `session/prompt` contro OpenCode.
///
/// Ogni aggiornamento viene stampato come stato unificato. Su
/// `session/request_permission` stampa la riga e termina: la risposta è il passo successivo.
pub async fn run(cwd: impl AsRef<Path>, task: &str) -> anyhow::Result<()> {
    let task = task.trim();
    if task.is_empty() {
        anyhow::bail!("uso: bitbot-spike <cartella> <compito>");
    }
    let cwd = prepare_cwd(cwd)?;
    let task = task.to_string();
    println!(
        "{}",
        Status::new(status::AGENT_NAME, AgentState::Starting).line()
    );
    let connected = agent_client_protocol::Client
        .builder()
        .name(CLIENT_NAME)
        .connect_with(opencode_agent(), async move |connection| {
            let initialized = connection
                .send_request(initialize_request())
                .block_task()
                .await?;
            if initialized.protocol_version != ProtocolVersion::V1 {
                let version = initialized.protocol_version.as_u16();
                return Err(Error::into_internal_error(std::io::Error::other(format!(
                    "versione di protocollo non supportata: {version}"
                ))));
            }
            println!("{}", format_initialize(&initialized));

            connection
                .build_session(&cwd)
                .block_task()
                .run_until(async |mut session| {
                    println!(
                        "{}",
                        format_session(session.config_options(), session.modes())
                    );
                    session.send_prompt(&task)?;
                    loop {
                        match session.read_update().await? {
                            SessionMessage::SessionMessage(dispatch) => {
                                print_dispatch(dispatch).await?;
                            }
                            SessionMessage::StopReason(reason) => {
                                println!("{}", status::status_from_stop(&reason).line());
                                break;
                            }
                            _ => println!("{}", status::status_unknown().line()),
                        }
                    }
                    Ok(())
                })
                .await
        })
        .await;
    if connected.is_err() {
        println!(
            "{}",
            Status::new(status::AGENT_NAME, AgentState::Failed).line()
        );
    }
    connected.context("connessione con opencode acp interrotta")?;
    Ok(())
}

async fn print_dispatch(dispatch: agent_client_protocol::Dispatch) -> Result<(), Error> {
    agent_client_protocol::util::MatchDispatch::new(dispatch)
        .if_request(async |request: RequestPermissionRequest, _responder| {
            leave_for_permission(&request)
        })
        .await
        .if_notification(async |notification: SessionNotification| {
            println!(
                "{}",
                status::status_from_update(&notification.update).line()
            );
            Ok(())
        })
        .await
        .otherwise_ignore()
}

fn leave_for_permission(request: &RequestPermissionRequest) -> Result<(), Error> {
    println!("{}", status::status_from_permission(request).line());
    std::process::exit(0);
}

/// Testo su stdout per la risposta di `initialize`.
pub fn format_initialize(response: &InitializeResponse) -> String {
    let session = &response.agent_capabilities.session_capabilities;
    let mut lines = vec![
        format!("versione: {}", response.protocol_version.as_u16()),
        format!("session/list: {}", yes_no(session.list.is_some())),
        format!("session/resume: {}", yes_no(session.resume.is_some())),
        format!(
            "session/load: {}",
            yes_no(response.agent_capabilities.load_session)
        ),
        "autenticazione:".to_string(),
    ];
    if response.auth_methods.is_empty() {
        lines.push("nessuna".to_string());
    } else {
        lines.extend(response.auth_methods.iter().map(format_auth_method));
    }
    lines.join("\n")
}

/// Testo su stdout per le opzioni di `session/new`.
///
/// Se l'agente non dichiara un'opzione di categoria `mode`, si ripiega sul
/// campo `modes`.
pub fn format_session(
    options: Option<&[SessionConfigOption]>,
    modes: Option<&SessionModeState>,
) -> String {
    let options = options.unwrap_or(&[]);
    let mut lines = vec!["opzioni:".to_string()];
    if options.is_empty() {
        lines.push("nessuna".to_string());
    } else {
        lines.extend(options.iter().map(format_option));
    }

    let has_mode_option = options
        .iter()
        .any(|option| matches!(option.category, Some(SessionConfigOptionCategory::Mode)));
    if !has_mode_option {
        if let Some(modes) = modes {
            lines.push("modalità:".to_string());
            lines.push(format!(
                "corrente: {}",
                json_string!(&modes.current_mode_id)
            ));
            lines.extend(
                modes
                    .available_modes
                    .iter()
                    .map(|mode| format!("- id={} nome={}", json_string!(&mode.id), mode.name)),
            );
        }
    }

    lines.join("\n")
}

fn format_auth_method(method: &AuthMethod) -> String {
    let mut line = format!("- id={} nome={}", json_string!(method.id()), method.name());
    if let Some(description) = method.description() {
        line.push_str(" descrizione=");
        line.push_str(description);
    }
    line
}

fn format_option(option: &SessionConfigOption) -> String {
    format!(
        "- id={} nome={} categoria={} valore={}",
        json_string!(&option.id),
        option.name,
        category_label(&option.category),
        current_value(&option.kind)
    )
}

fn category_label(category: &Option<SessionConfigOptionCategory>) -> String {
    match category {
        Some(SessionConfigOptionCategory::Mode) => "mode".to_string(),
        Some(SessionConfigOptionCategory::Model) => "model".to_string(),
        Some(SessionConfigOptionCategory::ModelConfig) => "model_config".to_string(),
        Some(SessionConfigOptionCategory::ThoughtLevel) => "thought_level".to_string(),
        Some(SessionConfigOptionCategory::Other(name)) => name.clone(),
        Some(_) => "altro".to_string(),
        None => "-".to_string(),
    }
}

fn current_value(kind: &SessionConfigKind) -> String {
    match kind {
        SessionConfigKind::Select(select) => json_string!(&select.current_value),
        SessionConfigKind::Boolean(boolean) => boolean.current_value.to_string(),
        _ => "sconosciuto".to_string(),
    }
}

fn yes_no(value: bool) -> &'static str {
    if value {
        "sì"
    } else {
        "no"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use agent_client_protocol::schema::v1::{
        SessionConfigSelect, SessionConfigSelectOption, SessionMode,
    };

    fn select_option(
        id: &str,
        name: &str,
        category: SessionConfigOptionCategory,
        current: &str,
    ) -> SessionConfigOption {
        SessionConfigOption::new(
            id.to_string(),
            name.to_string(),
            SessionConfigKind::Select(SessionConfigSelect::new(
                current.to_string(),
                vec![SessionConfigSelectOption::new(
                    current.to_string(),
                    name.to_string(),
                )],
            )),
        )
        .category(category)
    }

    #[test]
    fn initialize_does_not_advertise_file_or_terminal() {
        let json = serde_json::to_value(initialize_request()).unwrap();
        let capabilities = &json["clientCapabilities"];
        assert_ne!(capabilities["fs"]["readTextFile"], true);
        assert_ne!(capabilities["fs"]["writeTextFile"], true);
        assert_ne!(capabilities["terminal"], true);
        assert_eq!(json["clientInfo"]["name"], CLIENT_NAME);
        assert_eq!(json["protocolVersion"], 1);
    }

    #[test]
    fn format_session_prints_model_reasoning_mode_and_legacy_modes() {
        let declared = [
            select_option(
                "model",
                "Modello",
                SessionConfigOptionCategory::Model,
                "gpt-5",
            ),
            select_option(
                "reasoning",
                "Ragionamento",
                SessionConfigOptionCategory::ThoughtLevel,
                "high",
            ),
            select_option(
                "mode",
                "Modalità",
                SessionConfigOptionCategory::Mode,
                "build",
            ),
        ];
        let with_mode = format_session(Some(&declared), None);
        assert!(with_mode.contains("id=model"));
        assert!(with_mode.contains("categoria=model"));
        assert!(with_mode.contains("valore=gpt-5"));
        assert!(with_mode.contains("categoria=thought_level"));
        assert!(with_mode.contains("valore=high"));
        assert!(with_mode.contains("categoria=mode"));
        assert!(with_mode.contains("valore=build"));
        assert!(!with_mode.contains("modalità:"));

        let without_mode = [
            select_option(
                "model",
                "Modello",
                SessionConfigOptionCategory::Model,
                "gpt-5",
            ),
            select_option(
                "reasoning",
                "Ragionamento",
                SessionConfigOptionCategory::ThoughtLevel,
                "high",
            ),
        ];
        let modes = SessionModeState::new(
            "ask",
            vec![
                SessionMode::new("ask", "Ask"),
                SessionMode::new("build", "Build"),
            ],
        );
        let fallback = format_session(Some(&without_mode), Some(&modes));
        assert!(fallback.contains("categoria=model"));
        assert!(fallback.contains("categoria=thought_level"));
        assert!(fallback.contains("modalità:"));
        assert!(fallback.contains("corrente: ask"));
        assert!(fallback.contains("id=ask nome=Ask"));
        assert!(fallback.contains("id=build nome=Build"));
    }

    #[test]
    fn prepare_cwd_accepts_a_directory_and_rejects_a_file() {
        let cwd = prepare_cwd(".").unwrap();
        assert!(cwd.is_absolute());
        assert!(cwd.is_dir());

        let file = std::env::temp_dir().join("bitbot-spike-not-a-directory");
        std::fs::write(&file, "x").unwrap();
        let error = prepare_cwd(&file).unwrap_err();
        let _ = std::fs::remove_file(&file);
        assert!(error.to_string().contains("non è una cartella"));
    }
}
