//! Busta JSON-RPC 2.0 del demone.
//!
//! Un messaggio per riga. Ogni richiesta riceve una risposta, le
//! notifiche (senza `id`) no.
//! shortcut: niente batch: un array riceve `invalid request`; quando
//! servirà, si valuta il batching sul trasporto WebSocket.

use serde_json::{json, Value};

pub(crate) const PARSE_ERROR: i64 = -32700;
pub(crate) const INVALID_REQUEST: i64 = -32600;
pub(crate) const METHOD_NOT_FOUND: i64 = -32601;
pub(crate) const INVALID_PARAMS: i64 = -32602;

pub(crate) enum Incoming {
    /// Notifica valida: nessuna risposta.
    Silent,
    /// Riga già pronta, di solito un errore di protocollo.
    Respond(String),
    Call {
        id: Value,
        method: String,
        params: Value,
    },
}

/// Interpreta una riga. Il dispaccio dei metodi sta nel demone.
pub(crate) fn parse(line: &str) -> Incoming {
    let value = match serde_json::from_str::<Value>(line) {
        Ok(value) => value,
        Err(_) => {
            return Incoming::Respond(error_response(Value::Null, PARSE_ERROR, "parse error"));
        }
    };
    let Some(object) = value.as_object() else {
        return Incoming::Respond(error_response(
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
        return Incoming::Respond(error_response(
            id.unwrap_or(Value::Null),
            INVALID_REQUEST,
            "invalid request",
        ));
    }
    let method = method.unwrap_or_default().to_string();
    let Some(id) = id else {
        return Incoming::Silent;
    };
    let params = object.get("params").cloned().unwrap_or(Value::Null);
    Incoming::Call { id, method, params }
}

pub(crate) fn result_response(id: Value, result: Value) -> String {
    json!({"jsonrpc": "2.0", "id": id, "result": result}).to_string()
}

pub(crate) fn error_response(id: Value, code: i64, message: &str) -> String {
    json!({"jsonrpc": "2.0", "id": id, "error": {"code": code, "message": message}}).to_string()
}

pub(crate) fn notification(method: &str, params: Value) -> String {
    json!({"jsonrpc": "2.0", "method": method, "params": params}).to_string()
}
