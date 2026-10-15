//! Profile, encoded caps and server-capture checks before any owned decision.

use super::{
    AdmissionLimits, EMPTY_PATCH, EncodingError, OriginalPolicySetMapOrders, PolicySchemaCharge,
    PolicySchemaError, Raw, RequestError, TestResult, digest, empty_orders, frame, profile,
    verify_original_policy_set_frame,
};

#[test]
fn public_set_frame_preserves_profiles_caps_and_capture_correspondence() -> TestResult {
    let limits = profile(2_000_000_000)?;
    let orders = empty_orders();
    let bytes = frame(Raw::List(&[]), &orders)?;
    let mut charge = PolicySchemaCharge::new(limits);
    assert_eq!(
        verify_original_policy_set_frame(
            &[],
            &orders,
            profile(1_999_999_999)?,
            AdmissionLimits::default(),
            &mut charge
        )
        .err(),
        Some(RequestError::Encoding(EncodingError::Schema(
            PolicySchemaError::ProfileMismatch
        )))
    );
    assert_eq!((charge.g(), charge.p()), (0, 0));
    let mut charge = PolicySchemaCharge::new(limits);
    assert_eq!(
        verify_original_policy_set_frame(
            &bytes,
            &orders,
            limits,
            AdmissionLimits::new(1_048_576, 182, 1_000)?,
            &mut charge
        )
        .err(),
        Some(RequestError::Encoding(EncodingError::ByteLimit))
    );
    assert_eq!((charge.g(), charge.p()), (0, 0));
    for captured in [
        OriginalPolicySetMapOrders::new([0; 32], None, &[(0, &[], &[])], &[], &[]),
        OriginalPolicySetMapOrders::new(digest(EMPTY_PATCH), None, &[], &[], &[]),
        OriginalPolicySetMapOrders::new(digest(EMPTY_PATCH), None, &[(0, &[0], &[0])], &[], &[]),
        OriginalPolicySetMapOrders::new(digest(EMPTY_PATCH), None, &[(1, &[], &[])], &[], &[]),
    ] {
        let mut charge = PolicySchemaCharge::new(limits);
        assert_eq!(
            verify_original_policy_set_frame(
                &bytes,
                &captured,
                limits,
                AdmissionLimits::default(),
                &mut charge
            )
            .err(),
            Some(RequestError::Encoding(EncodingError::Schema(
                PolicySchemaError::InvalidAuthority
            )))
        );
    }
    for end in 0..bytes.len() {
        let mut charge = PolicySchemaCharge::new(limits);
        assert!(
            verify_original_policy_set_frame(
                &bytes[..end],
                &orders,
                limits,
                AdmissionLimits::default(),
                &mut charge
            )
            .is_err()
        );
    }
    let mut charge = PolicySchemaCharge::new(limits);
    let verified = verify_original_policy_set_frame(
        &bytes,
        &orders,
        limits,
        AdmissionLimits::new(1_048_576, 183, 1_000)?,
        &mut charge,
    )?;
    assert_eq!(verified.encoded(), bytes);
    assert_producer_capture_comparison()?;
    assert_producer_events()?;
    assert_verifier_events()?;
    assert_continuous_account()?;
    assert_continuous_fresh()?;
    Ok(())
}

fn assert_producer_capture_comparison() -> TestResult {
    let limits = profile(2_000_000_000)?;
    let prepared = super::prepared(Raw::List(&[]))?;
    let orders = OriginalPolicySetMapOrders::new([0; 32], None, &[(0, &[], &[])], &[], &[]);
    let input = super::OriginalPolicySetFrameInput::new(
        super::scope()?,
        super::RequestId::new(b"q".to_vec())?,
        super::stamp(0)?,
        &prepared,
        &orders,
    );
    let mut charge = PolicySchemaCharge::new(limits);
    assert_eq!(
        super::prepare_original_policy_set_frame(
            input,
            limits,
            AdmissionLimits::default(),
            &mut charge
        )
        .err(),
        Some(RequestError::Encoding(EncodingError::Schema(
            PolicySchemaError::InvalidAuthority
        )))
    );
    assert_eq!((charge.g(), charge.p()), (6_918, 1_441));
    Ok(())
}

