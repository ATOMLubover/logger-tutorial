//! 用 `#[inline(never)]` 包住 `format!`，让采样器至少能看到宏展开后的这一层。
//!
//! ```sh
//! CARGO_PROFILE_RELEASE_DEBUG=1 cargo build -p logger-bench --release --example format_probe
//! ./target/release/examples/format_probe 16 &
//! sample $! 8 -file /tmp/format-probe.txt
//! ```

use std::hint::black_box;
use std::time::{Duration, Instant};

const PREFIX: &str = "2026-11-03 10:58:11";

/// 和 `cache-prefix` 命中路径里的 `format!("{}.{:03}", prefix, millis)` 完全一致。
#[inline(never)]
fn join(prefix: &str, millis: u32) -> String {
    format!("{}.{:03}", prefix, millis)
}

fn main() {
    let seconds = std::env::args()
        .nth(1)
        .and_then(|arg| arg.parse::<u64>().ok())
        .unwrap_or(10);

    let start = Instant::now();
    let mut count = 0u64;

    while start.elapsed() < Duration::from_secs(seconds) {
        for _ in 0..1024 {
            black_box(join(black_box(PREFIX), black_box(123)));
        }
        count += 1024;
    }

    let elapsed = start.elapsed();
    println!(
        "{count} joins in {elapsed:?} ({:.1} ns/join)",
        elapsed.as_nanos() as f64 / count as f64
    );
}
