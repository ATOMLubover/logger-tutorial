# logger-bench

用 [Criterion.rs](https://bheisler.github.io/criterion.rs/book/) 对比同一份 **128 B 标准负载**下的七个实现：

- `logger-baseline-simple`：文章「实现最简单 `log` 函数」的朴素版，不支持格式化；
- `logger-cache-prefix`：在 baseline-simple 上只加秒级格式化前缀缓存，仍不支持格式化；
- `static-alloc-prefix-simple`：时间格式化改为写入栈 buffer，前缀缓存也改成 `[u8; 19]`；
- `adaptive-buffer-simple`：短日志写进 128 B 栈 buffer，超长才 fallback 到堆；
- `logger-baseline`：引入 `fmt::Arguments` 后、仍用 `format!` 拼整行 `String` 的版本；
- `logger-demo`：buffer + 时间缓存 + `fmt::Arguments` 直写的优化版；
- `spdlog-rs` 0.5.3：外部对照。

全部走 discard sink，只关注非 appender 部分。

## 编译与运行配置

workspace 根部统一为：

```toml
[profile.release]
opt-level = 2
lto = true
codegen-units = 1

[profile.bench]
inherits = "release"
```

`cargo bench` 使用 `bench` profile，所有实现和 benchmark 本身都是 **O2 + LTO + 单 codegen unit** 的 release 构建。

## 128 B 标准负载

```
[2026-11-03 10:58:11.123] [INFO] user=alice action=login latency_ms=0123 status=ok session=550e8400-e29b-41d4-a716-446655440000
```

- 前缀 `[<23 B 时间戳>] [INFO] ` 共 33 B；
- 消息体 94 B：`latency_ms` 固定 4 位，`session` 是 36 B UUID；
- 加末尾换行共 128 B；
- `logger-demo` / `logger-baseline` / `spdlog-rs` 在计时路径内格式化 `latency_ms`；
- `logger-baseline-simple` / `logger-cache-prefix` / `static-alloc-prefix-simple` / `adaptive-buffer-simple` 不支持格式化，所以传入预先拼好的 94 B `&str`（`latency_ms=0123` 固定），时间戳由调用方取好再传入；它们测的是 **时间序列化 + 整行 `format!` + 派发**，不含消息格式化。

`src/lib.rs` 的测试会校验每一路的整行长度和业务字段。

## Sink

全部 discard：

| sink | 行为 |
| --- | --- |
| `sink::SimpleDiscard` | 读取 `&str` 指针和长度后丢弃 |
| `sink::CachePrefixDiscard` | 读取 `&str` 指针和长度后丢弃 |
| `sink::StaticAllocDiscard` | 读取 `&str` 指针和长度后丢弃 |
| `sink::AdaptiveBufferDiscard` | 读取 `&str` 指针和长度后丢弃 |
| `sink::BaselineDiscard` | 读取 `&str` 指针和长度后丢弃 |
| `sink::DemoDiscard` | 读取 `&[u8]` 指针和长度后丢弃 |
| `sink::SpdlogDiscard` | `FullFormatter` 格式化到 `StringBuf` 后丢弃 |

`black_box` 保证格式化结果被编译器认为「被使用」。没有 stdout、文件或任何系统调用。

## 跑法

```sh
cargo bench -p logger-bench -- --verbose --save-baseline o2-128b
```

默认预热 3 s、测量 5 s、100 个样本；`--verbose` 额外输出 median / MAD。

## Profile

默认开启 LTO 时，`Logger::log`、`format_time` 会被内联进 `main`，采样只能看到 `main` 和 libc 叶子函数。profile 构建启用 `profile` feature，**只**给这些函数加 `#[inline(never)]`（不改变逻辑），让 `sample` 能归因到代码里实际调用的函数：

```sh
CARGO_PROFILE_RELEASE_DEBUG=1 \
  cargo build -p logger-bench --release --example profile --features profile

./target/release/examples/profile simple 20 &
sample $! 8 -file /tmp/simple.txt
```

`Logger::log` 调用树（macOS `sample`，1 ms 间隔，leaf 共 6154 个样本）：

| 函数 | inclusive 样本 | 占 `log` | 作用 |
| --- | ---: | ---: | --- |
| `Logger::log` | 5796 | 100% | 整个 `log` 调用 |
| ├ `Logger::format_time` | 3017 | 52.1% | 取时区 + 时间序列化 |
| ├ `Logger::format_line` | 2411 | 41.6% | `format!` 拼整行 + `String` 分配 |
| ├ `Logger::dispatch` | 20 | 0.3% | appender 派发 |
| └ `log` 自身 | 348 | 6.0% | 等级检查、函数调用 |

结论：**时间序列化约占一半，整行 `String` 拼接约占四成，派发可以忽略**。`format_time` 内部主要是 `localtime_r`/`tzset` 和 `time` 格式化；`format_line` 内部主要是 `malloc`/`realloc`/`memcpy`。

### 再拆时间路径

`cargo bench -p logger-bench --bench phases -- --verbose` 把各阶段单独测（同样 O2 + LTO，wrapper `#[inline(never)]`）：

| 阶段 | 函数 | median | 占单条日志（~270 ns） |
| --- | --- | ---: | ---: |
| 取时区偏移 | `current_offset` | 93.5–93.9 ns | ~35% |
| 转本地时间 | `to_local` | 6.83 ns | ~2.5% |
| 时间转 23 B `String` | `format_datetime` | 51.8–52.0 ns | ~19% |
| 整行转 128 B `String` | `format_line` | 102.2–102.5 ns | ~38% |
| 等级检查 / 派发 / 调用 | — | ~15 ns | ~5% |

在 `format_time` 内部：时区偏移约占 62%，`format` 生成时间 `String` 约占 34%，`to_local` 只占 4%。所以在三个候选里，**本地时间转换本身不是瓶颈**；真正贵的是每次重新查询时区（`current_offset`）以及两次 `String` 分配/写入。

profile 构建每条约 282–289 ns，默认 O2 benchmark 约 270 ns（不同 session 会漂移），差异来自 `inline(never)` 的调用开销；只看占比，不要拿 profile 构建的绝对值跟 benchmark 比。

### cache-prefix 的 profile

同样方式采样 `cache-prefix`（profile 构建 171.3 ns/log，默认 benchmark 约 153–154 ns）：

```sh
./target/release/examples/profile cache-prefix 20 &
sample $! 8 -file /tmp/cache-prefix.txt
```

| 函数 | inclusive 样本 | 占 `log` | 作用 |
| --- | ---: | ---: | --- |
| `Logger::log` | 5732 | 100% | 整个 `log` 调用 |
| ├ `Logger::format_line` | 3908 | 68.2% | 128 B 整行 `String` |
| ├ `format_time` | 1520 | 26.5% | 时间字符串 |
| │ └ `format_prefix`（冷路径） | 0 | <0.1% | 每秒一次的时区 + 格式化 |
| ├ `Logger::dispatch` | 49 | 0.9% | appender 派发 |
| └ `log` 自身 | 255 | 4.4% | 等级检查、函数调用 |

前缀缓存把 `format_time` 从约 57% 压到 26.5%，冷路径在 8 s 采样里完全看不到；此时 **`format_line` 的 128 B `String` 拼接升到约 68%，成为新的瓶颈**。

再单独测 `format_time` 内部（命中缓存为主）：

| 阶段 | 函数/操作 | 单次耗时 | 占命中调用 |
| --- | --- | ---: | ---: |
| 命中：拼接 `prefix.mmm` | `format!("{}.{:03}")` | 38.0 ns | 95.2% |
| 缓存检查 + 借用 | `format_time` 自身 | 1.9 ns | 4.8% |
| 未命中：计算前缀 | `format_prefix` | 149.5 ns（仅每秒一次） | 摊销后 ≈0% |

命中时 `format_time` 的 39.9 ns 里，95% 都花在最终那次 `format!` 上；`format_prefix` 只在每秒第一次调用时执行，摊销后可以忽略。

### static-alloc-prefix 的 profile

`static-alloc-prefix-simple` 把时间写入 23 B 栈 buffer，前缀缓存改为 `[u8; 19]`，不再为时间创建 `String`；整行仍然用 `format!`。profile 构建 130.3 ns/log，默认 benchmark 约 117.6–118.4 ns：

```sh
./target/release/examples/profile static-alloc-prefix 20 &
sample $! 8 -file /tmp/static-alloc-prefix.txt
```

| 函数 | inclusive 样本 | 占 `log` | 作用 |
| --- | ---: | ---: | --- |
| `Logger::log` | 5472 | 100% | 整个 `log` 调用 |
| ├ `Logger::format_line`（被内联进 `log`） | ~5370 | ~98.1% | 128 B 整行 `String` |
| ├ `format_time` | 74 | 1.4% | 写 23 B 栈 buffer |
| └ `log` 自身 | 28 | 0.5% | 等级检查、调用 |

时间部分从 cache-prefix 的 26.5% 降到 **1.4%**，静态分配把时间格式化的堆分配彻底消掉了；此时 **几乎全部时间都在整行 `format!` 的 `String` 分配/拼接上**，这也是下一步引入 buffer 的直接动机。

### adaptive-buffer 的 profile

`adaptive-buffer-simple` 把整行写进 thread-local `AdaptiveBuffer`：默认 128 B 固定缓冲，超长才升级为堆。profile 构建 48.5 ns/log，默认 benchmark 约 45.8–46.0 ns；单独测 `format_line` 约 30.1 ns：

```sh
./target/release/examples/profile adaptive-buffer 20 &
sample $! 8 -file /tmp/adaptive-buffer.txt

./target/release/examples/profile adaptive-buffer-format-line 20 &
sample $! 8 -file /tmp/adaptive-format-line.txt
```

`log` 内部分布（leaf 共 4808 个样本）：

| 阶段 | 函数 | 占比 |
|:--:|:----:|:-:|
| 整行写 buffer | `Logger::format_line` | 65.4% |
| buffer 借用/clear/其它 | `Logger::log` 自身 | 14.6% |
| `as_ref` 的 UTF-8 校验 | `from_utf8` | 12.3% |
| 时间格式化 | `format_time` | 4.1% |
| 派发 | `dispatch` | 3.7% |

`format_line` 内部分布（leaf 共 5652 个样本）：

| 阶段 | 函数 | 占比 |
|:--:|:----:|:-:|
| 写 128 B buffer（含拷贝） | `AdaptiveBuffer::write_str` + `memmove` | 58.1% |
| core::fmt 分派 | `fmt::write` / `Formatter` | 27.3% |
| 时间 buffer 的 UTF-8 校验 | `from_utf8` | 12.6% |
| 其它 | — | 2.0% |

即使固定缓冲消除了堆分配，`format_line` 的主要开销仍是 **写 128 B 固定缓冲（含拷贝）和 `core::fmt` 分派**；另外 `FixedBuffer::as_ref` 每次都会对 128 B 做一次 UTF-8 校验，在 `log` 里约占 12%。

## 覆盖范围

- `discard-128B/baseline-simple`：朴素版，无格式化、无 buffer；
- `discard-128B/cache-prefix`：朴素版 + 秒级格式化前缀缓存；
- `discard-128B/static-alloc-prefix`：时间写入栈 buffer + 静态前缀缓存；
- `discard-128B/adaptive-buffer`：128 B 固定 buffer，超长 fallback 堆；
- `discard-128B/baseline`：`fmt::Arguments` + 整行 `String`；
- `discard-128B/optimized`：thread-local buffer + 秒级时间缓存 + `Arguments` 直写；
- `discard-128B/spdlog-rs`：默认 `FullFormatter` + `flexible-string`。

## 这个 benchmark 不能说明什么

- **同负载**：`baseline-simple` / `cache-prefix` / `static-alloc-prefix` / `adaptive-buffer` 不格式化，消息是预拼好的，和其余三路不是完全对等的计算量；
- **appender / IO**：全部 discard，`write`、flush、文件、管道、终端都不在测量范围内；
- **多线程**：单线程，没有锁竞争、cache line 和分配器并发开销；
- **尾延迟**：只有 Criterion 采样分布的 median/MAD，没有单独统计 p99；
- **等级过滤**：只测 `Info` 全开；
- **profile 精度**：1 ms 采样 + 符号归并，只能看数量级和占比，不能当 cycle 级归因；
- **跨机器**：只适合同一 session、同一电源状态、同一后台负载下背靠背比较。
