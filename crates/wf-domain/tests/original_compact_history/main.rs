//! Pinned FSH2 source scenarios and independent wire/resource controls.

#![forbid(unsafe_code)]

mod fixtures;
mod limits;
mod malformed;
mod metadata;
mod numeric;
mod source;

use wf_domain::history::compact::{
    DecodedOriginalCompactHistory as D, OriginalCompactHistoryContext as C,
    OriginalCompactHistoryError as E, OriginalCompactHistoryLimits as L,
    OriginalCompactHistorySnapshot as S, PreparedOriginalCompactHistory as P,
    decode_original_compact_history as decode, prepare_original_compact_history as prepare,
};
use wf_domain::metadata::OriginalMetadataInput as M;

type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;
