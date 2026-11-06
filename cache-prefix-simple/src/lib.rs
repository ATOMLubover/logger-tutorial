//! 「格式化前缀缓存」版本，对应文章 `### 格式化前缀缓存` 一节。
//!
//! 与 baseline-simple 相同：不支持格式化、没有 buffer、没有 `fmt::Arguments`，
//! 只按秒缓存 `YYYY-MM-DD HH:MM:SS` 前缀，命中时再拼毫秒。
//!
//! 实现保持文章里的结构：`format_time` 内部声明 thread-local、`calculate_prefix`
//! 闭包、命中/未命中分支和最后的 `format!`；唯一拆出来的是 `format_prefix`，
//! 方便在 profile 里单独观察冷路径。

use std::cell::RefCell;
use std::io;
use std::sync::Arc;

use time::format_description::StaticFormatDescription;
use time::{OffsetDateTime, UtcOffset};

/// 到秒为止的前缀格式，长度固定 19 B。
const PREFIX_FORMAT: StaticFormatDescription =
    time::macros::format_description!("[year]-[month]-[day] [hour]:[minute]:[second]");

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[cfg_attr(debug_assertions, derive(Debug))]
#[repr(u8)]
pub enum Level {
    Trace,
    Debug,
    Info,
    Warn,
    Error,
}

impl Level {
    pub const fn as_str(self) -> &'static str {
        match self {
            Level::Trace => "TRACE",
            Level::Debug => "DEBUG",
            Level::Info => "INFO",
            Level::Warn => "WARN",
            Level::Error => "ERROR",
        }
    }
}

pub trait Appender {
    fn append(&self, buf: &str) -> io::Result<usize>;
}

type BoxAppender = Box<dyn Appender + Send + Sync>;

pub struct Logger {
    min_level: Level,
    appenders: Arc<[BoxAppender]>,
}

#[derive(Default)]
pub struct FormattedTimeCache {
    pub second: u64,
    pub prefix: String,
}

/// 与文章一致：按秒缓存前缀，命中时只拼毫秒。
#[cfg_attr(feature = "profile", inline(never))]
pub fn format_time(timestamp_millis: u64) -> String {
    thread_local! {
        static FORMATTED_TIME_CACHE: RefCell<FormattedTimeCache> =
            RefCell::new(FormattedTimeCache::default());
    }

    let calculate_prefix = || format_prefix(timestamp_millis);

    FORMATTED_TIME_CACHE.with(|cache| {
        let mut cache = cache.borrow_mut();

        // 如果当前 second 已缓存，则复用对应的 prefix；否则重新计算并替换缓存。
        let prefix = if timestamp_millis / 1_000 == cache.second {
            &cache.prefix
        } else {
            cache.second = timestamp_millis / 1_000;
            cache.prefix = calculate_prefix();
            &cache.prefix
        };

        format!("{}.{:03}", prefix, timestamp_millis % 1_000)
    })
}

/// 冷路径：取时区、转本地时间、格式化到秒。
#[cfg_attr(feature = "profile", inline(never))]
fn format_prefix(timestamp_millis: u64) -> String {
    let offset = UtcOffset::current_local_offset().unwrap_or(UtcOffset::UTC);
    let local = OffsetDateTime::from_unix_timestamp_nanos(i128::from(timestamp_millis) * 1_000_000)
        .expect("timestamp is in range")
        .to_offset(offset);

    local
        .format(PREFIX_FORMAT)
        .expect("date/time text fits the format")
}

impl Logger {
    pub fn new(min_level: Level, appenders: Vec<BoxAppender>) -> Self {
        Self {
            min_level,
            appenders: appenders.into(),
        }
    }

    pub fn min_level(&self) -> Level {
        self.min_level
    }

    #[cfg_attr(feature = "profile", inline(never))]
    pub fn log(&self, level: Level, timestamp_millis: u64, message: &str) {
        if self.min_level > level {
            return;
        }

        let formatted_time = format_time(timestamp_millis);
        let formatted_full_message = Self::format_line(&formatted_time, level, message);
        self.dispatch(&formatted_full_message);
    }

    #[cfg_attr(feature = "profile", inline(never))]
    fn format_line(formatted_time: &str, level: Level, message: &str) -> String {
        format!("[{}] [{}] {}\n", formatted_time, level.as_str(), message)
    }

    #[cfg_attr(feature = "profile", inline(never))]
    fn dispatch(&self, formatted_full_message: &str) {
        for appender in self.appenders.iter() {
            let _ = appender.append(formatted_full_message);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    const MESSAGE: &str = "user=alice action=login latency_ms=0123 status=ok session=550e8400-e29b-41d4-a716-446655440000";

    #[derive(Clone, Default)]
    struct Capture(Arc<Mutex<Vec<u8>>>);

    impl Capture {
        fn text(&self) -> String {
            String::from_utf8(self.0.lock().unwrap().clone()).expect("log output is utf-8")
        }
    }

    impl Appender for Capture {
        fn append(&self, buf: &str) -> io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(buf.as_bytes());
            Ok(buf.len())
        }
    }

    fn logger(level: Level, capture: &Capture) -> Logger {
        Logger::new(level, vec![Box::new(capture.clone())])
    }

    #[test]
    fn standard_line_is_128_bytes() {
        let capture = Capture::default();
        let logger = logger(Level::Info, &capture);

        logger.log(Level::Info, 1_700_000_000_123, MESSAGE);

        let line = capture.text();
        assert_eq!(line.len(), 128, "unexpected line: {line:?}");
        assert!(line.starts_with("[20"), "unexpected line: {line:?}");
        assert!(
            line.ends_with(&format!(" [INFO] {MESSAGE}\n")),
            "unexpected line: {line:?}"
        );
    }

    #[test]
    fn same_second_reuses_prefix_and_next_second_refreshes() {
        let capture = Capture::default();
        let logger = logger(Level::Info, &capture);

        logger.log(Level::Info, 1_700_000_000_123, "a");
        logger.log(Level::Info, 1_700_000_000_999, "b");
        logger.log(Level::Info, 1_700_000_001_000, "c");

        let text = capture.text();
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines.len(), 3);

        let (first_prefix, first_millis) = lines[0].split_once('.').expect("millis");
        let (second_prefix, second_millis) = lines[1].split_once('.').expect("millis");
        let (third_prefix, third_millis) = lines[2].split_once('.').expect("millis");

        assert_eq!(first_prefix, second_prefix);
        assert_eq!(first_millis, "123] [INFO] a");
        assert_eq!(second_millis, "999] [INFO] b");
        assert_ne!(second_prefix, third_prefix);
        assert_eq!(third_millis, "000] [INFO] c");
    }

    #[test]
    fn disabled_level_does_not_reach_appender() {
        let capture = Capture::default();
        let logger = logger(Level::Info, &capture);

        logger.log(Level::Debug, 1_700_000_000_123, "debug");

        assert!(capture.text().is_empty());
    }
}
