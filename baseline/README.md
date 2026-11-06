# logger-baseline

文章《简单 logger 实现中的常见性能优化》中 **「实现最初版本 `log`」** 一节的基线实现，用来和同仓库的优化版 `logger-demo` 做 benchmark 对照。

## 与文章小节的对应

| 文章内容 | 代码 |
| --- | --- |
| `Record { level, timestamp_millis, message: String }` | [`src/lib.rs`](src/lib.rs) 的 `Record` |
| `Level` 枚举 | [`src/level.rs`](src/level.rs) |
| `Appender::append` | [`src/appender.rs`](src/appender.rs) |
| `Logger { min_level, appenders: Arc<[BoxAppender]> }` | [`src/lib.rs`](src/lib.rs) |
| 最初版本的 `log` | `Logger::log` |

## 刻意保留的朴素之处

- `Record.message` 是独占的 `String`：每条日志都要分配、拼接一次；
- 时间戳每次调用都用 `time` 重新做时区转换和格式化，并产生新的 `String`；
- 没有 thread-local buffer，也没有时间戳缓存；
- `appender` 接收 `&str`（与文章一致），没有采用优化版后来的字节 buffer。

## 与文章的少量偏差

1. 文章的 `Appender::append` 签名写作 `std::fmt::Result<usize>`，但 `fmt::Result` 没有泛型参数，无法通过编译。这里改成 `std::io::Result<usize>`，与优化版保持一致。
2. 文章在这一小节只实现了 `Logger::log(&Record)`，没有给出记录构造和格式化路径。为了能端到端 benchmark，补充了最朴素的 `Logger::log_fmt(level, fmt::Arguments)`：用 `format!` 拼出整行后交给 `log`。它是 benchmark 的入口，不属于文章正文代码。
3. 额外提供了 `NullAppender`（丢弃输出、只返回长度），用于在 benchmark 中排除 IO 干扰。

## 运行

```sh
# 示例输出，对应文章里的期望输出
cargo run -p logger-baseline --example hello

# 测试
cargo test -p logger-baseline

# benchmark 请务必使用 release 构建
cargo build -p logger-baseline --release
```

benchmark 时建议：

- 用 `NullAppender` 或自写的计数 appender 隔离 IO，只测前端构造与派发；
- 用 `std::hint::black_box` 包住 logger / 参数，避免优化被消除；
- 基线每条日志至少有一次 `String` 分配和一次时间戳格式化，和优化版的差距会随消息长度、线程数变化，测量时固定消息内容与等级；
- 若要和 `logger-demo` 对比，注意两者入口不同：基线是 `log_fmt`，优化版是 `info!` 宏（宏内部还会先做一次等级检查）。
