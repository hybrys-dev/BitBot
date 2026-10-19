//! Windows: finestra trasparente, sempre in primo piano, che non diventa attiva.
//! I clic fuori dalla pillola passano oltre perché la regione della finestra è arrotondata.

use anyhow::Context;
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use tauri::WebviewWindow;

use crate::geometry::{self, PxBounds, FULLSCREEN_TOLERANCE};

const GWL_EXSTYLE: i32 = -20;
const WS_EX_NOACTIVATE: isize = 0x0800_0000;
const WS_EX_TOOLWINDOW: isize = 0x0000_0080;
const WS_EX_TOPMOST: isize = 0x0000_0008;
const HWND_TOPMOST: isize = -1;
const SWP_NOSIZE: u32 = 0x0001;
const SWP_NOMOVE: u32 = 0x0002;
const SWP_NOACTIVATE: u32 = 0x0010;
const SWP_SHOWWINDOW: u32 = 0x0040;
const SW_SHOWNOACTIVATE: i32 = 4;
const MONITOR_DEFAULTTONEAREST: u32 = 2;

pub(crate) fn open(app: &tauri::App) -> anyhow::Result<()> {
    eprintln!("bitbot-desktop: isola su Windows, finestra trasparente sempre in primo piano");
    let window = crate::create_window(app)?;
    crate::place_top_center(&window)?;
    if let Err(err) = prevent_activation(&window) {
        eprintln!("bitbot-desktop: stile non attivante non applicato: {err:#}");
    }
    window.show().context("visualizzazione dell'isola")?;
    if let Some(hwnd) = hwnd_of(&window) {
        show_without_activation(hwnd);
        apply_round_region(hwnd);
    }
    crate::fullscreen::watch(app.handle().clone(), false);
    Ok(())
}

pub(crate) fn set_visible(window: &WebviewWindow, visible: bool) {
    if visible {
        let _ = window.show();
        if let Some(hwnd) = hwnd_of(window) {
            show_without_activation(hwnd);
        }
    } else {
        let _ = window.hide();
    }
}

pub(crate) fn foreground_is_fullscreen(our_hwnd: isize) -> Option<bool> {
    let foreground = unsafe { GetForegroundWindow() };
    if foreground == 0 || foreground == our_hwnd {
        return Some(false);
    }
    let mut window_rect = WinRect::default();
    if unsafe { GetWindowRect(foreground, &mut window_rect) } == 0 {
        return None;
    }
    let monitor = unsafe { MonitorFromWindow(foreground, MONITOR_DEFAULTTONEAREST) };
    if monitor == 0 {
        return None;
    }
    let mut info = MonitorInfo {
        cb_size: std::mem::size_of::<MonitorInfo>() as u32,
        ..MonitorInfo::default()
    };
    if unsafe { GetMonitorInfoW(monitor, &mut info) } == 0 {
        return None;
    }
    Some(geometry::rect_covers(
        window_rect.into(),
        info.rc_monitor.into(),
        FULLSCREEN_TOLERANCE,
    ))
}

fn prevent_activation(window: &WebviewWindow) -> anyhow::Result<()> {
    let hwnd = hwnd_of(window).context("handle della finestra non disponibile")?;
    let previous = get_ex_style(hwnd);
    let style = previous | WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW | WS_EX_TOPMOST;
    set_ex_style(hwnd, style);
    let flags = SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_SHOWWINDOW;
    let ok = unsafe { SetWindowPos(hwnd, HWND_TOPMOST, 0, 0, 0, 0, flags) };
    if ok == 0 {
        anyhow::bail!("SetWindowPos non ha applicato lo stile");
    }
    Ok(())
}

fn show_without_activation(hwnd: isize) {
    unsafe {
        ShowWindow(hwnd, SW_SHOWNOACTIVATE);
    }
}

fn apply_round_region(hwnd: isize) {
    let mut rect = WinRect::default();
    if unsafe { GetWindowRect(hwnd, &mut rect) } == 0 {
        return;
    }
    let width = rect.right - rect.left;
    let height = rect.bottom - rect.top;
    if width <= 0 || height <= 0 {
        return;
    }
    let region = unsafe { CreateRoundRectRgn(0, 0, width, height, height, height) };
    if region == 0 {
        return;
    }
    if unsafe { SetWindowRgn(hwnd, region, 1) } == 0 {
        unsafe {
            DeleteObject(region);
        }
    }
}

pub(super) fn hwnd_of(window: &WebviewWindow) -> Option<isize> {
    let handle = window.window_handle().ok()?;
    match handle.as_raw() {
        RawWindowHandle::Win32(win32) => Some(win32.hwnd.get()),
        _ => None,
    }
}

#[cfg(target_pointer_width = "64")]
fn get_ex_style(hwnd: isize) -> isize {
    unsafe { GetWindowLongPtrW(hwnd, GWL_EXSTYLE) }
}

#[cfg(target_pointer_width = "64")]
fn set_ex_style(hwnd: isize, style: isize) {
    unsafe {
        SetWindowLongPtrW(hwnd, GWL_EXSTYLE, style);
    }
}

#[cfg(target_pointer_width = "32")]
fn get_ex_style(hwnd: isize) -> isize {
    unsafe { isize::from(GetWindowLongW(hwnd, GWL_EXSTYLE)) }
}

#[cfg(target_pointer_width = "32")]
fn set_ex_style(hwnd: isize, style: isize) {
    unsafe {
        SetWindowLongW(hwnd, GWL_EXSTYLE, style as i32);
    }
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct WinRect {
    left: i32,
    top: i32,
    right: i32,
    bottom: i32,
}

impl From<WinRect> for PxBounds {
    fn from(rect: WinRect) -> Self {
        Self {
            left: rect.left,
            top: rect.top,
            right: rect.right,
            bottom: rect.bottom,
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct MonitorInfo {
    cb_size: u32,
    rc_monitor: WinRect,
    rc_work: WinRect,
    dw_flags: u32,
}

#[link(name = "user32")]
unsafe extern "system" {
    fn GetForegroundWindow() -> isize;
    fn GetWindowRect(hwnd: isize, rect: *mut WinRect) -> i32;
    fn MonitorFromWindow(hwnd: isize, flags: u32) -> isize;
    fn GetMonitorInfoW(monitor: isize, info: *mut MonitorInfo) -> i32;
    fn SetWindowPos(
        hwnd: isize,
        insert_after: isize,
        x: i32,
        y: i32,
        cx: i32,
        cy: i32,
        flags: u32,
    ) -> i32;
    fn ShowWindow(hwnd: isize, cmd: i32) -> i32;
    fn SetWindowRgn(hwnd: isize, region: isize, redraw: i32) -> i32;
}

#[cfg(target_pointer_width = "64")]
#[link(name = "user32")]
unsafe extern "system" {
    fn GetWindowLongPtrW(hwnd: isize, index: i32) -> isize;
    fn SetWindowLongPtrW(hwnd: isize, index: i32, value: isize) -> isize;
}

#[cfg(target_pointer_width = "32")]
#[link(name = "user32")]
unsafe extern "system" {
    fn GetWindowLongW(hwnd: isize, index: i32) -> i32;
    fn SetWindowLongW(hwnd: isize, index: i32, value: i32) -> i32;
}

#[link(name = "gdi32")]
unsafe extern "system" {
    fn CreateRoundRectRgn(
        left: i32,
        top: i32,
        right: i32,
        bottom: i32,
        width: i32,
        height: i32,
    ) -> isize;
    fn DeleteObject(object: isize) -> i32;
}
