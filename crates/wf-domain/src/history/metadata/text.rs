//! Bounded formatter destination, without implicit String growth.

use std::fmt;

pub(super) struct ScalarText {
    bytes: [u8; 32],
    length: usize,
}

impl ScalarText {
    pub(super) const fn new() -> Self {
        Self {
            bytes: [0; 32],
            length: 0,
        }
    }

    pub(super) fn as_bytes(&self) -> &[u8] {
        &self.bytes[..self.length]
    }

    pub(super) fn push(&mut self, bytes: &[u8]) -> fmt::Result {
        let end = self.length.checked_add(bytes.len()).ok_or(fmt::Error)?;
        self.bytes
            .get_mut(self.length..end)
            .ok_or(fmt::Error)?
            .copy_from_slice(bytes);
        self.length = end;
        Ok(())
    }
}

impl fmt::Write for ScalarText {
    fn write_str(&mut self, text: &str) -> fmt::Result {
        self.push(text.as_bytes())
    }
}
