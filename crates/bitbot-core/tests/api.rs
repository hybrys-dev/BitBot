//! Prova l'API locale end-to-end: listener, permessi del socket e
//! risposte JSON-RPC su una vera connessione.

use bitbot_core::{listen, serve, socket::SocketEndpoint};
use interprocess::local_socket::tokio::prelude::*;
use interprocess::local_socket::tokio::Stream;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

fn scratch_endpoint(label: &str) -> SocketEndpoint {
    let dir = std::env::temp_dir().join(format!("bitbot-core-{label}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    SocketEndpoint::in_dir(dir, "test.sock")
}

async fn roundtrip(endpoint: &SocketEndpoint, request: &str) -> String {
    let stream = Stream::connect(endpoint.name()).await.unwrap();
    let (reader, mut writer) = stream.split();
    writer.write_all(request.as_bytes()).await.unwrap();
    writer.write_all(b"\n").await.unwrap();
    writer.flush().await.unwrap();
    let mut line = String::new();
    BufReader::new(reader).read_line(&mut line).await.unwrap();
    line.trim_end().to_string()
}

#[tokio::test]
async fn serves_version_and_unknown_method_over_the_socket() {
    let endpoint = scratch_endpoint("api");
    let listener = listen(&endpoint).unwrap();

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(endpoint.unix_path())
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o600, "il socket deve essere del solo utente");
    }

    let server = tokio::spawn(async move { serve(listener).await });
    let response = roundtrip(&endpoint, r#"{"jsonrpc":"2.0","id":1,"method":"version"}"#).await;
    assert!(response.contains(r#""name":"bitbotd""#), "{response}");
    assert!(response.contains(r#""id":1"#), "{response}");

    let response = roundtrip(&endpoint, r#"{"jsonrpc":"2.0","id":2,"method":"nope"}"#).await;
    assert!(response.contains(r#""code":-32601"#), "{response}");

    server.abort();
    let _ = server.await;
    let _ = std::fs::remove_dir_all(
        std::env::temp_dir().join(format!("bitbot-core-api-{}", std::process::id())),
    );
}
