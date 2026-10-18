//! Dispatch JSON-RPC 2.0 del demone.
//!
//! Un messaggio per riga. Ogni richiesta riceve una risposta, le
//! notifiche (senza `id`) no.
//! shortcut: niente batch: un array riceve `invalid request`; quando
//! servirà, si valuta il batching sul trasporto WebSocket.

use bitbot_protocol::{DaemonVersion, VERSION_METHOD};
use serde_json::{json, Value};

const PARSE_ERROR: i64 = -32700;
const INVALID_REQUEST: i64 = -32600;
const METHOD_NOT_FOUND: i64 = -32601;

/// Elabora una riga e restituisce la riga di risposta, se prevista.
pub(crate) fn handle_line(line: &str) -> Option<String> {
    let value = match serde_json::from_str::<Value>(line) {
        Ok(value) => value,
        Err(_) => {
            return Some(error_response(Value::Null, PARSE_ERROR, "parse error"));
        }
    };
    let Some(object) = value.as_object() else {
        return Some(error_response(
            Value::Null,
            INVALID_REQUEST,
            "invalid request",
        ));
    };
    let id = object.get("id").cloned();
    let method = object.get("method").and_then(Value::as_str);
    let is_valid = object.get("jsonrpc").and_then(Value::as_str) == Some("2.0")
        && method.is_some()
        && !method.is_some_and(str::is_empty);
    if !is_valid {
        return Some(error_response(
            id.unwrap_or(Value::Null),
            INVALID_REQUEST,
            "invalid request",
        ));
    }
    let method = method.unwrap_or_default();
    // Le notifiche non ricevano risposta, nemmeno per metodo sconosciuto.
    let id = id?;
    let result = match method {
        VERSION_METHOD => serde_json::to_value(DaemonVersion {
            name: "bitbotd".to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
        })
        .expect("DaemonVersion è serializzabile"),
        _ => {
            return Some(error_response(
                id,
                METHOD_NOT_FOUND,
                &format!("method not found: {method}"),
            ));
        }
    };
    Some(json!({"jsonrpc": "2.0", "id": id, "result": result}).to_string())
}

fn error_response(id: Value, code: i64, message: &str) -> String {
    json!({"jsonrpc": "2.0", "id": id, "error": {"code": code, "message": message}}).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_returns_daemon_identity() {
        let response = handle_line(r#"{"jsonrpc":"2.0","id":1,"method":"version"}"#).unwrap();
        let value: Value = serde_json::from_str(&response).unwrap();
        assert_eq!(value["id"], 1);
        assert_eq!(value["result"]["name"], "bitbotd");
        assert_eq!(value["result"]["version"], env!("CARGO_PKG_VERSION"));
    }

    #[test]
    fn unknown_method_is_reported_with_its_name() {
        let response = handle_line(r#"{"jsonrpc":"2.0","id":7,"method":"nope"}"#).unwrap();
        assert!(response.contains(r#""code":-32601"#));
        assert!(response.contains("method not found: nope"));
    }

    #[test]
    fn notifications_and_garbage() {
        assert_eq!(handle_line(r#"{"jsonrpc":"2.0","method":"version"}"#), None);
        let parse_error = handle_line(r#"{"jsonrpc":"2.0","id":3,"method":"nope"},"#).unwrap();
        let value: Value = serde_json::from_str(&parse_error).unwrap();
        assert_eq!(value["id"], Value::Null);
        assert_eq!(value["error"]["code"], PARSE_ERROR);
        for garbage in [
            r#"[{"jsonrpc":"2.0","id":1,"method":"version"}]"#,
            r#"{"jsonrpc":"1.0","id":2,"method":"version"}"#,
            r#"{"id":4,"method":"version"}"#,
        ] {
            let value: Value = serde_json::from_str(&handle_line(garbage).unwrap()).unwrap();
            assert_eq!(value["error"]["code"], INVALID_REQUEST, "{garbage}");
        }
        // La richiesta invalida con id noto risponde con quell'id.
        let value: Value = serde_json::from_str(
            &handle_line(r#"{"jsonrpc":"1.0","id":2,"method":"version"}"#).unwrap(),
        )
        .unwrap();
        assert_eq!(value["id"], 2);
    }
}
