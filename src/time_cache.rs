use time::format_description::StaticFormatDescription;
use time::{OffsetDateTime, UtcOffset};

const DATE_TIME_FORMAT: StaticFormatDescription =
    time::macros::format_description!("[year]-[month]-[day] [hour]:[minute]:[second]");

const TIME_TEXT_LIMIT: usize = 19;

struct TimeText([u8; TIME_TEXT_LIMIT]);

impl TimeText {
    fn new() -> Self {
        Self([0; 19])
    }

    fn as_bytes(&self) -> &[u8] {
        &self.0[..TIME_TEXT_LIMIT]
    }

    fn as_mut_slice(&mut self) -> &mut [u8] {
        &mut self.0[..TIME_TEXT_LIMIT]
    }
}

pub struct TimeCache {
    second: u64,
    text: TimeText,
}

impl TimeCache {
    pub fn new() -> Self {
        Self {
            second: u64::MAX,
            text: TimeText::new(),
        }
    }

    pub fn get(&mut self, timestamp_millis: u64) -> (&[u8], u32) {
        let second = timestamp_millis / 1000;
        if self.second != second {
            // 当 second 不对齐时，重新构造
            self.second = second;

            let offset = UtcOffset::current_local_offset().unwrap_or(UtcOffset::UTC);
            let local =
                OffsetDateTime::from_unix_timestamp_nanos(timestamp_millis as i128 * 1_000_000)
                    .expect("timestamp is in range")
                    .to_offset(offset);

            let mut output = self.text.as_mut_slice();
            let written = local
                .format_into(&mut output, DATE_TIME_FORMAT)
                .expect("date/time text fits in 19 bytes");

            debug_assert_eq!(written, TIME_TEXT_LIMIT);
        }

        (self.text.as_bytes(), (timestamp_millis % 1000) as u32)
    }
}

#[cfg(test)]
mod tests {
    use super::TimeCache;

    #[test]
    fn caches_within_second_and_refreshes_on_next() {
        let mut cache = TimeCache::new();

        let (first, millis) = cache.get(1_700_000_000_123);
        assert_eq!(first.len(), 19);
        assert_eq!(millis, 123);
        let first = first.to_vec();

        let (same_second, millis) = cache.get(1_700_000_000_999);
        assert_eq!(same_second, first.as_slice());
        assert_eq!(millis, 999);

        let (next_second, _) = cache.get(1_700_000_001_000);
        assert_ne!(next_second, first.as_slice());
    }
}
