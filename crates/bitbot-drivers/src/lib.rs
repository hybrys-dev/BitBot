//! Client ACP per il primo spike: avvia un agente, apre una sessione e segue un compito.

mod config_change;
mod cursor;
mod manifest;
mod permission;
mod profile;
pub mod runner;
mod sessions;
mod status;
mod terminal;

pub use profile::AgentProfile;

use std::future::Future;
use std::path::{Path, PathBuf};

use agent_client_protocol::role::acp::Agent;
use agent_client_protocol::role::HasPeer;
use agent_client_protocol::schema::v1::{
    AuthMethod, ClientCapabilities, ContentBlock, Implementation, InitializeRequest,
    InitializeResponse, ListSessionsRequest, RequestPermissionRequest, SessionConfigKind,
    SessionConfigOption, SessionConfigOptionCategory, SessionConfigSelectOptions, SessionInfo,
    SessionModeState, SessionNotification, SessionUpdate, SetSessionConfigOptionRequest,
};
use agent_client_protocol::schema::ProtocolVersion;
use agent_client_protocol::{
    AcpAgent, AcpAgentConfig, ActiveSession, ConnectionTo, Dispatch, Error, LineDirection,
    SessionMessage,
};
use anyhow::Context;
use bitbot_protocol::{AgentState, Status};

const CLIENT_NAME: &str = "bitbot-spike";
const DEFAULT_AGENT: &str = "opencode";
const USAGE: &str =
    "uso: bitbot-spike [--agent <nome>] [--model <valore>] [--resume] <cartella> <compito>";

pub(crate) fn json_text(value: &impl serde::Serialize) -> String {
    match serde_json::to_value(value) {
        Ok(serde_json::Value::String(text)) => text,
        Ok(other) => other.to_string(),
        Err(err) => err.to_string(),
    }
}

/// Argomenti di una invocazione dello spike.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Invocation {
    pub resume: bool,
    pub agent: String,
    /// Valore dell'opzione `model` da impostare prima del primo prompt.
    pub model: Option<String>,
    pub cwd: String,
    pub task: String,
}

/// Legge `bitbot-spike [--agent <nome>] [--model <valore>] [--resume] <cartella> <compito>`.
///
/// `--agent`, `--model` e `--resume` stanno prima della cartella, in
/// qualunque ordine. Senza `--agent` il profilo è `opencode`.
pub fn parse_invocation<I, S>(args: I) -> anyhow::Result<Invocation>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let args: Vec<String> = args
        .into_iter()
        .map(|arg| arg.as_ref().to_string())
        .collect();
    let mut resume = false;
    let mut agent = None;
    let mut model = None;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--resume" => {
                if resume {
                    anyhow::bail!(USAGE);
                }
                resume = true;
                index += 1;
            }
            "--agent" => {
                if agent.is_some() {
                    anyhow::bail!(USAGE);
                }
                let Some(name) = args.get(index + 1).filter(|name| !name.starts_with("--")) else {
                    anyhow::bail!(USAGE);
                };
                agent = Some(name.clone());
                index += 2;
            }
            "--model" => {
                if model.is_some() {
                    anyhow::bail!(USAGE);
                }
                let Some(value) = args.get(index + 1).filter(|value| !value.starts_with("--"))
                else {
                    anyhow::bail!(USAGE);
                };
                model = Some(value.clone());
                index += 2;
            }
            other if other.starts_with("--") => anyhow::bail!(USAGE),
            _ => break,
        }
    }
    let rest = &args[index..];
    if rest.len() < 2 || rest[1..].join(" ").trim().is_empty() {
        anyhow::bail!(USAGE);
    }
    Ok(Invocation {
        resume,
        agent: agent.unwrap_or_else(|| DEFAULT_AGENT.to_string()),
        model,
        cwd: rest[0].clone(),
        task: rest[1..].join(" "),
    })
}

/// Richiesta `initialize` per il protocollo v1, senza capacità di file o terminale.
pub fn initialize_request() -> InitializeRequest {
    InitializeRequest::new(ProtocolVersion::V1)
        .client_capabilities(ClientCapabilities::new())
        .client_info(Implementation::new(CLIENT_NAME, env!("CARGO_PKG_VERSION")))
}

