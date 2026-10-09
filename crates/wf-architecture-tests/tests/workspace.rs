//! Checks the real repository rather than a hand-constructed passing graph.
#![forbid(unsafe_code)]

use std::{error::Error, path::Path};

#[test]
fn complete_workspace_obeys_policy() -> Result<(), Box<dyn Error>> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()?;
    let violations = wf_architecture_tests::check_workspace(&root)?;
    assert!(
        violations.is_empty(),
        "architecture violations: {violations:#?}"
    );
    Ok(())
}
