//! Honest M0 lineage inventory. Runtime collection/evidence formats are future work.

use std::{collections::BTreeSet, path::Path};

use serde::Deserialize;

use crate::{CheckError, REFERENCE_MANIFEST_SHA256, artifact, reference::Manifest};

const MAPPING_PATH: &str = "tests/parity/workflow-mapping.json";

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Mapping {
    schema_version: u32,
    stage: String,
    reference_manifest_sha256: String,
    runtime_collection: MissingEvidence,
    rust_execution: MissingEvidence,
    declarations: Vec<MappingEntry>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct MissingEvidence {
    status: String,
    reason: String,
    evidence: serde_json::Value,
    cases: Vec<serde_json::Value>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct MappingEntry {
    source_declaration_id: String,
    status: String,
    runtime_case_ids: Vec<String>,
    rust_counterparts: Vec<serde_json::Value>,
}

pub(crate) fn check(root: &Path, manifest: &Manifest) -> Result<(), CheckError> {
    let bytes = artifact::read(root, MAPPING_PATH, 8 * 1024 * 1024)?;
    let mapping: Mapping = serde_json::from_slice(&bytes)
        .map_err(|error| CheckError::new(format!("invalid M0 parity mapping: {error}")))?;
    validate(&mapping, manifest)
}

fn validate(mapping: &Mapping, manifest: &Manifest) -> Result<(), CheckError> {
    if mapping.schema_version != 1
        || mapping.stage != "m0-infrastructure"
        || mapping.reference_manifest_sha256 != REFERENCE_MANIFEST_SHA256
        || mapping.runtime_collection.status != "not-collected"
        || mapping.rust_execution.status != "not-executed"
    {
        return Err(CheckError::new(
            "unsupported parity mapping schema/stage or fabricated collection/execution status",
        ));
    }
    for evidence in [&mapping.runtime_collection, &mapping.rust_execution] {
        if evidence.reason.is_empty() || !evidence.evidence.is_null() || !evidence.cases.is_empty()
        {
            return Err(CheckError::new(
                "M0 cannot accept manually attached runtime/execution evidence",
            ));
        }
    }
    let expected: BTreeSet<_> = manifest
        .declarations
        .iter()
        .map(|declaration| declaration.id.as_str())
        .collect();
    let mut mapped = BTreeSet::new();
    for entry in &mapping.declarations {
        if !mapped.insert(entry.source_declaration_id.as_str())
            || entry.status != "unported"
            || !entry.runtime_case_ids.is_empty()
            || !entry.rust_counterparts.is_empty()
        {
            return Err(CheckError::new(
                "duplicate lineage ID or manually asserted parity without collected/mapped/executed proof",
            ));
        }
    }
    if mapped != expected {
        return Err(CheckError::new(
            "parity lineage mapping has missing or unknown source declaration IDs",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::error::Error;

    use serde_json::json;

    use super::{Mapping, validate};
    use crate::reference::Manifest;

    fn fixtures() -> Result<(Mapping, Manifest), serde_json::Error> {
        let mapping = serde_json::from_slice(include_bytes!(
            "../../../tests/parity/workflow-mapping.json"
        ))?;
        let manifest = serde_json::from_slice(include_bytes!(
            "../../../tests/reference/workflow-test-manifest.json"
        ))?;
        Ok((mapping, manifest))
    }

    #[test]
    fn honest_m0_mapping_covers_all_unported_source_ids() -> Result<(), Box<dyn Error>> {
        let (mapping, manifest) = fixtures()?;
        validate(&mapping, &manifest)?;
        assert_eq!(mapping.declarations.len(), 5_858);
        Ok(())
    }

    #[test]
    fn manual_pass_and_fake_counterparts_cannot_certify_parity() -> Result<(), Box<dyn Error>> {
        let (mut mapping, manifest) = fixtures()?;
        mapping.declarations[0].status = "passing".into();
        assert!(validate(&mapping, &manifest).is_err());
        mapping.declarations[0].status = "unported".into();
        mapping.declarations[0]
            .rust_counterparts
            .push(json!({"test": "pretend", "result": "passed"}));
        assert!(validate(&mapping, &manifest).is_err());
        mapping.declarations[0].rust_counterparts.clear();
        mapping.declarations[0]
            .runtime_case_ids
            .push("invented-exunit-case".into());
        assert!(validate(&mapping, &manifest).is_err());
        Ok(())
    }

    #[test]
    fn fake_collection_or_execution_evidence_is_rejected() -> Result<(), Box<dyn Error>> {
        let (mut mapping, manifest) = fixtures()?;
        mapping.runtime_collection.evidence = json!({"result": "passed", "cases": 5858});
        assert!(validate(&mapping, &manifest).is_err());
        mapping.runtime_collection.evidence = serde_json::Value::Null;
        mapping.runtime_collection.status = "collected".into();
        assert!(validate(&mapping, &manifest).is_err());
        mapping.runtime_collection.status = "not-collected".into();
        mapping
            .rust_execution
            .cases
            .push(json!({"ignored": true, "result": "passed"}));
        assert!(validate(&mapping, &manifest).is_err());
        Ok(())
    }

    #[test]
    fn missing_unknown_and_duplicate_lineage_cannot_be_hidden() -> Result<(), Box<dyn Error>> {
        let (mut mapping, manifest) = fixtures()?;
        mapping.declarations[0].source_declaration_id =
            mapping.declarations[1].source_declaration_id.clone();
        assert!(validate(&mapping, &manifest).is_err());
        mapping.declarations[0].source_declaration_id = "0000000000000000".into();
        assert!(validate(&mapping, &manifest).is_err());
        mapping.declarations.remove(0);
        assert!(validate(&mapping, &manifest).is_err());
        Ok(())
    }

    #[test]
    fn unknown_fields_and_missing_explicit_evidence_are_schema_errors() {
        assert!(
            serde_json::from_value::<Mapping>(json!({"schema_version":1,"passing":true})).is_err()
        );
        assert!(
            serde_json::from_value::<super::MissingEvidence>(
                json!({"status":"not-collected","reason":"absent","cases":[]})
            )
            .is_err()
        );
    }
}
