//! Workflow value byte helpers retain purity and server-side boundaries.

use super::{CheckResult, source_check};

#[test]
fn workflow_value_bytes_stay_pure_and_server_side() -> CheckResult<()> {
    for module in [
        "blob",
        "blob::reference",
        "blob::reference::tag",
        "blob::encoding",
        "consensus::value",
        "log::value_batch",
        "clock",
    ] {
        assert!(
            source_check(
                "wf-ports",
                module,
                r"use sha2::{Digest, Sha256};
                pub fn digest(value: &[u8]) -> [u8; 32] {
                    Sha256::digest(value).into()
                }
                pub fn owned(value: &[u8]) -> Vec<u8> { value.to_vec() }",
            )?
            .is_empty(),
            "safe value-byte operations must remain allowed in {module}"
        );
        for effect in ["use std::fs;", "use std::time;", "use std::thread;"] {
            assert!(
                source_check("wf-ports", module, effect)?
                    .iter()
                    .any(|failure| failure.contains("standard-library module")),
                "value-byte helpers must reject {effect} in {module}"
            );
        }
    }
    assert_ne!(
        source_check("wf-ports", "client", "pub use crate::blob;")?,
        Vec::<String>::new()
    );
    assert_ne!(
        source_check("wf-worker", "worker", "use wf_ports::blob;")?,
        Vec::<String>::new()
    );
    assert_overlay_metadata_rejections()?;
    assert_overlay_metadata_positives()?;
    Ok(())
}

fn assert_overlay_metadata_rejections() -> CheckResult<()> {
    let standard = "standard-library module";
    let facade = "approved domain value module";
    let mut rows = Vec::new();
    for namespace in ["std", "core"] {
        for suffix in [
            "array",
            "array as Hidden",
            "array::*",
            "array::IntoIter::*",
            "array::from_fn",
            "array::try_from_fn",
            "array::IntoIterBackend",
        ] {
            rows.push((
                "wf-domain",
                "history::values::budget",
                format!("use {namespace}::{suffix};"),
                standard,
            ));
        }
        for source in [
            format!("fn hidden() {{ let _ = {namespace}::array::IntoIter::new([]); }}"),
            format!(
                "use {namespace}::array::IntoIter as First; fn hidden() {{ let _ = First::new([]); }}"
            ),
            format!(
                "use {namespace}::array::IntoIter as First; use First as Second; fn hidden() {{ let _ = Second::new([]); }}"
            ),
            format!("use {namespace}::array as Hidden; type Header = Hidden::IntoIter<usize, 35>;"),
        ] {
            rows.push(("wf-domain", "history::values::decode", source, standard));
        }
    }
    for source in [
        r"type Header = alloc::array::IntoIter<usize, 35>;",
        r"type Header = std::array::IntoIter<std::fs::File, 35>;",
        r#"type Header = core::array::IntoIter<usize, { std::env::var("X"); 35 }>;"#,
    ] {
        rows.push((
            "wf-domain",
            "history::values::budget",
            source.to_owned(),
            standard,
        ));
    }
    for module in ["value", "value::original_flow"] {
        for source in [
            r"pub(crate) use crate::workflow::Record;",
            r"pub(in crate) fn hidden(_: crate::workflow::Record) {}",
            r"pub(crate) fn hidden() { let _ = crate::workflow::decide; }",
            r"pub(crate) fn hidden() { use crate::workflow::Record as Hidden; }",
            r"use crate;",
        ] {
            rows.push(("wf-domain", module, source.to_owned(), facade));
        }
        for source in [
            r#"pub(crate) fn decode_fixed_bytes() { let _ = std::fs::read("private"); }"#,
            r#"pub(in crate) fn hidden() { let _ = std::env::var("PRIVATE"); }"#,
        ] {
            rows.push(("wf-domain", module, source.to_owned(), standard));
        }
    }
    for (package, module, source, diagnosis) in [
        (
            "wf-ports",
            "client",
            "pub(crate) use crate::blob::WorkflowValueReadBatch;",
            "client port facade",
        ),
        (
            "wf-ports",
            "client",
            "pub(crate) use wf_domain::history::values;",
            "client port facade",
        ),
        (
            "wf-wire",
            "value",
            "pub(crate) type Hidden = wf_domain::history::HistoryEntry;",
            "wire may reference only wf_domain::value",
        ),
        (
            "wf-worker",
            "client",
            "pub(crate) mod http;",
            "worker client transport modules must remain private",
        ),
    ] {
        rows.push((package, module, source.to_owned(), diagnosis));
    }
    for (package, module, source, diagnosis) in rows {
        let findings = source_check(package, module, &source)?;
        assert!(
            findings.iter().any(|finding| finding.contains(diagnosis)),
            "{package}::{module}: {source}: expected {diagnosis}, got {findings:?}"
        );
    }
    Ok(())
}

