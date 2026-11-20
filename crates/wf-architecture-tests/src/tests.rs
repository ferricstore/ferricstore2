use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    process::Command,
};

use serde_json::{Value, json};

use crate::{
    CheckResult, graph, manifest,
    policy::Policy,
    source::{self, Context},
};

mod blob_policy;
mod codec_policy;
mod metadata_policy;
mod options;
mod runtime_rules;
mod source_pins;
mod state_values;
mod test_roots;

fn root() -> CheckResult<PathBuf> {
    Ok(Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()?)
}

fn policy() -> CheckResult<Policy> {
    Policy::load(&root()?.join("architecture/policy.toml"))
}

fn metadata() -> CheckResult<Value> {
    let output = Command::new(env!("CARGO"))
        .args([
            "metadata",
            "--format-version=1",
            "--no-deps",
            "--offline",
            "--locked",
        ])
        .current_dir(root()?)
        .output()?;
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(serde_json::from_slice(&output.stdout)?)
}

fn package_mut<'a>(metadata: &'a mut Value, name: &str) -> CheckResult<&'a mut Value> {
    metadata["packages"]
        .as_array_mut()
        .ok_or("missing packages")?
        .iter_mut()
        .find(|package| package["name"] == name)
        .ok_or_else(|| format!("missing {name}").into())
}

fn source_check(package: &str, module: &str, source: &str) -> CheckResult<Vec<String>> {
    source_check_kind(package, module, source, false)
}

fn source_check_kind(
    package: &str,
    module: &str,
    source: &str,
    test_file: bool,
) -> CheckResult<Vec<String>> {
    let policy = policy()?;
    let dependency_names = BTreeMap::new();
    let classification = policy
        .packages
        .get(package)
        .ok_or("missing source classification")?;
    source::check(
        source,
        &Context {
            package,
            module,
            root_file: false,
            test_file,
            classification,
            policy: &policy,
            dependency_names: &dependency_names,
        },
    )
}

#[test]
fn production_file_size_boundary_counts_comments_and_blank_lines() -> CheckResult<()> {
    let at_limit = "// A documented production responsibility.\n".repeat(400);
    assert_eq!(
        source_check("wf-domain", "workflow", &at_limit)?,
        Vec::<String>::new()
    );
    let oversized = format!("{at_limit}\n");
    assert!(
        source_check("wf-domain", "workflow", &oversized)?
            .iter()
            .any(|failure| failure.contains("401 physical lines exceeds 400"))
    );
    Ok(())
}

#[test]
fn tooling_and_integration_tests_have_a_separate_checked_size_limit() -> CheckResult<()> {
    let at_limit = "// Test and tooling explanation.\n".repeat(600);
    let oversized = format!("{at_limit}// One line beyond the limit.");
    for (package, module, test_file) in [
        ("wf-architecture-tests", "source", false),
        ("wf-domain", "tests::regression", true),
    ] {
        assert_eq!(
            source_check_kind(package, module, &at_limit, test_file)?,
            Vec::<String>::new()
        );
        assert!(
            source_check_kind(package, module, &oversized, test_file)?
                .iter()
                .any(|failure| failure.contains("601 physical lines exceeds 600"))
        );
    }
    Ok(())
}

#[test]
fn physical_size_counts_crlf_and_an_unterminated_final_line() -> CheckResult<()> {
    let at_limit = "// Platform-independent sizing.\r\n".repeat(400);
    assert_eq!(
        source_check("wf-types", "bounds", &at_limit)?,
        Vec::<String>::new()
    );
    let oversized = format!("{at_limit}// No final newline");
    assert!(
        source_check("wf-types", "bounds", &oversized)?
            .iter()
            .any(|failure| failure.contains("401 physical lines exceeds 400"))
    );
    Ok(())
}

#[test]
fn runtime_source_cannot_get_a_larger_budget_by_naming_it_tests() -> CheckResult<()> {
    let oversized = "// Runtime source remains in the production budget.\n".repeat(401);
    assert!(
        source_check("wf-domain", "workflow::tests", &oversized)?
            .iter()
            .any(|failure| failure.contains("401 physical lines exceeds 400"))
    );
    Ok(())
}

#[test]
fn declared_edges_reject_inactive_optional_target_build_and_dev_bypasses() -> CheckResult<()> {
    let original = metadata()?;
    let policy = policy()?;
    for (kind, optional, target) in [
        (Value::Null, false, Value::Null),
        (Value::Null, true, json!("cfg(windows)")),
        (json!("build"), false, Value::Null),
        (json!("dev"), false, Value::Null),
    ] {
        let mut changed = original.clone();
        package_mut(&mut changed, "wf-domain")?["dependencies"].as_array_mut().ok_or("missing dependencies")?.push(json!({
            "name": "wf-storage", "rename": "sneaky", "kind": kind, "optional": optional, "target": target,
            "source": null, "path": root()?.join("crates/wf-storage"), "features": [], "req": "*", "uses_default_features": true,
        }));
        let failures = graph::check(&changed, &policy, &root()?)?;
        assert!(
            failures
                .iter()
                .any(|failure| failure.contains("forbidden declared edge wf-domain -> wf-storage")),
            "{failures:?}"
        );
    }
    Ok(())
}

