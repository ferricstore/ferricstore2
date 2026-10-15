//! Independent prospective primitives, raw metric boundaries and held owners.

use super::{
    CompletePolicyEncodingError, OriginalPolicySetInputRejection, PolicySchemaCharge,
    PolicySchemaError, RawFlowPolicyValue, TestResult,
    encode_original_policy_set_input_rejection_body, limits, prepare_original_policy_set_request,
};
use wf_domain::policy::complete::normalize_original_complete_policy_input;

// Zero marks the sole SUM-scratch admission; all other entries are atomic G.
const MIN: &[u64] = &[
    128, 128, 1, 128, 128, 128, 128, 128, 128, 128, 128, 0, 256, 144, 192, 128, 128, 128, 1, 128,
    5, 142, 128,
];
const CONTROL: &[u64] = &[
    128, 128, 1, 128, 128, 128, 7, 128, 4, 128, 128, 128, 140, 154, 142, 128, 128, 128, 128, 128,
    0, 448, 168, 288, 128, 128, 140, 154, 142, 128, 128, 1, 128, 5, 142, 128,
];
const DATA: &[u64] = &[
    128, 128, 1, 128, 128, 128, 1, 128, 1, 128, 128, 128, 134, 148, 136, 128, 128, 128, 128, 128,
    0, 448, 168, 288, 128, 128, 134, 148, 136, 129, 128, 128, 128, 139, 128, 135, 128, 128, 128,
    128, 128, 128, 128, 128, 1, 128, 5, 128, 3, 128, 6, 142, 128,
];
const TWO: &[u64] = &[
    128, 128, 1, 128, 128, 128, 1, 128, 128, 128, 1, 128, 128, 128, 128, 134, 148, 136, 128, 134,
    148, 136, 128, 128, 128, 128, 128, 0, 640, 192, 384, 128, 128, 134, 148, 136, 129, 128, 128,
    134, 148, 136, 129, 128, 128, 128, 130, 256, 256, 16, 128, 128, 139, 128, 139, 128, 135, 128,
    128, 128, 128, 135, 128, 128, 128, 128, 128, 128, 128, 128, 128, 128, 128, 128, 1, 128, 5, 128,
    3, 128, 5, 128, 3, 128, 5, 142, 128,
];
const DUPLICATE: &[u64] = &[
    128, 128, 1, 128, 128, 128, 1, 128, 1, 128, 128, 1, 128, 1, 128, 128, 128, 134, 148, 136, 128,
    134, 148, 136, 128, 128, 128, 128, 128, 0, 640, 192, 384, 128, 128, 134, 148, 136, 129, 128,
    128, 134, 148, 136, 129, 128, 128, 128, 130, 16, 256, 256, 16, 128, 128, 130, 128, 128, 128,
    139, 128, 135, 128, 128, 128, 128, 128, 128, 128, 128, 1, 128, 5, 128, 3, 128, 6, 142, 128,
];
const LIST: &[u64] = &[
    128, 128, 1, 128, 128, 128, 1, 128, 128, 128, 128, 128, 128, 134, 148, 136, 128, 128, 128, 128,
    128, 0, 576, 184, 352, 128, 128, 134, 148, 136, 129, 128, 128, 128, 139, 128, 135, 128, 128,
    128, 128, 128, 128, 128, 128, 128, 128, 128, 128, 128, 128, 128, 128, 128, 128, 128, 1, 128, 5,
    128, 3, 128, 128, 128, 5, 142, 128,
];

