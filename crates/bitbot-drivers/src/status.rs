//! Traduzione degli eventi ACP nello stato unificato.

use agent_client_protocol::schema::v1::{
    ContentBlock, ContentChunk, Diff, RequestPermissionRequest, SessionUpdate, StopReason,
    ToolCall, ToolCallContent, ToolCallLocation, ToolCallUpdate, ToolKind,
};
use bitbot_protocol::{Action, AgentState, Status};

pub(crate) const AGENT_NAME: &str = "opencode";

pub(crate) fn status_from_update(update: &SessionUpdate) -> Status {
    let action = match update {
        SessionUpdate::UserMessageChunk(chunk) | SessionUpdate::AgentMessageChunk(chunk) => {
            Some(message_action("messaggio", chunk))
        }
        SessionUpdate::AgentThoughtChunk(chunk) => Some(message_action("ragionamento", chunk)),
        SessionUpdate::ToolCall(call) => Some(action_from_call(call)),
        SessionUpdate::ToolCallUpdate(update) => Some(action_from_update(update)),
        SessionUpdate::Plan(_) => Some(Action::other("piano")),
        SessionUpdate::AvailableCommandsUpdate(_) => Some(Action::other("comandi")),
        SessionUpdate::CurrentModeUpdate(_) => Some(Action::other("modalità")),
        SessionUpdate::ConfigOptionUpdate(_) => Some(Action::other("opzioni")),
        SessionUpdate::SessionInfoUpdate(_) => Some(Action::other("sessione")),
        SessionUpdate::UsageUpdate(_) => Some(Action::other("consumo")),
        SessionUpdate::Notice(_) => Some(Action::other("avviso")),
        SessionUpdate::CompactionUpdate(_) | SessionUpdate::CompactionSummaryChunk(_) => {
            Some(Action::other("compattazione"))
        }
        _ => Some(Action::other("aggiornamento")),
    };
    working(action)
}

pub(crate) fn status_unknown() -> Status {
    working(Some(Action::other("aggiornamento")))
}

pub(crate) fn status_from_stop(reason: &StopReason) -> Status {
    let state = match reason {
        StopReason::EndTurn | StopReason::Cancelled | StopReason::MaxTurnRequests => {
            AgentState::Finished
        }
        StopReason::Refusal | StopReason::MaxTokens => AgentState::Failed,
        _ => AgentState::Failed,
    };
    Status::new(AGENT_NAME, state)
}

pub(crate) fn status_from_permission(request: &RequestPermissionRequest) -> Status {
    Status::new(AGENT_NAME, AgentState::AwaitingPermission)
        .with_action(action_from_update(&request.tool_call))
}

fn working(action: Option<Action>) -> Status {
    let status = Status::new(AGENT_NAME, AgentState::Working);
    match action {
        Some(action) => status.with_action(action),
        None => status,
    }
}

fn message_action(label: &str, chunk: &ContentChunk) -> Action {
    let preview = match &chunk.content {
        ContentBlock::Text(text) => text.text.split_whitespace().next().unwrap_or(""),
        _ => "",
    };
    if preview.is_empty() {
        Action::other(label)
    } else {
        Action::other(format!("{label} {preview}"))
    }
}

fn action_from_call(call: &ToolCall) -> Action {
    action_from_parts(call.kind, &call.title, &call.locations, &call.content)
}

fn action_from_update(update: &ToolCallUpdate) -> Action {
    let fields = &update.fields;
    action_from_parts(
        fields.kind.unwrap_or_default(),
        fields.title.as_deref().unwrap_or(""),
        fields.locations.as_deref().unwrap_or(&[]),
        fields.content.as_deref().unwrap_or(&[]),
    )
}

