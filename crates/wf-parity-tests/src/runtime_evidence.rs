//! Checked original registration, complement accounting and source-origin links.

use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
};

use serde::Deserialize;
use serde_json::Value;

use crate::{
    CheckError, SOURCE_COMMIT, artifact, macro_origin,
    progress::{ArtifactRef, ProgressSummary},
    registration::Registration,
};

#[derive(Deserialize)]
pub(crate) struct CollectionDetails {
    schema_version: u32,
    source_commit: String,
    test_execution: bool,
    registered_case_count: usize,
    registered_module_count: usize,
    pub(crate) helper_results: Vec<Value>,
    pub(crate) helper_profiles: Value,
    pub(crate) all_registered_cases: Vec<RawCase>,
    #[serde(default)]
    pub(crate) additional_source_files: Vec<AdditionalSource>,
    #[serde(default)]
    additional_root_review: Option<ArtifactRef>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RootSelectionReview {
    schema_version: u32,
    source_commit: String,
    roots: Vec<String>,
    files: Vec<ReviewedSource>,
    scope_decision: String,
    selection_omission_reason: String,
    review_note: String,
    registration_is_execution: bool,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ReviewedSource {
    source_path: String,
    sha256: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct AdditionalSource {
    pub(crate) source_path: String,
    pub(crate) source_ref: ArtifactRef,
    pub(crate) sha256: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RawCase {
    pub(crate) source_path: String,
    pub(crate) source_line: usize,
    pub(crate) module: String,
    pub(crate) name: String,
    pub(crate) tags: BTreeMap<String, Value>,
}

pub(crate) fn read_ref(root: &Path, evidence: &ArtifactRef) -> Result<Vec<u8>, CheckError> {
    let bytes = artifact::read(root, &evidence.path, 32 * 1024 * 1024)?;
    if !artifact::valid_hex(&evidence.sha256, 64) || artifact::digest(&bytes) != evidence.sha256 {
        return Err(CheckError::new(format!(
            "evidence SHA-256 mismatch: {}",
            evidence.path
        )));
    }
    Ok(bytes)
}

pub(crate) fn identity(case: &RawCase) -> String {
    artifact::digest(
        [
            SOURCE_COMMIT,
            &case.source_path,
            &case.source_line.to_string(),
            &case.module,
            &case.name,
        ]
        .join("\0")
        .as_bytes(),
    )
}

pub(crate) fn check(
    root: &Path,
    collection: &Registration,
    summary: &mut ProgressSummary,
) -> Result<(), CheckError> {
    let details_ref = collection
        .collection_details
        .as_ref()
        .ok_or_else(|| CheckError::new("schema 2 requires original collection details"))?;
    let bytes = read_ref(root, details_ref)?;
    let details: CollectionDetails = serde_json::from_slice(&bytes)
        .map_err(|error| CheckError::new(format!("invalid original collection: {error}")))?;
    let raw = check_details(&details)?;
    check_additional_sources(root, &details)?;
    let origin_ref = collection
        .macro_origins
        .as_ref()
        .ok_or_else(|| CheckError::new("schema 2 requires macro origins artifact"))?;
    let origins = macro_origin::load(root, origin_ref, details_ref, &details)?;
    let linked = check_linked(collection, &raw, &origins)?;
    check_unmatched(collection, &raw, &linked)?;
    summary.registered_runtime_cases = raw.len();
    summary.unmatched_runtime_cases = raw.len() - linked.len();
    summary.raw_runtime_accounting = true;
    Ok(())
}

fn check_additional_sources(root: &Path, details: &CollectionDetails) -> Result<(), CheckError> {
    let Some(evidence) = &details.additional_root_review else {
        if details.additional_source_files.is_empty() {
            return Ok(());
        }
        return Err(CheckError::new(
            "additional sources require pinned root review",
        ));
    };
    let review: RootSelectionReview = serde_json::from_slice(&read_ref(root, evidence)?)
        .map_err(|error| CheckError::new(format!("invalid additional root review: {error}")))?;
    if review.schema_version != 1
        || review.source_commit != SOURCE_COMMIT
        || review.registration_is_execution
        || review.scope_decision != "pending-case-review"
        || review.selection_omission_reason.trim().is_empty()
        || review.roots.is_empty()
    {
        return Err(CheckError::new("invalid additional root scope/provenance"));
    }
    artifact::read(root, &review.review_note, 1024 * 1024)?;
    let mut reviewed = BTreeMap::new();
    for file in &review.files {
        artifact::portable_path(&file.source_path)?;
        if reviewed
            .insert(file.source_path.as_str(), file.sha256.as_str())
            .is_some()
        {
            return Err(CheckError::new("duplicate additional reviewed source"));
        }
    }
    let mut sources = BTreeSet::new();
    for source in &details.additional_source_files {
        artifact::portable_path(&source.source_path)?;
        if reviewed.get(source.source_path.as_str()) != Some(&source.sha256.as_str())
            || source.sha256 != source.source_ref.sha256
            || !source
                .source_ref
                .path
                .starts_with("tests/parity/collection/")
            || !sources.insert(source.source_path.as_str())
            || root
                .join("tests/reference/elixir-baseline")
                .join(&source.source_path)
                .exists()
        {
            return Err(CheckError::new(
                "additional source differs from pinned review or overlaps frozen reference",
            ));
        }
        read_ref(root, &source.source_ref)?;
    }
    let expected: BTreeSet<_> = reviewed.keys().copied().collect();
    let roots: BTreeSet<_> = review.roots.iter().map(String::as_str).collect();
    if expected != sources || roots.len() != review.roots.len() || !roots.is_subset(&sources) {
        return Err(CheckError::new(
            "additional root/source accounting differs from review",
        ));
    }
    Ok(())
}

fn check_details(details: &CollectionDetails) -> Result<BTreeMap<String, &RawCase>, CheckError> {
    if !matches!(details.schema_version, 1 | 2)
        || details.source_commit != SOURCE_COMMIT
        || details.test_execution
        || details.registered_case_count != details.all_registered_cases.len()
        || details.all_registered_cases.is_empty()
    {
        return Err(CheckError::new(
            "invalid original registration provenance/counts",
        ));
    }
    let mut raw = BTreeMap::new();
    let mut scopes = BTreeSet::new();
    let mut modules = BTreeSet::new();
    for case in &details.all_registered_cases {
        artifact::portable_path(&case.source_path)?;
        if case.source_line == 0
            || !case.module.starts_with("Elixir.")
            || !case.name.starts_with("test ")
            || case.module.contains('\0')
            || case.name.contains('\0')
            || raw.insert(identity(case), case).is_some()
            || !scopes.insert((&case.module, &case.name))
        {
            return Err(CheckError::new(
                "duplicate or malformed original registered case",
            ));
        }
        modules.insert(&case.module);
    }
    if modules.len() != details.registered_module_count {
        return Err(CheckError::new("original registered module count differs"));
    }
    Ok(raw)
}

fn check_linked<'a>(
    collection: &'a Registration,
    raw: &BTreeMap<String, &RawCase>,
    origins: &BTreeMap<String, macro_origin::SourceOrigin>,
) -> Result<BTreeSet<&'a str>, CheckError> {
    let mut linked = BTreeSet::new();
    let mut used_origins = BTreeSet::new();
    for case in &collection.cases {
        let id = case
            .runtime_case_id
            .as_deref()
            .ok_or_else(|| CheckError::new("linked case requires original runtime identity"))?;
        let original = raw
            .get(id)
            .ok_or_else(|| CheckError::new("missing original registered case"))?;
        if original.module != case.module
            || original.name != case.name
            || original.tags != case.tags
            || !linked.insert(id)
        {
            return Err(CheckError::new(
                "linked case differs from original registered case",
            ));
        }
        let scope = case.scope_decision.as_deref().ok_or_else(|| {
            CheckError::new("linked case requires reviewed scope or pending decision")
        })?;
        if !matches!(scope, "included-workflow" | "pending-case-review") {
            return Err(CheckError::new("invalid reviewed scope"));
        }
        if let Some(origin_id) = &case.origin_id {
            let origin = origins
                .get(origin_id)
                .ok_or_else(|| CheckError::new("linked macro case lacks reviewed source origin"))?;
            origin.matches(case, original)?;
            if !used_origins.insert(origin_id.as_str()) {
                return Err(CheckError::new("duplicate used macro origin"));
            }
        } else if original.source_path != case.source_path
            || original.source_line != case.source_line
            || scope != "pending-case-review"
        {
            return Err(CheckError::new(
                "non-exact registration link requires reviewed macro origin",
            ));
        }
    }
    let expected: BTreeSet<_> = origins.keys().map(String::as_str).collect();
    if expected != used_origins {
        return Err(CheckError::new("unused or missing reviewed macro origins"));
    }
    Ok(linked)
}

fn check_unmatched(
    collection: &Registration,
    raw: &BTreeMap<String, &RawCase>,
    linked: &BTreeSet<&str>,
) -> Result<(), CheckError> {
    let unmatched = collection
        .unmatched_runtime_case_ids
        .as_ref()
        .ok_or_else(|| CheckError::new("schema 2 requires unmatched runtime accounting"))?;
    let expected: BTreeSet<_> = raw
        .keys()
        .map(String::as_str)
        .filter(|id| !linked.contains(id))
        .collect();
    let actual: BTreeSet<_> = unmatched.iter().map(String::as_str).collect();
    if expected != actual
        || actual.len() != unmatched.len()
        || (collection.collection_complete && !actual.is_empty())
    {
        return Err(CheckError::new(
            "unmatched runtime accounting has missing/duplicate/fabricated identities",
        ));
    }
    Ok(())
}
