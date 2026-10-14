//! Estensioni bloccanti di Cursor: `cursor/ask_question` e `cursor/create_plan`.

use agent_client_protocol::role::Role;
use agent_client_protocol::util::MatchDispatch;
use agent_client_protocol::{
    ConnectionTo, Dispatch, HandleDispatchFrom, Handled, JsonRpcMessage, JsonRpcRequest,
    JsonRpcResponse,
};
use serde::{Deserialize, Serialize};

use crate::status;
use crate::terminal::{flush_stdout, read_stdin_line, require_terminal};

#[derive(Debug, Clone, Serialize, Deserialize, JsonRpcRequest)]
#[request(method = "cursor/ask_question", response = AskQuestionResponse)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AskQuestionRequest {
    tool_call_id: String,
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    questions: Vec<Question>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
struct Question {
    id: String,
    prompt: String,
    #[serde(default)]
    options: Vec<QuestionOption>,
    #[serde(default)]
    allow_multiple: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
struct QuestionOption {
    id: String,
    label: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonRpcResponse, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AskQuestionResponse {
    outcome: AskOutcome,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "outcome", rename_all = "snake_case")]
enum AskOutcome {
    Answered {
        answers: Vec<AskAnswer>,
    },
    Skipped {
        #[serde(skip_serializing_if = "Option::is_none")]
        reason: Option<String>,
    },
    Cancelled,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
struct AskAnswer {
    question_id: String,
    selected_option_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Selection {
    Skip,
    Ids(Vec<String>),
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonRpcRequest)]
#[request(method = "cursor/create_plan", response = CreatePlanResponse)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CreatePlanRequest {
    tool_call_id: String,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    overview: Option<String>,
    plan: String,
    #[serde(default)]
    todos: Vec<PlanTodo>,
    #[serde(default)]
    is_project: Option<bool>,
    #[serde(default)]
    phases: Vec<PlanPhase>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
struct PlanTodo {
    id: String,
    content: String,
    status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
struct PlanPhase {
    name: String,
    #[serde(default)]
    todos: Vec<PlanTodo>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonRpcResponse, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CreatePlanResponse {
    outcome: PlanOutcome,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "outcome", rename_all = "snake_case")]
enum PlanOutcome {
    Accepted {
        #[serde(skip_serializing_if = "Option::is_none")]
        plan_uri: Option<String>,
    },
    Rejected {
        #[serde(skip_serializing_if = "Option::is_none")]
        reason: Option<String>,
    },
    Cancelled,
}

pub(crate) async fn answer_question(
    request: &AskQuestionRequest,
) -> Result<AskQuestionResponse, agent_client_protocol::Error> {
    let detail = request
        .questions
        .first()
        .map(|question| format!("domanda {}", question.prompt))
        .unwrap_or_else(|| "domanda".to_string());
    println!("{}", status::status_awaiting_reply(&detail).line());
    println!("chiamata: {}", request.tool_call_id);
    require_terminal("fare la domanda")?;
    if request.questions.is_empty() {
        return Err(crate::terminal::acp_error("la domanda non ha voci"));
    }
    if let Some(title) = request
        .title
        .as_deref()
        .filter(|title| !title.trim().is_empty())
    {
        println!("{title}");
    }
    let mut answers = Vec::new();
    for question in &request.questions {
        println!("{}", question.prompt);
        if question.options.is_empty() {
            return Err(crate::terminal::acp_error("la domanda non ha opzioni"));
        }
        for (index, option) in question.options.iter().enumerate() {
            println!("  {}. {} ({})", index + 1, option.label, option.id);
        }
        loop {
            let hint = if question.allow_multiple {
                "numeri separati da virgola, oppure salta"
            } else {
                "numero, oppure salta"
            };
            println!("scelta [{hint}]");
            flush_stdout();
            let line = read_stdin_line().await?;
            match parse_option_selection(question, &line) {
                Ok(Selection::Skip) => {
                    println!("scelta: skipped");
                    return Ok(AskQuestionResponse {
                        outcome: AskOutcome::Skipped {
                            reason: Some("saltata".to_string()),
                        },
                    });
                }
                Ok(Selection::Ids(ids)) => {
                    println!("scelta: {}", ids.join(","));
                    answers.push(AskAnswer {
                        question_id: question.id.clone(),
                        selected_option_ids: ids,
                    });
                    break;
                }
                Err(message) => println!("{message}"),
            }
        }
    }
    Ok(AskQuestionResponse {
        outcome: AskOutcome::Answered { answers },
    })
}

pub(crate) async fn answer_plan(
    request: &CreatePlanRequest,
) -> Result<CreatePlanResponse, agent_client_protocol::Error> {
    let detail = request
        .name
        .as_deref()
        .filter(|name| !name.trim().is_empty())
        .or(request.overview.as_deref())
        .unwrap_or("piano");
    println!(
        "{}",
        status::status_awaiting_reply(&format!("piano {detail}")).line()
    );
    println!("chiamata: {}", request.tool_call_id);
    require_terminal("chiedere l'approvazione del piano")?;
    if request.is_project == Some(true) {
        println!("piano di progetto");
    }
    if let Some(name) = request
        .name
        .as_deref()
        .filter(|name| !name.trim().is_empty())
    {
        println!("piano: {name}");
    }
    if let Some(overview) = request
        .overview
        .as_deref()
        .filter(|overview| !overview.trim().is_empty())
    {
        println!("{overview}");
    }
    println!("{}", request.plan);
    for todo in &request.todos {
        println!("- [{}] {}", todo.status, todo.content);
    }
    for phase in &request.phases {
        println!("fase: {}", phase.name);
        for todo in &phase.todos {
            println!("- [{}] {}", todo.status, todo.content);
        }
    }
    loop {
        println!("accetti il piano? [sì/no]");
        flush_stdout();
        let line = read_stdin_line().await?;
        match parse_plan_choice(&line) {
            Some(true) => {
                println!("scelta: accepted");
                return Ok(CreatePlanResponse {
                    outcome: PlanOutcome::Accepted { plan_uri: None },
                });
            }
            Some(false) => {
                println!("scelta: rejected");
                return Ok(CreatePlanResponse {
                    outcome: PlanOutcome::Rejected {
                        reason: Some("rifiutato".to_string()),
                    },
                });
            }
            None => println!("risposta non valida: scrivi sì o no"),
        }
    }
}

fn parse_option_selection(question: &Question, line: &str) -> Result<Selection, String> {
    let line = line.trim();
    if line.eq_ignore_ascii_case("salta") {
        return Ok(Selection::Skip);
    }
    let parts: Vec<&str> = line
        .split(',')
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .collect();
    if parts.is_empty() {
        return Err("risposta vuota".to_string());
    }
    if !question.allow_multiple && parts.len() != 1 {
        return Err("una sola scelta".to_string());
    }
    let mut ids = Vec::new();
    for part in parts {
        let id = resolve_option(question, part)?;
        if !ids.contains(&id) {
            ids.push(id);
        }
    }
    Ok(Selection::Ids(ids))
}

fn resolve_option(question: &Question, part: &str) -> Result<String, String> {
    if let Ok(number) = part.parse::<usize>() {
        if (1..=question.options.len()).contains(&number) {
            return Ok(question.options[number - 1].id.clone());
        }
    }
    if let Some(option) = question
        .options
        .iter()
        .find(|option| option.id == part || option.label.eq_ignore_ascii_case(part))
    {
        return Ok(option.id.clone());
    }
    Err(format!("opzione sconosciuta: {part}"))
}

pub(crate) fn cancelled_question() -> AskQuestionResponse {
    AskQuestionResponse {
        outcome: AskOutcome::Cancelled,
    }
}

pub(crate) fn cancelled_plan() -> CreatePlanResponse {
    CreatePlanResponse {
        outcome: PlanOutcome::Cancelled,
    }
}

/// `cursor/ask_question` e `cursor/create_plan` non portano `sessionId`.
/// Il router della sessione non li consegna: senza un handler sulla
/// connessione la richiesta resta senza risposta e l'agente non riparte.
pub(crate) fn claim_extension(method: &str, has_session_id: bool) -> bool {
    !has_session_id
        && (AskQuestionRequest::matches_method(method) || CreatePlanRequest::matches_method(method))
}

pub(crate) struct ExtensionHandler;

impl<Counterpart: Role> HandleDispatchFrom<Counterpart> for ExtensionHandler {
    async fn handle_dispatch_from(
        &mut self,
        message: Dispatch,
        connection: ConnectionTo<Counterpart>,
    ) -> Result<Handled<Dispatch>, agent_client_protocol::Error> {
        if !claim_extension(message.method(), message.has_field("sessionId")) {
            return Ok(Handled::No {
                message,
                retry: false,
            });
        }
        let ask = connection.clone();
        let plan = connection;
        MatchDispatch::new(message)
            .if_request(async |request: AskQuestionRequest, responder| {
                ask.spawn(async move {
                    let response = match answer_question(&request).await {
                        Ok(response) => response,
                        Err(error) => {
                            println!("cursor/ask_question: {error}");
                            cancelled_question()
                        }
                    };
                    responder.respond(response)
                })?;
                Ok(())
            })
            .await
            .if_request(async |request: CreatePlanRequest, responder| {
                plan.spawn(async move {
                    let response = match answer_plan(&request).await {
                        Ok(response) => response,
                        Err(error) => {
                            println!("cursor/create_plan: {error}");
                            cancelled_plan()
                        }
                    };
                    responder.respond(response)
                })?;
                Ok(())
            })
            .await
            .done()
    }

