//! Same-account refusals, parent-derived replay and sequential view transfers.

use super::*;

pub(super) const CORE_PATCH_PARENT_PHASES: &[(u64, u64)] = &[
    (4_551, 4_423),
    (4_685, 4_551),
    (4_813, 4_685),
    (4_942, 4_813),
    (5_070, 4_942),
    (5_198, 5_070),
    (5_231, 5_198),
    (5_359, 5_231),
    (5_363, 5_359),
    (5_364, 5_363),
    (5_365, 5_364),
    (5_493, 5_365),
    (5_621, 5_493),
    (5_625, 5_621),
    (5_626, 5_625),
    (5_755, 5_626),
    (5_756, 5_755),
    (5_757, 5_756),
    (5_758, 5_757),
    (5_759, 5_758),
    (5_763, 5_759),
    (5_891, 5_763),
    (6_019, 5_891),
    (6_147, 6_019),
    (6_276, 6_147),
    (6_404, 6_276),
    (6_532, 6_404),
    (6_660, 6_532),
    (6_788, 6_660),
    (6_916, 6_788),
    (7_044, 6_916),
    (7_172, 7_044),
    (7_300, 7_172),
    (7_428, 7_300),
    (7_556, 7_428),
    (7_684, 7_556),
    (7_812, 7_684),
    (7_940, 7_812),
    (7_941, 7_940),
    (8_198, 7_941),
    (8_326, 8_198),
    (8_455, 8_326),
    (8_584, 8_455),
    (8_712, 8_584),
    (8_840, 8_712),
    (8_841, 8_840),
    (9_098, 8_841),
    (9_226, 9_098),
    (9_354, 9_226),
    (9_358, 9_354),
    (9_360, 9_358),
    (9_368, 9_360),
    (9_372, 9_368),
    (9_500, 9_372),
    (9_504, 9_500),
    (9_506, 9_504),
    (9_634, 9_506),
    (9_638, 9_634),
    (9_639, 9_638),
    (9_640, 9_639),
    (9_641, 9_640),
    (9_642, 9_641),
    (9_643, 9_642),
    (9_647, 9_643),
    (9_775, 9_647),
    (9_776, 9_775),
    (9_777, 9_776),
    (9_778, 9_777),
    (9_946, 9_778),
    (10_074, 9_946),
    (10_078, 10_074),
    (10_080, 10_078),
    (10_088, 10_080),
    (10_092, 10_088),
    (10_220, 10_092),
    (10_224, 10_220),
    (10_226, 10_224),
    (10_354, 10_226),
    (10_358, 10_354),
    (10_359, 10_358),
    (10_360, 10_359),
    (10_361, 10_360),
    (10_362, 10_361),
    (10_363, 10_362),
    (10_367, 10_363),
    (10_495, 10_367),
    (10_496, 10_495),
    (10_497, 10_496),
    (10_498, 10_497),
    (10_626, 10_498),
    (10_754, 10_626),
    (10_882, 10_754),
    (11_050, 10_882),
    (11_178, 11_050),
];

