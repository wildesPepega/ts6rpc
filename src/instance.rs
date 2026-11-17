//! Makes sure only one ts6rpc runs per user.

/// Returns false if another instance is already running.
/// The lock is held until the process exits.
#[cfg(target_os = "linux")]
pub fn acquire() -> bool {
    use std::os::linux::net::SocketAddrExt;
    use std::os::unix::net::{SocketAddr, UnixListener};

    // Abstract socket: freed by the kernel when the process dies, nothing to clean up.
    let name = format!("ts6rpc:{}", crate::config::dir().display());
    let Ok(addr) = SocketAddr::from_abstract_name(name.as_bytes()) else { return true };
    match UnixListener::bind_addr(&addr) {
        Ok(l) => {
            std::mem::forget(l);
            true
        }
        Err(e) => e.kind() != std::io::ErrorKind::AddrInUse,
    }
}

#[cfg(windows)]
pub fn acquire() -> bool {
    use windows_sys::Win32::Foundation::{ERROR_ALREADY_EXISTS, GetLastError};
    use windows_sys::Win32::System::Threading::CreateMutexW;

    let name: Vec<u16> = "Local\\ts6rpc".encode_utf16().chain(Some(0)).collect();
    // The handle is intentionally leaked: Windows releases it on exit.
    let handle = unsafe { CreateMutexW(std::ptr::null(), 0, name.as_ptr()) };
    handle.is_null() || unsafe { GetLastError() } != ERROR_ALREADY_EXISTS
}

#[cfg(not(any(target_os = "linux", windows)))]
pub fn acquire() -> bool {
    true
}