fn fees(opts: &RawFlowPolicyValue<'_>, events: &[u64], peak: u64, total: u64) -> TestResult {
    let mut paid = 0;
    let mut old_peak = 0;
    for &event in events {
        let next_peak = if event == 0 { peak } else { old_peak };
        let target = paid + event + next_peak;
        let profile = limits(target - 1)?;
        let mut charge = PolicySchemaCharge::new(profile);
        let result = prepare_original_policy_set_request(
            &RawFlowPolicyValue::Binary(b"r"),
            opts,
            profile,
            &mut charge,
        );
        assert!(
            matches!(result, Err(CompletePolicyEncodingError::Schema(
            PolicySchemaError::WorkBytes { bytes, maximum }
        )) if bytes == target && maximum == target - 1),
            "target {target}"
        );
        assert_eq!((charge.g(), charge.p()), (paid, old_peak));
        paid += event;
        old_peak = next_peak;
    }
    assert_eq!((paid, old_peak), (total, peak));
    let profile = limits(total + peak)?;
    let mut charge = PolicySchemaCharge::new(profile);
    assert!(
        prepare_original_policy_set_request(
            &RawFlowPolicyValue::Binary(b"r"),
            opts,
            profile,
            &mut charge
        )?
        .is_ok()
    );
    assert_eq!((charge.g(), charge.p()), (total, peak));
    Ok(())
}

#[test]
fn original_public_set_prepays_each_reached_operation() -> TestResult {
    fees(&RawFlowPolicyValue::List(&[]), MIN, 464, 2661)?;
    let control = [
        RawFlowPolicyValue::Atom("replace"),
        RawFlowPolicyValue::Atom("true"),
    ];
    fees(
        &RawFlowPolicyValue::List(&[RawFlowPolicyValue::Tuple(&control)]),
        CONTROL,
        1032,
        4496,
    )?;
    let data = [
        RawFlowPolicyValue::Atom("x"),
        RawFlowPolicyValue::Binary(b"v"),
    ];
    fees(
        &RawFlowPolicyValue::List(&[RawFlowPolicyValue::Tuple(&data)]),
        DATA,
        1032,
        6399,
    )?;
    let a = [RawFlowPolicyValue::Atom("a"), RawFlowPolicyValue::Nil];
    let b = [RawFlowPolicyValue::Atom("b"), RawFlowPolicyValue::Nil];
    let rows = [RawFlowPolicyValue::Tuple(&a), RawFlowPolicyValue::Tuple(&b)];
    fees(&RawFlowPolicyValue::List(&rows), TWO, 1344, 11047)?;
    let shared = [RawFlowPolicyValue::Tuple(&data); 2];
    fees(&RawFlowPolicyValue::List(&shared), DUPLICATE, 1344, 9890)?;
    let values = [
        RawFlowPolicyValue::Integer(1),
        RawFlowPolicyValue::Integer(2),
    ];
    let list = [
        RawFlowPolicyValue::Atom("x"),
        RawFlowPolicyValue::List(&values),
    ];
    fees(
        &RawFlowPolicyValue::List(&[RawFlowPolicyValue::Tuple(&list)]),
        LIST,
        1368,
        8525,
    )?;
    assert_failed_probe()?;
    assert_improper_state_prefix()?;
    assert_body_literals()?;
    assert_visible_bounds()?;
    super::cases::assert_callback_order()?;
    super::metric::assert_default_work()?;
    Ok(())
}

fn assert_failed_probe() -> TestResult {
    let values = [
        RawFlowPolicyValue::Integer(1),
        RawFlowPolicyValue::Integer(256),
    ];
    let pair = [
        RawFlowPolicyValue::Atom("x"),
        RawFlowPolicyValue::List(&values),
    ];
    let mut events = LIST[..LIST.len() - 3].to_vec();
    // Replace STRING5 with LIST5, fresh head terms and implicit proper NIL1.
    events.extend_from_slice(&[5, 128, 2, 128, 5, 1, 142, 128]);
    fees(
        &RawFlowPolicyValue::List(&[RawFlowPolicyValue::Tuple(&pair)]),
        &events,
        1368,
        8789,
    )
}

