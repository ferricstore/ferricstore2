use super::super::{CheckResult, source_check};

#[test]
fn record_batch_parent_stays_pure_and_server_side() -> CheckResult<()> {
    assert_record_batch_effect_boundary()?;
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
    assert_record_batch_exports()?;
    for module in [
        "workflow::request",
        "workflow::indexed",
        "workflow::indexed::decision",
        "workflow::batch::authority",
        "workflow::indexed::authority",
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

fn assert_record_batch_effect_boundary() -> CheckResult<()> {
    for module in [
        "log::record_batch",
        "log::record_batch::intent",
        "log::record_batch::phase",
        "log::record_batch::phase::nested",
        "log::record_batch::fresh",
        "log::record_batch::fresh::nested",
        "log::record_batch::independent",
        "log::record_batch::independent::nested",
        "log::record_batch::response::independent",
        "log::record_batch::response::independent::nested",
    ] {
        assert_eq!(
            source_check("wf-ports", module, "use std::collections::BTreeMap;")?,
            Vec::<String>::new()
        );
        for (source, diagnosis) in [
            ("use std::fs;", "standard-library module"),
            (
                "use std as hidden; pub use hidden::env;",
                "standard-library module",
            ),
            ("pub async fn effect() {}", "declares async behavior"),
            ("fn hidden() { future.await; }", "awaits effects"),
            ("static GLOBAL: u64 = 0;", "declares global state"),
            ("fn hidden() { unsafe {} }", "unsafe block"),
            (
                "fn hidden() { let _ = format!(\"{}\", std::env::var(\"X\")); }",
                "standard-library module",
            ),
        ] {
            let findings = source_check("wf-ports", module, source)?;
            assert!(
                findings.iter().any(|finding| finding.contains(diagnosis)),
                "{module}: missing {diagnosis}: {findings:?}"
            );
        }
    }
    for neighbor in [
        "log::record_batch::response_backend",
        "log::record_batch::phase_backend",
        "log::record_batch::fresh_backend",
        "log::record_batch::independent_backend",
        "log::record_batch::response::independent_backend",
    ] {
        assert_eq!(
            source_check(
                "wf-ports",
                neighbor,
                "use std::fs; pub async fn effect() {} static GLOBAL: u64 = 0;"
            )?,
            Vec::<String>::new()
        );
    }
    Ok(())
}

fn assert_record_batch_exports() -> CheckResult<()> {
    for name in [
        "RecordBatchApplyPlan",
        "RecordBatchApplyDecision",
        "RecordBatchApplyError",
        "RecordBatchInspection",
        "FreshRecordBatch",
        "encode_record_batch",
        "decide_record_batch",
        "prepare_record_batch",
        "inspect_record_batch",
        "inspect_record_batch_point",
        "decide_fresh_record_batch",
        "decide_fresh_record_batch_touched",
        "record_batch_retained_status",
        "encode_independent_record_batch",
        "prepare_independent_record_batch",
        "inspect_independent_record_batch_point",
        "decide_fresh_independent_record_batch_touched",
        "record_batch_retained_summary",
        "independent_record_batch_response_matches",
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
