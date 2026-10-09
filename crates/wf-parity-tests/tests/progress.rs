//! Adversarial progress validation, separate from immutable full-parity evidence.

#![forbid(unsafe_code)]

use std::{error::Error, fs, path::Path, process::Command};

use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use wf_parity_tests::{REFERENCE_MANIFEST_SHA256, SOURCE_COMMIT, workspace_root};
use wf_test_support::resources::TestResources;

type TestResult = Result<(), Box<dyn Error>>;

const PROGRESS: &str = "tests/parity/workflow-progress.json";
const COLLECTION: &str = "tests/parity/registration-fixture.json";
const RUST_FILE: &str = "crates/wf-types/tests/progress_fixture.rs";
const RUST_SOURCE: &str = "mod fencing {\n    #[test]\n    fn complete_retry() {\n        let completed_events = 1;\n        assert_eq!(completed_events, 1);\n    }\n}\n";

fn sha256(bytes: &[u8]) -> String {
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
    source: Value,
}

impl Fixture {
    fn new() -> Result<Self, Box<dyn Error>> {
        let resources = TestResources::new()?;
        copy_tree(
            &workspace_root().join("tests/reference"),
            &resources.path().join("tests/reference"),
        )?;
        fs::create_dir_all(resources.path().join("tests/parity"))?;
        let manifest: Value = serde_json::from_slice(include_bytes!(
            "../../../tests/reference/workflow-test-manifest.json"
        ))?;
        let declarations = manifest["declarations"].as_array().ok_or("declarations")?;
        let source = declarations
            .iter()
            .find(|item| {
                item["source_path"]
                    .as_str()
                    .is_some_and(|path| path.ends_with("flow/fencing_retry_test.exs"))
            })
            .ok_or("fencing declaration")?
            .clone();
        let ledger = json!({
            "schema_version": 1,
            "source_commit": SOURCE_COMMIT,
            "reference_manifest_sha256": REFERENCE_MANIFEST_SHA256,
            "runtime_collection": null,
            "declarations": declarations.iter().map(|item| json!({
                "source_declaration_id": item["id"],
                "runtime_case_ids": [],
                "rust_assertions": []
            })).collect::<Vec<_>>()
        });
        Ok(Self {
            resources,
            ledger,
            source,
        })
    }

    fn index(&self) -> Result<usize, Box<dyn Error>> {
        self.ledger["declarations"]
            .as_array()
            .ok_or("ledger declarations")?
            .iter()
            .position(|item| item["source_declaration_id"] == self.source["id"])
            .ok_or_else(|| "missing fencing lineage".into())
    }

    fn write(&self) -> TestResult {
        fs::write(
            self.resources.path().join(PROGRESS),
            serde_json::to_vec(&self.ledger)?,
        )?;
        Ok(())
    }

    fn output(&self) -> Result<std::process::Output, Box<dyn Error>> {
        self.write()?;
        Ok(Command::new(env!("CARGO_BIN_EXE_wf-parity-tests"))
            .arg("check-progress")
            .arg("--root")
            .arg(self.resources.path())
            .output()?)
    }

    fn rejected(&self, diagnostic: &str) -> TestResult {
        let output = self.output()?;
        assert!(!output.status.success(), "accepted malformed progress");
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            stderr.contains(diagnostic),
            "expected {diagnostic:?}: {stderr}"
        );
        Ok(())
    }

    fn add_assertion(&mut self) -> TestResult {
        let path = self.resources.path().join(RUST_FILE);
        fs::create_dir_all(path.parent().ok_or("Rust parent")?)?;
        fs::write(path, RUST_SOURCE)?;
        let source_path = self.source["source_path"].as_str().ok_or("source path")?;
        let source = fs::read_to_string(
            self.resources
                .path()
                .join("tests/reference/elixir-baseline")
                .join(source_path),
        )?;
        let (line, text) = source
            .lines()
            .enumerate()
            .find(|(_, text)| text.contains("assert history_event_count"))
            .ok_or("history assertion")?;
        let index = self.index()?;
        self.ledger["declarations"][index]["rust_assertions"] = json!([{
            "scenario_id": "complete-retry-history",
            "rust_file": RUST_FILE,
            "rust_file_sha256": sha256(RUST_SOURCE.as_bytes()),
            "rust_module": "fencing",
            "rust_function": "complete_retry",
            "assertions": [{
                "source_line": line + 1,
                "source_text": text.trim(),
                "rust_line": 5,
                "rust_text": "assert_eq!(completed_events, 1);"
            }],
            "limitations": ["Synthetic temporary fixture for validator tests, not behavioral evidence."]
        }]);
        Ok(())
    }

    fn collection(&mut self) -> Result<Value, Box<dyn Error>> {
        let id = self.source["id"].as_str().ok_or("source ID")?;
        let path = self.source["source_path"].as_str().ok_or("source path")?;
        let line = self.source["line"]
            .as_u64()
            .ok_or("source line")?
            .to_string();
        let module = "Elixir.Ferricstore.Flow.FencingRetryTest";
        let name = "test complete retry after committed response loss is a no-op success";
        let identity = [SOURCE_COMMIT, id, path, &line, module, name].join("\0");
        let case_id = sha256(identity.as_bytes());
        let index = self.index()?;
        self.ledger["declarations"][index]["runtime_case_ids"] = json!([case_id]);
        let unresolved = self.ledger["declarations"]
            .as_array()
            .ok_or("entries")?
            .iter()
            .filter(|entry| entry["source_declaration_id"] != id)
            .map(|entry| entry["source_declaration_id"].clone())
            .collect::<Vec<_>>();
        Ok(json!({
            "schema_version": 1,
            "source_commit": SOURCE_COMMIT,
            "reference_manifest_sha256": REFERENCE_MANIFEST_SHA256,
            "collection_kind": "exunit-registration-only",
            "collection_complete": false,
            "blockers": ["synthetic temporary fixture; registration not execution"],
            "unresolved_declaration_ids": unresolved,
            "cases": [{
                "case_id": case_id,
                "source_declaration_id": id,
                "source_path": path,
                "source_line": self.source["line"],
                "module": module,
                "name": name,
                "tags": {}
            }]
        }))
    }

    fn write_collection(&mut self, collection: &Value) -> TestResult {
        let bytes = serde_json::to_vec(collection)?;
        fs::write(self.resources.path().join(COLLECTION), &bytes)?;
        self.ledger["runtime_collection"] = json!({"path": COLLECTION, "sha256": sha256(&bytes)});
        Ok(())
    }
}

