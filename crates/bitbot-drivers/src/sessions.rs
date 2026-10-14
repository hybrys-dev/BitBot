//! Scelta della sessione da riprendere per una cartella.

use std::path::Path;

use agent_client_protocol::schema::v1::SessionInfo;

use crate::json_text;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RestoreMethod {
    Resume,
    Load,
}

/// `session/resume` se l'agente lo dichiara, altrimenti `session/load`.
pub(crate) fn choose_restore(can_resume: bool, can_load: bool) -> Option<RestoreMethod> {
    if can_resume {
        Some(RestoreMethod::Resume)
    } else if can_load {
        Some(RestoreMethod::Load)
    } else {
        None
    }
}

pub(crate) fn matching_sessions<'a>(
    sessions: &'a [SessionInfo],
    cwd: &Path,
) -> Vec<&'a SessionInfo> {
    sessions
        .iter()
        .filter(|session| same_directory(&session.cwd, cwd))
        .collect()
}

pub(crate) fn latest_session<'a>(sessions: &[&'a SessionInfo]) -> Option<&'a SessionInfo> {
    sessions
        .iter()
        .copied()
        .max_by(|left, right| match (&left.updated_at, &right.updated_at) {
            (Some(left), Some(right)) => left.cmp(right),
            (Some(_), None) => std::cmp::Ordering::Greater,
            (None, Some(_)) => std::cmp::Ordering::Less,
            (None, None) => std::cmp::Ordering::Equal,
        })
}

pub(crate) fn format_session_list(sessions: &[SessionInfo]) -> String {
    let mut lines = vec![format!("sessioni: {}", sessions.len())];
    if sessions.is_empty() {
        lines.push("nessuna".to_string());
    } else {
        lines.extend(sessions.iter().map(|session| {
            format!(
                "- id={} cartella={} aggiornata={} titolo={}",
                json_text(&session.session_id),
                session.cwd.display(),
                session.updated_at.as_deref().unwrap_or("-"),
                session.title.as_deref().unwrap_or("-"),
            )
        }));
    }
    lines.join("\n")
}

fn same_directory(listed: &Path, wanted: &Path) -> bool {
    if listed == wanted {
        return true;
    }
    match (std::fs::canonicalize(listed), std::fs::canonicalize(wanted)) {
        (Ok(listed), Ok(wanted)) => listed == wanted,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prefers_resume_and_falls_back_to_load() {
        assert_eq!(choose_restore(true, true), Some(RestoreMethod::Resume));
        assert_eq!(choose_restore(false, true), Some(RestoreMethod::Load));
        assert_eq!(choose_restore(true, false), Some(RestoreMethod::Resume));
        assert_eq!(choose_restore(false, false), None);
    }

    #[test]
    fn picks_the_latest_session_in_the_folder() {
        let root =
            std::env::temp_dir().join(format!("bitbot-spike-sessions-{}", std::process::id()));
        let project = root.join("project");
        let other = root.join("other");
        std::fs::create_dir_all(&project).unwrap();
        std::fs::create_dir_all(&other).unwrap();
        let sessions = vec![
            SessionInfo::new("old", &project).updated_at("2026-10-14T10:00:00Z"),
            SessionInfo::new("other", &other).updated_at("2026-10-14T12:00:00Z"),
            SessionInfo::new("new", &project).updated_at("2026-10-14T11:00:00Z"),
        ];
        let matches = matching_sessions(&sessions, &project);
        let chosen = latest_session(&matches).unwrap();
        assert_eq!(serde_json::to_value(&chosen.session_id).unwrap(), "new");
        let _ = std::fs::remove_dir_all(root);
    }
}
