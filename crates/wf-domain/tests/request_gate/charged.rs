//! Charged point classifications and independently priced continuous accounts.

use super::{
    AdmissionLimits, ApplyError, BTreeMap, ClientIntent, CommittedHeader, ControlState,
    EntryIdentity, EntryStamp, GateDecision, GateView, PointGateView, PolicyError, PolicyLimits,
    PolicyRejection, RequestId, RequestScope, RetainedMetadata, RetainedOutcome, RetainedStatus,
    TestResult, TimestampMillis, identity, inspect_point_request, point_header, point_outcome,
    scope, time,
};
use wf_domain::policy::complete::{
    CompletePolicyData, PolicySchemaCharge, PolicySchemaError, PolicySchemaLimits,
    restore_complete_policy,
};
use wf_domain::workflow::request::gate::{
    ChargedRequestGateError, inspect_complete_core_policy_patch_gate,
    inspect_complete_core_policy_patch_point_gate,
};

#[test]
fn charged_point_gate_preserves_complete_decisions_and_authority() -> TestResult {
    let scope = scope(b"tenant")?;
    let request = RequestId::new(b"request".to_vec())?;
    let control = ControlState::restored(
        time(10, 10)?,
        identity(1),
        EntryStamp::Stamped(time(10, 10)?),
    )?;
    for status in [
        RetainedStatus::Accepted,
        RetainedStatus::PolicyRejected(PolicyRejection::new(PolicyError::StaleGeneration)?),
    ] {
        let original = RetainedOutcome::new(
            RetainedMetadata {
                scope: scope.clone(),
                request: request.clone(),
                intent: ClientIntent::new(1, [1; 32])?,
                original_position: identity(1).log(),
                original_time: TimestampMillis::new(10),
                expires_at: TimestampMillis::new(100),
                status,
            },
            1,
            b"original\0\xff".to_vec(),
            AdmissionLimits::default(),
        )?;
        for class in 0..6 {
            assert_class(&original, control, class)?;
        }
    }
    assert_authority(&scope, &request, control)?;
    let limits = PolicySchemaLimits::default();
    let admission = AdmissionLimits::default();
    let initial_view = PointGateView {
        control: ControlState::initial(time(0, 0)?),
        retained_nonempty: false,
        outcome: None,
        scope: &scope,
    };
    let header = point_header(&scope, &request, 0, 1)?;
    let mut charge = PolicySchemaCharge::new(limits);
    let initial = inspect_complete_core_policy_patch_point_gate(
        initial_view,
        header,
        limits,
        admission,
        &mut charge,
    )?;
    assert_eq!(initial.decision(), GateDecision::Fresh);
    assert_eq!(
        initial
            .control()
            .next()
            .last_entry()
            .map(EntryIdentity::log),
        Some(identity(0).log())
    );
    let applied = inspect_complete_core_policy_patch_point_gate(
        PointGateView {
            control: initial.control().next(),
            ..initial_view
        },
        header,
        limits,
        admission,
        &mut charge,
    )?;
    assert_eq!(applied.decision(), GateDecision::LogReplay);
    assert_eq!(applied.control().next(), initial.control().next());
    Ok(())
}

