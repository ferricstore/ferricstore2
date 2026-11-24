//! Appending to one cold file (ADR-016).

use std::fs::File;
use std::io::{self, Write};
use std::sync::Arc;

use super::{ColdStore, Location};

/// Appends images to one cold file.
pub struct ColdWriter {
    pub(super) store: Arc<ColdStore>,
    pub(super) handle: File,
    pub(super) file: u64,
    pub(super) length: u64,
    /// Appended images not yet written to the file.
    pub(super) buffer: Vec<u8>,
    pub(super) dirty: bool,
    /// Whether the directory entry of the file is durable.
    pub(super) named: bool,
}

impl ColdWriter {
    /// The file this writer appends to.
    #[must_use]
    pub const fn file(&self) -> u64 {
        self.file
    }

    /// Bytes appended so far, written or not.
    #[must_use]
    pub fn length(&self) -> u64 {
        self.length
            .saturating_add(u64::try_from(self.buffer.len()).unwrap_or(u64::MAX))
    }

    /// Appends one image to the writer's buffer; readable after the next
    /// [`Self::flush`], durable after the next [`Self::sync`].
    ///
    /// # Errors
    /// The image is larger than a record can hold.
    pub fn append(&mut self, image: &[u8]) -> io::Result<Location> {
        let length = u32::try_from(image.len())
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "cold image too large"))?;
        let location = Location {
            file: self.file,
            offset: self.length(),
            length,
        };
        self.buffer.extend_from_slice(&length.to_be_bytes());
        self.buffer
            .extend_from_slice(&crc32fast::hash(image).to_be_bytes());
        self.buffer.extend_from_slice(image);
        Ok(location)
    }

    /// Writes the buffered images to the file with one call.
    ///
    /// # Errors
    /// The write failed; the buffered images are kept for the next try.
    pub fn flush(&mut self) -> io::Result<()> {
        if self.buffer.is_empty() {
            return Ok(());
        }
        self.handle.write_all(&self.buffer)?;
        self.length = self.length();
        self.buffer.clear();
        self.dirty = true;
        Ok(())
    }

    /// Makes every appended image durable.
    ///
    /// # Errors
    /// The sync failed; the images may or may not survive a crash.
    pub fn sync(&mut self) -> io::Result<()> {
        self.flush()?;
        if self.dirty {
            self.handle.sync_data()?;
            self.dirty = false;
        }
        if !self.named {
            File::open(&self.store.directory)?.sync_all()?;
            self.named = true;
        }
        Ok(())
    }

    /// Syncs and closes the file for appends; it is deleted once no stub
    /// points into it.
    ///
    /// # Errors
    /// The sync or a deletion failed.
    pub fn seal(mut self) -> io::Result<()> {
        self.sync()?;
        self.store.seal(self.file)
    }
}
