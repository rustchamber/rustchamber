//! Startup adjustments that must run before the window and event loop exist.

/// Corrects the initial window scale on X11.
///
/// winit derives the X11 scale factor from the `Xft.dpi` resource when it is
/// set and otherwise falls back to the monitor's physical size reported by
/// RandR. EDID millimetre sizes are frequently wrong -- a 1920x1080 panel that
/// reports 160x90mm claims roughly 305 DPI -- which yields absurd factors such
/// as 3.17x and an unusable, oversized interface. Every other X11 toolkit
/// scales from `Xft.dpi` and treats "unset" as 96 DPI, so do the same and hand
/// the result to winit through its documented override.
#[cfg(target_os = "linux")]
pub fn apply_scale_override() {
    // Wayland compositors manage fractional scaling themselves; only adjust
    // the X11 path (including an explicit X11 winit backend under XWayland).
    let wayland = std::env::var_os("WAYLAND_DISPLAY").is_some()
        && !matches!(std::env::var("WINIT_UNIX_BACKEND").as_deref(), Ok("x11"));
    // Respect explicit user overrides.
    let user_override = std::env::var_os("WINIT_X11_SCALE_FACTOR").is_some()
        || std::env::var_os("WINIT_HIDPI_FACTOR").is_some();
    let Some(scale) = scale_override(
        wayland,
        std::env::var_os("DISPLAY").is_some(),
        user_override,
        xft_dpi(),
    ) else {
        return;
    };
    // Runs before the event loop or any other thread starts.
    unsafe { std::env::set_var("WINIT_X11_SCALE_FACTOR", format!("{scale:.4}")) };
}

#[cfg(not(target_os = "linux"))]
pub fn apply_scale_override() {}

/// The scale factor to force on X11, or `None` to leave winit's value alone.
#[cfg(target_os = "linux")]
fn scale_override(
    wayland: bool,
    has_display: bool,
    user_override: bool,
    dpi: Option<f64>,
) -> Option<f64> {
    if wayland || !has_display || user_override {
        return None;
    }
    // No `Xft.dpi` means the X11 default of 96 DPI, matching GTK and Qt.
    // Never fall through to the RandR millimetre heuristic winit uses.
    Some(dpi.map_or(1.0, scale_from_dpi))
}

/// Maps an `Xft.dpi` value to a window scale factor.
#[cfg(target_os = "linux")]
fn scale_from_dpi(dpi: f64) -> f64 {
    (dpi / 96.0).clamp(1.0, 4.0)
}

/// Reads `Xft.dpi` from the X server's `RESOURCE_MANAGER` property, the same
/// source GTK, Qt and winit use for X11 scaling.
#[cfg(target_os = "linux")]
fn xft_dpi() -> Option<f64> {
    use x11rb::connection::Connection;
    use x11rb::protocol::xproto::{AtomEnum, ConnectionExt};

    let (connection, screen) = x11rb::connect(None).ok()?;
    let root = connection.setup().roots.get(screen)?.root;
    let resource_manager = connection
        .intern_atom(false, b"RESOURCE_MANAGER")
        .ok()?
        .reply()
        .ok()?
        .atom;
    let property = connection
        .get_property(false, root, resource_manager, AtomEnum::STRING, 0, u32::MAX)
        .ok()?
        .reply()
        .ok()?;
    parse_xft_dpi(&String::from_utf8_lossy(&property.value))
}

/// Extracts `Xft.dpi` from the text form of the X resource database.
///
/// Values outside a plausible 48-768 DPI range are ignored so a corrupt
/// resource falls back to the 96 DPI default instead of an absurd scale.
#[cfg(target_os = "linux")]
fn parse_xft_dpi(resources: &str) -> Option<f64> {
    resources.lines().find_map(|line| {
        line.trim()
            .strip_prefix("Xft.dpi:")
            .and_then(|value| value.trim().parse::<f64>().ok())
            .filter(|dpi| (48.0..=768.0).contains(dpi))
    })
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::*;

    #[test]
    fn parses_xft_dpi_from_resources() {
        let resources = "Xft.antialias:\t1\nXft.dpi:\t120\nXft.rgba:\trgb";
        assert_eq!(parse_xft_dpi(resources), Some(120.0));
    }

    #[test]
    fn tolerates_spacing_around_the_value() {
        assert_eq!(parse_xft_dpi("  Xft.dpi:   144  \n"), Some(144.0));
        assert_eq!(parse_xft_dpi("Xft.dpi:96"), Some(96.0));
    }

    #[test]
    fn ignores_missing_or_implausible_values() {
        assert_eq!(parse_xft_dpi(""), None);
        assert_eq!(parse_xft_dpi("Xft.antialias:\t1\n"), None);
        assert_eq!(parse_xft_dpi("Xft.dpi: dense\n"), None);
        assert_eq!(parse_xft_dpi("Xft.dpi: 0\n"), None);
        assert_eq!(parse_xft_dpi("Xft.dpi: 4096\n"), None);
    }

    #[test]
    fn scales_honest_dpi_values() {
        assert_eq!(scale_from_dpi(96.0), 1.0);
        assert_eq!(scale_from_dpi(120.0), 1.25);
        assert_eq!(scale_from_dpi(144.0), 1.5);
        assert_eq!(scale_from_dpi(192.0), 2.0);
    }

    #[test]
    fn clamps_scale_to_a_usable_range() {
        assert_eq!(scale_from_dpi(1.0), 1.0);
        assert_eq!(scale_from_dpi(10_000.0), 4.0);
    }

    #[test]
    fn defaults_to_one_without_xft_dpi() {
        // The regression from issue #3: a monitor whose EDID claims 160x90mm
        // made winit pick 3.17x. Without Xft.dpi the correct X11 scale is 1.0.
        assert_eq!(scale_override(false, true, false, None), Some(1.0));
    }

    #[test]
    fn uses_xft_dpi_when_present() {
        assert_eq!(scale_override(false, true, false, Some(192.0)), Some(2.0));
    }

    #[test]
    fn leaves_wayland_display_and_user_overrides_alone() {
        assert_eq!(scale_override(true, true, false, Some(192.0)), None);
        assert_eq!(scale_override(false, false, false, None), None);
        assert_eq!(scale_override(false, true, true, Some(192.0)), None);
    }
}
