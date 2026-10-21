//! Minimal file logger. Writes `HitManVK\hm1vk.log` next to the DLL.

use std::collections::HashSet;
use std::fs::File;
use std::io::Write;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::Instant;

struct Logger {
    file: Option<File>,
    start: Instant,
    once: HashSet<String>,
    level: u32,
}

static LOGGER: Mutex<Option<Logger>> = Mutex::new(None);

pub fn init(dir: &std::path::Path, level: u32) {
    let path: PathBuf = dir.join("hm1vk.log");
    let file = File::create(&path).ok();
    *LOGGER.lock().unwrap() = Some(Logger { file, start: Instant::now(), once: HashSet::new(), level });
}

pub fn level() -> u32 {
    LOGGER.lock().map(|l| l.as_ref().map(|l| l.level).unwrap_or(0)).unwrap_or(0)
}

pub fn write(lvl: u32, msg: std::fmt::Arguments<'_>) {
    if let Ok(mut g) = LOGGER.lock() {
        if let Some(l) = g.as_mut() {
            if lvl > l.level {
                return;
            }
            let t = l.start.elapsed().as_secs_f64();
            if let Some(f) = l.file.as_mut() {
                let _ = writeln!(f, "[{t:9.3}] {msg}");
                let _ = f.flush();
            }
        }
    }
}

/// Logs a message only the first time `key` is seen.
pub fn once(key: &str, msg: std::fmt::Arguments<'_>) {
    let first = LOGGER
        .lock()
        .map(|mut g| g.as_mut().map(|l| l.once.insert(key.to_string())).unwrap_or(false))
        .unwrap_or(false);
    if first {
        write(0, msg);
    }
}

#[macro_export]
macro_rules! info {
    ($($t:tt)*) => { $crate::log::write(0, format_args!($($t)*)) };
}
#[macro_export]
macro_rules! debug {
    ($($t:tt)*) => { $crate::log::write(1, format_args!($($t)*)) };
}
#[macro_export]
macro_rules! trace {
    ($($t:tt)*) => { $crate::log::write(2, format_args!($($t)*)) };
}
/// Logs the first call of an API (key = message format string location).
#[macro_export]
macro_rules! first_call {
    ($name:expr) => {
        $crate::log::once($name, format_args!("first call: {}", $name))
    };
    ($name:expr, $($t:tt)*) => {
        $crate::log::once($name, format_args!("first call: {} {}", $name, format_args!($($t)*)))
    };
}
#[macro_export]
macro_rules! unimpl {
    ($name:expr) => {
        $crate::log::once($name, format_args!("UNIMPLEMENTED: {}", $name))
    };
}
