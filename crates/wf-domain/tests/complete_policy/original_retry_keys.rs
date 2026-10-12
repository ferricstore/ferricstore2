//! Original deprecated-key semantics, without an ETF/current-reader claim.

use wf_domain::policy::complete::{
    CompletePolicyEncodingError as Error, OriginalPolicyMapKey as Key,
    OriginalPolicyMapValue as Value, PolicySchemaCharge, PolicySchemaError, PolicySchemaLimits,
    original_complete_policy_uses_max_attempts,
};

use super::{TestResult, limits};

mod refusals;

fn old_retry() -> [(Key<'static>, Value<'static>); 1] {
    [(Key::Atom("max_attempts"), Value::Other)]
}

fn check(
    policy: &[(Key<'_>, Value<'_>)],
    profile: PolicySchemaLimits,
    charge: &mut PolicySchemaCharge,
) -> TestResult<bool> {
    Ok(original_complete_policy_uses_max_attempts(
        policy, profile, charge,
    )?)
}

#[test]
fn original_retry_keys_preserves_top_atom_precedence_and_short_circuit() -> TestResult {
    let profile = limits(20_000_000)?;
    let mut charge = PolicySchemaCharge::new(profile);
    let old = old_retry();
    let bad_state = [(Key::Binary(b"retry"), Value::Map(&old))];
    let states = [(Key::Other, Value::Map(&bad_state))];
    for name in ["retry", "states"] {
        let bad = if name == "retry" {
            &old[..]
        } else {
            &states[..]
        };
        for reversed in [false, true] {
            let mut policy = [
                (Key::Binary(name.as_bytes()), Value::Map(bad)),
                (Key::Atom(name), Value::Other),
            ];
            if reversed {
                policy.reverse();
            }
            assert!(!check(&policy, profile, &mut charge)?);
            assert_eq!(policy.len(), 2);
            assert!(matches!(policy[usize::from(!reversed)].1, Value::Other));
            assert!(matches!(old[0], (Key::Atom("max_attempts"), Value::Other)));
        }
    }
    let exact = limits(2_441)?;
    let mut exact_charge = PolicySchemaCharge::new(exact);
    let policy = [(Key::Atom("retry"), Value::Map(&old))];
    assert!(check(&policy, exact, &mut exact_charge)?);
    assert_eq!(
        (exact_charge.g(), exact_charge.p(), exact_charge.total()),
        (2_313, 128, 2_441)
    );
    // Any extra semantic states lookup would exceed this independently fixed budget.
    assert!(matches!(policy[0].1, Value::Map(map) if map.len() == 1));
    assert_eq!(policy.len(), 1);
    if let Value::Map(map) = policy[0].1 {
        assert!(std::ptr::eq(map.as_ptr(), old.as_ptr()));
    }
    Ok(())
}

#[test]
fn original_retry_keys_preserves_guarded_state_retry_fallback() -> TestResult {
    let profile = limits(20_000_000)?;
    let mut charge = PolicySchemaCharge::new(profile);
    let old = old_retry();
    let clean = [(Key::Atom("max_retries"), Value::Other)];
    for reversed in [false, true] {
        let mut fallback = [
            (Key::Atom("retry"), Value::Other),
            (Key::Binary(b"retry"), Value::Map(&old)),
        ];
        if reversed {
            fallback.reverse();
        }
        let rows = [(Key::Binary(b"queued"), Value::Map(&fallback))];
        let policy = [(Key::Atom("states"), Value::Map(&rows))];
        assert!(check(&policy, profile, &mut charge)?);
        let mut preferred = [
            (Key::Atom("retry"), Value::Map(&clean)),
            (Key::Binary(b"retry"), Value::Map(&old)),
        ];
        if reversed {
            preferred.reverse();
        }
        let rows = [(Key::Other, Value::Map(&preferred))];
        assert!(!check(
            &[(Key::Binary(b"states"), Value::Map(&rows))],
            profile,
            &mut charge,
        )?);
        let mut rejected = [
            (Key::Atom("retry"), Value::Map(&old)),
            (Key::Binary(b"retry"), Value::Map(&clean)),
        ];
        if reversed {
            rejected.reverse();
        }
        let rows = [(Key::Other, Value::Map(&rejected))];
        assert!(check(
            &[(Key::Binary(b"states"), Value::Map(&rows))],
            profile,
            &mut charge,
        )?);
        assert_eq!(fallback.len(), 2);
        assert_eq!(preferred.len(), 2);
        assert_eq!(rejected.len(), 2);
    }
    let first = [(Key::Atom("retry"), Value::Map(&old))];
    let later = [(Key::Binary(b"retry"), Value::Map(&clean))];
    let rows = [
        (Key::Other, Value::Other),
        (Key::Binary(b"first"), Value::Map(&first)),
        (Key::Binary(b"later"), Value::Map(&later)),
    ];
    assert!(check(
        &[(Key::Atom("states"), Value::Map(&rows))],
        profile,
        &mut charge,
    )?);
    let exact = limits(7_281)?;
    let mut reached = PolicySchemaCharge::new(exact);
    assert!(check(
        &[(Key::Atom("states"), Value::Map(&rows))],
        exact,
        &mut reached,
    )?);
    // Preflight 4635 + reached semantic 2518; the later clean row is not visited.
    assert_eq!(
        (reached.g(), reached.p(), reached.total()),
        (7_153, 128, 7_281)
    );
    assert!(!check(
        &[(Key::Atom("states"), Value::Other)],
        profile,
        &mut charge,
    )?);
    assert!(matches!(rows[0], (Key::Other, Value::Other)));
    assert_eq!(rows.len(), 3);
    Ok(())
}

