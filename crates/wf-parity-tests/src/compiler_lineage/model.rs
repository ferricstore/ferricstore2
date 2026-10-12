//! Strict proposal-only metadata; caller flags never confer authority.

use std::collections::BTreeMap;

use serde::{Deserialize, Deserializer};
use serde_json::Value;

use crate::progress::ArtifactRef;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Capture {
    pub schema_version: u32,
    pub origin_rule: String,
    pub admission: String,
    pub source_commit: String,
    pub reference_manifest_sha256: String,
    pub source_checkout: String,
    pub elixir_version: String,
    pub elixir_build_info: Value,
    pub otp_release: String,
    pub normalization: String,
    pub collection_details: ArtifactRef,
    pub exact_registration: ArtifactRef,
    pub collector: ArtifactRef,
    pub tools: Vec<ArtifactRef>,
    pub framework: FrameworkProof,
    pub compiler_fingerprints: Vec<FrameworkProof>,
    pub elixir_launcher_path: String,
    pub elixir_launcher_sha256: String,
    pub mix_lock_sha256: String,
    pub current_inputs: Vec<ArtifactRef>,
    pub limits: CaptureLimits,
    pub modules: Vec<ModuleProof>,
    pub calls: Vec<TraceCall>,
    pub registered: Vec<ObservedCase>,
    pub candidate_origins: Vec<CandidateOrigin>,
    pub unresolved_runtime_case_ids: Vec<String>,
    pub unresolved_declaration_ids: Vec<String>,
    pub requirements: CaptureRequirements,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ModuleProof {
    pub proof_id: String,
    pub sequence: u64,
    pub module: String,
    #[serde(deserialize_with = "nullable")]
    pub module_definition_line: Option<usize>,
    #[serde(deserialize_with = "nullable")]
    pub source_path: Option<String>,
    #[serde(deserialize_with = "nullable")]
    pub source_sha256: Option<String>,
    #[serde(deserialize_with = "nullable")]
    pub source: Option<ArtifactRef>,
    pub beam: ArtifactRef,
    pub loaded_md5: String,
    pub definition_sha256: String,
    pub definition_matches_debug: bool,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct TraceCall {
    pub sequence: u64,
    pub kind: String,
    pub source_path: String,
    pub source_line: usize,
    #[serde(deserialize_with = "nullable")]
    pub column: Option<usize>,
    pub consumer_module: String,
    pub target_module: String,
    pub name: String,
    pub arity: u32,
    #[serde(deserialize_with = "nullable")]
    pub target_proof_id: Option<String>,
    pub current_version: bool,
    #[serde(deserialize_with = "nullable")]
    pub target_loaded_md5: Option<String>,
    pub genuine_case: bool,
    #[serde(deserialize_with = "nullable")]
    pub framework_sha256: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ObservedCase {
    pub runtime_case_id: String,
    pub source_path: String,
    pub source_line: usize,
    pub module: String,
    pub name: String,
    pub tags: BTreeMap<String, Value>,
    #[serde(deserialize_with = "nullable")]
    pub consumer_proof_id: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct CandidateOrigin {
    pub runtime_case_id: String,
    pub source_declaration_id: String,
    pub source_path: String,
    pub source_line: usize,
    pub source_sha256: String,
    pub macro_module: String,
    pub macro_definition_line: usize,
    pub consumer_module: String,
    pub registration_path: String,
    pub registration_line: usize,
    pub registration_sha256: String,
    pub name: String,
    pub compiled_quote_sha256: String,
    pub source_quote_sha256: String,
    pub compiled_module_sha256: String,
    pub macro_proof_id: String,
    pub consumer_proof_id: String,
    pub use_sequence: u64,
    pub origin_rule: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct CaptureRequirements {
    pub registration_is_execution: bool,
    pub test_execution: bool,
    pub setup_execution: bool,
    pub fixture_review: String,
    pub mandatory_profile_execution: String,
}

#[derive(Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub(super) struct CaptureLimits {
    pub content_bytes: u64,
    pub aggregate_content_bytes: u64,
    pub capture_metadata_bytes: u64,
    pub metadata_bytes: u64,
    pub module_versions: usize,
    pub calls: usize,
    pub runtime_cases: usize,
    pub stdout_bytes: u64,
    pub stderr_bytes: u64,
    pub verifier_deadline_ms: u64,
}

impl CaptureLimits {
    pub const fn fixed() -> Self {
        Self {
            content_bytes: 16 * 1024 * 1024,
            aggregate_content_bytes: 256 * 1024 * 1024,
            capture_metadata_bytes: 64 * 1024 * 1024,
            metadata_bytes: 64 * 1024 * 1024,
            module_versions: 10_000,
            calls: 1_000_000,
            runtime_cases: 100_000,
            stdout_bytes: 1024 * 1024,
            stderr_bytes: 1024 * 1024,
            verifier_deadline_ms: 60_000,
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct FrameworkProof {
    pub module: String,
    pub beam: ArtifactRef,
    pub loaded_md5: String,
    pub source_path: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct PureVerification {
    pub schema_version: u32,
    pub capture_sha256: String,
    pub source_commit: String,
    pub elixir_version: String,
    pub otp_release: String,
    pub normalization: String,
    pub registered_cases: usize,
    pub candidate_origins: usize,
    pub unresolved_runtime_cases: usize,
    pub unresolved_declarations: usize,
    pub statistics: VerificationStats,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct VerificationStats {
    pub source_reads: usize,
    pub source_parses: usize,
    pub beam_decodes: usize,
    pub registered_cases: usize,
    pub module_versions: usize,
    pub macro_events: usize,
}

fn nullable<'de, D: Deserializer<'de>, T: Deserialize<'de>>(
    deserializer: D,
) -> Result<Option<T>, D::Error> {
    Option::deserialize(deserializer)
}

#[cfg(test)]
pub(super) fn synthetic_capture() -> Capture {
    let reference = || ArtifactRef {
        path: "capture/beams/synthetic.beam".to_owned(),
        sha256: "0".repeat(64),
    };
    let native = |name: &str| FrameworkProof {
        module: name.to_owned(),
        beam: reference(),
        loaded_md5: "0".repeat(32),
        source_path: "/synthetic".to_owned(),
    };
    Capture {
        schema_version: 1,
        origin_rule: super::RULE.to_owned(),
        admission: "proposal-only".to_owned(),
        source_commit: crate::SOURCE_COMMIT.to_owned(),
        reference_manifest_sha256: crate::REFERENCE_MANIFEST_SHA256.to_owned(),
        source_checkout: "/synthetic".to_owned(),
        elixir_version: "1.19.5".to_owned(),
        elixir_build_info: serde_json::json!({"version":"synthetic"}),
        otp_release: "28".to_owned(),
        normalization: super::NORMALIZATION.to_owned(),
        collection_details: reference(),
        exact_registration: reference(),
        collector: reference(),
        tools: super::refs::TOOLS
            .iter()
            .map(|path| ArtifactRef {
                path: (*path).to_owned(),
                sha256: "0".repeat(64),
            })
            .collect(),
        framework: native("Elixir.ExUnit.Case"),
        compiler_fingerprints: [
            "Elixir.Code",
            "Elixir.Module",
            "Elixir.Macro",
            "Elixir.JSON",
            "elixir_compiler",
        ]
        .into_iter()
        .map(native)
        .collect(),
        elixir_launcher_path: "/synthetic".to_owned(),
        elixir_launcher_sha256: "0".repeat(64),
        mix_lock_sha256: "0".repeat(64),
        current_inputs: vec![],
        limits: CaptureLimits::fixed(),
        modules: vec![],
        calls: vec![],
        registered: vec![ObservedCase {
            runtime_case_id: "0".repeat(64),
            source_path: "source/consumer.exs".to_owned(),
            source_line: 3,
            module: "Elixir.Synthetic".to_owned(),
            name: "test synthetic".to_owned(),
            tags: BTreeMap::new(),
            consumer_proof_id: None,
        }],
        candidate_origins: vec![],
        unresolved_runtime_case_ids: vec!["0".repeat(64)],
        unresolved_declaration_ids: vec![],
        requirements: CaptureRequirements {
            registration_is_execution: false,
            test_execution: false,
            setup_execution: false,
            fixture_review: "pending".to_owned(),
            mandatory_profile_execution: "unexecuted".to_owned(),
        },
    }
}

#[cfg(test)]
#[test]
fn nullable_header_facts_are_required_keys_and_cannot_disappear() {
    let mut row = serde_json::json!({"proof_id":"0".repeat(64),"sequence":1,"module":"Elixir.Synthetic",
        "module_definition_line":null,"source_path":null,"source_sha256":null,"source":null,
        "beam":{"path":"capture/beams/synthetic.beam","sha256":"0".repeat(64)},
        "loaded_md5":"0".repeat(32),"definition_sha256":"0".repeat(64),"definition_matches_debug":false});
    assert!(serde_json::from_value::<ModuleProof>(row.clone()).is_ok());
    if let Some(object) = row.as_object_mut() {
        object.remove("source");
    }
    assert!(serde_json::from_value::<ModuleProof>(row).is_err());
}

#[cfg(test)]
#[test]
fn compiler_schema_required_arrays_are_unique() -> Result<(), crate::CheckError> {
    let schema: Value = serde_json::from_slice(&crate::artifact::read(
        &crate::workspace_root(),
        "tests/parity/workflow-compiler-lineage.schema.json",
        CaptureLimits::fixed().content_bytes,
    )?)
    .map_err(|error| crate::CheckError::new(error.to_string()))?;
    let mut pending = vec![&schema];
    let mut checked = 0;
    let mut duplicates = Vec::new();
    while let Some(value) = pending.pop() {
        match value {
            Value::Object(object) => {
                if let Some(required) = object.get("required") {
                    let names = required.as_array().ok_or_else(|| {
                        crate::CheckError::new("compiler schema required field must be an array")
                    })?;
                    let mut unique = std::collections::BTreeSet::new();
                    for name in names {
                        let name = name.as_str().ok_or_else(|| {
                            crate::CheckError::new("compiler schema required names must be strings")
                        })?;
                        if !unique.insert(name) {
                            duplicates.push(name);
                        }
                    }
                    checked += 1;
                }
                pending.extend(object.values());
            }
            Value::Array(array) => pending.extend(array),
            _ => {}
        }
    }
    assert!(
        checked > 0,
        "compiler schema has no required arrays to check"
    );
    assert!(
        duplicates.is_empty(),
        "duplicate compiler schema required names: {duplicates:?}"
    );
    Ok(())
}
