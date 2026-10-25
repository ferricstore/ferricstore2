//! One prospective continuation per entered encoded level.

use std::ops::Range;

use crate::value::original_flow::scan::TermSummary;

pub(in crate::value::original_flow) enum ParseFrame {
    Sequence {
        tag: u8,
        offset: usize,
        depth: usize,
        slots: Range<usize>,
        filled: usize,
        total: usize,
    },
    Alias {
        tag: u8,
        offset: usize,
        depth: usize,
    },
    String {
        tag: u8,
        offset: usize,
        depth: usize,
        bytes: Range<usize>,
        slots: Range<usize>,
        next: usize,
    },
    Identity {
        tag: u8,
        offset: usize,
        depth: usize,
        words: usize,
    },
    Export {
        tag: u8,
        offset: usize,
        depth: usize,
        module: Option<TermSummary>,
        function: Option<TermSummary>,
    },
    LocalFun {
        tag: u8,
        offset: usize,
        depth: usize,
        module: Option<TermSummary>,
        arity: u8,
        index: i32,
        uniq: [u8; 16],
        old_index: Option<i64>,
        old_uniq: Option<i64>,
        creator: Option<usize>,
        free: Range<usize>,
        filled: usize,
    },
}

pub(in crate::value::original_flow) fn expects_atom(frame: &ParseFrame) -> bool {
    matches!(
        frame,
        ParseFrame::Identity { .. }
            | ParseFrame::Export { module: None, .. }
            | ParseFrame::Export { function: None, .. }
            | ParseFrame::LocalFun { module: None, .. }
    )
}