fn action_from_parts(
    kind: ToolKind,
    title: &str,
    locations: &[ToolCallLocation],
    content: &[ToolCallContent],
) -> Action {
    let path = first_diff(content).map(|(path, _, _)| path).or_else(|| {
        locations
            .first()
            .map(|location| location.path.display().to_string())
    });
    match kind {
        ToolKind::Read | ToolKind::Search | ToolKind::Fetch => {
            Action::read(path.unwrap_or_else(|| title.to_string()))
        }
        ToolKind::Edit | ToolKind::Delete | ToolKind::Move => {
            if let Some((path, added, removed)) = first_diff(content) {
                Action::edit(path, added, removed)
            } else {
                Action::edit(path.unwrap_or_else(|| title.to_string()), 0, 0)
            }
        }
        ToolKind::Execute => Action::command(if title.is_empty() {
            path.unwrap_or_default()
        } else {
            title.to_string()
        }),
        ToolKind::Think | ToolKind::SwitchMode | ToolKind::Other | _ => {
            Action::other(if title.is_empty() {
                "aggiornamento".to_string()
            } else {
                title.to_string()
            })
        }
    }
}

fn first_diff(content: &[ToolCallContent]) -> Option<(String, u32, u32)> {
    content.iter().find_map(|item| match item {
        ToolCallContent::Diff(diff) => Some(diff_change(diff)),
        _ => None,
    })
}

fn diff_change(diff: &Diff) -> (String, u32, u32) {
    let (added, removed) = line_delta(diff.old_text.as_deref(), &diff.new_text);
    (diff.path.display().to_string(), added, removed)
}

fn line_delta(old: Option<&str>, new: &str) -> (u32, u32) {
    let mut counts = std::collections::HashMap::<&str, i32>::new();
    if let Some(old) = old {
        for line in old.lines() {
            *counts.entry(line).or_default() -= 1;
        }
    }
    for line in new.lines() {
        *counts.entry(line).or_default() += 1;
    }
    let mut added = 0u32;
    let mut removed = 0u32;
    for count in counts.values() {
        if *count > 0 {
            added += *count as u32;
        } else {
            removed += count.unsigned_abs();
        }
    }
    (added, removed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use agent_client_protocol::schema::v1::{ContentChunk, TextContent, ToolCallUpdateFields};

    fn sample_diff() -> Diff {
        let old = "gone1\ngone2\nkeep\n";
        let new = "keep\nn1\nn2\nn3\nn4\nn5\nn6\nn7\nn8\n";
        Diff::new("src/lib.rs", new).old_text(old)
    }

    #[test]
    fn translates_edit_update_into_unified_line() {
        let update = SessionUpdate::ToolCall(
            ToolCall::new("call-1", "Modifica")
                .kind(ToolKind::Edit)
                .content(vec![sample_diff().into()]),
        );
        assert_eq!(
            status_from_update(&update).line(),
            "[opencode] al lavoro · modifica src/lib.rs +8 −2"
        );
    }

    #[test]
    fn translates_read_command_stop_and_permission() {
        let read = SessionUpdate::ToolCall(
            ToolCall::new("call-2", "Legge")
                .kind(ToolKind::Read)
                .locations(vec![ToolCallLocation::new("Cargo.toml")]),
        );
        assert_eq!(
            status_from_update(&read).line(),
            "[opencode] al lavoro · lettura Cargo.toml"
        );

        let command =
            SessionUpdate::ToolCall(ToolCall::new("call-3", "cargo test").kind(ToolKind::Execute));
        assert_eq!(
            status_from_update(&command).line(),
            "[opencode] al lavoro · comando cargo test"
        );

        let message = SessionUpdate::AgentMessageChunk(ContentChunk::new(ContentBlock::Text(
            TextContent::new("fatto"),
        )));
        assert_eq!(
            status_from_update(&message).line(),
            "[opencode] al lavoro · messaggio fatto"
        );

        assert_eq!(
            status_from_stop(&StopReason::EndTurn).line(),
            "[opencode] finito"
        );
        assert_eq!(
            status_from_stop(&StopReason::Refusal).line(),
            "[opencode] errore"
        );

        let permission = RequestPermissionRequest::new(
            "sessione",
            ToolCallUpdate::new(
                "call-1",
                ToolCallUpdateFields::new()
                    .kind(ToolKind::Edit)
                    .content(vec![sample_diff().into()]),
            ),
            Vec::new(),
        );
        assert_eq!(
            status_from_permission(&permission).line(),
            "[opencode] in attesa di permesso · modifica src/lib.rs +8 −2"
        );
    }
}
