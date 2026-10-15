//! Public GET ingress preserves source ordering without SET's patch traversal.

use super::{TestResult, limits};
use wf_domain::policy::complete::{
    CompletePolicyEncodingError as EncodingError, OriginalPolicyGetRejection as Rejection,
    OriginalPolicyGetRequest as Request, PolicySchemaCharge, PolicySchemaError, PolicySchemaLimits,
    RawFlowPolicyValue as Raw, prepare_original_policy_get_request as prepare,
};

mod refusals;

#[test]
fn original_public_get_preserves_complete_shape_and_caller_priority() -> TestResult {
    assert_heads()?;
    assert_complete_keyword_shape()?;
    assert_type_state_key_priority()?;
    assert_bodies();
    Ok(())
}

#[test]
fn original_public_get_keeps_first_state_and_ignores_other_values() -> TestResult {
    assert_first_state()?;
    assert_ignored_values()?;
    assert_key_and_state_bytes()?;
    assert_atom_spellings()?;
    assert_long_keyword_spine()?;
    Ok(())
}

fn assert_heads() -> TestResult {
    let empty = Raw::List(&[]);
    for ty in [
        Raw::Nil,
        Raw::Atom("r"),
        Raw::Integer(1),
        Raw::List(&[]),
        Raw::Map(&[]),
        Raw::Other,
    ] {
        rejected(&ty, &empty, Rejection::Options)?;
    }
    for opts in [
        Raw::Nil,
        Raw::Atom("nil"),
        Raw::Integer(0),
        Raw::Map(&[]),
        Raw::Tuple(&[]),
        Raw::Other,
    ] {
        rejected(&Raw::Binary(b"r"), &opts, Rejection::Options)?;
    }
    rejected(&Raw::Binary(b""), &empty, Rejection::Type)?;
    Ok(())
}

fn assert_complete_keyword_shape() -> TestResult {
    let ty = Raw::Binary(b"");
    for key in [
        Raw::Binary(b"state"),
        Raw::Integer(1),
        Raw::List(&[]),
        Raw::Other,
    ] {
        let pair = [key, Raw::Other];
        let rows = [Raw::Tuple(&pair)];
        rejected(&ty, &Raw::List(&rows), Rejection::Options)?;
    }
    for pair in [&[][..], &[Raw::Atom("state")][..], &[Raw::Nil; 3][..]] {
        let rows = [Raw::Tuple(pair)];
        rejected(&ty, &Raw::List(&rows), Rejection::Options)?;
    }
    let first = [Raw::Atom("state"), Raw::Integer(1)];
    let rows = [Raw::Tuple(&first), Raw::Other];
    rejected(&ty, &Raw::List(&rows), Rejection::Options)?;
    rejected(&Raw::Binary(b"r"), &Raw::List(&rows), Rejection::Options)?;
    let valid = [Raw::Atom("state"), Raw::Nil];
    let prefix = [Raw::Tuple(&valid)];
    for tail in [Raw::Nil, Raw::Integer(7), Raw::Other] {
        let opts = Raw::ImproperList {
            prefix: &prefix,
            tail: &tail,
        };
        rejected(&ty, &opts, Rejection::Options)?;
        rejected(&Raw::Binary(b"r"), &opts, Rejection::Options)?;
    }
    let tail = Raw::List(&[]);
    let opts = Raw::ImproperList {
        prefix: &prefix,
        tail: &tail,
    };
    assert_eq!(request(&Raw::Binary(b"r"), &opts)?.response_state(), None);
    let profile = PolicySchemaLimits::default();
    let mut charge = PolicySchemaCharge::new(profile);
    let invalid_descriptor = Raw::ImproperList {
        prefix: &[],
        tail: &tail,
    };
    assert!(matches!(
        prepare(
            &Raw::Binary(b"r"),
            &invalid_descriptor,
            profile,
            &mut charge
        ),
        Err(EncodingError::Malformed)
    ));
    Ok(())
}

