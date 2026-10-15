use super::{CheckResult, graph, json, metadata, package_mut, policy, root, source_check};

#[test]
fn state_value_codecs_and_response_parser_reject_effects() -> CheckResult<()> {
    for (package, module) in [
        ("wf-storage", "state::value"),
        ("wf-storage", "state::value::control"),
        ("wf-storage", "state::value::record"),
        ("wf-storage", "state::value::record::scan"),
        ("wf-storage", "state::value::outcome"),
        ("wf-storage", "state::value::history"),
        ("wf-storage", "state::key"),
        ("wf-storage", "state::key::point"),
        ("wf-storage", "state::key::ordered"),
        ("wf-storage", "state::key::ordered::nested"),
        ("wf-ports", "log::record_batch::response"),
    ] {
        assert_eq!(
            source_check(
                package,
                module,
                "use std::collections::BTreeMap; use std::fmt; use wf_types::version;"
            )?,
            Vec::<String>::new()
        );
        for (source, diagnosis) in [
            ("use std::fs;", "standard-library module"),
            ("pub use std::fs as exported;", "standard-library module"),
            (
                "use std as hidden; use hidden::env;",
                "standard-library module",
            ),
            ("pub async fn effect() {}", "declares async behavior"),
            ("static GLOBAL: u64 = 0;", "declares global state"),
            (
                "fn hidden() { let _ = format!(\"{}\", std::env::var(\"X\")); }",
                "standard-library module",
            ),
            ("fn unchecked() { unsafe {} }", "unsafe block"),
            (
                "include!(\"generated.rs\");",
                "runtime macro/generated code",
            ),
        ] {
            let findings = source_check(package, module, source)?;
            assert!(
                findings.iter().any(|finding| finding.contains(diagnosis)),
                "{package}::{module}: missing {diagnosis}: {findings:?}"
            );
        }
    }
    Ok(())
}

#[test]
fn state_transaction_stays_effectful_and_server_only() -> CheckResult<()> {
    for (package, module) in [
        ("wf-ports", "state::transaction"),
        ("wf-storage", "transaction"),
        ("wf-storage", "state::value_backend"),
        ("wf-storage", "state::key_backend"),
        ("wf-storage", "state::keyed"),
        ("wf-ports", "log::record_batch::response_backend"),
    ] {
        assert_eq!(
            source_check(
                package,
                module,
                "use std::fs; pub async fn acquire() {} static COUNTER: u64 = 0;"
            )?,
            Vec::<String>::new()
        );
    }
    for name in [
        "GroupApplyStore",
        "GroupApplyTransaction",
        "GroupDueCursor",
        "GroupApplyChanges",
    ] {
        assert!(
            source_check(
                "wf-ports",
                "client",
                &format!("pub use crate::state::{name};")
            )?
            .iter()
            .any(|finding| finding.contains("client port facade"))
        );
    }
    let original = metadata()?;
    for name in ["PointGateView", "inspect_point_request"] {
        for (package, module, domain, diagnosis) in [
            ("wf-ports", "client", "wf_domain", "client port facade"),
            (
                "wf-wire",
                "value",
                "wf_domain",
                "wire may reference only wf_domain::value",
            ),
            (
                "wf-domain",
                "value",
                "crate",
                "approved domain value module",
            ),
        ] {
            assert!(
                source_check(
                    package,
                    module,
                    &format!("pub use {domain}::workflow::request::gate::{name};")
                )?
                .iter()
                .any(|finding| finding.contains(diagnosis))
            );
        }
    }
    for (from, to) in [
        ("wf-wire", "wf-ports"),
        ("wf-wire", "wf-storage"),
        ("wf-storage", "wf-consensus"),
    ] {
        let mut changed = original.clone();
        package_mut(&mut changed, from)?["dependencies"]
            .as_array_mut()
            .ok_or("missing dependencies")?
            .push(json!({
                "name": to, "rename": "hidden_state", "kind": null,
                "optional": true, "target": "cfg(unix)", "source": null,
                "path": root()?.join("crates").join(to), "features": [],
                "req": "*", "uses_default_features": true,
            }));
        let findings = graph::check(&changed, &policy()?, &root()?)?;
        assert!(findings.iter().any(|finding| finding.contains(
            &format!("forbidden declared edge {from} -> {to}")
        )), "{findings:?}");
    }
    Ok(())
}