#[test]
fn all_runtime_tooling_edges_and_unclassified_external_dependencies_fail() -> CheckResult<()> {
    let original = metadata()?;
    let policy = policy()?;
    for tool in [
        "wf-architecture-tests",
        "wf-test-support",
        "wf-parity-tests",
    ] {
        let mut changed = original.clone();
        package_mut(&mut changed, "wf-node")?["dependencies"]
            .as_array_mut()
            .ok_or("missing dependencies")?
            .push(json!({
                "name": tool, "kind": null, "optional": true, "target": "cfg(unix)", "source": null,
                "path": root()?.join("crates").join(tool), "features": [],
            }));
        assert!(
            graph::check(&changed, &policy, &root()?)?
                .iter()
                .any(|failure| failure.contains(&format!("wf-node -> {tool}")))
        );
    }
    let mut changed = original;
    package_mut(&mut changed, "wf-domain")?["dependencies"]
        .as_array_mut()
        .ok_or("missing dependencies")?
        .push(json!({"name":"tokio", "kind":null, "optional":true, "target":"cfg(windows)"}));
    assert!(
        graph::check(&changed, &policy, &root()?)?
            .iter()
            .any(|failure| failure.contains("unclassified external edge wf-domain -> tokio"))
    );
    Ok(())
}

#[test]
fn coverage_features_and_dependency_feature_drift_fail() -> CheckResult<()> {
    let original = metadata()?;
    let policy = policy()?;
    let mut changed = original.clone();
    let id = package_mut(&mut changed, "wf-wire")?["id"].clone();
    changed["workspace_members"]
        .as_array_mut()
        .ok_or("missing members")?
        .retain(|member| *member != id);
    assert!(
        graph::check(&changed, &policy, &root()?)?
            .iter()
            .any(|failure| failure.contains("policy package wf-wire missing"))
    );
    let mut changed = original.clone();
    package_mut(&mut changed, "wf-domain")?["features"] = json!({"effects": ["dep:tokio"]});
    assert!(
        graph::check(&changed, &policy, &root()?)?
            .iter()
            .any(|failure| failure.contains("unclassified features"))
    );
    let mut changed = original;
    package_mut(&mut changed, "wf-architecture-tests")?["dependencies"][0]["features"] =
        json!(["unreviewed"]);
    assert!(
        graph::check(&changed, &policy, &root()?)?
            .iter()
            .any(|failure| failure.contains("unreviewed external version/source/features"))
    );
    Ok(())
}

#[test]
fn application_hash_policy_is_pinned_and_cannot_leak_into_domain() -> CheckResult<()> {
    let original = metadata()?;
    let policy = policy()?;
    let edge = json!({
        "name":"sha2", "kind":null, "optional":false, "target":null,
        "source":"registry+https://github.com/rust-lang/crates.io-index", "path":null,
        "features":[], "req":"=0.11.0", "uses_default_features":false,
    });
    let mut accepted = original.clone();
    let dependencies = package_mut(&mut accepted, "wf-application")?["dependencies"]
        .as_array_mut()
        .ok_or("missing dependencies")?;
    dependencies.retain(|dependency| dependency["name"] != "sha2");
    dependencies.push(edge.clone());
    assert_eq!(
        graph::check(&accepted, &policy, &root()?)?,
        Vec::<String>::new()
    );
    for (field, value) in [
        ("req", json!("^0.11")),
        ("features", json!(["zeroize"])),
        ("uses_default_features", json!(true)),
        ("source", json!("git+https://example.invalid/hash")),
    ] {
        let mut changed = accepted.clone();
        let dependencies = package_mut(&mut changed, "wf-application")?["dependencies"]
            .as_array_mut()
            .ok_or("missing dependencies")?;
        let hash = dependencies
            .iter_mut()
            .find(|dependency| dependency["name"] == "sha2")
            .ok_or("missing hash edge")?;
        hash[field] = value;
        assert!(
            graph::check(&changed, &policy, &root()?)?
                .iter()
                .any(|failure| failure.contains("wf-application -> sha2: unreviewed"))
        );
    }
    let mut changed = original;
    package_mut(&mut changed, "wf-domain")?["dependencies"]
        .as_array_mut()
        .ok_or("missing dependencies")?
        .push(edge);
    assert!(
        graph::check(&changed, &policy, &root()?)?
            .iter()
            .any(|failure| failure.contains("unclassified external edge wf-domain -> sha2"))
    );
    Ok(())
}

