//! 供 `sample` / Instruments 采样：以 128 B 标准负载持续打日志若干秒。
//!
//! ```sh
//! CARGO_PROFILE_RELEASE_DEBUG=1 cargo build -p logger-bench --release --example profile
//! ./target/release/examples/profile simple 20
//! ./target/release/examples/profile optimized 20
//! ```

use std::hint::black_box;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use adaptive_buffer_simple::{
    AdaptiveBuffer, Level as AdaptiveBufferLevel, Logger as AdaptiveBufferLogger, TIME_STR_LEN,
    format_time,
};
use logger_baseline_simple::{Level as SimpleLevel, Logger as SimpleLogger};
use logger_bench::sink::{
    AdaptiveBufferDiscard, CachePrefixDiscard, DemoDiscard, SimpleDiscard, StaticAllocDiscard,
};
use logger_cache_prefix::{Level as CachePrefixLevel, Logger as CachePrefixLogger};
use logger_demo::{Level as DemoLevel, Logger as DemoLogger};
use static_alloc_prefix_simple::{Level as StaticAllocLevel, Logger as StaticAllocLogger};

const MESSAGE: &str = "user=alice action=login latency_ms=0123 status=ok session=550e8400-e29b-41d4-a716-446655440000";

fn main() {
    let mut args = std::env::args().skip(1);
    let target = args.next().unwrap_or_else(|| "simple".to_owned());
    let seconds = args
        .next()
        .and_then(|arg| arg.parse::<u64>().ok())
        .unwrap_or(10);

    match target.as_str() {
        "simple" => {
            let logger = SimpleLogger::new(SimpleLevel::Info, vec![Box::new(SimpleDiscard)]);

            drive(seconds, || {
                let timestamp_millis = now_millis();

                black_box(&logger).log(
                    SimpleLevel::Info,
                    black_box(timestamp_millis),
                    black_box(MESSAGE),
                );
            });
        }
        "cache-prefix" => {
            let logger =
                CachePrefixLogger::new(CachePrefixLevel::Info, vec![Box::new(CachePrefixDiscard)]);

            drive(seconds, || {
                let timestamp_millis = now_millis();

                black_box(&logger).log(
                    CachePrefixLevel::Info,
                    black_box(timestamp_millis),
                    black_box(MESSAGE),
                );
            });
        }
        "static-alloc-prefix" => {
            let logger =
                StaticAllocLogger::new(StaticAllocLevel::Info, vec![Box::new(StaticAllocDiscard)]);

            drive(seconds, || {
                let timestamp_millis = now_millis();

                black_box(&logger).log(
                    StaticAllocLevel::Info,
                    black_box(timestamp_millis),
                    black_box(MESSAGE),
                );
            });
        }
        "adaptive-buffer" => {
            let logger = AdaptiveBufferLogger::new(
                AdaptiveBufferLevel::Info,
                vec![Box::new(AdaptiveBufferDiscard)],
            );

            drive(seconds, || {
                let timestamp_millis = now_millis();

                black_box(&logger).log(
                    AdaptiveBufferLevel::Info,
                    black_box(timestamp_millis),
                    black_box(MESSAGE),
                );
            });
        }
        "adaptive-buffer-format-line" => {
            let mut buffer = AdaptiveBuffer::default();
            let mut time_buffer = [0u8; TIME_STR_LEN];
            format_time(&mut time_buffer, 1_700_000_000_123);

            drive(seconds, || {
                buffer.clear();
                AdaptiveBufferLogger::format_line(
                    &mut buffer,
                    &time_buffer,
                    AdaptiveBufferLevel::Info,
                    black_box(MESSAGE),
                );
                black_box(buffer.as_ref());
            });
        }
        "optimized" => {
            let logger = DemoLogger::new(DemoLevel::Info, vec![Box::new(DemoDiscard)]);
            let mut sequence = 0u32;

            drive(seconds, || {
                sequence = sequence.wrapping_add(1);
                let latency_ms = black_box(sequence) & 1023;

                logger_demo::info!(
                    black_box(&logger),
                    "user=alice action=login latency_ms={latency_ms:04} status=ok \
                     session=550e8400-e29b-41d4-a716-446655440000"
                );
            });
        }
        other => panic!("unknown target {other:?}; use `simple` or `optimized`"),
    }
}

fn now_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

fn drive(seconds: u64, mut log: impl FnMut()) {
    let mut count = 0u64;
    let start = Instant::now();

    while start.elapsed() < Duration::from_secs(seconds) {
        for _ in 0..1024 {
            log();
        }
        count += 1024;
    }

    let elapsed = start.elapsed();
    println!(
        "{count} logs in {elapsed:?} ({:.1} ns/log)",
        elapsed.as_nanos() as f64 / count as f64
    );
}
