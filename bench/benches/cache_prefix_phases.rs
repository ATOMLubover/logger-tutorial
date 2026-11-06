use std::hint::black_box;

use criterion::{Criterion, criterion_group, criterion_main};
use logger_cache_prefix::format_time;

const PREFIX: &str = "2026-11-03 10:58:11";
const TIMESTAMP_MILLIS: u64 = 1_700_000_000_123;

fn cache_prefix_phases(c: &mut Criterion) {
    // 先预热，确保 hit 组从缓存命中开始。
    let _ = format_time(TIMESTAMP_MILLIS);

    c.bench_function("cache-prefix/format_time_hit", |b| {
        b.iter(|| black_box(format_time(black_box(TIMESTAMP_MILLIS))));
    });

    let mut second = TIMESTAMP_MILLIS / 1_000;
    c.bench_function("cache-prefix/format_time_miss", |b| {
        b.iter(|| {
            second += 1;
            black_box(format_time(black_box(second * 1_000 + 123)))
        });
    });

    c.bench_function("cache-prefix/join_only", |b| {
        b.iter(|| black_box(format!("{}.{:03}", black_box(PREFIX), black_box(123u32))));
    });
}

criterion_group!(benches, cache_prefix_phases);
criterion_main!(benches);