pub(super) fn assert_core_patch_parent_account(
    maximum: u64,
    target: u64,
    attempts: u64,
    peak: bool,
) -> TestResult {
    let limits = profile(maximum)?;
    let mut charge = PolicySchemaCharge::new(limits);
    let mut held_data = minimal_data();
    held_data.indexed_attributes = Some(vec![b"a".to_vec()]);
    let held = if peak {
        let mut data = minimal_data();
        data.indexed_attributes = Some(vec![b"a".to_vec()]);
        Some(restore_complete_policy(1, data, limits, &mut charge)?)
    } else {
        None
    };
    let (setup, scratch) = if peak { (1_928, 64) } else { (0, 0) };
    assert_eq!(
        (charge.g(), charge.p(), charge.total()),
        (setup, scratch, setup + scratch)
    );
    let scope = core_patch_parent_scope()?;
    let frame = core_patch_parent_frame(0, b"q", 0)?;
    let control = ControlState::initial(core_patch_parent_stamp(0)?);
    let outcomes = BTreeMap::new();
    let policies = BTreeMap::new();
    let migrations = BTreeMap::new();
    let view = core_patch_parent_view(
        &scope,
        control,
        &outcomes,
        &policies,
        &migrations,
        &migrations,
    );
    let mut owners = Vec::new();
    for attempt in 1..=attempts {
        let result = core_patch_parent_apply(view, &frame, 0, limits, &mut charge);
        let refused = attempt == attempts && maximum < target;
        if refused {
            assert_eq!(
                result
                    .err()
                    .ok_or("expected retained-owner prospective refusal")?,
                CompleteCorePolicyPatchApplyError::Request(
                    PolicySchemaError::WorkBytes {
                        bytes: target,
                        maximum
                    }
                    .into()
                )
            );
        } else {
            let plan = result?;
            owners.push(plan);
        }
        for owner in &owners {
            assert_core_patch_parent_accepted(
                owner,
                &minimal_data(),
                CORE_PATCH_PARENT_MINIMAL,
                0,
                &frame,
                (b"q", 0, 0),
            )?;
            assert_core_patch_parent_control(owner, view, &frame, 0, 0, false)?;
        }
        let g = setup + attempt * 11_178 - if refused { 128 } else { 0 };
        assert_eq!(
            (charge.g(), charge.p(), charge.total()),
            (g, scratch, g + scratch)
        );
        assert_eq!(u64::try_from(owners.len())?, attempt - u64::from(refused));
        assert!(outcomes.is_empty() && policies.is_empty() && migrations.is_empty());
        if let Some(held) = &held {
            assert_eq!(held.generation(), 1);
            assert!(held.data() == &held_data);
        }
    }
    Ok(())
}

pub(super) fn assert_core_patch_parent_replay(
    limits: PolicySchemaLimits,
    charge: &mut PolicySchemaCharge,
    rejected: bool,
) -> TestResult {
    let scope = core_patch_parent_scope()?;
    let outcomes = BTreeMap::new();
    let policies = BTreeMap::new();
    let migrations = BTreeMap::new();
    let control = ControlState::initial(core_patch_parent_stamp(0)?);
    let case = if rejected { 14 } else { 0 };
    let frame = core_patch_parent_frame(case, b"q", 10)?;
    let view = core_patch_parent_view(
        &scope,
        control,
        &outcomes,
        &policies,
        &migrations,
        &migrations,
    );
    let plan = core_patch_parent_apply(view, &frame, 0, limits, charge)?;
    assert_core_patch_parent_control(&plan, view, &frame, 0, 10, false)?;
    let (expected, status) = if rejected {
        (
            CORE_PATCH_PARENT_BUSINESS[2].1,
            RetainedStatus::PolicyRejected(PolicyRejection::new(PolicyError::InvalidStateMode)?),
        )
    } else {
        (CORE_PATCH_PARENT_MINIMAL, RetainedStatus::Accepted)
    };
    if !rejected {
        assert_core_patch_parent_accepted(
            &plan,
            &minimal_data(),
            expected,
            0,
            &frame,
            (b"q", 10, 0),
        )?;
    }
    let (_, _, proposed, decision) = plan.into_parts();
    let DispatchDecision::Fresh((installed, original)) = decision else {
        return Err("first processing must derive retained owner".into());
    };
    assert_eq!(installed.is_some(), !rejected);
    assert_core_patch_parent_outcome(&original, expected, status, &frame[83..], (b"q", 10, 0))?;
    let outcomes = BTreeMap::from([(RequestId::new(b"q".to_vec())?, original)]);
    let poison_policies = BTreeMap::from([(b"r".to_vec(), Err(ApplyError::AllocationFailed))]);
    let poison_migrations = BTreeMap::from([(b"r".to_vec(), Err(ApplyError::PositionConflict))]);
    let mut view = core_patch_parent_view(
        &scope,
        proposed.next(),
        &outcomes,
        &poison_policies,
        &poison_migrations,
        &poison_migrations,
    );
    view.history_max_events_limit = Err(ApplyError::AllocationFailed);
    view.outcome_retention_ms = Err(ApplyError::PositionConflict);
    for branch in 0..4 {
        let incoming_case = if branch == 1 {
            if rejected { 0 } else { 3 }
        } else {
            case
        };
        let time = match branch {
            2 => 110,
            3 => 10,
            _ => 11,
        };
        let incoming = core_patch_parent_frame(incoming_case, b"q", time)?;
        let verifier = FROZEN_CORE_PATCH_FRAMES[incoming_case].3;
        assert_core_patch_parent_replay_branch(
            view,
            &incoming,
            (&frame, expected, status),
            (branch, time, verifier),
            limits,
            charge,
        )?;
    }
    assert_eq!(
        installed
            .as_ref()
            .map(|(_, value)| match value {
                StoredPolicyValue::Complete(value) => Ok(value.generation()),
                StoredPolicyValue::Original(_) => Err("expected Complete fixture"),
            })
            .transpose()?,
        (!rejected).then_some(1)
    );
    if let Some((stored, value)) = &installed {
        assert_eq!(*stored, 0);
        let StoredPolicyValue::Complete(value) = value else {
            return Err("expected Complete fixture".into());
        };
        assert!(value.data() == &minimal_data());
        assert_eq!(value.encoded(), expected);
    }
    Ok(())
}

