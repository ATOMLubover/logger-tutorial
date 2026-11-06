/// 日志后端接口。
///
/// 与文章一致，接收不可变的 `&self` 以便跨线程共享，并接收已经格式化好的 `&str`。
pub trait Appender {
    fn append(&self, buf: &str) -> std::io::Result<usize>;
}

/// 丢弃输出、只回报写入长度的 appender。
///
/// 用来在 benchmark 中排除 IO 干扰，只测量日志前端构造与派发的开销。
#[derive(Debug, Default, Clone, Copy)]
pub struct NullAppender;

impl Appender for NullAppender {
    fn append(&self, buf: &str) -> std::io::Result<usize> {
        Ok(buf.len())
    }
}
