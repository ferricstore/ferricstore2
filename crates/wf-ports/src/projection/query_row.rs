//! Original query metadata and physical-row production without storage authority.
mod budget;
mod config;
mod create;
mod dynamic;
mod locator;
mod metadata;
mod model;
mod primitive;
mod read;
mod record;
mod row;
mod writer;
pub use create::{OriginalCreatePreparationError, PreparedOriginalCreate, prepare_original_create};
pub use model::{
    OriginalQueryLocator, OriginalQueryRecordError, OriginalQueryRecordLimits,
    PreparedOriginalQueryMetadata, PreparedOriginalQueryRow,
};
pub use read::{
    DecodedOriginalQueryRow, OriginalQueryDecodePlan, OriginalQueryInternalRecord,
    OriginalQueryLocation, OriginalQueryRead, OriginalQueryReadLimits, OriginalQueryRecord,
    OriginalQueryScope, OriginalQuerySelection, OriginalQueryStateSelection,
    decode_original_query_for_query, decode_original_query_reference, decode_original_query_row,
    prepare_original_query_internal_record, relocate_original_query_row,
};
use wf_domain::metadata::OriginalMetadataInput as I;

/// Validate and prepare original query metadata without a physical locator.
/// # Errors
/// Source invalidity/size and operational refusals remain distinct.
pub fn prepare_original_query_metadata<'a>(
    state_key: I<'_, '_>,
    record: I<'_, 'a>,
    limits: OriginalQueryRecordLimits,
) -> Result<PreparedOriginalQueryMetadata<'a>, OriginalQueryRecordError> {
    let mut budget = budget::Budget::new(limits)?;
    let plan = record::plan(state_key, record, &mut budget)?;
    budget.output(budget::add(
        plan.metadata_bytes,
        plan.scope.as_ref().map_or(0, Vec::len),
    )?)?;
    let mut out = writer::Writer::owned(plan.metadata_bytes, &mut budget)?;
    metadata::emit(&plan, &mut out, &mut budget)?;
    Ok(PreparedOriginalQueryMetadata {
        encoded: out.finish()?,
        scope: plan.scope,
        id: plan.id,
        partition: plan.partition,
        version: plan.version,
        work: budget.work,
        temporary: budget.peak,
    })
}

/// Encode an original physical query row from captured source data.
/// # Errors
/// Refuses invalid source records/locators and independent operational limits.
pub fn encode_original_query_row(
    state_key: I<'_, '_>,
    record: I<'_, '_>,
    locator: OriginalQueryLocator<'_, '_>,
    expiry: I<'_, '_>,
    limits: OriginalQueryRecordLimits,
) -> Result<PreparedOriginalQueryRow, OriginalQueryRecordError> {
    let mut budget = budget::Budget::new(limits)?;
    let encoded = row::encode(state_key, record, locator, expiry, &mut budget)?;
    Ok(PreparedOriginalQueryRow {
        encoded,
        work: budget.work,
        temporary: budget.peak,
    })
}
