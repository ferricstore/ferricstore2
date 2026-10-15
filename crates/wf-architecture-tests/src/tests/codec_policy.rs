use serde_json::{Value, json};

use super::{CheckResult, graph, metadata, package_mut, policy, root};

fn dependency() -> Value {
    json!({
        "name": "sha2", "kind": null, "optional": false, "target": null,
        "source": "registry+https://github.com/rust-lang/crates.io-index",
        "req": "=0.11.0", "features": [], "uses_default_features": false,
    })
}

#[test]
fn private_ports_codec_hash_policy_is_exact_and_cannot_leak() -> CheckResult<()> {
    let original = metadata()?;
    let policy = policy()?;
    let mut approved = original.clone();
    let dependencies = package_mut(&mut approved, "wf-ports")?["dependencies"]
        .as_array_mut()
        .ok_or("missing dependencies")?;
    dependencies.retain(|entry| entry["name"] != "sha2");
    dependencies.push(dependency());
    assert_eq!(
        graph::check(&approved, &policy, &root()?)?,
        Vec::<String>::new()
    );
    for (field, value) in [
        ("req", json!("=0.10.9")),
        ("source", json!("git+https://example.invalid/sha2")),
        ("features", json!(["std"])),
        ("uses_default_features", json!(true)),
    ] {
        let mut changed = approved.clone();
        let hash = package_mut(&mut changed, "wf-ports")?["dependencies"]
            .as_array_mut()
            .ok_or("missing dependencies")?
            .iter_mut()
            .find(|entry| entry["name"] == "sha2")
            .ok_or("missing exact hash")?;
        hash[field] = value;
        assert_ne!(
            graph::check(&changed, &policy, &root()?)?,
            Vec::<String>::new()
        );
    }
    for package in ["wf-types", "wf-domain", "wf-wire"] {
        let mut changed = approved.clone();
        package_mut(&mut changed, package)?["dependencies"]
            .as_array_mut()
            .ok_or("missing dependencies")?
            .push(dependency());
        assert_ne!(
            graph::check(&changed, &policy, &root()?)?,
            Vec::<String>::new()
        );
    }
    Ok(())
}

#[test]
fn complete_policy_outer_hash_and_dependency_boundaries_remain_closed() -> CheckResult<()> {
    let mut original = metadata()?;
    let rules = policy()?;
    let workspace = root()?;
    assert_eq!(
        graph::check(&original, &rules, &workspace)?,
        Vec::<String>::new()
    );
    let expected = dependency();
    let mut hash_count = 0;
    for entry in package_mut(&mut original, "wf-ports")?["dependencies"]
        .as_array()
        .ok_or("missing dependencies")?
    {
        if entry["name"] == "sha2" {
            for field in [
                "kind",
                "optional",
                "target",
                "source",
                "req",
                "features",
                "uses_default_features",
            ] {
                assert_eq!(entry[field], expected[field]);
            }
            hash_count += 1;
        }
    }
    assert_eq!(hash_count, 1);

    for package in ["wf-types", "wf-domain", "wf-wire"] {
        let mut leaked = original.clone();
        package_mut(&mut leaked, package)?["dependencies"]
            .as_array_mut()
            .ok_or("missing dependencies")?
            .push(dependency());
        assert_eq!(
            graph::check(&leaked, &rules, &workspace)?,
            vec![format!(
                "unclassified external edge {package} -> sha2 (normal, optional=false, target=null)"
            )]
        );
    }

    let mut backwards = original;
    let mut back_edge = dependency();
    back_edge["name"] = json!("wf-ports");
    back_edge["source"] = Value::Null;
    back_edge["req"] = json!("*");
    back_edge["path"] = json!(workspace.join("crates/wf-ports"));
    package_mut(&mut backwards, "wf-domain")?["dependencies"]
        .as_array_mut()
        .ok_or("missing dependencies")?
        .push(back_edge);
    assert_eq!(
        graph::check(&backwards, &rules, &workspace)?,
        vec![String::from(
            "forbidden declared edge wf-domain -> wf-ports (normal, optional=false, target=null)"
        )]
    );
    assert_complete_policy_outer_modules()?;
    for facade in [
        "pub use crate::log::CompleteCorePolicyPatchIntent;",
        "pub use crate::log::VerifiedCompleteCorePolicyPatchFrame;",
        "pub use crate::log::DecodedCompleteCorePolicyPatchFrame;",
        "pub use crate::log::CompletePolicyReplaceIntent;",
        "pub use crate::log::VerifiedCompletePolicyReplaceFrame;",
        "pub use crate::log::DecodedCompletePolicyReplaceFrame;",
        "pub use crate::log::decode_complete_policy_replace_frame;",
        "pub use crate::log::prepare_complete_policy_replace_frame;",
        "pub use crate::log::verify_complete_policy_replace_frame;",
    ] {
        assert!(
            super::source_check("wf-ports", "client", facade)?
                .iter()
                .any(|finding| finding.contains("client port facade"))
        );
    }
    Ok(())
}

