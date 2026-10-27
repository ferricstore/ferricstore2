//! One rendered owner per actual typed identity and bounded borrowed row views.

use std::mem::size_of;

use wf_domain::history::values::{HistoryValueEntry, HistoryValueReference};
use wf_domain::workflow::active::{ActiveValueKind, ActiveValueRef};
use wf_ports::blob::{
    OriginalValueBuildError, OriginalValuePartition, OriginalValueRefKind,
    WorkflowValueBindingKnowledge, render_original_value_ref,
};
use wf_ports::state::{
    StateTransactionError as Error, StoredWorkflowHistoryRow, WorkflowHistoryPage,
};

use super::budget::{self, ValueCharge, product, total};

pub(super) struct HistoryReferences<'page> {
    unique: Vec<(&'page ActiveValueRef, Vec<u8>)>,
    indices: Vec<[Option<usize>; 3]>,
    pub(super) live: usize,
}

impl<'page> HistoryReferences<'page> {
    pub(super) fn capture(
        page: &'page WorkflowHistoryPage,
        charge: &mut ValueCharge,
        held: usize,
    ) -> Result<Self, Error> {
        let count = total(page.entries.len(), [usize::from(page.lookahead.is_some())])?;
        charge.work(product(count, 512)?)?;
        let mut nonempty = 0;
        for row in page.entries.iter().chain(page.lookahead.iter()) {
            nonempty = total(nonempty, [fields(row)?.iter().flatten().count()])?;
        }
        let spine = product(count, size_of::<[Option<usize>; 3]>())?;
        let unique_bytes = product(nonempty, size_of::<(&ActiveValueRef, Vec<u8>)>())?;
        let indices = budget::reserve(charge, count, held)?;
        let unique = budget::reserve(charge, nonempty, total(held, [spine])?)?;
        let mut captured = Self {
            unique,
            indices,
            live: total(spine, [unique_bytes])?,
        };
        charge.work(product(count, 512)?)?;
        for row in page.entries.iter().chain(page.lookahead.iter()) {
            let indices = captured.capture_row(row, charge, held)?;
            captured.indices.push(indices);
        }
        Ok(captured)
    }

    fn capture_row(
        &mut self,
        row: &'page StoredWorkflowHistoryRow,
        charge: &mut ValueCharge,
        held: usize,
    ) -> Result<[Option<usize>; 3], Error> {
        let mut indices = [None; 3];
        for (index, source) in fields(row)?.into_iter().enumerate() {
            if let Some(source) = source {
                indices[index] = Some(self.remember(source, charge, held)?);
            }
        }
        Ok(indices)
    }

    fn remember(
        &mut self,
        source: &'page ActiveValueRef,
        charge: &mut ValueCharge,
        held: usize,
    ) -> Result<usize, Error> {
        let length = identity_bytes(source)?;
        charge.work(product(self.unique.len(), 128)?)?;
        for (index, (prior, _)) in self.unique.iter().enumerate() {
            charge.compare(length, identity_bytes(prior)?)?;
            if source == *prior {
                return Ok(index);
            }
        }
        let bound = total(length, [80])?;
        charge.work(total(128, [product(length, 16)?])?)?;
        budget::temporary(charge, held, total(self.live, [bound])?)?;
        let rendered = render(source, bound)?;
        self.live = total(self.live, [rendered.capacity()])?;
        budget::temporary(charge, held, self.live)?;
        let index = self.unique.len();
        self.unique.push((source, rendered));
        Ok(index)
    }

    pub(super) fn views<'a>(
        &'a self,
        page: &'a WorkflowHistoryPage,
        charge: &mut ValueCharge,
        held: usize,
    ) -> Result<Vec<HistoryValueEntry<'a, StoredWorkflowHistoryRow>>, Error> {
        let count = total(page.entries.len(), [usize::from(page.lookahead.is_some())])?;
        if count != self.indices.len() {
            return Err(Error::Corrupt);
        }
        charge.work(product(count, 512)?)?;
        let mut views = budget::reserve(charge, count, total(held, [self.live])?)?;
        for (source, indices) in page
            .entries
            .iter()
            .chain(page.lookahead.iter())
            .zip(&self.indices)
        {
            let references = self.view_indices(indices)?;
            views.push(HistoryValueEntry { source, references });
        }
        Ok(views)
    }

    fn view_indices(
        &self,
        indices: &[Option<usize>; 3],
    ) -> Result<[HistoryValueReference<'_>; 3], Error> {
        let mut references = [HistoryValueReference::Nil; 3];
        for (target, index) in references.iter_mut().zip(indices) {
            if let Some(index) = index {
                let bytes = &self.unique.get(*index).ok_or(Error::Corrupt)?.1;
                *target = HistoryValueReference::Binary(bytes);
            }
        }
        Ok(references)
    }
}

fn fields(row: &StoredWorkflowHistoryRow) -> Result<[Option<&ActiveValueRef>; 3], Error> {
    match &row.values {
        WorkflowValueBindingKnowledge::Known(Some(binding)) => Ok([
            binding.refs.payload.as_ref(),
            binding.refs.result.as_ref(),
            binding.refs.error.as_ref(),
        ]),
        WorkflowValueBindingKnowledge::LegacyUnavailable => Err(Error::UnsupportedLayout),
        WorkflowValueBindingKnowledge::Known(None) => Err(Error::Corrupt),
    }
}

fn identity_bytes(reference: &ActiveValueRef) -> Result<usize, Error> {
    match reference {
        ActiveValueRef::Versioned { partition, id, .. } => {
            total(partition.as_bytes().len(), [id.as_bytes().len()])
        }
        ActiveValueRef::Opaque(_) => Err(Error::UnsupportedLayout),
    }
}

fn render(reference: &ActiveValueRef, bound: usize) -> Result<Vec<u8>, Error> {
    let ActiveValueRef::Versioned {
        partition,
        id,
        kind,
        version,
    } = reference
    else {
        return Err(Error::UnsupportedLayout);
    };
    let kind = match kind {
        ActiveValueKind::Payload => OriginalValueRefKind::Payload,
        ActiveValueKind::Result => OriginalValueRefKind::Result,
        ActiveValueKind::Error => OriginalValueRefKind::Error,
    };
    render_original_value_ref(
        id.as_bytes(),
        kind,
        *version,
        OriginalValuePartition::Explicit(partition.as_bytes()),
        bound,
    )
    .map_err(|error| match error {
        OriginalValueBuildError::InvalidInput => Error::Corrupt,
        OriginalValueBuildError::Limit => Error::Limit,
        OriginalValueBuildError::ReserveFailed(_) => Error::Allocation,
    })
}
