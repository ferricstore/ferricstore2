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

use wf_domain::lineage::caller::{exhaust_to_opts, validate_children, validate_no_parent_child_id};
use wf_domain::policy::complete::RawFlowPolicyValue as Raw;

// Remaining pinned declarations ad58b42e24cd22f6, c19941c513d4a376,
// aa52ad13e737fc75 and 42748dd4f4677cbc retain source assertions in order.
// Source async:true/flow/no-local-setup provenance is shared with declaration981.

#[test]
fn validate_children_requires_a_non_empty_list() {
    let child = [(Raw::Atom("id"), Raw::Binary(b"child"))];
    let children = [Raw::Map(&child)];
    assert_eq!(validate_children(Raw::List(&children)), Ok(()));
    assert_eq!(
        validate_children(Raw::List(&[])),
        Err("ERR flow children must be a non-empty list")
    );
}

#[test]
fn validate_no_parent_child_id_rejects_parent_id_as_child_id() {
    let child = [(Raw::Atom("id"), Raw::Binary(b"child"))];
    let parent = [(Raw::Atom("id"), Raw::Binary(b"parent"))];
    assert_eq!(validate_no_parent_child_id(b"parent", &[&child]), Ok(()));
    assert_eq!(
        validate_no_parent_child_id(b"parent", &[&parent]),
        Err("ERR flow child id must differ from parent id")
    );
}

#[test]
fn exhaust_to_opts_accepts_explicit_map_or_success_failure_options() {
    let atoms = [
        (Raw::Atom("success"), Raw::Binary(b"done")),
        (Raw::Atom("failure"), Raw::Binary(b"failed")),
    ];
    let binaries = [
        (Raw::Binary(b"success"), Raw::Binary(b"done")),
        (Raw::Binary(b"failure"), Raw::Binary(b"failed")),
    ];
    let expected = Ok((b"done".as_slice(), b"failed".as_slice()));
    assert_eq!(
        exhaust_to_opts(&[
            ("success", Raw::Binary(b"done")),
            ("failure", Raw::Binary(b"failed")),
        ]),
        expected
    );
    assert_eq!(
        exhaust_to_opts(&[("exhaust_to", Raw::Map(&atoms))]),
        expected
    );
    assert_eq!(
        exhaust_to_opts(&[("exhaust_to", Raw::Map(&binaries))]),
        expected
    );
}

#[test]
fn exhaust_to_opts_rejects_missing_or_empty_states() {
    assert_eq!(
        exhaust_to_opts(&[]),
        Err("ERR flow exhaust_to must include success and failure states")
    );
    assert_eq!(
        exhaust_to_opts(&[
            ("success", Raw::Binary(b"")),
            ("failure", Raw::Binary(b"failed")),
        ]),
        Err("ERR flow exhaust_to must include success and failure states")
    );
}

#[test]
fn validate_children_preserves_shallow_nonempty_cons_semantics() {
    let arbitrary = [Raw::Integer(7), Raw::Other];
    let tail = Raw::Binary(b"not-a-list-tail");
    assert_eq!(validate_children(Raw::List(&arbitrary)), Ok(()));
    assert_eq!(
        validate_children(Raw::ImproperList {
            prefix: &arbitrary,
            tail: &tail,
        }),
        Ok(())
    );
    for rejected in [
        Raw::List(&[]),
        Raw::Nil,
        Raw::Atom("nil"),
        Raw::Binary(b"children"),
        Raw::Integer(1),
        Raw::Map(&[]),
        Raw::Tuple(&[]),
        Raw::Other,
        Raw::ImproperList {
            prefix: &[],
            tail: &tail,
        },
    ] {
        assert_eq!(
            validate_children(rejected),
            Err("ERR flow children must be a non-empty list")
        );
    }
}

#[test]
fn validate_no_parent_child_id_uses_atom_id_and_binary_equality() {
    let binary_key = [(Raw::Binary(b"id"), Raw::Binary(b"parent"))];
    let nonbinary = [(Raw::Atom("id"), Raw::Integer(7))];
    let missing = [];
    let child = [
        (Raw::Atom("id"), Raw::Binary(b"child")),
        (Raw::Binary(b"id"), Raw::Binary(b"parent")),
    ];
    assert_eq!(
        validate_no_parent_child_id(b"parent", &[&binary_key, &nonbinary, &missing, &child]),
        Ok(())
    );
    assert_eq!(
        validate_no_parent_child_id(b"parent", &[&child, &child]),
        Ok(())
    );
    assert_eq!(validate_no_parent_child_id(b"parent", &[]), Ok(()));
    let opaque = [0xff, 0];
    let opaque_child = [(Raw::Atom("id"), Raw::Binary(&opaque))];
    assert_eq!(
        validate_no_parent_child_id(&opaque, &[&opaque_child]),
        Err("ERR flow child id must differ from parent id")
    );
    assert_eq!(
        validate_no_parent_child_id(&[0xff, 1], &[&opaque_child]),
        Ok(())
    );
    let empty = [(Raw::Atom("id"), Raw::Binary(b""))];
    assert_eq!(validate_no_parent_child_id(b"", &[&missing]), Ok(()));
    assert_eq!(
        validate_no_parent_child_id(b"", &[&empty]),
        Err("ERR flow child id must differ from parent id")
    );
}