const HEADER: &[u64] = &[
    128, 4, 2, 1, 128, 4, 1, 128, 4, 1, 128, 4, 1, 128, 4, 1, 2, 32, 8, 2, 8, 4, 4,
];
const INTENT_HEAD: &[u64] = &[128, 33, 128, 4, 1, 128, 4, 14, 1, 1, 1];
const SIDE_ROOT: &[u64] = &[128, 4, 4, 4];
const EMPTY_MAP_MEASURE: &[u64] = &[128, 4, 1, 128, 4, 4, 4, 128, 128, 16, 16, 4, 4];
const EMPTY_MAP_EMIT: &[u64] = &[
    128, 4, 1, 128, 4, 4, 4, 128, 128, 16, 16, 132, 132, 132, 132, 132,
];
const X_MAP_MEASURE: &[u64] = &[
    128, 4, 1, 128, 128, 128, 4, 4, 4, 4, 4, 128, 128, 16, 16, 4, 16, 4, 4, 4, 4, 4, 128, 128, 4,
    1, 4, 1, 128, 4, 1, 1, 4, 1,
];
const X_MAP_EMIT: &[u64] = &[
    128, 4, 1, 128, 128, 128, 4, 4, 4, 4, 4, 128, 128, 16, 16, 4, 16, 4, 132, 132, 132, 4, 4, 132,
    128, 132, 128, 4, 1, 4, 1, 132, 132, 128, 4, 1, 1, 4, 1, 132, 132, 132,
];

fn producer_events(nonempty: bool) -> Vec<(u64, u64)> {
    // Source-derived old node/sink grammar plus the selected typed controls,
    // one metadata admission, reached record calls and three real hashes.
    let (frame, intent, patch, first_peak, final_peak, reserve) = if nonempty {
        (220, 105, 46, 1_626, 1_846, 152)
    } else {
        (183, 76, 17, 1_258, 1_441, 136)
    };
    let mut events = Vec::new();
    push_fees(&mut events, &[128, 128, 14, 128]);
    if nonempty {
        push_fees(&mut events, &[128, 1, 128]);
    }
    push_fees(&mut events, &[128, 16, 128, 16, 16, 16]);
    events.push((0, first_peak));
    push_fees(&mut events, &[reserve]);
    push_fees(&mut events, HEADER);
    push_fees(&mut events, INTENT_HEAD);
    push_fees(
        &mut events,
        if nonempty {
            X_MAP_MEASURE
        } else {
            EMPTY_MAP_MEASURE
        },
    );
    push_fees(&mut events, SIDE_ROOT);
    push_fees(&mut events, &[16]);
    events.push((0, final_peak));
    push_fees(&mut events, &[128 + frame, frame]);
    push_fees(&mut events, HEADER);
    push_fees(&mut events, SIDE_ROOT);
    push_fees(&mut events, INTENT_HEAD);
    push_fees(
        &mut events,
        if nonempty { X_MAP_EMIT } else { EMPTY_MAP_EMIT },
    );
    push_fees(
        &mut events,
        &[132, 128 + patch, 128 + intent, 160, 128 + frame, 192],
    );
    events
}

fn push_fees(events: &mut Vec<(u64, u64)>, fees: &[u64]) {
    events.extend(fees.iter().map(|&fee| (fee, 0)));
}

fn producer_input<'request, 'input, 'capture>(
    prepared: &'request super::OriginalPolicySetRequest<'input>,
    orders: &'capture OriginalPolicySetMapOrders<'capture>,
) -> TestResult<super::OriginalPolicySetFrameInput<'request, 'input, 'capture>> {
    Ok(super::OriginalPolicySetFrameInput::new(
        super::scope()?,
        super::RequestId::new(b"q".to_vec())?,
        super::stamp(0)?,
        prepared,
        orders,
    ))
}

