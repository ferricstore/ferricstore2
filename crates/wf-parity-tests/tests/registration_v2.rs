//! Original registration and reviewed source origins must reconcile both ways.

#![forbid(unsafe_code)]

use std::{error::Error, fs, path::Path, process::Command};

use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use wf_parity_tests::workspace_root;
use wf_test_support::resources::TestResources;

type TestResult = Result<(), Box<dyn Error>>;

const LEDGER: &str = "tests/parity/workflow-progress.json";
const COLLECTION: &str = "tests/parity/collection/reconciled-registration.json";
const ORIGINS: &str = "tests/parity/collection/macro-origins.json";
const DETAILS: &str = "tests/parity/collection/supplemental/collection-details.json";
const REVIEW: &str = "tests/parity/macro-origin-review.json";

fn digest(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut result = String::new();
    for byte in Sha256::digest(bytes) {
        result.push(char::from(HEX[usize::from(byte / 16)]));
        result.push(char::from(HEX[usize::from(byte % 16)]));
    }
    result
}

fn copy_tree(source: &Path, target: &Path) -> std::io::Result<()> {
    fs::create_dir_all(target)?;
    for entry in fs::read_dir(source)? {
        let entry = entry?;
        let target = target.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_tree(&entry.path(), &target)?;
        } else {
            fs::copy(entry.path(), target)?;
        }
    }
    Ok(())
}

struct Fixture {
    resources: TestResources,
    ledger: Value,
    collection: Value,
    origins: Value,
    details: Value,
    review: Value,
}

impl Fixture {
    fn new() -> Result<Self, Box<dyn Error>> {
        let resources = TestResources::new()?;
        let root = workspace_root();
        for directory in ["tests/reference", "tests/parity", "scripts"] {
            copy_tree(&root.join(directory), &resources.path().join(directory))?;
        }
        fs::create_dir_all(resources.path().join("docs/reviews"))?;
        fs::copy(
            root.join("docs/reviews/workflow-m1-parity.md"),
            resources.path().join("docs/reviews/workflow-m1-parity.md"),
        )?;
        let read = |path: &str| -> Result<Value, Box<dyn Error>> {
            Ok(serde_json::from_slice(&fs::read(root.join(path))?)?)
        };
        let mut ledger = read(LEDGER)?;
        for entry in ledger["declarations"].as_array_mut().ok_or("entries")? {
            entry["rust_assertions"] = json!([]);
            entry["runtime_case_ids"] = json!([]);
        }
        let mut fixture = Self {
            resources,
            ledger,
            collection: read(COLLECTION)?,
            origins: read(ORIGINS)?,
            details: read(DETAILS)?,
            review: read(REVIEW)?,
        };
        fixture.reconcile_ledger()?;
        Ok(fixture)
    }

    fn reconcile_ledger(&mut self) -> TestResult {
        for entry in self.ledger["declarations"]
            .as_array_mut()
            .ok_or("entries")?
        {
            entry["runtime_case_ids"] = json!(
                self.collection["cases"]
                    .as_array()
                    .ok_or("cases")?
                    .iter()
                    .filter(|case| case["source_declaration_id"] == entry["source_declaration_id"])
                    .map(|case| case["case_id"].clone())
                    .collect::<Vec<_>>()
            );
        }
        Ok(())
    }

    fn save(&self, path: &str, value: &Value) -> Result<Value, Box<dyn Error>> {
        let bytes = serde_json::to_vec(value)?;
        fs::write(self.resources.path().join(path), &bytes)?;
        Ok(json!({"path": path, "sha256": digest(&bytes)}))
    }

    fn output(&mut self) -> Result<std::process::Output, Box<dyn Error>> {
        let details = self.save(DETAILS, &self.details)?;
        self.origins["source_collection"] = details.clone();
        self.origins["review"] = self.save(REVIEW, &self.review)?;
        self.collection["collection_details"] = details;
        self.collection["macro_origins"] = self.save(ORIGINS, &self.origins)?;
        self.ledger["runtime_collection"] = self.save(COLLECTION, &self.collection)?;
        self.save(LEDGER, &self.ledger)?;
        Ok(Command::new(env!("CARGO_BIN_EXE_wf-parity-tests"))
            .args(["check-progress", "--root"])
            .arg(self.resources.path())
            .output()?)
    }

