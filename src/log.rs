//! Minimal logging.
//!
//! Everything is written to stderr so that stdout stays usable for --print-plan.
//! The default level depends on how hk itself was built:
//!   * debug build   -> INFO  (per-step progress)
//!   * release build -> WARN  (warnings, errors and the final summary only)
//!
//! -v / -vv raise the level, -q lowers it.

use std::fmt;
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::Mutex;

pub const QUIET: u8 = 0;
pub const ERROR: u8 = 1;
pub const WARN: u8 = 2;
pub const INFO: u8 = 3;
pub const DEBUG: u8 = 4;
pub const TRACE: u8 = 5;

/// Default level for this build of hk.
#[cfg(debug_assertions)]
pub const DEFAULT_LEVEL: u8 = INFO;
#[cfg(not(debug_assertions))]
pub const DEFAULT_LEVEL: u8 = WARN;

static LEVEL: AtomicU8 = AtomicU8::new(DEFAULT_LEVEL);
static LOCK: Mutex<()> = Mutex::new(());

pub fn set_level(level: u8) {
    LEVEL.store(level, Ordering::Relaxed);
}

pub fn level() -> u8 {
    LEVEL.load(Ordering::Relaxed)
}

pub fn enabled(level: u8) -> bool {
    self::level() >= level
}

/// Write one log line. All logging shares a single lock so that parallel builds
/// never interleave partial lines.
pub fn log_line(level: u8, args: fmt::Arguments<'_>) {
    if !enabled(level) {
        return;
    }
    let _guard = LOCK.lock();
    let tag = match level {
        QUIET | ERROR => "error",
        WARN => "warn ",
        INFO => "info ",
        DEBUG => "debug",
        _ => "trace",
    };
    eprintln!("[{tag}] {args}");
}

/// Print a whole block at once (e.g. compiler diagnostics) under the same lock.
pub fn print_block(text: &str) {
    let trimmed = text.trim_end();
    if trimmed.is_empty() {
        return;
    }
    let _guard = LOCK.lock();
    eprintln!("{trimmed}");
}

/// Level-independent progress/result output (build summary, artifacts, size report).
pub fn print_line(text: impl AsRef<str>) {
    let _guard = LOCK.lock();
    eprintln!("{}", text.as_ref());
}

#[macro_export]
macro_rules! log_error { ($($arg:tt)*) => { $crate::log::log_line($crate::log::ERROR, format_args!($($arg)*)) } }
#[macro_export]
macro_rules! log_warn  { ($($arg:tt)*) => { $crate::log::log_line($crate::log::WARN,  format_args!($($arg)*)) } }
#[macro_export]
macro_rules! log_info  { ($($arg:tt)*) => { $crate::log::log_line($crate::log::INFO,  format_args!($($arg)*)) } }
#[macro_export]
macro_rules! log_debug { ($($arg:tt)*) => { $crate::log::log_line($crate::log::DEBUG, format_args!($($arg)*)) } }
#[macro_export]
macro_rules! log_trace { ($($arg:tt)*) => { $crate::log::log_line($crate::log::TRACE, format_args!($($arg)*)) } }
#[macro_export]
macro_rules! log_plain { ($($arg:tt)*) => { $crate::log::print_line(format!($($arg)*)) } }
