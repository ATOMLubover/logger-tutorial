mod appender;
mod buffer;
mod level;
mod macros;
mod record;
mod time_cache;

use std::cell::RefCell;
use std::fmt;
use std::io::Write as _;
use std::sync::Arc;

use crate::buffer::AdaptiveBuffer;
use crate::record::{Metadata, Record};
use crate::time_cache::TimeCache;

pub use crate::appender::Appender;
pub use crate::level::Level;

pub struct Logger {
    level: Level,
    appenders: Arc<[Box<dyn Appender + Send + Sync>]>,
}

thread_local! {
    static LOG_BUFFER: RefCell<AdaptiveBuffer> = RefCell::new(AdaptiveBuffer::default());
    static TIME_CACHE: RefCell<TimeCache> = RefCell::new(TimeCache::new());
}

impl Logger {
    pub fn new(level: Level, appenders: Vec<Box<dyn Appender + Send + Sync>>) -> Self {
        Self {
            level,
            appenders: appenders.into(),
        }
    }

    pub fn is_enabled(&self, level: Level) -> bool {
        level >= self.level && !self.appenders.is_empty()
    }

    pub fn log_fmt(&self, level: Level, arguments: fmt::Arguments<'_>) {
        let record = Record::new(Metadata::new(level), now(), arguments);
        self.log(&record);
    }

    fn log(&self, record: &Record<'_>) {
        debug_assert!(self.is_enabled(record.metadata().level()));

        LOG_BUFFER.with(|buffer| {
            // 为了防止在 writeln! 或者 appender 中嵌套 log 而 panic
            // 此处采用 try_borrow_mut 静默丢弃内层 log
            let Ok(mut buffer) = buffer.try_borrow_mut() else {
                return;
            };

            buffer.clear();

            let _ = TIME_CACHE.with(|cache| {
                let mut cache = cache.borrow_mut();
                let (date, millis) = cache.get(record.timestamp());

                write_line(
                    &mut buffer,
                    date,
                    millis,
                    record.metadata().level(),
                    record.arguments(),
                )
            });
            for appender in self.appenders.iter() {
                let _ = appender.append(buffer.as_ref());
            }
        });
    }
}

fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

fn write_line(
    buffer: &mut AdaptiveBuffer,
    date: &[u8],
    millis: u32,
    level: Level,
    arguments: fmt::Arguments<'_>,
) -> std::io::Result<()> {
    buffer.write_all(b"[")?;
    buffer.write_all(date)?;
    buffer.write_all(b".")?;
    buffer.write_all(&millis_digits(millis))?;
    buffer.write_all(b"] [")?;
    buffer.write_all(level.as_str().as_bytes())?;
    buffer.write_all(b"] ")?;
    buffer.write_fmt(arguments)?;
    buffer.write_all(b"\n")
}