fn assert_type_state_key_priority() -> TestResult {
    let bad_state = [Raw::Atom("state"), Raw::Other];
    let rows = [Raw::Tuple(&bad_state)];
    rejected(&Raw::Binary(b""), &Raw::List(&rows), Rejection::Type)?;
    let overlong = vec![b'r'; 65_523];
    rejected(&Raw::Binary(&overlong), &Raw::List(&rows), Rejection::State)?;
    rejected(
        &Raw::Binary(&overlong),
        &Raw::List(&[]),
        Rejection::KeyBytes,
    )?;
    for state in [
        Raw::Integer(1),
        Raw::Atom("false"),
        Raw::Atom("fifo"),
        Raw::List(&[]),
        Raw::Map(&[]),
        Raw::Other,
    ] {
        let pair = [Raw::Atom("state"), state];
        let rows = [Raw::Tuple(&pair)];
        rejected(&Raw::Binary(b"r"), &Raw::List(&rows), Rejection::State)?;
    }
    Ok(())
}

fn assert_first_state() -> TestResult {
    for (first, later, expected) in [
        (Raw::Nil, Raw::Binary(b"q"), None),
        (Raw::Atom("nil"), Raw::Other, None),
        (Raw::Binary(b""), Raw::Nil, Some(&b""[..])),
        (Raw::Binary(b"q"), Raw::Other, Some(&b"q"[..])),
    ] {
        let a = [Raw::Atom("state"), first];
        let b = [Raw::Atom("state"), later];
        let prefix = [Raw::Tuple(&a)];
        let suffix = [Raw::Tuple(&b)];
        let tail = Raw::List(&suffix);
        let opts = Raw::ImproperList {
            prefix: &prefix,
            tail: &tail,
        };
        assert_eq!(
            request(&Raw::Binary(b"r"), &opts)?.response_state(),
            expected
        );
    }
    let a = [Raw::Atom("state"), Raw::Integer(1)];
    let b = [Raw::Atom("state"), Raw::Binary(b"q")];
    let rows = [Raw::Tuple(&a), Raw::Tuple(&b)];
    rejected(&Raw::Binary(b"r"), &Raw::List(&rows), Rejection::State)?;
    Ok(())
}

fn assert_ignored_values() -> TestResult {
    let impossible_atom = "x".repeat(1_024);
    let malformed = Raw::ImproperList {
        prefix: &[],
        tail: &Raw::Other,
    };
    let structure = [(Raw::Atom("__struct__"), Raw::Atom("Elixir.Dummy"))];
    let pairs = [
        [Raw::Nil, Raw::Other],
        [Raw::Atom("nil"), Raw::Atom(&impossible_atom)],
        [Raw::Atom("true"), malformed],
        [Raw::Atom("false"), Raw::Map(&structure)],
        [Raw::Atom("expected_generation"), Raw::Integer(-1)],
        [Raw::Atom("replace"), Raw::Other],
        [Raw::Atom("unknown"), Raw::Other],
    ];
    let rows = pairs
        .iter()
        .map(|pair| Raw::Tuple(pair))
        .collect::<Vec<_>>();
    let result = request(&Raw::Binary(b"r"), &Raw::List(&rows))?;
    assert_eq!(result.response_state(), None);
    assert_eq!(result.policy_key(), b"f:{f}:policy:r");
    // Ignored values confer no source-origin or hidden-payload validity proof.
    Ok(())
}

