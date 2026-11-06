//! 文章 `### 静态分配解决时间格式化` 的实现：时间直接格式化到固定大小的栈 buffer，
//! 不再为时间生成 `String`。
//!
//! 与 cache-prefix 一样，仍然不支持格式化，整行还是用 `format!` 生成 `String`；
//! 区别只在时间部分：
//!
//! - 前缀缓存从 `String` 换成 `[u8; 19]`；
//! - `format_time` 改成写入调用方提供的 `&mut [u8]`；
//! - 毫秒部分逐字节写入，不再走 `format!`。

use std::cell::RefCell;
use std::io;
use std::sync::Arc;

use time::format_description::StaticFormatDescription;
use time::{OffsetDateTime, UtcOffset};

/// 到秒为止的前缀格式，长度固定 19 B。
const PREFIX_FORMAT: StaticFormatDescription =
    time::macros::format_description!("[year]-[month]-[day] [hour]:[minute]:[second]");

const PREFIX_LEN: usize = 19;
/// 完整时间 `YYYY-MM-DD HH:MM:SS.mmm` 固定 23 B。
const TIME_STR_LEN: usize = 23;

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

struct FormattedPrefixCache {
    second: u64,
    prefix: [u8; PREFIX_LEN],
}

thread_local! {
    static FORMATTED_PREFIX_CACHE: RefCell<FormattedPrefixCache> = const {
        RefCell::new(FormattedPrefixCache {
            second: 0,
            prefix: [0; PREFIX_LEN],
        })
    };
}

/// 把 23 B 的完整时间写入 `buffer`，不产生堆分配。
#[cfg_attr(feature = "profile", inline(never))]
pub fn format_time(buffer: &mut [u8], timestamp_millis: u64) {
    debug_assert!(buffer.len() >= TIME_STR_LEN);

    let format_prefix = |prefix_buffer: &mut [u8], timestamp_millis: u64| {
        let offset = UtcOffset::current_local_offset().unwrap_or(UtcOffset::UTC);
        let local =
            OffsetDateTime::from_unix_timestamp_nanos(i128::from(timestamp_millis) * 1_000_000)
                .expect("timestamp is in range")
                .to_offset(offset);

        let mut output: &mut [u8] = prefix_buffer;
        local
            .format_into(&mut output, PREFIX_FORMAT)
            .expect("date/time text fits the prefix buffer");
    };

    let format_full = |buffer: &mut [u8], prefix: &str, timestamp_millis: u64| {
        debug_assert!(buffer.len() >= TIME_STR_LEN);
        debug_assert!(prefix.len() == PREFIX_LEN);

        // 先复制 19 B 前缀。
        buffer[..PREFIX_LEN].copy_from_slice(prefix.as_bytes());

        // 再逐字节写入 ".mmm" 4 B。
        let millis = (timestamp_millis % 1_000) as u32;
        buffer[PREFIX_LEN..TIME_STR_LEN].copy_from_slice(&[
            b'.',
            b'0' + (millis / 100) as u8,
            b'0' + ((millis / 10) % 10) as u8,
            b'0' + (millis % 10) as u8,
        ]);
    };

    FORMATTED_PREFIX_CACHE.with(|cache| {
        let mut cache = cache.borrow_mut();
        let second = timestamp_millis / 1_000;

        if cache.second != second {
            cache.second = second;
            format_prefix(&mut cache.prefix, timestamp_millis);
        }

        format_full(
            buffer,
            std::str::from_utf8(&cache.prefix).expect("prefix buffer is valid utf-8"),
            timestamp_millis,
        );
    });
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

        let mut time_buffer = [0u8; TIME_STR_LEN];
        format_time(&mut time_buffer, timestamp_millis);
        let formatted_time =
            std::str::from_utf8(&time_buffer).expect("formatted time is valid utf-8");

        let formatted_full_message = Self::format_line(formatted_time, level, message);
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
