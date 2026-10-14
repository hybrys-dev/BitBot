//! Traduzione di sì, no e sempre nelle opzioni inviate dall'agente.

use agent_client_protocol::schema::v1::{
    PermissionOption, PermissionOptionKind, RequestPermissionOutcome, RequestPermissionResponse,
    SelectedPermissionOutcome,
};

use crate::json_text;

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
    for kind in preferred_kinds(choice) {
        if let Some(option) = options.iter().find(|option| option.kind == *kind) {
            return Some(option);
        }
    }
    options.iter().find(|option| label_matches(option, choice))
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

pub(crate) fn format_permission_options(options: &[PermissionOption]) -> String {
    options
        .iter()
        .map(|option| {
            format!(
                "- {} id={} nome={}",
                kind_label(option.kind),
                json_text(&option.option_id),
                option.name
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn preferred_kinds(choice: PermissionChoice) -> &'static [PermissionOptionKind] {
    match choice {
        PermissionChoice::Allow => &[
            PermissionOptionKind::AllowOnce,
            PermissionOptionKind::AllowAlways,
        ],
        PermissionChoice::Reject => &[
            PermissionOptionKind::RejectOnce,
            PermissionOptionKind::RejectAlways,
        ],
        PermissionChoice::Always => &[PermissionOptionKind::AllowAlways],
    }
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

fn label_matches(option: &PermissionOption, choice: PermissionChoice) -> bool {
    let name = option.name.to_lowercase();
    let id = json_text(&option.option_id)
        .to_lowercase()
        .replace('-', "_");
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
