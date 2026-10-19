//! Ritiro a schermo intero. Su macOS lo fa il collection behavior del pannello,
//! che non ha `FullScreenAuxiliary` e quindi non entra in quello spazio.

#[cfg(any(target_os = "linux", target_os = "windows"))]
use std::thread;
#[cfg(any(target_os = "linux", target_os = "windows"))]
use std::time::Duration;

use tauri::AppHandle;
#[cfg(any(target_os = "linux", target_os = "windows"))]
use tauri::Manager;

pub(crate) fn watch(app: AppHandle, layer_shell: bool) {
    #[cfg(any(target_os = "linux", target_os = "windows"))]
    spawn(app, layer_shell);
    #[cfg(not(any(target_os = "linux", target_os = "windows")))]
    {
        let _ = (app, layer_shell);
    }
}

#[cfg(any(target_os = "linux", target_os = "windows"))]
fn spawn(app: AppHandle, layer_shell: bool) {
    let Some(window) = app.get_webview_window(crate::WINDOW_LABEL) else {
        eprintln!(
            "bitbot-desktop: finestra dell'isola assente, niente controllo dello schermo intero"
        );
        return;
    };
    // L'hwnd va letto sul thread che ha creato la finestra.
    #[cfg(target_os = "windows")]
    let hwnd = crate::platform::windows_hwnd(&window);
    let watcher = thread::Builder::new()
        .name("island-fullscreen".into())
        .spawn(move || {
            let mut shown = true;
            loop {
                #[cfg(target_os = "linux")]
                let fullscreen = crate::compositor::detect_fullscreen();
                #[cfg(target_os = "windows")]
                let fullscreen = crate::platform::windows_fullscreen(hwnd);
                if let Some(fullscreen) = fullscreen {
                    let want = crate::geometry::island_visible(fullscreen);
                    if want != shown {
                        let window = window.clone();
                        let app = app.clone();
                        let scheduled = app.run_on_main_thread(move || {
                            crate::platform::set_visible(&window, want, layer_shell);
                        });
                        if scheduled.is_err() {
                            break;
                        }
                        shown = want;
                    }
                }
                thread::sleep(Duration::from_millis(400));
            }
        });
    if watcher.is_err() {
        eprintln!("bitbot-desktop: controllo dello schermo intero non avviato");
    }
}