fn assert_improper_state_prefix() -> TestResult {
    let config = [
        RawFlowPolicyValue::Binary(b"queued"),
        RawFlowPolicyValue::List(&[]),
    ];
    let prefix = [RawFlowPolicyValue::Tuple(&config)];
    let tail = RawFlowPolicyValue::Nil;
    let pair = [
        RawFlowPolicyValue::Atom("states"),
        RawFlowPolicyValue::ImproperList {
            prefix: &prefix,
            tail: &tail,
        },
    ];
    let rows = [RawFlowPolicyValue::Tuple(&pair)];
    for maximum in [7776, 7777] {
        let profile = limits(maximum)?;
        let mut charge = PolicySchemaCharge::new(profile);
        let result = prepare_original_policy_set_request(
            &RawFlowPolicyValue::Binary(b"r"),
            &RawFlowPolicyValue::List(&rows),
            profile,
            &mut charge,
        );
        if maximum == 7777 {
            assert!(matches!(
                result,
                Err(CompletePolicyEncodingError::Schema(
                    PolicySchemaError::InvalidAuthority
                ))
            ));
            assert_eq!((charge.g(), charge.p()), (6073, 1704));
        } else {
            assert!(matches!(
                result,
                Err(CompletePolicyEncodingError::Schema(
                    PolicySchemaError::WorkBytes {
                        bytes: 7777,
                        maximum: 7776
                    }
                ))
            ));
            assert_eq!((charge.g(), charge.p()), (5945, 1704));
        }
    }
    Ok(())
}

fn assert_body_literals() -> TestResult {
    for (reason, body, expected) in [
        (
            OriginalPolicySetInputRejection::Options,
            b"ERR flow opts must be a keyword list".as_slice(),
            292,
        ),
        (
            OriginalPolicySetInputRejection::Type,
            b"ERR flow type must be a non-empty string".as_slice(),
            296,
        ),
        (
            OriginalPolicySetInputRejection::KeyBytes,
            b"ERR key too large (max 65535 bytes)".as_slice(),
            291,
        ),
        (
            OriginalPolicySetInputRejection::State,
            b"ERR flow state must be a string".as_slice(),
            287,
        ),
        (
            OriginalPolicySetInputRejection::ExpectedGeneration,
            b"ERR flow expected_generation must be a non-negative integer".as_slice(),
            315,
        ),
        (
            OriginalPolicySetInputRejection::Replace,
            b"ERR flow replace must be boolean".as_slice(),
            288,
        ),
        (
            OriginalPolicySetInputRejection::SnapshotBytes,
            b"ERR flow policy snapshot exceeds 262144 bytes".as_slice(),
            301,
        ),
    ] {
        assert_eq!(reason.as_bytes(), body);
        for (maximum, retained, target) in [(127, 0, 128), (expected - 1, 128, expected)] {
            let profile = limits(maximum)?;
            let mut charge = PolicySchemaCharge::new(profile);
            assert!(
                matches!(encode_original_policy_set_input_rejection_body(reason,profile,&mut charge),
                Err(CompletePolicyEncodingError::Schema(PolicySchemaError::WorkBytes { bytes, .. })) if bytes == target)
            );
            assert_eq!((charge.g(), charge.p()), (retained, 0));
        }
        let profile = limits(expected)?;
        let mut charge = PolicySchemaCharge::new(profile);
        assert_eq!(
            encode_original_policy_set_input_rejection_body(reason, profile, &mut charge)?,
            body
        );
        assert_eq!(charge.g(), expected);
    }
    Ok(())
}

