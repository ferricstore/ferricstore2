//! Actual CLI integrity checks and expected fail-closed results.

#![forbid(unsafe_code)]

use std::{error::Error, fs, path::Path, process::Command};

use wf_parity_tests::{check_reference, require_parity, workspace_root};
use wf_test_support::resources::TestResources;

type TestResult = Result<(), Box<dyn Error>>;

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

#[test]
fn preserved_artifacts_pass_but_full_workflow_parity_fails() -> TestResult {
    let summary = check_reference(&workspace_root())?;
    assert_eq!(summary.files, 480);
    assert_eq!(summary.declarations, 5_858);
    assert_eq!(summary.working_tree_changes, 9);
    assert_eq!(summary.static_require_edges, 88);
    let error = require_parity(&workspace_root())
        .err()
        .ok_or("M0 must not pass parity")?;
    assert!(
        error
            .to_string()
            .contains("ExUnit runtime cases have not been collected")
    );
    Ok(())
}

#[test]
fn cli_exit_status_distinguishes_preservation_and_parity() -> TestResult {
    let reference = Command::new(env!("CARGO_BIN_EXE_wf-parity-tests"))
        .arg("check-reference")
        .output()?;
    assert!(
        reference.status.success(),
        "{}",
        String::from_utf8_lossy(&reference.stderr)
    );
    assert!(String::from_utf8_lossy(&reference.stdout).contains("M0 artifact integrity verified"));
    let parity = Command::new(env!("CARGO_BIN_EXE_wf-parity-tests"))
        .arg("require-parity")
        .output()?;
    assert!(!parity.status.success());
    assert!(String::from_utf8_lossy(&parity.stderr).contains("workflow parity incomplete"));
    Ok(())
}

#[test]
fn cli_rejects_unknown_commands_and_ambiguous_arguments() -> TestResult {
    for args in [
        vec![],
        vec!["pretend-passing"],
        vec!["check-reference", "--root"],
        vec!["check-reference", "--skip-integrity"],
        vec!["check-reference", "--root", ".", "extra"],
    ] {
        let output = Command::new(env!("CARGO_BIN_EXE_wf-parity-tests"))
            .args(args)
            .output()?;
        assert!(!output.status.success());
        assert!(String::from_utf8_lossy(&output.stderr).contains("usage:"));
    }
    Ok(())
}

#[test]
fn missing_reference_setup_has_a_distinct_failure_from_incomplete_parity() -> TestResult {
    let resources = TestResources::new()?;
    for command in ["check-reference", "require-parity"] {
        let output = Command::new(env!("CARGO_BIN_EXE_wf-parity-tests"))
            .arg(command)
            .arg("--root")
            .arg(resources.path())
            .output()?;
        assert!(!output.status.success());
        let diagnostic = String::from_utf8_lossy(&output.stderr);
        assert!(diagnostic.contains("cannot inspect"));
        assert!(!diagnostic.contains("workflow parity incomplete"));
    }
    Ok(())
}

#[test]
fn changed_missing_extra_and_dirty_snapshot_artifacts_fail_integrity() -> TestResult {
    let resources = TestResources::new()?;
    let target = resources.path().join("tests/reference");
    copy_tree(&workspace_root().join("tests/reference"), &target)?;
    check_reference(resources.path())?;
    let source = target.join("elixir-baseline/test/fixtures/flow_query_result_v1.json");
    let bytes = fs::read(&source)?;
    fs::write(&source, b"changed")?;
    assert!(check_reference(resources.path()).is_err());
    fs::write(&source, &bytes)?;
    fs::remove_file(&source)?;
    assert!(check_reference(resources.path()).is_err());
    fs::write(&source, &bytes)?;
    let extra = target.join("elixir-baseline/unmanifested.exs");
    fs::write(&extra, b"not inventoried")?;
    assert!(check_reference(resources.path()).is_err());
    fs::remove_file(extra)?;
    let dirty = target.join(
        "elixir-working-tree/apps/ferricstore/test/ferricstore/flow/lmdb_index_read_test.exs",
    );
    fs::write(dirty, b"changed working-tree input")?;
    assert!(check_reference(resources.path()).is_err());
    Ok(())
}

#[test]
fn changed_manifest_and_missing_mapping_fail_closed() -> TestResult {
    let resources = TestResources::new()?;
    let target = resources.path().join("tests/reference");
    copy_tree(&workspace_root().join("tests/reference"), &target)?;
    assert!(require_parity(resources.path()).is_err());
    let manifest_path = target.join("workflow-test-manifest.json");
    let bytes = fs::read(&manifest_path)?;
    let mut manifest: serde_json::Value = serde_json::from_slice(&bytes)?;
    manifest["declarations"][0]["port_status"] = "passing".into();
    fs::write(&manifest_path, serde_json::to_vec(&manifest)?)?;
    let error = check_reference(resources.path())
        .err()
        .ok_or("modified manifest must fail")?;
    assert!(error.to_string().contains("manifest SHA-256 differs"));
    Ok(())
}
