//! Current hash/input bindings and actual aggregate content accounting.

use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
};

use serde_json::Value;

use super::{
    METADATA, NORMALIZATION, RULE,
    model::{Capture, CaptureLimits},
};
use crate::{
    CheckError, REFERENCE_MANIFEST_SHA256, SOURCE_COMMIT, artifact, progress::ArtifactRef,
};

pub(super) const TOOLS: [&str; 9] = [
    "scripts/collect-elixir-cases.exs",
    "scripts/compiler-lineage.exs",
    "scripts/compiler-lineage-data.exs",
    "scripts/compiler-lineage-verifier.exs",
    "scripts/compiler-lineage-artifact.exs",
    "scripts/collect-compiler-lineage.exs",
    "scripts/verify-compiler-lineage.exs",
    "tests/parity/workflow-compiler-lineage.schema.json",
    "scripts/case-metadata.exs",
];
const CAPTURE_ROOT: &str = "tests/parity/collection/compiler-v1";

pub(super) fn check(
    root: &Path,
    capture: &Capture,
    metadata: &[u8],
) -> Result<(Value, Value), CheckError> {
    header(capture)?;
    let (details, registration) = metadata_refs(root, capture, metadata)?;
    let mut expected_paths = BTreeSet::from([
        METADATA.to_owned(),
        capture.collection_details.path.clone(),
        capture.exact_registration.path.clone(),
    ]);
    let mut content = BTreeMap::new();
    module_content(root, capture, &mut content, &mut expected_paths)?;
    native_content(root, capture, &mut content, &mut expected_paths)?;
    current_inputs(root, capture, &details, &mut content, &mut expected_paths)?;
    ensure(
        artifact::snapshot_paths(root, CAPTURE_ROOT)? == expected_paths,
        "compiler snapshot missing/extra paths",
    )?;
    Ok((details, registration))
}

fn metadata_refs(
    root: &Path,
    capture: &Capture,
    metadata: &[u8],
) -> Result<(Value, Value), CheckError> {
    for (record, path) in capture.tools.iter().zip(TOOLS) {
        ensure(record.path == path, "compiler tool path/order mismatch")?;
        read_ref(root, record, capture.limits.content_bytes)?;
    }
    ensure(
        capture.collector == capture.tools[0],
        "compiler collector binding mismatch",
    )?;
    ensure(
        capture.collection_details.path == format!("{CAPTURE_ROOT}/collection-details.json")
            && capture.exact_registration.path
                == format!("{CAPTURE_ROOT}/runtime-registration.json"),
        "compiler metadata path mismatch",
    )?;
    let details_bytes = read_ref(
        root,
        &capture.collection_details,
        capture.limits.metadata_bytes,
    )?;
    let registration_bytes = read_ref(
        root,
        &capture.exact_registration,
        capture.limits.metadata_bytes,
    )?;
    let total = metadata
        .len()
        .checked_add(details_bytes.len())
        .and_then(|size| size.checked_add(registration_bytes.len()))
        .ok_or_else(|| CheckError::new("compiler metadata sum overflow"))?;
    ensure(
        u64::try_from(total).map_err(|error| CheckError::new(error.to_string()))?
            <= capture.limits.capture_metadata_bytes,
        "compiler metadata aggregate exceeds fixed profile",
    )?;
    let details: Value = serde_json::from_slice(&details_bytes)
        .map_err(|error| CheckError::new(error.to_string()))?;
    let registration: Value = serde_json::from_slice(&registration_bytes)
        .map_err(|error| CheckError::new(error.to_string()))?;
    ensure(
        details["source_commit"] == SOURCE_COMMIT
            && details["test_execution"] == false
            && details["setup_execution"] == false
            && details["mix_environment"] == "test"
            && details["fixture_review"] == "not-performed"
            && details["elixir_version"] == capture.elixir_version
            && details["otp_release"] == capture.otp_release
            && details["mix_lock_sha256"] == capture.mix_lock_sha256
            && details["collector_sha256"] == capture.collector.sha256,
        "compiler raw provenance/execution mismatch",
    )?;
    check_profiles(&details)?;
    Ok((details, registration))
}

fn check_profiles(details: &Value) -> Result<(), CheckError> {
    ensure(
        details["helper_profiles"]
            .as_object()
            .is_some_and(|profiles| profiles.values().all(profile))
            && profile(&details["final_global_profile"])
            && details["helper_results"].is_array()
            && details["root_file_results"].is_array(),
        "compiler helpers/profiles absent",
    )
}