#[test]
fn runtime_reexports_aliases_and_qualified_paths_fail() -> CheckResult<()> {
    for text in [
        include_str!("../tests/fixtures/wire-reexport.rs"),
        include_str!("../tests/fixtures/wire-alias.rs"),
        include_str!("../tests/fixtures/wire-qualified.rs"),
    ] {
        assert!(
            source_check("wf-wire", "dto", text)?
                .iter()
                .any(|failure| failure.contains("wire may reference only wf_domain::value"))
        );
    }
    assert!(
        source_check(
            "wf-worker",
            "worker",
            include_str!("../tests/fixtures/worker-wrong-port.rs")
        )?
        .iter()
        .any(|failure| failure.contains("wf_ports::client"))
    );
    assert!(
        source_check(
            "wf-worker",
            "worker",
            include_str!("../tests/fixtures/worker-wrong-wire.rs")
        )?
        .iter()
        .any(|failure| failure.contains("private client transport module"))
    );
    assert!(
        source_check("wf-worker", "", "pub mod client;")?
            .iter()
            .any(|failure| failure.contains("must remain private"))
    );
    assert!(
        source_check("wf-worker", "", "pub use client::http::*;")?
            .iter()
            .any(|failure| failure.contains("cannot be re-exported"))
    );
    assert!(
        source_check("wf-domain", "value", "pub use crate::workflow::*;")?
            .iter()
            .any(|failure| failure.contains("cannot expose other domain modules"))
    );
    Ok(())
}

#[test]
fn safe_approved_value_and_private_transport_paths_are_accepted() -> CheckResult<()> {
    assert_eq!(
        source_check(
            "wf-wire",
            "dto",
            "use wf_domain::value as values; type Value = values::Bounded;"
        )?,
        Vec::<String>::new()
    );
    assert_eq!(
        source_check(
            "wf-worker",
            "client::http",
            "use wf_ports::client; use wf_wire::dto;"
        )?,
        Vec::<String>::new()
    );
    assert_eq!(
        source_check(
            "wf-domain",
            "workflow",
            "use std::collections::BTreeMap; use wf_types::identity;"
        )?,
        Vec::<String>::new()
    );
    Ok(())
}

#[test]
fn purity_unsafe_suppression_ignore_and_generated_code_are_rejected() -> CheckResult<()> {
    let effects = source_check(
        "wf-domain",
        "workflow",
        include_str!("../tests/fixtures/domain-effects.rs"),
    )?;
    assert!(
        effects
            .iter()
            .any(|failure| failure.contains("standard-library module"))
    );
    assert!(effects.iter().any(|failure| failure.contains("async")));
    for (text, expected) in [
        (include_str!("../tests/fixtures/unsafe.rs"), "unsafe block"),
        (
            include_str!("../tests/fixtures/lint-bypass.rs"),
            "lint exceptions",
        ),
        (
            include_str!("../tests/fixtures/archunit-ignore.rs"),
            "archunit ignore",
        ),
        (
            include_str!("../tests/fixtures/generated.rs"),
            "runtime macro/generated code",
        ),
    ] {
        assert!(
            source_check("wf-domain", "workflow", text)?
                .iter()
                .any(|failure| failure.contains(expected)),
            "missing {expected}"
        );
    }
    assert!(
        source_check(
            "wf-domain",
            "workflow",
            "use std as hidden; use hidden::fs;"
        )?
        .iter()
        .any(|failure| failure.contains("standard-library module"))
    );
    Ok(())
}

#[test]
fn cargo_rename_does_not_hide_restricted_dependency() -> CheckResult<()> {
    let policy = policy()?;
    let dependency_names = BTreeMap::from([("hidden".into(), "wf-domain".into())]);
    let classification = policy.packages.get("wf-wire").ok_or("missing wire")?;
    let failures = source::check(
        "type Hidden = hidden::workflow::Record;",
        &Context {
            package: "wf-wire",
            module: "dto",
            root_file: false,
            test_file: false,
            classification,
            policy: &policy,
            dependency_names: &dependency_names,
        },
    )?;
    assert!(
        failures
            .iter()
            .any(|failure| failure.contains("wire may reference only"))
    );
    Ok(())
}