fn assert_class(original: &RetainedOutcome, control: ControlState, class: u8) -> TestResult {
    let admission = AdmissionLimits::default();
    let limits = PolicySchemaLimits::default();
    let scope = original.scope();
    let request = original.request();
    let expected = scope.clone();
    let mut header = point_header(
        scope,
        request,
        if class == 4 { 1 } else { 2 },
        match class {
            3 => 100,
            4 => 10,
            _ => 20,
        },
    )?;
    if class == 2 {
        header.intent = ClientIntent::new(1, [2; 32])?;
    }
    let retained = (class != 0 && class != 5).then_some(original);
    let outcomes = if class == 0 {
        BTreeMap::new()
    } else if class == 5 {
        let other = RequestId::new(b"other".to_vec())?;
        BTreeMap::from([(other.clone(), point_outcome(scope.clone(), other, 1, 10)?)])
    } else {
        BTreeMap::from([(request.clone(), original.clone())])
    };
    let mut charge = PolicySchemaCharge::new(limits);
    let point = inspect_complete_core_policy_patch_point_gate(
        PointGateView {
            scope: &expected,
            control,
            retained_nonempty: class != 0,
            outcome: retained,
        },
        header,
        limits,
        admission,
        &mut charge,
    )?;
    drop(expected);
    let mut full_charge = PolicySchemaCharge::new(limits);
    let full = inspect_complete_core_policy_patch_gate(
        GateView {
            scope,
            control,
            outcomes: &outcomes,
        },
        header,
        limits,
        admission,
        &mut full_charge,
    )?;
    assert_eq!(point, full);
    let work = match class {
        0 | 5 => 557,
        2 => 1_189,
        4 => 422,
        _ => 1_317,
    };
    assert_eq!((charge.g(), charge.p(), charge.total()), (work, 0, work));
    assert_eq!((full_charge.g(), full_charge.p()), (work, 0));
    if class == 1 {
        let GateDecision::Replay(replayed) = point.decision() else {
            return Err("missing original replay".into());
        };
        assert!(std::ptr::eq(replayed, original));
        assert_eq!(replayed, original);
        assert_eq!(replayed.response(), b"original\0\xff");
        assert_eq!(
            (replayed.original_time().get(), replayed.expires_at().get()),
            (10, 100)
        );
        assert_eq!(replayed.status(), original.status());
    }
    Ok(())
}

fn assert_authority(
    query: &RequestScope,
    request: &RequestId,
    control: ControlState,
) -> TestResult {
    let admission = AdmissionLimits::default();
    let limits = PolicySchemaLimits::default();
    let original = point_outcome(query.clone(), request.clone(), 1, 10)?;
    let foreign = scope(b"foreign")?;
    let view = PointGateView {
        scope: query,
        control,
        retained_nonempty: true,
        outcome: Some(&original),
    };
    let replay_header = point_header(query, request, 1, 10)?;
    for bad in [
        PointGateView {
            retained_nonempty: false,
            ..view
        },
        PointGateView {
            control: ControlState::initial(time(0, 0)?),
            outcome: None,
            ..view
        },
        PointGateView {
            scope: &foreign,
            outcome: None,
            ..view
        },
    ] {
        let mut charge = PolicySchemaCharge::new(limits);
        assert_eq!(
            inspect_complete_core_policy_patch_point_gate(
                bad,
                replay_header,
                limits,
                admission,
                &mut charge
            ),
            Err(ChargedRequestGateError::Apply(ApplyError::InvalidAuthority))
        );
        assert_eq!(
            inspect_point_request(bad, replay_header, admission),
            Err(ApplyError::InvalidAuthority)
        );
    }
    for bad in [
        point_outcome(foreign, request.clone(), 1, 10)?,
        point_outcome(query.clone(), RequestId::new(b"other".to_vec())?, 1, 10)?,
        point_outcome(query.clone(), request.clone(), 2, 10)?,
        point_outcome(query.clone(), request.clone(), 1, 11)?,
    ] {
        let bad_view = PointGateView {
            outcome: Some(&bad),
            ..view
        };
        let mut charge = PolicySchemaCharge::new(limits);
        assert_eq!(
            inspect_complete_core_policy_patch_point_gate(
                bad_view,
                point_header(query, request, 2, 20)?,
                limits,
                admission,
                &mut charge
            ),
            Err(ChargedRequestGateError::Apply(ApplyError::InvalidAuthority))
        );
        assert_eq!(
            inspect_complete_core_policy_patch_point_gate(
                bad_view,
                replay_header,
                limits,
                admission,
                &mut charge
            )?
            .decision(),
            GateDecision::LogReplay
        );
    }
    Ok(())
}

#[test]
fn charged_point_gate_admits_every_reached_event_on_one_account() -> TestResult {
    let scope = scope(b"tenant")?;
    let request = RequestId::new(b"request".to_vec())?;
    let original = point_outcome(scope.clone(), request.clone(), 1, 10)?;
    let control = ControlState::restored(
        time(10, 10)?,
        identity(1),
        EntryStamp::Stamped(time(10, 10)?),
    )?;
    let view = PointGateView {
        scope: &scope,
        control,
        retained_nonempty: true,
        outcome: Some(&original),
    };
    let header = point_header(&scope, &request, 2, 20)?;
    let phases = [128, 294, 422, 557, 723, 865, 993, 1_189, 1_317];
    for (index, target) in phases.into_iter().enumerate() {
        for maximum in [target - 1, target] {
            assert_boundary(view, header, phases, index, maximum)?;
        }
    }
    assert_repeated(
        PointGateView {
            outcome: None,
            ..view
        },
        header,
    )
}