fn assert_visible_bounds() -> TestResult {
    let profile = limits(1_073_741_824)?;
    let mismatch = limits(1_073_741_823)?;
    let mut charge = PolicySchemaCharge::new(profile);
    assert!(matches!(
        prepare_original_policy_set_request(
            &RawFlowPolicyValue::Binary(b"r"),
            &RawFlowPolicyValue::List(&[]),
            mismatch,
            &mut charge
        ),
        Err(CompletePolicyEncodingError::Schema(
            PolicySchemaError::ProfileMismatch
        ))
    ));
    assert_eq!((charge.g(), charge.p()), (0, 0));
    assert!(matches!(
        encode_original_policy_set_input_rejection_body(
            OriginalPolicySetInputRejection::State,
            mismatch,
            &mut charge
        ),
        Err(CompletePolicyEncodingError::Schema(
            PolicySchemaError::ProfileMismatch
        ))
    ));
    assert_eq!((charge.g(), charge.p()), (0, 0));
    for (count, expected) in [
        (131_070, None),
        (131_071, Some(CompletePolicyEncodingError::ItemLimit)),
    ] {
        let values = vec![RawFlowPolicyValue::Other; count];
        let mut charge = PolicySchemaCharge::new(profile);
        let result = prepare_original_policy_set_request(
            &RawFlowPolicyValue::Binary(b"r"),
            &RawFlowPolicyValue::List(&values),
            profile,
            &mut charge,
        );
        if let Some(error) = expected {
            assert!(matches!(result,Err(actual) if actual == error));
            assert_eq!(charge.g(), 16_777_473);
        } else {
            assert!(matches!(
                result?,
                Err(OriginalPolicySetInputRejection::Options)
            ));
            assert_eq!(charge.g(), 16_777_729);
        }
        assert_eq!(charge.p(), 0);
    }
    for (depth, expected, paid) in [
        (64, None, 8705),
        (65, Some(CompletePolicyEncodingError::ItemLimit), 8705),
    ] {
        depth_fixture(depth, RawFlowPolicyValue::Other, &mut |value| {
            let mut charge = PolicySchemaCharge::new(profile);
            let result = prepare_original_policy_set_request(
                &RawFlowPolicyValue::Binary(b"r"),
                &value,
                profile,
                &mut charge,
            );
            match expected {
                Some(error) => assert!(matches!(result,Err(actual) if actual == error)),
                None => assert!(matches!(
                    result?,
                    Err(OriginalPolicySetInputRejection::Options)
                )),
            }
            assert_eq!((charge.g(), charge.p()), (paid, 0));
            Ok(())
        })?;
    }
    assert_byte_bound(profile)?;
    assert_profile_exclusions(profile);
    Ok(())
}

fn depth_fixture(
    depth: usize,
    value: RawFlowPolicyValue<'_>,
    check: &mut dyn for<'view> FnMut(RawFlowPolicyValue<'view>) -> TestResult,
) -> TestResult {
    if depth == 0 {
        return check(value);
    }
    let items = [value];
    depth_fixture(depth - 1, RawFlowPolicyValue::Tuple(&items), check)
}

fn assert_byte_bound(profile: wf_domain::policy::complete::PolicySchemaLimits) -> TestResult {
    for (length, paid, overflow) in [(67_108_863, 67_109_504, false), (67_108_864, 513, true)] {
        let bytes = vec![0_u8; length];
        let fields = [RawFlowPolicyValue::Binary(&bytes)];
        let mut charge = PolicySchemaCharge::new(profile);
        let result = prepare_original_policy_set_request(
            &RawFlowPolicyValue::Binary(b"r"),
            &RawFlowPolicyValue::Tuple(&fields),
            profile,
            &mut charge,
        );
        if overflow {
            assert!(matches!(
                result,
                Err(CompletePolicyEncodingError::ByteLimit)
            ));
        } else {
            assert!(matches!(
                result?,
                Err(OriginalPolicySetInputRejection::Options)
            ));
        }
        assert_eq!((charge.g(), charge.p()), (paid, 0));
    }
    Ok(())
}

