//! Private bounded comparison preparation, distinct from persisted/wire codecs.

mod claim;
mod policy;
pub(crate) use claim::prepare_claim_due;
#[cfg(test)]
mod tests;

use sha2::{Digest, Sha256};
use wf_domain::policy::installation::InstallCommand;
use wf_domain::workflow::request::ClientIntent;
use wf_types::identity::TenantId;

use crate::commands::{IntentInput, IntentLimits, IntentValue};
use crate::error::PreparationError;

const PREFIX: &[u8] = b"WF-INTENT\0V1\0";
const HEADER_BYTES: usize = 9;
const COUNT_BYTES: usize = 8;

enum Segment<'a> {
    Header([u8; HEADER_BYTES]),
    Number([u8; COUNT_BYTES]),
    Borrowed(&'a [u8]),
}

struct CanonicalPlan<'a> {
    segments: Vec<Segment<'a>>,
    bytes: usize,
    values: usize,
    limits: IntentLimits,
}

impl<'a> CanonicalPlan<'a> {
    fn write(&self, mut accept: impl FnMut(&[u8])) {
        for segment in &self.segments {
            match segment {
                Segment::Header(bytes) => accept(bytes),
                Segment::Number(bytes) => accept(bytes),
                Segment::Borrowed(bytes) => accept(bytes),
            }
        }
    }

    fn check_bytes(&self, additional: usize) -> Result<usize, PreparationError> {
        let total = self
            .bytes
            .checked_add(additional)
            .ok_or(PreparationError::LengthOverflow)?;
        if total > self.limits.bytes {
            Err(PreparationError::ByteLimit)
        } else {
            Ok(total)
        }
    }

    fn push(&mut self, segment: Segment<'a>, bytes: usize) -> Result<(), PreparationError> {
        let total = self.check_bytes(bytes)?;
        self.segments
            .try_reserve(1)
            .map_err(|_| PreparationError::Allocation)?;
        self.segments.push(segment);
        self.bytes = total;
        Ok(())
    }

    fn children(&self, count: usize, depth: usize) -> Result<(), PreparationError> {
        let values = self
            .values
            .checked_add(count)
            .ok_or(PreparationError::LengthOverflow)?;
        if values > self.limits.values {
            Err(PreparationError::ValueLimit)
        } else if count != 0 && depth >= self.limits.depth {
            Err(PreparationError::DepthLimit)
        } else {
            Ok(())
        }
    }