#[test]
fn all_source_ids_without_collection_are_explicitly_incomplete() -> TestResult {
    let fixture = Fixture::new()?;
    let output = fixture.output()?;
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("5858"));
    assert!(stdout.contains("runtime reconciliation incomplete: collection absent"));
    assert!(stdout.contains("full parity incomplete"));
    Ok(())
}

#[test]
fn missing_ledger_is_a_setup_failure() -> TestResult {
    let fixture = Fixture::new()?;
    let output = Command::new(env!("CARGO_BIN_EXE_wf-parity-tests"))
        .args(["check-progress", "--root"])
        .arg(fixture.resources.path())
        .output()?;
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("workflow-progress.json"));
    Ok(())
}

#[test]
fn dropped_duplicate_and_fabricated_source_ids_are_rejected() -> TestResult {
    let mut fixture = Fixture::new()?;
    let original = fixture.ledger["declarations"].clone();
    fixture.ledger["declarations"]
        .as_array_mut()
        .ok_or("entries")?
        .remove(0);
    fixture.rejected("missing or unknown source declaration IDs")?;
    fixture.ledger["declarations"] = original.clone();
    fixture.ledger["declarations"][0]["source_declaration_id"] =
        fixture.ledger["declarations"][1]["source_declaration_id"].clone();
    fixture.rejected("duplicate source declaration ID")?;
    fixture.ledger["declarations"] = original;
    fixture.ledger["declarations"][0]["source_declaration_id"] = "0000000000000000".into();
    fixture.rejected("missing or unknown source declaration IDs")
}

#[test]
fn handwritten_status_unknown_fields_and_provenance_changes_cannot_pass() -> TestResult {
    let mut fixture = Fixture::new()?;
    fixture.ledger["declarations"][0]["status"] = "passing".into();
    fixture.rejected("unknown field")?;
    fixture.ledger["declarations"][0]
        .as_object_mut()
        .ok_or("entry")?
        .remove("status");
    fixture.ledger["execution"] = json!({"passed": 5858});
    fixture.rejected("unknown field")?;
    fixture
        .ledger
        .as_object_mut()
        .ok_or("ledger")?
        .remove("execution");
    fixture.ledger["source_commit"] = "different-source".into();
    fixture.rejected("progress provenance")?;
    fixture.ledger["source_commit"] = SOURCE_COMMIT.into();
    fixture.ledger["reference_manifest_sha256"] = "0".repeat(64).into();
    fixture.rejected("progress provenance")
}

#[test]
fn actual_source_assertion_and_scoped_rust_test_can_be_mapped_without_execution_claim() -> TestResult
{
    let mut fixture = Fixture::new()?;
    fixture.add_assertion()?;
    let output = fixture.output()?;
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("1 source-derived declaration"));
    assert!(stdout.contains("full parity incomplete"));
    Ok(())
}

