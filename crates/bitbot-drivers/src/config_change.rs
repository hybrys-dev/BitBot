//! Scelta di un'opzione dichiarata dall'agente da cambiare a metà sessione.

use agent_client_protocol::schema::v1::{
    SessionConfigKind, SessionConfigOption, SessionConfigOptionCategory, SessionConfigSelectOptions,
};

use crate::json_text;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ConfigChange {
    pub id: String,
    pub category: &'static str,
    pub from: String,
    pub to: String,
}

/// Preferisce il livello di ragionamento. Se non ha alternative, passa al modello.
pub(crate) fn pick_config_change(options: &[SessionConfigOption]) -> Option<ConfigChange> {
    pick_category(
        options,
        &SessionConfigOptionCategory::ThoughtLevel,
        "thought_level",
    )
    .or_else(|| pick_category(options, &SessionConfigOptionCategory::Model, "model"))
}

pub(crate) fn current_option_value(options: &[SessionConfigOption], id: &str) -> Option<String> {
    let option = options.iter().find(|option| json_text(&option.id) == id)?;
    match &option.kind {
        SessionConfigKind::Select(select) => Some(json_text(&select.current_value)),
        SessionConfigKind::Boolean(value) => Some(value.current_value.to_string()),
        _ => None,
    }
}

/// Prompt del turno successivo. Non contiene il valore atteso: la verifica
/// legge lo stato di configurazione, non una frase ripetuta dal modello.
pub(crate) fn verification_prompt() -> &'static str {
    "Continua il compito precedente."
}

/// Valore in vigore per il turno successivo.
///
/// Se durante il turno arriva un `config_option_update`, vale l'ultimo.
/// Altrimenti vale il valore restituito da `session/set_config_option`.
pub(crate) fn effective_value(applied: Option<&str>, updates: &[Option<String>]) -> Option<String> {
    match updates.last() {
        Some(value) => value.clone(),
        None => applied.map(str::to_string),
    }
}

pub(crate) fn turn_uses(
    applied: Option<&str>,
    updates: &[Option<String>],
    requested: &str,
) -> bool {
    effective_value(applied, updates).as_deref() == Some(requested)
}

fn pick_category(
    options: &[SessionConfigOption],
    category: &SessionConfigOptionCategory,
    label: &'static str,
) -> Option<ConfigChange> {
    let option = options
        .iter()
        .find(|option| option.category.as_ref() == Some(category))?;
    let SessionConfigKind::Select(select) = &option.kind else {
        return None;
    };
    let values = select_values(&select.options);
    let from = json_text(&select.current_value);
    let to = next_value(&from, &values)?;
    Some(ConfigChange {
        id: json_text(&option.id),
        category: label,
        from,
        to,
    })
}

fn select_values(options: &SessionConfigSelectOptions) -> Vec<String> {
    match options {
        SessionConfigSelectOptions::Ungrouped(options) => options
            .iter()
            .map(|option| json_text(&option.value))
            .filter(|value| !value.is_empty())
            .collect(),
        SessionConfigSelectOptions::Grouped(groups) => groups
            .iter()
            .flat_map(|group| group.options.iter().map(|option| json_text(&option.value)))
            .filter(|value| !value.is_empty())
            .collect(),
        _ => Vec::new(),
    }
}

fn next_value(current: &str, values: &[String]) -> Option<String> {
    let Some(index) = values.iter().position(|value| value == current) else {
        return values
            .iter()
            .find(|value| value.as_str() != current)
            .cloned();
    };
    let next = values.get(index + 1).or_else(|| values.first())?;
    if next == current {
        values
            .iter()
            .find(|value| value.as_str() != current)
            .cloned()
    } else {
        Some(next.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use agent_client_protocol::schema::v1::{
        SessionConfigSelect, SessionConfigSelectGroup, SessionConfigSelectOption,
        SetSessionConfigOptionRequest,
    };

    fn select(
        id: &str,
        category: SessionConfigOptionCategory,
        current: &str,
        values: &[&str],
    ) -> SessionConfigOption {
        SessionConfigOption::new(
            id.to_string(),
            id.to_string(),
            SessionConfigKind::Select(SessionConfigSelect::new(
                current.to_string(),
                values
                    .iter()
                    .map(|value| SessionConfigSelectOption::new((*value).to_string(), *value))
                    .collect::<Vec<_>>(),
            )),
        )
        .category(category)
    }

    #[test]
    fn prefers_thought_level_over_model() {
        let options = [
            select(
                "model",
                SessionConfigOptionCategory::Model,
                "glm",
                &["glm", "other"],
            ),
            select(
                "effort",
                SessionConfigOptionCategory::ThoughtLevel,
                "low",
                &["low", "high"],
            ),
        ];
        let change = pick_config_change(&options).unwrap();
        assert_eq!(change.id, "effort");
        assert_eq!(change.category, "thought_level");
        assert_eq!(change.from, "low");
        assert_eq!(change.to, "high");
        assert_eq!(verification_prompt(), "Continua il compito precedente.");
        assert!(turn_uses(Some("high"), &[], "high"));
        assert!(!turn_uses(Some("low"), &[], "high"));
        assert!(turn_uses(Some("low"), &[Some("high".to_string())], "high"));
        assert!(!turn_uses(Some("high"), &[Some("low".to_string())], "high"));
        assert!(!turn_uses(None, &[], "high"));
        assert_eq!(effective_value(Some("high"), &[None]).as_deref(), None);
    }

    #[test]
    fn uses_model_when_effort_has_no_alternative() {
        let options = [
            select(
                "effort",
                SessionConfigOptionCategory::ThoughtLevel,
                "low",
                &["low"],
            ),
            select(
                "model",
                SessionConfigOptionCategory::Model,
                "glm",
                &["glm", "other"],
            ),
        ];
        let change = pick_config_change(&options).unwrap();
        assert_eq!(change.id, "model");
        assert_eq!(change.to, "other");
    }

    #[test]
    fn skips_the_change_when_nothing_else_is_declared() {
        let options = [select(
            "model",
            SessionConfigOptionCategory::Model,
            "glm",
            &["glm"],
        )];
        assert!(pick_config_change(&options).is_none());
    }

    #[test]
    fn reads_the_next_value_inside_a_group() {
        let option = SessionConfigOption::new(
            "effort",
            "Effort",
            SessionConfigKind::Select(SessionConfigSelect::new(
                "low",
                SessionConfigSelectOptions::Grouped(vec![SessionConfigSelectGroup::new(
                    "g",
                    "Livelli",
                    vec![
                        SessionConfigSelectOption::new("low", "Low"),
                        SessionConfigSelectOption::new("high", "High"),
                    ],
                )]),
            )),
        )
        .category(SessionConfigOptionCategory::ThoughtLevel);
        let change = pick_config_change(&[option]).unwrap();
        assert_eq!(change.to, "high");
    }

    #[test]
    fn set_config_request_sends_the_value_as_a_string() {
        let request = SetSessionConfigOptionRequest::new("ses", "effort", "high");
        let value = serde_json::to_value(request).unwrap();
        assert_eq!(value["sessionId"], "ses");
        assert_eq!(value["configId"], "effort");
        assert_eq!(value["value"], "high");
        assert!(value.get("type").is_none());
    }
}
