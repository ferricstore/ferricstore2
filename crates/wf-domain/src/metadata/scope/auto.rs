//! Exact ordered automatic partition recognition with a fixed decimal buffer.
use super::budget::{Budget, add};
use super::{OriginalScopeError as E, original_physical_scope, split};

pub(super) fn run<'a>(partitions: &[&'a [u8]], budget: &mut Budget) -> Result<&'a [u8], E> {
    if partitions.len() != 256 {
        return Err(E::InvalidSystemMetadata);
    }
    let first = partitions.first().ok_or(E::InvalidSystemMetadata)?;
    let scope = original_physical_scope(first).ok_or(E::InvalidSystemMetadata)?;
    budget.output(scope.len())?;
    for (bucket, physical) in partitions.iter().enumerate() {
        budget.spend(add(scope.len(), 34)?)?;
        let (encoded, logical) = split(physical).ok_or(E::InvalidSystemMetadata)?;
        let mut expected = [0; 17];
        expected
            .get_mut(..14)
            .ok_or(E::Limit)?
            .copy_from_slice(b"__flow_auto__:");
        let mut end = 17;
        let mut value = bucket;
        loop {
            end -= 1;
            *expected.get_mut(end).ok_or(E::Limit)? =
                b'0' + u8::try_from(value % 10).map_err(|_| E::Limit)?;
            value /= 10;
            if value == 0 {
                break;
            }
        }
        let digits = 17 - end;
        expected.copy_within(end..17, 14);
        if encoded != scope || logical != expected.get(..14 + digits).ok_or(E::Limit)? {
            return Err(E::InvalidSystemMetadata);
        }
    }
    Ok(scope)
}
