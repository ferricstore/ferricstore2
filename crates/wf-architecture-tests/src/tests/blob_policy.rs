//! Workflow value byte helpers retain purity and server-side boundaries.

use super::{CheckResult, source_check};

#[test]
fn workflow_value_bytes_stay_pure_and_server_side() -> CheckResult<()> {
    for module in [
        "blob",
        "blob::reference",
        "blob::reference::tag",
        "blob::encoding",
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
    assert!(!source_check("wf-ports", "client", "pub use crate::blob;")?.is_empty());
    assert!(!source_check("wf-worker", "worker", "use wf_ports::blob;")?.is_empty());
    Ok(())
}
