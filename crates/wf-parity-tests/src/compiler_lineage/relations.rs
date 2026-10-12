//! Complete identities, ordered versions and exact unresolved complements.

use std::collections::{BTreeMap, BTreeSet};

use serde_json::Value;

use super::{
    RULE,
    model::{CandidateOrigin, Capture, ModuleProof, ObservedCase, TraceCall},
    refs::ensure,
};
use crate::{
    CheckError, SOURCE_COMMIT, artifact,
    reference::Manifest,
    registration::Registration,
    runtime_evidence::{self, RawCase},
};

pub(super) fn check(
    capture: &Capture,
    manifest: &Manifest,
    details: &Value,
    registration: &Value,
) -> Result<(), CheckError> {
    let raw: Vec<RawCase> = serde_json::from_value(details["all_registered_cases"].clone())
        .map_err(|error| CheckError::new(error.to_string()))?;
    ensure(
        raw.len() == capture.registered.len()
            && details["registered_case_count"].as_u64() == u64::try_from(raw.len()).ok(),
        "compiler raw registration cardinality differs",
    )?;
    let originals = original_index(&raw)?;
    let modules: BTreeMap<_, _> = capture
        .modules
        .iter()
        .map(|row| (row.proof_id.as_str(), row))
        .collect();
    ensure(
        modules.len() == capture.modules.len(),
        "compiler proof identity duplicated",
    )?;
    let versions = version_index(capture);
    let calls = trace_index(capture);
    check_observed(capture, &originals, &modules, &versions)?;
    let observed: BTreeMap<_, _> = capture
        .registered
        .iter()
        .map(|row| (row.runtime_case_id.as_str(), row))
        .collect();
    check_traces(capture, &modules, &versions)?;
    let (mut linked, mut declaration_ids) =
        exact_links(&raw, manifest, registration, capture, &originals)?;
    check_unmatched(details, &originals, &linked)?;
    let declarations: BTreeMap<_, _> = manifest
        .declarations
        .iter()
        .map(|row| (row.id.as_str(), row))
        .collect();
    for candidate in &capture.candidate_origins {
        let row = observed
            .get(candidate.runtime_case_id.as_str())
            .ok_or_else(|| CheckError::new("compiler candidate raw identity missing"))?;
        let declaration = declarations
            .get(candidate.source_declaration_id.as_str())
            .ok_or_else(|| CheckError::new("compiler candidate declaration missing"))?;
        ensure(
            declaration.source_path == candidate.source_path
                && declaration.line == candidate.source_line
                && linked.insert(candidate.runtime_case_id.clone()),
            "compiler candidate/exact overlap or source location disagreement",
        )?;
        candidate_check(
            capture,
            candidate,
            row,
            &modules,
            &versions,
            calls
                .get(&(
                    row.module.as_str(),
                    row.source_path.as_str(),
                    row.source_line,
                ))
                .map_or(&[], Vec::as_slice),
        )?;
        declaration_ids.insert(declaration.id.as_str());
    }
    let unresolved: BTreeSet<_> = originals
        .keys()
        .filter(|id| !linked.contains(*id))
        .map(String::as_str)
        .collect();
    ensure(
        capture
            .unresolved_runtime_case_ids
            .iter()
            .map(String::as_str)
            .collect::<BTreeSet<_>>()
            == unresolved
            && capture.unresolved_runtime_case_ids.len() == unresolved.len(),
        "compiler unresolved raw complement differs",
    )?;
    let unresolved_declarations: BTreeSet<_> = declarations
        .keys()
        .copied()
        .filter(|id| !declaration_ids.contains(id))
        .collect();
    ensure(
        capture
            .unresolved_declaration_ids
            .iter()
            .map(String::as_str)
            .collect::<BTreeSet<_>>()
            == unresolved_declarations
            && capture.unresolved_declaration_ids.len() == unresolved_declarations.len(),
        "compiler unresolved declaration complement differs",
    )?;
    Ok(())
}

fn check_unmatched(
    details: &Value,
    originals: &BTreeMap<String, &RawCase>,
    exact: &BTreeSet<String>,
) -> Result<(), CheckError> {
    let rows: Vec<RawCase> = serde_json::from_value(details["unmatched_runtime_cases"].clone())
        .map_err(|error| CheckError::new(error.to_string()))?;
    let unmatched = original_index(&rows)?;
    ensure(
        unmatched.len() == originals.len() - exact.len(),
        "compiler raw unmatched detail cardinality differs",
    )?;
    for (id, row) in unmatched {
        ensure(
            !exact.contains(&id)
                && originals
                    .get(&id)
                    .is_some_and(|original| original.tags == row.tags),
            "compiler raw unmatched identity/tag complement differs",
        )?;
    }
    Ok(())
}

