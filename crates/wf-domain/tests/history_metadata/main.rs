//! Captured original Codec.Support metadata, without native/profile proof.

#![forbid(unsafe_code)]

mod budgets;
mod fixtures;
mod refusals;
mod scalars;
mod source;

use wf_domain::history::metadata::{
    OriginalHistoryMetadataError as E, OriginalHistoryMetadataLimits as L,
    PreparedOriginalHistoryMetadata as P, prepare_original_history_metadata as prepare,
};
use wf_domain::value::{OriginalValueInput as I, OriginalValueNode as N};

type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;