fn assert_producer_events() -> TestResult {
    let field = [Raw::Atom("x"), Raw::Integer(7)];
    let opts = [Raw::Tuple(&field)];
    let canonical = super::hex(
        "0000002e04000000000000000100000019000000230000002e0000000a0200000001780000000b01000000000107",
    )?;
    for (nonempty, expected_g, expected_p, length) in
        [(false, 6_918, 1_441, 183), (true, 9_422, 1_846, 220)]
    {
        let prepared = super::prepared(if nonempty {
            Raw::List(&opts)
        } else {
            Raw::List(&[])
        })?;
        let patch = if nonempty {
            canonical.as_slice()
        } else {
            EMPTY_PATCH
        };
        let records: &[(u32, &[u32], &[u32])] = if nonempty {
            &[(0, &[0], &[0])]
        } else {
            &[(0, &[], &[])]
        };
        let orders = OriginalPolicySetMapOrders::new(digest(patch), None, records, &[], &[]);
        let events = producer_events(nonempty);
        check_producer_events(&prepared, &orders, &events)?;
        assert_eq!(events.iter().map(|&(g, _)| g).sum::<u64>(), expected_g);
        assert_eq!(events.iter().map(|&(_, p)| p).max(), Some(expected_p));
        let limits = profile(expected_g + expected_p)?;
        let mut charge = PolicySchemaCharge::new(limits);
        let bytes = super::prepare_original_policy_set_frame(
            producer_input(&prepared, &orders)?,
            limits,
            AdmissionLimits::default(),
            &mut charge,
        )?;
        assert_eq!(bytes.as_bytes().len(), length);
        assert_eq!((charge.g(), charge.p()), (expected_g, expected_p));
    }
    Ok(())
}

fn check_producer_events(
    prepared: &super::OriginalPolicySetRequest<'_>,
    orders: &OriginalPolicySetMapOrders<'_>,
    events: &[(u64, u64)],
) -> TestResult {
    let (mut g, mut p) = (0, 0);
    for &(delta, scratch) in events {
        let target = g + delta + p.max(scratch);
        let limits = profile(target - 1)?;
        let mut charge = PolicySchemaCharge::new(limits);
        assert_eq!(
            super::prepare_original_policy_set_frame(
                producer_input(prepared, orders)?,
                limits,
                AdmissionLimits::default(),
                &mut charge,
            )
            .err(),
            Some(RequestError::Encoding(EncodingError::Schema(
                PolicySchemaError::WorkBytes {
                    bytes: target,
                    maximum: target - 1,
                }
            )))
        );
        assert_eq!((charge.g(), charge.p()), (g, p));
        g += delta;
        p = p.max(scratch);
    }
    Ok(())
}

const EMPTY_SIDE_SCAN: &[u64] = &[128, 4, 4, 4, 128, 4, 4];
const X_SIDE_SCAN: &[u64] = &[128, 4, 4, 4, 128, 4, 4, 4, 16, 4];
const EMPTY_MAP_SCAN: &[u64] = &[128, 4, 1, 4, 4, 4];
const X_MAP_SCAN: &[u64] = &[
    128, 4, 1, 4, 4, 4, 4, 4, 128, 4, 1, 4, 1, 128, 4, 1, 1, 4, 1,
];

pub(super) fn verifier_events(nonempty: bool) -> Vec<(u64, u64)> {
    let (frame, intent, patch, backing) = if nonempty {
        (220, 105, 46, 292)
    } else {
        (183, 76, 17, 247)
    };
    let mut events = Vec::new();
    push_fees(&mut events, HEADER);
    push_fees(
        &mut events,
        if nonempty {
            X_SIDE_SCAN
        } else {
            EMPTY_SIDE_SCAN
        },
    );
    push_fees(&mut events, INTENT_HEAD);
    push_fees(
        &mut events,
        if nonempty { X_MAP_SCAN } else { EMPTY_MAP_SCAN },
    );
    push_fees(&mut events, &[130, 128 + intent, 128 + patch]);
    push_fees(
        &mut events,
        &[192, 128, 16, 128, 16, 16, 16, 192, 128, 16, 16],
    );
    if nonempty {
        push_fees(&mut events, &[4, 16, 4, 16, 16]);
    }
    push_fees(&mut events, &[16, 128, 128]);
    for identity in 1..=4 {
        events.push((0, backing + 129 * identity));
        push_fees(&mut events, &[129]);
    }
    push_fees(&mut events, &[128, 533]);
    events.push((0, backing + 516 + 21));
    push_fees(&mut events, &[149, 533, 128 + frame]);
    events
}

