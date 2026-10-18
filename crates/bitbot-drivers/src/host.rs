//! Ingresso condiviso fra lo spike e il demone.
//!
//! Lo spike stampa lo stato e legge il terminale. Il demone inoltra gli
//! stessi eventi sul socket e aspetta `session/respond`. Il giro ACP non
//! sa chi dei due sta rispondendo.

use bitbot_protocol::{SessionReply, SessionRequestBody, Status};
use tokio::sync::{mpsc, oneshot};

use crate::cursor;
use crate::permission;

/// Messaggio dal giro ACP verso chi possiede la sessione.
pub enum HostEvent {
    Status(Status),
    Ask(AskNotice),
}

/// Richiesta dell'agente, con il canale per la risposta.
pub struct AskNotice {
    pub body: SessionRequestBody,
    pub reply_tx: mpsc::UnboundedSender<ReplyEnvelope>,
}

/// Scelta del client, e conferma che il driver l'ha accettata.
pub struct ReplyEnvelope {
    pub reply: SessionReply,
    pub ack: oneshot::Sender<Result<(), String>>,
}

/// Lato ricezione di [`SessionHost::channel`].
pub struct SessionIngress {
    pub events: mpsc::UnboundedReceiver<HostEvent>,
}

/// Chi riceve gli stati e risponde alle richieste dell'agente.
#[derive(Clone)]
pub struct SessionHost {
    client_name: &'static str,
    inner: HostInner,
}

#[derive(Clone)]
enum HostInner {
    Terminal,
    Channel {
        events: mpsc::UnboundedSender<HostEvent>,
    },
}

impl SessionHost {
    /// Spike: stampa la riga di stato e chiede sul terminale.
    pub fn terminal() -> Self {
        Self {
            client_name: "bitbot-spike",
            inner: HostInner::Terminal,
        }
    }

    /// Demone: stati e richieste escono da [`SessionIngress`].
    pub fn channel() -> (Self, SessionIngress) {
        let (events, receiver) = mpsc::unbounded_channel();
        (
            Self {
                client_name: "bitbotd",
                inner: HostInner::Channel { events },
            },
            SessionIngress { events: receiver },
        )
    }

    /// Nome mandato in `initialize`, diverso fra spike e demone.
    pub fn client_name(&self) -> &'static str {
        self.client_name
    }

    /// Pubblica uno stato unificato.
    pub fn emit(&self, status: &Status) {
        match &self.inner {
            HostInner::Terminal => println!("{}", status.line()),
            HostInner::Channel { events } => {
                let _ = events.send(HostEvent::Status(status.clone()));
            }
        }
    }

    /// Chiede una decisione. `accept` rifiuta una scelta che l'agente non
    /// può applicare: sul canale si riprova, sul terminale è un errore
    /// perché il prompt ha già filtrato.
    pub async fn ask<F>(&self, body: SessionRequestBody, accept: F) -> Result<SessionReply, String>
    where
        F: Fn(&SessionReply) -> Result<(), String>,
    {
        match &self.inner {
            HostInner::Terminal => {
                let reply = interact(&body).await?;
                accept(&reply)?;
                Ok(reply)
            }
            HostInner::Channel { events } => {
                let (reply_tx, mut replies) = mpsc::unbounded_channel();
                if events
                    .send(HostEvent::Ask(AskNotice { body, reply_tx }))
                    .is_err()
                {
                    return Ok(SessionReply::cancelled());
                }
                loop {
                    let Some(envelope) = replies.recv().await else {
                        return Ok(SessionReply::cancelled());
                    };
                    match accept(&envelope.reply) {
                        Ok(()) => {
                            let _ = envelope.ack.send(Ok(()));
                            return Ok(envelope.reply);
                        }
                        Err(message) => {
                            let _ = envelope.ack.send(Err(message));
                        }
                    }
                }
            }
        }
    }
}

async fn interact(body: &SessionRequestBody) -> Result<SessionReply, String> {
    match body {
        SessionRequestBody::Permission { options } => permission::prompt_permission(options).await,
        SessionRequestBody::Question {
            call_id,
            title,
            questions,
        } => cursor::prompt_question(call_id.as_deref(), title.as_deref(), questions).await,
        SessionRequestBody::Plan {
            call_id,
            name,
            overview,
            plan,
            is_project,
            todos,
            phases,
        } => {
            cursor::prompt_plan(
                call_id.as_deref(),
                name.as_deref(),
                overview.as_deref(),
                plan,
                *is_project,
                todos,
                phases,
            )
            .await
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bitbot_protocol::{AgentState, PermissionOptionView, RespondOutcome};

    fn allow_once() -> SessionRequestBody {
        SessionRequestBody::Permission {
            options: vec![PermissionOptionView {
                id: "once".to_string(),
                name: "Allow once".to_string(),
                kind: "allow_once".to_string(),
            }],
        }
    }

    #[tokio::test]
    async fn dropped_listener_cancels_the_request() {
        let (host, ingress) = SessionHost::channel();
        drop(ingress);
        let reply = host.ask(allow_once(), |_| Ok(())).await.unwrap();
        assert_eq!(reply.outcome, RespondOutcome::Cancelled);
    }

    #[tokio::test]
    async fn status_precedes_the_ask_and_a_bad_reply_can_be_retried() {
        let (host, mut ingress) = SessionHost::channel();
        let host_task = tokio::spawn(async move {
            host.emit(&Status::new("opencode", AgentState::AwaitingPermission));
            host.ask(allow_once(), |reply| match reply.outcome {
                RespondOutcome::AllowOnce | RespondOutcome::Cancelled => Ok(()),
                _ => Err("esito non valido per un permesso".to_string()),
            })
            .await
        });

        let HostEvent::Status(status) = ingress.events.recv().await.unwrap() else {
            panic!("il primo messaggio deve essere lo stato");
        };
        assert_eq!(status.state, AgentState::AwaitingPermission);

        let HostEvent::Ask(ask) = ingress.events.recv().await.unwrap() else {
            panic!("la richiesta segue lo stato");
        };
        let (ack_tx, ack_rx) = oneshot::channel();
        ask.reply_tx
            .send(ReplyEnvelope {
                reply: SessionReply {
                    outcome: RespondOutcome::Accepted,
                    answers: Vec::new(),
                },
                ack: ack_tx,
            })
            .unwrap();
        let rejected = ack_rx.await.unwrap().unwrap_err();
        assert!(rejected.contains("permesso"), "{rejected}");

        let (ack_tx, ack_rx) = oneshot::channel();
        ask.reply_tx
            .send(ReplyEnvelope {
                reply: SessionReply {
                    outcome: RespondOutcome::AllowOnce,
                    answers: Vec::new(),
                },
                ack: ack_tx,
            })
            .unwrap();
        assert!(ack_rx.await.unwrap().is_ok());
        let reply = host_task.await.unwrap().unwrap();
        assert_eq!(reply.outcome, RespondOutcome::AllowOnce);
    }
}
