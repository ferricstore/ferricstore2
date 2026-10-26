//! Exact field keys and owned ordered writes over an unchanged borrowed base.

use super::model::{HistoryValueEntry, HistoryValueKind};
use crate::value::OriginalValueDocument;

/// One of the fifteen exact source field keys.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HistoryValueField {
    /// The value field itself.
    Value(HistoryValueKind),
    /// The corresponding missing flag.
    Missing(HistoryValueKind),
    /// The corresponding omitted flag.
    Omitted(HistoryValueKind),
    /// The complete stored-size field.
    Size(HistoryValueKind),
    /// The corresponding sanitized error field.
    Error(HistoryValueKind),
}

impl HistoryValueField {
    /// Return the exact static field key, including the error-kind error key.
    #[must_use]
    pub const fn key(self) -> &'static str {
        match self {
            Self::Value(HistoryValueKind::Payload) => "payload",
            Self::Value(HistoryValueKind::Result) => "result",
            Self::Value(HistoryValueKind::Error) => "error",
            Self::Missing(HistoryValueKind::Payload) => "payload_missing",
            Self::Missing(HistoryValueKind::Result) => "result_missing",
            Self::Missing(HistoryValueKind::Error) => "error_missing",
            Self::Omitted(HistoryValueKind::Payload) => "payload_omitted",
            Self::Omitted(HistoryValueKind::Result) => "result_omitted",
            Self::Omitted(HistoryValueKind::Error) => "error_omitted",
            Self::Size(HistoryValueKind::Payload) => "payload_size",
            Self::Size(HistoryValueKind::Result) => "result_size",
            Self::Size(HistoryValueKind::Error) => "error_size",
            Self::Error(HistoryValueKind::Payload) => "payload_error",
            Self::Error(HistoryValueKind::Result) => "result_error",
            Self::Error(HistoryValueKind::Error) => "error_error",
        }
    }
}

/// An owned field value without cloning or rebuilding the original document.
#[derive(Debug)]
pub enum HistoryValue {
    /// A complete decoded document, including all common and foreign data.
    Decoded(OriginalValueDocument),
    /// The source nil value for a missing result.
    Nil,
    /// A source boolean flag.
    Boolean(bool),
    /// The complete stored byte count.
    StoredSize(usize),
    /// A static sanitized source error.
    Error(&'static str),
}

/// One field write in original event and payload/result/error processing order.
#[derive(Debug)]
pub struct HistoryValueWrite {
    /// Original history entry index.
    pub entry_index: usize,
    /// Exact destination field.
    pub field: HistoryValueField,
    /// Independently owned decoded value or scalar write.
    pub value: HistoryValue,
}

/// An unchanged original history view plus exact ordered field writes.
#[derive(Debug)]
pub struct HistoryValueOverlay<'a, T: ?Sized> {
    /// Original entries; fields absent from writes remain untouched.
    pub history: &'a [HistoryValueEntry<'a, T>],
    /// Owned typed field writes in source order.
    pub writes: Vec<HistoryValueWrite>,
    /// Conservatively admitted work cohorts across the complete attempt.
    pub charged_work: usize,
    /// Cumulative requested-owner liabilities, without reset or refund.
    pub charged_requested_bytes: usize,
}