#[test]
fn original_retry_keys_detects_both_old_names_without_revalidating_content() -> TestResult {
    let profile = limits(20_000_000)?;
    let mut charge = PolicySchemaCharge::new(profile);
    for key in [Key::Atom("max_attempts"), Key::Binary(b"max_attempts")] {
        for value in [Value::Other, Value::Map(&[])] {
            let retry = [(key, value)];
            let policy = [(Key::Atom("retry"), Value::Map(&retry))];
            assert!(check(&policy, profile, &mut charge)?);
            // Other is a nonmap class, not proof of a particular encoded nil/integer.
            assert_eq!(retry.len(), 1);
            assert_eq!(policy.len(), 1);
        }
    }
    let arbitrary = [
        (Key::Atom("max_retries"), Value::Other),
        (Key::Binary(&[255]), Value::Other),
        (Key::Atom("unknown"), Value::Map(&[])),
    ];
    let policy = [
        (Key::Atom("type"), Value::Other),
        (Key::Atom("max_attempts"), Value::Other),
        (Key::Binary(b"retry"), Value::Map(&arbitrary)),
    ];
    assert!(!check(&policy, profile, &mut charge)?);
    assert!(matches!(arbitrary[1].0, Key::Binary([255])));
    assert!(matches!(policy[0], (Key::Atom("type"), Value::Other)));
    assert_eq!(arbitrary.len(), 3);
    let whitespace = [(Key::Binary(b" max_attempts "), Value::Other)];
    assert!(!check(
        &[(Key::Atom("retry"), Value::Map(&whitespace))],
        profile,
        &mut charge,
    )?);
    let unsupported = [(Key::Other, Value::Other)];
    assert!(!check(
        &[(Key::Atom("states"), Value::Map(&unsupported))],
        profile,
        &mut charge,
    )?);
    Ok(())
}

#[test]
fn original_retry_keys_keeps_map_order_and_descriptor_scope_explicit() -> TestResult {
    let profile = limits(20_000_000)?;
    let mut charge = PolicySchemaCharge::new(profile);
    let old = old_retry();
    for reversed in [false, true] {
        let mut policy = [
            (Key::Binary(b"retry"), Value::Other),
            (Key::Atom("retry"), Value::Map(&old)),
        ];
        if reversed {
            policy.reverse();
        }
        assert!(check(&policy, profile, &mut charge)?);
        assert_eq!(policy.len(), 2);
    }
    for name in ["retry", "states", "max_attempts"] {
        for key in [Key::Atom(name), Key::Binary(name.as_bytes())] {
            let policy = [(key, Value::Other), (key, Value::Map(&[]))];
            assert_eq!(
                original_complete_policy_uses_max_attempts(&policy, profile, &mut charge),
                Err(Error::Malformed)
            );
            assert_eq!(policy.len(), 2);
        }
    }
    let opaque = [
        (Key::Other, Value::Other),
        (Key::Other, Value::Map(&[])),
        (Key::Atom("unqueried"), Value::Other),
        (Key::Atom("unqueried"), Value::Other),
    ];
    assert!(!check(&opaque, profile, &mut charge)?);
    // Accepted bool is deliberately no proof of arbitrary source map uniqueness.
    assert_eq!(opaque.len(), 4);
    assert!(matches!(opaque[0].0, Key::Other));
    assert!(matches!(opaque[1].0, Key::Other));
    let empty = [];
    assert!(!check(&empty, profile, &mut charge)?);
    assert_eq!(empty.len(), 0);
    Ok(())
}