#[test]
fn core_patch_request_parent_module_and_client_boundaries_remain_closed() -> CheckResult<()> {
    assert_complete_policy_replace_content_modules()?;
    assert_eq!(
        super::source_check(
            "wf-domain",
            "policy::complete::request",
            "use std::collections::BTreeMap; use wf_types::identity; use crate::workflow::request;",
        )?,
        Vec::<String>::new()
    );
    assert!(
        super::source_check("wf-domain", "policy::complete::request", "use std::env;")?
            .iter()
            .any(|finding| finding.contains("unclassified standard-library module"))
    );
    assert_eq!(
        super::source_check(
            "wf-ports",
            "log::complete_policy::core_patch::frame::apply",
            "use wf_domain::policy::complete; use wf_types::version;",
        )?,
        Vec::<String>::new()
    );
    for facade in [
        "pub use crate::log::CompleteCorePolicyPatchRequestPlan;",
        "pub use crate::log::decide_complete_core_policy_patch_frame;",
        "pub use wf_domain::policy::complete::CompleteCorePolicyPatchApplyView;",
        "pub use wf_domain::policy::complete::CompleteCorePolicyPatchFresh;",
        "pub use wf_domain::policy::complete::CompleteCorePolicyPatchApplyError;",
        "pub use wf_domain::policy::complete::prepare_complete_core_policy_patch_fresh;",
    ] {
        assert!(
            super::source_check("wf-ports", "client", facade)?
                .iter()
                .any(|finding| finding.contains("client port facade"))
        );
    }
    assert!(
        super::source_check(
            "wf-wire",
            "value",
            "pub use wf_domain::policy::complete::CompleteCorePolicyPatchApplyView;",
        )?
        .iter()
        .any(|finding| finding.contains("wire may reference only wf_domain::value"))
    );
    assert!(
        super::source_check(
            "wf-domain",
            "value",
            "pub use crate::policy::complete::CompleteCorePolicyPatchApplyView;",
        )?
        .iter()
        .any(|finding| finding.contains("approved domain value module"))
    );
    Ok(())
}

fn assert_complete_policy_outer_modules() -> CheckResult<()> {
    for module in [
        "policy::complete::outer::caller::core_patch",
        "policy::complete::outer::core_patch_frame",
        "policy::complete::outer::core_patch_frame::scan",
        "policy::complete::outer::core_patch_frame::decode",
        "policy::complete::outer::replace_frame",
        "policy::complete::outer::replace_frame::scan",
        "policy::complete::outer::replace_frame::decode",
    ] {
        assert_eq!(
            super::source_check(
                "wf-domain",
                module,
                "use std::collections::BTreeMap; use wf_types::identity;"
            )?,
            Vec::<String>::new()
        );
        assert!(
            super::source_check("wf-domain", module, "use std::env;")?
                .iter()
                .any(|finding| finding.contains("unclassified standard-library module"))
        );
    }
    Ok(())
}

fn assert_complete_policy_replace_content_modules() -> CheckResult<()> {
    for module in [
        "policy::complete::replace_content",
        "policy::complete::replace_content::core",
        "policy::complete::replace_content::governance",
        "policy::complete::replace_content::indexed",
        "policy::complete::replace_content::rejection",
    ] {
        assert_eq!(
            super::source_check(
                "wf-domain",
                module,
                "use std::collections::BTreeSet; use crate::retention;"
            )?,
            Vec::<String>::new()
        );
        assert!(
            super::source_check("wf-domain", module, "use std::env;")?
                .iter()
                .any(|finding| finding.contains("unclassified standard-library module"))
        );
    }
    for name in [
        "CompletePolicyReplaceRejection",
        "CompletePolicyReplaceRejectionKind",
        "check_complete_policy_replace_content",
        "encode_complete_policy_replace_rejection_body",
    ] {
        let exported = format!("pub use wf_domain::policy::complete::{name};");
        assert!(
            super::source_check("wf-ports", "client", &exported)?
                .iter()
                .any(|finding| finding.contains("client port facade"))
        );
        assert!(
            super::source_check("wf-wire", "value", &exported)?
                .iter()
                .any(|finding| finding.contains("wire may reference only wf_domain::value"))
        );
        assert!(
            super::source_check(
                "wf-domain",
                "value",
                &format!("pub use crate::policy::complete::{name};"),
            )?
            .iter()
            .any(|finding| finding.contains("approved domain value module"))
        );
    }
    Ok(())
}

