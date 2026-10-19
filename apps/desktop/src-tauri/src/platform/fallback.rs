//! Sistemi diversi da macOS, Windows e Linux: la finestra c'è, senza le API native.

use anyhow::Context;

pub(crate) fn open(app: &tauri::App) -> anyhow::Result<()> {
    eprintln!("bitbot-desktop: sistema senza modulo nativo, finestra sempre in primo piano");
    let window = crate::create_window(app)?;
    crate::place_top_center(&window, 0.0)?;
    window.show().context("visualizzazione dell'isola")?;
    Ok(())
}