#[test]
fn lint_inheritance_and_reasoned_narrow_test_exceptions_are_checked() -> CheckResult<()> {
    let temp = tempfile::tempdir()?;
    let path = temp.path().join("Cargo.toml");
    std::fs::write(&path, "[package]\nname='fixture'\n")?;
    assert!(
        manifest::check_package(&path, "fixture")?
            .iter()
            .any(|failure| failure.contains("inherit workspace lints"))
    );
    let test = "#[test]\n#[allow(clippy::expect_used, reason = \"This negative fixture must assert the precise expected error\")]\nfn negative() {}";
    assert_eq!(
        source_check("wf-test-support", "tests", test)?,
        Vec::<String>::new()
    );
    let broad = "#[test]\n#[allow(clippy::all, reason = \"This fixture must not bypass all lint policy\")]\nfn negative() {}";
    assert!(
        source_check("wf-test-support", "tests", broad)?
            .iter()
            .any(|failure| failure.contains("lint exceptions"))
    );
    Ok(())
}

#[test]
fn malformed_source_is_a_technical_failure_and_empty_policy_fails() {
    for malformed in ["", "[packages]\n", "schema='wrong'\n", "schema=1\n"] {
        assert!(Policy::parse(malformed).is_err());
    }
    assert!(source_check("wf-wire", "dto", "fn { this is invalid").is_err());
    assert!(Policy::parse("schema=1\nwire_domain_module='value'\nworker_port_module='client'\nworker_wire_modules=[]\n[packages]\n").is_err());
}

#[test]
fn approved_expression_macros_cannot_hide_effects_or_layer_access() -> CheckResult<()> {
    let effects = source_check(
        "wf-domain",
        "workflow",
        include_str!("../tests/fixtures/macro-effects.rs"),
    )?;
    assert!(
        effects
            .iter()
            .filter(|failure| failure.contains("standard-library module"))
            .count()
            >= 6,
        "{effects:?}"
    );
    let hidden = source_check(
        "wf-worker",
        "worker",
        "fn forbidden() { let _ = vec![wf_wire::dto::Record]; }",
    )?;
    assert!(
        hidden
            .iter()
            .any(|failure| failure.contains("private client transport module"))
    );
    for (source, diagnosis) in [
        (
            "fn forbidden() { let _ = write!(formatter, \"{:?}\", wf_wire::dto::Record); }",
            "private client transport module",
        ),
        (
            "fn forbidden() { let _ = write!(formatter, \"{}\", unsafe { 1 }); }",
            "first-party unsafe block",
        ),
        (
            "fn malformed() { let _ = write!(formatter; \"{}\", 1); }",
            "uninspectable approved expression macro",
        ),
        (
            "fn malformed() { let _ = write!(formatter); }",
            "uninspectable approved expression macro",
        ),
        (
            "fn malformed() { let _ = write!(formatter, format_string, 1); }",
            "uninspectable approved expression macro",
        ),
    ] {
        assert!(
            source_check("wf-worker", "worker", source)?
                .iter()
                .any(|failure| failure.contains(diagnosis))
        );
    }
    assert_eq!(
        source_check(
            "wf-domain",
            "workflow",
            "fn pure() { let _ = vec![1; 3]; let _ = format!(\"{}\", 1); let _ = matches!(1, 1 | 2 if true); let _ = write!(formatter, \"{n:?}\", n = 1); }"
        )?,
        Vec::<String>::new()
    );
    Ok(())
}

#[test]
fn client_port_facade_cannot_reexport_server_capabilities() -> CheckResult<()> {
    let failures = source_check(
        "wf-ports",
        "client",
        include_str!("../tests/fixtures/client-facade.rs"),
    )?;
    assert!(
        failures
            .iter()
            .filter(|failure| failure.contains("client port facade"))
            .count()
            >= 3,
        "{failures:?}"
    );
    assert_eq!(
        source_check(
            "wf-ports",
            "client",
            "use wf_types::identity; use wf_domain::value;"
        )?,
        Vec::<String>::new()
    );
    Ok(())
}

#[test]
fn weakened_required_root_lint_is_rejected() -> CheckResult<()> {
    let real = root()?;
    let fixture = tempfile::tempdir()?;
    source_pins::write_readability_configuration(fixture.path())?;
    std::fs::copy(
        real.join("rust-toolchain.toml"),
        fixture.path().join("rust-toolchain.toml"),
    )?;
    let original = std::fs::read_to_string(real.join("Cargo.toml"))?;
    assert!(original.contains("rust_2018_idioms = { level = \"deny\""));
    let weakened = original.replace(
        "rust_2018_idioms = { level = \"deny\"",
        "rust_2018_idioms = { level = \"allow\"",
    );
    std::fs::write(fixture.path().join("Cargo.toml"), weakened)?;
    assert!(
        manifest::check_root(fixture.path())?
            .iter()
            .any(|failure| failure.contains("rust_2018_idioms must be deny"))
    );
    source_pins::assert_readability_policy(&real)?;
    Ok(())
}