    fn rejected(&mut self, diagnostic: &str) -> TestResult {
        let output = self.output()?;
        assert!(
            !output.status.success(),
            "accepted malformed reconciliation"
        );
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            stderr.contains(diagnostic),
            "expected {diagnostic:?}: {stderr}"
        );
        Ok(())
    }

    fn restore_unproved_origin(&mut self) -> TestResult {
        let previous: Value = serde_json::from_slice(&fs::read(
            self.resources
                .path()
                .join("scripts/tests/fixtures/unproved-prior-origin.json"),
        )?)?;
        assert_eq!(previous["review_only"], true);
        let origin = previous["origin"].clone();
        let case = previous["case"].clone();
        self.origins["origins"]
            .as_array_mut()
            .ok_or("origins")?
            .push(origin);
        self.collection["cases"]
            .as_array_mut()
            .ok_or("cases")?
            .push(case.clone());
        self.collection["unmatched_runtime_case_ids"]
            .as_array_mut()
            .ok_or("unmatched")?
            .retain(|id| *id != case["runtime_case_id"]);
        self.collection["unresolved_declaration_ids"]
            .as_array_mut()
            .ok_or("unresolved")?
            .retain(|id| *id != case["source_declaration_id"]);
        self.reconcile_ledger()?;
        Ok(())
    }
}

#[test]
fn schema2_retains_real_cases_origins_tags_and_explicit_execution_gaps() -> TestResult {
    let mut fixture = Fixture::new()?;
    let output = fixture.output()?;
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("5903 registered runtime cases"));
    assert!(stdout.contains("4222 linked cases"));
    assert!(stdout.contains("1681 unmatched runtime cases"));
    assert!(stdout.contains("1655 unresolved declarations"));
    assert!(stdout.contains("full parity incomplete"));
    Ok(())
}

#[test]
fn every_runtime_identity_must_be_linked_once_or_retained_unmatched() -> TestResult {
    let mut fixture = Fixture::new()?;
    let case = fixture.collection["cases"]
        .as_array_mut()
        .ok_or("cases")?
        .remove(0);
    fixture.collection["unmatched_runtime_case_ids"]
        .as_array_mut()
        .ok_or("unmatched")?
        .push(case["runtime_case_id"].clone());
    fixture.reconcile_ledger()?;
    let id = &case["source_declaration_id"];
    if !fixture.collection["cases"]
        .as_array()
        .ok_or("cases")?
        .iter()
        .any(|other| other["source_declaration_id"] == *id)
    {
        fixture.collection["unresolved_declaration_ids"]
            .as_array_mut()
            .ok_or("unresolved")?
            .push(id.clone());
    }
    if !case["origin_id"].is_null() {
        fixture.origins["origins"]
            .as_array_mut()
            .ok_or("origins")?
            .retain(|origin| origin["origin_id"] != case["origin_id"]);
    }
    let output = fixture.output()?;
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    fixture.collection["unmatched_runtime_case_ids"] = json!([]);
    fixture.rejected("unmatched runtime accounting")
}

#[test]
fn changed_tags_and_duplicate_runtime_identity_cannot_be_linked() -> TestResult {
    let mut fixture = Fixture::new()?;
    let cases = fixture.collection["cases"].clone();
    fixture.collection["cases"][0]["tags"]["invented"] = true.into();
    fixture.rejected("original registered case")?;
    fixture.collection["cases"] = cases;
    fixture.collection["cases"][1]["runtime_case_id"] =
        fixture.collection["cases"][0]["runtime_case_id"].clone();
    fixture.rejected("original registered case")
}

#[test]
fn a_rule_or_scaffold_label_cannot_certify_an_unproved_macro_environment() -> TestResult {
    let mut fixture = Fixture::new()?;
    let origins = fixture.origins.clone();
    fixture.origins["origin_rule"] = "literal-section-origin-v1".into();
    fixture.rejected("unsupported macro origin provenance")?;
    fixture.origins = origins;
    fixture.review["sections"][0]["environment_decision"] = "proven-by-registration".into();
    fixture.rejected("unsupported macro environment decision")
}

#[test]
fn origins_require_reviewed_scope_helper_profiles_and_unexecuted_requirements() -> TestResult {
    let mut fixture = Fixture::new()?;
    let origins = fixture.origins.clone();
    fixture.origins["requirements"]["setup_execution"] = "passed".into();
    fixture.rejected("origin requirements")?;
    fixture.origins = origins.clone();
    fixture.origins["helper_profiles"] = json!({});
    fixture.rejected("helper/profile provenance")?;
    fixture.origins = origins;
    fixture.review["sections"][0]["scope_decision"] = "excluded-and-passing".into();
    fixture.rejected("reviewed scope")
}

#[test]
fn supplemental_root_review_hash_is_required_even_for_unmatched_original_cases() -> TestResult {
    let mut fixture = Fixture::new()?;
    fixture.details["additional_root_review"]["sha256"] = "0".repeat(64).into();
    fixture.rejected("evidence SHA-256 mismatch")
}

#[test]
fn formerly_accepted_macro_origins_with_unproved_environments_must_reject() -> TestResult {
    let mut fixture = Fixture::new()?;
    fixture.restore_unproved_origin()?;
    fixture.rejected("unproved preceding environment")
}
