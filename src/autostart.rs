//! "Start on login": an XDG autostart entry on Linux, the HKCU Run key on Windows.

use std::io;

#[cfg(target_os = "linux")]
mod imp {
    use std::io;
    use std::path::PathBuf;

    fn entry() -> Option<PathBuf> {
        Some(dirs::config_dir()?.join("autostart").join("ts6rpc.desktop"))
    }

    pub fn is_enabled() -> bool {
        entry().is_some_and(|p| {
            std::fs::read_to_string(p).is_ok_and(|s| !s.lines().any(|l| l.trim() == "Hidden=true"))
        })
    }

    pub fn set(on: bool) -> io::Result<()> {
        let path = entry().ok_or_else(|| io::Error::other("no config directory"))?;
        if !on {
            return match std::fs::remove_file(&path) {
                Err(e) if e.kind() != io::ErrorKind::NotFound => Err(e),
                _ => Ok(()),
            };
        }
        let exe = std::env::current_exe()?;
        let exe = exe.to_string_lossy().replace('\\', "\\\\").replace('"', "\\\"");
        std::fs::create_dir_all(path.parent().unwrap())?;
        std::fs::write(
            &path,
            format!(
                "[Desktop Entry]\nType=Application\nName=ts6rpc\nComment=Discord Rich Presence for TeamSpeak 6\n\
                 Exec=\"{exe}\"\nIcon=ts6rpc\nTerminal=false\nX-GNOME-Autostart-enabled=true\n"
            ),
        )
    }
}

#[cfg(windows)]
mod imp {
    use std::io;
    use std::ptr::null_mut;

    use windows_sys::Win32::Foundation::ERROR_SUCCESS;
    use windows_sys::Win32::System::Registry::{
        HKEY_CURRENT_USER, REG_SZ, RRF_RT_REG_SZ, RegDeleteKeyValueW, RegGetValueW, RegSetKeyValueW,
    };

    const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
    const VALUE: &str = "ts6rpc";

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(Some(0)).collect()
    }

    pub fn is_enabled() -> bool {
        let (key, val) = (wide(RUN_KEY), wide(VALUE));
        let r = unsafe {
            RegGetValueW(HKEY_CURRENT_USER, key.as_ptr(), val.as_ptr(), RRF_RT_REG_SZ, null_mut(), null_mut(), null_mut())
        };
        r == ERROR_SUCCESS
    }

    pub fn set(on: bool) -> io::Result<()> {
        let (key, val) = (wide(RUN_KEY), wide(VALUE));
        let r = if on {
            let exe = std::env::current_exe()?;
            let data = wide(&format!("\"{}\"", exe.display()));
            unsafe {
                RegSetKeyValueW(
                    HKEY_CURRENT_USER,
                    key.as_ptr(),
                    val.as_ptr(),
                    REG_SZ,
                    data.as_ptr().cast(),
                    (data.len() * 2) as u32,
                )
            }
        } else {
            match unsafe { RegDeleteKeyValueW(HKEY_CURRENT_USER, key.as_ptr(), val.as_ptr()) } {
                2 => ERROR_SUCCESS, // ERROR_FILE_NOT_FOUND: already off
                r => r,
            }
        };
        if r == ERROR_SUCCESS { Ok(()) } else { Err(io::Error::from_raw_os_error(r as i32)) }
    }
}

#[cfg(not(any(target_os = "linux", windows)))]
mod imp {
    pub fn is_enabled() -> bool {
        false
    }
    pub fn set(_on: bool) -> std::io::Result<()> {
        Err(std::io::Error::other("not supported on this platform"))
    }
}

pub fn is_enabled() -> bool {
    imp::is_enabled()
}

pub fn set(on: bool) -> io::Result<()> {
    imp::set(on)
}