fn millis_digits(millis: u32) -> [u8; 3] {
    [
        b'0' + (millis / 100) as u8,
        b'0' + ((millis / 10) % 10) as u8,
        b'0' + (millis % 10) as u8,
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;
    use std::sync::{Arc, Mutex};

    #[derive(Clone, Default)]
    struct Capture(Arc<Mutex<Vec<u8>>>);

    impl Capture {
        fn text(&self) -> String {
            String::from_utf8(self.0.lock().unwrap().clone()).expect("log output is utf-8")
        }
    }

    impl Appender for Capture {
        fn append(&self, buf: &[u8]) -> std::io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(buf);
            Ok(buf.len())
        }
    }

    fn logger(level: Level, capture: &Capture) -> Logger {
        Logger::new(level, vec![Box::new(capture.clone())])
    }

    fn assert_timestamp(timestamp: &str) {
        let timestamp = timestamp
            .strip_prefix('[')
            .and_then(|rest| rest.strip_suffix(']'))
            .unwrap_or_else(|| panic!("timestamp is not bracketed: {timestamp:?}"));
        let (datetime, millis) = timestamp
            .split_once('.')
            .unwrap_or_else(|| panic!("missing milliseconds: {timestamp:?}"));
        assert_eq!(datetime.len(), 19, "datetime: {datetime:?}");
        assert_eq!(&datetime[4..5], "-");
        assert_eq!(&datetime[7..8], "-");
        assert_eq!(&datetime[10..11], " ");
        assert_eq!(&datetime[13..14], ":");
        assert_eq!(&datetime[16..17], ":");

        let bytes = datetime.as_bytes();
        for range in [0..4, 5..7, 8..10, 11..13, 14..16, 17..19] {
            assert!(
                bytes[range].iter().all(|byte| byte.is_ascii_digit()),
                "non-digit in {datetime:?}"
            );
        }

        assert_eq!(millis.len(), 3);
        assert!(millis.parse::<u32>().is_ok(), "milliseconds: {millis:?}");
    }

    #[test]
    fn is_enabled_respects_level_order() {
        let capture = Capture::default();
        let logger = logger(Level::Info, &capture);

        assert!(!logger.is_enabled(Level::Trace));
        assert!(!logger.is_enabled(Level::Debug));
        assert!(logger.is_enabled(Level::Info));
        assert!(logger.is_enabled(Level::Warn));
        assert!(logger.is_enabled(Level::Error));
    }

    #[test]
    fn info_macro_writes_fixed_format() {
        let capture = Capture::default();
        let logger = logger(Level::Info, &capture);

        crate::info!(logger, "user {} logged in", "alice");

        let text = capture.text();
        let timestamp = text
            .strip_suffix(" [INFO] user alice logged in\n")
            .unwrap_or_else(|| panic!("unexpected output: {text:?}"));
        assert_timestamp(timestamp);
    }

    #[test]
    fn disabled_level_does_not_evaluate_arguments() {
        let capture = Capture::default();
        let logger = logger(Level::Info, &capture);
        let calls = Cell::new(0);

        crate::debug!(logger, "{}", {
            calls.set(calls.get() + 1);
            "debug"
        });

        assert_eq!(calls.get(), 0);
        assert!(capture.text().is_empty());
    }

    #[test]
    fn all_levels_are_written() {
        let capture = Capture::default();
        let logger = logger(Level::Trace, &capture);

        crate::trace!(logger, "t");
        crate::debug!(logger, "d");
        crate::info!(logger, "i");
        crate::warn!(logger, "w");
        crate::error!(logger, "e");

        let text = capture.text();
        for expected in [
            "[TRACE] t\n",
            "[DEBUG] d\n",
            "[INFO] i\n",
            "[WARN] w\n",
            "[ERROR] e\n",
        ] {
            assert!(text.contains(expected), "missing {expected:?} in {text:?}");
        }
    }

    #[test]
    fn long_message_spills_and_next_log_starts_clean() {
        let capture = Capture::default();
        let logger = logger(Level::Info, &capture);
        let long = "x".repeat(300);

        crate::info!(logger, "{}", long);
        crate::info!(logger, "short");

        let text = capture.text();
        assert!(text.contains(&format!(" [INFO] {long}\n")));
        assert!(text.contains(" [INFO] short\n"));
    }

    #[test]
    fn log_formats_timestamp_with_millis() {
        let capture = Capture::default();
        let logger = logger(Level::Info, &capture);
        let record = Record::new(
            Metadata::new(Level::Info),
            1_700_000_000_123,
            format_args!("x"),
        );

        logger.log(&record);

        let text = capture.text();
        let timestamp = text
            .strip_suffix(" [INFO] x\n")
            .unwrap_or_else(|| panic!("unexpected output: {text:?}"));
        assert_timestamp(timestamp);
        let (_, millis) = timestamp.split_once('.').expect("millis");
        assert_eq!(millis.trim_end_matches(']'), "123");
    }
}
