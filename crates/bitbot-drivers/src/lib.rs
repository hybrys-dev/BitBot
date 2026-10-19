//! Client ACP per il primo spike: avvia un agente, apre una sessione e segue un compito.

mod config_change;
mod cursor;
mod host;
mod manifest;
mod permission;
mod profile;
mod sessions;
mod status;
mod terminal;

pub use host::{AskNotice, HostEvent, ReplyEnvelope, SessionHost, SessionIngress};

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
    initialize_request_named(CLIENT_NAME)
}

fn initialize_request_named(name: &str) -> InitializeRequest {
    InitializeRequest::new(ProtocolVersion::V1)
        .client_capabilities(ClientCapabilities::new())
        .client_info(Implementation::new(name, env!("CARGO_PKG_VERSION")))
}

/// Una sessione nuova: un prompt, modello opzionale, niente verifica di configurazione.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewSession {
    pub agent: String,
    pub cwd: PathBuf,
    pub prompt: String,
    pub model: Option<String>,
}

/// Avvia l'agente del profilo come processo figlio.
///
/// Stdin e stdout restano il trasporto JSON-RPC. Lo stderr del figlio viene
/// inoltrato sullo stderr di questo processo. L'ambiente del profilo si aggiunge
/// a quello ereditato: una mappa vuota lascia il processo come prima.
fn spawn_agent(profile: &AgentProfile) -> AcpAgent {
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
    Ok(plain_cwd(cwd))
}

/// `canonicalize` su Windows antepone `\\?\`. Gli agenti non accettano
/// quel prefisso come cartella di `session/new`.
fn plain_cwd(path: PathBuf) -> PathBuf {
    #[cfg(windows)]
    {
        let Some(text) = path.to_str() else {
            return path;
        };
        PathBuf::from(strip_verbatim_prefix(text))
    }
    #[cfg(not(windows))]
    {
        path
    }
}

#[cfg(any(windows, test))]
fn strip_verbatim_prefix(path: &str) -> String {
    if let Some(rest) = path.strip_prefix(r"\\?\UNC\") {
        format!(r"\\{rest}")
    } else if let Some(rest) = path.strip_prefix(r"\\?\") {
        rest.to_string()
    } else {
        path.to_string()
    }
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
    drive(
        agent,
        cwd.as_ref(),
        SessionHost::terminal(),
        SessionJob::Spike {
            resume,
            task: task.to_string(),
            model: initial_model.map(str::to_string),
        },
    )
    .await
}

/// Apre una sessione ACP, manda un solo prompt e pubblica stato e richieste su `host`.
///
/// Non cambia la configurazione da solo e non riprende una sessione già chiusa:
/// quello resta dello spike. Se la connessione cade, emette lo stato `errore`.
pub async fn run_new_session(spec: NewSession, host: SessionHost) -> anyhow::Result<()> {
    let prompt = spec.prompt.trim();
    if prompt.is_empty() {
        anyhow::bail!("il prompt è vuoto");
    }
    drive(
        &spec.agent,
        &spec.cwd,
        host,
        SessionJob::Once {
            task: prompt.to_string(),
            model: spec.model,
        },
    )
    .await
}

enum SessionJob {
    Spike {
        resume: bool,
        task: String,
        model: Option<String>,
    },
    Once {
        task: String,
        model: Option<String>,
    },
}

async fn drive(agent: &str, cwd: &Path, host: SessionHost, job: SessionJob) -> anyhow::Result<()> {
    let profile = AgentProfile::resolve(agent)?;
    let cwd = prepare_cwd(cwd)?;
    let agent_name = profile.name.clone();
    let failure_name = agent_name.clone();
    let client_name = host.client_name();
    host.emit(&Status::new(&agent_name, AgentState::Starting));
    let host_for_failure = host.clone();
    let connected = agent_client_protocol::Client
        .builder()
        .name(client_name)
        .connect_with(spawn_agent(&profile), async move |connection| {
            // Il guard deve vivere per tutta la connessione: se scade prima,
            // le richieste senza sessionId restano senza risposta.
            let _cursor_extensions = if profile.uses_cursor_extensions() {
                Some(
                    connection.add_dynamic_handler(cursor::ExtensionHandler::new(
                        agent_name.clone(),
                        host.clone(),
                    ))?,
                )
            } else {
                None
            };
            let initialized = connection
                .send_request(initialize_request_named(client_name))
                .block_task()
                .await?;
            if initialized.protocol_version != ProtocolVersion::V1 {
                let version = initialized.protocol_version.as_u16();
                return Err(terminal::acp_error(format!(
                    "versione di protocollo non supportata: {version}"
                )));
            }
            match job {
                SessionJob::Spike {
                    resume,
                    task,
                    model,
                } => {
                    println!("{}", format_initialize(&initialized));
                    if resume {
                        run_resume(
                            &connection,
                            &initialized,
                            &cwd,
                            &task,
                            model.as_deref(),
                            &agent_name,
                            &host,
                        )
                        .await
                    } else {
                        run_new(
                            &connection,
                            &cwd,
                            &task,
                            model.as_deref(),
                            &agent_name,
                            &host,
                        )
                        .await
                    }
                }
                SessionJob::Once { task, model } => {
                    run_once(
                        &connection,
                        &cwd,
                        &task,
                        model.as_deref(),
                        &agent_name,
                        &host,
                    )
                    .await
                }
            }
        })
        .await;
    if connected.is_err() {
        host_for_failure.emit(&Status::new(&failure_name, AgentState::Failed));
    }
    connected.context(format!("connessione con {failure_name} interrotta"))?;
    Ok(())
}

