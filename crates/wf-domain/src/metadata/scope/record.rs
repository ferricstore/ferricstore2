//! Original record overloads; a nil partition requires exactly an empty map.
use super::budget::Budget;
use super::{OriginalMetadataInput as I, OriginalScopeError as E, decode, encode, system};
use std::borrow::Cow;

pub(super) fn run<'a>(
    partition: Option<I<'_, 'a>>,
    metadata: Option<I<'_, '_>>,
    logical: bool,
    budget: &mut Budget,
) -> Result<Option<Cow<'a, [u8]>>, E> {
    budget.spend(1)?;
    let metadata = metadata.unwrap_or(I::Map(&[]));
    let partition = match partition {
        Some(I::Nil | I::Atom("nil")) => {
            return if matches!(metadata, I::Map(rows) if rows.is_empty()) {
                Ok(None)
            } else {
                Err(E::InvalidSystemMetadata)
            };
        }
        Some(I::Binary(partition)) => partition,
        _ => return Err(E::InvalidSystemMetadata),
    };
    let scope = system::run(metadata, budget)?;
    if logical {
        let value = match scope.as_deref() {
            Some(scope) => decode(partition, scope, budget)?,
            None => partition,
        };
        budget.output(value.len())?;
        Ok(Some(Cow::Borrowed(value)))
    } else {
        encode(partition, scope.as_deref(), budget).map(Some)
    }
}
