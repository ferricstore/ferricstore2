use super::{CheckResult, graph, json, metadata, package_mut, policy, root, source_check};

#[test]
fn state_value_codecs_and_response_parser_reject_effects() -> CheckResult<()> {
    for (package, module) in [
        ("wf-storage", "state::value"),
        ("wf-storage", "state::value::control"),
        ("wf-storage", "state::value::record"),
        ("wf-storage", "state::value::record::scan"),
        ("wf-storage", "state::value::outcome"),
        ("wf-storage", "state::value::outcome::atomic_start"),
        ("wf-storage", "state::value::outcome::atomic_start::nested"),
        ("wf-storage", "state::value::history"),
        ("wf-storage", "state::value::membership"),
        ("wf-storage", "state::value::membership::nested"),
        ("wf-storage", "state::key"),
        ("wf-storage", "state::key::point"),
        ("wf-storage", "state::key::ordered"),
        ("wf-storage", "state::key::ordered::history"),
        ("wf-storage", "state::key::ordered::nested"),
        ("wf-domain", "policy::complete::record_projection"),
        ("wf-domain", "policy::complete::record_projection::nested"),
        ("wf-domain", "history::metadata"),
        ("wf-domain", "history::metadata::model"),
        ("wf-domain", "history::metadata::budget"),
        ("wf-domain", "history::metadata::scalar"),
        ("wf-domain", "history::metadata::numeric"),
        ("wf-domain", "history::metadata::numeric::float"),
        ("wf-domain", "history::metadata::text"),
        ("wf-domain", "history::compact"),
        ("wf-domain", "history::compact::model"),
        ("wf-domain", "history::compact::budget"),
        ("wf-domain", "history::compact::primitives"),
        ("wf-domain", "history::compact::number"),
        ("wf-domain", "history::compact::encode"),
        ("wf-domain", "history::compact::decode"),
        ("wf-domain", "history::compact::project"),
        ("wf-domain", "value::original_flow"),
        ("wf-domain", "value::original_flow::prepare"),
        ("wf-domain", "value::original_flow::prepare::model"),
        ("wf-domain", "value::original_flow::prepare::write"),
        ("wf-ports", "log::record_batch::response"),
        ("wf-ports", "log::record_batch::step_continue"),
        ("wf-ports", "log::record_batch::step_continue::input"),
        ("wf-ports", "log::record_batch::step_continue::wire"),
        ("wf-ports", "log::claim"),
        ("wf-ports", "log::claim::apply"),
        ("wf-ports", "log::claim::apply::nested"),
        ("wf-ports", "log::claim::response"),
        ("wf-ports", "log::claim::response::scan"),
        ("wf-ports", "log::claim::response::scan::nested"),
        ("wf-ports", "state::transaction::independent"),
        ("wf-ports", "state::transaction::independent::nested"),
        ("wf-ports", "state::transaction::atomic_start"),
        ("wf-ports", "state::transaction::atomic_start::nested"),
        ("wf-ports", "log::public_mutation::response"),
        ("wf-ports", "log::public_mutation::response::nested"),
        ("wf-ports", "snapshot::metadata"),
        ("wf-ports", "snapshot::metadata::nested"),
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

fn assert_effectful_transaction_neighbors() -> CheckResult<()> {
    for (package, module) in [
        ("wf-ports", "state::transaction"),
        ("wf-ports", "state::claim"),
        ("wf-ports", "state::claim::nested"),
        ("wf-storage", "transaction"),
        ("wf-storage", "transaction::record_batch"),
        ("wf-storage", "transaction::load"),
        ("wf-storage", "transaction::observations"),
        ("wf-storage", "state::value_backend"),
        ("wf-storage", "state::key_backend"),
        ("wf-storage", "state::keyed"),
        ("wf-storage", "state::redb::history"),
        ("wf-storage", "state::redb::history::read"),
        ("wf-storage", "state::redb::history::page"),
        ("wf-storage", "state::redb::history::row"),
        ("wf-storage", "state::redb::history::hydrate"),
        ("wf-storage", "state::redb::history::hydrate::references"),
        ("wf-storage", "state::redb::history::hydrate::budget"),
        ("wf-storage", "state::redb::values::read::sized::captured"),
        ("wf-storage", "state::redb::values::read::sized::point"),
        ("wf-ports", "log::record_batch::response_backend"),
        ("wf-ports", "log::claim_backend"),
        ("wf-ports", "state::transaction::independent_backend"),
        ("wf-ports", "state::transaction::atomic_start_backend"),
        ("wf-ports", "log::public_mutation_backend"),
        ("wf-ports", "consensus::mutation"),
        ("wf-ports", "snapshot"),
        ("wf-ports", "snapshot::metadata_backend"),
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
    Ok(())
}

#[test]
fn state_transaction_stays_effectful_and_server_only() -> CheckResult<()> {
    assert_effectful_transaction_neighbors()?;
    for name in [
        "GroupApplyStore",
        "GroupApplyTransaction",
        "GroupDueCursor",
        "GroupApplyChanges",
        "GroupControlChanges",
        "GroupControlTransaction",
        "GroupClaimChanges",
        "GroupClaimTransaction",
        "GroupClaimDueCursor",
        "GroupClaimLaneCursor",
        "ClaimDueKind",
        "ClaimDueStep",
        "ClaimLaneStep",
        "WorkflowHistoryDirection",
        "WorkflowHistoryQuery",
        "WorkflowHistoryReadLimits",
        "StoredWorkflowHistoryRow",
        "WorkflowHistoryPage",
        "WorkflowHistoryHydrationRequest",
        "WorkflowHistoryHydrationLimits",
        "WorkflowHistoryHydrationError",
        "WorkflowHistoryValuePage",
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
    assert_domain_values_stay_server_only()?;
    for name in [
        "StateSnapshotStore",
        "SnapshotMetadata",
        "SnapshotLimits",
        "SnapshotError",
    ] {
        assert!(
            source_check(
                "wf-ports",
                "client",
                &format!("pub use crate::snapshot::{name};")
            )?
            .iter()
            .any(|finding| finding.contains("client port facade"))
        );
    }
    for name in [
        "MutationSubmitPort",
        "MutationSubmission",
        "MutationUnknown",
        "MutationNotSubmitted",
    ] {
        assert!(
            source_check(
                "wf-ports",
                "client",
                &format!("pub use crate::consensus::{name};")
            )?
            .iter()
            .any(|finding| finding.contains("client port facade"))
        );
    }
    let original = metadata()?;
    for (from, to) in [
        ("wf-wire", "wf-ports"),
        ("wf-wire", "wf-storage"),
        ("wf-storage", "wf-consensus"),
        ("wf-consensus", "wf-storage"),
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

#[test]
fn checkpoint_join_dependency_is_exact_and_layered() -> CheckResult<()> {
    let mut accepted = metadata()?;
    let dependencies = package_mut(&mut accepted, "wf-consensus")?["dependencies"]
        .as_array_mut()
        .ok_or("missing dependencies")?;
    dependencies.retain(|edge| edge["name"] != "sha2");
    dependencies.push(json!({
        "name": "sha2", "kind": null, "optional": false, "target": null,
        "source": "registry+https://github.com/rust-lang/crates.io-index",
        "path": null, "features": [], "req": "=0.11.0", "uses_default_features": false,
    }));
    assert_eq!(
        graph::check(&accepted, &policy()?, &root()?)?,
        Vec::<String>::new()
    );
    assert_eq!(
        source_check("wf-consensus", "recovery", "use std::fmt;")?,
        Vec::<String>::new()
    );
    for (field, value) in [
        ("req", json!("^0.11.0")),
        ("features", json!(["std"])),
        ("uses_default_features", json!(true)),
        ("source", json!("registry+https://example.invalid/index")),
    ] {
        let mut changed = accepted.clone();
        let edge = package_mut(&mut changed, "wf-consensus")?["dependencies"]
            .as_array_mut()
            .ok_or("missing dependencies")?
            .iter_mut()
            .find(|edge| edge["name"] == "sha2")
            .ok_or("missing SHA edge")?;
        edge[field] = value;
        assert!(
            graph::check(&changed, &policy()?, &root()?)?
                .iter()
                .any(|finding| finding.contains("wf-consensus -> sha2: unreviewed")),
            "missed drift in {field}"
        );
    }
    Ok(())
}

fn assert_domain_values_stay_server_only() -> CheckResult<()> {
    for path in [
        "history::metadata::OriginalHistoryMetadataField",
        "history::metadata::OriginalHistoryMetadataLimits",
        "history::metadata::OriginalHistoryMetadataError",
        "history::metadata::PreparedOriginalHistoryMetadata",
        "history::metadata::prepare_original_history_metadata",
        "workflow::request::gate::PointGateView",
        "workflow::request::gate::inspect_point_request",
        "workflow::indexed::TouchedIndexedGroupView",
        "workflow::indexed::decide_touched_indexed_batch",
        "policy::complete::project_record_batch_policy",
        "policy::complete::record_batch_policy_projection_bytes",
    ] {
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
                source_check(package, module, &format!("pub use {domain}::{path};"))?
                    .iter()
                    .any(|finding| finding.contains(diagnosis))
            );
        }
    }
    Ok(())
}