pub(super) fn assert_core_patch_parent_limits(
    limits: PolicySchemaLimits,
    charge: &mut PolicySchemaCharge,
) -> TestResult {
    let scope = core_patch_parent_scope()?;
    let control = ControlState::initial(core_patch_parent_stamp(0)?);
    let outcomes = BTreeMap::new();
    let empty = BTreeMap::new();
    let migrations = BTreeMap::new();
    let default = core_patch_parent_frame(0, b"q", 0)?;
    let policies = BTreeMap::from([(
        b"r".to_vec(),
        Ok(core_patch_parent_value(0, limits, charge)?),
    )]);
    let view = core_patch_parent_view(
        &scope,
        control,
        &outcomes,
        &policies,
        &migrations,
        &migrations,
    );
    assert_eq!(
        decide_complete_core_policy_patch_frame(
            view,
            &default,
            core_patch_parent_log(0),
            limits,
            AdmissionLimits::new(1_048_576, 767, 1_000)?,
            charge
        )
        .err()
        .ok_or("whole R=768 must refuse atomically")?,
        CompleteCorePolicyPatchApplyError::Request(CompletePolicyEncodingError::ByteLimit.into())
    );
    let reduced = PolicySchemaLimits::new(PolicyLimits::new(1)?, 20_000_000)?;
    let mut reduced_charge = PolicySchemaCharge::new(reduced);
    let frame = core_patch_parent_frame(7, b"q", 0)?;
    let mut view =
        core_patch_parent_view(&scope, control, &outcomes, &empty, &migrations, &migrations);
    view.outcome_retention_ms = Err(ApplyError::PositionConflict);
    assert_eq!(
        decide_complete_core_policy_patch_frame(
            view,
            &frame,
            core_patch_parent_log(0),
            reduced,
            AdmissionLimits::default(),
            &mut reduced_charge
        )
        .err()
        .ok_or("new state budget stays operational")?,
        CompleteCorePolicyPatchApplyError::Request(
            PolicySchemaError::Policy(PolicyError::StateBudget).into()
        )
    );
    let policies = BTreeMap::from([(
        b"r".to_vec(),
        Ok(core_patch_parent_value(3, limits, charge)?),
    )]);
    let frame = core_patch_parent_frame(1, b"q", 0)?;
    let view = core_patch_parent_view(
        &scope,
        control,
        &outcomes,
        &policies,
        &migrations,
        &migrations,
    );
    assert_eq!(
        decide_complete_core_policy_patch_frame(
            view,
            &frame,
            core_patch_parent_log(0),
            reduced,
            AdmissionLimits::default(),
            &mut reduced_charge
        )
        .err()
        .ok_or("present selected-profile authority precedes stale CAS")?,
        CompleteCorePolicyPatchApplyError::Request(PolicySchemaError::InvalidAuthority.into())
    );
    Ok(())
}

