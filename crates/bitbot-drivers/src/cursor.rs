//! Estensioni bloccanti di Cursor: `cursor/ask_question` e `cursor/create_plan`.

use agent_client_protocol::role::Role;
use agent_client_protocol::util::MatchDispatch;
use agent_client_protocol::{
    ConnectionTo, Dispatch, HandleDispatchFrom, Handled, JsonRpcMessage, JsonRpcRequest,
    JsonRpcResponse,
};
use bitbot_protocol::{
    PlanPhaseView, PlanTodoView, QuestionAnswer, QuestionOptionView, QuestionView, RespondOutcome,
    SessionReply, SessionRequestBody,
};
use serde::{Deserialize, Serialize};

use crate::host::SessionHost;
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

pub(crate) fn question_detail(title: Option<&str>, questions: &[QuestionView]) -> String {
    let _ = title;
    questions
        .first()
        .map(|question| format!("domanda {}", question.prompt))
        .unwrap_or_else(|| "domanda".to_string())
}

pub(crate) fn question_body(request: &AskQuestionRequest) -> SessionRequestBody {
    SessionRequestBody::Question {
        call_id: Some(request.tool_call_id.clone()),
        title: request.title.clone(),
        questions: request.questions.iter().map(question_view).collect(),
    }
}

fn question_view(question: &Question) -> QuestionView {
    QuestionView {
        id: question.id.clone(),
        prompt: question.prompt.clone(),
        options: question
            .options
            .iter()
            .map(|option| QuestionOptionView {
                id: option.id.clone(),
                label: option.label.clone(),
            })
            .collect(),
        allow_multiple: question.allow_multiple,
    }
}

pub(crate) fn question_reply_ok(
    questions: &[QuestionView],
    reply: &SessionReply,
) -> Result<(), String> {
    match reply.outcome {
        RespondOutcome::Cancelled | RespondOutcome::Skipped => Ok(()),
        RespondOutcome::Answered => answered_ok(questions, reply),
        _ => Err("esito non valido per una domanda".to_string()),
    }
}

fn answered_ok(questions: &[QuestionView], reply: &SessionReply) -> Result<(), String> {
    if questions.is_empty() {
        return Err("la domanda non ha voci".to_string());
    }
    for question in questions {
        let Some(answer) = reply
            .answers
            .iter()
            .find(|answer| answer.question_id == question.id)
        else {
            return Err(format!("manca la risposta a {}", question.id));
        };
        if question.options.is_empty() {
            return Err("la domanda non ha opzioni".to_string());
        }
        if !question.allow_multiple && answer.selected_option_ids.len() != 1 {
            return Err("una sola scelta".to_string());
        }
        if answer.selected_option_ids.is_empty() {
            return Err("risposta vuota".to_string());
        }
        for id in &answer.selected_option_ids {
            if !question.options.iter().any(|option| &option.id == id) {
                return Err(format!("opzione sconosciuta: {id}"));
            }
        }
    }
    Ok(())
}

pub(crate) fn question_response(reply: &SessionReply) -> Result<AskQuestionResponse, String> {
    let outcome = match reply.outcome {
        RespondOutcome::Cancelled => AskOutcome::Cancelled,
        RespondOutcome::Skipped => AskOutcome::Skipped {
            reason: Some("saltata".to_string()),
        },
        RespondOutcome::Answered => AskOutcome::Answered {
            answers: reply
                .answers
                .iter()
                .map(|answer| AskAnswer {
                    question_id: answer.question_id.clone(),
                    selected_option_ids: answer.selected_option_ids.clone(),
                })
                .collect(),
        },
        _ => return Err("esito non valido per una domanda".to_string()),
    };
    Ok(AskQuestionResponse { outcome })
}

pub(crate) async fn prompt_question(
    call_id: Option<&str>,
    title: Option<&str>,
    questions: &[QuestionView],
) -> Result<SessionReply, String> {
    require_terminal("fare la domanda").map_err(|error| error.to_string())?;
    if let Some(call_id) = call_id {
        println!("chiamata: {call_id}");
    }
    if questions.is_empty() {
        return Err("la domanda non ha voci".to_string());
    }
    if let Some(title) = title.filter(|title| !title.trim().is_empty()) {
        println!("{title}");
    }
    let mut answers = Vec::new();
    for question in questions {
        println!("{}", question.prompt);
        if question.options.is_empty() {
            return Err("la domanda non ha opzioni".to_string());
        }
        for (index, option) in question.options.iter().enumerate() {
            println!("  {}. {} ({})", index + 1, option.label, option.id);
        }
        let local = Question {
            id: question.id.clone(),
            prompt: question.prompt.clone(),
            options: question
                .options
                .iter()
                .map(|option| QuestionOption {
                    id: option.id.clone(),
                    label: option.label.clone(),
                })
                .collect(),
            allow_multiple: question.allow_multiple,
        };
        loop {
            let hint = if question.allow_multiple {
                "numeri separati da virgola, oppure salta"
            } else {
                "numero, oppure salta"
            };
            println!("scelta [{hint}]");
            flush_stdout();
            let line = read_stdin_line().await.map_err(|error| error.to_string())?;
            match parse_option_selection(&local, &line) {
                Ok(Selection::Skip) => {
                    println!("scelta: skipped");
                    return Ok(SessionReply {
                        outcome: RespondOutcome::Skipped,
                        answers: Vec::new(),
                    });
                }
                Ok(Selection::Ids(ids)) => {
                    println!("scelta: {}", ids.join(","));
                    answers.push(QuestionAnswer {
                        question_id: question.id.clone(),
                        selected_option_ids: ids,
                    });
                    break;
                }
                Err(message) => println!("{message}"),
            }
        }
    }
    Ok(SessionReply {
        outcome: RespondOutcome::Answered,
        answers,
    })
}

