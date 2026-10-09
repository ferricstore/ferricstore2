use std::{fs, path::Path};

use crate::CheckResult;

pub(crate) fn check_root(root: &Path) -> CheckResult<Vec<String>> {
    let manifest: toml::Value = fs::read_to_string(root.join("Cargo.toml"))?.parse()?;
    let mut violations = Vec::new();
    for (group, lint, required) in [
        ("rust", "unsafe_code", "forbid"),
        ("rust", "unsafe_op_in_unsafe_fn", "deny"),
        ("rust", "missing_docs", "deny"),
        ("rust", "unused_must_use", "deny"),
        ("rust", "rust_2018_idioms", "deny"),
        ("rustdoc", "broken_intra_doc_links", "deny"),
        ("rustdoc", "bare_urls", "deny"),
        ("clippy", "all", "deny"),
        ("clippy", "pedantic", "deny"),
        ("clippy", "unwrap_used", "deny"),
        ("clippy", "expect_used", "deny"),
        ("clippy", "panic", "deny"),
        ("clippy", "todo", "deny"),
        ("clippy", "unimplemented", "deny"),
        ("clippy", "dbg_macro", "deny"),
        ("clippy", "await_holding_lock", "deny"),
    ] {
        let value = manifest
            .get("workspace")
            .and_then(|value| value.get("lints"))
            .and_then(|value| value.get(group))
            .and_then(|value| value.get(lint));
        let level = value.and_then(|value| {
            value
                .as_str()
                .or_else(|| value.get("level").and_then(toml::Value::as_str))
        });
        if level != Some(required) {
            violations.push(format!("workspace lint {group}::{lint} must be {required}"));
        }
    }
    let toolchain: toml::Value = fs::read_to_string(root.join("rust-toolchain.toml"))?.parse()?;
    if toolchain
        .get("toolchain")
        .and_then(|value| value.get("channel"))
        .and_then(toml::Value::as_str)
        != Some("1.99.0")
    {
        violations.push("Rust toolchain must be pinned to 1.99.0".into());
    }
    let profiles = manifest
        .get("workspace")
        .and_then(|value| value.get("metadata"))
        .and_then(|value| value.get("ferricstore"))
        .and_then(|value| value.get("supported-feature-profiles"))
        .and_then(toml::Value::as_array);
    if profiles.is_none_or(|values| values.len() != 1 || values[0].as_str() != Some("default")) {
        violations.push("unclassified workspace feature profile".into());
    }
    Ok(violations)
}

pub(crate) fn check_package(path: &Path, name: &str) -> CheckResult<Vec<String>> {
    let value: toml::Value = fs::read_to_string(path)?.parse()?;
    let mut violations = Vec::new();
    let lints = value.get("lints").and_then(toml::Value::as_table);
    if lints.is_none_or(|table| {
        table.len() != 1 || table.get("workspace").and_then(toml::Value::as_bool) != Some(true)
    }) {
        violations.push(format!(
            "{name}: must inherit workspace lints without local overrides"
        ));
    }
    for field in [
        "version",
        "edition",
        "rust-version",
        "publish",
        "repository",
    ] {
        if value
            .get("package")
            .and_then(|value| value.get(field))
            .and_then(|value| value.get("workspace"))
            .and_then(toml::Value::as_bool)
            != Some(true)
        {
            violations.push(format!("{name}: package {field} must inherit workspace"));
        }
    }
    Ok(violations)
}