#[test]
fn complete_replace_request_parent_module_and_client_boundaries_remain_closed() -> CheckResult<()> {
    assert_complete_policy_replace_parent_modules()?;
    for name in [
        "CompletePolicyReplaceRequestPlan",
        "decide_complete_policy_replace_frame",
    ] {
        let exported = format!("pub use crate::log::{name};");
        assert!(
            super::source_check("wf-ports", "client", &exported)?
                .iter()
                .any(|finding| finding.contains("client port facade"))
        );
    }
    for name in [
        "CompletePolicyReplaceApplyView",
        "CompletePolicyReplaceApplyError",
        "CompletePolicyReplaceFresh",
        "prepare_complete_policy_replace_fresh",
    ] {
        let exported = format!("pub use wf_domain::policy::complete::{name};");
        assert!(
            super::source_check("wf-ports", "client", &exported)?
                .iter()
                .any(|finding| finding.contains("client port facade"))
        );
        assert!(
            super::source_check("wf-wire", "value", &exported)?
                .iter()
                .any(|finding| finding.contains("wire may reference only wf_domain::value"))
        );
        assert!(
            super::source_check(
                "wf-domain",
                "value",
                &format!("pub use crate::policy::complete::{name};"),
            )?
            .iter()
            .any(|finding| finding.contains("approved domain value module"))
        );
    }
    let original = metadata()?;
    let rules = policy()?;
    let workspace = root()?;
    for (owner, target) in [("wf-domain", "wf-ports"), ("wf-ports", "wf-application")] {
        let mut backwards = original.clone();
        let mut back_edge = dependency();
        back_edge["name"] = json!(target);
        back_edge["source"] = Value::Null;
        back_edge["req"] = json!("*");
        back_edge["path"] = json!(workspace.join("crates").join(target));
        package_mut(&mut backwards, owner)?["dependencies"]
            .as_array_mut()
            .ok_or("missing dependencies")?
            .push(back_edge);
        assert_eq!(
            graph::check(&backwards, &rules, &workspace)?,
            vec![format!(
                "forbidden declared edge {owner} -> {target} (normal, optional=false, target=null)"
            )]
        );
    }
    Ok(())
}

fn assert_complete_policy_replace_parent_modules() -> CheckResult<()> {
    for module in [
        "policy::complete::replace_request",
        "policy::complete::replace_request::retention",
    ] {
        assert_eq!(
            super::source_check(
                "wf-domain",
                module,
                "use std::collections::BTreeMap; use wf_types::identity; use crate::workflow::request;",
            )?,
            Vec::<String>::new()
        );
        for (source, diagnosis) in [
            ("use std::env;", "unclassified standard-library module"),
            ("pub async fn effect() {}", "declares async behavior"),
            (
                "use wf_parity_tests::progress;",
                "runtime source references M0-only tooling",
            ),
        ] {
            assert!(
                super::source_check("wf-domain", module, source)?
                    .iter()
                    .any(|finding| finding.contains(diagnosis))
            );
        }
    }
    assert_eq!(
        super::source_check(
            "wf-ports",
            "log::complete_policy::replace::apply",
            "use wf_domain::policy::complete; use wf_types::version;",
        )?,
        Vec::<String>::new()
    );
    Ok(())
}

#[test]
fn complete_policy_indexed_ingress_stays_pure_and_server_side() -> CheckResult<()> {
    for module in [
        "policy::complete::indexed_input",
        "policy::complete::indexed_input::key",
    ] {
        assert_eq!(
            super::source_check(
                "wf-domain",
                module,
                "use std::str; use crate::policy::complete; use wf_types::identity;",
            )?,
            Vec::<String>::new()
        );
        for (source, diagnosis) in [
            ("use std::env;", "unclassified standard-library module"),
            ("pub async fn effect() {}", "declares async behavior"),
            (
                "use wf_parity_tests::progress;",
                "runtime source references M0-only tooling",
            ),
        ] {
            assert!(
                super::source_check("wf-domain", module, source)?
                    .iter()
                    .any(|finding| finding.contains(diagnosis))
            );
        }
    }
    for name in [
        "RawPolicyIndexedValue",
        "PolicyIndexedInputRejection",
        "normalize_complete_policy_indexed_input",
    ] {
        let exported = format!("pub use wf_domain::policy::complete::{name};");
        assert!(
            super::source_check("wf-ports", "client", &exported)?
                .iter()
                .any(|finding| finding.contains("client port facade"))
        );
        assert!(
            super::source_check("wf-wire", "value", &exported)?
                .iter()
                .any(|finding| finding.contains("wire may reference only wf_domain::value"))
        );
        assert!(
            super::source_check(
                "wf-domain",
                "value",
                &format!("pub use crate::policy::complete::{name};"),
            )?
            .iter()
            .any(|finding| finding.contains("approved domain value module"))
        );
    }
    Ok(())
}