fn profile(value: &Value) -> bool {
    let Some(rows) = value.as_array() else {
        return false;
    };
    let mut names = BTreeSet::new();
    rows.len() == 2
        && rows.iter().all(|row| {
            row.as_array().is_some_and(|pair| {
                pair.len() == 2
                    && pair[0].as_str().is_some_and(|name| {
                        ["exclude", "include"].contains(&name) && names.insert(name)
                    })
                    && pair[1].is_array()
            })
        })
}

fn module_content(
    root: &Path,
    capture: &Capture,
    content: &mut BTreeMap<String, Vec<u8>>,
    paths: &mut BTreeSet<String>,
) -> Result<(), CheckError> {
    for row in &capture.modules {
        ensure(
            artifact::valid_hex(&row.proof_id, 64)
                && artifact::valid_hex(&row.loaded_md5, 32)
                && artifact::valid_hex(&row.definition_sha256, 64)
                && !row.module.is_empty()
                && row.module_definition_line != Some(0),
            "invalid compiler module header",
        )?;
        match (&row.source, &row.source_sha256, &row.source_path) {
            (Some(source), Some(hash), Some(path)) => {
                artifact::portable_path(path)?;
                ensure(source.sha256 == *hash, "compiler source reference mismatch")?;
                store(root, source, "sources", "exs", capture, content, paths)?;
            }
            (None, None, path) => {
                if let Some(path) = path {
                    artifact::portable_path(path)?;
                }
            }
            _ => {
                return Err(CheckError::new(
                    "inconsistent nullable compiler source facts",
                ));
            }
        }
        store(root, &row.beam, "beams", "beam", capture, content, paths)?;
    }
    Ok(())
}

fn native_content(
    root: &Path,
    capture: &Capture,
    content: &mut BTreeMap<String, Vec<u8>>,
    paths: &mut BTreeSet<String>,
) -> Result<(), CheckError> {
    let native_names = [
        "Elixir.ExUnit.Case",
        "Elixir.Code",
        "Elixir.Module",
        "Elixir.Macro",
        "Elixir.JSON",
        "elixir_compiler",
    ];
    let native_rows = std::iter::once(&capture.framework).chain(&capture.compiler_fingerprints);
    for (row, expected) in native_rows.zip(native_names) {
        ensure(
            row.module == expected
                && artifact::valid_hex(&row.loaded_md5, 32)
                && !row.source_path.is_empty(),
            "compiler native fingerprint mismatch",
        )?;
        store(root, &row.beam, "beams", "beam", capture, content, paths)?;
    }
    Ok(())
}

fn current_inputs(
    root: &Path,
    capture: &Capture,
    details: &Value,
    content: &mut BTreeMap<String, Vec<u8>>,
    paths: &mut BTreeSet<String>,
) -> Result<(), CheckError> {
    let source_root = Path::new(&capture.source_checkout);
    ensure(
        source_root.is_absolute() && source_root.canonicalize().is_ok(),
        "compiler source checkout missing",
    )?;
    let manifest: Value = serde_json::from_slice(&artifact::read(
        root,
        "tests/reference/workflow-test-manifest.json",
        capture.limits.metadata_bytes,
    )?)
    .map_err(|error| CheckError::new(error.to_string()))?;
    let mut inputs = BTreeMap::new();
    for file in array(&manifest, "files")? {
        inputs.insert(
            string(file, "source_path")?.to_owned(),
            string(file, "sha256")?.to_owned(),
        );
    }
    inputs.insert("mix.lock".to_owned(), capture.mix_lock_sha256.clone());
    additional_sources(
        root,
        source_root,
        capture,
        details,
        &mut inputs,
        content,
        paths,
    )?;
    ensure(
        capture.current_inputs.len() == inputs.len(),
        "compiler current-input complement differs",
    )?;
    let mut previous = None;
    for record in &capture.current_inputs {
        ensure(
            previous.is_none_or(|path: &str| path < record.path.as_str())
                && inputs.get(&record.path) == Some(&record.sha256),
            "compiler current-input identity/order mismatch",
        )?;
        let bytes = read_ref(source_root, record, capture.limits.content_bytes)?;
        charge(record, bytes, capture, content)?;
        previous = Some(record.path.as_str());
    }
    let helpers = details["helper_source_sha256"]
        .as_object()
        .ok_or_else(|| CheckError::new("compiler helper hashes absent"))?;
    for (path, hash) in helpers {
        ensure(
            inputs
                .get(path)
                .is_some_and(|expected| hash.as_str() == Some(expected)),
            "compiler helper source hash mismatch",
        )?;
    }
    Ok(())
}

