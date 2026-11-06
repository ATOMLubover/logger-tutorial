//! 文章「实现最简单 `log` 函数」的朴素实现，对应「优化时间序列化」之前的状态。
//!
//! 这一版刻意不做任何优化：
//!
//! - 不支持格式化，调用方只能传已经拼好的 `&str`；
//! - 没有 `fmt::Arguments`、没有 buffer、没有 thread-local；
//! - 每次 `log` 都重新做时区转换、时间格式化，并 `format!` 出一整行 `String`；
//! - 时间戳由调用方传入，`log` 只负责拼行和派发。
//!
//! 唯一为 benchmark 增加的是可注入的 [`Appender`]，用来替换文章里的 `_mock_print`。

use std::io;
use std::sync::Arc;

use time::format_description::StaticFormatDescription;
use time::{OffsetDateTime, UtcOffset};

const DATE_TIME_FORMAT: StaticFormatDescription = time::macros::format_description!(
    "[year]-[month]-[day] [hour]:[minute]:[second].[subsecond digits:3]"
);

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

/// 对应文章里的 `_mock_print` 位置，实际作为 sink 注入。
pub trait Appender {
    fn append(&self, buf: &str) -> io::Result<usize>;
}

type BoxAppender = Box<dyn Appender + Send + Sync>;

pub struct Logger {
    min_level: Level,
    appenders: Arc<[BoxAppender]>,
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

    /// 文章里的朴素 `log`：检查等级、格式化时间、拼整行、派发。
    ///
    /// 开了 `profile` feature 时会关闭这几个函数的內联，让采样能归因到
    /// 用户代码本身而不是只剩 `main` + libc。
    #[cfg_attr(feature = "profile", inline(never))]
    pub fn log(&self, level: Level, timestamp_millis: u64, message: &str) {
        if self.min_level > level {
            return;
        }

        let formatted_time = Self::format_time(timestamp_millis);
        let formatted_full_message = Self::format_line(&formatted_time, level, message);
        self.dispatch(&formatted_full_message);
    }

    /// 与文章一致：每次都重新取时区并序列化成 `String`。
    #[cfg_attr(feature = "profile", inline(never))]
    fn format_time(timestamp_millis: u64) -> String {
        let offset = Self::current_offset();
        let local = Self::to_local(timestamp_millis, offset);
        Self::format_datetime(local)
    }

    /// 阶段 1：取本地时区偏移。
    #[cfg_attr(feature = "profile", inline(never))]
    fn current_offset() -> UtcOffset {
        UtcOffset::current_local_offset().unwrap_or(UtcOffset::UTC)
    }

    /// 阶段 2：把毫秒时间戳转成带偏移的本地时间。
    #[cfg_attr(feature = "profile", inline(never))]
    fn to_local(timestamp_millis: u64, offset: UtcOffset) -> OffsetDateTime {
        OffsetDateTime::from_unix_timestamp_nanos(i128::from(timestamp_millis) * 1_000_000)
            .expect("timestamp is in range")
            .to_offset(offset)
    }

    /// 阶段 3：把本地时间格式化成 23 B 的时间 `String`。
    #[cfg_attr(feature = "profile", inline(never))]
    fn format_datetime(local: OffsetDateTime) -> String {
        local
            .format(DATE_TIME_FORMAT)
            .expect("date/time text fits the format")
    }

    /// 把时间、等级、原始 message 拼成最终整行 `String`。
    #[cfg_attr(feature = "profile", inline(never))]
    fn format_line(formatted_time: &str, level: Level, message: &str) -> String {
        format!("[{}] [{}] {}\n", formatted_time, level.as_str(), message)
    }

    /// 将整行交给每个 appender。
    #[cfg_attr(feature = "profile", inline(never))]
    fn dispatch(&self, formatted_full_message: &str) {
        for appender in self.appenders.iter() {
            let _ = appender.append(formatted_full_message);
        }
    }
}

/// 仅供 `bench/benches/phases.rs` 做分阶段测量，不属于公开 API。
#[doc(hidden)]
pub mod phases {
    use super::*;

    #[inline(never)]
    pub fn current_offset() -> UtcOffset {
        Logger::current_offset()
    }

    #[inline(never)]
    pub fn to_local(timestamp_millis: u64, offset: UtcOffset) -> OffsetDateTime {
        Logger::to_local(timestamp_millis, offset)
    }

    #[inline(never)]
    pub fn format_datetime(local: OffsetDateTime) -> String {
        Logger::format_datetime(local)
    }

    #[inline(never)]
    pub fn format_line(formatted_time: &str, level: Level, message: &str) -> String {
        Logger::format_line(formatted_time, level, message)
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
    fn disabled_level_does_not_reach_appender() {
        let capture = Capture::default();
        let logger = logger(Level::Info, &capture);

        logger.log(Level::Debug, 1_700_000_000_123, "debug");

        assert!(capture.text().is_empty());
    }

    #[test]
    fn every_appender_receives_the_same_line() {
        let first = Capture::default();
        let second = Capture::default();
        let logger = Logger::new(
            Level::Trace,
            vec![Box::new(first.clone()), Box::new(second.clone())],
        );

        logger.log(Level::Error, 1_700_000_000_123, "boom");

        assert_eq!(first.text(), second.text());
    }
}