fn assert_verifier_events() -> TestResult {
    let field = [Raw::Atom("x"), Raw::Integer(7)];
    let opts = [Raw::Tuple(&field)];
    let canonical = super::hex(
        "0000002e04000000000000000100000019000000230000002e0000000a0200000001780000000b01000000000107",
    )?;
    for (nonempty, expected_g, expected_p) in [(false, 5_376, 784), (true, 5_836, 829)] {
        let patch = if nonempty {
            canonical.as_slice()
        } else {
            EMPTY_PATCH
        };
        let records: &[(u32, &[u32], &[u32])] = if nonempty {
            &[(0, &[0], &[0])]
        } else {
            &[(0, &[], &[])]
        };
        let orders = OriginalPolicySetMapOrders::new(digest(patch), None, records, &[], &[]);
        let bytes = frame(
            if nonempty {
                Raw::List(&opts)
            } else {
                Raw::List(&[])
            },
            &orders,
        )?;
        let events = verifier_events(nonempty);
        check_verifier_events(&bytes, &orders, &events)?;
        assert_eq!(events.iter().map(|&(g, _)| g).sum::<u64>(), expected_g);
        assert_eq!(events.iter().map(|&(_, p)| p).max(), Some(expected_p));
        let limits = profile(expected_g + expected_p)?;
        let mut charge = PolicySchemaCharge::new(limits);
        let verified = verify_original_policy_set_frame(
            &bytes,
            &orders,
            limits,
            AdmissionLimits::default(),
            &mut charge,
        )?;
        assert_eq!(verified.encoded(), bytes);
        assert_eq!((charge.g(), charge.p()), (expected_g, expected_p));
    }
    Ok(())
}

fn check_verifier_events(
    bytes: &[u8],
    orders: &OriginalPolicySetMapOrders<'_>,
    events: &[(u64, u64)],
) -> TestResult {
    let (mut g, mut p) = (0, 0);
    for &(delta, scratch) in events {
        let target = g + delta + p.max(scratch);
        let limits = profile(target - 1)?;
        let mut charge = PolicySchemaCharge::new(limits);
        assert_eq!(
            verify_original_policy_set_frame(
                bytes,
                orders,
                limits,
                AdmissionLimits::default(),
                &mut charge
            )
            .err(),
            Some(RequestError::Encoding(EncodingError::Schema(
                PolicySchemaError::WorkBytes {
                    bytes: target,
                    maximum: target - 1,
                }
            )))
        );
        assert_eq!((charge.g(), charge.p()), (g, p));
        g += delta;
        p = p.max(scratch);
    }
    Ok(())
}

fn assert_continuous_account() -> TestResult {
    // Existing seed oracles precede this batch: G1928/P64 and G13726/P6912.
    // Public MIN2661, producer6918 and verifier5376 all use this SAME account.
    for (seed, g, p) in [(0, 14_955, 1_441), (1, 16_883, 1_441), (2, 28_681, 6_912)] {
        for exact in [false, true] {
            let maximum = g + p - u64::from(!exact);
            let limits = profile(maximum)?;
            let mut charge = PolicySchemaCharge::new(limits);
            seed_account(seed, limits, &mut charge)?;
            let prepared = super::prepare_original_policy_set_request(
                &Raw::Binary(b"r"),
                &Raw::List(&[]),
                limits,
                &mut charge,
            )?
            .map_err(|reason| format!("valid public MIN: {reason:?}"))?;
            let orders = empty_orders();
            let bytes = super::prepare_original_policy_set_frame(
                producer_input(&prepared, &orders)?,
                limits,
                AdmissionLimits::default(),
                &mut charge,
            )?;
            // These owners actually drop. Their historical P remains a maximum.
            drop(prepared);
            let result = verify_original_policy_set_frame(
                bytes.as_bytes(),
                &orders,
                limits,
                AdmissionLimits::default(),
                &mut charge,
            );
            if exact {
                assert_eq!(result?.header(super::log(0)).intent.version(), 4);
                assert_eq!((charge.g(), charge.p()), (g, p));
            } else {
                assert_eq!(
                    result.err(),
                    Some(RequestError::Encoding(EncodingError::Schema(
                        PolicySchemaError::WorkBytes {
                            bytes: g + p,
                            maximum
                        },
                    )))
                );
                assert_eq!((charge.g(), charge.p()), (g - 311, p));
            }
        }
    }
    Ok(())
}