fn original_index(raw: &[RawCase]) -> Result<BTreeMap<String, &RawCase>, CheckError> {
    let mut originals = BTreeMap::new();
    for row in raw {
        artifact::portable_path(&row.source_path)?;
        ensure(
            row.source_line > 0
                && !row.module.is_empty()
                && !row.name.is_empty()
                && originals
                    .insert(runtime_evidence::identity(row), row)
                    .is_none(),
            "compiler raw identity is invalid/duplicated",
        )?;
    }
    Ok(originals)
}

fn check_observed(
    capture: &Capture,
    originals: &BTreeMap<String, &RawCase>,
    modules: &BTreeMap<&str, &ModuleProof>,
    versions: &BTreeMap<&str, Vec<&ModuleProof>>,
) -> Result<(), CheckError> {
    let mut identities = BTreeSet::new();
    for row in &capture.registered {
        let original = originals.get(&row.runtime_case_id).ok_or_else(|| {
            CheckError::new("compiler observed identity missing from raw capture")
        })?;
        ensure(
            row.source_path == original.source_path
                && row.source_line == original.source_line
                && row.module == original.module
                && row.name == original.name
                && row.tags == original.tags
                && identities.insert(row.runtime_case_id.as_str()),
            "compiler raw tag/identity complement differs",
        )?;
        if let Some(proof) = &row.consumer_proof_id {
            let consumer = modules
                .get(proof.as_str())
                .ok_or_else(|| CheckError::new("compiler consumer proof missing"))?;
            ensure(
                consumer.module == row.module
                    && consumer.source_path.as_deref() == Some(row.source_path.as_str())
                    && latest(versions, &row.module, None)
                        .is_some_and(|latest| latest.proof_id == consumer.proof_id),
                "compiler consumer QName/path/current-version disagreement",
            )?;
        }
    }
    Ok(())
}

fn check_traces(
    capture: &Capture,
    modules: &BTreeMap<&str, &ModuleProof>,
    versions: &BTreeMap<&str, Vec<&ModuleProof>>,
) -> Result<(), CheckError> {
    for call in &capture.calls {
        artifact::portable_path(&call.source_path)?;
        ensure(
            call.source_line > 0
                && call.column != Some(0)
                && !call.consumer_module.is_empty()
                && !call.target_module.is_empty()
                && [
                    "remote_macro",
                    "imported_macro",
                    "remote_function",
                    "imported_function",
                ]
                .contains(&call.kind.as_str())
                && ["__using__", "test", "describe", "setup", "setup_all"]
                    .contains(&call.name.as_str())
                && call
                    .target_loaded_md5
                    .as_deref()
                    .is_none_or(|value| artifact::valid_hex(value, 32))
                && call
                    .framework_sha256
                    .as_deref()
                    .is_none_or(|value| artifact::valid_hex(value, 64)),
            "invalid compiler trace fields",
        )?;
        if let Some(proof) = &call.target_proof_id {
            let target = modules
                .get(proof.as_str())
                .ok_or_else(|| CheckError::new("compiler trace target proof missing"))?;
            ensure(
                target.module == call.target_module
                    && latest(versions, &target.module, Some(call.sequence))
                        .is_some_and(|latest| latest.proof_id == target.proof_id),
                "compiler resolved trace target/version disagreement",
            )?;
        }
    }
    Ok(())
}

