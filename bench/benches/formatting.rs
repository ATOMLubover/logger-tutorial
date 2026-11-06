use std::hint::black_box;
use std::sync::Arc;

use adaptive_buffer_simple::{Level as AdaptiveBufferLevel, Logger as AdaptiveBufferLogger};
use criterion::{Criterion, Throughput, criterion_group, criterion_main};
use logger_baseline::{Level as BaselineLevel, Logger as BaselineLogger};
use logger_baseline_simple::{Level as SimpleLevel, Logger as SimpleLogger};
use logger_bench::LINE_BYTES;
use logger_bench::sink::{
    AdaptiveBufferDiscard, BaselineDiscard, CachePrefixDiscard, DemoDiscard, SimpleDiscard,
    SpdlogDiscard, StaticAllocDiscard,
};
use logger_cache_prefix::{Level as CachePrefixLevel, Logger as CachePrefixLogger};
use logger_demo::{Level as DemoLevel, Logger as DemoLogger};
use spdlog::{LevelFilter, Logger as SpdlogLogger};
use static_alloc_prefix_simple::{Level as StaticAllocLevel, Logger as StaticAllocLogger};

/// 128 B 标准负载，全部走 discard sink，只测非 appender 部分。
fn discard(c: &mut Criterion) {
    let mut group = c.benchmark_group("discard-128B");
    group.throughput(Throughput::Bytes(LINE_BYTES));

    group.bench_function("baseline", |b| {
        let logger = BaselineLogger::new(BaselineLevel::Info, vec![Box::new(BaselineDiscard)]);
        let mut sequence = 0u32;

        b.iter(|| {
            sequence = sequence.wrapping_add(1);
            let latency_ms = black_box(sequence) & 1023;

            black_box(&logger).log_fmt(
                BaselineLevel::Info,
                format_args!(
                    "user=alice action=login latency_ms={latency_ms:04} status=ok \
                     session=550e8400-e29b-41d4-a716-446655440000"
                ),
            );
        });
    });

    group.bench_function("baseline-simple", |b| {
        const MESSAGE: &str = "user=alice action=login latency_ms=0123 status=ok session=550e8400-e29b-41d4-a716-446655440000";

        let logger = SimpleLogger::new(SimpleLevel::Info, vec![Box::new(SimpleDiscard)]);
        b.iter(|| {
            let timestamp_millis = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64;

            black_box(&logger).log(
                SimpleLevel::Info,
                black_box(timestamp_millis),
                black_box(MESSAGE),
            );
        });
    });

    group.bench_function("cache-prefix", |b| {
        const MESSAGE: &str = "user=alice action=login latency_ms=0123 status=ok session=550e8400-e29b-41d4-a716-446655440000";

        let logger = CachePrefixLogger::new(CachePrefixLevel::Info, vec![Box::new(CachePrefixDiscard)]);
        b.iter(|| {
            let timestamp_millis = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64;

            black_box(&logger).log(
                CachePrefixLevel::Info,
                black_box(timestamp_millis),
                black_box(MESSAGE),
            );
        });
    });

    group.bench_function("static-alloc-prefix", |b| {
        const MESSAGE: &str = "user=alice action=login latency_ms=0123 status=ok session=550e8400-e29b-41d4-a716-446655440000";

        let logger = StaticAllocLogger::new(
            StaticAllocLevel::Info,
            vec![Box::new(StaticAllocDiscard)],
        );
        b.iter(|| {
            let timestamp_millis = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64;

            black_box(&logger).log(
                StaticAllocLevel::Info,
                black_box(timestamp_millis),
                black_box(MESSAGE),
            );
        });
    });

    group.bench_function("adaptive-buffer", |b| {
        const MESSAGE: &str = "user=alice action=login latency_ms=0123 status=ok session=550e8400-e29b-41d4-a716-446655440000";

        let logger = AdaptiveBufferLogger::new(
            AdaptiveBufferLevel::Info,
            vec![Box::new(AdaptiveBufferDiscard)],
        );
        b.iter(|| {
            let timestamp_millis = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64;

            black_box(&logger).log(
                AdaptiveBufferLevel::Info,
                black_box(timestamp_millis),
                black_box(MESSAGE),
            );
        });
    });

    group.bench_function("optimized", |b| {
        let logger = DemoLogger::new(DemoLevel::Info, vec![Box::new(DemoDiscard)]);
        let mut sequence = 0u32;

        b.iter(|| {
            sequence = sequence.wrapping_add(1);
            let latency_ms = black_box(sequence) & 1023;

            logger_demo::info!(
                black_box(&logger),
                "user=alice action=login latency_ms={latency_ms:04} status=ok \
                 session=550e8400-e29b-41d4-a716-446655440000"
            );
        });
    });

    group.bench_function("spdlog-rs", |b| {
        let logger = SpdlogLogger::builder()
            .level_filter(LevelFilter::All)
            .sink(Arc::new(SpdlogDiscard::new()))
            .build()
            .expect("spdlog-rs logger builds");
        let mut sequence = 0u32;

        b.iter(|| {
            sequence = sequence.wrapping_add(1);
            let latency_ms = black_box(sequence) & 1023;

            spdlog::info!(
                logger: black_box(&logger),
                "user=alice action=login latency_ms={latency_ms:04} status=ok \
                 session=550e8400-e29b-41d4-a716-446655440000"
            );
        });
    });

    group.finish();
}

criterion_group!(benches, discard);
criterion_main!(benches);
