//! Executable M0 architecture checks; these do not establish workflow parity.
//!
//! Cargo's declared graph, conservative syntax checks, archunit fixtures, and
//! real compiler visibility checks provide complementary evidence.

#![forbid(unsafe_code)]

mod graph;
mod manifest;
mod policy;
mod size;
mod source;

use std::{error::Error, fs, path::Path, process::Command};

type CheckResult<T> = Result<T, Box<dyn Error>>;

/// Check every declared workspace dependency, manifest, and first-party source.
///
/// Returns policy violations separately from failures to read/parse/inspect the
/// workspace. Cargo metadata is locked, offline, and unfiltered by target or
/// features; its *declared* dependencies include inactive optional/target edges.
///
/// # Errors
/// Returns an error if policy, metadata, manifests, or Rust source cannot be
/// inspected. Technical failures never become a passing empty report.
pub fn check_workspace(root: &Path) -> CheckResult<Vec<String>> {
    let policy = policy::Policy::load(&root.join("architecture/policy.toml"))?;
    let cargo = std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
    let output = Command::new(cargo)
        .args([
            "metadata",
            "--format-version=1",
            "--no-deps",
            "--offline",
            "--locked",
        ])
        .arg("--manifest-path")
        .arg(root.join("Cargo.toml"))
        .output()?;
    if !output.status.success() {
        return Err(format!(
            "Cargo metadata failed: {}",
            String::from_utf8_lossy(&output.stderr)
        )
        .into());
    }
    let metadata: serde_json::Value = serde_json::from_slice(&output.stdout)?;
    let mut violations = graph::check(&metadata, &policy, root)?;
    violations.extend(manifest::check_root(root)?);
    let packages = metadata["packages"]
        .as_array()
        .ok_or("missing Cargo packages")?;
    for package in packages {
        let name = package["name"].as_str().ok_or("missing package name")?;
        let Some(classification) = policy.packages.get(name) else {
            continue;
        };
        let directory = root.join("crates").join(name);
        violations.extend(manifest::check_package(
            &directory.join("Cargo.toml"),
            name,
        )?);
        let dependency_names = graph::dependency_names(package)?;
        let targets = package["targets"]
            .as_array()
            .ok_or("missing Cargo targets")?;
        let target_roots: Vec<_> = targets
            .iter()
            .filter_map(|target| target["src_path"].as_str())
            .collect();
        let mut files = Vec::new();
        source_files(&directory, name == "wf-architecture-tests", &mut files)?;
        if files.is_empty() {
            violations.push(format!("{name}: empty source selection"));
        }
        for file in files {
            let relative = file
                .strip_prefix(&directory)?
                .to_string_lossy()
                .replace('\\', "/");
            let module = source::module_name(&relative);
            let root_file = target_roots.iter().any(|target| Path::new(target) == file);
            let test_file = relative.starts_with("tests/") || relative.starts_with("benches/");
            let context = source::Context {
                package: name,
                module: &module,
                root_file,
                test_file,
                classification,
                policy: &policy,
                dependency_names: &dependency_names,
            };
            let source = fs::read_to_string(&file)?;
            for finding in source::check(&source, &context)? {
                violations.push(format!("{name}/{relative}: {finding}"));
            }
        }
    }
    violations.sort();
    violations.dedup();
    Ok(violations)
}

fn source_files(
    directory: &Path,
    fixture_data: bool,
    files: &mut Vec<std::path::PathBuf>,
) -> CheckResult<()> {
    for entry in fs::read_dir(directory)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_symlink() {
            return Err(format!("unreviewed source symlink: {}", path.display()).into());
        }
        if path.is_dir() {
            let name = entry.file_name();
            if name == "target"
                || name == ".git"
                || (fixture_data && name == "fixtures" && directory.ends_with("tests"))
            {
                continue;
            }
            source_files(&path, fixture_data, files)?;
        } else if path.extension().is_some_and(|extension| extension == "rs") {
            files.push(path);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests;