fn exact_links<'a>(
    raw: &[RawCase],
    manifest: &'a Manifest,
    registration: &Value,
    capture: &Capture,
    originals: &BTreeMap<String, &RawCase>,
) -> Result<(BTreeSet<String>, BTreeSet<&'a str>), CheckError> {
    let exact: Registration = serde_json::from_value(registration.clone())
        .map_err(|error| CheckError::new(format!("compiler exact registration: {error}")))?;
    ensure(
        exact.schema_version == 1
            && registration["source_commit"] == SOURCE_COMMIT
            && registration["reference_manifest_sha256"] == capture.reference_manifest_sha256
            && registration["collection_kind"] == "exunit-registration-only",
        "compiler exact-registration provenance differs",
    )?;
    let mut locations = BTreeMap::<_, Vec<_>>::new();
    for declaration in &manifest.declarations {
        locations
            .entry((declaration.source_path.as_str(), declaration.line))
            .or_default()
            .push(declaration);
    }
    let expected_exact: BTreeMap<_, _> = raw
        .iter()
        .filter_map(|row| {
            let declarations = locations.get(&(row.source_path.as_str(), row.source_line))?;
            if declarations.len() == 1 {
                Some((runtime_evidence::identity(row), declarations[0]))
            } else {
                None
            }
        })
        .collect();
    let mut linked = BTreeSet::new();
    let mut declaration_ids = BTreeSet::new();
    for row in &exact.cases {
        let raw_id = artifact::digest(
            [
                SOURCE_COMMIT,
                &row.source_path,
                &row.source_line.to_string(),
                &row.module,
                &row.name,
            ]
            .join("\0")
            .as_bytes(),
        );
        let declaration = expected_exact
            .get(&raw_id)
            .ok_or_else(|| CheckError::new("compiler exact case has no unique source location"))?;
        let original = originals
            .get(&raw_id)
            .ok_or_else(|| CheckError::new("compiler exact raw case absent"))?;
        let expected = artifact::digest(
            [
                SOURCE_COMMIT,
                declaration.id.as_str(),
                &row.source_path,
                &row.source_line.to_string(),
                &row.module,
                &row.name,
            ]
            .join("\0")
            .as_bytes(),
        );
        ensure(
            row.source_declaration_id == declaration.id
                && row.case_id == expected
                && row.tags == original.tags
                && linked.insert(raw_id),
            "compiler exact-case source/tag identity differs",
        )?;
        declaration_ids.insert(declaration.id.as_str());
    }
    ensure(
        linked.len() == expected_exact.len(),
        "compiler exact-case complement incomplete",
    )?;
    Ok((linked, declaration_ids))
}

fn candidate_check(
    capture: &Capture,
    candidate: &CandidateOrigin,
    row: &ObservedCase,
    modules: &BTreeMap<&str, &ModuleProof>,
    versions: &BTreeMap<&str, Vec<&ModuleProof>>,
    calls: &[&TraceCall],
) -> Result<(), CheckError> {
    let macro_row = modules
        .get(candidate.macro_proof_id.as_str())
        .ok_or_else(|| CheckError::new("compiler candidate macro proof absent"))?;
    let consumer = modules
        .get(candidate.consumer_proof_id.as_str())
        .ok_or_else(|| CheckError::new("compiler candidate consumer proof absent"))?;
    for hash in [
        &candidate.source_sha256,
        &candidate.registration_sha256,
        &candidate.compiled_quote_sha256,
        &candidate.source_quote_sha256,
        &candidate.compiled_module_sha256,
    ] {
        ensure(
            artifact::valid_hex(hash, 64),
            "compiler candidate digest malformed",
        )?;
    }
    ensure(
        candidate.origin_rule == RULE
            && candidate.compiled_quote_sha256 == candidate.source_quote_sha256
            && candidate.compiled_module_sha256 == macro_row.beam.sha256
            && macro_row.definition_matches_debug
            && candidate.macro_module == macro_row.module
            && candidate.source_path == macro_row.source_path.as_deref().unwrap_or("")
            && Some(candidate.source_sha256.as_str()) == macro_row.source_sha256.as_deref()
            && macro_row.source.is_some()
            && candidate.macro_definition_line > 0
            && macro_row
                .module_definition_line
                .is_some_and(|line| line <= candidate.macro_definition_line),
        "compiler candidate macro source/whole-quote disagreement",
    )?;
    ensure(
        candidate.consumer_module == row.module
            && candidate.registration_path == row.source_path
            && candidate.registration_line == row.source_line
            && candidate.name == row.name
            && row.consumer_proof_id.as_deref() == Some(candidate.consumer_proof_id.as_str())
            && consumer.module == row.module
            && consumer.source_path.as_deref() == Some(row.source_path.as_str())
            && consumer.source_sha256.as_deref() == Some(candidate.registration_sha256.as_str())
            && consumer.source.is_some()
            && latest(versions, &consumer.module, None)
                .is_some_and(|latest| latest.proof_id == consumer.proof_id),
        "compiler candidate full consumer/version disagreement",
    )?;
    let uses: Vec<_> = calls
        .iter()
        .filter(|call| {
            call.name == "__using__"
                && call.arity == 1
                && call.consumer_module == row.module
                && call.source_path == row.source_path
                && call.source_line == row.source_line
                && call.target_proof_id.is_some()
        })
        .collect();
    ensure(uses.len() == 1, "compiler candidate use identity ambiguous")?;
    let using = uses[0];
    ensure(
        using.sequence == candidate.use_sequence
            && ["remote_macro", "imported_macro"].contains(&using.kind.as_str())
            && using.target_module == macro_row.module
            && using.target_proof_id.as_deref() == Some(candidate.macro_proof_id.as_str())
            && using.current_version
            && using.target_loaded_md5.as_deref() == Some(macro_row.loaded_md5.as_str())
            && macro_row.sequence < using.sequence
            && using.sequence < consumer.sequence
            && latest(versions, &macro_row.module, Some(using.sequence))
                .is_some_and(|latest| latest.proof_id == macro_row.proof_id),
        "compiler candidate resolved use/current QName disagreement",
    )?;
    let calls: Vec<_> = calls
        .iter()
        .filter(|call| {
            call.consumer_module == row.module
                && call.source_path == row.source_path
                && call.source_line == row.source_line
                && using.sequence < call.sequence
                && call.sequence < consumer.sequence
                && ["test", "describe", "setup", "setup_all"].contains(&call.name.as_str())
        })
        .collect();
    ensure(
        calls
            .iter()
            .any(|call| call.name == "test" && [2, 3].contains(&call.arity))
            && calls.iter().all(|call| {
                call.genuine_case
                    && ["remote_macro", "imported_macro"].contains(&call.kind.as_str())
                    && call.target_module == "Elixir.ExUnit.Case"
                    && call.framework_sha256.as_deref()
                        == Some(capture.framework.beam.sha256.as_str())
                    && call.target_loaded_md5.as_deref()
                        == Some(capture.framework.loaded_md5.as_str())
            }),
        "compiler DSL authority/delegation disagreement",
    )?;
    Ok(())
}

