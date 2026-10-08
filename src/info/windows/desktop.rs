//! Display resolution and refresh rate of the attached adapters.

use std::mem::size_of;
use std::ptr::null;

use windows_sys::Win32::Graphics::Gdi::{
    EnumDisplayDevicesW, EnumDisplaySettingsW, DEVMODEW, DISPLAY_DEVICEW,
    DISPLAY_DEVICE_ATTACHED_TO_DESKTOP, ENUM_CURRENT_SETTINGS,
};

/// Adapters to enumerate. Enumeration stops at the first missing adapter.
const MAX_ADAPTERS: u32 = 16;

/// Current mode of every attached adapter, joined with ", ", e.g. `"2560x1440 @ 144Hz"`.
pub(super) fn resolution() -> Option<String> {
    let mut modes = Vec::new();
    for index in 0..MAX_ADAPTERS {
        let mut adapter = DISPLAY_DEVICEW {
            cb: size_of::<DISPLAY_DEVICEW>() as u32,
            ..Default::default()
        };
        // SAFETY: `adapter` is writable and its cb is set. A null device name lists adapters.
        if unsafe { EnumDisplayDevicesW(null(), index, &mut adapter, 0) } == 0 {
            break;
        }
        if adapter.StateFlags & DISPLAY_DEVICE_ATTACHED_TO_DESKTOP == 0 {
            continue;
        }
        if let Some(mode) = current_mode(&adapter.DeviceName) {
            modes.push(mode);
        }
    }
    (!modes.is_empty()).then(|| modes.join(", "))
}

fn current_mode(device: &[u16]) -> Option<String> {
    let mut mode = DEVMODEW {
        dmSize: size_of::<DEVMODEW>() as u16,
        ..Default::default()
    };
    // SAFETY: `device` is a NUL-terminated device name and `mode` is writable with dmSize set.
    if unsafe { EnumDisplaySettingsW(device.as_ptr(), ENUM_CURRENT_SETTINGS, &mut mode) } == 0 {
        return None;
    }
    Some(format_mode(
        mode.dmPelsWidth,
        mode.dmPelsHeight,
        mode.dmDisplayFrequency,
    ))
}

/// `"2560x1440 @ 144Hz"`. A rate of 0 or 1 means the hardware default and is left out.
fn format_mode(width: u32, height: u32, hz: u32) -> String {
    if hz > 1 {
        format!("{width}x{height} @ {hz}Hz")
    } else {
        format!("{width}x{height}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mode_with_and_without_refresh_rate() {
        assert_eq!(format_mode(2560, 1440, 144), "2560x1440 @ 144Hz");
        assert_eq!(format_mode(1920, 1080, 0), "1920x1080");
        assert_eq!(format_mode(1920, 1080, 1), "1920x1080");
    }

    #[test]
    fn resolution_does_not_panic() {
        let _ = resolution();
    }
}