    fn describe_chain(&self) -> impl std::fmt::Debug {
        "cursor/ask_question, cursor/create_plan"
    }
}

pub(crate) fn parse_plan_choice(input: &str) -> Option<bool> {
    match input.trim().to_lowercase().as_str() {
        "sì" | "si" | "s" | "y" | "yes" | "accetta" => Some(true),
        "no" | "n" | "rifiuta" => Some(false),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use agent_client_protocol::JsonRpcMessage;

    use super::*;

    #[test]
    fn ask_question_response_matches_the_cursor_shape() {
        let raw = r#"{
          "toolCallId": "call_123",
          "title": "Need input",
          "questions": [{
            "id": "q1",
            "prompt": "Which mode should I use?",
            "options": [
              {"id": "agent", "label": "Agent"},
              {"id": "plan", "label": "Plan"}
            ],
            "allowMultiple": false
          }]
        }"#;
        let request: AskQuestionRequest = serde_json::from_str(raw).unwrap();
        assert!(AskQuestionRequest::matches_method("cursor/ask_question"));
        assert_eq!(request.tool_call_id, "call_123");
        assert_eq!(
            parse_option_selection(&request.questions[0], "2").unwrap(),
            Selection::Ids(vec!["plan".to_string()])
        );
        assert_eq!(
            parse_option_selection(&request.questions[0], "salta").unwrap(),
            Selection::Skip
        );
        let response = AskQuestionResponse {
            outcome: AskOutcome::Answered {
                answers: vec![AskAnswer {
                    question_id: "q1".to_string(),
                    selected_option_ids: vec!["plan".to_string()],
                }],
            },
        };
        let json = serde_json::to_value(response).unwrap();
        assert_eq!(json["outcome"]["outcome"], "answered");
        assert_eq!(json["outcome"]["answers"][0]["questionId"], "q1");
        assert_eq!(
            json["outcome"]["answers"][0]["selectedOptionIds"][0],
            "plan"
        );
    }

    #[test]
    fn multiple_answers_use_option_ids() {
        let mut question = Question {
            id: "q".to_string(),
            prompt: "quali".to_string(),
            options: vec![
                QuestionOption {
                    id: "a".to_string(),
                    label: "Alpha".to_string(),
                },
                QuestionOption {
                    id: "b".to_string(),
                    label: "Beta".to_string(),
                },
            ],
            allow_multiple: true,
        };
        assert_eq!(
            parse_option_selection(&question, "1, Beta").unwrap(),
            Selection::Ids(vec!["a".to_string(), "b".to_string()])
        );
        question.allow_multiple = false;
        assert!(parse_option_selection(&question, "1, 2").is_err());
    }

    #[test]
    fn create_plan_response_matches_the_cursor_shape() {
        let raw = r#"{
          "toolCallId": "call_124",
          "name": "Refactor tabs layout",
          "overview": "Tighten layout.",
          "plan": "1. Inspect",
          "todos": [{"id": "todo-1", "content": "Inspect", "status": "completed"}],
          "isProject": false
        }"#;
        let request: CreatePlanRequest = serde_json::from_str(raw).unwrap();
        assert!(CreatePlanRequest::matches_method("cursor/create_plan"));
        assert_eq!(request.name.as_deref(), Some("Refactor tabs layout"));
        assert_eq!(request.todos[0].status, "completed");
        assert_eq!(parse_plan_choice("sì"), Some(true));
        assert_eq!(parse_plan_choice("no"), Some(false));
        assert!(claim_extension("cursor/ask_question", false));
        assert!(claim_extension("cursor/create_plan", false));
        assert!(!claim_extension("cursor/ask_question", true));
        assert!(!claim_extension("cursor/update_todos", false));
        assert!(!claim_extension("session/request_permission", false));
        let cancelled = serde_json::to_value(cancelled_question()).unwrap();
        assert_eq!(cancelled["outcome"]["outcome"], "cancelled");
        let cancelled_plan = serde_json::to_value(cancelled_plan()).unwrap();
        assert_eq!(cancelled_plan["outcome"]["outcome"], "cancelled");
        let accepted = serde_json::to_value(CreatePlanResponse {
            outcome: PlanOutcome::Accepted { plan_uri: None },
        })
        .unwrap();
        assert_eq!(accepted["outcome"]["outcome"], "accepted");
        assert!(accepted["outcome"].get("planUri").is_none());
        let rejected = serde_json::to_value(CreatePlanResponse {
            outcome: PlanOutcome::Rejected {
                reason: Some("rifiutato".to_string()),
            },
        })
        .unwrap();
        assert_eq!(rejected["outcome"]["outcome"], "rejected");
        assert_eq!(rejected["outcome"]["reason"], "rifiutato");
    }
}
