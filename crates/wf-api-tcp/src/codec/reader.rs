//! Bounded big-endian reads from untrusted bytes.

/// The input ended before a complete field.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Truncated;

/// A cursor that never reads past its input; every read returns `None`
/// instead of panicking when the input is too short.
pub(crate) struct Reader<'a> {
    rest: &'a [u8],
}

impl<'a> Reader<'a> {
    pub(crate) const fn new(bytes: &'a [u8]) -> Self {
        Self { rest: bytes }
    }

    pub(crate) const fn is_empty(&self) -> bool {
        self.rest.is_empty()
    }

    pub(crate) fn take(&mut self, length: usize) -> Option<&'a [u8]> {
        let (head, tail) = self.rest.split_at_checked(length)?;
        self.rest = tail;
        Some(head)
    }

    fn array<const N: usize>(&mut self) -> Option<[u8; N]> {
        self.take(N)?.try_into().ok()
    }

    pub(crate) fn u8(&mut self) -> Option<u8> {
        self.array::<1>().map(|[byte]| byte)
    }

    pub(crate) fn u32(&mut self) -> Option<u32> {
        self.array().map(u32::from_be_bytes)
    }

    pub(crate) fn u64(&mut self) -> Option<u64> {
        self.array().map(u64::from_be_bytes)
    }

    pub(crate) fn i64(&mut self) -> Option<i64> {
        self.array().map(i64::from_be_bytes)
    }

    /// A `u32` item count whose items, at `min_item_bytes` each, fit in the
    /// remaining input. A count that cannot fit is refused before allocation.
    pub(crate) fn count(&mut self, min_item_bytes: usize) -> Option<usize> {
        let count = usize::try_from(self.u32()?).ok()?;
        self.fits(count, min_item_bytes).then_some(count)
    }

    /// Whether `count` items of at least `min_item_bytes` each can remain.
    pub(crate) fn fits(&self, count: usize, min_item_bytes: usize) -> bool {
        count
            .checked_mul(min_item_bytes)
            .is_some_and(|needed| needed <= self.rest.len())
    }

    /// A binary with a `u32` length prefix.
    pub(crate) fn bin(&mut self) -> Option<&'a [u8]> {
        let length = usize::try_from(self.u32()?).ok()?;
        self.take(length)
    }

    /// A compact optional binary: the length `u32::MAX` means absent.
    pub(crate) fn optional_bin(&mut self) -> Result<Option<&'a [u8]>, Truncated> {
        match self.u32().ok_or(Truncated)? {
            u32::MAX => Ok(None),
            length => usize::try_from(length)
                .ok()
                .and_then(|length| self.take(length))
                .map(Some)
                .ok_or(Truncated),
        }
    }
}
