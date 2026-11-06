use std::hint::black_box;

use adaptive_buffer_simple::{AdaptiveBuffer, Level, Logger, TIME_STR_LEN, format_time};
use criterion::{Criterion, criterion_group, criterion_main};

const MESSAGE: &str = "user=alice action=login latency_ms=0123 status=ok session=550e8400-e29b-41d4-a716-446655440000";
const TIMESTAMP_MILLIS: u64 = 1_700_000_000_123;

fn adaptive_buffer_phases(c: &mut Criterion) {
    let mut time_buffer = [0u8; TIME_STR_LEN];
    format_time(&mut time_buffer, TIMESTAMP_MILLIS);

    c.bench_function("adaptive-buffer/format_line", |b| {
        let mut buffer = AdaptiveBuffer::default();
        b.iter(|| {
            buffer.clear();
            Logger::format_line(
                &mut buffer,
                black_box(&time_buffer),
                Level::Info,
                black_box(MESSAGE),
            );
            black_box(buffer.as_ref());
        });
    });

    c.bench_function("adaptive-buffer/log", |b| {
        let logger = Logger::new(Level::Info, vec![Box::new(Discard)]);
        b.iter(|| {
            let timestamp_millis = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64;

            black_box(&logger).log(Level::Info, black_box(timestamp_millis), black_box(MESSAGE));
        });
    });
}

struct Discard;

impl adaptive_buffer_simple::Appender for Discard {
    fn append(&self, buf: &str) -> std::io::Result<usize> {
        black_box(buf.as_bytes().as_ptr());
        black_box(buf.len());
        Ok(buf.len())
    }
}

criterion_group!(benches, adaptive_buffer_phases);
criterion_main!(benches);