fn header(capture: &Capture) -> Result<(), CheckError> {
    ensure(
        capture.schema_version == 1
            && capture.origin_rule == RULE
            && capture.admission == "proposal-only"
            && capture.source_commit == SOURCE_COMMIT
            && capture.reference_manifest_sha256 == REFERENCE_MANIFEST_SHA256
            && capture.normalization == NORMALIZATION
            && !capture.elixir_version.is_empty()
            && !capture.otp_release.is_empty()
            && capture
                .elixir_build_info
                .as_object()
                .is_some_and(|value| !value.is_empty()),
        "invalid compiler proposal header",
    )?;
    ensure(
        capture.limits == CaptureLimits::fixed(),
        "compiler resource profile drift",
    )?;
    let requirements = &capture.requirements;
    ensure(
        !requirements.registration_is_execution
            && !requirements.test_execution
            && !requirements.setup_execution
            && requirements.fixture_review == "pending"
            && requirements.mandatory_profile_execution == "unexecuted",
        "compiler registration cannot certify execution",
    )?;
    ensure(
        !capture.registered.is_empty()
            && capture.registered.len() <= capture.limits.runtime_cases
            && capture.modules.len() <= capture.limits.module_versions
            && capture.calls.len() <= capture.limits.calls
            && capture.tools.len() == TOOLS.len()
            && capture.compiler_fingerprints.len() == 5
            && artifact::valid_hex(&capture.mix_lock_sha256, 64)
            && artifact::valid_hex(&capture.elixir_launcher_sha256, 64),
        "compiler cardinality/current-input mismatch",
    )?;
    let mut sequences = BTreeSet::new();
    for sequence in capture
        .modules
        .iter()
        .map(|row| row.sequence)
        .chain(capture.calls.iter().map(|row| row.sequence))
    {
        ensure(
            sequence > 0 && sequences.insert(sequence),
            "compiler sequences must be positive globally unique integers",
        )?;
    }
    Ok(())
}

fn store(
    root: &Path,
    record: &ArtifactRef,
    kind: &str,
    extension: &str,
    capture: &Capture,
    content: &mut BTreeMap<String, Vec<u8>>,
    paths: &mut BTreeSet<String>,
) -> Result<(), CheckError> {
    ensure(
        record.path == format!("{CAPTURE_ROOT}/{kind}/{}.{}", record.sha256, extension),
        "compiler content address/path mismatch",
    )?;
    let bytes = read_ref(root, record, capture.limits.content_bytes)?;
    charge(record, bytes, capture, content)?;
    paths.insert(record.path.clone());
    Ok(())
}

fn charge(
    record: &ArtifactRef,
    bytes: Vec<u8>,
    capture: &Capture,
    content: &mut BTreeMap<String, Vec<u8>>,
) -> Result<(), CheckError> {
    if let Some(existing) = content.get(&record.sha256) {
        ensure(
            existing == &bytes,
            "compiler duplicate content digest disagreement",
        )?;
    } else {
        let total = content
            .values()
            .try_fold(bytes.len(), |total, value| total.checked_add(value.len()))
            .ok_or_else(|| CheckError::new("compiler unique-byte sum overflow"))?;
        ensure(
            u64::try_from(total).map_err(|error| CheckError::new(error.to_string()))?
                <= capture.limits.aggregate_content_bytes,
            "compiler actual unique content exceeds profile",
        )?;
        content.insert(record.sha256.clone(), bytes);
    }
    Ok(())
}

