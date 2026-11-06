//! 文章 `### 短日志内联分配` 的实现：短日志写进 128 B 栈缓冲，超长才 fallback 到堆。
//!
//! 在 static-alloc-prefix-simple 的基础上：
//!
//! - 时间仍然写进 23 B 栈 buffer，前缀缓存为 `[u8; 19]`；
//! - 整行不再用 `format!` 生成 `String`，而是写进 thread-local [`AdaptiveBuffer`]；
//! - `AdaptiveBuffer` 默认是 [`FixedBuffer`]（128 B），写不下时升级为 [`DynamicBuffer`]；
//! - 每次 `log` 先 `clear`，短日志回到固定缓冲，长日志才分配堆。

use std::cell::RefCell;
use std::fmt::{self, Write as _};
use std::io;
use std::sync::Arc;

use time::format_description::StaticFormatDescription;
use time::{OffsetDateTime, UtcOffset};

/// 到秒为止的前缀格式，长度固定 19 B。
const PREFIX_FORMAT: StaticFormatDescription =
    time::macros::format_description!("[year]-[month]-[day] [hour]:[minute]:[second]");

const PREFIX_LEN: usize = 19;
/// 完整时间 `YYYY-MM-DD HH:MM:SS.mmm` 固定 23 B。
pub const TIME_STR_LEN: usize = 23;

/// 固定缓冲阈值：128 B。
const BUFFER_THRESHOLD: usize = 1 << 7;

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

// ---------------------------------------------------------------------------
// 时间格式化
// ---------------------------------------------------------------------------

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

/// 把 23 B 的完整时间写入 `buffer`。
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

        buffer[..PREFIX_LEN].copy_from_slice(prefix.as_bytes());

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

// ---------------------------------------------------------------------------
// AdaptiveBuffer
// ---------------------------------------------------------------------------

/// 读取接口与写入接口，写不下时自动从栈缓冲切换为堆缓冲。
pub struct AdaptiveBuffer {
    data: Variant,
}

enum Variant {
    Fixed(FixedBuffer),
    Dynamic(DynamicBuffer),
}

/// 固定容量缓冲区，阈值 128 B。
pub struct FixedBuffer {
    data: [u8; BUFFER_THRESHOLD],
    len: usize,
}

impl Default for FixedBuffer {
    fn default() -> Self {
        Self {
            data: [0; BUFFER_THRESHOLD],
            len: 0,
        }
    }
}

impl FixedBuffer {
    pub const fn capacity(&self) -> usize {
        BUFFER_THRESHOLD
    }

    pub fn len(&self) -> usize {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    pub fn clear(&mut self) {
        self.len = 0;
    }
}

impl AsRef<str> for FixedBuffer {
    fn as_ref(&self) -> &str {
        std::str::from_utf8(&self.data[..self.len]).expect("data is valid str")
    }
}

impl fmt::Write for FixedBuffer {
    fn write_str(&mut self, buf: &str) -> fmt::Result {
        // 注意，这里采用减法是为了防止加法溢出。
        if buf.len() > BUFFER_THRESHOLD - self.len {
            return Err(fmt::Error);
        }

        let buf = buf.as_bytes();
        self.data[self.len..self.len + buf.len()].copy_from_slice(buf);
        self.len += buf.len();

        Ok(())
    }
}

/// 动态容量缓冲区。
pub struct DynamicBuffer {
    data: String,
}

impl DynamicBuffer {
    pub fn with_capacity(cap: usize) -> Self {
        Self {
            data: String::with_capacity(cap),
        }
    }

    pub fn len(&self) -> usize {
        self.data.len()
    }

    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }
}

impl AsRef<str> for DynamicBuffer {
    fn as_ref(&self) -> &str {
        &self.data
    }
}

impl fmt::Write for DynamicBuffer {
    fn write_str(&mut self, buf: &str) -> fmt::Result {
        self.data.push_str(buf);
        Ok(())
    }
}

impl AdaptiveBuffer {
    pub fn clear(&mut self) {
        if let Variant::Fixed(buf) = &mut self.data {
            buf.clear();
            return;
        }

        self.data = Variant::Fixed(FixedBuffer::default());
    }
}

impl Default for AdaptiveBuffer {
    fn default() -> Self {
        Self {
            data: Variant::Fixed(FixedBuffer::default()),
        }
    }
}

impl AsRef<str> for AdaptiveBuffer {
    fn as_ref(&self) -> &str {
        match &self.data {
            Variant::Fixed(buf) => buf.as_ref(),
            Variant::Dynamic(buf) => buf.as_ref(),
        }
    }
}

impl fmt::Write for AdaptiveBuffer {
    fn write_str(&mut self, buf: &str) -> fmt::Result {
        match &mut self.data {
            Variant::Fixed(fixed) => {
                if fixed.write_str(buf).is_ok() {
                    return Ok(());
                }

                // 固定缓冲放不下：带着已有内容升级为堆缓冲。
                let mut dynamic = DynamicBuffer::with_capacity(fixed.len() + buf.len());
                dynamic.write_str(fixed.as_ref())?;
                dynamic.write_str(buf)?;
                self.data = Variant::Dynamic(dynamic);
                Ok(())
            }
            Variant::Dynamic(dynamic) => dynamic.write_str(buf),
        }
    }
}

// ---------------------------------------------------------------------------
// Logger
// ---------------------------------------------------------------------------

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

        thread_local! {
            static ADAPTIVE_BUFFER: RefCell<AdaptiveBuffer> =
                RefCell::new(AdaptiveBuffer::default());
        }

        ADAPTIVE_BUFFER.with(|buffer| {
            let mut buffer = buffer.borrow_mut();
            buffer.clear();

            Self::format_line(&mut buffer, &time_buffer, level, message);

            self.dispatch(buffer.as_ref());
        });
    }

    #[cfg_attr(feature = "profile", inline(never))]
    pub fn format_line(
        buffer: &mut AdaptiveBuffer,
        time_buffer: &[u8; TIME_STR_LEN],
        level: Level,
        message: &str,
    ) {
        let formatted_time =
            std::str::from_utf8(time_buffer).expect("formatted time is valid utf-8");

        writeln!(
            buffer,
            "[{}] [{}] {}",
            formatted_time,
            level.as_str(),
            message
        )
        .expect("adaptive buffer write cannot fail");
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
    fn long_message_spills_and_next_line_is_short_again() {
        let capture = Capture::default();
        let logger = logger(Level::Info, &capture);
        let long = "x".repeat(300);

        logger.log(Level::Info, 1_700_000_000_123, &long);
        logger.log(Level::Info, 1_700_000_000_124, "short");

        let text = capture.text();
        assert!(text.contains(&format!(" [INFO] {long}\n")));
        assert!(text.contains(" [INFO] short\n"));

        // 下一条短日志应回到 Fixed 分支。
        assert!(text.ends_with(" [INFO] short\n"));
    }

    #[test]
    fn disabled_level_does_not_reach_appender() {
        let capture = Capture::default();
        let logger = logger(Level::Info, &capture);

        logger.log(Level::Debug, 1_700_000_000_123, "debug");

        assert!(capture.text().is_empty());
    }
}
