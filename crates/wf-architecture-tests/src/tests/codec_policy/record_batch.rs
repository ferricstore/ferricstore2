use super::super::{CheckResult, source_check};

#[test]
fn record_batch_parent_stays_pure_and_server_side() -> CheckResult<()> {
    for module in [
        "log::record_batch",
        "log::record_batch::intent",
        "log::record_batch::response",
    ] {
        assert_eq!(
            source_check(
                "wf-ports",
                module,
                "use std::collections::BTreeMap; use sha2::{Digest, Sha256}; use wf_domain::workflow; use wf_types::version;"
            )?,
            Vec::<String>::new()
        );
        assert!(
            source_check("wf-ports", module, "use wf_parity_tests::progress;")?
                .iter()
                .any(|finding| finding.contains("runtime source references M0-only tooling"))
        );
    }
    for name in [
        "RecordBatchApplyPlan",
        "RecordBatchApplyDecision",
        "RecordBatchApplyError",
        "encode_record_batch",
        "decide_record_batch",
        "record_batch_retained_status",
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
    for module in [
        "workflow::request",
        "workflow::indexed",
        "workflow::indexed::decision",
    ] {
        assert_eq!(
            source_check(
                "wf-domain",
                module,
                "use std::collections::BTreeMap; use wf_types::version; use crate::workflow::batch;"
            )?,
            Vec::<String>::new()
        );
        for (source, diagnosis) in [
            ("use std::env;", "unclassified standard-library module"),
            ("pub async fn effect() {}", "declares async behavior"),
        ] {
            assert!(
                source_check("wf-domain", module, source)?
                    .iter()
                    .any(|finding| finding.contains(diagnosis))
            );
        }
    }
    assert!(
        source_check(
            "wf-wire",
            "value",
            "pub use wf_domain::workflow::request::ClientIntent;"
        )?
        .iter()
        .any(|finding| finding.contains("wire may reference only wf_domain::value"))
    );
    assert!(
        source_check(
            "wf-domain",
            "value",
            "pub use crate::workflow::request::ClientIntent;"
        )?
        .iter()
        .any(|finding| finding.contains("approved domain value module"))
    );
    Ok(())
}