pub(super) fn assert_core_patch_parent_writers(
    limits: PolicySchemaLimits,
    charge: &mut PolicySchemaCharge,
) -> TestResult {
    let scope = core_patch_parent_scope()?;
    let mut outcomes = BTreeMap::new();
    let mut policies = BTreeMap::from([(
        b"r".to_vec(),
        Ok(core_patch_parent_value(0, limits, charge)?),
    )]);
    let migrations = BTreeMap::new();
    let mut control = ControlState::initial(core_patch_parent_stamp(0)?);
    let mut revision = 7;
    let anchor = decode_core_patch_literal(CORE_PATCH_PARENT_VALUES[0].0)?;
    let mut rows = BTreeMap::from([(b"s".to_vec(), anchor[96..130].to_vec())]);
    let mut data = present_data();
    let source = decode_core_patch_literal(FROZEN_CORE_PATCH_FRAMES[7].0)?;
    let minimal = decode_core_patch_literal(FROZEN_CORE_PATCH_FRAMES[0].0)?;
    let mut additions = core_patch_parent_source8_data().states;
    let mut first_frame = Vec::new();
    let mut first_response = Vec::new();
    for step in 0..14 {
        let intent = core_patch_parent_writer_intent(
            step,
            &source,
            &minimal,
            &mut rows,
            &mut data,
            &mut additions,
        )?;
        let request = [b'0' + u8::try_from(step)?];
        let frame = core_patch_prepare_literal_frame(
            &intent,
            [b"t", b"g", b"r", &request],
            core_patch_parent_stamp(step)?,
        )?;
        let expected = core_patch_parent_rows(step + 2, &rows)?;
        let mut view = core_patch_parent_view(
            &scope,
            control,
            &outcomes,
            &policies,
            &migrations,
            &migrations,
        );
        view.revision = revision;
        let (next, fresh) = core_patch_parent_writer_step(
            view,
            &frame,
            (&data, &expected, step + 1),
            (&request, step),
            limits,
            charge,
        )?;
        if step == 0 {
            first_frame = frame;
            first_response = expected;
        }
        let (installed, outcome) = fresh;
        let (_, value) = installed.ok_or("writer accepted policy owner")?;
        policies.insert(b"r".to_vec(), Ok(value));
        outcomes.insert(RequestId::new(request.to_vec())?, outcome);
        control = next;
        revision += 1;
        assert_eq!(outcomes.len(), usize::try_from(step + 1)?);
        let held = outcomes
            .get(&RequestId::new(b"0".to_vec())?)
            .ok_or("first held original")?;
        assert_core_patch_parent_outcome(
            held,
            &first_response,
            RetainedStatus::Accepted,
            &first_frame[83..],
            (b"0", 0, 0),
        )?;
    }
    let expected = core_patch_parent_rows(15, &rows)?;
    assert_core_patch_parent_writer_duplicates(
        (&scope, control, revision),
        &mut outcomes,
        &policies,
        &migrations,
        (&first_frame, &first_response, &data, &expected),
        limits,
        charge,
    )?;
    Ok(())
}