fn assert_key_and_state_bytes() -> TestResult {
    let ty = vec![0xff; 65_522];
    let state = [0xff, 0];
    let pair = [Raw::Atom("state"), Raw::Binary(&state)];
    let rows = [Raw::Tuple(&pair)];
    let result = request(&Raw::Binary(&ty), &Raw::List(&rows))?;
    assert_eq!(result.policy_key().len(), 65_535);
    assert_eq!(&result.policy_key()[..13], b"f:{f}:policy:");
    assert_eq!(&result.policy_key()[13..], ty);
    assert_eq!(result.workflow_type(), ty);
    assert_eq!(result.response_state(), Some(&state[..]));
    assert!(std::ptr::eq(result.workflow_type().as_ptr(), ty.as_ptr()));
    assert!(std::ptr::eq(
        result.response_state().ok_or("state")?.as_ptr(),
        state.as_ptr()
    ));
    drop(result);
    let large_query = vec![0; 67_108_865];
    let pair = [Raw::Atom("state"), Raw::Binary(&large_query)];
    let rows = [Raw::Tuple(&pair)];
    assert_eq!(
        request(&Raw::Binary(b"r"), &Raw::List(&rows))?
            .response_state()
            .ok_or("large state")?
            .len(),
        67_108_865
    );
    Ok(())
}

fn assert_atom_spellings() -> TestResult {
    let maximum = "🦀".repeat(255);
    let pair = [Raw::Atom(&maximum), Raw::Other];
    let rows = [Raw::Tuple(&pair)];
    assert_eq!(
        request(&Raw::Binary(b"r"), &Raw::List(&rows))?.response_state(),
        None
    );
    let overlong = "🦀".repeat(256);
    let pair = [Raw::Atom(&overlong), Raw::Other];
    let rows = [Raw::Tuple(&pair)];
    let profile = PolicySchemaLimits::default();
    let mut charge = PolicySchemaCharge::new(profile);
    assert!(matches!(
        prepare(&Raw::Binary(b"r"), &Raw::List(&rows), profile, &mut charge),
        Err(EncodingError::ByteLimit)
    ));
    Ok(())
}

fn assert_long_keyword_spine() -> TestResult {
    let pair = [Raw::Atom("x"), Raw::Other];
    let rows = vec![Raw::Tuple(&pair); 70_000];
    let result = request(&Raw::Binary(b"r"), &Raw::List(&rows))?;
    assert_eq!(result.response_state(), None);
    assert_eq!(result.policy_key(), b"f:{f}:policy:r");
    Ok(())
}

fn assert_bodies() {
    for (reason, bytes) in [
        (
            Rejection::Options,
            &b"ERR flow opts must be a keyword list"[..],
        ),
        (
            Rejection::Type,
            &b"ERR flow type must be a non-empty string"[..],
        ),
        (Rejection::State, &b"ERR flow state must be a string"[..]),
        (
            Rejection::KeyBytes,
            &b"ERR key too large (max 65535 bytes)"[..],
        ),
        (Rejection::Corrupt, &b"ERR flow policy is corrupt"[..]),
        (
            Rejection::ShardUnavailable,
            &b"ERR flow policy shard not available"[..],
        ),
        (Rejection::ReadFailed, &b"ERR flow policy read failed"[..]),
    ] {
        assert_eq!(reason.as_bytes(), bytes);
        assert_eq!(reason.to_string().as_bytes(), bytes);
    }
}

fn request<'input>(ty: &Raw<'input>, opts: &Raw<'input>) -> TestResult<Request<'input>> {
    let profile = PolicySchemaLimits::default();
    let mut charge = PolicySchemaCharge::new(profile);
    accepted(ty, opts, profile, &mut charge)
}

pub(super) fn accepted<'input>(
    ty: &Raw<'input>,
    opts: &Raw<'input>,
    profile: PolicySchemaLimits,
    charge: &mut PolicySchemaCharge,
) -> TestResult<Request<'input>> {
    Ok(prepare(ty, opts, profile, charge)?
        .map_err(|reason| format!("unexpected GET input rejection: {reason}"))?)
}

fn rejected(ty: &Raw<'_>, opts: &Raw<'_>, expected: Rejection) -> TestResult {
    let profile = PolicySchemaLimits::default();
    let mut charge = PolicySchemaCharge::new(profile);
    let Err(reason) = prepare(ty, opts, profile, &mut charge)? else {
        return Err("expected GET input rejection".into());
    };
    assert_eq!(reason, expected);
    Ok(())
}
