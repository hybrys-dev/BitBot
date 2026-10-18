//! Demone `bitbotd`: possiede processi, stato e chiavi. Qui c'è il
//! server dell'API locale, JSON-RPC 2.0 una riga per messaggio.

pub mod rpc;
pub mod socket;

use std::io;

use interprocess::local_socket::tokio::prelude::*;
use interprocess::local_socket::{tokio::Listener, tokio::Stream, ListenerOptions};
use socket::SocketEndpoint;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

/// Apre il listener sull'endpoint: un socket nuovo sostituisce quello
/// rimasto da una corsa precedente. Va chiamato dentro un runtime tokio.
pub fn listen(endpoint: &SocketEndpoint) -> io::Result<Listener> {
    endpoint.remove_stale();
    let listener = ListenerOptions::new()
        .name(endpoint.name())
        .create_tokio()?;
    restrict_permissions(endpoint);
    Ok(listener)
}

/// Socket al solo utente: 0600 su Unix, ACL predefinita su Windows.
#[cfg(unix)]
fn restrict_permissions(endpoint: &SocketEndpoint) {
    use std::os::unix::fs::PermissionsExt;
    let _ = std::fs::set_permissions(endpoint.unix_path(), std::fs::Permissions::from_mode(0o600));
}

#[cfg(not(unix))]
fn restrict_permissions(_endpoint: &SocketEndpoint) {}

/// Accetta connessioni finché la task non viene annullata.
pub async fn serve(listener: Listener) -> io::Result<()> {
    loop {
        let stream = listener.accept().await?;
        tokio::spawn(async move {
            let _ = handle_connection(stream).await;
        });
    }
}

async fn handle_connection(stream: Stream) -> io::Result<()> {
    let (reader, mut writer) = stream.split();
    let mut lines = BufReader::new(reader).lines();
    while let Some(line) = lines.next_line().await? {
        if let Some(response) = rpc::handle_line(&line) {
            writer.write_all(response.as_bytes()).await?;
            writer.write_all(b"\n").await?;
            writer.flush().await?;
        }
    }
    Ok(())
}
