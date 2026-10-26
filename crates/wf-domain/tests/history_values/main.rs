//! Complete pure `HistoryValues` collector/overlay acceptance.
//!
//! Original cases preserve async:true, the flow module tag and no local setup
//! as source provenance; these synchronous pure tests acquire no store context.

#![forbid(unsafe_code)]

mod collection;
mod decoding;
mod fixtures;
mod limits;
mod overlay;

use wf_domain::history::values::{
    HistoryValue, HistoryValueEntry, HistoryValueError, HistoryValueField, HistoryValueKind,
    HistoryValueLimits, HistoryValueOverlay, HistoryValueReadBatch, HistoryValueReadResult,
    HistoryValueReference, HistoryValueReturn, HistoryValueWrite, apply_history_value_results,
    prepare_history_values,
};
use wf_domain::value::{
    OriginalFlowValueError, OriginalValueContext, OriginalValueDocument, OriginalValueLimits,
    OriginalValueNode, decode_original_flow_value,
};

type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;