/// Avvia l'agente del profilo come processo figlio.
///
/// Stdin e stdout restano il trasporto JSON-RPC. Lo stderr del figlio viene
/// inoltrato sullo stderr di questo processo. L'ambiente del profilo si aggiunge
/// a quello ereditato: una mappa vuota lascia il processo come prima.
pub(crate) fn spawn_agent(profile: &AgentProfile) -> AcpAgent {
    AcpAgent::new(
        AcpAgentConfig::new(profile.command.as_str())
            .args(profile.args.clone())
            .envs(profile.env.clone()),
    )
    .with_debug(|line, direction| {
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

/// Esegue lo spike contro il profilo scelto.
///
/// Senza `resume` apre una sessione, esegue il compito, cambia effort o modello
/// e manda un secondo prompt. Con `resume` chiude il ciclo precedente: elenca
/// le sessioni della cartella e le riprende con `session/resume` o `session/load`.
pub async fn run(
    agent: &str,
    cwd: impl AsRef<Path>,
    task: &str,
    resume: bool,
    initial_model: Option<&str>,
) -> anyhow::Result<()> {
    let task = task.trim();
    if task.is_empty() {
        anyhow::bail!(USAGE);
    }
    let profile = AgentProfile::resolve(agent)?;
    let cwd = prepare_cwd(cwd)?;
    let task = task.to_string();
    let agent_name = profile.name.clone();
    let session_agent = agent_name.clone();
    let use_cursor = profile.uses_cursor_extensions();
    println!("{}", Status::new(&agent_name, AgentState::Starting).line());
    let connected = agent_client_protocol::Client
        .builder()
        .name(CLIENT_NAME)
        .connect_with(spawn_agent(&profile), async move |connection| {
            // Il guard deve vivere per tutta la connessione: se scade prima,
            // le richieste senza sessionId restano senza risposta.
            let _cursor_extensions = if use_cursor {
                Some(
                    connection.add_dynamic_handler(cursor::ExtensionHandler::new(
                        session_agent.clone(),
                    ))?,
                )
            } else {
                None
            };
            let initialized = connection
                .send_request(initialize_request())
                .block_task()
                .await?;
            if initialized.protocol_version != ProtocolVersion::V1 {
                let version = initialized.protocol_version.as_u16();
                return Err(terminal::acp_error(format!(
                    "versione di protocollo non supportata: {version}"
                )));
            }
            println!("{}", format_initialize(&initialized));
            if resume {
                run_resume(
                    &connection,
                    &initialized,
                    &cwd,
                    &task,
                    initial_model,
                    &session_agent,
                )
                .await
            } else {
                run_new(&connection, &cwd, &task, initial_model, &session_agent).await
            }
        })
        .await;
    if connected.is_err() {
        println!("{}", Status::new(&agent_name, AgentState::Failed).line());
    }
    connected.context(format!("connessione con {agent_name} interrotta"))?;
    Ok(())
}

async fn run_new<Link>(
    connection: &ConnectionTo<Link>,
    cwd: &Path,
    task: &str,
    initial_model: Option<&str>,
    agent: &str,
) -> Result<(), Error>
where
    Link: HasPeer<Agent> + HasPeer<Link>,
{
    connection
        .build_session(cwd)
        .block_task()
        .run_until(async |mut session| {
            println!("sessione: {}", json_text(session.session_id()));
            println!(
                "{}",
                format_session(session.config_options(), session.modes())
            );
            if let Some(model) = initial_model {
                apply_initial_model(&mut session, model).await?;
            }
            let first = drive_turn(&mut session, task, None, agent).await?;
            apply_config_change(&mut session, first.config_options.as_deref(), agent).await
        })
        .await
}

async fn run_resume<Link>(
    connection: &ConnectionTo<Link>,
    initialized: &InitializeResponse,
    cwd: &Path,
    task: &str,
    initial_model: Option<&str>,
    agent: &str,
) -> Result<(), Error>
where
    Link: HasPeer<Agent> + HasPeer<Link>,
{
    let capabilities = &initialized.agent_capabilities;
    if capabilities.session_capabilities.list.is_none() {
        return Err(terminal::acp_error("l'agente non supporta session/list"));
    }
    let method = sessions::choose_restore(
        capabilities.session_capabilities.resume.is_some(),
        capabilities.load_session,
    )
    .ok_or_else(|| terminal::acp_error("l'agente non supporta session/resume né session/load"))?;
    let listed = list_sessions(connection, cwd).await?;
    println!("{}", sessions::format_session_list(&listed));
    let matches = sessions::matching_sessions(&listed, cwd);
    let chosen = sessions::latest_session(&matches)
        .ok_or_else(|| terminal::acp_error(format!("nessuna sessione per {}", cwd.display())))?;
    let session_id = chosen.session_id.clone();
    let method_name = match method {
        sessions::RestoreMethod::Resume => "session/resume",
        sessions::RestoreMethod::Load => "session/load",
    };
    println!("ripresa: {method_name} {}", json_text(&session_id));
    let session = match method {
        sessions::RestoreMethod::Resume => connection
            .resume_session(session_id, cwd)
            .block_task()
            .start_session()
            .await?
            .into_session(),
        sessions::RestoreMethod::Load => connection
            .load_session(session_id, cwd)
            .block_task()
            .start_session()
            .await?
            .into_session(),
    };
    prompt_restored(session, task, initial_model, agent).await
}

async fn list_sessions<Link>(
    connection: &ConnectionTo<Link>,
    cwd: &Path,
) -> Result<Vec<SessionInfo>, Error>
where
    Link: HasPeer<Agent> + HasPeer<Link>,
{
    let mut cursor = None;
    let mut sessions = Vec::new();
    for _ in 0..32 {
        let mut request = ListSessionsRequest::new().cwd(cwd.to_path_buf());
        if let Some(token) = cursor.clone() {
            request = request.cursor(token);
        }
        let response = connection.send_request(request).block_task().await?;
        let next = response.next_cursor.filter(|token| !token.is_empty());
        sessions.extend(response.sessions);
        match next {
            Some(token) => cursor = Some(token),
            None => return Ok(sessions),
        }
    }
    println!("sessioni: elenco troncato");
    Ok(sessions)
}

async fn prompt_restored<Link>(
    mut session: ActiveSession<'_, Link>,
    task: &str,
    initial_model: Option<&str>,
    agent: &str,
) -> Result<(), Error>
where
    Link: HasPeer<Agent> + HasPeer<Link>,
{
    println!(
        "{}",
        format_session(session.config_options(), session.modes())
    );
    if let Some(model) = initial_model {
        apply_initial_model(&mut session, model).await?;
    }
    show_restored_conversation(&mut session, agent).await?;
    drive_turn(&mut session, task, None, agent).await?;
    Ok(())
}

const RESTORE_LIMIT: usize = 256;

/// Stampa gli aggiornamenti già in coda dopo `session/load`.
///
/// `session/resume` non riproduce la conversazione: la coda è vuota e il
/// turno successivo continua la stessa sessione. `session/load` mette il
/// replay in coda prima della risposta, quindi è già pronto.
async fn show_restored_conversation<Link>(
    session: &mut ActiveSession<'_, Link>,
    agent: &str,
) -> Result<(), Error>
where
    Link: HasPeer<Agent> + HasPeer<Link>,
{
    let mut restored = 0usize;
    let mut turn = empty_turn();
    loop {
        if restored == RESTORE_LIMIT {
            println!("conversazione: interrompo la lettura degli aggiornamenti già pronti");
            break;
        }
        match next_ready(session)? {
            Some(SessionMessage::SessionMessage(dispatch)) => {
                print_dispatch(dispatch, None, &mut turn, agent).await?;
                restored += 1;
            }
            Some(SessionMessage::StopReason(reason)) => {
                println!("{}", status::status_from_stop(agent, &reason).line());
                restored += 1;
            }
            Some(_) => {
                println!("{}", status::status_unknown(agent).line());
                restored += 1;
            }
            None => break,
        }
    }
    if restored == 0 {
        println!("conversazione: nessun aggiornamento già pronto, il turno successivo la continua");
    } else {
        println!("conversazione: {restored} aggiornamenti ripresi");
    }
    Ok(())
}

fn next_ready<Link>(session: &mut ActiveSession<'_, Link>) -> Result<Option<SessionMessage>, Error>
where
    Link: HasPeer<Agent> + HasPeer<Link>,
{
    let pending = session.read_update();
    let mut pending = std::pin::pin!(pending);
    let waker = std::task::Waker::noop();
    let mut context = std::task::Context::from_waker(waker);
    match pending.as_mut().poll(&mut context) {
        std::task::Poll::Ready(message) => Ok(Some(message?)),
        std::task::Poll::Pending => Ok(None),
    }
}

struct TurnRecord {
    agent_text: String,
    config_options: Option<Vec<SessionConfigOption>>,
    config_updates: Vec<Option<String>>,
}

fn empty_turn() -> TurnRecord {
    TurnRecord {
        agent_text: String::new(),
        config_options: None,
        config_updates: Vec::new(),
    }
}

async fn drive_turn<Link>(
    session: &mut ActiveSession<'_, Link>,
    task: &str,
    watch_id: Option<&str>,
    agent: &str,
) -> Result<TurnRecord, Error>
where
    Link: HasPeer<Agent> + HasPeer<Link>,
{
    session.send_prompt(task)?;
    let mut turn = empty_turn();
    loop {
        match session.read_update().await? {
            SessionMessage::SessionMessage(dispatch) => {
                print_dispatch(dispatch, watch_id, &mut turn, agent).await?;
            }
            SessionMessage::StopReason(reason) => {
                println!("{}", status::status_from_stop(agent, &reason).line());
                break;
            }
            _ => println!("{}", status::status_unknown(agent).line()),
        }
    }
    Ok(turn)
}

async fn apply_config_change<Link>(
    session: &mut ActiveSession<'_, Link>,
    declared: Option<&[SessionConfigOption]>,
    agent: &str,
) -> Result<(), Error>
where
    Link: HasPeer<Agent> + HasPeer<Link>,
{
    let options = declared.unwrap_or_else(|| session.config_options().unwrap_or(&[]));
    let Some(change) = config_change::pick_config_change(options) else {
        println!("nessuna opzione alternativa di modello o effort");
        return Ok(());
    };
    println!(
        "cambio {}: {} {} → {}",
        change.category, change.id, change.from, change.to
    );
    let request = SetSessionConfigOptionRequest::new(
        session.session_id().clone(),
        change.id.clone(),
        change.to.as_str(),
    );
    let sent = session.connection().send_request(request);
    let response = sent.block_task().await?;
    println!(
        "{}",
        format_session(Some(&response.config_options), session.modes())
    );
    let applied = config_change::current_option_value(&response.config_options, &change.id);
    match &applied {
        Some(value) if value == &change.to => println!("impostato: {}={}", change.id, value),
        Some(value) => println!("impostato: l'agente ha lasciato {}={}", change.id, value),
        None => println!("impostato: l'agente non ha restituito {}", change.id),
    }
    let turn = drive_turn(
        session,
        config_change::verification_prompt(),
        Some(&change.id),
        agent,
    )
    .await?;
    let reply = status::one_line(&turn.agent_text);
    if !reply.is_empty() {
        println!("risposta agente: {reply}");
    }
    if config_change::turn_uses(applied.as_deref(), &turn.config_updates, &change.to) {
        println!(
            "verifica: il turno successivo usa {}={}",
            change.id, change.to
        );
    } else {
        let shown = config_change::effective_value(applied.as_deref(), &turn.config_updates)
            .unwrap_or_else(|| "assente".to_string());
        println!(
            "verifica: il turno successivo non usa {}={} (valore={shown})",
            change.id, change.to
        );
    }
    Ok(())
}

/// Imposta l'opzione di categoria `model` prima del primo prompt.
///
/// Serve a chi vuole partire con un valore preciso: per esempio `auto`
/// su Cursor, il cui ACP fattura male i modelli espliciti.
async fn apply_initial_model<Link>(
    session: &mut ActiveSession<'_, Link>,
    value: &str,
) -> Result<(), Error>
where
    Link: HasPeer<Agent> + HasPeer<Link>,
{
    let options = session.config_options().unwrap_or(&[]);
    let Some(option) = options
        .iter()
        .find(|option| matches!(option.category, Some(SessionConfigOptionCategory::Model)))
    else {
        return Err(terminal::acp_error(
            "l'agente non dichiara un'opzione model",
        ));
    };
    let id = json_text(&option.id);
    let request =
        SetSessionConfigOptionRequest::new(session.session_id().clone(), option.id.clone(), value);
    let response = session
        .connection()
        .send_request(request)
        .block_task()
        .await?;
    let applied = config_change::current_option_value(&response.config_options, &id);
    match applied.as_deref() {
        Some(current) if current == value => println!("model iniziale: impostato {id}={current}"),
        Some(current) => println!("model iniziale: l'agente ha lasciato {id}={current}"),
        None => println!("model iniziale: l'agente non ha restituito {id}"),
    }
    Ok(())
}

async fn print_dispatch(
    dispatch: Dispatch,
    watch_id: Option<&str>,
    turn: &mut TurnRecord,
    agent: &str,
) -> Result<(), Error> {
    agent_client_protocol::util::MatchDispatch::new(dispatch)
        .if_request(async |request: RequestPermissionRequest, responder| {
            match answer_permission(agent, &request).await {
                Ok(response) => responder.respond(response),
                Err(error) => {
                    println!("{error}");
                    responder.respond(permission::cancelled_permission())?;
                    Err(error)
                }
            }
        })
        .await
        .if_request(async |request: cursor::AskQuestionRequest, responder| {
            match cursor::answer_question(agent, &request).await {
                Ok(response) => responder.respond(response),
                Err(error) => {
                    println!("cursor/ask_question: {error}");
                    responder.respond(cursor::cancelled_question())?;
                    Err(error)
                }
            }
        })
        .await
        .if_request(
            async |request: cursor::CreatePlanRequest, responder| match cursor::answer_plan(
                agent, &request,
            )
            .await
            {
                Ok(response) => responder.respond(response),
                Err(error) => {
                    println!("cursor/create_plan: {error}");
                    responder.respond(cursor::cancelled_plan())?;
                    Err(error)
                }
            },
        )
        .await
        .if_notification(async |notification: SessionNotification| {
            record_update(&notification.update, watch_id, turn);
            println!(
                "{}",
                status::status_from_update(agent, &notification.update).line()
            );
            Ok(())
        })
        .await
        .otherwise(async |dispatch| match dispatch {
            Dispatch::Request(request, responder) if request.method().starts_with("cursor/") => {
                println!(
                    "{}",
                    status::status_awaiting_reply(
                        agent,
                        &format!("estensione {}", request.method())
                    )
                    .line()
                );
                responder.respond_with_internal_error(format!(
                    "metodo non gestito: {}",
                    request.method()
                ))
            }
            _ => Ok(()),
        })
        .await
}

async fn answer_permission(
    agent: &str,
    request: &RequestPermissionRequest,
) -> Result<agent_client_protocol::schema::v1::RequestPermissionResponse, Error> {
    println!("{}", status::status_from_permission(agent, request).line());
    terminal::require_terminal("chiedere il permesso")?;
    if request.options.is_empty() {
        return Err(terminal::acp_error(
            "l'agente non ha inviato opzioni di permesso",
        ));
    }
    loop {
        println!("permesso? [sì/no/sempre]");
        terminal::flush_stdout();
        let line = terminal::read_stdin_line().await?;
        let Some(choice) = permission::parse_permission_choice(&line) else {
            println!("risposta non valida: scrivi sì, no o sempre");
            continue;
        };
        let Some(option) = permission::select_permission_option(&request.options, choice) else {
            println!("nessuna opzione dell'agente corrisponde a questa risposta");
            println!(
                "{}",
                permission::format_permission_options(&request.options)
            );
            continue;
        };
        println!(
            "scelta: {} id={}",
            permission_kind(option),
            json_text(&option.option_id)
        );
        return Ok(permission::permission_response(json_text(
            &option.option_id,
        )));
    }
}

fn permission_kind(option: &agent_client_protocol::schema::v1::PermissionOption) -> &'static str {
    use agent_client_protocol::schema::v1::PermissionOptionKind;
    match option.kind {
        PermissionOptionKind::AllowOnce => "allow_once",
        PermissionOptionKind::AllowAlways => "allow_always",
        PermissionOptionKind::RejectOnce => "reject_once",
        PermissionOptionKind::RejectAlways => "reject_always",
        _ => "altro",
    }
}

fn record_update(update: &SessionUpdate, watch_id: Option<&str>, turn: &mut TurnRecord) {
    match update {
        SessionUpdate::AgentMessageChunk(chunk) => {
            if let ContentBlock::Text(text) = &chunk.content {
                turn.agent_text.push_str(&text.text);
            }
        }
        SessionUpdate::ConfigOptionUpdate(update) => {
            turn.config_options = Some(update.config_options.clone());
            if let Some(id) = watch_id {
                turn.config_updates
                    .push(config_change::current_option_value(
                        &update.config_options,
                        id,
                    ));
            }
        }
        _ => {}
    }
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
            lines.push(format!("corrente: {}", json_text(&modes.current_mode_id)));
            lines.extend(
                modes
                    .available_modes
                    .iter()
                    .map(|mode| format!("- id={} nome={}", json_text(&mode.id), mode.name)),
            );
        }
    }

    lines.join("\n")
}

fn format_auth_method(method: &AuthMethod) -> String {
    let mut line = format!("- id={} nome={}", json_text(method.id()), method.name());
    if let Some(description) = method.description() {
        line.push_str(" descrizione=");
        line.push_str(description);
    }
    line
}

fn format_option(option: &SessionConfigOption) -> String {
    let mut line = format!(
        "- id={} nome={} categoria={} valore={}",
        json_text(&option.id),
        option.name,
        category_label(&option.category),
        current_value(&option.kind)
    );
    if let Some(values) = select_values(&option.kind) {
        line.push_str(&format!(" valori=[{values}]"));
    }
    line
}

/// Valori dichiarati di un selettore, piatti o raggruppati.
fn select_values(kind: &SessionConfigKind) -> Option<String> {
    let SessionConfigKind::Select(select) = kind else {
        return None;
    };
    let ids: Vec<String> = match &select.options {
        SessionConfigSelectOptions::Ungrouped(options) => options
            .iter()
            .map(|option| json_text(&option.value))
            .collect(),
        SessionConfigSelectOptions::Grouped(groups) => groups
            .iter()
            .flat_map(|group| group.options.iter())
            .map(|option| json_text(&option.value))
            .collect(),
        _ => Vec::new(),
    };
    (!ids.is_empty()).then(|| ids.join(" "))
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
        SessionConfigKind::Select(select) => json_text(&select.current_value),
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

    #[test]
    fn parses_a_new_session_and_a_resume() {
        let created = parse_invocation(["/tmp/proj", "aggiungi", "un test"]).unwrap();
        assert!(!created.resume);
        assert_eq!(created.agent, "opencode");
        assert_eq!(created.cwd, "/tmp/proj");
        assert_eq!(created.task, "aggiungi un test");

        let resumed = parse_invocation(["--resume", "/tmp/proj", "continua"]).unwrap();
        assert!(resumed.resume);
        assert_eq!(resumed.agent, "opencode");
        assert_eq!(resumed.task, "continua");
        assert!(parse_invocation(["--resume", "/tmp/proj"]).is_err());
        assert!(parse_invocation(["--altro", "/tmp/proj", "x"]).is_err());
    }

    #[test]
    fn parses_agent_before_the_folder() {
        let cursor = parse_invocation(["--agent", "cursor", "/tmp/proj", "fai"]).unwrap();
        assert_eq!(cursor.agent, "cursor");
        assert!(!cursor.resume);
        assert_eq!(cursor.cwd, "/tmp/proj");
        assert_eq!(cursor.task, "fai");

        let resume_first =
            parse_invocation(["--resume", "--agent", "cursor", "/tmp/proj", "continua"]).unwrap();
        assert!(resume_first.resume);
        assert_eq!(resume_first.agent, "cursor");
        assert_eq!(resume_first.cwd, "/tmp/proj");
        assert_eq!(resume_first.task, "continua");

        let agent_first =
            parse_invocation(["--agent", "cursor", "--resume", "/tmp/proj", "continua"]).unwrap();
        assert!(agent_first.resume);
        assert_eq!(agent_first.agent, "cursor");
        assert_eq!(agent_first.task, "continua");

        assert!(parse_invocation(["--agent"]).is_err());
        assert!(parse_invocation(["--agent", "--resume", "/tmp/proj", "x"]).is_err());
        assert!(
            parse_invocation(["--agent", "cursor", "--agent", "opencode", "/tmp/proj", "x"])
                .is_err()
        );
        assert!(parse_invocation(["--resume", "--resume", "/tmp/proj", "x"]).is_err());
    }

    #[test]
    fn parses_model_before_the_folder() {
        let model =
            parse_invocation(["--agent", "cursor", "--model", "auto", "/tmp/proj", "fai"]).unwrap();
        assert_eq!(model.model.as_deref(), Some("auto"));
        assert_eq!(model.agent, "cursor");

        let model_first =
            parse_invocation(["--model", "auto", "--resume", "/tmp/proj", "x"]).unwrap();
        assert!(model_first.resume);
        assert_eq!(model_first.model.as_deref(), Some("auto"));

        assert_eq!(parse_invocation(["/tmp/proj", "x"]).unwrap().model, None);
        assert!(parse_invocation(["--model"]).is_err());
        assert!(parse_invocation(["--model", "--resume", "/tmp/proj", "x"]).is_err());
        assert!(
            parse_invocation(["--model", "auto", "--model", "auto", "/tmp/proj", "x"]).is_err()
        );
    }
}
