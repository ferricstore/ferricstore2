//! Runtime registration identity/accounting; registration is never execution.

use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
};

use serde::Deserialize;

use crate::{
    CheckError, REFERENCE_MANIFEST_SHA256, SOURCE_COMMIT, artifact,
    progress::{ArtifactRef, Entry, ProgressSummary},
    reference::{Manifest, SourceDeclaration},
    runtime_evidence,
};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Registration {
    pub(crate) schema_version: u32,
    source_commit: String,
    reference_manifest_sha256: String,
    collection_kind: String,
    pub(crate) collection_complete: bool,
    blockers: Vec<String>,
    unresolved_declaration_ids: Vec<String>,
    pub(crate) cases: Vec<RegisteredTest>,
    #[serde(default)]
    pub(crate) collection_details: Option<ArtifactRef>,
    #[serde(default)]
    pub(crate) macro_origins: Option<ArtifactRef>,
    #[serde(default)]
    pub(crate) unmatched_runtime_case_ids: Option<Vec<String>>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RegisteredTest {
    pub(crate) case_id: String,
    pub(crate) source_declaration_id: String,
    pub(crate) source_path: String,
    pub(crate) source_line: usize,
    pub(crate) module: String,
    pub(crate) name: String,
    pub(crate) tags: BTreeMap<String, serde_json::Value>,
    #[serde(default)]
    pub(crate) runtime_case_id: Option<String>,
    #[serde(default)]
    pub(crate) origin_id: Option<String>,
    #[serde(default)]
    pub(crate) scope_decision: Option<String>,
}

pub(crate) fn check(
    root: &Path,
    manifest: &Manifest,
    evidence: Option<&ArtifactRef>,
    entries: &[Entry],
    summary: &mut ProgressSummary,
) -> Result<(), CheckError> {
    let Some(evidence) = evidence else {
        if entries
            .iter()
            .any(|entry| !entry.runtime_case_ids.is_empty())
        {
            return Err(CheckError::new("runtime case IDs require collection"));
        }
        return Ok(());
    };
    let bytes = artifact::read(root, &evidence.path, 32 * 1024 * 1024)?;
    if !artifact::valid_hex(&evidence.sha256, 64) || artifact::digest(&bytes) != evidence.sha256 {
        return Err(CheckError::new("collection SHA-256 mismatch"));
    }
    let collection: Registration = serde_json::from_slice(&bytes)
        .map_err(|error| CheckError::new(format!("invalid runtime registration: {error}")))?;
    validate_header(&collection)?;
    let declarations: BTreeMap<_, _> = manifest
        .declarations
        .iter()
        .map(|item| (item.id.as_str(), item))
        .collect();
    let registered = validate_cases(&collection, &declarations)?;
    check_unresolved(&collection, &declarations, &registered)?;
    for entry in entries {
        let case_ids: BTreeSet<_> = entry.runtime_case_ids.iter().map(String::as_str).collect();
        let expected = registered
            .get(entry.source_declaration_id.as_str())
            .cloned()
            .unwrap_or_default();
        if case_ids.len() != entry.runtime_case_ids.len() || case_ids != expected {
            return Err(CheckError::new(
                "runtime case reconciliation differs from registration",
            ));
        }
    }
    summary.registered_runtime_cases = collection.cases.len();
    summary.linked_runtime_cases = collection.cases.len();
    summary.unresolved_declarations = collection.unresolved_declaration_ids.len();
    summary.collection_present = true;
    summary.reconciliation_complete = collection.collection_complete;
    if collection.schema_version == 2 {
        runtime_evidence::check(root, &collection, summary)?;
    }
    Ok(())
}

fn validate_header(collection: &Registration) -> Result<(), CheckError> {
    if !matches!(collection.schema_version, 1 | 2)
        || collection.source_commit != SOURCE_COMMIT
        || collection.reference_manifest_sha256 != REFERENCE_MANIFEST_SHA256
        || collection.collection_kind != "exunit-registration-only"
    {
        return Err(CheckError::new(
            "unsupported registration provenance or schema",
        ));
    }
    if collection.schema_version == 1
        && (collection.collection_details.is_some()
            || collection.macro_origins.is_some()
            || collection.unmatched_runtime_case_ids.is_some()
            || collection.cases.iter().any(|case| {
                case.runtime_case_id.is_some()
                    || case.origin_id.is_some()
                    || case.scope_decision.is_some()
            }))
    {
        return Err(CheckError::new(
            "schema-2 evidence requires registration schema 2",
        ));
    }
    if collection.collection_complete && collection.cases.is_empty() {
        return Err(CheckError::new(
            "complete collection cannot have zero cases",
        ));
    }
    if collection
        .blockers
        .iter()
        .any(|text| text.trim().is_empty())
        || (collection.collection_complete
            && (!collection.blockers.is_empty()
                || !collection.unresolved_declaration_ids.is_empty()))
        || (!collection.collection_complete
            && collection.blockers.is_empty()
            && collection.unresolved_declaration_ids.is_empty())
    {
        return Err(CheckError::new(
            "collection completeness and explicit blockers disagree",
        ));
    }
    Ok(())
}

fn validate_cases<'a>(
    collection: &'a Registration,
    declarations: &BTreeMap<&str, &SourceDeclaration>,
) -> Result<BTreeMap<&'a str, BTreeSet<&'a str>>, CheckError> {
    let mut registered: BTreeMap<&str, BTreeSet<&str>> = BTreeMap::new();
    let mut ids = BTreeSet::new();
    let mut scopes = BTreeSet::new();
    for case in &collection.cases {
        let declaration = declarations
            .get(case.source_declaration_id.as_str())
            .ok_or_else(|| CheckError::new("registered case has unknown source declaration ID"))?;
        let line = case.source_line.to_string();
        let identity = [
            SOURCE_COMMIT,
            &case.source_declaration_id,
            &case.source_path,
            &line,
            &case.module,
            &case.name,
        ]
        .join("\0");
        if case.source_path != declaration.source_path
            || case.source_line != declaration.line
            || !case.module.starts_with("Elixir.")
            || case.module.len() <= 7
            || !case.name.starts_with("test ")
            || case.name.len() <= 5
            || case.module.contains('\0')
            || case.name.contains('\0')
            || artifact::digest(identity.as_bytes()) != case.case_id
            || case.tags.keys().any(String::is_empty)
        {
            return Err(CheckError::new(
                "registered case identity disagrees with its source scope/hash",
            ));
        }
        if !ids.insert(case.case_id.as_str()) || !scopes.insert((&case.module, &case.name)) {
            return Err(CheckError::new(
                "duplicate registered case identity or module/name scope",
            ));
        }
        registered
            .entry(&case.source_declaration_id)
            .or_default()
            .insert(&case.case_id);
    }
    Ok(registered)
}

fn check_unresolved(
    collection: &Registration,
    declarations: &BTreeMap<&str, &SourceDeclaration>,
    registered: &BTreeMap<&str, BTreeSet<&str>>,
) -> Result<(), CheckError> {
    let expected: BTreeSet<_> = declarations
        .keys()
        .copied()
        .filter(|id| !registered.contains_key(id))
        .collect();
    let unresolved: BTreeSet<_> = collection
        .unresolved_declaration_ids
        .iter()
        .map(String::as_str)
        .collect();
    if expected != unresolved || unresolved.len() != collection.unresolved_declaration_ids.len() {
        return Err(CheckError::new(
            "unresolved declaration accounting has missing, duplicate or fabricated IDs",
        ));
    }
    Ok(())
}
