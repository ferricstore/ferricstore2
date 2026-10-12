//! Checked compiler proposals remain separate from runtime/execution accounting.

mod model;
mod refs;
mod relations;
mod verifier;

use std::path::Path;

use crate::{CheckError, artifact, reference};
use model::Capture;

const METADATA: &str = "tests/parity/collection/compiler-v1/compiler-lineage.json";
const NORMALIZATION: &str = "elixir-ast-metadata-hygiene-etf-v1";
const RULE: &str = "compiler-constant-using-v1";

/// Verified candidate counts. None certifies test execution or changes progress.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CompilerLineageSummary {
    /// Complete original registration rows retained in this separate capture.
    pub registered_cases: usize,
    /// Uniquely reconstructed constant-quote candidate origins.
    pub candidate_origins: usize,
    /// Raw identities outside exact links and verified candidate proposals.
    pub unresolved_runtime_cases: usize,
    /// Frozen declarations outside exact links and verified candidate proposals.
    pub unresolved_declarations: usize,
    /// Complete observed compiled module versions, including unproved headers.
    pub module_versions: usize,
    /// Resolved compiler trace rows, including function observations.
    pub macro_events: usize,
}

pub(super) fn check(root: &Path) -> Result<CompilerLineageSummary, CheckError> {
    let root = root
        .canonicalize()
        .map_err(|error| CheckError::new(format!("compiler root: {error}")))?;
    let (manifest, _) = reference::load(&root)
        .map_err(|error| CheckError::new(format!("compiler frozen prerequisite: {error}")))?;
    let bytes = artifact::read(&root, METADATA, 64 * 1024 * 1024)
        .map_err(|error| CheckError::new(format!("compiler capture prerequisite: {error}")))?;
    let capture: Capture = serde_json::from_slice(&bytes)
        .map_err(|error| CheckError::new(format!("invalid compiler capture: {error}")))?;
    let (details, registration) = refs::check(&root, &capture, &bytes)?;
    relations::check(&capture, &manifest, &details, &registration)?;
    verifier::check(&root, &capture, &artifact::digest(&bytes))?;
    Ok(CompilerLineageSummary {
        registered_cases: capture.registered.len(),
        candidate_origins: capture.candidate_origins.len(),
        unresolved_runtime_cases: capture.unresolved_runtime_case_ids.len(),
        unresolved_declarations: capture.unresolved_declaration_ids.len(),
        module_versions: capture.modules.len(),
        macro_events: capture.calls.len(),
    })
}
