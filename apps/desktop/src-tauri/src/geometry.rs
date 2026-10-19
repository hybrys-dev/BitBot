//! Geometria dell'isola, senza dipendere dal window manager.
//! Le coordinate Cocoa hanno l'origine in basso a sinistra; altrove l'origine è in alto a sinistra.

/// Larghezza a riposo della striscia, in punti logici.
pub(crate) const ISLAND_WIDTH: f64 = 240.0;

/// Altezza a riposo della striscia, in punti logici.
pub(crate) const ISLAND_HEIGHT: f64 = 36.0;

/// Sotto questa larghezza il buco fra le aree del menu bar non è un notch usabile.
#[cfg(any(test, target_os = "macos"))]
const MIN_NOTCH_WIDTH: f64 = 80.0;

/// Sotto questa altezza l'inset superiore non è un notch.
#[cfg(any(test, target_os = "macos"))]
const MIN_NOTCH_HEIGHT: f64 = 16.0;

/// Tolleranza in pixel fisici per dire che una finestra copre lo schermo.
#[cfg(any(test, target_os = "windows"))]
pub(crate) const FULLSCREEN_TOLERANCE: i32 = 2;

#[cfg(any(test, target_os = "macos"))]
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct RectF {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

#[cfg(any(test, target_os = "linux"))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PxRect {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

#[cfg(any(test, target_os = "windows"))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PxBounds {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

#[cfg(any(test, target_os = "macos"))]
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct IslandFrame {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    /// Vero quando il rettangolo sta nel notch, non nella striscia sotto il bordo.
    pub in_notch: bool,
}

/// Lo spike non ha eventi che richiedono l'utente: a schermo intero l'isola resta ritirata.
#[cfg(any(test, target_os = "linux", target_os = "windows"))]
pub(crate) fn island_visible(foreground_fullscreen: bool) -> bool {
    !foreground_fullscreen
}

/// Origine in alto a sinistra, centrata sul bordo alto del monitor.
///
/// `top_offset` è quanto copre il pannello in alto: l'isola scende sotto.
#[cfg(any(test, not(target_os = "macos")))]
pub(crate) fn top_center_origin(
    monitor_x: f64,
    monitor_y: f64,
    monitor_width: f64,
    island_width: f64,
    top_offset: f64,
) -> (f64, f64) {
    (
        monitor_x + (monitor_width - island_width) / 2.0,
        monitor_y + top_offset,
    )
}

/// Rettangolo Cocoa dell'isola.
///
/// `left` e `right` sono le aree del menu bar ai lati del notch, in coordinate schermo.
/// `safe_top` è l'inset superiore: sui Mac col notch è maggiore di zero anche senza quelle aree.
#[cfg(any(test, target_os = "macos"))]
pub(crate) fn macos_island_frame(
    screen: RectF,
    left: Option<RectF>,
    right: Option<RectF>,
    safe_top: f64,
) -> IslandFrame {
    if let (Some(left), Some(right)) = (left, right) {
        let gap_x = left.x + left.width;
        let gap_w = right.x - gap_x;
        let bottom = left.y.min(right.y);
        let top = (left.y + left.height).max(right.y + right.height);
        let gap_h = top - bottom;
        if gap_w >= MIN_NOTCH_WIDTH && gap_h >= MIN_NOTCH_HEIGHT {
            return IslandFrame {
                x: gap_x,
                y: bottom,
                width: gap_w,
                height: gap_h,
                in_notch: true,
            };
        }
    }

    if safe_top >= MIN_NOTCH_HEIGHT {
        let height = safe_top.min(screen.height);
        let width = ISLAND_WIDTH.min(screen.width);
        return IslandFrame {
            x: screen.x + (screen.width - width) / 2.0,
            y: screen.y + screen.height - height,
            width,
            height,
            in_notch: true,
        };
    }

    let width = ISLAND_WIDTH.min(screen.width);
    IslandFrame {
        x: screen.x + (screen.width - width) / 2.0,
        y: screen.y + screen.height - ISLAND_HEIGHT.min(screen.height),
        width,
        height: ISLAND_HEIGHT.min(screen.height),
        in_notch: false,
    }
}

/// Il punto è nel rettangolo arrotondato. L'origine è in alto a sinistra.
#[cfg(any(test, target_os = "linux", target_os = "macos"))]
pub(crate) fn point_in_rounded_rect(x: f64, y: f64, width: f64, height: f64, radius: f64) -> bool {
    if width <= 0.0 || height <= 0.0 || x < 0.0 || y < 0.0 || x >= width || y >= height {
        return false;
    }
    let radius = radius.max(0.0).min(width / 2.0).min(height / 2.0);
    if radius == 0.0 {
        return true;
    }
    let (cx, cy) = if x < radius && y < radius {
        (radius, radius)
    } else if x > width - radius && y < radius {
        (width - radius, radius)
    } else if x < radius && y > height - radius {
        (radius, height - radius)
    } else if x > width - radius && y > height - radius {
        (width - radius, height - radius)
    } else {
        return true;
    };
    let dx = x - cx;
    let dy = y - cy;
    dx * dx + dy * dy <= radius * radius
}

/// Righe orizzontali che coprono la stessa forma di [`point_in_rounded_rect`].
///
/// Servono all'input region di GDK, che accetta solo rettangoli.
#[cfg(any(test, target_os = "linux"))]
pub(crate) fn rounded_rect_rows(width: i32, height: i32, radius: i32) -> Vec<PxRect> {
    if width <= 0 || height <= 0 {
        return Vec::new();
    }
    let radius = radius.max(0).min(width / 2).min(height / 2);
    let mut rows = Vec::new();
    for y in 0..height {
        let mut start = None;
        let mut end = None;
        for x in 0..width {
            if point_in_rounded_rect(
                f64::from(x) + 0.5,
                f64::from(y) + 0.5,
                f64::from(width),
                f64::from(height),
                f64::from(radius),
            ) {
                if start.is_none() {
                    start = Some(x);
                }
                end = Some(x);
            }
        }
        if let (Some(start), Some(end)) = (start, end) {
            rows.push(PxRect {
                x: start,
                y,
                width: end - start + 1,
                height: 1,
            });
        }
    }
    rows
}

/// Una finestra massimizzata copre l'area di lavoro, non il monitor intero.
#[cfg(any(test, target_os = "windows"))]
pub(crate) fn rect_covers(window: PxBounds, monitor: PxBounds, tolerance: i32) -> bool {
    window.left <= monitor.left + tolerance
        && window.top <= monitor.top + tolerance
        && window.right >= monitor.right - tolerance
        && window.bottom >= monitor.bottom - tolerance
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fullscreen_hides_the_island() {
        assert!(!island_visible(true));
        assert!(island_visible(false));
    }

    #[test]
    fn top_center_uses_the_monitor_origin() {
        assert_eq!(
            top_center_origin(100.0, 40.0, 1000.0, 240.0, 0.0),
            (480.0, 40.0)
        );
    }

    #[test]
    fn top_center_descends_below_the_panel() {
        assert_eq!(
            top_center_origin(100.0, 40.0, 1000.0, 240.0, 32.0),
            (480.0, 72.0)
        );
    }

    #[test]
    fn notch_gap_places_the_island_between_menu_bar_areas() {
        let screen = RectF {
            x: 0.0,
            y: 0.0,
            width: 1440.0,
            height: 900.0,
        };
        let left = RectF {
            x: 0.0,
            y: 876.0,
            width: 600.0,
            height: 24.0,
        };
        let right = RectF {
            x: 840.0,
            y: 876.0,
            width: 600.0,
            height: 24.0,
        };
        let frame = macos_island_frame(screen, Some(left), Some(right), 0.0);
        assert_eq!(
            frame,
            IslandFrame {
                x: 600.0,
                y: 876.0,
                width: 240.0,
                height: 24.0,
                in_notch: true,
            }
        );
    }

    #[test]
    fn screen_without_notch_hangs_from_the_top_edge() {
        let screen = RectF {
            x: 0.0,
            y: 0.0,
            width: 1440.0,
            height: 900.0,
        };
        let frame = macos_island_frame(screen, None, None, 0.0);
        assert!(!frame.in_notch);
        assert_eq!(frame.width, ISLAND_WIDTH);
        assert_eq!(frame.height, ISLAND_HEIGHT);
        assert_eq!(frame.x, (1440.0 - ISLAND_WIDTH) / 2.0);
        assert_eq!(frame.y, 900.0 - ISLAND_HEIGHT);
    }

    #[test]
    fn safe_top_inset_counts_as_notch_when_areas_are_missing() {
        let screen = RectF {
            x: 0.0,
            y: 0.0,
            width: 1512.0,
            height: 982.0,
        };
        let frame = macos_island_frame(screen, None, None, 37.0);
        assert!(frame.in_notch);
        assert_eq!(frame.height, 37.0);
        assert_eq!(frame.y, 982.0 - 37.0);
    }

    #[test]
    fn pill_corner_is_outside_and_center_is_inside() {
        assert!(!point_in_rounded_rect(0.0, 0.0, 240.0, 36.0, 18.0));
        assert!(point_in_rounded_rect(120.0, 18.0, 240.0, 36.0, 18.0));
        assert!(point_in_rounded_rect(18.0, 18.0, 240.0, 36.0, 18.0));
        assert!(!point_in_rounded_rect(-1.0, 18.0, 240.0, 36.0, 18.0));
    }

    #[test]
    fn rounded_rows_match_the_pill_predicate() {
        let width = 240;
        let height = 36;
        let radius = 18;
        let rows = rounded_rect_rows(width, height, radius);
        for y in 0..height {
            for x in 0..width {
                let inside = point_in_rounded_rect(
                    f64::from(x) + 0.5,
                    f64::from(y) + 0.5,
                    f64::from(width),
                    f64::from(height),
                    f64::from(radius),
                );
                let covered = rows.iter().any(|row| {
                    x >= row.x && x < row.x + row.width && y >= row.y && y < row.y + row.height
                });
                assert_eq!(inside, covered, "{x},{y}");
            }
        }
        assert!(!rows.is_empty());
    }

    #[test]
    fn fullscreen_covers_the_monitor_and_maximized_does_not() {
        let monitor = PxBounds {
            left: 0,
            top: 0,
            right: 1920,
            bottom: 1080,
        };
        let fullscreen = monitor;
        let maximized = PxBounds {
            left: 0,
            top: 0,
            right: 1920,
            bottom: 1040,
        };
        assert!(rect_covers(fullscreen, monitor, FULLSCREEN_TOLERANCE));
        assert!(!rect_covers(maximized, monitor, FULLSCREEN_TOLERANCE));
    }
}
