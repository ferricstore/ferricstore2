//! Start-specific validation order is independent of create's option defaults.
use super::*;
use wf_ports::projection::query_row::prepare_original_start_and_claim as prepare;

#[test]
fn original_start_refuses_create_only_options_before_identity_validation() -> TestResult {
    for (key, expected) in [
        (
            "idempotent",
            "ERR flow idempotent start_and_claim is not supported",
        ),
        (
            "run_at_ms",
            "ERR flow run_at_ms is not supported by start_and_claim",
        ),
    ] {
        let pair = [I::Atom(key), I::Nil];
        let opts = [I::Tuple(&pair)];
        let outcome = prepare(
            I::Binary(b""),
            I::Binary(b""),
            I::Binary(b""),
            I::List(&opts),
            OriginalCreateContext::default(),
            limits(),
        )?;
        assert!(matches!(outcome, Outcome::Rejected {message, ..} if message == expected));
    }
    Ok(())
}

fn message(
    id: I<'_, '_>,
    kind: I<'_, '_>,
    state: I<'_, '_>,
    options: I<'_, '_>,
) -> TestResult<String> {
    match prepare(
        id,
        kind,
        state,
        options,
        OriginalCreateContext::default(),
        limits(),
    )? {
        Outcome::Rejected {
            message,
            work_units,
            temporary_bytes,
        } => {
            assert!(work_units > 16_384);
            assert!(temporary_bytes >= 16_384);
            Ok(message.into_owned())
        }
        other => Err(format!("expected source refusal: {other:?}").into()),
    }
}

#[test]
fn start_keeps_keyword_identity_type_state_worker_and_lease_error_order() -> TestResult {
    let bad_keyword = [I::Nil];
    assert_eq!(
        message(I::Nil, I::Nil, I::Nil, I::List(&bad_keyword))?,
        "ERR flow opts must be a keyword list"
    );
    assert_eq!(
        message(I::Nil, I::Nil, I::Nil, I::List(&[]))?,
        "ERR flow id must be a non-empty string"
    );
    assert_eq!(
        message(I::Binary(b"i"), I::Nil, I::Nil, I::List(&[]))?,
        "ERR flow type must be a non-empty string"
    );
    assert_eq!(
        message(I::Binary(b"i"), I::Binary(b"t"), I::Nil, I::List(&[]))?,
        "ERR flow state must be a non-empty string"
    );
    assert_eq!(
        message(
            I::Binary(b"i"),
            I::Binary(b"t"),
            I::Binary(b"running"),
            I::List(&[])
        )?,
        "ERR flow running state is only entered by FLOW.CLAIM_DUE"
    );
    assert_eq!(
        message(
            I::Binary(b"i"),
            I::Binary(b"t"),
            I::Binary(b"s"),
            I::List(&[])
        )?,
        "ERR flow worker is required"
    );
    let worker = [I::Atom("worker"), I::Nil];
    let opts = [I::Tuple(&worker)];
    assert_eq!(
        message(
            I::Binary(b"i"),
            I::Binary(b"t"),
            I::Binary(b"s"),
            I::List(&opts)
        )?,
        "ERR flow worker must be a non-empty string"
    );
    Ok(())
}

