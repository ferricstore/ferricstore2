//! Explicit public option-module classification preserves the domain boundary.

use super::{CheckResult, graph, metadata, package_mut, policy, root, source_check};
use serde_json::json;

#[test]
fn options_module_classification_preserves_purity_and_forbidden_edges() -> CheckResult<()> {
    for (module, path) in [
        ("options", "crates/wf-domain/src/options.rs"),
        ("options::put", "crates/wf-domain/src/options/put.rs"),
    ] {
        let actual = std::fs::read_to_string(root()?.join(path))?;
        assert!(source_check("wf-domain", module, &actual)?.is_empty());
        assert!(
            source_check(
                "wf-domain",
                module,
                "pub fn borrow(value: &[u8]) -> std::borrow::Cow<'_, [u8]> { std::borrow::Cow::Borrowed(value) }",
            )?.is_empty(),
            "pure option source must have an explicit module classification"
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
        configured.packages["wf-domain"].modules.get("options"),
        Some(&true)
    );
    let original = metadata()?;
    assert!(graph::check(&original, &configured, &root()?)?.is_empty());
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
