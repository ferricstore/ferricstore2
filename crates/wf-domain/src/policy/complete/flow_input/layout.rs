//! Three bounded metadata owners; borrowed payloads survive until final emission.

use super::{Error, PolicySchemaCharge, PolicySchemaError, charge_schema};

#[derive(Clone, Copy, Debug)]
pub(super) enum FlowPolicyCell<'input> {
    Atom(&'static str),
    Binary(&'input [u8]),
    Integer(u128),
    BigInteger(&'input [u8]),
    // Constructed internally only from a retained, validated original term span.
    RawTerm(&'input [u8]),
    List { start: usize, len: usize },
    Map { start: usize, len: usize },
    Tuple { start: usize, len: usize },
}

#[derive(Clone, Copy, Debug)]
pub(super) struct FlowPolicyWorkSlot {
    pub(super) hash: u64,
    pub(super) index: usize,
    pub(super) cell: usize,
    pub(super) order: u64,
}

pub(in crate::policy::complete) struct FlowPolicyLayout<'input> {
    pub(super) cells: Vec<FlowPolicyCell<'input>>,
    pub(super) edges: Vec<usize>,
    pub(super) work: Vec<FlowPolicyWorkSlot>,
    pub(super) root: Option<usize>,
}

impl<'input> FlowPolicyLayout<'input> {
    pub(super) fn new(
        count: usize,
        depth: usize,
        charge: &mut PolicySchemaCharge,
    ) -> Result<Self, Error> {
        Self::new_with_base(count, depth, 0, charge)
    }

    pub(super) fn new_with_base(
        count: usize,
        depth: usize,
        live: usize,
        charge: &mut PolicySchemaCharge,
    ) -> Result<Self, Error> {
        let length = count
            .checked_mul(8)
            .and_then(|n| n.checked_add(32))
            .ok_or_else(overflow)?;
        let cells = length.checked_mul(64).ok_or_else(overflow)?;
        let edges = length.checked_mul(16).ok_or_else(overflow)?;
        let work = count.checked_mul(32).ok_or_else(overflow)?;
        let peak = cells
            .checked_add(edges)
            .and_then(|n| n.checked_add(work))
            .and_then(|n| n.checked_add(depth.checked_mul(64)?))
            .and_then(|n| n.checked_add(192))
            .and_then(|n| n.checked_add(live))
            .ok_or_else(overflow)?;
        charge_schema(charge, 0, u64::try_from(peak).map_err(|_| overflow())?)
            .map_err(Error::Schema)?;
        let mut layout = Self {
            cells: Vec::new(),
            edges: Vec::new(),
            work: Vec::new(),
            root: None,
        };
        reserve(cells, charge)?;
        layout
            .cells
            .try_reserve_exact(length)
            .map_err(|_| Error::AllocationFailure)?;
        reserve(edges, charge)?;
        layout
            .edges
            .try_reserve_exact(length.checked_mul(2).ok_or_else(overflow)?)
            .map_err(|_| Error::AllocationFailure)?;
        reserve(work, charge)?;
        layout
            .work
            .try_reserve_exact(count)
            .map_err(|_| Error::AllocationFailure)?;
        Ok(layout)
    }

    pub(super) fn push_cell(
        &mut self,
        cell: FlowPolicyCell<'input>,
        charge: &mut PolicySchemaCharge,
    ) -> Result<usize, Error> {
        charge_schema(charge, 128, 0).map_err(Error::Schema)?;
        if self.cells.len() == self.cells.capacity() {
            return Err(Error::ItemLimit);
        }
        let index = self.cells.len();
        self.cells.push(cell);
        Ok(index)
    }

    pub(super) fn push_edge(
        &mut self,
        cell: usize,
        charge: &mut PolicySchemaCharge,
    ) -> Result<(), Error> {
        charge_schema(charge, 128, 0).map_err(Error::Schema)?;
        if self.edges.len() == self.edges.capacity() || cell >= self.cells.len() {
            return Err(Error::ItemLimit);
        }
        self.edges.push(cell);
        Ok(())
    }

    pub(super) fn push_work(
        &mut self,
        slot: FlowPolicyWorkSlot,
        charge: &mut PolicySchemaCharge,
    ) -> Result<usize, Error> {
        charge_schema(charge, 128, 0).map_err(Error::Schema)?;
        if self.work.len() == self.work.capacity() {
            return Err(Error::ItemLimit);
        }
        let index = self.work.len();
        self.work.push(slot);
        Ok(index)
    }

    pub(super) fn map(
        &mut self,
        names: &[&'static str],
        values: &[usize],
        charge: &mut PolicySchemaCharge,
    ) -> Result<usize, Error> {
        // Fixed schema names are static; compact IDs are the only staging array.
        if names.len() != values.len() {
            return Err(Error::Malformed);
        }
        let start = self.edges.len();
        let mut len = 0;
        for (&name, &value) in names.iter().zip(values) {
            if value != usize::MAX {
                let key = self.push_cell(FlowPolicyCell::Atom(name), charge)?;
                self.push_edge(key, charge)?;
                self.push_edge(value, charge)?;
                len += 1;
            }
        }
        self.push_cell(FlowPolicyCell::Map { start, len }, charge)
    }
}

fn reserve(bytes: usize, charge: &mut PolicySchemaCharge) -> Result<(), Error> {
    let fee = u64::try_from(bytes)
        .ok()
        .and_then(|n| n.checked_add(128))
        .ok_or_else(overflow)?;
    charge_schema(charge, fee, 0).map_err(Error::Schema)
}

fn overflow() -> Error {
    Error::Schema(PolicySchemaError::LengthOverflow)
}
