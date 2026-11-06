//! `logger-baseline`（朴素版）、`logger-demo`（优化版）与 `spdlog-rs` 共用的 benchmark 夹具。
//!
//! 三者使用同一条 128 B 标准负载，并统一用丢弃 sink，隔离 appender / IO，
//! 只测日志前端（格式化、分配、等级判断、派发循环）的开销。

use std::hint::black_box;
use std::io;

use adaptive_buffer_simple::Appender as AdaptiveBufferAppender;
use logger_baseline::Appender as BaselineAppender;
use logger_baseline_simple::Appender as SimpleAppender;
use logger_cache_prefix::Appender as CachePrefixAppender;
use logger_demo::Appender as DemoAppender;
use spdlog::formatter::FormatterContext;
use spdlog::sink::{GetSinkProp, Sink as SpdlogSink, SinkProp};
use spdlog::{Record as SpdlogRecord, StringBuf};
use static_alloc_prefix_simple::Appender as StaticAllocAppender;

/// 标准负载整行字节数（含末尾换行）。
pub const LINE_BYTES: u64 = 128;

/// 标准事件：4 个业务字段 + 定长 session，`latency_ms` 固定 4 位。
///
/// 整行形如（时间戳固定 23 B，等级 `INFO`）：
///
/// ```text
/// [2026-11-03 10:58:11.123] [INFO] user=alice action=login latency_ms=0123 status=ok session=550e8400-e29b-41d4-a716-446655440000
/// ```
pub mod sink {
    use super::*;

    /// 丢弃输出，只读取缓冲区内容与长度，用来隔离纯前端开销。
    #[derive(Debug, Default, Clone, Copy)]
    pub struct BaselineDiscard;

    impl BaselineAppender for BaselineDiscard {
        fn append(&self, buf: &str) -> io::Result<usize> {
            black_box(buf.as_bytes().as_ptr());
            black_box(buf.len());
            Ok(buf.len())
        }
    }

    /// 同 [`BaselineDiscard`]，适配优化版的字节缓冲区接口。
    #[derive(Debug, Default, Clone, Copy)]
    pub struct DemoDiscard;

    impl DemoAppender for DemoDiscard {
        fn append(&self, buf: &[u8]) -> io::Result<usize> {
            black_box(buf.as_ptr());
            black_box(buf.len());
            Ok(buf.len())
        }
    }

    /// 丢弃输出，适配 baseline-simple 的字节缓冲区接口。
    #[derive(Debug, Default, Clone, Copy)]
    pub struct SimpleDiscard;

    impl SimpleAppender for SimpleDiscard {
        fn append(&self, buf: &str) -> io::Result<usize> {
            black_box(buf.as_bytes().as_ptr());
            black_box(buf.len());
            Ok(buf.len())
        }
    }

    /// 丢弃输出，适配 cache-prefix 的字符串接口。
    #[derive(Debug, Default, Clone, Copy)]
    pub struct CachePrefixDiscard;

    impl CachePrefixAppender for CachePrefixDiscard {
        fn append(&self, buf: &str) -> io::Result<usize> {
            black_box(buf.as_bytes().as_ptr());
            black_box(buf.len());
            Ok(buf.len())
        }
    }

    /// 丢弃输出，适配 static-alloc-prefix-simple 的字符串接口。
    #[derive(Debug, Default, Clone, Copy)]
    pub struct StaticAllocDiscard;

    impl StaticAllocAppender for StaticAllocDiscard {
        fn append(&self, buf: &str) -> io::Result<usize> {
            black_box(buf.as_bytes().as_ptr());
            black_box(buf.len());
            Ok(buf.len())
        }
    }

    /// 丢弃输出，适配 adaptive-buffer-simple 的字符串接口。
    #[derive(Debug, Default, Clone, Copy)]
    pub struct AdaptiveBufferDiscard;

