//! Supplemental source-derived Keys and logical-binary controls at
//! 66586c596e271408d9184af32eb31cd7218c126e. Root's original packet is
//! 8948996+7569/c9f1f4dc:15 references,20 predicates,2 sizes,9 binaries and CRC.
//! Eligible builders intentionally exclude raw Keys renderings that fail its
//! predicate. No storage, publication, placement or `HistoryValues` parity follows.
//! Fixture allocations and borrowed root backing are outside production claims.

#![forbid(unsafe_code)]

use wf_ports::blob::{
    OriginalValueBuildError as E, OriginalValuePartition as P, OriginalValueRef as R,
    OriginalValueRefKind as K, original_binary_stored_len as stored_len,
    original_binary_stored_parts as parts, parse_original_value_ref as parse,
    render_original_named_value_ref as named, render_original_value_ref as render,
};

mod encoding;
mod references;