pub(crate) fn plan_detail(name: Option<&str>, overview: Option<&str>) -> String {
    let detail = name
        .filter(|name| !name.trim().is_empty())
        .or(overview)
        .unwrap_or("piano");
    format!("piano {detail}")
}

pub(crate) fn plan_body(request: &CreatePlanRequest) -> SessionRequestBody {
    SessionRequestBody::Plan {
        call_id: Some(request.tool_call_id.clone()),
        name: request.name.clone(),
        overview: request.overview.clone(),
        plan: request.plan.clone(),
        is_project: request.is_project.unwrap_or(false),
        todos: request.todos.iter().map(todo_view).collect(),
        phases: request.phases.iter().map(phase_view).collect(),
    }
}

fn todo_view(todo: &PlanTodo) -> PlanTodoView {
    PlanTodoView {
        id: todo.id.clone(),
        content: todo.content.clone(),
        status: todo.status.clone(),
    }
}

fn phase_view(phase: &PlanPhase) -> PlanPhaseView {
    PlanPhaseView {
        name: phase.name.clone(),
        todos: phase.todos.iter().map(todo_view).collect(),
    }
}

pub(crate) fn plan_reply_ok(reply: &SessionReply) -> Result<(), String> {
    match reply.outcome {
        RespondOutcome::Accepted | RespondOutcome::Rejected | RespondOutcome::Cancelled
            if reply.answers.is_empty() =>
        {
            Ok(())
        }
        RespondOutcome::Accepted | RespondOutcome::Rejected | RespondOutcome::Cancelled => {
            Err("un piano non ha risposte a domande".to_string())
        }
        _ => Err("esito non valido per un piano".to_string()),
    }
}

pub(crate) fn plan_response(reply: &SessionReply) -> Result<CreatePlanResponse, String> {
    let outcome = match reply.outcome {
        RespondOutcome::Accepted => PlanOutcome::Accepted { plan_uri: None },
        RespondOutcome::Rejected => PlanOutcome::Rejected {
            reason: Some("rifiutato".to_string()),
        },
        RespondOutcome::Cancelled => PlanOutcome::Cancelled,
        _ => return Err("esito non valido per un piano".to_string()),
    };
    Ok(CreatePlanResponse { outcome })
}

pub(crate) async fn prompt_plan(
    call_id: Option<&str>,
    name: Option<&str>,
    overview: Option<&str>,
    plan: &str,
    is_project: bool,
    todos: &[PlanTodoView],
    phases: &[PlanPhaseView],
) -> Result<SessionReply, String> {
    require_terminal("chiedere l'approvazione del piano").map_err(|error| error.to_string())?;
    if let Some(call_id) = call_id {
        println!("chiamata: {call_id}");
    }
    if is_project {
        println!("piano di progetto");
    }
    if let Some(name) = name.filter(|name| !name.trim().is_empty()) {
        println!("piano: {name}");
    }
    if let Some(overview) = overview.filter(|overview| !overview.trim().is_empty()) {
        println!("{overview}");
    }
    println!("{plan}");
    for todo in todos {
        println!("- [{}] {}", todo.status, todo.content);
    }
    for phase in phases {
        println!("fase: {}", phase.name);
        for todo in &phase.todos {
            println!("- [{}] {}", todo.status, todo.content);
        }
    }
    loop {
        println!("accetti il piano? [sì/no]");
        flush_stdout();
        let line = read_stdin_line().await.map_err(|error| error.to_string())?;
        match parse_plan_choice(&line) {
            Some(true) => {
                println!("scelta: accepted");
                return Ok(SessionReply {
                    outcome: RespondOutcome::Accepted,
                    answers: Vec::new(),
                });
            }
            Some(false) => {
                println!("scelta: rejected");
                return Ok(SessionReply {
                    outcome: RespondOutcome::Rejected,
                    answers: Vec::new(),
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

pub(crate) async fn settle_question(
    host: &SessionHost,
    agent: &str,
    request: &AskQuestionRequest,
) -> Result<AskQuestionResponse, String> {
    let body = question_body(request);
    let SessionRequestBody::Question {
        title, questions, ..
    } = &body
    else {
        return Err("richiesta inattesa".to_string());
    };
    host.emit(&status::status_awaiting_reply(
        agent,
        &question_detail(title.as_deref(), questions),
    ));
    let questions = questions.clone();
    let reply = host
        .ask(body, move |reply| question_reply_ok(&questions, reply))
        .await?;
    question_response(&reply)
}

pub(crate) async fn settle_plan(
    host: &SessionHost,
    agent: &str,
    request: &CreatePlanRequest,
) -> Result<CreatePlanResponse, String> {
    let body = plan_body(request);
    host.emit(&status::status_awaiting_reply(
        agent,
        &plan_detail(request.name.as_deref(), request.overview.as_deref()),
    ));
    let reply = host.ask(body, plan_reply_ok).await?;
    plan_response(&reply)
}

pub(crate) struct ExtensionHandler {
    agent: String,
    host: SessionHost,
}

impl ExtensionHandler {
    pub(crate) fn new(agent: impl Into<String>, host: SessionHost) -> Self {
        Self {
            agent: agent.into(),
            host,
        }
    }
}

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
        let agent = self.agent.clone();
        let host = self.host.clone();
        let ask = connection.clone();
        let plan = connection;
        MatchDispatch::new(message)
            .if_request(async |request: AskQuestionRequest, responder| {
                let agent = agent.clone();
                let host = host.clone();
                ask.spawn(async move {
                    let response = match settle_question(&host, &agent, &request).await {
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
                let agent = agent.clone();
                let host = host.clone();
                plan.spawn(async move {
                    let response = match settle_plan(&host, &agent, &request).await {
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
