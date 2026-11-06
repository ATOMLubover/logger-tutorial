//! 文章《简单 logger 实现中的常见性能优化》中「实现最初版本 `log`」一节的基线实现。
//!
//! 这个 crate 刻意保留文章里最初的朴素写法，作为后续优化版本的 benchmark 对照：
//!
//! - [`Record`] 直接持有一个 `String`，每打一条日志都要分配并拼接一次；
//! - 时间戳在每次调用时都重新做时区转换和格式化；
//! - 没有 thread-local buffer，也没有时间戳缓存。
//!
//! 文章给出的 `Appender::append` 签名是 `std::fmt::Result<usize>`，但 `fmt::Result`
//! 并不带泛型参数，此处改用 `std::io::Result<usize>`，与同仓库的优化版保持一致。
//!
//! 文章在该小节只实现了 `Logger::log(&Record)`。为了让基线能端到端地 benchmark，
//! 这里补了一个最朴素的 [`Logger::log_fmt`]：用 `format!` 把整行时间、等级和消息
//! 拼成一个新的 `String`，再交给 `log` 派发给各个 appender。

use std::fmt;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use time::format_description::StaticFormatDescription;
use time::{OffsetDateTime, UtcOffset};

mod appender;
mod level;

pub use appender::{Appender, NullAppender};
pub use level::Level;

/// 一条日志事件：时间、等级、文本。
///
/// 文本是独占的 `String`，这是最初版本最主要的分配来源，也是后续用格式化 buffer
/// 去优化的对象。
pub struct Record {
    pub level: Level,
    pub timestamp_millis: u64,
    pub message: String,
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

    /// 等级是否会被输出，并且至少存在一个 appender。
    pub fn is_enabled(&self, level: Level) -> bool {
        level >= self.min_level && !self.appenders.is_empty()
    }

    /// 真正写日志的入口，即文章中「实现最初版本 `log`」的代码。
    pub fn log(&self, record: &Record) {
        if record.level < self.min_level {
            return;
        }

        for appender in self.appenders.iter() {
            let _ = appender.append(record.message.as_str());
        }
    }

    /// 面向 benchmark 的朴素入口：先构造整行文本，再走 [`Logger::log`]。
    ///
    /// 与优化版的差别在于这里每次都新建 `String`，并重新格式化时间戳。
    pub fn log_fmt(&self, level: Level, arguments: fmt::Arguments<'_>) {
        if !self.is_enabled(level) {
            return;
        }

        let timestamp_millis = now_millis();
        let record = Record {
            level,
            timestamp_millis,
            message: format_line(level, timestamp_millis, arguments),
        };

        self.log(&record);
    }
}

const DATE_TIME_FORMAT: StaticFormatDescription = time::macros::format_description!(
    "[year]-[month]-[day] [hour]:[minute]:[second].[subsecond digits:3]"
);

/// 朴素的做法：每次都重新取时区、转换时间戳，并分配一个新的 `String`。
fn format_timestamp(timestamp_millis: u64) -> String {
    let offset = UtcOffset::current_local_offset().unwrap_or(UtcOffset::UTC);
    let local = OffsetDateTime::from_unix_timestamp_nanos(i128::from(timestamp_millis) * 1_000_000)
        .expect("timestamp is in range")
        .to_offset(offset);

    local
        .format(DATE_TIME_FORMAT)
        .expect("date/time text fits the format")
}

fn format_line(level: Level, timestamp_millis: u64, arguments: fmt::Arguments<'_>) -> String {
    let timestamp = format_timestamp(timestamp_millis);
    format!("[{timestamp}] [{}] {arguments}\n", level.as_str())
}

fn now_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    #[derive(Clone, Default)]
    struct Capture(Arc<Mutex<Vec<String>>>);

    impl Capture {
        fn lines(&self) -> Vec<String> {
            self.0.lock().unwrap().clone()
        }
    }

    impl Appender for Capture {
        fn append(&self, buf: &str) -> std::io::Result<usize> {
            self.0.lock().unwrap().push(buf.to_owned());
            Ok(buf.len())
        }
    }

    fn logger(level: Level, capture: &Capture) -> Logger {
        Logger::new(level, vec![Box::new(capture.clone())])
    }

    fn assert_timestamp(timestamp: &str) {
        assert_eq!(timestamp.len(), 23, "timestamp: {timestamp:?}");
        assert_eq!(&timestamp[4..5], "-");
        assert_eq!(&timestamp[7..8], "-");
        assert_eq!(&timestamp[10..11], " ");
        assert_eq!(&timestamp[13..14], ":");
        assert_eq!(&timestamp[16..17], ":");
        assert_eq!(&timestamp[19..20], ".");

        let bytes = timestamp.as_bytes();
        for range in [0..4, 5..7, 8..10, 11..13, 14..16, 17..19, 20..23] {
            assert!(
                bytes[range].iter().all(u8::is_ascii_digit),
                "non-digit in {timestamp:?}"
            );
        }
    }

    #[test]
    fn log_writes_message_verbatim() {
        let capture = Capture::default();
        let logger = logger(Level::Info, &capture);

        logger.log(&Record {
            level: Level::Info,
            timestamp_millis: 1_700_000_000_123,
            message: "hello".to_owned(),
        });

        assert_eq!(capture.lines(), ["hello"]);
    }

    #[test]
    fn disabled_level_does_not_reach_appender() {
        let capture = Capture::default();
        let logger = logger(Level::Info, &capture);

        logger.log_fmt(Level::Debug, format_args!("debug"));
        logger.log(&Record {
            level: Level::Trace,
            timestamp_millis: 0,
            message: "trace".to_owned(),
        });

        assert!(capture.lines().is_empty());
    }

    #[test]
    fn log_fmt_writes_expected_line() {
        let capture = Capture::default();
        let logger = logger(Level::Info, &capture);

        logger.log_fmt(Level::Info, format_args!("user {} logged in", "alice"));

        let lines = capture.lines();
        assert_eq!(lines.len(), 1);
        let line = &lines[0];
        let (timestamp, rest) = line
            .split_once("] [INFO] ")
            .unwrap_or_else(|| panic!("unexpected line: {line:?}"));
        assert_eq!(rest, "user alice logged in\n");
        assert_timestamp(timestamp.strip_prefix('[').expect("leading bracket"));
    }

    #[test]
    fn every_appender_receives_the_same_line() {
        let first = Capture::default();
        let second = Capture::default();
        let logger = Logger::new(
            Level::Trace,
            vec![Box::new(first.clone()), Box::new(second.clone())],
        );

        logger.log_fmt(Level::Error, format_args!("boom"));

        assert_eq!(first.lines(), second.lines());
        assert!(first.lines()[0].ends_with(" [ERROR] boom\n"));
    }

    #[test]
    fn null_appender_reports_written_length() {
        let logger = Logger::new(Level::Info, vec![Box::new(NullAppender)]);

        logger.log_fmt(Level::Info, format_args!("hello"));
    }

    #[test]
    fn is_enabled_respects_level_and_appenders() {
        let empty = Logger::new(Level::Trace, Vec::new());
        assert!(!empty.is_enabled(Level::Error));

        let capture = Capture::default();
        let logger = logger(Level::Info, &capture);
        assert!(!logger.is_enabled(Level::Debug));
        assert!(logger.is_enabled(Level::Info));
        assert!(logger.is_enabled(Level::Error));
    }
}