    impl AdaptiveBufferAppender for AdaptiveBufferDiscard {
        fn append(&self, buf: &str) -> io::Result<usize> {
            black_box(buf.as_bytes().as_ptr());
            black_box(buf.len());
            Ok(buf.len())
        }
    }

    /// spdlog-rs 的丢弃 sink：用其默认 `FullFormatter` 格式化到 `StringBuf`，然后丢弃。
    ///
    /// 对应 spdlog-rs 自带的 sink 路径（而不是绕过 formatter），这样测到的是
    /// 记录构造 + pattern 格式化 + 时间格式化 + 派发。
    pub struct SpdlogDiscard {
        prop: SinkProp,
    }

    impl SpdlogDiscard {
        pub fn new() -> Self {
            Self {
                prop: SinkProp::default(),
            }
        }
    }

    impl Default for SpdlogDiscard {
        fn default() -> Self {
            Self::new()
        }
    }

    impl GetSinkProp for SpdlogDiscard {
        fn prop(&self) -> &SinkProp {
            &self.prop
        }
    }

    impl SpdlogSink for SpdlogDiscard {
        fn log(&self, record: &SpdlogRecord) -> spdlog::Result<()> {
            let mut buffer = StringBuf::new();
            let mut context = FormatterContext::new();
            self.prop
                .formatter()
                .format(record, &mut buffer, &mut context)?;

            black_box(buffer.as_str());
            black_box(buffer.len());
            Ok(())
        }

        fn flush(&self) -> spdlog::Result<()> {
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use logger_baseline::{Level as BaselineLevel, Logger as BaselineLogger};
    use logger_demo::{Level as DemoLevel, Logger as DemoLogger};
    use spdlog::{LevelFilter, Logger as SpdlogLogger};

    use super::*;

    #[derive(Clone, Default)]
    struct BaselineCapture(Arc<Mutex<Vec<u8>>>);

    impl BaselineAppender for BaselineCapture {
        fn append(&self, buf: &str) -> io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(buf.as_bytes());
            Ok(buf.len())
        }
    }

    #[derive(Clone, Default)]
    struct DemoCapture(Arc<Mutex<Vec<u8>>>);

    impl DemoAppender for DemoCapture {
        fn append(&self, buf: &[u8]) -> io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(buf);
            Ok(buf.len())
        }
    }

    fn baseline_line() -> String {
        let capture = BaselineCapture::default();
        let logger = BaselineLogger::new(BaselineLevel::Info, vec![Box::new(capture.clone())]);

        logger.log_fmt(
            BaselineLevel::Info,
            format_args!(
                "user=alice action=login latency_ms={:04} status=ok session=550e8400-e29b-41d4-a716-446655440000",
                123u32
            ),
        );

        let bytes = capture.0.lock().unwrap().clone();
        String::from_utf8(bytes).expect("log line is utf-8")
    }

    fn demo_line() -> String {
        let capture = DemoCapture::default();
        let logger = DemoLogger::new(DemoLevel::Info, vec![Box::new(capture.clone())]);

        logger_demo::info!(
            logger,
            "user=alice action=login latency_ms={:04} status=ok session=550e8400-e29b-41d4-a716-446655440000",
            123u32
        );

        let bytes = capture.0.lock().unwrap().clone();
        String::from_utf8(bytes).expect("log line is utf-8")
    }

    #[derive(Clone, Default)]
    struct SimpleCapture(Arc<Mutex<Vec<u8>>>);