fn assert_core_patch_parent_writer_duplicates(
    context: (&RequestScope, ControlState, u64),
    outcomes: &mut BTreeMap<RequestId, RetainedOutcome>,
    policies: &BTreeMap<Vec<u8>, Result<StoredPolicyValue, ApplyError>>,
    migrations: &BTreeMap<Vec<u8>, Result<u64, ApplyError>>,
    expected: (&[u8], &[u8], &CompletePolicyData, &[u8]),
    limits: PolicySchemaLimits,
    charge: &mut PolicySchemaCharge,
) -> TestResult {
    let (scope, control, revision) = context;
    let mut view =
        core_patch_parent_view(scope, control, outcomes, policies, migrations, migrations);
    view.revision = revision;
    let rejected = core_patch_parent_business_frame(5)?;
    let stale = core_patch_prepare_literal_frame(
        &rejected[83..],
        [b"t", b"g", b"r", b"z"],
        core_patch_parent_stamp(20)?,
    )?;
    let plan = core_patch_parent_apply(view, &stale, 14, limits, charge)?;
    assert_core_patch_parent_control(&plan, view, &stale, 14, 20, false)?;
    let DispatchDecision::Fresh((None, outcome)) = plan.decision() else {
        return Err("writer stale CAS rejection".into());
    };
    assert_core_patch_parent_outcome(
        outcome,
        CORE_PATCH_PARENT_BUSINESS[5].1,
        RetainedStatus::PolicyRejected(PolicyRejection::new(PolicyError::StaleGeneration)?),
        &stale[83..],
        (b"z", 20, 14),
    )?;
    let (_, _, next, decision) = plan.into_parts();
    let DispatchDecision::Fresh((None, stale_outcome)) = decision else {
        return Err("whole stale bundle".into());
    };
    outcomes.insert(RequestId::new(b"z".to_vec())?, stale_outcome);
    let incoming = core_patch_prepare_literal_frame(
        &expected.0[83..],
        [b"t", b"g", b"r", b"0"],
        core_patch_parent_stamp(21)?,
    )?;
    let mut replay_view = core_patch_parent_view(
        scope,
        next.next(),
        outcomes,
        policies,
        migrations,
        migrations,
    );
    replay_view.revision = revision + 1;
    let replay = core_patch_parent_apply(replay_view, &incoming, 15, limits, charge)?;
    assert_core_patch_parent_control(&replay, replay_view, &incoming, 15, 21, false)?;
    let original = outcomes
        .get(&RequestId::new(b"0".to_vec())?)
        .ok_or("first writer retained owner")?;
    assert_core_patch_parent_outcome(
        original,
        expected.1,
        RetainedStatus::Accepted,
        &expected.0[83..],
        (b"0", 0, 0),
    )?;
    let current = core_patch_parent_stored(policies)?;
    assert_eq!(current.generation(), 15);
    assert!(current.data() == expected.2);
    assert_eq!(current.encoded(), expected.3);
    assert_eq!(outcomes.len(), 15);
    let DispatchDecision::Replay(replayed) = replay.decision() else {
        return Err("first writer replay".into());
    };
    assert_core_patch_parent_outcome(
        replayed,
        expected.1,
        RetainedStatus::Accepted,
        &expected.0[83..],
        (b"0", 0, 0),
    )?;
    assert!(std::ptr::eq(*replayed, original));
    assert_eq!(replayed.response().as_ptr(), original.response().as_ptr());
    Ok(())
}

fn core_patch_parent_writer_intent(
    step: u64,
    source: &[u8],
    minimal: &[u8],
    rows: &mut BTreeMap<Vec<u8>, Vec<u8>>,
    data: &mut CompletePolicyData,
    additions: &mut BTreeMap<Vec<u8>, CompleteStatePolicyData>,
) -> TestResult<Vec<u8>> {
    let mut intent = if step < 2 {
        source[83..135].to_vec()
    } else {
        minimal[83..135].to_vec()
    };
    intent[48..52].copy_from_slice(&1_u32.to_be_bytes());
    let name = match step {
        0 => b"queued".to_vec(),
        1 => b"review".to_vec(),
        _ => vec![b'a' + u8::try_from(step - 2)?],
    };
    if step < 2 {
        let (range, literal) = if step == 0 {
            (
                135..160,
                "00000006717565756564000100000100000004646561640000",
            )
        } else {
            (160..175, "000000067265766965770101000000")
        };
        intent.extend_from_slice(&source[range]);
        rows.insert(name.clone(), decode_core_patch_literal(literal)?);
        data.states.insert(
            name.clone(),
            additions
                .remove(name.as_slice())
                .ok_or("fixed independent SOURCE8 state")?,
        );
    } else {
        intent.extend_from_slice(b"\0\0\0\x01");
        intent.extend_from_slice(&name);
        intent.extend_from_slice(&[1, 0, 0, 0]);
        let mut row = b"\0\0\0\x01".to_vec();
        row.extend_from_slice(&name);
        row.extend_from_slice(&[0, 0, 0, 0]);
        rows.insert(name.clone(), row);
        data.states.insert(
            name,
            CompleteStatePolicyData {
                mode: None,
                retry: None,
                retention: None,
                governance: None,
            },
        );
    }

    Ok(intent)
}
