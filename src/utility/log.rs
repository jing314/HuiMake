#[cfg(debug_assertions)]
#[macro_export]
macro_rules! loge {
    ($($arg:tt)*) => {
        println!("[Error]: {}:{} {}", file!(), line!(), format!($($arg)*));
    };
} 
#[cfg(not(debug_assertions))]
#[macro_export]
macro_rules! loge {
    ($($arg:tt)*) => {};
}

#[cfg(debug_assertions)]
#[macro_export]
macro_rules! logd {
    ($($arg:tt)*) => {
        eprintln!("[Debug]: {}:{} {}", file!(), line!(), format_args!($($arg)*));
    };
}

/// Debug 日志的“空”版本：release 模式下不产生任何代码
#[cfg(not(debug_assertions))]
#[macro_export]
macro_rules! logd {
    ($($arg:tt)*) => {};
}
#[cfg(debug_assertions)]
#[macro_export]
macro_rules! logi {
    ($($arg:tt)*) => {
        println!("[Info]: {}:{} {}", file!(), line!(), format!($($arg)*));
    };
} 
#[cfg(not(debug_assertions))]
#[macro_export]
macro_rules! logi {
    ($($arg:tt)*) => {};
}