fn assert_profile_exclusions(profile: wf_domain::policy::complete::PolicySchemaLimits) {
    let tail = RawFlowPolicyValue::Binary(b"unreached");
    let empty = RawFlowPolicyValue::ImproperList {
        prefix: &[],
        tail: &tail,
    };
    let mut charge = PolicySchemaCharge::new(profile);
    assert!(matches!(
        prepare_original_policy_set_request(
            &RawFlowPolicyValue::Binary(b"r"),
            &empty,
            profile,
            &mut charge
        ),
        Err(CompletePolicyEncodingError::Malformed)
    ));
    assert_eq!((charge.g(), charge.p()), (385, 0));
    let atom = "a".repeat(256);
    for value in [RawFlowPolicyValue::Atom(&atom), RawFlowPolicyValue::Other] {
        let field = [RawFlowPolicyValue::Atom("x"), value];
        let rows = [RawFlowPolicyValue::Tuple(&field)];
        let mut charge = PolicySchemaCharge::new(profile);
        let result = prepare_original_policy_set_request(
            &RawFlowPolicyValue::Binary(b"r"),
            &RawFlowPolicyValue::List(&rows),
            profile,
            &mut charge,
        );
        let expected = if matches!(value, RawFlowPolicyValue::Other) {
            CompletePolicyEncodingError::UnsupportedTag
        } else {
            CompletePolicyEncodingError::Schema(PolicySchemaError::InvalidAuthority)
        };
        assert!(matches!(result,Err(error) if error == expected));
    }
    let tuple = [RawFlowPolicyValue::Other, RawFlowPolicyValue::Integer(-7)];
    let field = [
        RawFlowPolicyValue::Atom("x"),
        RawFlowPolicyValue::Tuple(&tuple),
    ];
    let mut charge = PolicySchemaCharge::new(profile);
    assert!(matches!(
        prepare_original_policy_set_request(
            &RawFlowPolicyValue::Binary(b"r"),
            &RawFlowPolicyValue::List(&[RawFlowPolicyValue::Tuple(&field)]),
            profile,
            &mut charge
        ),
        Err(CompletePolicyEncodingError::UnsupportedTag)
    ));
}

#[test]
fn original_public_set_keeps_one_account_and_borrowed_owners() -> TestResult {
    let profile = limits(8446)?;
    let mut charge = PolicySchemaCharge::new(profile);
    let first = prepare_original_policy_set_request(
        &RawFlowPolicyValue::Binary(b"r"),
        &RawFlowPolicyValue::List(&[]),
        profile,
        &mut charge,
    )?
    .map_err(|reason| format!("{reason:?}"))?;
    let pointer = first.policy_key().as_ptr();
    assert_eq!(charge.g(), 2661);
    let second = prepare_original_policy_set_request(
        &RawFlowPolicyValue::Binary(b"r"),
        &RawFlowPolicyValue::List(&[]),
        profile,
        &mut charge,
    )?
    .map_err(|reason| format!("{reason:?}"))?;
    assert_eq!(charge.g(), 5322);
    assert_eq!(first.policy_key().as_ptr(), pointer);
    assert!(matches!(
        prepare_original_policy_set_request(
            &RawFlowPolicyValue::Binary(b"r"),
            &RawFlowPolicyValue::List(&[]),
            profile,
            &mut charge
        ),
        Err(CompletePolicyEncodingError::Schema(
            PolicySchemaError::WorkBytes { bytes: 8447, .. }
        ))
    ));
    assert_eq!((charge.g(), charge.p()), (7855, 464));
    assert_eq!(first.policy_key(), b"f:{f}:policy:r");
    assert_eq!(first.policy_key().as_ptr(), pointer);
    assert_eq!(second.policy_key(), first.policy_key());
    assert_borrowed_payload()?;
    assert_prior_peak()?;
    assert_small_seed()?;
    Ok(())
}

