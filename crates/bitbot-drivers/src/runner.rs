//! Pilota un agente senza terminale: l'entry point del demone.
//!
//! Stessa traduzione dello spike (`status`), ma niente stdin e niente
//! stampa: gli stati vanno a un osservatore, e ogni richiesta bloccante
//! dell'agente riceve `cancelled`. Nessun permesso concesso, nessun
//! piano accettato: il motore permessi arriva in una fase successiva.

use std::cell::RefCell;
use std::path::Path;

use agent_client_protocol::role::Role;
use agent_client_protocol::schema::v1::{RequestPermissionRequest, SessionNotification};
use agent_client_protocol::schema::ProtocolVersion;
use agent_client_protocol::util::MatchDispatch;
use agent_client_protocol::{
    ConnectionTo, Dispatch, Error, HandleDispatchFrom, Handled, SessionMessage,
};
use bitbot_protocol::{AgentState, Status};

use crate::profile::AgentProfile;
use crate::status;
use crate::{cursor, initialize_request, json_text, permission, spawn_agent, terminal};

/// Esito del primo turno di una sessione pilotata.
#[derive(Debug, Clone)]
pub struct TaskOutcome {
    pub session_id: String,
    pub final_status: Status,
}

/// Apre una sessione nuova e ne pilota il primo turno fino alla fine.
///
/// `on_status` riceve ogni stato unificato man mano che arriva, compreso
/// l'avvio e l'esito finale. La funzione ritorna quando il turno finisce
/// o la connessione con l'agente cade.
pub async fn run_task(
    profile: &AgentProfile,
    cwd: &Path,
    task: &str,
    mut on_status: impl FnMut(Status) + Send,
) -> anyhow::Result<TaskOutcome> {
    on_status(Status::new(profile.name.clone(), AgentState::Starting));
    let agent_name = profile.name.clone();
    let use_cursor = profile.uses_cursor_extensions();
    let cwd = cwd.to_path_buf();
    let task = task.to_string();
    let connected = agent_client_protocol::Client
        .builder()
        .name("bitbotd")
        .connect_with(spawn_agent(profile), async move |connection| {
            let _cursor_extensions = if use_cursor {
                Some(connection.add_dynamic_handler(CancelledExtensions)?)
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
            connection
                .build_session(&cwd)
                .block_task()
                .run_until(async |mut session| {
                    let session_id = json_text(session.session_id());
                    session.send_prompt(&task)?;
                    let mut final_status = Status::new(agent_name.clone(), AgentState::Working);
                    loop {
                        match session.read_update().await? {
                            SessionMessage::SessionMessage(dispatch) => {
                                let observed = observe_dispatch(&agent_name, dispatch).await?;
                                if let Some(last) = observed.last() {
                                    final_status = last.clone();
                                }
                                for status in observed {
                                    on_status(status);
                                }
                            }
                            SessionMessage::StopReason(reason) => {
                                final_status = status::status_from_stop(&agent_name, &reason);
                                break;
                            }
                            _ => {}
                        }
                    }
                    on_status(final_status.clone());
                    Ok(TaskOutcome {
                        session_id,
                        final_status,
                    })
                })
                .await
        })
        .await;
    let agent_name = &profile.name;
    connected.map_err(|error| anyhow::anyhow!("agente {agent_name}: {error}"))
}

/// Traduce un messaggio di sessione in stati unificati e risponde
/// `cancelled` a ogni richiesta bloccante.
async fn observe_dispatch(agent: &str, dispatch: Dispatch) -> Result<Vec<Status>, Error> {
    let observed: RefCell<Vec<Status>> = RefCell::new(Vec::new());
    MatchDispatch::new(dispatch)
        .if_request(async |request: RequestPermissionRequest, responder| {
            observed
                .borrow_mut()
                .push(status::status_from_permission(agent, &request));
            responder.respond(permission::cancelled_permission())
        })
        .await
        .if_request(async |_request: cursor::AskQuestionRequest, responder| {
            observed
                .borrow_mut()
                .push(status::status_awaiting_reply(agent, "domanda"));
            responder.respond(cursor::cancelled_question())
        })
        .await
        .if_request(async |_request: cursor::CreatePlanRequest, responder| {
            observed
                .borrow_mut()
                .push(status::status_awaiting_reply(agent, "piano"));
            responder.respond(cursor::cancelled_plan())
        })
        .await
        .if_notification(async |notification: SessionNotification| {
            observed
                .borrow_mut()
                .push(status::status_from_update(agent, &notification.update));
            Ok(())
        })
        .await
        .otherwise(async |dispatch| match dispatch {
            Dispatch::Request(request, responder) if request.method().starts_with("cursor/") => {
                responder.respond_with_internal_error(format!(
                    "metodo non gestito: {}",
                    request.method()
                ))
            }
            _ => Ok(()),
        })
        .await?;
    Ok(observed.into_inner())
}

/// Le estensioni `cursor/*` senza sessionId, gestite senza terminale:
/// ricevono sempre `cancelled` e l'agente riparte.
/// shortcut: lo stato «in attesa di risposta» di queste richieste non
/// arriva all'osservatore; quando il demone avrà il motore permessi,
/// passerà da lì anche questo.
#[derive(Default)]
struct CancelledExtensions;

impl<Counterpart: Role> HandleDispatchFrom<Counterpart> for CancelledExtensions {
    async fn handle_dispatch_from(
        &mut self,
        message: Dispatch,
        connection: ConnectionTo<Counterpart>,
    ) -> Result<Handled<Dispatch>, Error> {
        if !cursor::claim_extension(message.method(), message.has_field("sessionId")) {
            return Ok(Handled::No {
                message,
                retry: false,
            });
        }
        let ask = connection.clone();
        let plan = connection;
        MatchDispatch::new(message)
            .if_request(async |_request: cursor::AskQuestionRequest, responder| {
                ask.spawn(async move { responder.respond(cursor::cancelled_question()) })?;
                Ok(())
            })
            .await
            .if_request(async |_request: cursor::CreatePlanRequest, responder| {
                plan.spawn(async move { responder.respond(cursor::cancelled_plan()) })?;
                Ok(())
            })
            .await
            .done()
    }

    fn describe_chain(&self) -> impl std::fmt::Debug {
        "cursor/ask_question, cursor/create_plan (annullate)"
    }
}
