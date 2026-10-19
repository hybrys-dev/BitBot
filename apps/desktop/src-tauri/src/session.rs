//! Scelta del backend Linux a partire dalle variabili di sessione.
//! Resta pura così i test non dipendono dal desktop della macchina.

#![cfg_attr(not(any(test, target_os = "linux")), allow(dead_code))]

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SessionEnv {
    pub session_type: String,
    pub current_desktop: String,
    pub session_desktop: String,
    pub wayland_display: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LinuxPresentation {
    /// Overlay layer-shell ancorato al bordo alto.
    LayerShell,
    /// GNOME non ha layer-shell: finestra dock attraverso XWayland.
    GnomeDock,
    /// X11, oppure un Wayland che non sappiamo ancora classificare come GNOME.
    X11Above,
}

impl SessionEnv {
    #[cfg(target_os = "linux")]
    pub(crate) fn from_process() -> Self {
        Self {
            session_type: env_var("XDG_SESSION_TYPE"),
            current_desktop: env_var("XDG_CURRENT_DESKTOP"),
            session_desktop: env_var("XDG_SESSION_DESKTOP"),
            wayland_display: env_var("WAYLAND_DISPLAY"),
        }
    }
}

#[cfg(target_os = "linux")]
fn env_var(name: &str) -> String {
    std::env::var(name).unwrap_or_default()
}

pub(crate) fn linux_presentation(env: &SessionEnv) -> LinuxPresentation {
    if !is_wayland(env) {
        return LinuxPresentation::X11Above;
    }
    if is_gnome(&env.current_desktop) || is_gnome(&env.session_desktop) {
        LinuxPresentation::GnomeDock
    } else {
        LinuxPresentation::LayerShell
    }
}

fn is_wayland(env: &SessionEnv) -> bool {
    let session = env.session_type.trim().to_ascii_lowercase();
    if session == "x11" || session == "tty" {
        return false;
    }
    session == "wayland" || !env.wayland_display.trim().is_empty()
}

fn is_gnome(desktop: &str) -> bool {
    desktop.split([':', ';', ',']).any(|token| {
        let token = token.trim().to_ascii_lowercase();
        token == "gnome" || token.starts_with("gnome-") || token.starts_with("gnome_")
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env(
        session_type: &str,
        current_desktop: &str,
        session_desktop: &str,
        wayland_display: &str,
    ) -> SessionEnv {
        SessionEnv {
            session_type: session_type.to_string(),
            current_desktop: current_desktop.to_string(),
            session_desktop: session_desktop.to_string(),
            wayland_display: wayland_display.to_string(),
        }
    }

    #[test]
    fn ubuntu_gnome_on_wayland_is_a_dock() {
        let presentation =
            linux_presentation(&env("wayland", "ubuntu:GNOME", "ubuntu", "wayland-0"));
        assert_eq!(presentation, LinuxPresentation::GnomeDock);
    }

    #[test]
    fn gnome_classic_on_wayland_is_a_dock() {
        let presentation = linux_presentation(&env("wayland", "GNOME-Classic", "", "wayland-0"));
        assert_eq!(presentation, LinuxPresentation::GnomeDock);
    }

    #[test]
    fn gnome_on_x11_stays_above() {
        let presentation = linux_presentation(&env("x11", "ubuntu:GNOME", "ubuntu", ""));
        assert_eq!(presentation, LinuxPresentation::X11Above);
    }

    #[test]
    fn kde_hyprland_and_sway_use_layer_shell() {
        for desktop in ["KDE", "Hyprland", "sway"] {
            let presentation = linux_presentation(&env("wayland", desktop, "", "wayland-0"));
            assert_eq!(presentation, LinuxPresentation::LayerShell, "{desktop}");
        }
    }

    #[test]
    fn wayland_display_without_session_type_is_layer_shell() {
        let presentation = linux_presentation(&env("", "Hyprland", "", "wayland-1"));
        assert_eq!(presentation, LinuxPresentation::LayerShell);
    }

    #[test]
    fn x11_session_wins_over_a_stale_wayland_display() {
        let presentation = linux_presentation(&env("x11", "KDE", "", "wayland-0"));
        assert_eq!(presentation, LinuxPresentation::X11Above);
    }

    #[test]
    fn empty_session_falls_back_to_always_on_top() {
        let presentation = linux_presentation(&env("", "", "", ""));
        assert_eq!(presentation, LinuxPresentation::X11Above);
    }
}