fn additional_sources(
    root: &Path,
    source_root: &Path,
    capture: &Capture,
    details: &Value,
    inputs: &mut BTreeMap<String, String>,
    content: &mut BTreeMap<String, Vec<u8>>,
    paths: &mut BTreeSet<String>,
) -> Result<(), CheckError> {
    let sources = array(details, "additional_source_files")?;
    let review_ref: ArtifactRef = serde_json::from_value(details["additional_root_review"].clone())
        .map_err(|error| CheckError::new(error.to_string()))?;
    ensure(
        review_ref.path == "tests/parity/collection-root-review.json",
        "compiler supplemental review path changed",
    )?;
    let review: Value =
        serde_json::from_slice(&read_ref(root, &review_ref, capture.limits.metadata_bytes)?)
            .map_err(|error| CheckError::new(error.to_string()))?;
    ensure(
        review["source_commit"] == SOURCE_COMMIT
            && review["scope_decision"] == "pending-case-review"
            && review["registration_is_execution"] == false,
        "compiler supplemental scope changed",
    )?;
    let reviewed: BTreeMap<_, _> = array(&review, "files")?
        .iter()
        .map(|file| {
            Ok((
                string(file, "source_path")?.to_owned(),
                string(file, "sha256")?.to_owned(),
            ))
        })
        .collect::<Result<_, CheckError>>()?;
    ensure(
        sources.len() == reviewed.len(),
        "compiler supplemental complement differs",
    )?;
    let mut seen = BTreeSet::new();
    for source in sources {
        let path = string(source, "source_path")?;
        let hash = string(source, "sha256")?;
        ensure(
            seen.insert(path) && reviewed.get(path).is_some_and(|expected| expected == hash),
            "compiler supplemental source identity mismatch",
        )?;
        let record: ArtifactRef = serde_json::from_value(source["source_ref"].clone())
            .map_err(|error| CheckError::new(error.to_string()))?;
        ensure(
            record.path == format!("{CAPTURE_ROOT}/source-supplement/{path}")
                && record.sha256 == hash,
            "compiler supplemental copy binding mismatch",
        )?;
        let bytes = read_ref(root, &record, capture.limits.content_bytes)?;
        ensure(
            bytes == artifact::read(source_root, path, capture.limits.content_bytes)?,
            "compiler supplemental current bytes differ",
        )?;
        charge(&record, bytes, capture, content)?;
        paths.insert(record.path);
        inputs.insert(path.to_owned(), hash.to_owned());
    }
    Ok(())
}

pub(super) fn read_ref(
    root: &Path,
    record: &ArtifactRef,
    limit: u64,
) -> Result<Vec<u8>, CheckError> {
    ensure(
        artifact::valid_hex(&record.sha256, 64),
        "invalid compiler artifact digest",
    )?;
    let bytes = artifact::read(root, &record.path, limit)?;
    ensure(
        artifact::digest(&bytes) == record.sha256,
        "compiler artifact/current-input hash mismatch",
    )?;
    Ok(bytes)
}

pub(super) fn ensure(condition: bool, message: &str) -> Result<(), CheckError> {
    if condition {
        Ok(())
    } else {
        Err(CheckError::new(message))
    }
}

pub(super) fn array<'a>(value: &'a Value, field: &str) -> Result<&'a Vec<Value>, CheckError> {
    value[field]
        .as_array()
        .ok_or_else(|| CheckError::new(format!("compiler array absent: {field}")))
}

pub(super) fn string<'a>(value: &'a Value, field: &str) -> Result<&'a str, CheckError> {
    value[field]
        .as_str()
        .ok_or_else(|| CheckError::new(format!("compiler string absent: {field}")))
}

#[cfg(test)]
#[test]
fn zero_capture_raised_limits_missing_verifier_and_execution_flags_are_rejected() {
    let mut capture = super::model::synthetic_capture();
    assert!(header(&capture).is_ok()); // Header stage only; synthetic full admission rejects.
    capture.registered.clear();
    assert!(header(&capture).is_err());
    capture = super::model::synthetic_capture();
    capture.limits.content_bytes += 1;
    assert!(header(&capture).is_err());
    capture = super::model::synthetic_capture();
    capture.tools.retain(|tool| tool.path != TOOLS[6]);
    assert!(header(&capture).is_err());
    capture = super::model::synthetic_capture();
    capture.requirements.test_execution = true;
    assert!(header(&capture).is_err());
}

#[cfg(test)]
#[test]
fn original_collector_keyword_profiles_are_preserved() -> Result<(), CheckError> {
    let details: Value = serde_json::from_slice(&artifact::read(
        &crate::workspace_root(),
        "tests/parity/collection/supplemental/collection-details.json",
        CaptureLimits::fixed().metadata_bytes,
    )?)
    .map_err(|error| CheckError::new(error.to_string()))?;
    assert!(check_profiles(&details).is_ok());
    Ok(())
}
