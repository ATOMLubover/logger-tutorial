#[macro_export]
macro_rules! trace {
    ($logger:expr, $($arg:tt)+) => {{
        let logger = &$logger;
        let level = $crate::Level::Trace;
        if logger.is_enabled(level) {
            logger.log_fmt(level, format_args!($($arg)+));
        }
    }};
}

#[macro_export]
macro_rules! debug {
    ($logger:expr, $($arg:tt)+) => {{
        let logger = &$logger;
        let level = $crate::Level::Debug;
        if logger.is_enabled(level) {
            logger.log_fmt(level, format_args!($($arg)+));
        }
    }};
}

#[macro_export]
macro_rules! info {
    ($logger:expr, $($arg:tt)+) => {{
        let logger = &$logger;
        let level = $crate::Level::Info;
        if logger.is_enabled(level) {
            logger.log_fmt(level, format_args!($($arg)+));
        }
    }};
}

#[macro_export]
macro_rules! warn {
    ($logger:expr, $($arg:tt)+) => {{
        let logger = &$logger;
        let level = $crate::Level::Warn;
        if logger.is_enabled(level) {
            logger.log_fmt(level, format_args!($($arg)+));
        }
    }};
}

#[macro_export]
macro_rules! error {
    ($logger:expr, $($arg:tt)+) => {{
        let logger = &$logger;
        let level = $crate::Level::Error;
        if logger.is_enabled(level) {
            logger.log_fmt(level, format_args!($($arg)+));
        }
    }};
}
