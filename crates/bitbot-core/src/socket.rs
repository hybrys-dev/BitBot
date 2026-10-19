//! Dove vive il socket locale del demone.
//!
//! Su Unix è un file con permessi del solo utente: dentro
//! `XDG_RUNTIME_DIR` quando c'è (cartella già 0700), altrimenti nella
//! cartella temporanea con il socket a 0600. Su Windows è una named
//! pipe. Il nome comprende un hash della cartella: il tubo è globale
//! alla macchina e due cartelle non possono condividere `test.sock`.
//! L'ACL resta quella predefinita del listener, che non la espone.

#[cfg(any(not(unix), test))]
use std::path::Path;
use std::path::PathBuf;

#[cfg(unix)]
use interprocess::local_socket::ToFsName as _;
use interprocess::local_socket::{GenericFilePath, Name};
#[cfg(not(unix))]
use interprocess::local_socket::{GenericNamespaced, ToFsName as _, ToNsName as _};

/// Nome del socket: percorso Unix o nome della named pipe.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SocketEndpoint {
    name: Name<'static>,
    /// Percorso mostrato all'avvio: file su Unix, `\\.\pipe\...` su Windows.
    label: String,
    /// Percorso del file su Unix, per i permessi e la rimozione.
    #[cfg(unix)]
    unix_path: PathBuf,
}

impl SocketEndpoint {
    /// Endpoint predefinito dell'utente, `bitbotd.sock`.
    pub fn default_endpoint() -> Self {
        Self::in_dir(default_dir(), "bitbotd.sock")
    }

    /// Endpoint in una cartella scelta, per i test.
    pub fn in_dir(dir: PathBuf, file: &str) -> Self {
        #[cfg(unix)]
        {
            let path = dir.join(file);
            let label = path.display().to_string();
            let name = path
                .clone()
                .to_fs_name::<GenericFilePath>()
                .expect("nome del socket Unix valido");
            Self {
                name,
                label,
                unix_path: path,
            }
        }
        #[cfg(not(unix))]
        {
            let label = windows_pipe_path(&dir, file);
            let name = local_pipe_name(&label);
            Self { name, label }
        }
    }

    /// Nome per il listener e i client.
    pub fn name(&self) -> Name<'static> {
        self.name.borrow().into_owned()
    }

    /// Percorso del file del socket, solo su Unix.
    #[cfg(unix)]
    pub fn unix_path(&self) -> &std::path::Path {
        &self.unix_path
    }

    /// Rimuove un file socket rimasto da una corsa precedente.
    /// Su Windows non c'è un file: il listener ricicla il nome da solo.
    pub fn remove_stale(&self) {
        #[cfg(unix)]
        {
            let _ = std::fs::remove_file(&self.unix_path);
        }
    }

    /// Stringa da mostrare all'utente all'avvio.
    pub fn display(&self) -> String {
        self.label.clone()
    }
}

fn default_dir() -> PathBuf {
    #[cfg(unix)]
    {
        std::env::var_os("XDG_RUNTIME_DIR")
            .or_else(|| std::env::var_os("TMPDIR"))
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("/tmp"))
    }
    #[cfg(not(unix))]
    {
        // `TMP`/`TEMP` dell'utente: due profili non ottengono la stessa pipe.
        std::env::temp_dir()
    }
}

/// `to_ns_name` antepone da solo `\\.\pipe\`. Il ripiego usa il nome nudo,
/// altrimenti il prefisso compare due volte e il listener non si apre.
#[cfg(not(unix))]
fn local_pipe_name(pipe: &str) -> Name<'static> {
    if let Ok(name) = pipe.to_owned().to_fs_name::<GenericFilePath>() {
        return name;
    }
    let bare = pipe.strip_prefix(r"\\.\pipe\").unwrap_or(pipe);
    bare.to_owned()
        .to_ns_name::<GenericNamespaced>()
        .expect("nome della named pipe valido")
}

