//! Lettura di una risposta dal terminale.

use std::io::{IsTerminal, Write};

use agent_client_protocol::Error;

pub(crate) fn acp_error(message: impl ToString) -> Error {
    Error::into_internal_error(std::io::Error::other(message.to_string()))
}

/// Errore da mostrare quando stdin non è un terminale.
pub(crate) fn missing_terminal_message(is_terminal: bool, action: &str) -> Option<String> {
    if is_terminal {
        None
    } else {
        Some(format!("stdin non è un terminale: non posso {action}"))
    }
}

pub(crate) fn require_terminal(action: &str) -> Result<(), Error> {
    match missing_terminal_message(std::io::stdin().is_terminal(), action) {
        Some(message) => Err(acp_error(message)),
        None => Ok(()),
    }
}

pub(crate) fn flush_stdout() {
    let _ = std::io::stdout().flush();
}

pub(crate) async fn read_stdin_line() -> Result<String, Error> {
    let read = tokio::task::spawn_blocking(|| {
        let mut line = String::new();
        let read = std::io::stdin().read_line(&mut line)?;
        if read == 0 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::UnexpectedEof,
                "stdin chiuso",
            ));
        }
        Ok(line)
    })
    .await
    .map_err(|err| acp_error(format!("lettura di stdin interrotta: {err}")))?;
    read.map_err(Error::into_internal_error)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_terminal_explains_the_action() {
        assert_eq!(missing_terminal_message(true, "chiedere il permesso"), None);
        assert_eq!(
            missing_terminal_message(false, "chiedere il permesso").as_deref(),
            Some("stdin non è un terminale: non posso chiedere il permesso")
        );
    }
}
