//! System tray icon: shows the status and lets you switch the presence on/off.

#[cfg(target_os = "linux")]
pub mod linux;
#[cfg(windows)]
pub mod windows;

const ICON_SIZE: u32 = 64;
static ICON_RGBA: &[u8] = include_bytes!("../../assets/icon-64.rgba");

/// The tray icon as RGBA; greyed out while the presence is off.
pub fn icon_rgba(enabled: bool) -> Vec<u8> {
    let mut data = ICON_RGBA.to_vec();
    if !enabled {
        for px in data.as_chunks_mut::<4>().0 {
            let y = (px[0] as u32 * 30 + px[1] as u32 * 59 + px[2] as u32 * 11) / 100;
            let y = (y * 3 / 4 + 32) as u8;
            px[0] = y;
            px[1] = y;
            px[2] = y;
            px[3] = (px[3] as u32 * 3 / 4) as u8;
        }
    }
    data
}

pub fn open_config_dir() {
    let dir = crate::config::dir();
    #[cfg(windows)]
    let cmd = "explorer";
    #[cfg(target_os = "macos")]
    let cmd = "open";
    #[cfg(not(any(windows, target_os = "macos")))]
    let cmd = "xdg-open";
    if let Err(e) = std::process::Command::new(cmd).arg(&dir).spawn() {
        crate::log!("Could not open {}: {e}", dir.display());
    }
}

pub fn toggle_autostart() {
    let on = !crate::autostart::is_enabled();
    match crate::autostart::set(on) {
        Ok(()) => crate::log!("Start on login {}", if on { "enabled" } else { "disabled" }),
        Err(e) => crate::log!("Could not change start on login: {e}"),
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn icon_has_expected_size() {
        assert_eq!(super::ICON_RGBA.len(), (super::ICON_SIZE * super::ICON_SIZE * 4) as usize);
        assert_eq!(super::icon_rgba(false).len(), super::ICON_RGBA.len());
    }
}