fn assert_borrowed_payload() -> TestResult {
    let bytes = b"\xff\0".to_vec();
    let field = [
        RawFlowPolicyValue::Atom("x"),
        RawFlowPolicyValue::Binary(&bytes),
    ];
    let rows = [RawFlowPolicyValue::Tuple(&field)];
    let profile = limits(12754)?;
    let mut charge = PolicySchemaCharge::new(profile);
    let request = {
        let wrapper = RawFlowPolicyValue::List(&rows);
        prepare_original_policy_set_request(
            &RawFlowPolicyValue::Binary(b"r"),
            &wrapper,
            profile,
            &mut charge,
        )?
        .map_err(|reason| format!("{reason:?}"))?
    };
    let key_pointer = request.policy_key().as_ptr();
    assert_eq!((charge.g(), charge.p()), (6401, 1032));
    let second = prepare_original_policy_set_request(
        &RawFlowPolicyValue::Binary(b"r"),
        &RawFlowPolicyValue::List(&[]),
        profile,
        &mut charge,
    )?
    .map_err(|reason| format!("{reason:?}"))?;
    assert_eq!((charge.g(), charge.p()), (9062, 1032));
    assert_eq!(request.policy_key().as_ptr(), key_pointer);
    assert!(matches!(
        prepare_original_policy_set_request(
            &RawFlowPolicyValue::Binary(b"r"),
            &RawFlowPolicyValue::List(&[]),
            profile,
            &mut charge
        ),
        Err(CompletePolicyEncodingError::Schema(
            PolicySchemaError::WorkBytes { bytes: 12755, .. }
        ))
    ));
    assert_eq!((charge.g(), charge.p()), (11595, 1032));
    assert_eq!(request.policy_key().as_ptr(), key_pointer);
    assert_eq!(second.policy_key(), request.policy_key());
    let (_, leaf) = request.patch().map_entry(0).ok_or("x")?;
    match leaf.raw().ok_or("binary")? {
        RawFlowPolicyValue::Binary(value) => {
            assert_eq!(value.as_ptr(), bytes.as_ptr());
            assert_eq!(value, bytes);
        }
        other => return Err(format!("different leaf {other:?}").into()),
    }
    assert_eq!(request.policy_key(), b"f:{f}:policy:r");
    Ok(())
}

fn assert_small_seed() -> TestResult {
    for maximum in [5052, 5053] {
        let profile = limits(maximum)?;
        let mut charge = PolicySchemaCharge::new(profile);
        let mut data = super::super::data(b"r".to_vec(), None, std::collections::BTreeMap::new());
        data.indexed_attributes = Some(vec![b"x".to_vec()]);
        let held =
            wf_domain::policy::complete::restore_complete_policy(1, data, profile, &mut charge)?;
        assert_eq!((charge.g(), charge.p()), (1928, 64));
        let result = prepare_original_policy_set_request(
            &RawFlowPolicyValue::Binary(b"r"),
            &RawFlowPolicyValue::List(&[]),
            profile,
            &mut charge,
        );
        if maximum == 5053 {
            assert!(result?.is_ok());
            assert_eq!((charge.g(), charge.p()), (4589, 464));
        } else {
            assert!(matches!(
                result,
                Err(CompletePolicyEncodingError::Schema(
                    PolicySchemaError::WorkBytes { bytes: 5053, .. }
                ))
            ));
            assert_eq!((charge.g(), charge.p()), (4461, 464));
        }
        assert_eq!(held.data().indexed_attributes, Some(vec![b"x".to_vec()]));
    }
    Ok(())
}

fn assert_prior_peak() -> TestResult {
    let profile = limits(23298)?;
    let mut charge = PolicySchemaCharge::new(profile);
    let list = [RawFlowPolicyValue::Nil, RawFlowPolicyValue::Nil];
    let attrs = [(RawFlowPolicyValue::Other, RawFlowPolicyValue::List(&list))];
    let held = normalize_original_complete_policy_input(
        &RawFlowPolicyValue::Binary(b"r"),
        &RawFlowPolicyValue::Map(&attrs),
        1_000_000,
        profile,
        &mut charge,
    )?
    .map_err(|reason| format!("{reason:?}"))?;
    let pointer = held.as_ptr();
    assert_eq!((charge.g(), charge.p()), (13726, 6912));
    assert!(matches!(
        prepare_original_policy_set_request(
            &RawFlowPolicyValue::Binary(b"r"),
            &RawFlowPolicyValue::List(&[]),
            profile,
            &mut charge
        ),
        Err(CompletePolicyEncodingError::Schema(
            PolicySchemaError::WorkBytes { bytes: 23299, .. }
        ))
    ));
    assert_eq!((charge.g(), charge.p()), (16259, 6912));
    assert_eq!(held.as_ptr(), pointer);
    assert_eq!(
        held,
        b"\x83t\0\0\0\x02w\x06statest\0\0\0\0w\x04typem\0\0\0\x01r"
    );
    Ok(())
}