#[test]
fn missing_tampered_wrong_scoped_and_comment_only_counterparts_are_rejected() -> TestResult {
    let mut fixture = Fixture::new()?;
    fixture.add_assertion()?;
    let index = fixture.index()?;
    fixture.ledger["declarations"][index]["rust_assertions"][0]["rust_file"] =
        "crates/wf-types/tests/missing.rs".into();
    fixture.rejected("cannot inspect")?;
    fixture.ledger["declarations"][index]["rust_assertions"][0]["rust_file"] = RUST_FILE.into();
    fs::write(
        fixture.resources.path().join(RUST_FILE),
        format!("{RUST_SOURCE}// changed\n"),
    )?;
    fixture.rejected("Rust counterpart SHA-256 mismatch")?;
    fs::write(fixture.resources.path().join(RUST_FILE), RUST_SOURCE)?;
    fixture.ledger["declarations"][index]["rust_assertions"][0]["rust_module"] = "other".into();
    fixture.rejected("scoped Rust test")?;
    fixture.ledger["declarations"][index]["rust_assertions"][0]["rust_module"] = "fencing".into();
    let comment = format!("/*\n{RUST_SOURCE}*/\n");
    fs::write(fixture.resources.path().join(RUST_FILE), &comment)?;
    fixture.ledger["declarations"][index]["rust_assertions"][0]["rust_file_sha256"] =
        sha256(comment.as_bytes()).into();
    fixture.rejected("scoped Rust test")
}

#[test]
fn false_empty_cross_declaration_and_nonassertion_links_are_rejected() -> TestResult {
    let mut fixture = Fixture::new()?;
    fixture.add_assertion()?;
    let index = fixture.index()?;
    let original = fixture.ledger["declarations"][index]["rust_assertions"][0].clone();
    fixture.ledger["declarations"][index]["rust_assertions"][0]["assertions"] = json!([]);
    fixture.rejected("empty assertion mapping")?;
    fixture.ledger["declarations"][index]["rust_assertions"][0] = original.clone();
    fixture.ledger["declarations"][index]["rust_assertions"][0]["assertions"][0]["source_text"] =
        "assert fabricated == true".into();
    fixture.rejected("source assertion")?;
    fixture.ledger["declarations"][index]["rust_assertions"][0] = original.clone();
    fixture.ledger["declarations"][index]["rust_assertions"][0]["assertions"][0]["source_line"] =
        28.into();
    fixture.rejected("source assertion")?;
    fixture.ledger["declarations"][index]["rust_assertions"][0] = original;
    fixture.ledger["declarations"][index]["rust_assertions"][0]["assertions"][0]["rust_line"] =
        4.into();
    fixture.ledger["declarations"][index]["rust_assertions"][0]["assertions"][0]["rust_text"] =
        "let completed_events = 1;".into();
    fixture.rejected("Rust assertion")
}

#[test]
fn runtime_case_ids_require_collection_and_bidirectional_registration() -> TestResult {
    let mut fixture = Fixture::new()?;
    let mut collection = fixture.collection()?;
    fixture.rejected("runtime case IDs require collection")?;
    fixture.write_collection(&collection)?;
    let output = fixture.output()?;
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains("1 registered runtime case"));
    let index = fixture.index()?;
    fixture.ledger["declarations"][index]["runtime_case_ids"] = json!([]);
    fixture.rejected("runtime case reconciliation")?;
    fixture.ledger["declarations"][index]["runtime_case_ids"] =
        json!([collection["cases"][0]["case_id"]]);
    collection["unresolved_declaration_ids"]
        .as_array_mut()
        .ok_or("unresolved")?
        .pop();
    fixture.write_collection(&collection)?;
    fixture.rejected("unresolved declaration accounting")
}

#[test]
fn altered_collection_hash_identity_duplicate_and_zero_case_complete_are_rejected() -> TestResult {
    let mut fixture = Fixture::new()?;
    let original = fixture.collection()?;
    fixture.write_collection(&original)?;
    fixture.ledger["runtime_collection"]["sha256"] = "0".repeat(64).into();
    fixture.rejected("collection SHA-256 mismatch")?;
    let mut collection = original.clone();
    collection["cases"][0]["module"] = "Elixir.OtherTest".into();
    fixture.write_collection(&collection)?;
    fixture.rejected("registered case identity")?;
    collection = original.clone();
    let case = collection["cases"][0].clone();
    collection["cases"]
        .as_array_mut()
        .ok_or("cases")?
        .push(case);
    fixture.write_collection(&collection)?;
    fixture.rejected("duplicate registered case")?;
    collection = original;
    collection["collection_complete"] = true.into();
    collection["blockers"] = json!([]);
    collection["cases"] = json!([]);
    collection["unresolved_declaration_ids"] = json!([]);
    fixture.write_collection(&collection)?;
    fixture.rejected("complete collection cannot have zero cases")
}

#[test]
fn unrecognized_collection_kind_and_handwritten_case_result_are_rejected() -> TestResult {
    let mut fixture = Fixture::new()?;
    let mut collection = fixture.collection()?;
    collection["collection_kind"] = "executed-and-passing".into();
    fixture.write_collection(&collection)?;
    fixture.rejected("registration provenance")?;
    collection["collection_kind"] = "exunit-registration-only".into();
    collection["cases"][0]["result"] = "passing".into();
    fixture.write_collection(&collection)?;
    fixture.rejected("unknown field")
}
