//! Immutable public preparation owners and bounded converted-term metadata.

use super::{Error, PolicySchemaCharge, Raw, debit, overflow};

/// Public controls and a converted raw patch, without dispatch or origin proof.
pub struct OriginalPolicySetRequest<'input> {
    pub(super) policy_key: Vec<u8>,
    pub(super) expected_generation: Option<u64>,
    pub(super) replace: bool,
    pub(super) response_state: Option<&'input [u8]>,
    pub(super) patch: PolicySetPatchArena<'input>,
    pub(super) reservation: (usize, usize),
}

impl OriginalPolicySetRequest<'_> {
    pub(in crate::policy::complete) const fn reservation_count(&self) -> usize {
        self.reservation.0
    }

    pub(in crate::policy::complete) const fn reservation_depth(&self) -> usize {
        self.reservation.1
    }

    /// Borrows the independently owned prefixed routing key.
    #[must_use]
    pub fn policy_key(&self) -> &[u8] {
        &self.policy_key
    }

    /// Borrows the original opaque type from that same key owner.
    #[must_use]
    pub fn workflow_type(&self) -> &[u8] {
        &self.policy_key[13..]
    }

    /// Reads the first public expected-generation control.
    #[must_use]
    pub const fn expected_generation(&self) -> Option<u64> {
        self.expected_generation
    }

    /// Reads the original mode; false still requires a future merge parent.
    #[must_use]
    pub const fn replace(&self) -> bool {
        self.replace
    }

    /// Borrows the first original reply-state selector, including empty bytes.
    #[must_use]
    pub const fn response_state(&self) -> Option<&[u8]> {
        self.response_state
    }

    /// Borrows mathematical patch data, without VM iteration-order authority.
    #[must_use]
    pub fn patch(&self) -> OriginalPolicySetPatchView<'_> {
        self.patch.view(self.patch.root)
    }
}

/// A checked converted-term view; it grants no payment or captured-order proof.
#[derive(Clone, Copy)]
pub struct OriginalPolicySetPatchView<'view> {
    cells: &'view [PolicySetPatchCell<'view>],
    edges: &'view [usize],
    cell: usize,
}

impl<'view> OriginalPolicySetPatchView<'view> {
    /// Reads an unchanged scalar, tuple or opaque map-key term.
    #[must_use]
    pub fn raw(self) -> Option<Raw<'view>> {
        match self.cells.get(self.cell)? {
            PolicySetPatchCell::Raw(value) => Some(*value),
            _ => None,
        }
    }

    /// Reads a transformed map's pair count.
    #[must_use]
    pub fn map_len(self) -> Option<usize> {
        match self.cells.get(self.cell)? {
            PolicySetPatchCell::Map { pairs, .. } => Some(*pairs),
            _ => None,
        }
    }

    /// Reads a transformed pair in the explicitly represented logical order.
    #[must_use]
    pub fn map_entry(self, index: usize) -> Option<(Self, Self)> {
        let PolicySetPatchCell::Map { start, pairs } = self.cells.get(self.cell)? else {
            return None;
        };
        if index >= *pairs {
            return None;
        }
        let offset = start.checked_add(index.checked_mul(2)?)?;
        Some((
            self.child(*self.edges.get(offset)?),
            self.child(*self.edges.get(offset.checked_add(1)?)?),
        ))
    }

    /// Reads a transformed proper list's element count.
    #[must_use]
    pub fn list_len(self) -> Option<usize> {
        match self.cells.get(self.cell)? {
            PolicySetPatchCell::List { len, .. } => Some(*len),
            _ => None,
        }
    }

    /// Reads one transformed proper-list element.
    #[must_use]
    pub fn list_item(self, index: usize) -> Option<Self> {
        let PolicySetPatchCell::List { start, len } = self.cells.get(self.cell)? else {
            return None;
        };
        if index >= *len {
            return None;
        }
        Some(self.child(*self.edges.get(start.checked_add(index)?)?))
    }

    fn child(self, cell: usize) -> Self {
        Self { cell, ..self }
    }
}

