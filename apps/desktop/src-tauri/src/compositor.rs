//! Lettura dello schermo intero sui compositor che lo espongono senza un crate in più.
//! I parser sono puri; il socket resta dietro `cfg` Linux.

#![cfg_attr(not(any(test, target_os = "linux")), allow(dead_code))]

use serde_json::Value;

/// `fullscreen_mode` di Sway: 0 assente, 1 schermo intero, 2 globale.
pub(crate) fn sway_tree_has_focused_fullscreen(node: &Value) -> bool {
    if fullscreen_mode(node) > 0 && subtree_has_focus(node) {
        return true;
    }
    for_each_child(node, sway_tree_has_focused_fullscreen)
}

fn fullscreen_mode(node: &Value) -> i64 {
    node.get("fullscreen_mode")
        .and_then(Value::as_i64)
        .unwrap_or(0)
}

fn subtree_has_focus(node: &Value) -> bool {
    if node.get("focused").and_then(Value::as_bool) == Some(true) {
        return true;
    }
    for_each_child(node, subtree_has_focus)
}

fn for_each_child(node: &Value, mut visit: impl FnMut(&Value) -> bool) -> bool {
    for key in ["nodes", "floating_nodes"] {
        let Some(children) = node.get(key).and_then(Value::as_array) else {
            continue;
        };
        for child in children {
            if visit(child) {
                return true;
            }
        }
    }
    false
}

/// Un albero senza `nodes` né `id` non è una risposta di `GET_TREE`.
pub(crate) fn sway_tree_is_usable(value: &Value) -> bool {
    value.get("nodes").is_some() || value.get("id").is_some()
}

/// Hyprland: 0 niente, 1 massimizzata, 2 schermo intero. Un booleano resta com'è.
pub(crate) fn hyprland_fullscreen_flag(value: &Value) -> Option<bool> {
    match value.get("fullscreen") {
        Some(Value::Number(number)) => Some(number.as_i64().unwrap_or(0) >= 2),
        Some(Value::Bool(flag)) => Some(*flag),
        Some(_) => None,
        None if value.is_object() => Some(false),
        None => None,
    }
}

#[cfg(target_os = "linux")]
pub(crate) fn detect_fullscreen() -> Option<bool> {
    if let Some(value) = sway_fullscreen() {
        return Some(value);
    }
    if let Some(value) = hyprland_fullscreen() {
        return Some(value);
    }
    if std::env::var_os("DISPLAY").is_some() {
        return crate::platform::x11::active_window_fullscreen();
    }
    None
}

#[cfg(target_os = "linux")]
fn sway_fullscreen() -> Option<bool> {
    let path = std::env::var("SWAYSOCK").ok()?;
    let body = i3_roundtrip(std::path::Path::new(&path), 4)?;
    let value = serde_json::from_slice::<Value>(&body).ok()?;
    if !sway_tree_is_usable(&value) {
        return None;
    }
    Some(sway_tree_has_focused_fullscreen(&value))
}

#[cfg(target_os = "linux")]
fn hyprland_fullscreen() -> Option<bool> {
    let signature = std::env::var("HYPRLAND_INSTANCE_SIGNATURE").ok()?;
    let runtime = std::env::var("XDG_RUNTIME_DIR").ok()?;
    let path = std::path::PathBuf::from(runtime)
        .join("hypr")
        .join(signature)
        .join(".socket.sock");
    let body = command_roundtrip(&path, b"j/activewindow")?;
    let value = serde_json::from_slice::<Value>(&body).ok()?;
    hyprland_fullscreen_flag(&value)
}

#[cfg(target_os = "linux")]
fn i3_roundtrip(path: &std::path::Path, message_type: u32) -> Option<Vec<u8>> {
    use std::io::{Read, Write};
    use std::os::unix::net::UnixStream;
    use std::time::Duration;

    let mut stream = UnixStream::connect(path).ok()?;
    stream
        .set_read_timeout(Some(Duration::from_millis(200)))
        .ok()?;
    stream
        .set_write_timeout(Some(Duration::from_millis(200)))
        .ok()?;
    let mut header = [0_u8; 14];
    header[..6].copy_from_slice(b"i3-ipc");
    header[10..14].copy_from_slice(&message_type.to_le_bytes());
    stream.write_all(&header).ok()?;
    let mut response = [0_u8; 14];
    stream.read_exact(&mut response).ok()?;
    if &response[..6] != b"i3-ipc" {
        return None;
    }
    let len = u32::from_le_bytes(response[6..10].try_into().ok()?) as usize;
    if len > 8 * 1024 * 1024 {
        return None;
    }
    let mut body = vec![0_u8; len];
    stream.read_exact(&mut body).ok()?;
    Some(body)
}

#[cfg(target_os = "linux")]
fn command_roundtrip(path: &std::path::Path, command: &[u8]) -> Option<Vec<u8>> {
    use std::io::{Read, Write};
    use std::os::unix::net::UnixStream;
    use std::time::Duration;

    let mut stream = UnixStream::connect(path).ok()?;
    stream
        .set_read_timeout(Some(Duration::from_millis(200)))
        .ok()?;
    stream
        .set_write_timeout(Some(Duration::from_millis(200)))
        .ok()?;
    stream.write_all(command).ok()?;
    let _ = stream.shutdown(std::net::Shutdown::Write);
    let mut body = Vec::new();
    stream.read_to_end(&mut body).ok()?;
    if body.is_empty() || body.len() > 1024 * 1024 {
        return None;
    }
    Some(body)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn sway_focused_fullscreen_node_retreats() {
        let tree = json!({
            "id": 1,
            "nodes": [{
                "id": 2,
                "focused": true,
                "fullscreen_mode": 1,
                "nodes": []
            }]
        });
        assert!(sway_tree_has_focused_fullscreen(&tree));
    }

    #[test]
    fn sway_fullscreen_parent_with_focused_child_retreats() {
        let tree = json!({
            "id": 1,
            "fullscreen_mode": 2,
            "nodes": [{
                "id": 2,
                "focused": true,
                "fullscreen_mode": 0
            }]
        });
        assert!(sway_tree_has_focused_fullscreen(&tree));
    }

    #[test]
    fn sway_fullscreen_on_another_workspace_does_not_retreat() {
        let tree = json!({
            "id": 1,
            "nodes": [
                {
                    "id": 2,
                    "focused": true,
                    "fullscreen_mode": 0,
                    "nodes": []
                },
                {
                    "id": 3,
                    "focused": false,
                    "fullscreen_mode": 1,
                    "nodes": []
                }
            ]
        });
        assert!(!sway_tree_has_focused_fullscreen(&tree));
    }

    #[test]
    fn sway_error_payload_is_not_a_tree() {
        assert!(!sway_tree_is_usable(&json!({"success": false})));
        assert!(sway_tree_is_usable(&json!({"id": 1, "nodes": []})));
    }

    #[test]
    fn hyprland_fullscreen_is_mode_two() {
        assert_eq!(
            hyprland_fullscreen_flag(&json!({"fullscreen": 0})),
            Some(false)
        );
        assert_eq!(
            hyprland_fullscreen_flag(&json!({"fullscreen": 1})),
            Some(false)
        );
        assert_eq!(
            hyprland_fullscreen_flag(&json!({"fullscreen": 2})),
            Some(true)
        );
        assert_eq!(
            hyprland_fullscreen_flag(&json!({"fullscreen": true})),
            Some(true)
        );
        assert_eq!(hyprland_fullscreen_flag(&json!({})), Some(false));
        assert_eq!(hyprland_fullscreen_flag(&json!([])), None);
    }
}
