pub trait Appender {
    fn append(&self, buf: &[u8]) -> std::io::Result<usize>;
}
