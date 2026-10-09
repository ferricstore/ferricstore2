use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::Path,
};

use serde_json::Value;

use crate::{
    CheckResult,
    policy::{Package, Policy},
};

pub(crate) fn check(metadata: &Value, policy: &Policy, root: &Path) -> CheckResult<Vec<String>> {
    let mut violations = Vec::new();
    let packages = metadata["packages"]
        .as_array()
        .ok_or("missing Cargo packages")?;
    let members = metadata["workspace_members"]
        .as_array()
        .ok_or("missing workspace members")?;
    let member_ids: BTreeSet<_> = members.iter().filter_map(Value::as_str).collect();
    let mut seen = BTreeSet::new();
    for package in packages {
        let name = text(package, "name")?;
        if !member_ids.contains(text(package, "id")?) {
            continue;
        }
        if !seen.insert(name) {
            violations.push(format!("duplicate first-party package {name}"));
        }
        let Some(classification) = policy.packages.get(name) else {
            violations.push(format!("unclassified workspace package {name}"));
            continue;
        };
        let expected = root.join("crates").join(name).join("Cargo.toml");
        if Path::new(text(package, "manifest_path")?) != expected {
            violations.push(format!("{name}: unexpected manifest path"));
        }
        if package["edition"] != "2024"
            || package["rust_version"] != "1.99"
            || package["publish"]
                .as_array()
                .is_none_or(|publish| !publish.is_empty())
        {
            violations.push(format!(
                "{name}: edition/rust-version/publish policy differs"
            ));
        }
        if !package["features"]
            .as_object()
            .ok_or("missing package features")?
            .is_empty()
        {
            violations.push(format!("{name}: unclassified features (M0 supports default only, with no declared features)"));
        }
        for target in package["targets"]
            .as_array()
            .ok_or("missing package targets")?
        {
            let src = text(target, "src_path")?.replace('\\', "/");
            if src.contains("/tests/fixtures/") {
                violations.push(format!("{name}: fixture data is a compiler target"));
            }
            let kinds = target["kind"].as_array().ok_or("missing target kind")?;
            if kinds
                .iter()
                .any(|kind| kind == "custom-build" || kind == "proc-macro")
            {
                violations.push(format!(
                    "{name}: generated-code/build/proc-macro target requires a new audit gate"
                ));
            }
        }
        for dependency in package["dependencies"]
            .as_array()
            .ok_or("missing declared dependencies")?
        {
            check_edge(
                name,
                classification,
                dependency,
                policy,
                root,
                &mut violations,
            )?;
        }
    }
    for name in policy.packages.keys() {
        if !seen.contains(name.as_str()) {
            violations.push(format!("policy package {name} missing from workspace"));
        }
    }
    for entry in fs::read_dir(root.join("crates"))? {
        let path = entry?.path();
        if path.join("Cargo.toml").is_file() {
            let name = path
                .file_name()
                .ok_or("missing crate directory name")?
                .to_string_lossy();
            if !seen.contains(name.as_ref()) {
                violations.push(format!("unclassified crate directory {name}"));
            }
        }
    }
    Ok(violations)
}

fn check_edge(
    owner: &str,
    classification: &Package,
    dependency: &Value,
    policy: &Policy,
    root: &Path,
    violations: &mut Vec<String>,
) -> CheckResult<()> {
    let name = text(dependency, "name")?;
    let kind = dependency["kind"].as_str().unwrap_or("normal");
    if !matches!(kind, "normal" | "dev" | "build") {
        return Err(format!("unknown dependency kind {kind}").into());
    }
    let suffix = format!(
        "{kind}, optional={}, target={}",
        dependency["optional"], dependency["target"]
    );
    if let Some(target) = policy.packages.get(name) {
        let allowed = match kind {
            "normal" => &classification.normal,
            "dev" => &classification.dev,
            _ => &Vec::new(),
        };
        if !allowed.iter().any(|candidate| candidate == name)
            || (classification.runtime && !target.runtime && kind != "dev")
        {
            violations.push(format!(
                "forbidden declared edge {owner} -> {name} ({suffix})"
            ));
        }
        let expected = root.join("crates").join(name);
        if dependency["path"]
            .as_str()
            .is_none_or(|path| Path::new(path) != expected)
            || !dependency["source"].is_null()
        {
            violations.push(format!(
                "{owner}: first-party {name} must use its workspace path"
            ));
        }
        if !dependency["features"]
            .as_array()
            .ok_or("missing dependency features")?
            .is_empty()
        {
            violations.push(format!(
                "{owner} -> {name}: unclassified dependency features"
            ));
        }
    } else {
        let external = match kind {
            "normal" => &classification.external_normal,
            "dev" => &classification.external_dev,
            _ => &Vec::new(),
        };
        let Some(approved) = external.iter().find(|approved| approved.name == name) else {
            violations.push(format!(
                "unclassified external edge {owner} -> {name} ({suffix})"
            ));
            return Ok(());
        };
        let actual_features: BTreeSet<_> = dependency["features"]
            .as_array()
            .ok_or("missing external features")?
            .iter()
            .filter_map(Value::as_str)
            .collect();
        let expected_features: BTreeSet<_> = approved.features.iter().map(String::as_str).collect();
        if dependency["req"] != approved.version
            || actual_features != expected_features
            || dependency["uses_default_features"].as_bool() != Some(approved.default_features)
            || dependency["source"] != "registry+https://github.com/rust-lang/crates.io-index"
            || !dependency["path"].is_null()
        {
            violations.push(format!(
                "{owner} -> {name}: unreviewed external version/source/features"
            ));
        }
    }
    Ok(())
}

pub(crate) fn dependency_names(package: &Value) -> CheckResult<BTreeMap<String, String>> {
    let mut names = BTreeMap::new();
    for dependency in package["dependencies"]
        .as_array()
        .ok_or("missing declared dependencies")?
    {
        let name = text(dependency, "name")?;
        let symbol = dependency["rename"]
            .as_str()
            .unwrap_or(name)
            .replace('-', "_");
        names.insert(symbol, name.to_owned());
    }
    Ok(names)
}

fn text<'a>(value: &'a Value, key: &str) -> CheckResult<&'a str> {
    value
        .get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| format!("missing Cargo string {key}").into())
}
