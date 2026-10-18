//! Traduzione di sì, no e sempre nelle opzioni inviate dall'agente.

use agent_client_protocol::schema::v1::{
    PermissionOption, PermissionOptionKind, RequestPermissionOutcome, RequestPermissionResponse,
    SelectedPermissionOutcome,
};
use bitbot_protocol::{PermissionOptionView, RespondOutcome, SessionReply, SessionRequestBody};

use crate::json_text;
use crate::terminal;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PermissionChoice {
    Allow,
    Reject,
    Always,
}

pub(crate) fn parse_permission_choice(input: &str) -> Option<PermissionChoice> {
    match input.trim().to_lowercase().as_str() {
        "sì" | "si" | "s" | "y" | "yes" | "allow" | "allow_once" | "allow-once" | "consenti" => {
            Some(PermissionChoice::Allow)
        }
        "no" | "n" | "reject" | "nega" | "deny" => Some(PermissionChoice::Reject),
        "sempre" | "always" | "allow_always" | "allow-always" => Some(PermissionChoice::Always),
        _ => None,
    }
}

/// Sceglie l'opzione dell'agente per la risposta dell'utente.
///
/// Prima confronta il `kind` (`allow_once`, `reject_once`, `allow_always`),
/// poi le etichette più vicine.
pub(crate) fn select_permission_option(
    options: &[PermissionOption],
    choice: PermissionChoice,
) -> Option<&PermissionOption> {
    let views: Vec<PermissionOptionView> = options.iter().map(permission_view).collect();
    let id = select_view(&views, choice)?.id.clone();
    options
        .iter()
        .find(|option| json_text(&option.option_id) == id)
}

pub(crate) fn permission_body(options: &[PermissionOption]) -> SessionRequestBody {
    SessionRequestBody::Permission {
        options: options.iter().map(permission_view).collect(),
    }
}

pub(crate) fn permission_view(option: &PermissionOption) -> PermissionOptionView {
    PermissionOptionView {
        id: json_text(&option.option_id),
        name: option.name.clone(),
        kind: kind_label(option.kind).to_string(),
    }
}

/// `Ok(None)` è un annullamento. `Ok(Some(id))` è l'opzione dell'agente.
pub(crate) fn permission_reply(
    options: &[PermissionOptionView],
    reply: &SessionReply,
) -> Result<Option<String>, String> {
    if !reply.answers.is_empty() {
        return Err("un permesso non ha risposte a domande".to_string());
    }
    let choice = match reply.outcome {
        RespondOutcome::Cancelled => return Ok(None),
        RespondOutcome::AllowOnce => PermissionChoice::Allow,
        RespondOutcome::AllowAlways => PermissionChoice::Always,
        RespondOutcome::Reject => PermissionChoice::Reject,
        _ => return Err("esito non valido per un permesso".to_string()),
    };
    match select_view(options, choice) {
        Some(option) => Ok(Some(option.id.clone())),
        None => Err("nessuna opzione dell'agente corrisponde a questa risposta".to_string()),
    }
}

pub(crate) fn permission_decision(
    options: &[PermissionOption],
    reply: &SessionReply,
) -> Result<RequestPermissionResponse, String> {
    if !reply.answers.is_empty() {
        return Err("un permesso non ha risposte a domande".to_string());
    }
    let choice = match reply.outcome {
        RespondOutcome::Cancelled => return Ok(cancelled_permission()),
        RespondOutcome::AllowOnce => PermissionChoice::Allow,
        RespondOutcome::AllowAlways => PermissionChoice::Always,
        RespondOutcome::Reject => PermissionChoice::Reject,
        _ => return Err("esito non valido per un permesso".to_string()),
    };
    match select_permission_option(options, choice) {
        Some(option) => Ok(permission_response(json_text(&option.option_id))),
        None => Err("nessuna opzione dell'agente corrisponde a questa risposta".to_string()),
    }
}

pub(crate) async fn prompt_permission(
    options: &[PermissionOptionView],
) -> Result<SessionReply, String> {
    require_prompt_terminal("chiedere il permesso")?;
    if options.is_empty() {
        return Err("l'agente non ha inviato opzioni di permesso".to_string());
    }
    loop {
        println!("permesso? [sì/no/sempre]");
        terminal::flush_stdout();
        let line = terminal::read_stdin_line()
            .await
            .map_err(|error| error.to_string())?;
        let Some(choice) = parse_permission_choice(&line) else {
            println!("risposta non valida: scrivi sì, no o sempre");
            continue;
        };
        let outcome = match choice {
            PermissionChoice::Allow => RespondOutcome::AllowOnce,
            PermissionChoice::Reject => RespondOutcome::Reject,
            PermissionChoice::Always => RespondOutcome::AllowAlways,
        };
        let reply = SessionReply {
            outcome,
            answers: Vec::new(),
        };
        if let Err(message) = permission_reply(options, &reply) {
            println!("{message}");
            println!("{}", format_views(options));
            continue;
        }
        if let Some(option) = select_view(options, choice) {
            println!("scelta: {} id={}", option.kind, option.id);
        }
        return Ok(reply);
    }
}

fn require_prompt_terminal(action: &str) -> Result<(), String> {
    terminal::require_terminal(action).map_err(|error| error.to_string())
}

