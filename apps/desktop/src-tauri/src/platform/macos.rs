//! macOS: pannello non attivante sopra la barra dei menu.
//! Senza notch è una striscia sul bordo alto. A schermo intero manca
//! `FullScreenAuxiliary`, così AppKit non lo porta in quello spazio.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use objc2::runtime::NSObjectProtocol;
use objc2::{sel, ClassType, MainThreadMarker};
use objc2_app_kit::{NSApplication, NSApplicationActivationPolicy, NSScreen, NSWindow};
use objc2_foundation::{NSPoint, NSRect, NSSize};
use tauri::{AppHandle, WebviewUrl};
use tauri_nspanel::{CollectionBehavior, Panel, PanelBuilder, PanelLevel, StyleMask};

use crate::geometry::{self, RectF};

tauri_nspanel::tauri_panel! {
    panel!(IslandPanel {
        config: {
            can_become_key_window: false,
            can_become_main_window: false,
            is_floating_panel: true,
            hides_on_deactivate: false
        }
    })
}

pub(crate) fn open(app: &tauri::App) -> anyhow::Result<()> {
    eprintln!("bitbot-desktop: isola su macOS, pannello non attivante sopra la barra dei menu");
    // Durante la creazione il plugin passa a Prohibited e poi ripristina la policy.
    // Accessory va impostata prima, così il ripristino non torna a un'app normale.
    become_accessory();
    let panel = PanelBuilder::<_, IslandPanel>::new(app.handle(), crate::WINDOW_LABEL)
        .url(WebviewUrl::App("index.html".into()))
        .title("BitBot")
        .with_window(|builder| crate::configure_window(builder))
        .level(PanelLevel::Status)
        .floating(true)
        .has_shadow(false)
        .transparent(true)
        .hides_on_deactivate(false)
        .ignores_mouse_events(true)
        .no_activate(true)
        .add_style_mask(StyleMask::empty().nonactivating_panel())
        .collection_behavior(
            CollectionBehavior::new()
                .can_join_all_spaces()
                .stationary()
                .ignores_cycle()
                .full_screen_none(),
        )
        .build()
        .context("creazione del pannello macOS")?;
    become_accessory();
    place(panel.as_ref());
    panel.show();
    watch_hits(app.handle().clone(), panel);
    // Niente polling: senza FullScreenAuxiliary il pannello non entra negli spazi a schermo intero.
    crate::fullscreen::watch(app.handle().clone(), false);
    Ok(())
}

fn become_accessory() {
    let Some(marker) = MainThreadMarker::new() else {
        return;
    };
    let application = NSApplication::sharedApplication(marker);
    let _ = application.setActivationPolicy(NSApplicationActivationPolicy::Accessory);
}

fn place(panel: &dyn Panel<tauri::Wry>) {
    let Some(marker) = MainThreadMarker::new() else {
        eprintln!("bitbot-desktop: posizionamento fuori dal thread principale");
        return;
    };
    let Some(screen) = NSScreen::mainScreen(marker) else {
        eprintln!("bitbot-desktop: schermo principale assente, posizione predefinita");
        return;
    };
    let cocoa = screen.frame();
    let screen_rect = RectF {
        x: cocoa.origin.x as f64,
        y: cocoa.origin.y as f64,
        width: cocoa.size.width as f64,
        height: cocoa.size.height as f64,
    };
    let (left, right) = notch_areas(&screen);
    let frame = geometry::macos_island_frame(screen_rect, left, right, safe_top(&screen));
    if frame.in_notch {
        eprintln!("bitbot-desktop: notch rilevato, pannello nella barra dei menu");
    } else {
        eprintln!("bitbot-desktop: nessuno notch, striscia sul bordo alto");
    }
    let origin = NSPoint::new(frame.x, frame.y);
    let size = NSSize::new(frame.width, frame.height);
    panel
        .as_panel()
        .as_super()
        .setFrame_display(NSRect::new(origin, size), true);
}

fn notch_areas(screen: &NSScreen) -> (Option<RectF>, Option<RectF>) {
    if !screen.respondsToSelector(sel!(auxiliaryTopLeftArea)) {
        return (None, None);
    }
    (
        area(screen.auxiliaryTopLeftArea()),
        area(screen.auxiliaryTopRightArea()),
    )
}

fn safe_top(screen: &NSScreen) -> f64 {
    if !screen.respondsToSelector(sel!(safeAreaInsets)) {
        return 0.0;
    }
    screen.safeAreaInsets().top as f64
}

fn area(rect: NSRect) -> Option<RectF> {
    let width = rect.size.width as f64;
    let height = rect.size.height as f64;
    if width <= 0.0 || height <= 0.0 {
        return None;
    }
    Some(RectF {
        x: rect.origin.x as f64,
        y: rect.origin.y as f64,
        width,
        height,
    })
}

fn watch_hits(app: AppHandle, panel: Arc<dyn Panel<tauri::Wry>>) {
    let ignoring = Arc::new(AtomicBool::new(true));
    let started = thread::Builder::new()
        .name("island-hit-test".into())
        .spawn(move || loop {
            let panel = Arc::clone(&panel);
            let ignoring = Arc::clone(&ignoring);
            let scheduled = app.run_on_main_thread(move || {
                let inside = cursor_inside(panel.as_ref());
                if inside == ignoring.load(Ordering::Relaxed) {
                    panel.set_ignores_mouse_events(!inside);
                    ignoring.store(!inside, Ordering::Relaxed);
                }
            });
            if scheduled.is_err() {
                break;
            }
            thread::sleep(Duration::from_millis(32));
        });
    if started.is_err() {
        eprintln!("bitbot-desktop: i clic sulle parti trasparenti non vengono filtrati");
    }
}

fn cursor_inside(panel: &dyn Panel<tauri::Wry>) -> bool {
    let window: &NSWindow = panel.as_panel().as_super();
    let frame = window.frame();
    let point = window.mouseLocationOutsideOfEventStream();
    let width = frame.size.width as f64;
    let height = frame.size.height as f64;
    if width <= 0.0 || height <= 0.0 {
        return false;
    }
    geometry::point_in_rounded_rect(
        point.x as f64,
        height - point.y as f64,
        width,
        height,
        height / 2.0,
    )
}
