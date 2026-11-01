//! Source `PayloadReturn` flag/maximum behavior against explicit actual captures.
#![forbid(unsafe_code)]
use wf_domain::{
    options::{
        normalize_original_history_payload_return as history,
        normalize_original_payload_return as payload,
    },
    policy::complete::RawFlowPolicyValue as R,
};
mod validation;
type TestResult = Result<(), Box<dyn std::error::Error>>;
#[test]
fn payload_return_preserves_full_payload_defaults_and_first_occurrence() -> TestResult {
    assert!(!payload(&[], false, R::Integer(8))?.enabled());
    assert!(payload(&[], true, R::Integer(8))?.enabled());
    assert!(payload(&[("full", R::Atom("true"))], false, R::Integer(8))?.enabled());
    assert!(
        !payload(
            &[
                ("full", R::Atom("true")),
                ("payload", R::Atom("false")),
                ("payload", R::Atom("true"))
            ],
            false,
            R::Integer(8)
        )?
        .enabled()
    );
    Ok(())
}
#[test]
fn payload_return_validates_the_selected_maximum_even_when_disabled() -> TestResult {
    let result = payload(
        &[
            ("payload", R::Atom("false")),
            ("payload_max_bytes", R::Integer(9)),
        ],
        true,
        R::Integer(8),
    );
    assert_eq!(
        result.err().ok_or("expected error")?.to_string(),
        "ERR flow payload_max_bytes exceeds maximum 8"
    );
    Ok(())
}
#[test]
fn history_payload_return_uses_values_and_the_same_selected_maximum() -> TestResult {
    let result = history(
        &[
            ("values", R::Atom("true")),
            ("payload_max_bytes", R::Integer(4)),
            ("payload", R::Atom("false")),
        ],
        R::Integer(8),
    )?;
    assert!(result.enabled());
    assert_eq!(result.maximum_bytes(), 4);
    Ok(())
}