/// Exact public validation diagnoses, distinct from allocator/business errors.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OriginalPolicySetInputRejection {
    /// Function-head or complete keyword shape is invalid.
    Options,
    /// The binary workflow type is empty.
    Type,
    /// The derived policy key exceeds65535 bytes.
    KeyBytes,
    /// First reply-state control is not nil or binary.
    State,
    /// First public generation control is outside its source class/range.
    ExpectedGeneration,
    /// First replacement control is not a boolean atom.
    Replace,
    /// Converted raw patch exceeds its Router snapshot metric cap.
    SnapshotBytes,
}

impl OriginalPolicySetInputRejection {
    /// Returns the fixed original caller response without allocation.
    #[must_use]
    pub const fn as_bytes(self) -> &'static [u8] {
        match self {
            Self::Options => b"ERR flow opts must be a keyword list",
            Self::Type => b"ERR flow type must be a non-empty string",
            Self::KeyBytes => b"ERR key too large (max 65535 bytes)",
            Self::State => b"ERR flow state must be a string",
            Self::ExpectedGeneration => {
                b"ERR flow expected_generation must be a non-negative integer"
            }
            Self::Replace => b"ERR flow replace must be boolean",
            Self::SnapshotBytes => b"ERR flow policy snapshot exceeds 262144 bytes",
        }
    }
}

#[derive(Clone, Copy)]
pub(super) enum PolicySetPatchCell<'input> {
    Raw(Raw<'input>),
    Map { start: usize, pairs: usize },
    List { start: usize, len: usize },
}

pub(super) struct PolicySetPatchArena<'input> {
    pub(super) cells: Vec<PolicySetPatchCell<'input>>,
    pub(super) edges: Vec<usize>,
    pub(super) root: usize,
}

#[derive(Clone, Copy)]
pub(super) struct PolicySetPatchSlot {
    pub(super) source: usize,
    pub(super) cell: usize,
    pub(super) hash: u64,
}

impl<'input> PolicySetPatchArena<'input> {
    pub(super) fn new(
        count: usize,
        depth: usize,
        charge: &mut PolicySchemaCharge,
    ) -> Result<(Self, Vec<PolicySetPatchSlot>), Error> {
        let peak = count
            .checked_mul(104)
            .and_then(|n| n.checked_add(depth.checked_mul(128)?))
            .and_then(|n| n.checked_add(256))
            .ok_or_else(overflow)?;
        super::charge_schema(charge, 0, u64::try_from(peak).map_err(|_| overflow())?)
            .map_err(Error::Schema)?;
        let mut arena = Self {
            cells: Vec::new(),
            edges: Vec::new(),
            root: usize::MAX,
        };
        debit(charge, 128 + count.checked_mul(64).ok_or_else(overflow)?)?;
        arena
            .cells
            .try_reserve_exact(count)
            .map_err(|_| Error::AllocationFailure)?;
        debit(charge, 128 + count.checked_mul(8).ok_or_else(overflow)?)?;
        arena
            .edges
            .try_reserve_exact(count)
            .map_err(|_| Error::AllocationFailure)?;
        debit(charge, 128 + count.checked_mul(32).ok_or_else(overflow)?)?;
        let mut work = Vec::new();
        work.try_reserve_exact(count)
            .map_err(|_| Error::AllocationFailure)?;
        Ok((arena, work))
    }

    pub(super) fn cell(
        &mut self,
        value: PolicySetPatchCell<'input>,
        charge: &mut PolicySchemaCharge,
    ) -> Result<usize, Error> {
        debit(charge, 128)?;
        if self.cells.len() == self.cells.capacity() {
            return Err(Error::ItemLimit);
        }
        let id = self.cells.len();
        self.cells.push(value);
        Ok(id)
    }

    pub(super) fn edge(
        &mut self,
        cell: usize,
        charge: &mut PolicySchemaCharge,
    ) -> Result<(), Error> {
        debit(charge, 128)?;
        if self.edges.len() == self.edges.capacity() || cell >= self.cells.len() {
            return Err(Error::Malformed);
        }
        self.edges.push(cell);
        Ok(())
    }

    pub(super) fn view(&self, cell: usize) -> OriginalPolicySetPatchView<'_> {
        OriginalPolicySetPatchView {
            cells: &self.cells,
            edges: &self.edges,
            cell,
        }
    }
}
