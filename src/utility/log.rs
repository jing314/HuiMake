#[macro_export]
macro_rules! loge {
    ($($arg:tt)*) => {
        println!("[Error]: {}:{} {}", file!(), line!(), format!($($arg)*));
    };
} 
#[macro_export]
macro_rules! logd {
    ($($arg:tt)*) => {
        println!("[Debug]: {}:{} {}", file!(), line!(), format!($($arg)*));
    };
} 
#[macro_export]
macro_rules! logi {
    ($($arg:tt)*) => {
        println!("[Info]: {}:{} {}", file!(), line!(), format!($($arg)*));
    };
} 