    fn map(
        &mut self,
        entries: &[(&'a str, IntentValue<'a>)],
        depth: usize,
    ) -> Result<(), PreparationError> {
        let children = entries
            .len()
            .checked_mul(2)
            .ok_or(PreparationError::LengthOverflow)?;
        self.children(children, depth)?;
        let mut minimum = children
            .checked_mul(HEADER_BYTES)
            .and_then(|bytes| bytes.checked_add(COUNT_BYTES))
            .ok_or(PreparationError::LengthOverflow)?;
        self.check_bytes(minimum)?;
        // Counts bound this scan; key bytes are capped before allocating/sorting.
        for (key, _) in entries {
            minimum = minimum
                .checked_add(key.len())
                .ok_or(PreparationError::LengthOverflow)?;
            self.check_bytes(minimum)?;
        }
        let mut ordered: Vec<&(&str, IntentValue<'a>)> = Vec::new();
        ordered
            .try_reserve_exact(entries.len())
            .map_err(|_| PreparationError::Allocation)?;
        ordered.extend(entries.iter());
        ordered.sort_unstable_by(|left, right| left.0.as_bytes().cmp(right.0.as_bytes()));
        if ordered.windows(2).any(|pair| pair[0].0 == pair[1].0) {
            return Err(PreparationError::DuplicateKey);
        }
        self.push(Segment::Number(format_length(entries.len())?), COUNT_BYTES)?;
        for (key, value) in ordered {
            self.value(IntentValue::String(key), depth + 1)?;
            self.value(*value, depth + 1)?;
        }
        Ok(())
    }

    // Operation registries supply unique static keys in UTF-8 byte order.
    // The bounded scalar presence scan precedes metadata for map children.
    fn explicit_map(
        &mut self,
        fields: &[(&'static str, bool)],
        depth: usize,
        write: impl FnOnce(&mut Self, usize) -> Result<(), PreparationError>,
    ) -> Result<(), PreparationError> {
        let (header, before) = self.begin(depth)?;
        let count = fields.iter().filter(|(_, present)| *present).count();
        let children = count
            .checked_mul(2)
            .ok_or(PreparationError::LengthOverflow)?;
        self.children(children, depth)?;
        let mut minimum = children
            .checked_mul(HEADER_BYTES)
            .and_then(|bytes| bytes.checked_add(COUNT_BYTES))
            .ok_or(PreparationError::LengthOverflow)?;
        for (key, present) in fields {
            if *present {
                minimum = minimum
                    .checked_add(key.len())
                    .ok_or(PreparationError::LengthOverflow)?;
            }
        }
        self.check_bytes(minimum)?;
        self.push(Segment::Number(format_length(count)?), COUNT_BYTES)?;
        write(self, depth + 1)?;
        self.finish(header, before, 8)
    }

    fn entry(
        &mut self,
        key: &'static str,
        value: IntentValue<'a>,
        depth: usize,
    ) -> Result<(), PreparationError> {
        self.value(IntentValue::String(key), depth)?;
        self.value(value, depth)
    }

    fn begin(&mut self, depth: usize) -> Result<(usize, usize), PreparationError> {
        self.values = self
            .values
            .checked_add(1)
            .ok_or(PreparationError::LengthOverflow)?;
        if self.values > self.limits.values {
            return Err(PreparationError::ValueLimit);
        }
        if depth > self.limits.depth {
            return Err(PreparationError::DepthLimit);
        }
        let header_index = self.segments.len();
        let before = self.bytes;
        self.push(Segment::Header([0; HEADER_BYTES]), HEADER_BYTES)?;
        Ok((header_index, before))
    }

    fn value(&mut self, value: IntentValue<'a>, depth: usize) -> Result<(), PreparationError> {
        let (header_index, before) = self.begin(depth)?;
        let tag = match value {
            IntentValue::Null => 0,
            IntentValue::False => 1,
            IntentValue::True => 2,
            IntentValue::Unsigned(number) => {
                self.push(Segment::Number(number.to_be_bytes()), COUNT_BYTES)?;
                3
            }
            IntentValue::Signed(number) => {
                self.push(Segment::Number(number.to_be_bytes()), COUNT_BYTES)?;
                4
            }
            IntentValue::String(text) => {
                self.push(Segment::Borrowed(text.as_bytes()), text.len())?;
                5
            }
            IntentValue::Bytes(bytes) => {
                self.push(Segment::Borrowed(bytes), bytes.len())?;
                6
            }
            IntentValue::List(items) => {
                self.children(items.len(), depth)?;
                let minimum = items
                    .len()
                    .checked_mul(HEADER_BYTES)
                    .and_then(|bytes| bytes.checked_add(COUNT_BYTES))
                    .ok_or(PreparationError::LengthOverflow)?;
                self.check_bytes(minimum)?;
                self.push(Segment::Number(format_length(items.len())?), COUNT_BYTES)?;
                for item in items {
                    self.value(*item, depth + 1)?;
                }
                7
            }
            IntentValue::Map(entries) => {
                self.map(entries, depth)?;
                8
            }
            IntentValue::Float(number) => {
                if !number.is_finite() {
                    return Err(PreparationError::NonFiniteNumber);
                }
                self.push(Segment::Number(number.to_bits().to_be_bytes()), COUNT_BYTES)?;
                9
            }
        };
        self.finish(header_index, before, tag)
    }

    fn finish(
        &mut self,
        header_index: usize,
        before: usize,
        tag: u8,
    ) -> Result<(), PreparationError> {
        let length = self
            .bytes
            .checked_sub(before)
            .and_then(|bytes| bytes.checked_sub(HEADER_BYTES))
            .ok_or(PreparationError::LengthOverflow)?;
        let mut header = [0; HEADER_BYTES];
        header[0] = tag;
        header[1..].copy_from_slice(&format_length(length)?);
        let target = self
            .segments
            .get_mut(header_index)
            .ok_or(PreparationError::LengthOverflow)?;
        *target = Segment::Header(header);
        Ok(())
    }
}

fn format_length(value: usize) -> Result<[u8; COUNT_BYTES], PreparationError> {
    Ok(u64::try_from(value)
        .map_err(|_| PreparationError::LengthOverflow)?
        .to_be_bytes())
}

fn canonical(
    input: IntentInput<'_>,
    limits: IntentLimits,
) -> Result<CanonicalPlan<'_>, PreparationError> {
    if input.operation.is_empty() {
        return Err(PreparationError::InvalidOperation);
    }
    let root = [
        ("normalization_version", IntentValue::String("1")),
        ("schema_version", IntentValue::String("1")),
        ("tenant_id", IntentValue::Bytes(input.tenant.as_bytes())),
        ("operation", IntentValue::String(input.operation)),
        ("args", IntentValue::Map(input.arguments)),
    ];
    let mut plan = CanonicalPlan {
        segments: Vec::new(),
        bytes: 0,
        values: 0,
        limits,
    };
    plan.push(Segment::Borrowed(PREFIX), PREFIX.len())?;
    let (header, before) = plan.begin(1)?;
    plan.map(&root, 1)?;
    plan.finish(header, before, 8)?;
    Ok(plan)
}

pub(crate) fn prepare(
    input: IntentInput<'_>,
    limits: IntentLimits,
) -> Result<ClientIntent, PreparationError> {
    let plan = canonical(input, limits)?;
    digest(&plan)
}

pub(crate) fn prepare_policy_set(
    tenant: &TenantId,
    command: &InstallCommand,
    limits: IntentLimits,
) -> Result<ClientIntent, PreparationError> {
    let plan = policy::canonical(tenant, command, limits)?;
    digest(&plan)
}

fn digest(plan: &CanonicalPlan<'_>) -> Result<ClientIntent, PreparationError> {
    let mut hash = Sha256::new();
    plan.write(|bytes| hash.update(bytes));
    ClientIntent::new(1, hash.finalize().into()).map_err(|_| PreparationError::UnsupportedVersion)
}
