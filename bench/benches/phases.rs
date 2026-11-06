use std::hint::black_box;

use criterion::{Criterion, criterion_group, criterion_main};
use logger_baseline_simple::{Level, phases};

const MESSAGE: &str = "user=alice action=login latency_ms=0123 status=ok session=550e8400-e29b-41d4-a716-446655440000";
const TIMESTAMP_MILLIS: u64 = 1_700_000_000_123;

fn phases_bench(c: &mut Criterion) {
    let offset = phases::current_offset();
    let local = phases::to_local(TIMESTAMP_MILLIS, offset);
    let formatted_time = phases::format_datetime(local);

    c.bench_function("phase/current_offset", |b| {
        b.iter(|| black_box(phases::current_offset()));
    });

    c.bench_function("phase/to_local", |b| {
        b.iter(|| {
            black_box(phases::to_local(
                black_box(TIMESTAMP_MILLIS),
                black_box(offset),
            ))
        });
    });

    c.bench_function("phase/format_datetime_to_string", |b| {
        b.iter(|| black_box(phases::format_datetime(black_box(local))));
    });

    c.bench_function("phase/format_line_to_string", |b| {
        b.iter(|| {
            black_box(phases::format_line(
                black_box(&formatted_time),
                Level::Info,
                black_box(MESSAGE),
            ))
        });
    });
}

criterion_group!(benches, phases_bench);
criterion_main!(benches);
