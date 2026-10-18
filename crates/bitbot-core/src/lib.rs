//! Demone `bitbotd`: possiede processi, stato e chiavi. Qui c'è il
//! server dell'API locale, JSON-RPC 2.0 una riga per messaggio.

mod daemon;
pub mod rpc;
pub mod socket;

pub use daemon::Daemon;

use std::io;

use daemon::LineOutcome;
use interprocess::local_socket::tokio::prelude::*;
use interprocess::local_socket::{tokio::Listener, tokio::Stream, ListenerOptions};
use socket::SocketEndpoint;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::sync::mpsc;

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
pub async fn serve(listener: Listener, daemon: Daemon) -> io::Result<()> {
    loop {
        let stream = listener.accept().await?;
        let daemon = daemon.clone();
        tokio::spawn(async move {
            let _ = handle_connection(stream, daemon).await;
        });
    }
}

async fn handle_connection(stream: Stream, daemon: Daemon) -> io::Result<()> {
    let (outbound, mut inbound) = mpsc::unbounded_channel();
    let replies = outbound.clone();
    let subscription = daemon.subscribe(outbound);
    let (reader, mut writer) = stream.split();
    let writer = tokio::spawn(async move {
        while let Some(line) = inbound.recv().await {
            if writer.write_all(line.as_bytes()).await.is_err() {
                break;
            }
            if writer.write_all(b"\n").await.is_err() {
                break;
            }
            if writer.flush().await.is_err() {
                break;
            }
        }
    });
    let mut lines = BufReader::new(reader).lines();
    while let Some(line) = lines.next_line().await? {
        match daemon.handle(&line).await {
            LineOutcome::Empty => {}
            LineOutcome::Message(message) => {
                let _ = replies.send(message);
            }
            LineOutcome::Start { message, launch } => {
                let _ = replies.send(message);
                daemon.launch(launch);
            }
        }
    }
    daemon.unsubscribe(subscription);
    drop(replies);
    let _ = writer.await;
    Ok(())
}