    impl SimpleAppender for SimpleCapture {
        fn append(&self, buf: &str) -> io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(buf.as_bytes());
            Ok(buf.len())
        }
    }

    fn simple_line() -> String {
        use logger_baseline_simple::{Level as SimpleLevel, Logger as SimpleLogger};

        const MESSAGE: &str = "user=alice action=login latency_ms=0123 status=ok session=550e8400-e29b-41d4-a716-446655440000";

        let capture = SimpleCapture::default();
        let logger = SimpleLogger::new(SimpleLevel::Info, vec![Box::new(capture.clone())]);

        logger.log(SimpleLevel::Info, 1_700_000_000_123, MESSAGE);

        let bytes = capture.0.lock().unwrap().clone();
        String::from_utf8(bytes).expect("log line is utf-8")
    }

    #[derive(Clone, Default)]
    struct CachePrefixCapture(Arc<Mutex<Vec<u8>>>);

    impl CachePrefixAppender for CachePrefixCapture {
        fn append(&self, buf: &str) -> io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(buf.as_bytes());
            Ok(buf.len())
        }
    }

    fn cache_prefix_line() -> String {
        use logger_cache_prefix::{Level as CachePrefixLevel, Logger as CachePrefixLogger};

        const MESSAGE: &str = "user=alice action=login latency_ms=0123 status=ok session=550e8400-e29b-41d4-a716-446655440000";

        let capture = CachePrefixCapture::default();
        let logger =
            CachePrefixLogger::new(CachePrefixLevel::Info, vec![Box::new(capture.clone())]);

        logger.log(CachePrefixLevel::Info, 1_700_000_000_123, MESSAGE);

        let bytes = capture.0.lock().unwrap().clone();
        String::from_utf8(bytes).expect("log line is utf-8")
    }

    #[derive(Clone, Default)]
    struct StaticAllocCapture(Arc<Mutex<Vec<u8>>>);

    impl StaticAllocAppender for StaticAllocCapture {
        fn append(&self, buf: &str) -> io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(buf.as_bytes());
            Ok(buf.len())
        }
    }

    fn static_alloc_line() -> String {
        use static_alloc_prefix_simple::{Level as StaticAllocLevel, Logger as StaticAllocLogger};

        const MESSAGE: &str = "user=alice action=login latency_ms=0123 status=ok session=550e8400-e29b-41d4-a716-446655440000";

        let capture = StaticAllocCapture::default();
        let logger =
            StaticAllocLogger::new(StaticAllocLevel::Info, vec![Box::new(capture.clone())]);

        logger.log(StaticAllocLevel::Info, 1_700_000_000_123, MESSAGE);

        let bytes = capture.0.lock().unwrap().clone();
        String::from_utf8(bytes).expect("log line is utf-8")
    }

    #[derive(Clone, Default)]
    struct AdaptiveBufferCapture(Arc<Mutex<Vec<u8>>>);

    impl AdaptiveBufferAppender for AdaptiveBufferCapture {
        fn append(&self, buf: &str) -> io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(buf.as_bytes());
            Ok(buf.len())
        }
    }

    fn adaptive_buffer_line() -> String {
        use adaptive_buffer_simple::{Level as AdaptiveLevel, Logger as AdaptiveLogger};

        const MESSAGE: &str = "user=alice action=login latency_ms=0123 status=ok session=550e8400-e29b-41d4-a716-446655440000";

        let capture = AdaptiveBufferCapture::default();
        let logger = AdaptiveLogger::new(AdaptiveLevel::Info, vec![Box::new(capture.clone())]);

        logger.log(AdaptiveLevel::Info, 1_700_000_000_123, MESSAGE);

        let bytes = capture.0.lock().unwrap().clone();
        String::from_utf8(bytes).expect("log line is utf-8")
    }

    struct SpdlogCapture {
        prop: SinkProp,
        lines: Arc<Mutex<Vec<String>>>,
    }

    impl SpdlogCapture {
        fn new() -> Self {
            Self {
                prop: SinkProp::default(),
                lines: Arc::default(),
            }
        }
    }

    impl GetSinkProp for SpdlogCapture {
        fn prop(&self) -> &SinkProp {
            &self.prop
        }
    }

    impl SpdlogSink for SpdlogCapture {
        fn log(&self, record: &SpdlogRecord) -> spdlog::Result<()> {
            let mut buffer = StringBuf::new();
            let mut context = FormatterContext::new();
            self.prop
                .formatter()
                .format(record, &mut buffer, &mut context)?;
            self.lines.lock().unwrap().push(buffer.to_string());
            Ok(())
        }

        fn flush(&self) -> spdlog::Result<()> {
            Ok(())
        }
    }

    fn spdlog_line() -> String {
        let capture = Arc::new(SpdlogCapture::new());
        let logger = SpdlogLogger::builder()
            .level_filter(LevelFilter::All)
            .sink(capture.clone())
            .build()
            .expect("spdlog-rs logger builds");

        spdlog::info!(
            logger: logger,
            "user=alice action=login latency_ms={:04} status=ok session=550e8400-e29b-41d4-a716-446655440000",
            123u32
        );

        let lines = capture.lines.lock().unwrap();
        lines[0].clone()
    }

    #[test]
    fn standard_line_is_128_bytes() {
        let line = baseline_line();

        assert_eq!(line.len() as u64, LINE_BYTES);
        assert!(line.starts_with("[20"), "unexpected line: {line:?}");
        assert!(
            line.ends_with(
                " [INFO] user=alice action=login latency_ms=0123 status=ok \
                 session=550e8400-e29b-41d4-a716-446655440000\n"
            ),
            "unexpected line: {line:?}"
        );
    }

    #[test]
    fn both_implementations_emit_the_same_line() {
        let baseline = baseline_line();
        let demo = demo_line();

        assert_eq!(baseline.len(), demo.len());
        // 跳过 `[<23 字节时间戳>]`，只比较业务字段，避免两次取时间跨秒造成误报。
        assert_eq!(&baseline[25..], &demo[25..]);
    }

    #[test]
    fn event_template_matches_expected_length() {
        let message = format!(
            "user=alice action=login latency_ms={:04} status=ok \
             session=550e8400-e29b-41d4-a716-446655440000",
            123u32
        );

        assert_eq!(message.len(), 94);
        assert_eq!(LINE_BYTES, (33 + message.len() + 1) as u64);
    }

    #[test]
    fn spdlog_rs_line_is_128_bytes() {
        let line = spdlog_line();

        assert_eq!(line.len() as u64, LINE_BYTES);
        assert!(
            line.ends_with(
                " [info] user=alice action=login latency_ms=0123 status=ok \
                 session=550e8400-e29b-41d4-a716-446655440000\n"
            ),
            "unexpected line: {line:?}"
        );
    }

    #[test]
    fn spdlog_rs_payload_matches_other_implementations() {
        let baseline = baseline_line();
        let spdlog = spdlog_line();

        assert_eq!(baseline.len(), spdlog.len());
        // 前缀长度相同，但等级大小写不同：从第 33 字节开始比较业务字段。
        assert_eq!(&baseline[33..], &spdlog[33..]);
    }

    #[test]
    fn baseline_simple_payload_matches_other_implementations() {
        let baseline = baseline_line();
        let simple = simple_line();

        assert_eq!(simple.len() as u64, LINE_BYTES);
        assert_eq!(&baseline[33..], &simple[33..]);
    }

    #[test]
    fn cache_prefix_payload_matches_other_implementations() {
        let baseline = baseline_line();
        let cached = cache_prefix_line();

        assert_eq!(cached.len() as u64, LINE_BYTES);
        assert_eq!(&baseline[33..], &cached[33..]);
    }

    #[test]
    fn static_alloc_prefix_payload_matches_other_implementations() {
        let baseline = baseline_line();
        let static_alloc = static_alloc_line();

        assert_eq!(static_alloc.len() as u64, LINE_BYTES);
        assert_eq!(&baseline[33..], &static_alloc[33..]);
    }

    #[test]
    fn adaptive_buffer_payload_matches_other_implementations() {
        let baseline = baseline_line();
        let adaptive = adaptive_buffer_line();

        assert_eq!(adaptive.len() as u64, LINE_BYTES);
        assert_eq!(&baseline[33..], &adaptive[33..]);
    }
}
