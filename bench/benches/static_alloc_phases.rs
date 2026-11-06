use std::hint::black_box;

use criterion::{Criterion, criterion_group, criterion_main};
use static_alloc_prefix_simple::format_time;

const TIMESTAMP_MILLIS: u64 = 1_700_000_000_123;
const PREFIX: &[u8; 19] = b"2023-11-14 22:13:20";

/// 与 `format_full` 相同的 23 B 写入，用来把「写 buffer」和「缓存检查」拆开。
#[inline(never)]
fn write_23b(buffer: &mut [u8; 23], prefix: &[u8; 19], millis: u32) {
    buffer[..19].copy_from_slice(prefix);
    buffer[19..].copy_from_slice(&[
        b'.',
        b'0' + (millis / 100) as u8,
        b'0' + ((millis / 10) % 10) as u8,
        b'0' + (millis % 10) as u8,
    ]);
}

fn static_alloc_phases(c: &mut Criterion) {
    let mut warm = [0u8; 23];
    format_time(&mut warm, TIMESTAMP_MILLIS);

    c.bench_function("static-alloc/format_time_hit", |b| {
        b.iter(|| {
            let mut buffer = [0u8; 23];
            format_time(black_box(&mut buffer), black_box(TIMESTAMP_MILLIS));
            black_box(buffer);
        });
    });

    let mut second = TIMESTAMP_MILLIS / 1_000;
    c.bench_function("static-alloc/format_time_miss", |b| {
        b.iter(|| {
            second += 1;
            let mut buffer = [0u8; 23];
            format_time(black_box(&mut buffer), black_box(second * 1_000 + 123));
            black_box(buffer);
        });
    });

    c.bench_function("static-alloc/write_23b", |b| {
        b.iter(|| {
            let mut buffer = [0u8; 23];
            write_23b(black_box(&mut buffer), black_box(PREFIX), black_box(123));
            black_box(buffer);
        });
    });
}

criterion_group!(benches, static_alloc_phases);
criterion_main!(benches);
