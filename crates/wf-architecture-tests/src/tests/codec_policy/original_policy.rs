use super::super::{CheckResult, source_check};

#[test]
fn original_current_replace_stays_pure_and_server_side() -> CheckResult<()> {
    let module = "policy::complete::replace_request::original";
    assert_eq!(
        source_check(
            "wf-domain",
            module,
            "use std::collections::BTreeMap; use crate::policy::complete; use wf_types::version;",
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
            source_check("wf-domain", module, source)?
                .iter()
                .any(|finding| finding.contains(diagnosis))
        );
    }
    for name in [
        "OriginalCompletePolicyReplaceApplyView",
        "OriginalCompletePolicyReplaceApplyError",
        "prepare_original_complete_policy_replace_fresh",
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
                source_check(package, module, &source)?
                    .iter()
                    .any(|finding| finding.contains(diagnosis))
            );
        }
    }
    assert_eq!(
        source_check(
            "wf-ports",
            "log::complete_policy::replace::apply",
            "use wf_domain::policy::complete; use wf_types::version;",
        )?,
        Vec::<String>::new()
    );
    assert!(
        source_check(
            "wf-ports",
            "client",
            "pub use crate::log::decide_original_complete_policy_replace_frame;",
        )?
        .iter()
        .any(|finding| finding.contains("client port facade"))
    );
    Ok(())
}

#[test]
fn original_retry_input_stays_pure_and_server_side() -> CheckResult<()> {
    for module in [
        "policy::complete::retry_input",
        "policy::complete::retry_input::value",
    ] {
        assert_eq!(
            source_check(
                "wf-domain",
                module,
                "use crate::policy::complete; use wf_types::version;",
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
                source_check("wf-domain", module, source)?
                    .iter()
                    .any(|finding| finding.contains(diagnosis))
            );
        }
    }
    for name in [
        "RawPolicyRetryValue",
        "PolicyRetryInputRejection",
        "normalize_complete_policy_retry_input",
        "normalize_complete_policy_retry_override_input",
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
                source_check(package, module, &source)?
                    .iter()
                    .any(|finding| finding.contains(diagnosis))
            );
        }
    }
    Ok(())
}

#[test]
fn original_retention_input_stays_pure_and_server_side() -> CheckResult<()> {
    let module = "policy::complete::retention_input";
    assert_eq!(
        source_check(
            "wf-domain",
            module,
            "use crate::policy::complete; use wf_types::version;",
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
            source_check("wf-domain", module, source)?
                .iter()
                .any(|finding| finding.contains(diagnosis))
        );
    }
    for name in [
        "PolicyRetentionInputRejection",
        "encode_complete_policy_retention_input_rejection_body",
        "normalize_complete_policy_retention_override_input",
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
                source_check(package, module, &source)?
                    .iter()
                    .any(|finding| finding.contains(diagnosis))
            );
        }
    }
    Ok(())
}

#[test]
fn original_flow_policy_input_stays_pure_and_server_side() -> CheckResult<()> {
    for module in [
        "policy::complete::flow_input",
        "policy::complete::flow_input::value",
        "policy::complete::flow_input::preflight",
        "policy::complete::flow_input::layout",
        "policy::complete::flow_input::core",
        "policy::complete::flow_input::states",
        "policy::complete::flow_input::governance",
        "policy::complete::flow_input::governance::groups",
        "policy::complete::flow_input::governance::rules",
        "policy::complete::flow_input::governance::lists",
        "policy::complete::flow_input::etf",
        "policy::complete::flow_input::rejection",
        "policy::complete::flow_input::order",
        "policy::complete::flow_input::order::hash",
    ] {
        assert_eq!(
            source_check(
                "wf-domain",
                module,
                "use crate::policy::complete; use wf_types::version;",
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
                source_check("wf-domain", module, source)?
                    .iter()
                    .any(|finding| finding.contains(diagnosis))
            );
        }
    }
    for name in [
        "RawFlowPolicyValue",
        "FlowPolicyInputRejection",
        "GovernancePolicyInputRejection",
        "normalize_original_complete_policy_input",
        "encode_original_complete_policy_input_rejection_body",
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
                source_check(package, module, &source)?
                    .iter()
                    .any(|finding| finding.contains(diagnosis))
            );
        }
    }
    Ok(())
}