async fn run_new<Link>(
    connection: &ConnectionTo<Link>,
    cwd: &Path,
    task: &str,
    initial_model: Option<&str>,
    agent: &str,
    host: &SessionHost,
) -> Result<(), Error>
where
    Link: HasPeer<Agent> + HasPeer<Link>,
{
    let host = host.clone();
    connection
        .build_session(cwd)
        .block_task()
        .run_until(async move |mut session| {
            println!("sessione: {}", json_text(session.session_id()));
            println!(
                "{}",
                format_session(session.config_options(), session.modes())
            );
            if let Some(model) = initial_model {
                let set = apply_initial_model(&mut session, model).await?;
                println!("{}", set.spike_line());
            }
            let first = drive_turn(&mut session, task, None, agent, &host).await?;
            apply_config_change(&mut session, first.config_options.as_deref(), agent, &host).await
        })
        .await
}

async fn run_once<Link>(
    connection: &ConnectionTo<Link>,
    cwd: &Path,
    task: &str,
    initial_model: Option<&str>,
    agent: &str,
    host: &SessionHost,
) -> Result<(), Error>
where
    Link: HasPeer<Agent> + HasPeer<Link>,
{
    let host = host.clone();
    let agent = agent.to_string();
    connection
        .build_session(cwd)
        .block_task()
        .run_until(async move |mut session| {
            if let Some(model) = initial_model {
                let set = apply_initial_model(&mut session, model).await?;
                host.emit(
                    &Status::new(&agent, AgentState::Working)
                        .with_action(bitbot_protocol::Action::other(set.status_action())),
                );
            }
            drive_turn(&mut session, task, None, &agent, &host).await?;
            Ok(())
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
    host: &SessionHost,
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
    prompt_restored(session, task, initial_model, agent, host).await
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
    host: &SessionHost,
) -> Result<(), Error>
where
    Link: HasPeer<Agent> + HasPeer<Link>,
{
    println!(
        "{}",
        format_session(session.config_options(), session.modes())
    );
    if let Some(model) = initial_model {
        let set = apply_initial_model(&mut session, model).await?;
        println!("{}", set.spike_line());
    }
    show_restored_conversation(&mut session, agent, host).await?;
    drive_turn(&mut session, task, None, agent, host).await?;
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
    host: &SessionHost,
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
                print_dispatch(dispatch, None, &mut turn, agent, host).await?;
                restored += 1;
            }
            Some(SessionMessage::StopReason(reason)) => {
                host.emit(&status::status_from_stop(agent, &reason));
                restored += 1;
            }
            Some(_) => {
                host.emit(&status::status_unknown(agent));
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
    host: &SessionHost,
) -> Result<TurnRecord, Error>
where
    Link: HasPeer<Agent> + HasPeer<Link>,
{
    session.send_prompt(task)?;
    let mut turn = empty_turn();
    loop {
        match session.read_update().await? {
            SessionMessage::SessionMessage(dispatch) => {
                print_dispatch(dispatch, watch_id, &mut turn, agent, host).await?;
            }
            SessionMessage::StopReason(reason) => {
                host.emit(&status::status_from_stop(agent, &reason));
                break;
            }
            _ => host.emit(&status::status_unknown(agent)),
        }
    }
    Ok(turn)
}

async fn apply_config_change<Link>(
    session: &mut ActiveSession<'_, Link>,
    declared: Option<&[SessionConfigOption]>,
    agent: &str,
    host: &SessionHost,
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
        host,
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

struct ModelSet {
    id: String,
    requested: String,
    applied: Option<String>,
}

impl ModelSet {
    fn spike_line(&self) -> String {
        match self.applied.as_deref() {
            Some(current) if current == self.requested => {
                format!("model iniziale: impostato {}={current}", self.id)
            }
            Some(current) => format!("model iniziale: l'agente ha lasciato {}={current}", self.id),
            None => format!("model iniziale: l'agente non ha restituito {}", self.id),
        }
    }

    fn status_action(&self) -> String {
        match self.applied.as_deref() {
            Some(current) => format!("modello {}={current}", self.id),
            None => format!("modello {} assente", self.id),
        }
    }
}

/// Imposta l'opzione di categoria `model` prima del primo prompt.
///
/// Serve a chi vuole partire con un valore preciso: per esempio `auto`
/// su Cursor, il cui ACP fattura male i modelli espliciti.
async fn apply_initial_model<Link>(
    session: &mut ActiveSession<'_, Link>,
    value: &str,
) -> Result<ModelSet, Error>
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
    Ok(ModelSet {
        id,
        requested: value.to_string(),
        applied,
    })
}

async fn print_dispatch(
    dispatch: Dispatch,
    watch_id: Option<&str>,
    turn: &mut TurnRecord,
    agent: &str,
    host: &SessionHost,
) -> Result<(), Error> {
    let host = host.clone();
    agent_client_protocol::util::MatchDispatch::new(dispatch)
        .if_request(async |request: RequestPermissionRequest, responder| {
            let host = host.clone();
            match settle_permission(&host, agent, &request).await {
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
            let host = host.clone();
            match cursor::settle_question(&host, agent, &request).await {
                Ok(response) => responder.respond(response),
                Err(error) => {
                    println!("cursor/ask_question: {error}");
                    responder.respond(cursor::cancelled_question())?;
                    Err(terminal::acp_error(error))
                }
            }
        })
        .await
        .if_request(async |request: cursor::CreatePlanRequest, responder| {
            let host = host.clone();
            match cursor::settle_plan(&host, agent, &request).await {
                Ok(response) => responder.respond(response),
                Err(error) => {
                    println!("cursor/create_plan: {error}");
                    responder.respond(cursor::cancelled_plan())?;
                    Err(terminal::acp_error(error))
                }
            }
        })
        .await
        .if_notification(async |notification: SessionNotification| {
            record_update(&notification.update, watch_id, turn);
            host.emit(&status::status_from_update(agent, &notification.update));
            Ok(())
        })
        .await
        .otherwise(async |dispatch| match dispatch {
            Dispatch::Request(request, responder) if request.method().starts_with("cursor/") => {
                host.emit(&status::status_awaiting_reply(
                    agent,
                    &format!("estensione {}", request.method()),
                ));
                responder.respond_with_internal_error(format!(
                    "metodo non gestito: {}",
                    request.method()
                ))
            }
            _ => Ok(()),
        })
        .await
}

async fn settle_permission(
    host: &SessionHost,
    agent: &str,
    request: &RequestPermissionRequest,
) -> Result<agent_client_protocol::schema::v1::RequestPermissionResponse, Error> {
    host.emit(&status::status_from_permission(agent, request));
    let options = request.options.clone();
    let body = permission::permission_body(&request.options);
    let reply = host
        .ask(body, move |reply| {
            permission::permission_decision(&options, reply).map(|_| ())
        })
        .await
        .map_err(terminal::acp_error)?;
    permission::permission_decision(&request.options, &reply).map_err(terminal::acp_error)
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
        assert!(
            !cwd.to_string_lossy().starts_with(r"\\?\"),
            "{}",
            cwd.display()
        );

        let file = std::env::temp_dir().join("bitbot-spike-not-a-directory");
        std::fs::write(&file, "x").unwrap();
        let error = prepare_cwd(&file).unwrap_err();
        let _ = std::fs::remove_file(&file);
        assert!(error.to_string().contains("non è una cartella"));
    }

    #[test]
    fn verbatim_windows_prefix_is_stripped() {
        assert_eq!(super::strip_verbatim_prefix(r"\\?\C:\repo"), r"C:\repo");
        assert_eq!(
            super::strip_verbatim_prefix(r"\\?\UNC\server\share\repo"),
            r"\\server\share\repo"
        );
        assert_eq!(super::strip_verbatim_prefix(r"C:\repo"), r"C:\repo");
        assert_eq!(super::strip_verbatim_prefix("/tmp/repo"), "/tmp/repo");
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