/// Nome di pipe senza barre: Windows rifiuta `\` nel tratto dopo `pipe\`.
#[cfg(any(not(unix), test))]
fn windows_pipe_path(dir: &Path, file: &str) -> String {
    format!(
        r"\\.\pipe\bitbot-{}-{}",
        directory_token(dir),
        sanitize_pipe_file(file)
    )
}

#[cfg(any(not(unix), test))]
fn directory_token(dir: &Path) -> String {
    let mut text = dir.to_string_lossy().replace('/', "\\");
    while text.ends_with('\\') {
        text.pop();
    }
    text.make_ascii_lowercase();
    format!("{:016x}", fnv1a64(text.as_bytes()))
}

#[cfg(any(not(unix), test))]
fn sanitize_pipe_file(file: &str) -> String {
    let mut out = String::new();
    for c in file.chars() {
        if out.len() >= 64 {
            break;
        }
        if c.is_ascii_alphanumeric() || c == '.' || c == '_' || c == '-' {
            out.push(c);
        } else {
            out.push('_');
        }
    }
    if out.is_empty() {
        "bitbotd.sock".to_string()
    } else {
        out
    }
}

#[cfg(any(not(unix), test))]
fn fnv1a64(bytes: &[u8]) -> u64 {
    let mut hash = 0xcbf29ce484222325;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;

    #[test]
    fn different_directories_do_not_share_an_endpoint() {
        let root = std::env::temp_dir().join(format!("bitbot-sock-{}", std::process::id()));
        let left = SocketEndpoint::in_dir(root.join("a"), "bitbotd.sock");
        let right = SocketEndpoint::in_dir(root.join("b"), "bitbotd.sock");
        assert_ne!(left, right);
        assert_ne!(left.display(), right.display());
        assert!(
            left.display().contains("bitbotd.sock"),
            "{}",
            left.display()
        );
        assert_eq!(SocketEndpoint::in_dir(root.join("a"), "bitbotd.sock"), left);

        #[cfg(unix)]
        {
            assert!(left.unix_path().ends_with("bitbotd.sock"));
            assert!(left.unix_path().starts_with(root.join("a")));
        }
        #[cfg(windows)]
        {
            assert!(left.display().starts_with(r"\\.\pipe\"));
            let rest = left.display().trim_start_matches(r"\\.\pipe\");
            assert!(!rest.contains('\\'), "{}", left.display());
        }
    }

    #[test]
    fn default_endpoint_is_stable() {
        let first = SocketEndpoint::default_endpoint();
        let second = SocketEndpoint::default_endpoint();
        assert_eq!(first, second);
        assert!(first.display().contains("bitbotd"), "{}", first.display());
    }

    #[test]
    fn windows_pipe_path_folds_case_and_slashes_and_drops_backslashes() {
        let upper = super::windows_pipe_path(Path::new(r"C:\Temp\Bitbot"), "bitbotd.sock");
        let lower = super::windows_pipe_path(Path::new("C:/temp/bitbot/"), "bitbotd.sock");
        assert_eq!(upper, lower);
        assert!(upper.starts_with(r"\\.\pipe\bitbot-"), "{upper}");
        let rest = upper.trim_start_matches(r"\\.\pipe\");
        assert!(!rest.contains('\\'), "{upper}");
        assert!(rest.ends_with("-bitbotd.sock"), "{upper}");

        let dirty = super::windows_pipe_path(Path::new(r"C:\Temp\Bitbot"), r"a\b.sock");
        let dirty_rest = dirty.trim_start_matches(r"\\.\pipe\");
        assert!(!dirty_rest.contains('\\'), "{dirty}");
        assert!(dirty_rest.ends_with("-a_b.sock"), "{dirty}");
        assert_ne!(
            upper,
            super::windows_pipe_path(Path::new(r"C:\Temp\Other"), "bitbotd.sock")
        );
    }
}
