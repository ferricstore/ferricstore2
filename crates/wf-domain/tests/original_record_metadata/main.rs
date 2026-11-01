//! Supplemental complete data-capture controls; no storage/source-case credit.
#![forbid(unsafe_code)]
use wf_domain::{
    metadata::record::{
        OriginalRecordMetadataError as E, OriginalRecordMetadataField as F,
        decode_original_record_metadata as decode, prepare_original_record_metadata as prepare,
    },
    value::{
        OriginalValueContext as Context, OriginalValueEncodeLimits as EncodeLimits,
        OriginalValueInput, OriginalValueLimits as Limits, OriginalValueNode as Node,
    },
};
type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;
mod fixtures;
mod resources;
mod structure;
mod values;