fn version_index(capture: &Capture) -> BTreeMap<&str, Vec<&ModuleProof>> {
    let mut versions: BTreeMap<&str, Vec<&ModuleProof>> = BTreeMap::new();
    for row in &capture.modules {
        versions.entry(&row.module).or_default().push(row);
    }
    for rows in versions.values_mut() {
        rows.sort_by_key(|row| row.sequence);
    }
    versions
}

fn trace_index(capture: &Capture) -> BTreeMap<(&str, &str, usize), Vec<&TraceCall>> {
    let mut calls: BTreeMap<(&str, &str, usize), Vec<&TraceCall>> = BTreeMap::new();
    for row in &capture.calls {
        calls
            .entry((&row.consumer_module, &row.source_path, row.source_line))
            .or_default()
            .push(row);
    }
    calls
}

fn latest<'a>(
    versions: &BTreeMap<&str, Vec<&'a ModuleProof>>,
    module: &str,
    before: Option<u64>,
) -> Option<&'a ModuleProof> {
    let rows = versions.get(module)?;
    let count = before.map_or(rows.len(), |sequence| {
        rows.partition_point(|row| row.sequence < sequence)
    });
    count
        .checked_sub(1)
        .and_then(|index| rows.get(index).copied())
}

#[cfg(test)]
#[test]
fn unmatched_complements_cannot_change_original_tags() -> Result<(), CheckError> {
    let mut row = serde_json::json!({"source_path":"source/consumer.exs","source_line":3,
        "module":"Elixir.Synthetic","name":"test synthetic","tags":{"cluster":true}});
    let raw: Vec<RawCase> = serde_json::from_value(serde_json::json!([row]))
        .map_err(|error| CheckError::new(error.to_string()))?;
    let originals = original_index(&raw)?;
    assert!(
        check_unmatched(
            &serde_json::json!({"unmatched_runtime_cases":[row]}),
            &originals,
            &BTreeSet::new()
        )
        .is_ok()
    );
    row["tags"]["cluster"] = Value::Bool(false);
    assert!(
        check_unmatched(
            &serde_json::json!({"unmatched_runtime_cases":[row]}),
            &originals,
            &BTreeSet::new()
        )
        .is_err()
    );
    Ok(())
}
