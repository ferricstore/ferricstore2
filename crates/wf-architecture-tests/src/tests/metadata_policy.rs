//! Explicit metadata classification preserves domain purity and graph limits.

use super::{CheckResult, graph, metadata, package_mut, policy, root, source_check};
use serde_json::json;

#[test]
fn catalog_publication_boundaries_remain_configured_and_pure() -> CheckResult<()> {
    assert_eq!(
        policy()?.packages["wf-application"]
            .modules
            .get("governance"),
        Some(&true)
    );
    for module in [
        "governance",
        "governance::publication",
        "governance::publication::dispatch",
        "governance::publication::plan",
    ] {
        assert!(
            source_check(
                "wf-application",
                module,
                "use wf_ports::consensus::CreditCatalogPublicationPort;"
            )?
            .is_empty()
        );
    }
    for module in [
        "state::credit_catalog",
        "state::credit_catalog::member",
        "state::credit_catalog::read",
        "consensus::catalog_publication",
    ] {
        assert!(
            source_check(
                "wf-ports",
                module,
                "use std::vec::Vec; use wf_domain::workflow::request::RequestScope;"
            )?
            .is_empty()
        );
        for (source, diagnosis) in [
            ("use std::fs;", "standard-library module"),
            ("use std::env;", "standard-library module"),
            ("pub async fn hidden_effect() {}", "declares async behavior"),
            ("static GLOBAL: u64 = 0;", "declares global state"),
        ] {
            assert!(
                source_check("wf-ports", module, source)?
                    .iter()
                    .any(|failure| failure.contains(diagnosis)),
                "{module}: {source}"
            );
        }
    }
    let original = metadata()?;
    let rules = policy()?;
    let workspace = root()?;
    for target in [
        "wf-storage",
        "wf-consensus",
        "wf-node",
        "wf-api-http",
        "wf-api-tcp",
    ] {
        let mut changed = original.clone();
        package_mut(&mut changed, "wf-application")?["dependencies"]
            .as_array_mut()
            .ok_or("missing dependencies")?
            .push(json!({
                "name": target,
                "rename": null,
                "kind": null,
                "optional": false,
                "target": null,
                "source": null,
                "path": workspace.join("crates").join(target),
                "features": [],
                "req": "*",
                "uses_default_features": true
            }));
        assert!(
            graph::check(&changed, &rules, &workspace)?
                .iter()
                .any(|failure| failure.contains(&format!(
                    "forbidden declared edge wf-application -> {target}"
                )))
        );
    }
    Ok(())
}

#[test]
fn governance_credit_classification_preserves_pure_decisions() -> CheckResult<()> {
    assert_eq!(
        policy()?.packages["wf-domain"].modules.get("governance"),
        Some(&true)
    );
    for module in [
        "governance",
        "governance::credit",
        "governance::credit::model",
        "governance::credit::admission",
        "governance::credit::capacity",
        "governance::credit::decision",
        "governance::credit::reservation",
    ] {
        assert_eq!(
            source_check(
                "wf-domain",
                module,
                "pub fn finite(number: f64) -> bool { number.is_finite() }"
            )?,
            Vec::<String>::new()
        );
        for (source, diagnosis) in [
            ("use std::env;", "standard-library module"),
            ("use std::fs;", "standard-library module"),
            ("use std::io;", "standard-library module"),
            ("pub async fn effect() {}", "declares async behavior"),
            ("static GLOBAL: u64 = 0;", "declares global state"),
        ] {
            assert!(
                source_check("wf-domain", module, source)?
                    .iter()
                    .any(|finding| finding.contains(diagnosis))
            );
        }
    }
    for (module, path) in [
        ("governance", "crates/wf-domain/src/governance.rs"),
        (
            "governance::credit",
            "crates/wf-domain/src/governance/credit.rs",
        ),
        (
            "governance::credit::model",
            "crates/wf-domain/src/governance/credit/model.rs",
        ),
    ] {
        let actual = std::fs::read_to_string(root()?.join(path))?;
        assert_eq!(
            source_check("wf-domain", module, &actual)?,
            Vec::<String>::new()
        );
    }
    Ok(())
}

#[test]
fn metadata_module_classification_preserves_purity_and_forbidden_edges() -> CheckResult<()> {
    for (module, path) in [
        ("metadata", "crates/wf-domain/src/metadata.rs"),
        (
            "metadata::attributes",
            "crates/wf-domain/src/metadata/attributes.rs",
        ),
        (
            "metadata::state_meta",
            "crates/wf-domain/src/metadata/state_meta.rs",
        ),
    ] {
        let actual = std::fs::read_to_string(root()?.join(path))?;
        assert_eq!(
            source_check("wf-domain", module, &actual)?,
            Vec::<String>::new()
        );
        assert!(
            source_check(
                "wf-domain",
                module,
                r"pub fn borrowed(value: &[u8], number: f64) -> bool {
                    std::str::from_utf8(value).is_ok_and(|text| !text.trim().is_empty())
                        && number.is_finite()
                }",
            )?
            .is_empty(),
            "pure metadata source must retain an explicit module classification"
        );
        for (source, diagnosis) in [
            ("use std::env;", "standard-library module"),
            ("use std::fs;", "standard-library module"),
            ("use std::io;", "standard-library module"),
            ("pub async fn effect() {}", "declares async behavior"),
            ("static GLOBAL: u64 = 0;", "declares global state"),
        ] {
            assert!(
                source_check("wf-domain", module, source)?
                    .iter()
                    .any(|failure| failure.contains(diagnosis))
            );
        }
    }
    let configured = policy()?;
    assert_eq!(
        configured.packages["wf-domain"].modules.get("metadata"),
        Some(&true)
    );
    let original = metadata()?;
    assert_eq!(
        graph::check(&original, &configured, &root()?)?,
        Vec::<String>::new()
    );
    let mut ports = original.clone();
    package_mut(&mut ports, "wf-domain")?["dependencies"]
        .as_array_mut()
        .ok_or("missing dependencies")?
        .push(json!({
            "name": "wf-ports", "rename": "hidden_port", "kind": null,
            "optional": true, "target": "cfg(windows)", "source": null,
            "path": root()?.join("crates/wf-ports"), "features": [],
            "req": "*", "uses_default_features": true,
        }));
    assert!(
        graph::check(&ports, &configured, &root()?)?
            .iter()
            .any(|failure| failure.contains("forbidden declared edge wf-domain -> wf-ports"))
    );
    let mut runtime = original;
    package_mut(&mut runtime, "wf-domain")?["dependencies"]
        .as_array_mut()
        .ok_or("missing dependencies")?
        .push(json!({"name": "tokio", "kind": null, "optional": true, "target": "cfg(windows)"}));
    assert!(
        graph::check(&runtime, &configured, &root()?)?
            .iter()
            .any(|failure| failure.contains("unclassified external edge wf-domain -> tokio"))
    );
    Ok(())
}
