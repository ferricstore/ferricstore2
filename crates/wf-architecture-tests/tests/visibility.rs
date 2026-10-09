//! Compile actual workspace boundaries from isolated consumer crates.
#![forbid(unsafe_code)]

use std::{
    error::Error,
    fmt::Write as _,
    fs,
    path::Path,
    process::{Command, Output},
};

fn compile(source: &str, dependencies: &[&str]) -> Result<Output, Box<dyn Error>> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()?;
    let directory = tempfile::tempdir()?;
    fs::create_dir(directory.path().join("src"))?;
    let mut manifest = String::from(
        "[package]\nname='architecture-consumer-fixture'\nversion='0.0.0'\nedition='2024'\npublish=false\n[workspace]\n[dependencies]\n",
    );
    for dependency in dependencies {
        let path = root.join("crates").join(dependency);
        // TOML string encoding, passed to a file rather than through a shell.
        writeln!(
            manifest,
            "{dependency} = {{ path = {} }}",
            serde_json::to_string(&path.to_string_lossy())?
        )?;
    }
    fs::write(directory.path().join("Cargo.toml"), manifest)?;
    fs::write(directory.path().join("src/lib.rs"), source)?;
    let lock = Command::new(env!("CARGO"))
        .args(["generate-lockfile", "--offline", "--manifest-path"])
        .arg(directory.path().join("Cargo.toml"))
        .current_dir(&root)
        .output()?;
    assert!(
        lock.status.success(),
        "fixture resolution failed: {}",
        String::from_utf8_lossy(&lock.stderr)
    );
    Ok(Command::new(env!("CARGO"))
        .args([
            "check",
            "--offline",
            "--locked",
            "--message-format=json",
            "--manifest-path",
        ])
        .arg(directory.path().join("Cargo.toml"))
        .arg("--target-dir")
        .arg(root.join("target/architecture-visibility"))
        .current_dir(root)
        .output()?)
}

fn assert_error(output: &Output, expected: &str) -> Result<(), Box<dyn Error>> {
    assert!(!output.status.success(), "forbidden consumer compiled");
    let mut diagnostics = Vec::new();
    for line in String::from_utf8_lossy(&output.stdout).lines() {
        let value: serde_json::Value = serde_json::from_str(line)?;
        if value["reason"] == "compiler-message" && value["message"]["level"] == "error" {
            diagnostics.push(value["message"].clone());
        }
    }
    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic["code"]["code"] == expected
                || diagnostic["message"]
                    .as_str()
                    .is_some_and(|message| message.contains(expected))),
        "expected {expected}, got {diagnostics:?}; stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(())
}

#[test]
fn approved_facades_compile_and_private_facades_fail_for_the_right_reason()
-> Result<(), Box<dyn Error>> {
    let allowed = compile(
        include_str!("fixtures/visibility/positive.rs"),
        &["wf-worker", "wf-application", "wf-domain", "wf-ports"],
    )?;
    assert!(
        allowed.status.success(),
        "positive control: {}",
        String::from_utf8_lossy(&allowed.stderr)
    );
    assert_error(
        &compile(
            include_str!("fixtures/visibility/private-worker.rs"),
            &["wf-worker"],
        )?,
        "E0603",
    )?;
    assert_error(
        &compile(
            include_str!("fixtures/visibility/private-admission.rs"),
            &["wf-application"],
        )?,
        "E0603",
    )?;
    Ok(())
}

#[test]
fn client_adapters_cannot_directly_import_transitive_ports() -> Result<(), Box<dyn Error>> {
    for adapter in ["wf-api-http", "wf-api-tcp"] {
        assert_error(
            &compile(
                include_str!("fixtures/visibility/transitive-ports.rs"),
                &[adapter],
            )?,
            "E0432",
        )?;
    }
    Ok(())
}

#[test]
fn forbid_unsafe_is_enforced_by_rustc_in_fixture_consumer() -> Result<(), Box<dyn Error>> {
    assert_error(
        &compile(include_str!("fixtures/visibility/unsafe-root.rs"), &[])?,
        "unsafe_code",
    )?;
    Ok(())
}
