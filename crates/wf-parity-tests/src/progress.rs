//! Comprehensive source accounting and checked links, never execution proof.

use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
};

use serde::Deserialize;

use crate::{
    CheckError, REFERENCE_MANIFEST_SHA256, SOURCE_COMMIT, artifact,
    reference::{Manifest, SourceDeclaration},
    registration, rust_source,
};

const PROGRESS_PATH: &str = "tests/parity/workflow-progress.json";
const JSON_LIMIT: u64 = 16 * 1024 * 1024;
const SOURCE_LIMIT: u64 = 16 * 1024 * 1024;

/// Validated progress counts; none represents passing behavioral parity.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ProgressSummary {
    /// Frozen textual declaration identities accounted for exactly once.
    pub declarations: usize,
    /// Declarations with checked, source-derived Rust assertion links.
    pub source_derived_declarations: usize,
    /// Checked Rust test/scenario mapping records, not executed tests.
    pub rust_counterparts: usize,
    /// Checked source-to-Rust assertion links.
    pub assertions: usize,
    /// Cases registered by the referenced collection, not executed cases.
    pub registered_runtime_cases: usize,
    /// Registered cases linked to frozen declarations.
    pub linked_runtime_cases: usize,
    /// Original runtime identities retained without a declaration link.
    pub unmatched_runtime_cases: usize,
    /// Whether all original runtime identities are accounted for bidirectionally.
    pub raw_runtime_accounting: bool,
    /// Declarations still without a registered runtime case.
    pub unresolved_declarations: usize,
    /// Whether an actual collection artifact was supplied and validated.
    pub collection_present: bool,
    /// Whether registration accounts for every declaration without blockers.
    pub reconciliation_complete: bool,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Ledger {
    schema_version: u32,
    source_commit: String,
    reference_manifest_sha256: String,
    runtime_collection: Option<ArtifactRef>,
    declarations: Vec<Entry>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub(crate) struct ArtifactRef {
    pub(crate) path: String,
    pub(crate) sha256: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Entry {
    pub(crate) source_declaration_id: String,
    pub(crate) runtime_case_ids: Vec<String>,
    rust_assertions: Vec<Counterpart>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Counterpart {
    scenario_id: String,
    rust_file: String,
    rust_file_sha256: String,
    rust_module: String,
    rust_function: String,
    assertions: Vec<Assertion>,
    limitations: Vec<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Assertion {
    source_line: usize,
    source_text: String,
    rust_line: usize,
    rust_text: String,
}

pub(crate) fn check(root: &Path, manifest: &Manifest) -> Result<ProgressSummary, CheckError> {
    let bytes = artifact::read(root, PROGRESS_PATH, JSON_LIMIT)?;
    let ledger: Ledger = serde_json::from_slice(&bytes)
        .map_err(|error| CheckError::new(format!("invalid progress ledger: {error}")))?;
    if ledger.schema_version != 1
        || ledger.source_commit != SOURCE_COMMIT
        || ledger.reference_manifest_sha256 != REFERENCE_MANIFEST_SHA256
    {
        return Err(CheckError::new("unsupported progress provenance or schema"));
    }
    check_ids(&ledger.declarations, manifest)?;
    let mut summary = ProgressSummary {
        declarations: ledger.declarations.len(),
        source_derived_declarations: 0,
        rust_counterparts: 0,
        assertions: 0,
        registered_runtime_cases: 0,
        linked_runtime_cases: 0,
        unmatched_runtime_cases: 0,
        raw_runtime_accounting: false,
        unresolved_declarations: ledger.declarations.len(),
        collection_present: false,
        reconciliation_complete: false,
    };
    check_assertions(root, manifest, &ledger.declarations, &mut summary)?;
    registration::check(
        root,
        manifest,
        ledger.runtime_collection.as_ref(),
        &ledger.declarations,
        &mut summary,
    )?;
    Ok(summary)
}

fn check_ids(entries: &[Entry], manifest: &Manifest) -> Result<(), CheckError> {
    let expected: BTreeSet<_> = manifest
        .declarations
        .iter()
        .map(|item| item.id.as_str())
        .collect();
    let mut found = BTreeSet::new();
    for entry in entries {
        if !found.insert(entry.source_declaration_id.as_str()) {
            return Err(CheckError::new(
                "duplicate source declaration ID in progress",
            ));
        }
    }
    if expected != found {
        return Err(CheckError::new(
            "progress has missing or unknown source declaration IDs",
        ));
    }
    Ok(())
}

fn check_assertions(
    root: &Path,
    manifest: &Manifest,
    entries: &[Entry],
    summary: &mut ProgressSummary,
) -> Result<(), CheckError> {
    let declarations: BTreeMap<_, _> = manifest
        .declarations
        .iter()
        .map(|item| (item.id.as_str(), item))
        .collect();
    let mut scenarios = BTreeSet::new();
    for entry in entries {
        let declaration = declarations
            .get(entry.source_declaration_id.as_str())
            .ok_or_else(|| CheckError::new("unknown assertion source declaration"))?;
        if !entry.rust_assertions.is_empty() {
            summary.source_derived_declarations += 1;
        }
        for counterpart in &entry.rust_assertions {
            if counterpart.scenario_id.trim().is_empty()
                || !scenarios.insert(counterpart.scenario_id.as_str())
            {
                return Err(CheckError::new("empty or duplicate assertion scenario ID"));
            }
            check_counterpart(root, manifest, declaration, counterpart)?;
            summary.rust_counterparts += 1;
            summary.assertions += counterpart.assertions.len();
        }
    }
    Ok(())
}

fn check_counterpart(
    root: &Path,
    manifest: &Manifest,
    declaration: &SourceDeclaration,
    counterpart: &Counterpart,
) -> Result<(), CheckError> {
    if counterpart.assertions.is_empty() {
        return Err(CheckError::new(
            "empty assertion mapping cannot count as source-derived",
        ));
    }
    if counterpart
        .limitations
        .iter()
        .any(|text| text.trim().is_empty())
    {
        return Err(CheckError::new("empty counterpart limitation"));
    }
    artifact::portable_path(&counterpart.rust_file)?;
    if !counterpart.rust_file.starts_with("crates/")
        || Path::new(&counterpart.rust_file)
            .extension()
            .is_none_or(|extension| extension != "rs")
    {
        return Err(CheckError::new(
            "Rust counterpart must be a crate Rust source file",
        ));
    }
    let rust_bytes = artifact::read(root, &counterpart.rust_file, SOURCE_LIMIT)?;
    if !artifact::valid_hex(&counterpart.rust_file_sha256, 64)
        || artifact::digest(&rust_bytes) != counterpart.rust_file_sha256
    {
        return Err(CheckError::new("Rust counterpart SHA-256 mismatch"));
    }
    let rust = String::from_utf8(rust_bytes).map_err(|error| CheckError::new(error.to_string()))?;
    let test = rust_source::find_test(&rust, &counterpart.rust_module, &counterpart.rust_function)?;
    let source_path = format!(
        "tests/reference/elixir-baseline/{}",
        declaration.source_path
    );
    let source_bytes = artifact::read(root, &source_path, SOURCE_LIMIT)?;
    let source =
        String::from_utf8(source_bytes).map_err(|error| CheckError::new(error.to_string()))?;
    let end = manifest
        .declarations
        .iter()
        .filter(|item| item.source_path == declaration.source_path && item.line > declaration.line)
        .map(|item| item.line)
        .min()
        .unwrap_or(usize::MAX);
    let mut links = BTreeSet::new();
    for assertion in &counterpart.assertions {
        if !links.insert((assertion.source_line, assertion.rust_line)) {
            return Err(CheckError::new("duplicate source-to-Rust assertion link"));
        }
        check_source_assertion(&source, declaration.line, end, assertion)?;
        rust_source::check_assertion(&rust, &test, assertion.rust_line, &assertion.rust_text)?;
    }
    Ok(())
}

fn check_source_assertion(
    source: &str,
    start: usize,
    end: usize,
    assertion: &Assertion,
) -> Result<(), CheckError> {
    let line = assertion
        .source_line
        .checked_sub(1)
        .and_then(|index| source.lines().nth(index));
    if assertion.source_line <= start
        || assertion.source_line >= end
        || line.map(str::trim) != Some(assertion.source_text.as_str())
        || !(assertion.source_text.starts_with("assert ")
            || assertion.source_text.starts_with("refute "))
    {
        return Err(CheckError::new(
            "source assertion line/text is absent or outside its declaration",
        ));
    }
    Ok(())
}
