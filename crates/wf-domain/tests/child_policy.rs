//! Pinned `ChildPolicyTest` declaration 981b590378d8a60d, source line 21.
//! Source: `apps/ferricstore/test/ferricstore/flow/child_policy_test.exs` at
//! 66586c596e271408d9184af32eb31cd7218c126e. Source async: true, tag: flow;
//! there is no local setup or shared mutable fixture.

#![forbid(unsafe_code)]

use wf_domain::lineage::ChildFailure::{FailParent, Ignore};
use wf_domain::lineage::caller::ChildFailureValue::{Atom, Binary, Other};
use wf_domain::lineage::caller::optional_child_failure;

#[test]
fn optional_policy_normalizes_binary_values_and_rejects_unsupported_values() {
    assert!(matches!(
        optional_child_failure(
            &[("on_child_failure", Binary("fail-parent"))],
            "on_child_failure",
            Atom(Ignore),
            &[Ignore, FailParent],
        ),
        Ok(FailParent)
    ));

    assert_eq!(
        optional_child_failure(
            &[("on_child_failure", Binary("bad"))],
            "on_child_failure",
            Atom(Ignore),
            &[Ignore, FailParent],
        )
        .err()
        .map(|error| error.to_string()),
        Some("ERR flow on_child_failure has unsupported value".to_owned())
    );
}

#[test]
fn optional_child_failure_preserves_defaults_atoms_and_allowed_membership() {
    let key = "on_child_failure";
    let allowed = [Ignore, FailParent];
    for default in allowed {
        assert!(
            optional_child_failure(&[], key, Atom(default), &allowed)
                .is_ok_and(|value| value == default)
        );
    }
    assert!(matches!(
        optional_child_failure(&[(key, Atom(Ignore))], key, Atom(FailParent), &allowed),
        Ok(Ignore)
    ));
    assert!(optional_child_failure(&[(key, Other)], key, Atom(Ignore), &allowed).is_err());
    assert!(optional_child_failure(&[], key, Other, &allowed).is_err());
    for restricted in [&[][..], &[FailParent][..]] {
        assert!(
            optional_child_failure(&[(key, Atom(Ignore))], key, Atom(FailParent), restricted)
                .is_err()
        );
    }
    assert!(matches!(
        optional_child_failure(&[], key, Atom(Ignore), &[FailParent, Ignore]),
        Ok(Ignore)
    ));
}

#[test]
fn optional_child_failure_preserves_keyword_first_match_and_field_names() {
    let opts = [
        ("ON_CHILD_FAILURE", Other),
        ("on_child_failure", Atom(Ignore)),
        ("on_child_failure", Other),
        ("on_child_failed", Atom(FailParent)),
    ];
    assert!(matches!(
        optional_child_failure(
            &opts,
            "on_child_failure",
            Atom(FailParent),
            &[Ignore, FailParent]
        ),
        Ok(Ignore)
    ));
    assert!(matches!(
        optional_child_failure(
            &opts,
            "on_child_failed",
            Atom(Ignore),
            &[Ignore, FailParent]
        ),
        Ok(FailParent)
    ));
    for (key, diagnostic) in [
        (
            "on_child_failure",
            "ERR flow on_child_failure has unsupported value",
        ),
        (
            "on_child_failed",
            "ERR flow on_child_failed has unsupported value",
        ),
    ] {
        assert_eq!(
            optional_child_failure(
                &[(key, Other), (key, Atom(Ignore))],
                key,
                Atom(FailParent),
                &[Ignore, FailParent],
            )
            .err()
            .map(|error| error.to_string()),
            Some(diagnostic.to_owned())
        );
    }
}

#[test]
fn optional_child_failure_accepts_case_and_hyphen_aliases_without_trimming() {
    let key = "on_child_failure";
    let allowed = [Ignore, FailParent];
    for (name, expected) in [
        ("ignore", Ignore),
        ("IGNORE", Ignore),
        ("fail-parent", FailParent),
        ("FAIL-PARENT", FailParent),
        ("fail_parent", FailParent),
        ("Fail_Parent", FailParent),
    ] {
        assert!(
            optional_child_failure(&[(key, Binary(name))], key, Atom(Ignore), &allowed)
                .is_ok_and(|value| value == expected)
        );
        assert!(
            optional_child_failure(&[], key, Binary(name), &allowed)
                .is_ok_and(|value| value == expected)
        );
    }
    for name in [
        "bad",
        " fail-parent",
        "fail-parent ",
        "fail--parent",
        "fail parent",
    ] {
        assert!(
            optional_child_failure(&[(key, Binary(name))], key, Atom(Ignore), &allowed).is_err()
        );
    }
}
