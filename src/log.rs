//! 极简日志：级别由 -v 控制，全部写 stderr（stdout 留给 --print-plan 等可管道输出）。

use std::fmt;
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::Mutex;

pub const QUIET: u8 = 0;
pub const ERROR: u8 = 1;
pub const WARN: u8 = 2;
pub const INFO: u8 = 3;
pub const DEBUG: u8 = 4;
pub const TRACE: u8 = 5;

static LEVEL: AtomicU8 = AtomicU8::new(INFO);
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

/// 把所有日志写在同一把锁里，避免并行构建时多行输出互相穿插。
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

/// 一次性输出整块文本（例如编译器诊断），避免多行被并行任务穿插。
pub fn print_block(text: &str) {
    let trimmed = text.trim_end();
    if trimmed.is_empty() {
        return;
    }
    let _guard = LOCK.lock();
    eprintln!("{trimmed}");
}

/// 不带级别前缀的进度输出（同样加锁，保证整行原子）。
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
