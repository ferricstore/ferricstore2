//! Bounded source v1 traversal hints; no persisted progress or read authority.
use crate::workflow::MAX_EXACT_MILLIS as MAX;
use std::fmt;
/// Cursor grammar or prospective ownership refusal.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReleaseCursorError {
    /// Original cursor grammar, range or unique-shard validation failed.
    Invalid,
    /// Prospective byte/owner admission failed.
    Limit,
    /// A bounded host allocation failed.
    Allocation,
}
impl fmt::Display for ReleaseCursorError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Invalid => "ERR invalid flow governance release outbox cursor",
            Self::Limit => "release cursor resource limit",
            Self::Allocation => "release cursor allocation failed",
        })
    }
}
impl std::error::Error for ReleaseCursorError {}
/// Configured shard order and sorted inclusive next-sequence hints.
pub struct ReleaseReconcileCursor {
    count: u64,
    next: u64,
    positions: Vec<(u64, u64)>,
    maximum_owned: usize,
}
impl ReleaseReconcileCursor {
    /// Validates the complete original v1 cursor within finite resource bounds.
    /// # Errors
    /// Refuses invalid shard/sequence grammar, duplicates, byte/owner limits or allocation.
    pub fn decode(
        encoded: Option<&[u8]>,
        count: u64,
        maximum_owned: usize,
    ) -> Result<Self, ReleaseCursorError> {
        use ReleaseCursorError as E;
        if count == 0 || count > MAX {
            return Err(E::Invalid);
        }
        if maximum_owned < 128 {
            return Err(E::Limit);
        }
        let mut result = Self {
            count,
            next: 0,
            positions: Vec::new(),
            maximum_owned,
        };
        let Some(encoded) = encoded else {
            return Ok(result);
        };
        let bound = count
            .checked_mul(96)
            .and_then(|n| n.checked_add(128))
            .ok_or(E::Limit)?;
        if encoded.len() as u128 > u128::from(bound) {
            return Err(E::Invalid);
        }
        if encoded.len() > maximum_owned {
            return Err(E::Limit);
        }
        let body = encoded.strip_prefix(b"v1:").ok_or(E::Invalid)?;
        let split = body.iter().position(|b| *b == b'|').ok_or(E::Invalid)?;
        result.next = number(&body[..split])?;
        if result.next >= count {
            return Err(E::Invalid);
        }
        let positions = &body[split + 1..];
        if positions.is_empty() {
            return Ok(result);
        }
        let length = positions.split(|b| *b == b',').count();
        if length as u128 > u128::from(count) {
            return Err(E::Invalid);
        }
        let owned = length
            .checked_mul(size_of::<(u64, u64)>())
            .and_then(|n| n.checked_add(128))
            .ok_or(E::Limit)?;
        if owned > maximum_owned {
            return Err(E::Limit);
        }
        result
            .positions
            .try_reserve_exact(length)
            .map_err(|_| E::Allocation)?;
        for entry in positions.split(|b| *b == b',') {
            let split = entry.iter().position(|b| *b == b'=').ok_or(E::Invalid)?;
            let shard = number(&entry[..split])?;
            let sequence = number(&entry[split + 1..])?;
            if shard >= count || sequence == 0 || sequence > MAX {
                return Err(E::Invalid);
            }
            result.positions.push((shard, sequence));
        }
        result.positions.sort_unstable_by_key(|p| p.0);
        if result.positions.windows(2).any(|p| p[0].0 == p[1].0) {
            return Err(E::Invalid);
        }
        Ok(result)
    }
    /// Next configured shard; it is a hint rather than a routing capability.
    #[must_use]
    pub const fn next_shard(&self) -> u64 {
        self.next
    }
    /// Original inclusive next sequence for one configured shard.
    #[must_use]
    pub fn position(&self, shard: u64) -> Option<u64> {
        self.positions
            .binary_search_by_key(&shard, |p| p.0)
            .ok()
            .map(|at| self.positions[at].1)
    }
    /// Rotates after a visited shard, replacing or removing only its own hint.
    /// # Errors
    /// Refuses invalid indices/sequences or fallible descriptor allocation.
    pub fn advance(&mut self, shard: u64, position: Option<u64>) -> Result<(), ReleaseCursorError> {
        use ReleaseCursorError as E;
        if shard >= self.count || position.is_some_and(|p| p == 0 || p > MAX) {
            return Err(E::Invalid);
        }
        match (
            self.positions.binary_search_by_key(&shard, |p| p.0),
            position,
        ) {
            (Ok(at), Some(sequence)) => self.positions[at].1 = sequence,
            (Ok(at), None) => {
                self.positions.remove(at);
            }
            (Err(at), Some(sequence)) => {
                if (self.positions.len() + 1)
                    .checked_mul(size_of::<(u64, u64)>())
                    .and_then(|n| n.checked_add(128))
                    .is_none_or(|n| n > self.maximum_owned)
                {
                    return Err(E::Limit);
                }
                self.positions
                    .try_reserve_exact(1)
                    .map_err(|_| E::Allocation)?;
                self.positions.insert(at, (shard, sequence));
            }
            (Err(_), None) => (),
        }
        self.next = (shard + 1) % self.count;
        Ok(())
    }
    /// Emits the single source format with positions in numeric shard order.
    /// # Errors
    /// Refuses prospective byte ownership overflow/limit or allocation.
    pub fn encode(&self, maximum_owned: usize) -> Result<Vec<u8>, ReleaseCursorError> {
        use ReleaseCursorError as E;
        let length = self
            .positions
            .iter()
            .try_fold(4 + digits(self.next), |n, (s, p)| {
                n.checked_add(digits(*s) + digits(*p) + 2)
            })
            .and_then(|n| n.checked_sub(usize::from(!self.positions.is_empty())))
            .ok_or(E::Limit)?;
        if length.checked_add(128).is_none_or(|n| n > maximum_owned) {
            return Err(E::Limit);
        }
        let mut out = Vec::new();
        out.try_reserve_exact(length).map_err(|_| E::Allocation)?;
        out.extend_from_slice(b"v1:");
        write(&mut out, self.next);
        out.push(b'|');
        for (at, (shard, sequence)) in self.positions.iter().enumerate() {
            if at != 0 {
                out.push(b',');
            }
            write(&mut out, *shard);
            out.push(b'=');
            write(&mut out, *sequence);
        }
        Ok(out)
    }
}
fn number(bytes: &[u8]) -> Result<u64, ReleaseCursorError> {
    let (negative, bytes) = match bytes.first() {
        Some(b'-') => (true, &bytes[1..]),
        Some(b'+') => (false, &bytes[1..]),
        _ => (false, bytes),
    };
    if bytes.is_empty() {
        return Err(ReleaseCursorError::Invalid);
    }
    let value = bytes
        .iter()
        .try_fold(0_u64, |n, b| {
            if !b.is_ascii_digit() {
                return None;
            }
            n.checked_mul(10)?.checked_add(u64::from(*b - b'0'))
        })
        .ok_or(ReleaseCursorError::Invalid)?;
    if negative && value != 0 {
        return Err(ReleaseCursorError::Invalid);
    }
    Ok(value)
}
fn digits(mut n: u64) -> usize {
    let mut count = 1;
    while n >= 10 {
        count += 1;
        n /= 10;
    }
    count
}
fn write(out: &mut Vec<u8>, mut n: u64) {
    let mut digits = [0_u8; 20];
    let mut start = 20;
    loop {
        start -= 1;
        digits[start] = b'0' + (n % 10).to_le_bytes()[0];
        n /= 10;
        if n == 0 {
            break;
        }
    }
    out.extend_from_slice(&digits[start..]);
}
