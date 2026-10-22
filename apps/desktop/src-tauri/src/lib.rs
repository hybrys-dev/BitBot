//! Guscio dell'isola: una finestra trasparente in cima allo schermo.
//! La pillola mostra la riga di stato del demone. Non disegna Bit, schede o cockpit.

mod compositor;
mod daemon_link;
mod fullscreen;
mod geometry;
mod pill;
mod platform;
mod session;

use std::sync::{Arc, Mutex};

use anyhow::Context;
use tauri::{Emitter, Manager, Runtime, WebviewWindowBuilder};

#[cfg(not(target_os = "macos"))]
use tauri::{LogicalPosition, LogicalSize, WebviewUrl, WebviewWindow};

pub(crate) const WINDOW_LABEL: &str = "island";

/// Ultima riga mostrata. Il comando `pill_label` la legge all'avvio della pagina,
/// gli eventi `pill-label` la aggiornano dopo.
#[derive(Clone)]
struct PillText(Arc<Mutex<String>>);

impl PillText {
    fn new() -> Self {
        Self(Arc::new(Mutex::new(pill::REST_LABEL.to_string())))
    }

    fn set(&self, label: &str) {
        *self.0.lock().unwrap_or_else(|error| error.into_inner()) = label.to_string();
    }

    fn get(&self) -> String {
        self.0
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .clone()
    }
}

#[tauri::command]
fn pill_label(state: tauri::State<'_, PillText>) -> String {
    state.get()
}

pub fn run() -> anyhow::Result<()> {
    platform::prepare();
    let pill_text = PillText::new();
    let pill_for_page = pill_text.clone();
    let pill_for_watch = pill_text.clone();
    let builder = tauri::Builder::default().manage(pill_text);
    #[cfg(target_os = "macos")]
    let builder = builder.plugin(tauri_nspanel::init());
    builder
        .invoke_handler(tauri::generate_handler![pill_label])
        .on_page_load(move |webview, payload| {
            if payload.event() != tauri::webview::PageLoadEvent::Finished {
                return;
            }
            let _ = webview.emit(pill::PILL_EVENT, pill_for_page.get());
        })
        .setup(move |app| {
            platform::open(app)?;
            let handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                daemon_link::watch(
                    bitbot_core::socket::SocketEndpoint::default_endpoint(),
                    move |text| {
                        pill_for_watch.set(text);
                        let _ = handle.emit(pill::PILL_EVENT, text.to_string());
                    },
                )
                .await;
            });
            Ok(())
        })
        .run(tauri::generate_context!())
        .context("avvio del guscio Tauri")
}

pub(crate) fn configure_window<R, M>(
    builder: WebviewWindowBuilder<'_, R, M>,
) -> WebviewWindowBuilder<'_, R, M>
where
    R: Runtime,
    M: Manager<R>,
{
    builder
        .title("BitBot")
        .inner_size(geometry::ISLAND_WIDTH, geometry::ISLAND_HEIGHT)
        .min_inner_size(geometry::ISLAND_WIDTH, geometry::ISLAND_HEIGHT)
        .max_inner_size(geometry::ISLAND_WIDTH, geometry::ISLAND_HEIGHT)
        .resizable(false)
        .decorations(false)
        .transparent(true)
        .always_on_top(true)
        .skip_taskbar(true)
        .focused(false)
        .focusable(false)
        .visible(false)
        .shadow(false)
}

#[cfg(not(target_os = "macos"))]
pub(crate) fn create_window(app: &tauri::App) -> anyhow::Result<WebviewWindow> {
    configure_window(WebviewWindowBuilder::new(
        app,
        WINDOW_LABEL,
        WebviewUrl::App("index.html".into()),
    ))
    .build()
    .context("creazione della finestra dell'isola")
}

#[cfg(not(target_os = "macos"))]
pub(crate) fn place_top_center(window: &WebviewWindow, top_offset: f64) -> anyhow::Result<()> {
    // All'avvio della sessione lo schermo può non esserci ancora: si aspetta un po'
    // invece di lasciare l'isola nell'angolo.
    let mut monitor = None;
    for _ in 0..20 {
        monitor = window.primary_monitor().unwrap_or(None);
        if monitor.is_some() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(250));
    }
    let Some(monitor) = monitor else {
        eprintln!("bitbot-desktop: schermo principale assente, posizione predefinita");
        return Ok(());
    };
    let scale = monitor.scale_factor();
    if scale <= 0.0 {
        eprintln!("bitbot-desktop: fattore di scala non valido, posizione predefinita");
        return Ok(());
    }
    let origin_x = f64::from(monitor.position().x) / scale;
    let origin_y = f64::from(monitor.position().y) / scale;
    let width = f64::from(monitor.size().width) / scale;
    let (x, y) = geometry::top_center_origin(
        origin_x,
        origin_y,
        width,
        geometry::ISLAND_WIDTH,
        top_offset,
    );
    window
        .set_position(LogicalPosition::new(x, y))
        .context("posizione dell'isola")?;
    window
        .set_size(LogicalSize::new(
            geometry::ISLAND_WIDTH,
            geometry::ISLAND_HEIGHT,
        ))
        .context("dimensione dell'isola")?;
    Ok(())
}
