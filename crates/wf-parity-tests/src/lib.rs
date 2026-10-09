//! M0 artifact preservation checks, distinct from unimplemented runtime parity.

#![forbid(unsafe_code)]

mod artifact;
mod mapping;
mod reference;

use std::{
    error::Error,
    fmt,
    path::{Path, PathBuf},
};

pub use reference::ReferenceSummary;

/// SHA-256 of the preserved schema-1 manifest at the reviewed design HEAD.
pub const REFERENCE_MANIFEST_SHA256: &str =
    "7df8c7885a64b6276e84517dffa4f1ead0f592f6d0996b398cdb61551c1bae61";

/// Pinned Elixir application commit; dirty test deltas remain separate.
pub const SOURCE_COMMIT: &str = "66586c596e271408d9184af32eb31cd7218c126e";

/// An integrity failure or an explicitly incomplete workflow parity gate.
#[derive(Debug)]
pub struct CheckError(String);

impl CheckError {
    pub(crate) fn new(message: impl Into<String>) -> Self {
        Self(message.into())
    }
}

impl fmt::Display for CheckError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl Error for CheckError {}

/// Default checkout root, independent of the caller's current directory.
#[must_use]
pub fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// Verify preserved manifest bytes, snapshots and static section-file closure.
///
/// This performs no Elixir collection or Rust workflow execution.
///
/// # Errors
/// Fails on changed/missing/extra artifacts, unsafe paths or inconsistent IDs.
pub fn check_reference(root: &Path) -> Result<ReferenceSummary, CheckError> {
    reference::load(root).map(|(_, summary)| summary)
}

/// Require actual workflow parity evidence; M0 has no passing path.
///
/// Validates preservation and the honest unported mapping, then returns the
/// collection/execution blockers. Editing statuses or attaching fabricated
/// evidence cannot make this infrastructure-only binary certify parity.
///
/// # Errors
/// Always fails in M0; malformed evidence fails before the incomplete report.
pub fn require_parity(root: &Path) -> Result<(), CheckError> {
    let (manifest, summary) = reference::load(root)?;
    mapping::check(root, &manifest)?;
    Err(CheckError::new(format!(
        "workflow parity incomplete: {} textual declarations are unported; \
         ExUnit runtime cases have not been collected from the pinned application; \
         Rust counterparts, assertion mappings, mandatory profiles and execution proof \
         are absent. M0 artifact integrity is not behavioral parity",
        summary.declarations
    )))
}
