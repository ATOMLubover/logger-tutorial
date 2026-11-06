use crate::buffer::dynamic_buffer::DynamicBuffer;
use crate::buffer::fixed_buffer::FixedBuffer;

mod dynamic_buffer;
mod fixed_buffer;

#[derive(Default)]
pub struct AdaptiveBuffer {
    stack: FixedBuffer,
    heap: Option<DynamicBuffer>,
}

impl AdaptiveBuffer {
    pub fn clear(&mut self) {
        // 回到 stack，释放堆缓冲区
        self.heap = None;
        self.stack.clear();
    }
}

impl std::io::Write for AdaptiveBuffer {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        if let Some(heap) = &mut self.heap {
            return heap.write(buf);
        }

        // Fast path: 直接将数据先尝试放在 stack 上
        if buf.len() <= self.stack.capacity() - self.stack.len() {
            return self.stack.write(buf);
        }

        let expected_size = self
            .stack
            .len()
            .checked_add(buf.len())
            .expect("buffer size should not overflow");

        let mut heap = DynamicBuffer::with_capacity(expected_size.max(self.stack.capacity() * 2));
        heap.write(self.stack.as_ref())?;
        heap.write(buf)?;

        self.heap = Some(heap);

        // io::Write 的调用方只关心本次从 buf 消费了多少字节，
        // 栈上已有的旧数据不应计入返回值。
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl AsRef<[u8]> for AdaptiveBuffer {
    fn as_ref(&self) -> &[u8] {
        match &self.heap {
            Some(heap) => heap.as_ref(),
            None => self.stack.as_ref(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::AdaptiveBuffer;
    use std::io::Write as _;

    #[test]
    fn spills_to_heap_and_clears() {
        let mut buffer = AdaptiveBuffer::default();

        buffer.write_all(&[b'a'; 200]).unwrap();
        assert_eq!(buffer.as_ref().len(), 200);

        buffer.write_all(&[b'b'; 200]).unwrap();
        assert_eq!(buffer.as_ref().len(), 400);
        assert!(buffer.as_ref()[..200].iter().all(|&byte| byte == b'a'));
        assert!(buffer.as_ref()[200..].iter().all(|&byte| byte == b'b'));

        buffer.clear();
        assert!(buffer.as_ref().is_empty());

        buffer.write_all(b"xyz").unwrap();
        assert_eq!(buffer.as_ref(), &b"xyz"[..]);
    }
}
