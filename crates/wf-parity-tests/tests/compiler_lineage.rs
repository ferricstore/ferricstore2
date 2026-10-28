//! Candidate tooling is separate from preserved accounting and behavioral parity.

#![forbid(unsafe_code)]

use std::{error::Error, fs};

use wf_parity_tests::{check_compiler_lineage, require_parity, workspace_root};

#[test]
fn missing_current_compiler_capture_stays_closed() -> Result<(), Box<dyn Error>> {
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_nanos();
    let root = std::env::temp_dir().join(format!(
        "wf-compiler-artifact-tests-{}-{nonce}",
        std::process::id()
    ));
    fs::create_dir(&root)?;
    let result = (|| -> Result<(), Box<dyn Error>> {
        copy_missing_capture_reference(&workspace_root(), &root)?;
        wf_parity_tests::check_reference(&root)?;
        if !fs::symlink_metadata(root.join("tests/parity/collection/compiler-v1"))
            .is_err_and(|error| error.kind() == std::io::ErrorKind::NotFound)
        {
            return Err("missing-capture fixture unexpectedly contains compiler-v1".into());
        }
        let error = check_compiler_lineage(&root)
            .err()
            .ok_or("missing-capture fixture unexpectedly passed compiler verification")?;
        if !error.to_string().contains("compiler capture prerequisite") {
            return Err(format!("unexpected missing-capture failure: {error}").into());
        }
        Ok(())
    })();
    let cleanup = fs::remove_dir_all(&root);
    result?;
    cleanup?;
    Ok(())
}

fn copy_missing_capture_reference(
    root: &std::path::Path,
    target: &std::path::Path,
) -> Result<(), Box<dyn Error>> {
    wf_parity_tests::check_reference(root)?;
    let read = |relative: &str, limit: u64| -> Result<Vec<u8>, Box<dyn Error>> {
        if relative.is_empty()
            || relative.contains(['\\', ':'])
            || relative.chars().any(char::is_control)
            || relative
                .split('/')
                .any(|part| part.is_empty() || part == "." || part == "..")
        {
            return Err("unsafe missing-capture reference path".into());
        }
        let mut source = root.to_path_buf();
        for component in relative.split('/') {
            source.push(component);
            let metadata = fs::symlink_metadata(&source)?;
            if !metadata.is_file() && !metadata.is_dir() {
                return Err("nonregular or symlinked missing-capture reference path".into());
            }
        }
        let file = fs::File::open(source)?;
        if !file.metadata()?.is_file() {
            return Err("missing-capture reference is not a regular file".into());
        }
        let mut bytes = Vec::new();
        std::io::Read::read_to_end(&mut std::io::Read::take(file, limit + 1), &mut bytes)?;
        if u64::try_from(bytes.len())? > limit {
            return Err("missing-capture reference exceeds its read limit".into());
        }
        Ok(bytes)
    };
    let manifest_path = "tests/reference/workflow-test-manifest.json";
    let manifest_bytes = read(manifest_path, 8 * 1024 * 1024)?;
    let manifest: serde_json::Value = serde_json::from_slice(&manifest_bytes)?;
    let mut paths = vec![
        manifest_path,
        "tests/reference/elixir-dependencies/apps/ferricstore/lib/ferricstore/term_codec.ex",
        "tests/reference/elixir-dependencies/apps/ferricstore/test/ferricstore/term_codec_test.exs",
    ];
    let mut unique = paths
        .iter()
        .copied()
        .collect::<std::collections::BTreeSet<_>>();
    for (key, prefix, count) in [
        ("files", "tests/reference/elixir-baseline/", 480),
        (
            "working_tree_changes",
            "tests/reference/elixir-working-tree/",
            9,
        ),
    ] {
        let entries = manifest[key]
            .as_array()
            .ok_or("invalid reference inventory")?;
        if entries.len() != count {
            return Err("missing-capture reference inventory count differs".into());
        }
        for entry in entries {
            let path = entry["reference_path"]
                .as_str()
                .ok_or("invalid reference path")?;
            if !path.starts_with(prefix) || !unique.insert(path) {
                return Err("missing-capture reference has an unexpected or duplicate path".into());
            }
            paths.push(path);
        }
    }
    let mut total = 0_usize;
    for relative in paths {
        let content;
        let bytes = if relative == manifest_path {
            manifest_bytes.as_slice()
        } else {
            content = read(relative, 16 * 1024 * 1024)?;
            content.as_slice()
        };
        total = total
            .checked_add(bytes.len())
            .ok_or("reference copy byte count overflow")?;
        if total > 16 * 1024 * 1024 {
            return Err("missing-capture reference exceeds its aggregate copy limit".into());
        }
        let destination = target.join(relative);
        fs::create_dir_all(destination.parent().ok_or("reference copy has no parent")?)?;
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(destination)?;
        std::io::Write::write_all(&mut file, bytes)?;
    }
    Ok(())
}

#[test]
fn synthetic_frozen_identity_cannot_form_production_admission() -> Result<(), Box<dyn Error>> {
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_nanos();
    let root = std::env::temp_dir().join(format!(
        "wf-compiler-artifact-tests-{}-{nonce}",
        std::process::id()
    ));
    fs::create_dir_all(root.join("tests/reference"))?;
    fs::create_dir_all(root.join("capture"))?;
    fs::write(
        root.join("tests/reference/workflow-test-manifest.json"),
        b"{\"synthetic\":true}",
    )?;
    fs::write(
        root.join("capture/compiler-lineage.json"),
        b"{\"admission\":\"proposal-only\"}",
    )?;
    assert!(check_compiler_lineage(&root).is_err());
    fs::remove_dir_all(root)?;
    Ok(())
}

#[test]
fn compiler_candidates_never_open_full_behavioral_parity() {
    assert!(require_parity(&workspace_root()).is_err());
}
