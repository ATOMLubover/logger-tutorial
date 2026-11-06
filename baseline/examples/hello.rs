use std::io::Write as _;

use logger_baseline::{Appender, Level, Logger};

struct Stdout;

impl Appender for Stdout {
    fn append(&self, buf: &str) -> std::io::Result<usize> {
        let mut stdout = std::io::stdout().lock();
        stdout.write_all(buf.as_bytes())?;
        Ok(buf.len())
    }
}

fn main() {
    let logger = Logger::new(Level::Info, vec![Box::new(Stdout)]);

    logger.log_fmt(
        Level::Info,
        format_args!("message logged: {}", "Hello world!"),
    );
    logger.log_fmt(
        Level::Info,
        format_args!("message logged: {}", "Greeting from LB"),
    );

    // 低于 min_level，不会产生任何输出，也不会构造格式化文本。
    logger.log_fmt(Level::Debug, format_args!("filtered out"));
}
