//! Complete pinned `TermCodec` scenarios, with explicit captured compatibility data.

#![forbid(unsafe_code)]

mod binary;
mod deep;
mod fixtures;
mod foreign;
mod limits;
mod source;

use wf_domain::value::{
    OriginalFlowValueError as DError, OriginalValueContext as C, OriginalValueDocument as D,
    OriginalValueEncodeError as E, OriginalValueEncodeLimits as L, OriginalValueInput as I,
    OriginalValueLimits as DL, OriginalValueNode as N, PreparedOriginalExternalTerm as P,
    decode_original_external_term as decode, prepare_original_external_term as prepare,
};

type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;
