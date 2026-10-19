//! `_NET_WM_STATE_FULLSCREEN` sull'finestra attiva, via Xlib.
//! GTK è già collegato a X11; qui apriamo una connessione nostra, solo dal thread di controllo.

use std::ffi::{c_char, c_int, c_long, c_ulong, c_void, CString};

pub(crate) fn active_window_fullscreen() -> Option<bool> {
    unsafe {
        let display = XOpenDisplay(std::ptr::null());
        if display.is_null() {
            return None;
        }
        let result = query(display);
        XCloseDisplay(display);
        result
    }
}

unsafe fn query(display: *mut c_void) -> Option<bool> {
    let root = XDefaultRootWindow(display);
    let active = intern(display, "_NET_ACTIVE_WINDOW")?;
    let state = intern(display, "_NET_WM_STATE")?;
    let fullscreen = intern(display, "_NET_WM_STATE_FULLSCREEN")?;
    let active_window = read_xids(display, root, active)?.into_iter().next()?;
    if active_window == 0 {
        return Some(false);
    }
    let states = read_xids(display, active_window, state)?;
    Some(states.contains(&fullscreen))
}

unsafe fn intern(display: *mut c_void, name: &str) -> Option<c_ulong> {
    let name = CString::new(name).ok()?;
    let atom = XInternAtom(display, name.as_ptr(), 1);
    if atom == 0 {
        None
    } else {
        Some(atom)
    }
}

/// Xlib restituisce le proprietà di formato 32 come `long`, non come `u32`.
unsafe fn read_xids(
    display: *mut c_void,
    window: c_ulong,
    property: c_ulong,
) -> Option<Vec<c_ulong>> {
    let mut actual_type = 0;
    let mut actual_format = 0;
    let mut nitems = 0;
    let mut bytes_after = 0;
    let mut data: *mut u8 = std::ptr::null_mut();
    let status = XGetWindowProperty(
        display,
        window,
        property,
        0,
        1024,
        0,
        0,
        &mut actual_type,
        &mut actual_format,
        &mut nitems,
        &mut bytes_after,
        &mut data,
    );
    if status != 0 || data.is_null() || actual_format != 32 {
        if !data.is_null() {
            XFree(data.cast());
        }
        return None;
    }
    let count = nitems as usize;
    let values = std::slice::from_raw_parts(data.cast::<c_ulong>(), count).to_vec();
    XFree(data.cast());
    Some(values)
}

#[link(name = "X11")]
unsafe extern "C" {
    fn XOpenDisplay(name: *const c_char) -> *mut c_void;
    fn XCloseDisplay(display: *mut c_void) -> c_int;
    fn XDefaultRootWindow(display: *mut c_void) -> c_ulong;
    fn XInternAtom(display: *mut c_void, name: *const c_char, only_if_exists: c_int) -> c_ulong;
    fn XGetWindowProperty(
        display: *mut c_void,
        window: c_ulong,
        property: c_ulong,
        long_offset: c_long,
        long_length: c_long,
        delete: c_int,
        req_type: c_ulong,
        actual_type: *mut c_ulong,
        actual_format: *mut c_int,
        nitems: *mut c_ulong,
        bytes_after: *mut c_ulong,
        prop: *mut *mut u8,
    ) -> c_int;
    fn XFree(data: *mut c_void) -> c_int;
}