fn select_view(
    options: &[PermissionOptionView],
    choice: PermissionChoice,
) -> Option<&PermissionOptionView> {
    for kind in preferred_kind_names(choice) {
        if let Some(option) = options.iter().find(|option| option.kind == *kind) {
            return Some(option);
        }
    }
    options
        .iter()
        .find(|option| label_matches_view(option, choice))
}

fn preferred_kind_names(choice: PermissionChoice) -> &'static [&'static str] {
    match choice {
        PermissionChoice::Allow => &["allow_once", "allow_always"],
        PermissionChoice::Reject => &["reject_once", "reject_always"],
        PermissionChoice::Always => &["allow_always"],
    }
}

fn format_views(options: &[PermissionOptionView]) -> String {
    options
        .iter()
        .map(|option| format!("- {} id={} nome={}", option.kind, option.id, option.name))
        .collect::<Vec<_>>()
        .join("\n")
}

fn label_matches_view(option: &PermissionOptionView, choice: PermissionChoice) -> bool {
    let name = option.name.to_lowercase();
    let id = option.id.to_lowercase().replace('-', "_");
    let words = words(&name);
    let negative = words
        .iter()
        .any(|word| matches!(word.as_str(), "reject" | "deny" | "nega" | "no" | "rifiuta"))
        || id.contains("reject");
    if matches!(choice, PermissionChoice::Allow | PermissionChoice::Always) && negative {
        return false;
    }
    labels(choice).iter().any(|label| {
        let label = label.replace('-', "_");
        id == label || name == label || words.iter().any(|word| word == &label)
    })
}

pub(crate) fn permission_response(option_id: impl Into<String>) -> RequestPermissionResponse {
    RequestPermissionResponse::new(RequestPermissionOutcome::Selected(
        SelectedPermissionOutcome::new(option_id.into()),
    ))
}

/// Risposta quando non si può scegliere un'opzione. Senza una risposta
/// l'agente resta fermo sulla richiesta.
pub(crate) fn cancelled_permission() -> RequestPermissionResponse {
    RequestPermissionResponse::new(RequestPermissionOutcome::Cancelled)
}

fn kind_label(kind: PermissionOptionKind) -> &'static str {
    match kind {
        PermissionOptionKind::AllowOnce => "allow_once",
        PermissionOptionKind::AllowAlways => "allow_always",
        PermissionOptionKind::RejectOnce => "reject_once",
        PermissionOptionKind::RejectAlways => "reject_always",
        _ => "altro",
    }
}

fn labels(choice: PermissionChoice) -> &'static [&'static str] {
    match choice {
        PermissionChoice::Allow => &["once", "yes", "sì", "si", "allow_once", "allow-once"],
        PermissionChoice::Reject => &["reject", "no", "nega", "deny", "reject_once", "rifiuta"],
        PermissionChoice::Always => &["always", "sempre", "allow_always", "allow-always"],
    }
}

fn words(name: &str) -> Vec<String> {
    name.split(|c: char| !c.is_alphanumeric())
        .filter(|word| !word.is_empty())
        .map(str::to_string)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn option(
        id: &'static str,
        name: &'static str,
        kind: PermissionOptionKind,
    ) -> PermissionOption {
        PermissionOption::new(id, name, kind)
    }

    fn selected_id(options: &[PermissionOption], input: &str) -> Option<String> {
        let choice = parse_permission_choice(input)?;
        select_permission_option(options, choice).map(|option| json_text(&option.option_id))
    }

    fn opencode_options() -> Vec<PermissionOption> {
        vec![
            option("once", "Allow once", PermissionOptionKind::AllowOnce),
            option("always", "Always allow", PermissionOptionKind::AllowAlways),
            option("reject", "Reject", PermissionOptionKind::RejectOnce),
        ]
    }

    #[test]
    fn maps_yes_no_and_always_onto_the_agent_kinds() {
        let options = opencode_options();
        assert_eq!(selected_id(&options, "sì").as_deref(), Some("once"));
        assert_eq!(selected_id(&options, "SI").as_deref(), Some("once"));
        assert_eq!(selected_id(&options, "no").as_deref(), Some("reject"));
        assert_eq!(selected_id(&options, "sempre").as_deref(), Some("always"));
        assert_eq!(parse_permission_choice("forse"), None);
    }

    #[test]
    fn uses_the_closest_kind_when_the_exact_one_is_missing() {
        let options = vec![
            option("keep", "Always allow", PermissionOptionKind::AllowAlways),
            option("nope", "Reject", PermissionOptionKind::RejectOnce),
        ];
        assert_eq!(selected_id(&options, "sì").as_deref(), Some("keep"));
        assert_eq!(selected_id(&options, "sempre").as_deref(), Some("keep"));
        assert_eq!(selected_id(&options, "no").as_deref(), Some("nope"));
    }

    #[test]
    fn falls_back_to_labels_and_keeps_the_agent_option_id() {
        let options = vec![option("custom", "Sempre", PermissionOptionKind::RejectOnce)];
        assert_eq!(selected_id(&options, "sempre").as_deref(), Some("custom"));
        assert_eq!(selected_id(&options, "sì"), None);

        let response = serde_json::to_value(permission_response("custom")).unwrap();
        assert_eq!(response["outcome"]["outcome"], "selected");
        assert_eq!(response["outcome"]["optionId"], "custom");
        let cancelled = serde_json::to_value(cancelled_permission()).unwrap();
        assert_eq!(cancelled["outcome"]["outcome"], "cancelled");
    }
}
