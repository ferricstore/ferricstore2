//! One retained gate precedes changed current, capture and contextual reads.

use super::{
    AdmissionLimits, ApplyError, BTreeMap, ControlState, DispatchDecision, EMPTY_PATCH, GateView,
    OriginalEtfSafety, OriginalPolicySetMapOrders, PolicySchemaCharge, Raw, ReadValue, RequestId,
    TestResult, accepted, apply, digest, empty_orders, frame, frame_with, minimal, profile, scope,
    stamp, view,
};

#[test]
fn public_set_parent_keeps_replay_expiry_and_intent_conflicts_before_fresh_reads() -> TestResult {
    let limits = profile(2_000_000_000)?;
    let namespace = scope()?;
    let original_orders = empty_orders();
    let original = frame(Raw::List(&[]), &original_orders)?;
    let expected = minimal(1)?;
    let empty = BTreeMap::new();
    let marks = BTreeMap::new();
    let no_outcomes = BTreeMap::new();
    let mut charge = PolicySchemaCharge::new(limits);
    let origin = apply(
        &view(
            GateView {
                scope: &namespace,
                control: ControlState::initial(stamp(0)?),
                outcomes: &no_outcomes,
            },
            &empty,
            &marks,
            &marks,
            super::safety(),
        ),
        &original,
        &original_orders,
        0,
        limits,
        AdmissionLimits::default(),
        &mut charge,
    )?;
    accepted(&origin, 0, 1, &expected)?;
    let (_, _, control, decision) = origin.into_parts();
    let DispatchDecision::Fresh((install, outcome)) = decision else {
        return Err("original Fresh result".into());
    };
    let base = control.next();
    let request = RequestId::new(b"q".to_vec())?;
    let outcomes = BTreeMap::from([(request.clone(), outcome)]);
    let poison = BTreeMap::from([(b"r".to_vec(), ReadValue::Other)]);
    let failed = BTreeMap::from([(b"r".to_vec(), Err(ApplyError::PositionConflict))]);
    let poison_records = [(0, &[99_u32][..], &[99_u32][..])];
    let orders = OriginalPolicySetMapOrders::new(
        digest(EMPTY_PATCH),
        Some(&[0]),
        &[(0, &[], &[])],
        &poison_records,
        &poison_records,
    );
    let mut supplied = view(
        GateView {
            scope: &namespace,
            control: base,
            outcomes: &outcomes,
        },
        &poison,
        &failed,
        &failed,
        OriginalEtfSafety::new(&["x", "x"], &[], None, &[]),
    );
    supplied.history_max_events_limit = Err(ApplyError::InvalidAuthority);
    supplied.outcome_retention_ms = Err(ApplyError::AllocationFailed);
    for (case, time, index) in [(0, 0, 0), (1, 99, 1), (2, 100, 1)] {
        let bytes = frame_with(Raw::List(&[]), &orders, b"q", time)?;
        let mut charge = PolicySchemaCharge::new(limits);
        let plan = apply(
            &supplied,
            &bytes,
            &orders,
            index,
            limits,
            AdmissionLimits::default(),
            &mut charge,
        )?;
        assert_eq!(plan.base_control(), base);
        assert_eq!(plan.expected_revision(), 7);
        assert_eq!(plan.control().is_log_replay(), case == 0);
        match case {
            0 => assert!(matches!(plan.decision(), DispatchDecision::LogReplay)),
            1 => {
                let DispatchDecision::Replay(replayed) = plan.decision() else {
                    return Err("retained replay before poisoned reads".into());
                };
                let replayed: &super::RetainedOutcome = replayed;
                assert!(std::ptr::eq(
                    std::ptr::from_ref(replayed),
                    std::ptr::from_ref(&outcomes[&request]),
                ));
                assert_eq!(replayed.response(), expected);
            }
            _ => assert!(matches!(plan.decision(), DispatchDecision::Expired)),
        }
        assert_eq!((charge.g(), charge.p()), ([5_766, 6_611, 6_611][case], 784));
        assert_gate_boundaries(&supplied, &bytes, &orders, index, case == 0)?;
        assert_eq!(outcomes[&request].response(), expected);
        assert_eq!(outcomes[&request].expires_at().get(), 100);
    }
    assert_conflicts(&supplied, &orders)?;
    let install = install.ok_or("original installation")?;
    assert_eq!(install.encoded(), expected);
    Ok(())
}

fn assert_gate_boundaries(
    supplied: &super::RawView<'_, '_, '_>,
    bytes: &[u8],
    orders: &OriginalPolicySetMapOrders<'_>,
    index: u64,
    log_replay: bool,
) -> TestResult {
    // The same already-frozen verifier is followed by the unchanged header
    // gate: format, six scope lengths, control, and only then retained lookup.
    let mut events = super::refusals::verifier_events(false);
    events.extend([128, 134, 128].map(|g| (g, 0)));
    if !log_replay {
        events.extend([129, 134, 130, 128, 196, 128].map(|g| (g, 0)));
    }
    let (mut g, mut p) = (0, 0);
    for (delta, scratch) in events {
        let target = g + delta + p.max(scratch);
        let limits = profile(target - 1)?;
        let mut charge = PolicySchemaCharge::new(limits);
        assert_eq!(
            apply(
                supplied,
                bytes,
                orders,
                index,
                limits,
                AdmissionLimits::default(),
                &mut charge,
            )
            .err(),
            Some(super::RawError::Replace(super::ExistingError::Request(
                super::RequestError::Encoding(super::EncodingError::Schema(
                    super::PolicySchemaError::WorkBytes {
                        bytes: target,
                        maximum: target - 1,
                    },
                )),
            )))
        );
        assert_eq!((charge.g(), charge.p()), (g, p));
        g += delta;
        p = p.max(scratch);
    }
    assert_eq!((g, p), (if log_replay { 5_766 } else { 6_611 }, 784));
    let limits = profile(g + p)?;
    let mut charge = PolicySchemaCharge::new(limits);
    let result = apply(
        supplied,
        bytes,
        orders,
        index,
        limits,
        AdmissionLimits::default(),
        &mut charge,
    )?;
    assert_eq!((charge.g(), charge.p()), (g, p));
    assert_eq!(result.control().is_log_replay(), log_replay);
    Ok(())
}

fn assert_conflicts(
    supplied: &super::RawView<'_, '_, '_>,
    orders: &OriginalPolicySetMapOrders<'_>,
) -> TestResult {
    let limits = profile(2_000_000_000)?;
    for (name, value) in [
        ("replace", Raw::Atom("true")),
        ("state", Raw::Binary(b"")),
        ("state", Raw::Binary(&[255, 0])),
        ("expected_generation", Raw::Integer(0)),
    ] {
        let pair = [Raw::Atom(name), value];
        let opts = [Raw::Tuple(&pair)];
        let bytes = frame_with(Raw::List(&opts), orders, b"q", 99)?;
        let mut charge = PolicySchemaCharge::new(limits);
        let plan = apply(
            supplied,
            &bytes,
            orders,
            1,
            limits,
            AdmissionLimits::default(),
            &mut charge,
        )?;
        assert!(matches!(plan.decision(), DispatchDecision::IntentConflict));
        let (g, p) = match (name, value) {
            ("replace", _) => (6_483, 784),
            ("state", Raw::Binary(b"")) => (6_623, 788),
            ("state", _) => (6_629, 790),
            _ => (6_507, 792),
        };
        assert_eq!((charge.g(), charge.p()), (g, p));
    }
    Ok(())
}
