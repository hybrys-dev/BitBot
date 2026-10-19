//! Linux: layer-shell sul bordo alto, dock su GNOME, sempre in primo piano su X11.
//! Le parti fuori dalla pillola non ricevono clic: la maschera di input è la forma arrotondata.

use std::cell::RefCell;

use anyhow::Context;
use gtk::prelude::{ContainerExt, GtkWindowExt, WidgetExt};
use gtk_layer_shell::{Edge, KeyboardMode, Layer, LayerShell};
use tauri::WebviewWindow;

use crate::geometry::{self, rounded_rect_rows};
use crate::session::{self, LinuxPresentation};

thread_local! {
    static LAYER_SHELL: RefCell<Option<gtk::ApplicationWindow>> = const { RefCell::new(None) };
}

pub(crate) fn prepare_backend() {
    if std::env::var_os("GDK_BACKEND").is_some() {
        return;
    }
    if session::linux_presentation(&session::SessionEnv::from_process())
        == LinuxPresentation::GnomeDock
    {
        // Va deciso prima che GTK scelga il backend: dopo, XWayland non si può più attivare.
        std::env::set_var("GDK_BACKEND", "x11");
    }
}

pub(crate) fn open(app: &tauri::App) -> anyhow::Result<()> {
    let presentation = session::linux_presentation(&session::SessionEnv::from_process());
    let window = crate::create_window(app)?;
    let layer_shell = match presentation {
        LinuxPresentation::LayerShell if gtk_layer_shell::is_supported() => {
            eprintln!("bitbot-desktop: isola su Linux, overlay layer-shell ancorato in alto");
            attach_layer_shell(&window)?;
            true
        }
        LinuxPresentation::LayerShell => {
            eprintln!(
                "bitbot-desktop: layer-shell non disponibile, ripiego sulla finestra sempre in primo piano"
            );
            attach_above(&window)?;
            false
        }
        LinuxPresentation::GnomeDock => {
            eprintln!("bitbot-desktop: isola su GNOME, finestra dock via XWayland");
            attach_dock(&window)?;
            false
        }
        LinuxPresentation::X11Above => {
            eprintln!("bitbot-desktop: isola su X11, finestra sempre in primo piano");
            attach_above(&window)?;
            false
        }
    };
    crate::fullscreen::watch(app.handle().clone(), layer_shell);
    Ok(())
}

pub(crate) fn set_visible(window: &WebviewWindow, visible: bool, layer_shell: bool) {
    if layer_shell {
        LAYER_SHELL.with(|slot| {
            if let Some(shell) = slot.borrow().as_ref() {
                if visible {
                    shell.show_all();
                } else {
                    shell.hide();
                }
            }
        });
        return;
    }
    if visible {
        let _ = window.show();
    } else {
        let _ = window.hide();
    }
}

fn attach_layer_shell(window: &WebviewWindow) -> anyhow::Result<()> {
    let original = window
        .gtk_window()
        .context("finestra GTK dell'isola non disponibile")?;
    let application = original
        .application()
        .context("applicazione GTK non disponibile")?;
    let vbox = window
        .default_vbox()
        .context("contenuto della finestra non disponibile")?;
    // init_layer_shell va chiamato prima che la finestra sia realizzata.
    // Quella di Tauri lo è già: il contenuto passa su una finestra nuova.
    let shell = gtk::ApplicationWindow::builder()
        .application(&application)
        .decorated(false)
        .accept_focus(false)
        .focus_on_map(false)
        .default_width(geometry::ISLAND_WIDTH as i32)
        .default_height(geometry::ISLAND_HEIGHT as i32)
        .build();
    shell.set_title("BitBot");
    shell.set_app_paintable(true);
    if let Some(screen) = WidgetExt::screen(&shell) {
        if let Some(visual) = screen.rgba_visual() {
            shell.set_visual(Some(&visual));
        }
    }
    original.remove(&vbox);
    shell.add(&vbox);
    shell.set_size_request(
        geometry::ISLAND_WIDTH as i32,
        geometry::ISLAND_HEIGHT as i32,
    );
    shell.init_layer_shell();
    shell.set_layer(Layer::Overlay);
    shell.set_namespace("bitbot-island");
    shell.set_anchor(Edge::Top, true);
    shell.set_exclusive_zone(0);
    shell.set_keyboard_mode(KeyboardMode::None);
    shell.set_skip_taskbar_hint(true);
    shell.set_skip_pager_hint(true);
    install_input_shape(&shell);
    shell.show_all();
    apply_input_shape(&shell);
    let _ = window.hide();
    LAYER_SHELL.with(|slot| {
        *slot.borrow_mut() = Some(shell);
    });
    Ok(())
}

fn attach_dock(window: &WebviewWindow) -> anyhow::Result<()> {
    let gtk_window = window
        .gtk_window()
        .context("finestra GTK dell'isola non disponibile")?;
    gtk_window.set_type_hint(gtk::gdk::WindowTypeHint::Dock);
    configure_floating(&gtk_window);
    gtk_window.stick();
    show_placed(window, &gtk_window)
}

fn attach_above(window: &WebviewWindow) -> anyhow::Result<()> {
    let gtk_window = window
        .gtk_window()
        .context("finestra GTK dell'isola non disponibile")?;
    configure_floating(&gtk_window);
    show_placed(window, &gtk_window)
}

fn configure_floating(gtk_window: &gtk::ApplicationWindow) {
    gtk_window.set_decorated(false);
    gtk_window.set_accept_focus(false);
    gtk_window.set_focus_on_map(false);
    gtk_window.set_keep_above(true);
    gtk_window.set_skip_taskbar_hint(true);
    gtk_window.set_skip_pager_hint(true);
    install_input_shape(gtk_window);
}

fn show_placed(window: &WebviewWindow, gtk_window: &gtk::ApplicationWindow) -> anyhow::Result<()> {
    let width = geometry::ISLAND_WIDTH as i32;
    let height = geometry::ISLAND_HEIGHT as i32;
    // Il webview parte da 200×200: senza un tetto la striscia resta alta.
    gtk_window.set_default_size(width, height);
    gtk_window.set_size_request(width, height);
    gtk_window.resize(width, height);
    crate::place_top_center(window)?;
    window.show().context("visualizzazione dell'isola")?;
    gtk_window.resize(width, height);
    crate::place_top_center(window)?;
    apply_input_shape(gtk_window);
    Ok(())
}

fn install_input_shape(gtk_window: &gtk::ApplicationWindow) {
    gtk_window.connect_size_allocate(|widget, _allocation| {
        apply_input_shape(widget);
    });
}

fn apply_input_shape(widget: &impl WidgetExt) {
    let Some(gdk_window) = widget.window() else {
        return;
    };
    let width = gdk_window.width();
    let height = gdk_window.height();
    let Some(region) = pill_region(width, height) else {
        return;
    };
    gdk_window.input_shape_combine_region(&region, 0, 0);
}

fn pill_region(width: i32, height: i32) -> Option<cairo::Region> {
    if width <= 0 || height <= 0 {
        return None;
    }
    let region = cairo::Region::create();
    for row in rounded_rect_rows(width, height, height / 2) {
        let rectangle = cairo::RectangleInt::new(row.x, row.y, row.width, row.height);
        region.union_rectangle(&rectangle).ok()?;
    }
    Some(region)
}