fn assert_boundary(
    view: PointGateView<'_, '_>,
    header: CommittedHeader<'_>,
    phases: [u64; 9],
    index: usize,
    maximum: u64,
) -> TestResult {
    let limits = PolicySchemaLimits::new(PolicyLimits::default(), maximum)?;
    let mut charge = PolicySchemaCharge::new(limits);
    let target = phases[index];
    let original = view.outcome.ok_or("missing retained fixture")?;
    let result = inspect_complete_core_policy_patch_point_gate(
        view,
        header,
        limits,
        AdmissionLimits::default(),
        &mut charge,
    );
    if maximum == 1_317 {
        assert_eq!(result?.decision(), GateDecision::Replay(original));
    } else {
        let refused = if maximum == target {
            phases[index + 1]
        } else {
            target
        };
        assert_eq!(
            result,
            Err(ChargedRequestGateError::Schema(
                PolicySchemaError::WorkBytes {
                    bytes: refused,
                    maximum
                }
            ))
        );
    }
    let admitted = if maximum == target {
        target
    } else {
        index.checked_sub(1).map_or(0, |i| phases[i])
    };
    assert_eq!(
        (charge.g(), charge.p(), charge.total()),
        (admitted, 0, admitted)
    );
    assert_eq!(
        *original,
        point_outcome(original.scope().clone(), original.request().clone(), 1, 10)?
    );
    Ok(())
}

fn assert_repeated(view: PointGateView<'_, '_>, header: CommittedHeader<'_>) -> TestResult {
    for maximum in [3_105, 3_106] {
        let limits = PolicySchemaLimits::new(PolicyLimits::default(), maximum)?;
        let mut charge = PolicySchemaCharge::new(limits);
        let held = restore_complete_policy(
            1,
            CompletePolicyData {
                workflow_type: b"r".to_vec(),
                version: None,
                max_active_ms: None,
                retry: None,
                retention: None,
                states: BTreeMap::new(),
                indexed_attributes: Some(vec![b"a".to_vec()]),
                indexed_state_meta: None,
                governance: None,
            },
            limits,
            &mut charge,
        )?;
        assert_eq!((charge.g(), charge.p(), charge.total()), (1_928, 64, 1_992));
        assert_eq!(
            inspect_complete_core_policy_patch_point_gate(
                view,
                header,
                limits,
                AdmissionLimits::default(),
                &mut charge
            )?
            .decision(),
            GateDecision::Fresh
        );
        assert_eq!((charge.g(), charge.p(), charge.total()), (2_485, 64, 2_549));
        let result = inspect_complete_core_policy_patch_point_gate(
            view,
            header,
            limits,
            AdmissionLimits::default(),
            &mut charge,
        );
        if maximum == 3_106 {
            assert_eq!(result?.decision(), GateDecision::Fresh);
        } else {
            assert_eq!(
                result,
                Err(ChargedRequestGateError::Schema(
                    PolicySchemaError::WorkBytes {
                        bytes: 3_106,
                        maximum
                    }
                ))
            );
        }
        let g = if maximum == 3_106 { 3_042 } else { 2_907 };
        assert_eq!((charge.g(), charge.p(), charge.total()), (g, 64, g + 64));
        assert_eq!(held.generation(), 1);
        assert_eq!(held.data().indexed_attributes, Some(vec![b"a".to_vec()]));
        let other = PolicySchemaLimits::new(PolicyLimits::default(), 10_000)?;
        assert_eq!(
            inspect_complete_core_policy_patch_point_gate(
                view,
                header,
                other,
                AdmissionLimits::default(),
                &mut charge
            ),
            Err(ChargedRequestGateError::Schema(
                PolicySchemaError::ProfileMismatch
            ))
        );
        assert_eq!((charge.g(), charge.p(), charge.total()), (g, 64, g + 64));
    }
    Ok(())
}
