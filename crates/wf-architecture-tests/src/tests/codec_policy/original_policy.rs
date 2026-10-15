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
