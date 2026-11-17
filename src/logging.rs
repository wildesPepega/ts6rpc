//! Log lines go to stdout and to `ts6rpc.log` in the config dir
//! (the Windows build has no console window).

use std::fs::File;
use std::io::Write;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

static FILE: OnceLock<Mutex<File>> = OnceLock::new();

#[macro_export]
macro_rules! log {
    ($($arg:tt)*) => {
        $crate::logging::write(format_args!($($arg)*))
    };
}

pub fn path() -> PathBuf {
    crate::config::dir().join("ts6rpc.log")
}

/// Starts a fresh log file. Errors are ignored: logging to stdout still works.
pub fn init() {
    let p = path();
    if let Some(dir) = p.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    if let Ok(f) = File::create(&p) {
        let _ = FILE.set(Mutex::new(f));
    }
}

pub fn write(args: std::fmt::Arguments) {
    let line = format!("{} {}\n", chrono::Local::now().format("%H:%M:%S"), args);
    let _ = std::io::stdout().write_all(line.as_bytes());
    if let Some(f) = FILE.get()
        && let Ok(mut f) = f.lock()
    {
        let _ = f.write_all(line.as_bytes());
    }
}
