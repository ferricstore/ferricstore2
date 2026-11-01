//! Pinned original query-row production scenarios at66586c596e27.
#![forbid(unsafe_code)]
use wf_domain::metadata::OriginalMetadataInput as I;
use wf_ports::projection::query_row::{
    OriginalQueryLocator as Locator, OriginalQueryRecordError as E, OriginalQueryRecordLimits as L,
    encode_original_query_row as encode, prepare_original_query_metadata as prepare,
};
type TestResult<T = ()> = Result<T, String>;
mod fixtures;
mod resources;
mod source;
mod validation;
mod wire;
