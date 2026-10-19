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
    Builtin {
        name: "codex",
        command: "codex-acp",
        args: &[],
        env: &[],
        fallback_command: None,
        cursor_extensions: false,
    },
    Builtin {
        name: "claude",
        command: "claude-agent-acp",
        args: &[],
        env: &[],
        fallback_command: None,
        cursor_extensions: false,
    },
];

impl AgentProfile {
    /// Profilo incorporato risolto sul `PATH` del processo.
    ///
    /// Per Cursor, se `cursor-agent` non è eseguibile nel `PATH`, il comando
    /// diventa `agent`. Se manca anche quello, il profilo resta `agent`: il
    /// processo fallirà all'avvio, come per gli altri agenti assenti.
    /// Su Windows il nome può includere `.cmd` o `.bat`: è lo shim che il
    /// processo riesce ad avviare.
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
            Some(fallback) => spawnable_on_path(builtin.command, path)
                .or_else(|| spawnable_on_path(fallback, path))
                .unwrap_or_else(|| fallback.to_string()),
            None => spawnable_on_path(builtin.command, path)
                .unwrap_or_else(|| builtin.command.to_string()),
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

/// Nome da passare a `std::process::Command`, se il programma è nel PATH.
///
/// Su Unix conta il file con il bit di esecuzione. Su Windows la ricerca
/// aggiunge solo `.exe`: uno shim npm `opencode.cmd` va passato con
/// l'estensione, altrimenti il processo non parte. `.bat` è lo stesso
/// caso. Un file senza estensione non conta, il runtime non lo trova.
fn spawnable_on_path(command: &str, path_var: Option<&OsStr>) -> Option<String> {
    let path_var = path_var?;
    std::env::split_paths(path_var).find_map(|dir| spawnable_in_dir(&dir, command))
}

#[cfg(windows)]
fn spawnable_in_dir(dir: &Path, command: &str) -> Option<String> {
    command_from_hits(
        command,
        is_runnable(&dir.join(format!("{command}.exe"))),
        is_runnable(&dir.join(format!("{command}.cmd"))),
        is_runnable(&dir.join(format!("{command}.bat"))),
    )
}

#[cfg(not(windows))]
fn spawnable_in_dir(dir: &Path, command: &str) -> Option<String> {
    is_runnable(&dir.join(command)).then(|| command.to_string())
}

#[cfg(any(windows, test))]
fn command_from_hits(command: &str, has_exe: bool, has_cmd: bool, has_bat: bool) -> Option<String> {
    if has_exe {
        Some(command.to_string())
    } else if has_cmd {
        Some(format!("{command}.cmd"))
    } else if has_bat {
        Some(format!("{command}.bat"))
    } else {
        None
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
        assert!(message.contains("codex"), "{message}");
        assert!(message.contains("claude"), "{message}");
        assert_eq!(
            message,
            "profilo sconosciuto: inesistente (opencode, cursor, codex, claude)"
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

    #[test]
    fn windows_spawn_name_prefers_exe_then_cmd_then_bat() {
        assert_eq!(
            super::command_from_hits("opencode", true, true, true).as_deref(),
            Some("opencode")
        );
        assert_eq!(
            super::command_from_hits("opencode", false, true, true).as_deref(),
            Some("opencode.cmd")
        );
        assert_eq!(
            super::command_from_hits("codex-acp", false, false, true).as_deref(),
            Some("codex-acp.bat")
        );
        assert_eq!(
            super::command_from_hits("opencode", false, false, false),
            None
        );
    }

    #[cfg(windows)]
    #[test]
    fn windows_cmd_shim_is_the_spawn_name_and_exe_wins() {
        let dir = scratch_dir("win-shim");
        std::fs::write(dir.join("opencode.cmd"), b"@echo off\r\n").unwrap();
        std::fs::write(dir.join("cursor-agent.cmd"), b"@echo off\r\n").unwrap();
        std::fs::write(dir.join("cursor-agent.exe"), b"").unwrap();
        std::fs::write(dir.join("codex-acp.cmd"), b"@echo off\r\n").unwrap();

        let opencode = AgentProfile::resolve_with_path("opencode", Some(dir.as_os_str())).unwrap();
        assert_eq!(opencode.command, "opencode.cmd");
        let codex = AgentProfile::resolve_with_path("codex", Some(dir.as_os_str())).unwrap();
        assert_eq!(codex.command, "codex-acp.cmd");
        let cursor = AgentProfile::resolve_with_path("cursor", Some(dir.as_os_str())).unwrap();
        assert_eq!(cursor.command, "cursor-agent");

        std::fs::remove_file(dir.join("cursor-agent.exe")).unwrap();
        let cmd = AgentProfile::resolve_with_path("cursor", Some(dir.as_os_str())).unwrap();
        assert_eq!(cmd.command, "cursor-agent.cmd");

        std::fs::remove_file(dir.join("cursor-agent.cmd")).unwrap();
        std::fs::write(dir.join("agent.cmd"), b"@echo off\r\n").unwrap();
        let fallback = AgentProfile::resolve_with_path("cursor", Some(dir.as_os_str())).unwrap();
        assert_eq!(fallback.command, "agent.cmd");

        let bare = dir.join("opencode");
        std::fs::write(&bare, b"").unwrap();
        let still_cmd = AgentProfile::resolve_with_path("opencode", Some(dir.as_os_str())).unwrap();
        assert_eq!(still_cmd.command, "opencode.cmd");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn codex_profile_is_codex_acp_without_fallback_or_env() {
        let profile = AgentProfile::resolve_with_path("codex", None).unwrap();
        assert_eq!(profile.name, "codex");
        assert_eq!(profile.command, "codex-acp");
        assert!(profile.args.is_empty());
        assert!(profile.env.is_empty());
        assert!(!profile.uses_cursor_extensions());

        let empty = scratch_dir("codex-empty");
        let absent = AgentProfile::resolve_with_path("codex", Some(empty.as_os_str())).unwrap();
        let _ = std::fs::remove_dir_all(&empty);
        assert_eq!(absent.command, "codex-acp");
        assert!(absent.args.is_empty());
        assert!(absent.env.is_empty());
    }

    #[test]
    fn claude_profile_is_claude_agent_acp_without_fallback_or_env() {
        let profile = AgentProfile::resolve_with_path("claude", None).unwrap();
        assert_eq!(profile.name, "claude");
        assert_eq!(profile.command, "claude-agent-acp");
        assert!(profile.args.is_empty());
        assert!(profile.env.is_empty());
        assert!(!profile.uses_cursor_extensions());

        let empty = scratch_dir("claude-empty");
        let absent = AgentProfile::resolve_with_path("claude", Some(empty.as_os_str())).unwrap();
        let _ = std::fs::remove_dir_all(&empty);
        assert_eq!(absent.command, "claude-agent-acp");
        assert!(absent.args.is_empty());
        assert!(absent.env.is_empty());
    }
}
