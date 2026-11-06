pub struct DynamicBuffer {
    data: Vec<u8>,
}

impl DynamicBuffer {
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            data: Vec::with_capacity(capacity),
        }
    }

    // pub fn len(&self) -> usize {
    //     self.data.len()
    // }

    // pub fn clear(&mut self) {
    //     self.data.clear();
    // }
}

impl std::io::Write for DynamicBuffer {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.data.extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl AsRef<[u8]> for DynamicBuffer {
    fn as_ref(&self) -> &[u8] {
        &self.data
    }
}
