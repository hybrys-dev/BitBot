//! Codice di piattaforma. Ogni sistema ha il suo modulo; gli altri compilano il ripiego.

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "windows")]
mod windows;
#[cfg(target_os = "linux")]
pub(crate) mod x11;

#[cfg(not(target_os = "macos"))]
#[cfg_attr(any(target_os = "linux", target_os = "windows"), allow(dead_code))]
mod fallback;

pub(crate) fn prepare() {
    #[cfg(target_os = "linux")]
    linux::prepare_backend();
}

pub(crate) fn open(app: &tauri::App) -> anyhow::Result<()> {
    #[cfg(target_os = "macos")]
    {
        macos::open(app)
    }
    #[cfg(target_os = "windows")]
    {
        windows::open(app)
    }
    #[cfg(target_os = "linux")]
    {
        linux::open(app)
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
    {
        fallback::open(app)
    }
}

#[cfg(any(target_os = "linux", target_os = "windows"))]
pub(crate) fn set_visible(window: &tauri::WebviewWindow, visible: bool, layer_shell: bool) {
    #[cfg(target_os = "linux")]
    linux::set_visible(window, visible, layer_shell);
    #[cfg(target_os = "windows")]
    {
        let _ = layer_shell;
        windows::set_visible(window, visible);
    }
}

#[cfg(target_os = "windows")]
pub(crate) fn windows_hwnd(window: &tauri::WebviewWindow) -> isize {
    windows::hwnd_of(window).unwrap_or(0)
}

#[cfg(target_os = "windows")]
pub(crate) fn windows_fullscreen(hwnd: isize) -> Option<bool> {
    windows::foreground_is_fullscreen(hwnd)
}