#[test]
fn complete_policy_original_retry_key_inspection_stays_pure_and_server_side() -> CheckResult<()> {
    for module in [
        "policy::complete::original_retry_keys",
        "policy::complete::original_retry_keys::map",
    ] {
        assert_eq!(
            super::source_check(
                "wf-domain",
                module,
                "use std::str; use crate::policy::complete; use wf_types::identity;",
            )?,
            Vec::<String>::new()
        );
        for (source, diagnosis) in [
            ("use std::env;", "unclassified standard-library module"),
            ("pub async fn effect() {}", "declares async behavior"),
            (
                "use wf_parity_tests::progress;",
                "runtime source references M0-only tooling",
            ),
        ] {
            assert!(
                super::source_check("wf-domain", module, source)?
                    .iter()
                    .any(|finding| finding.contains(diagnosis))
            );
        }
    }
    for name in [
        "OriginalPolicyMapKey",
        "OriginalPolicyMapValue",
        "original_complete_policy_uses_max_attempts",
    ] {
        let exported = format!("pub use wf_domain::policy::complete::{name};");
        assert!(
            super::source_check("wf-ports", "client", &exported)?
                .iter()
                .any(|finding| finding.contains("client port facade"))
        );
        assert!(
            super::source_check("wf-wire", "value", &exported)?
                .iter()
                .any(|finding| finding.contains("wire may reference only wf_domain::value"))
        );
        assert!(
            super::source_check(
                "wf-domain",
                "value",
                &format!("pub use crate::policy::complete::{name};"),
            )?
            .iter()
            .any(|finding| finding.contains("approved domain value module"))
        );
    }
    Ok(())
}

#[test]
fn original_policy_entry_reader_stays_pure_and_server_side() -> CheckResult<()> {
    for module in [
        "policy::complete::original_entry",
        "policy::complete::original_entry::scan",
        "policy::complete::original_entry::scalar",
        "policy::complete::original_entry::compound",
        "policy::complete::original_entry::runtime",
        "policy::complete::original_entry::identity",
        "policy::complete::original_entry::visit",
        "policy::complete::original_entry::safety",
        "policy::complete::original_entry::callsite",
    ] {
        assert_eq!(
            super::source_check(
                "wf-domain",
                module,
                "use std::str; use crate::policy::complete; use wf_types::identity;",
            )?,
            Vec::<String>::new()
        );
        for (source, diagnosis) in [
            ("use std::env;", "unclassified standard-library module"),
            ("pub async fn effect() {}", "declares async behavior"),
            (
                "use wf_parity_tests::progress;",
                "runtime source references M0-only tooling",
            ),
        ] {
            assert!(
                super::source_check("wf-domain", module, source)?
                    .iter()
                    .any(|finding| finding.contains(diagnosis))
            );
        }
    }
    for name in [
        "OriginalEtfLocalFun",
        "OriginalEtfSafety",
        "OriginalPolicyEntry",
        "OriginalPolicyReadValue",
        "OriginalPolicyReadError",
        "decode_original_complete_policy_entry",
        "read_original_complete_policy_entry",
        "prepare_original_complete_policy_reference_bytes",
    ] {
        for (package, module, source, diagnosis) in [
            (
                "wf-ports",
                "client",
                format!("pub use wf_domain::policy::complete::{name};"),
                "client port facade",
            ),
            (
                "wf-wire",
                "value",
                format!("pub use wf_domain::policy::complete::{name};"),
                "wire may reference only wf_domain::value",
            ),
            (
                "wf-domain",
                "value",
                format!("pub use crate::policy::complete::{name};"),
                "approved domain value module",
            ),
        ] {
            assert!(
                super::source_check(package, module, &source)?
                    .iter()
                    .any(|finding| finding.contains(diagnosis))
            );
        }
    }
    assert_eq!(
        super::source_check(
            "wf-ports",
            "log::complete_policy::original_entry",
            "use sha2::{Digest, Sha256}; use wf_domain::policy::complete; use wf_types::version;",
        )?,
        Vec::<String>::new()
    );
    for name in [
        "OriginalPolicyReference",
        "prepare_original_complete_policy_reference",
    ] {
        assert!(
            super::source_check(
                "wf-ports",
                "client",
                &format!("pub use crate::log::{name};"),
            )?
            .iter()
            .any(|finding| finding.contains("client port facade"))
        );
    }
    Ok(())
}

mod original_policy;
mod record_batch;