fn assert_overlay_metadata_positives() -> CheckResult<()> {
    let mut observed = Vec::new();
    for (module, source) in [
        (
            "history::values::budget",
            r"pub fn bytes() -> usize { std::mem::size_of::<std::array::IntoIter<usize, 35>>() }",
        ),
        (
            "history::values::decode",
            r"fn bytes() -> usize { core::mem::size_of::<core::array::IntoIter<(usize, usize), 14>>() }",
        ),
        (
            "history::values::budget",
            r"type Header = (std::iter::Empty<usize>, std::slice::Iter<'static, u8>, std::vec::IntoIter<u8>); fn bytes() -> usize { std::mem::size_of::<Header>() }",
        ),
        (
            "history::values::budget",
            r"use std::array::IntoIter; type Header = IntoIter<usize, 35>;",
        ),
        (
            "history::values::decode",
            r"use core::array::IntoIter; type Header = IntoIter<(usize, usize), 6>;",
        ),
        (
            "history::values::budget",
            r"use std::array::IntoIter as Header; fn bytes() -> usize { std::mem::size_of::<Header<usize, 35>>() }",
        ),
        (
            "history::values::decode",
            r"use core::array::IntoIter as Header; fn bytes() -> usize { core::mem::size_of::<Header<(usize, usize), 14>>() }",
        ),
        (
            "history::values::budget",
            r"use std::array::IntoIter as First; use First as Second; type Header = Second<usize, 35>;",
        ),
        (
            "history::values::decode",
            r"use core::array::IntoIter as First; use First as Second; type Header = Second<(usize, usize), 6>;",
        ),
        (
            "value",
            r"pub(crate) use original_flow::decode_fixed_bytes;",
        ),
        (
            "value",
            r"pub(in crate) use original_flow::decode_fixed_bytes;",
        ),
        (
            "value::original_flow",
            r"pub(crate) fn decode_fixed_bytes() -> Result<usize, OriginalFlowValueError> { Ok(std::mem::size_of::<usize>()) }",
        ),
        (
            "value::original_flow",
            r"pub(in crate) fn decode_fixed_bytes() -> Result<usize, OriginalFlowValueError> { Ok(core::mem::size_of::<usize>()) }",
        ),
        (
            "value::original_flow::equality",
            r"pub(in crate::value::original_flow) fn fixed_owner_bytes() -> Result<usize, crate::value::OriginalFlowValueError> { Ok(std::mem::size_of::<usize>()) }",
        ),
    ] {
        // Collect even Err/findings rows before inspecting any positive result.
        observed.push((module, source, source_check("wf-domain", module, source)));
    }
    let diagnostics = format!("{observed:#?}");
    let mut clean = true;
    for (_, _, findings) in observed {
        if !findings?.is_empty() {
            clean = false;
        }
    }
    assert!(
        clean,
        "all metadata/visibility positive rows: {diagnostics}"
    );
    Ok(())
}
