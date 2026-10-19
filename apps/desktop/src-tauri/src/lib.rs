//! Guscio dell'isola: una finestra trasparente in cima allo schermo.
//! Lo spike non disegna Bit, pillole o cockpit.

mod compositor;
mod fullscreen;
mod geometry;
mod platform;
mod session;

use anyhow::Context;
use tauri::{Manager, Runtime, WebviewWindowBuilder};

#[cfg(not(target_os = "macos"))]
use tauri::{LogicalPosition, LogicalSize, WebviewUrl, WebviewWindow};

pub(crate) const WINDOW_LABEL: &str = "island";

pub fn run() -> anyhow::Result<()> {
    platform::prepare();
    let builder = tauri::Builder::default();
    #[cfg(target_os = "macos")]
    let builder = builder.plugin(tauri_nspanel::init());
    builder
        .setup(|app| {
            platform::open(app)?;
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
pub(crate) fn place_top_center(window: &WebviewWindow) -> anyhow::Result<()> {
    let Some(monitor) = window
        .primary_monitor()
        .context("lettura dello schermo principale")?
    else {
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
    let (x, y) = geometry::top_center_origin(origin_x, origin_y, width, geometry::ISLAND_WIDTH);
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