#[test]
fn exhaust_to_opts_preserves_first_option_and_map_key_precedence() {
    let expected = Ok((b"done".as_slice(), b"failed".as_slice()));
    let error = Err("ERR flow exhaust_to must include success and failure states");
    for nil in [Raw::Nil, Raw::Atom("nil")] {
        assert_eq!(
            exhaust_to_opts(&[
                ("exhaust_to", nil),
                ("exhaust_to", Raw::Other),
                ("success", Raw::Binary(b"done")),
                ("success", Raw::Other),
                ("failure", Raw::Binary(b"failed")),
                ("failure", Raw::Other),
            ]),
            expected
        );
    }
    let atom_pair = [
        (Raw::Atom("success"), Raw::Integer(7)),
        (Raw::Atom("failure"), Raw::Binary(b"failed")),
        (Raw::Binary(b"success"), Raw::Binary(b"done")),
        (Raw::Binary(b"failure"), Raw::Binary(b"failed")),
    ];
    assert_eq!(
        exhaust_to_opts(&[("exhaust_to", Raw::Map(&atom_pair))]),
        error
    );
    let partial_atom = [
        (Raw::Atom("success"), Raw::Other),
        (Raw::Binary(b"success"), Raw::Binary(b"done")),
        (Raw::Binary(b"failure"), Raw::Binary(b"failed")),
        (Raw::Atom("extra"), Raw::Other),
    ];
    assert_eq!(
        exhaust_to_opts(&[("exhaust_to", Raw::Map(&partial_atom))]),
        expected
    );
    let mixed = [
        (Raw::Atom("success"), Raw::Binary(b"done")),
        (Raw::Binary(b"failure"), Raw::Binary(b"failed")),
    ];
    assert_eq!(exhaust_to_opts(&[("exhaust_to", Raw::Map(&mixed))]), error);
    for nonnil in [
        Raw::Other,
        Raw::List(&[]),
        Raw::Binary(b"override"),
        Raw::Map(&mixed),
    ] {
        assert_eq!(
            exhaust_to_opts(&[
                ("exhaust_to", nonnil),
                ("exhaust_to", Raw::Map(&partial_atom)),
                ("success", Raw::Binary(b"done")),
                ("failure", Raw::Binary(b"failed")),
            ]),
            error
        );
    }
}

#[test]
fn exhaust_to_opts_preserves_opaque_state_bytes_and_rejects_nonbinary_states() {
    let success = [0xff, 0, b' '];
    let failure = [b'\t', 0xfe];
    let states = [
        (Raw::Atom("success"), Raw::Binary(&success)),
        (Raw::Atom("failure"), Raw::Binary(&failure)),
    ];
    let result = exhaust_to_opts(&[("exhaust_to", Raw::Map(&states))]);
    assert_eq!(result, Ok((success.as_slice(), failure.as_slice())));
    assert!(
        result.is_ok_and(|(s, f)| s.as_ptr() == success.as_ptr() && f.as_ptr() == failure.as_ptr())
    );
    assert_eq!(
        exhaust_to_opts(&[
            ("success", Raw::Binary(b"running")),
            ("failure", Raw::Binary(b" failed ")),
        ]),
        Ok((b"running".as_slice(), b" failed ".as_slice()))
    );
    for invalid in [
        Raw::Nil,
        Raw::Atom("done"),
        Raw::Integer(7),
        Raw::Other,
        Raw::List(&[]),
        Raw::Binary(b""),
    ] {
        for opts in [
            [("success", invalid), ("failure", Raw::Binary(b"failed"))],
            [("success", Raw::Binary(b"done")), ("failure", invalid)],
        ] {
            assert_eq!(
                exhaust_to_opts(&opts),
                Err("ERR flow exhaust_to must include success and failure states")
            );
        }
    }
}