fn seed_account(
    seed: u8,
    limits: super::PolicySchemaLimits,
    charge: &mut PolicySchemaCharge,
) -> TestResult {
    match seed {
        0 => (),
        1 => {
            let mut data = super::super::minimal_data();
            data.indexed_attributes = Some(vec![b"x".to_vec()]);
            let held =
                wf_domain::policy::complete::restore_complete_policy(1, data, limits, charge)?;
            assert_eq!(held.data().indexed_attributes, Some(vec![b"x".to_vec()]));
            assert_eq!((charge.g(), charge.p()), (1_928, 64));
            drop(held);
        }
        2 => {
            let children = [Raw::Nil, Raw::Nil];
            let attrs = [(Raw::Other, Raw::List(&children))];
            let held = wf_domain::policy::complete::normalize_original_complete_policy_input(
                &Raw::Binary(b"r"),
                &Raw::Map(&attrs),
                1_000_000,
                limits,
                charge,
            )?
            .map_err(|reason| format!("existing seed: {reason:?}"))?;
            assert_eq!((charge.g(), charge.p()), (13_726, 6_912));
            assert_eq!(
                held,
                b"\x83t\0\0\0\x02w\x06statest\0\0\0\0w\x04typem\0\0\0\x01r"
            );
            drop(held);
        }
        _ => return Err("unknown independent seed".into()),
    }
    Ok(())
}

fn assert_continuous_fresh() -> TestResult {
    // Public2661 + producer6918 + verify5376 + Fresh gate519 + Fresh46000.
    // The physical-based joined Layout and two final owners peak at38212.
    let namespace = super::scope()?;
    let outcomes = super::BTreeMap::new();
    let policies = super::BTreeMap::new();
    let marks = super::BTreeMap::new();
    let expected = super::minimal(1)?;
    let supplied = super::view(
        super::GateView {
            scope: &namespace,
            control: super::ControlState::initial(super::stamp(0)?),
            outcomes: &outcomes,
        },
        &policies,
        &marks,
        &marks,
        super::safety(),
    );
    for (seed, g) in [(0, 61_474), (1, 63_402), (2, 75_200)] {
        for exact in [false, true] {
            let maximum = g + 38_212 - u64::from(!exact);
            let limits = profile(maximum)?;
            let mut charge = PolicySchemaCharge::new(limits);
            seed_account(seed, limits, &mut charge)?;
            let prepared = super::prepare_original_policy_set_request(
                &Raw::Binary(b"r"),
                &Raw::List(&[]),
                limits,
                &mut charge,
            )?
            .map_err(|reason| format!("valid public MIN: {reason:?}"))?;
            let orders = empty_orders();
            let bytes = super::prepare_original_policy_set_frame(
                producer_input(&prepared, &orders)?,
                limits,
                AdmissionLimits::default(),
                &mut charge,
            )?;
            drop(prepared);
            let result = super::apply(
                &supplied,
                bytes.as_bytes(),
                &orders,
                0,
                limits,
                AdmissionLimits::default(),
                &mut charge,
            );
            drop(bytes);
            if exact {
                super::accepted(&result?, 0, 1, &expected)?;
                assert_eq!((charge.g(), charge.p()), (g, 38_212));
            } else {
                assert_eq!(
                    result.err(),
                    Some(super::RawError::Replace(super::ExistingError::Request(
                        RequestError::Encoding(EncodingError::Schema(
                            PolicySchemaError::WorkBytes {
                                bytes: g + 38_212,
                                maximum,
                            }
                        )),
                    )))
                );
                assert_eq!((charge.g(), charge.p()), (g - 128, 38_212));
            }
        }
    }
    Ok(())
}