#[test]
fn lease_and_now_checks_preserve_mathematical_aliases_and_deadline_endpoints() -> TestResult {
    for lease in [
        I::Nil,
        I::Integer(0),
        I::Integer(-1),
        I::Float(1.0),
        I::BigInteger {
            negative: true,
            little_endian: &[1],
        },
    ] {
        let pairs = [
            [I::Atom("worker"), I::Binary(b"w")],
            [I::Atom("lease_ms"), lease],
        ];
        let opts = pairs.iter().map(|p| I::Tuple(p)).collect::<Vec<_>>();
        assert_eq!(
            message(
                I::Binary(b"i"),
                I::Binary(b"t"),
                I::Binary(b"s"),
                I::List(&opts)
            )?,
            "ERR flow lease_ms must be a positive integer"
        );
    }
    for (now, lease) in [
        (I::Integer(0), I::Integer(9_007_199_254_740_991)),
        (
            I::Integer(9_007_199_254_740_990),
            I::BigInteger {
                negative: false,
                little_endian: &[1, 0],
            },
        ),
    ] {
        let pairs = [
            [I::Atom("worker"), I::Binary(b"w")],
            [I::Atom("lease_ms"), lease],
            [I::Atom("now_ms"), now],
        ];
        let opts = pairs.iter().map(|p| I::Tuple(p)).collect::<Vec<_>>();
        assert!(matches!(
            prepare(
                I::Binary(b"i"),
                I::Binary(b"t"),
                I::Binary(b"s"),
                I::List(&opts),
                OriginalCreateContext::default(),
                limits()
            )?,
            Outcome::Accepted { .. }
        ));
    }
    let pairs = [
        [I::Atom("worker"), I::Binary(b"w")],
        [I::Atom("lease_ms"), I::Integer(9_007_199_254_740_991)],
    ];
    let opts = pairs.iter().map(|p| I::Tuple(p)).collect::<Vec<_>>();
    assert!(matches!(
        prepare(
            I::Binary(b"i"),
            I::Binary(b"t"),
            I::Binary(b"s"),
            I::List(&opts),
            OriginalCreateContext::default(),
            limits()
        )?,
        Outcome::Accepted { .. }
    ));
    Ok(())
}

#[test]
fn start_preserves_parent_now_deadline_and_retention_precedence() -> TestResult {
    let too_long = vec![0; 4097];
    let pairs = [
        [I::Atom("worker"), I::Binary(b"w")],
        [I::Atom("parent_flow_id"), I::Binary(&too_long)],
        [I::Atom("now_ms"), I::Nil],
        [I::Atom("ttl_ms"), I::Nil],
    ];
    let mut opts = pairs.iter().map(|p| I::Tuple(p)).collect::<Vec<_>>();
    assert_eq!(
        message(
            I::Binary(b"i"),
            I::Binary(b"t"),
            I::Binary(b"s"),
            I::List(&opts)
        )?,
        "ERR flow parent_flow_id too large (max 4096 bytes)"
    );
    opts.remove(1);
    assert_eq!(
        message(
            I::Binary(b"i"),
            I::Binary(b"t"),
            I::Binary(b"s"),
            I::List(&opts)
        )?,
        "ERR flow now_ms must be a non-negative integer"
    );
    let pairs = [
        [I::Atom("worker"), I::Binary(b"w")],
        [I::Atom("now_ms"), I::Integer(9_007_199_254_740_991)],
        [I::Atom("ttl_ms"), I::Nil],
    ];
    let opts = pairs.iter().map(|p| I::Tuple(p)).collect::<Vec<_>>();
    assert_eq!(
        message(
            I::Binary(b"i"),
            I::Binary(b"t"),
            I::Binary(b"s"),
            I::List(&opts)
        )?,
        "ERR flow lease_ms deadline exceeds maximum 9007199254740991"
    );
    Ok(())
}

#[test]
fn start_requires_the_captured_internal_observation_for_reserved_identity() -> TestResult {
    let pairs = [
        [I::Atom("worker"), I::Binary(b"w")],
        [I::Atom("internal"), I::Boolean(true)],
    ];
    let opts = pairs.iter().map(|p| I::Tuple(p)).collect::<Vec<_>>();
    assert_eq!(
        message(
            I::Binary(b"__ferricstore_schedule__:s"),
            I::Binary(b"t"),
            I::Binary(b"s"),
            I::List(&opts)
        )?,
        "ERR flow id is reserved for internal use"
    );
    assert_eq!(
        message(
            I::Binary(b"i"),
            I::Binary(b"__ferricstore_schedule"),
            I::Binary(b"s"),
            I::List(&opts)
        )?,
        "ERR flow type is reserved for internal use"
    );
    let context = OriginalCreateContext {
        internal_allowed: true,
        ..OriginalCreateContext::default()
    };
    assert!(matches!(
        prepare(
            I::Binary(b"__ferricstore_schedule__:s"),
            I::Binary(b"__ferricstore_schedule"),
            I::Binary(b"s"),
            I::List(&opts),
            context,
            limits()
        )?,
        Outcome::Accepted { .. }
    ));
    Ok(())
}
