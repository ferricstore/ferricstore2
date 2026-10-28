//! Supplemental source-derived value codec controls, not `HistoryValues` mappings.
//! Pinned source: 66586c596e271408d9184af32eb31cd7218c126e, Codec.Support125..180
//! and TermCodec22/fa1f00df. The three `HistoryValues` cases are linked separately
//! in `history_values/overlay.rs`, with real storage conformance beside hydration.
//! These codec controls remain supplemental and receive no extra source links.
//! Foreign identities are data; no BEAM invocation or resource owner is implied.
//! Test fixture allocation is outside the decoder's operational ownership claim.

#![forbid(unsafe_code)]

use wf_domain::value::{
    OriginalFlowValueError as E, OriginalValueContext as C, OriginalValueDocument as D,
    OriginalValueFunctionIdentity as F, OriginalValueLimits as L, OriginalValueNode as N,
    decode_original_flow_value as decode,
};

mod context;
mod scalars;
mod source;
mod structure;
mod wrappers;
