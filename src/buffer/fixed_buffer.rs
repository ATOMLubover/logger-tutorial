const DEFAULT_STACK_SIZE: usize = 1 << 8;

pub struct FixedBuffer<const N: usize = DEFAULT_STACK_SIZE> {
    data: [u8; N],
    size: usize,
}

impl<const N: usize> FixedBuffer<N> {
    pub fn clear(&mut self) {
        self.size = 0;
    }
}

impl<const N: usize> FixedBuffer<N> {
    pub const fn capacity(&self) -> usize {
        N
    }

    pub fn len(&self) -> usize {
        self.size
    }
}

impl<const N: usize> Default for FixedBuffer<N> {
    fn default() -> Self {
        Self {
            data: [0; N],
            size: 0,
        }
    }
}

impl<const N: usize> std::io::Write for FixedBuffer<N> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        debug_assert!(buf.len() <= N - self.size);

        self.data[self.size..self.size + buf.len()].copy_from_slice(buf);
        self.size += buf.len();

        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl<const N: usize> AsRef<[u8]> for FixedBuffer<N> {
    fn as_ref(&self) -> &[u8] {
        &self.data[..self.size]
    }
}