#[test]
fn original_flow_policy_parent_stays_pure_and_server_side() -> CheckResult<()> {
    for module in [
        "policy::complete::outer::flow_frame",
        "policy::complete::outer::flow_frame::model",
        "policy::complete::outer::flow_frame::raw",
        "policy::complete::outer::flow_frame::canonical",
        "policy::complete::outer::flow_frame::codec",
        "policy::complete::outer::flow_frame::scan",
        "policy::complete::replace_request::flow",
        "policy::complete::replace_request::flow::retention",
        "policy::complete::flow_input::etf::envelope",
    ] {
        assert_eq!(
            source_check(
                "wf-domain",
                module,
                "use crate::policy::complete; use std::ops::Range; use wf_types::version;",
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
                source_check("wf-domain", module, source)?
                    .iter()
                    .any(|finding| finding.contains(diagnosis))
            );
        }
    }
    for name in [
        "OriginalFlowPolicyReplaceInput",
        "ScannedOriginalFlowPolicyReplaceFrame",
        "OriginalFlowPolicyReplaceFrameHeader",
        "OriginalFlowPolicyInstall",
        "OriginalFlowPolicyReplaceFresh",
        "OriginalFlowPolicyReplaceRejection",
        "prepare_original_flow_policy_replace_frame_bytes",
        "scan_original_flow_policy_replace_frame",
        "restore_original_flow_policy_replace_frame_header",
        "prepare_original_flow_policy_replace_fresh",
    ] {
        original_flow_export_is_server_only(name)?;
    }
    for module in [
        "log::complete_policy::flow",
        "log::complete_policy::flow::apply",
    ] {
        assert_eq!(
            source_check(
                "wf-ports",
                module,
                "use wf_domain::policy::complete; use wf_types::version;",
            )?,
            Vec::<String>::new()
        );
    }
    for name in [
        "VerifiedOriginalFlowPolicyReplaceFrame",
        "OriginalFlowPolicyReplaceRequestPlan",
        "prepare_original_flow_policy_replace_frame",
        "verify_original_flow_policy_replace_frame",
        "decide_original_flow_policy_replace_frame",
    ] {
        assert!(
            source_check(
                "wf-ports",
                "client",
                &format!("pub use crate::log::{name};")
            )?
            .iter()
            .any(|finding| finding.contains("client port facade"))
        );
    }
    Ok(())
}

fn original_flow_export_is_server_only(name: &str) -> CheckResult<()> {
    for (package, module, prefix, diagnosis) in [
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
                &format!("pub use {prefix}::policy::complete::{name};"),
            )?
            .iter()
            .any(|finding| finding.contains(diagnosis))
        );
    }
    Ok(())
}

#[test]
fn original_public_policy_set_preparation_stays_pure_and_server_side() -> CheckResult<()> {
    for module in [
        "policy::complete::public_set",
        "policy::complete::public_set::model",
        "policy::complete::public_set::patch",
        "policy::complete::public_set::patch::rows",
        "policy::complete::public_set::metric",
    ] {
        assert_eq!(
            source_check(
                "wf-domain",
                module,
                "use crate::policy::complete; use wf_types::version;",
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
                source_check("wf-domain", module, source)?
                    .iter()
                    .any(|finding| finding.contains(diagnosis))
            );
        }
    }
    for name in [
        "OriginalPolicySetRequest",
        "OriginalPolicySetPatchView",
        "OriginalPolicySetInputRejection",
        "prepare_original_policy_set_request",
        "encode_original_policy_set_input_rejection_body",
    ] {
        original_flow_export_is_server_only(name)?;
    }
    Ok(())
}
