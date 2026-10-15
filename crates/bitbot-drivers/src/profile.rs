//! Profili di avvio degli agenti ACP dello spike.
//!
//! Il client generico non nomina un agente: comando, argomenti e ambiente
//! arrivano da qui. Le differenze restano nel profilo, non nel flusso ACP.

use std::collections::BTreeMap;
use std::ffi::OsStr;
use std::path::Path;

/// Comando con cui lo spike avvia un agente.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentProfile {
    pub name: String,
    pub command: String,
    pub args: Vec<String>,
    pub env: BTreeMap<String, String>,
    /// Vero solo per il profilo Cursor: il flusso generico non confronta il nome.
    cursor_extensions: bool,
}

struct Builtin {
    name: &'static str,
    command: &'static str,
    args: &'static [&'static str],
    env: &'static [(&'static str, &'static str)],
    /// Se il comando non è nel PATH, si usa questo. `None` non ripiega.
    fallback_command: Option<&'static str>,
    /// Estensioni bloccanti `cursor/ask_question` e `cursor/create_plan`.
    cursor_extensions: bool,
}

const BUILTINS: &[Builtin] = &[
    Builtin {
        name: "opencode",
        command: "opencode",
        args: &["acp"],
        env: &[],
        fallback_command: None,
        cursor_extensions: false,
    },
    Builtin {
        name: "cursor",
        command: "cursor-agent",
        args: &["acp"],
        env: &[],
        fallback_command: Some("agent"),
        cursor_extensions: true,
    },
];

impl AgentProfile {
    /// Profilo incorporato risolto sul `PATH` del processo.
    ///
    /// Per Cursor, se `cursor-agent` non è eseguibile nel `PATH`, il comando
    /// diventa `agent`. Se manca anche quello, il profilo resta `agent`: il
    /// processo fallirà all'avvio, come per gli altri agenti assenti.
    pub fn resolve(name: &str) -> anyhow::Result<Self> {
        Self::resolve_with_path(name, std::env::var_os("PATH").as_deref())
    }

    pub(crate) fn resolve_with_path(name: &str, path: Option<&OsStr>) -> anyhow::Result<Self> {
        let Some(builtin) = BUILTINS.iter().find(|profile| profile.name == name) else {
            let known = BUILTINS
                .iter()
                .map(|profile| profile.name)
                .collect::<Vec<_>>()
                .join(", ");
            anyhow::bail!("profilo sconosciuto: {name} ({known})");
        };
        let command = match builtin.fallback_command {
            Some(fallback) if !command_in_path(builtin.command, path) => fallback.to_string(),
            _ => builtin.command.to_string(),
        };
        Ok(Self {
            name: builtin.name.to_string(),
            command,
            args: builtin.args.iter().map(|arg| (*arg).to_string()).collect(),
            env: builtin
                .env
                .iter()
                .map(|(key, value)| ((*key).to_string(), (*value).to_string()))
                .collect(),
            cursor_extensions: builtin.cursor_extensions,
        })
    }

    /// Vero solo per il profilo che deve rispondere alle estensioni di Cursor.
    pub(crate) fn uses_cursor_extensions(&self) -> bool {
        self.cursor_extensions
    }
}

fn command_in_path(command: &str, path_var: Option<&OsStr>) -> bool {
    let Some(path_var) = path_var else {
        return false;
    };
    let names = command_names(command);
    std::env::split_paths(path_var).any(|dir| names.iter().any(|name| is_runnable(&dir.join(name))))
}

fn command_names(command: &str) -> Vec<String> {
    #[cfg(windows)]
    {
        let mut names = vec![command.to_string()];
        if !command.to_ascii_lowercase().ends_with(".exe") {
            names.push(format!("{command}.exe"));
        }
        names
    }
    #[cfg(not(windows))]
    {
        vec![command.to_string()]
    }
}

#[cfg(unix)]
fn is_runnable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    match std::fs::metadata(path) {
        Ok(meta) => meta.is_file() && meta.permissions().mode() & 0o111 != 0,
        Err(_) => false,
    }
}

#[cfg(not(unix))]
fn is_runnable(path: &Path) -> bool {
    path.is_file()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch_dir(label: &str) -> std::path::PathBuf {
        let dir =
            std::env::temp_dir().join(format!("bitbot-profile-{label}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn unknown_profile_names_the_request_and_the_known_ones() {
        let error = AgentProfile::resolve("inesistente").unwrap_err();
        let message = error.to_string();
        assert!(message.contains("inesistente"), "{message}");
        assert!(message.contains("opencode"), "{message}");
        assert!(message.contains("cursor"), "{message}");
        assert_eq!(
            message,
            "profilo sconosciuto: inesistente (opencode, cursor)"
        );
    }

    #[test]
    fn opencode_profile_is_opencode_acp_without_env() {
        let profile = AgentProfile::resolve_with_path("opencode", None).unwrap();
        assert_eq!(profile.name, "opencode");
        assert_eq!(profile.command, "opencode");
        assert_eq!(profile.args, ["acp"]);
        assert!(profile.env.is_empty());
        assert!(!profile.uses_cursor_extensions());
    }

    #[test]
    fn cursor_falls_back_to_agent_when_cursor_agent_is_absent() {
        let empty = scratch_dir("empty");
        let profile = AgentProfile::resolve_with_path("cursor", Some(empty.as_os_str())).unwrap();
        let _ = std::fs::remove_dir_all(&empty);
        assert_eq!(profile.name, "cursor");
        assert_eq!(profile.command, "agent");
        assert_eq!(profile.args, ["acp"]);
        assert!(profile.env.is_empty());
        assert!(profile.uses_cursor_extensions());

        let missing = AgentProfile::resolve_with_path("cursor", None).unwrap();
        assert_eq!(missing.command, "agent");
    }

    #[cfg(unix)]
    #[test]
    fn cursor_prefers_an_executable_cursor_agent_on_path() {
        use std::os::unix::fs::PermissionsExt;

        let dir = scratch_dir("cursor-agent");
        let binary = dir.join("cursor-agent");
        std::fs::write(&binary, b"").unwrap();
        let mut permissions = std::fs::metadata(&binary).unwrap().permissions();
        permissions.set_mode(0o755);
        std::fs::set_permissions(&binary, permissions).unwrap();

        let found = AgentProfile::resolve_with_path("cursor", Some(dir.as_os_str())).unwrap();
        assert_eq!(found.command, "cursor-agent");
        assert_eq!(found.args, ["acp"]);

        let mut permissions = std::fs::metadata(&binary).unwrap().permissions();
        permissions.set_mode(0o644);
        std::fs::set_permissions(&binary, permissions).unwrap();
        let ignored = AgentProfile::resolve_with_path("cursor", Some(dir.as_os_str())).unwrap();
        assert_eq!(ignored.command, "agent");

        let _ = std::fs::remove_dir_all(&dir);
    }
}
