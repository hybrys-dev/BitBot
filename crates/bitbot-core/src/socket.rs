//! Dove vive il socket locale del demone.
//!
//! Su Unix è un file con permessi del solo utente: dentro
//! `XDG_RUNTIME_DIR` quando c'è (cartella già 0700), altrimenti nella
//! cartella temporanea con il socket a 0600. Su Windows è una named
//! pipe: l'ACL predefinita la lascia al solo utente corrente.

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
    /// Percorso del file su Unix, per i permessi e i messaggi.
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
        let path = dir.join(file);
        #[cfg(unix)]
        {
            let name = path
                .clone()
                .to_fs_name::<GenericFilePath>()
                .expect("nome del socket Unix valido");
            Self {
                name,
                unix_path: path,
            }
        }
        #[cfg(not(unix))]
        {
            let _ = dir;
            let pipe = format!(r"\\.\pipe\{file}");
            let name = pipe
                .clone()
                .to_fs_name::<GenericFilePath>()
                .or_else(|_| pipe.to_ns_name::<GenericNamespaced>())
                .expect("nome della named pipe valido");
            Self { name }
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
    /// Su Windows non c'è un file: `reclaim_name` del listener basta.
    pub fn remove_stale(&self) {
        #[cfg(unix)]
        {
            let _ = std::fs::remove_file(&self.unix_path);
        }
    }

    /// Stringa da mostrare all'utente all'avvio.
    pub fn display(&self) -> String {
        #[cfg(unix)]
        {
            self.unix_path.display().to_string()
        }
        #[cfg(not(unix))]
        {
            format!("{:?}", self.name)
        }
    }
}

fn default_dir() -> PathBuf {
    std::env::var_os("XDG_RUNTIME_DIR")
        .or_else(|| std::env::var_os("TMPDIR"))
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/tmp"))
}
