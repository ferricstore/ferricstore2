//! Pinned allocator recipes and original ETF output used as subsequent current.

use super::cases::{
    KEY, accepted, atom, big, binary, expected, frame, frame_with, list, log, map, policy,
    retained, safety, take, view,
};
use super::{
    AdmissionLimits, BTreeMap, EncodingError, Plan, PolicySchemaCharge, PolicySchemaLimits, Raw,
    ReadValue, Reason, Rejection, RetainedStatus, RetentionReason, TestResult, apply,
};
use wf_domain::policy::complete::decode_original_complete_policy_entry;
use wf_domain::policy::installation::{MAX_POLICY_GENERATION, PolicyError};
use wf_domain::workflow::request::gate::GateView;

pub(super) fn assert_source_sequence(gate: GateView<'_>, limits: PolicySchemaLimits) -> TestResult {
    // Freeze the source's generated positive unique suffix to1 in this local fixture.
    for guard in [Raw::Nil, Raw::Integer(1)] {
        assert_generation_sequence(gate, limits, guard)?;
    }
    Ok(())
}

fn assert_generation_sequence(
    gate: GateView<'_>,
    limits: PolicySchemaLimits,
    second_guard: Raw<'_>,
) -> TestResult {
    let ty = if matches!(second_guard, Raw::Nil) {
        b"generation-always-on-1".as_slice()
    } else {
        b"generation-cas-1".as_slice()
    };
    let key = policy_key(ty);
    let mut charge = PolicySchemaCharge::new(limits);
    let patch = [(Raw::Atom("max_active_ms"), Raw::Integer(1_000))];
    let first_bytes = expected(1, ty, &[("max_active_ms", number(1_000)?)])?;
    let first = allocator(
        gate,
        (ty, None),
        (Raw::Nil, Raw::Map(&patch)),
        (0, b"q"),
        limits,
        &mut charge,
    )?;
    accepted(&first, 0, 1, &key, &first_bytes)?;
    let (first, _) = take(first)?;
    let first = first.0.ok_or("first installed bytes")?;
    let patch = [(Raw::Atom("max_active_ms"), Raw::Integer(2_000))];
    let second_bytes = expected(2, ty, &[("max_active_ms", number(2_000)?)])?;
    let second = allocator(
        gate,
        (ty, Some(first.encoded())),
        (second_guard, Raw::Map(&patch)),
        (0, b"s"),
        limits,
        &mut charge,
    )?;
    accepted(&second, 1, 2, &key, &second_bytes)?;
    let (second, _) = take(second)?;
    let second = second.0.ok_or("second installed bytes")?;
    let patch = [(Raw::Atom("max_active_ms"), Raw::Integer(3_000))];
    let third = allocator(
        gate,
        (ty, Some(second.encoded())),
        (Raw::Integer(1), Raw::Map(&patch)),
        (0, b"t"),
        limits,
        &mut charge,
    )?;
    let out = retained(&third)?;
    assert!(
        matches!(out.status(), RetainedStatus::PolicyRejected(reason)
        if reason.error() == PolicyError::StaleGeneration)
    );
    assert_eq!(out.response(), b"ERR stale flow policy generation");
    assert!(matches!(
        third.decision(),
        super::DispatchDecision::Fresh((None, _))
    ));
    assert_eq!(first.encoded(), first_bytes);
    assert_eq!(second.encoded(), second_bytes);
    Ok(())
}

pub(super) fn assert_replace_reset(gate: GateView<'_>, limits: PolicySchemaLimits) -> TestResult {
    // Original17d first; ignored arbitrary stored data is a separate supplemental control.
    for extra in [false, true] {
        let ty = b"generation-replace-1";
        let key = policy_key(ty);
        let queued = map(&[(atom(b"mode")?, atom(b"fifo")?)])?;
        let mut fields = vec![
            ("max_active_ms", number(1_000)?),
            ("states", map(&[(binary(b"queued")?, queued)])?),
        ];
        if extra {
            fields.push(("unknown", binary(b"discarded")?));
        }
        let raw = expected(1, ty, &fields)?;
        let copy = raw.clone();
        let final_bytes = expected(2, ty, &[])?;
        let mut charge = PolicySchemaCharge::new(limits);
        let plan = allocator(
            gate,
            (ty, Some(&raw)),
            (Raw::Nil, Raw::Map(&[])),
            (0, b"q"),
            limits,
            &mut charge,
        )?;
        accepted(&plan, 1, 2, &key, &final_bytes)?;
        assert_eq!(raw, copy);
    }
    Ok(())
}

fn number(value: u64) -> TestResult<Vec<u8>> {
    super::cases::number(value)
}

fn policy_key(ty: &[u8]) -> Vec<u8> {
    let mut key = b"f:{f}:policy:".to_vec();
    key.extend_from_slice(ty);
    key
}

fn allocator<'view>(
    gate: GateView<'view>,
    current: (&[u8], Option<&[u8]>),
    command: (Raw<'_>, Raw<'_>),
    origin: (u64, &[u8]),
    limits: PolicySchemaLimits,
    charge: &mut PolicySchemaCharge,
) -> TestResult<Plan<'view>> {
    allocator_limit(gate, current, command, origin, (limits, 1_000_000), charge)
}

fn allocator_limit<'view>(
    gate: GateView<'view>,
    current: (&[u8], Option<&[u8]>),
    command: (Raw<'_>, Raw<'_>),
    origin: (u64, &[u8]),
    profile: (PolicySchemaLimits, u32),
    charge: &mut PolicySchemaCharge,
) -> TestResult<Plan<'view>> {
    let (limits, history_max_events_limit) = profile;
    let (ty, stored) = current;
    let policies = stored.map_or_else(BTreeMap::new, |raw| {
        BTreeMap::from([(ty.to_vec(), super::ReadValue::Binary(raw))])
    });
    let marks = BTreeMap::new();
    let key = policy_key(ty);
    let bytes = frame_with(&key, command.0, command.1, origin.1, origin.0)?;
    let mut supplied = view(gate, &policies, &marks, &marks, safety());
    supplied.history_max_events_limit = Ok(history_max_events_limit);
    let plan = apply(
        &supplied,
        &bytes,
        0,
        limits,
        AdmissionLimits::default(),
        charge,
    )?;
    let out = retained(&plan)?;
    assert_eq!(out.scope(), gate.scope);
    assert_eq!(out.request().as_bytes(), origin.1);
    assert_eq!(out.intent().version(), 3);
    assert_eq!(out.intent().digest().as_slice(), &bytes[29..61]);
    assert_eq!(out.original_position(), log(0));
    assert_eq!(out.original_time().get(), origin.0);
    assert_eq!(out.expires_at().get(), origin.0 + 100);
    assert_eq!(out.response_version(), 1);
    Ok(plan)
}

pub(super) fn assert_allocator_recipes(
    gate: GateView<'_>,
    limits: PolicySchemaLimits,
) -> TestResult {
    assert_cross_policy_recipe(gate, limits)?;
    assert_target_highwater_recipe(gate, limits)?;
    assert_invalid_generation_recipe(gate, limits)?;
    assert_opaque_policy_key(gate, limits)?;
    assert_replicated_limit_recipe(gate, limits)?;
    Ok(())
}

fn assert_replicated_limit_recipe(gate: GateView<'_>, limits: PolicySchemaLimits) -> TestResult {
    // Source639-692 uses the SAME history50 under captured C100 and C10.
    // Global application environment and physical ETS observations remain unported.
    let retention = [(Raw::Atom("history_max_events"), Raw::Integer(50))];
    let patch = [(Raw::Atom("retention"), Raw::Map(&retention))];
    let accepted_type = b"replicated-policy-limit-accepted";
    let final_bytes = expected(
        1,
        accepted_type,
        &[(
            "retention",
            map(&[(atom(b"history_max_events")?, number(50)?)])?,
        )],
    )?;
    let mut charge = PolicySchemaCharge::new(limits);
    let first = allocator_limit(
        gate,
        (accepted_type, None),
        (Raw::Nil, Raw::Map(&patch)),
        (1_000, b"q"),
        (limits, 100),
        &mut charge,
    )?;
    accepted(&first, 0, 1, &policy_key(accepted_type), &final_bytes)?;
    let rejected_type = b"replicated-policy-limit-rejected";
    let next = allocator_limit(
        gate,
        (rejected_type, None),
        (Raw::Nil, Raw::Map(&patch)),
        (2_000, b"r"),
        (limits, 10),
        &mut charge,
    )?;
    assert!(matches!(
        next.decision(),
        super::DispatchDecision::Fresh((None, _))
    ));
    let out = retained(&next)?;
    assert_eq!(
        out.status(),
        RetainedStatus::OriginalFlowPolicyReplaceRejected(Rejection::Input(Reason::Retention(
            RetentionReason::History { maximum: 10 }
        )))
    );
    let body = b"ERR flow retention history_max_events must be between 1 and 10";
    assert_eq!(body.len(), 62);
    assert_eq!(out.response(), body);
    accepted(&first, 0, 1, &policy_key(accepted_type), &final_bytes)?;
    Ok(())
}

fn assert_opaque_policy_key(gate: GateView<'_>, limits: PolicySchemaLimits) -> TestResult {
    let ty = b"\xff\0";
    let key = policy_key(ty);
    let first_bytes = expected(1, ty, &[])?;
    let next_bytes = expected(2, ty, &[])?;
    let mut charge = PolicySchemaCharge::new(limits);
    let first = allocator(
        gate,
        (ty, None),
        (Raw::Nil, Raw::Map(&[])),
        (0, b"q"),
        limits,
        &mut charge,
    )?;
    accepted(&first, 0, 1, &key, &first_bytes)?;
    let (first, _) = take(first)?;
    let first = first.0.ok_or("opaque binary type output")?;
    let next = allocator(
        gate,
        (ty, Some(first.encoded())),
        (Raw::Integer(1), Raw::Map(&[])),
        (0, b"s"),
        limits,
        &mut charge,
    )?;
    accepted(&next, 1, 2, &key, &next_bytes)?;
    assert_eq!(first.encoded(), first_bytes);
    Ok(())
}

fn assert_cross_policy_recipe(gate: GateView<'_>, limits: PolicySchemaLimits) -> TestResult {
    //470ba: the source passes ALREADY normalized maps and the actual replicated times.
    let ty = b"current-cross-policy-wire";
    let key = policy_key(ty);
    let initial = [
        (Raw::Atom("max_active_ms"), Raw::Integer(1_000)),
        (Raw::Atom("states"), Raw::Map(&[])),
        (Raw::Atom("type"), Raw::Binary(ty)),
    ];
    let first_bytes = expected(1, ty, &[("max_active_ms", number(1_000)?)])?;
    let mut charge = PolicySchemaCharge::new(limits);
    let first = allocator(
        gate,
        (ty, None),
        (Raw::Nil, Raw::Map(&initial)),
        (1_000, b"q"),
        limits,
        &mut charge,
    )?;
    accepted(&first, 0, 1, &key, &first_bytes)?;
    let (first, _) = take(first)?;
    let first = first.0.ok_or("original cross-policy install")?;
    let beta = b"beta-cross-policy-wire";
    let beta_policy = policy(beta, &[("max_active_ms", number(2_000)?)])?;
    let mut beta_input = vec![131, 104, 2];
    beta_input.extend_from_slice(&atom(b"flow_policy_v1")?);
    beta_input.extend_from_slice(&beta_policy);
    let bad = allocator(
        gate,
        (beta, None),
        (Raw::Nil, Raw::Binary(&beta_input)),
        (2_000, b"b"),
        limits,
        &mut charge,
    )?;
    assert!(matches!(
        bad.decision(),
        super::DispatchDecision::Fresh((None, _))
    ));
    assert_eq!(
        retained(&bad)?.status(),
        RetainedStatus::OriginalFlowPolicyReplaceRejected(Rejection::PatchShape)
    );
    assert_eq!(retained(&bad)?.response(), b"ERR invalid flow policy patch");
    let updated = [
        (Raw::Atom("max_active_ms"), Raw::Integer(3_000)),
        (Raw::Atom("states"), Raw::Map(&[])),
        (Raw::Atom("type"), Raw::Binary(ty)),
    ];
    let updated_bytes = expected(2, ty, &[("max_active_ms", number(3_000)?)])?;
    let next = allocator(
        gate,
        (ty, Some(first.encoded())),
        (Raw::Nil, Raw::Map(&updated)),
        (3_000, b"u"),
        limits,
        &mut charge,
    )?;
    accepted(&next, 1, 2, &key, &updated_bytes)?;
    assert_eq!(first.encoded(), first_bytes);
    Ok(())
}

fn assert_target_highwater_recipe(gate: GateView<'_>, limits: PolicySchemaLimits) -> TestResult {
    //006b: physical put/stale nonoverwrite remain separate required observations.
    let ty = b"target-high-water-policy";
    let key = policy_key(ty);
    let stored = expected(3, ty, &[("max_active_ms", number(1_000)?)])?;
    let patch = [
        (Raw::Atom("max_active_ms"), Raw::Integer(2_000)),
        (Raw::Atom("states"), Raw::Map(&[])),
        (Raw::Atom("type"), Raw::Binary(ty)),
    ];
    let next_bytes = expected(4, ty, &[("max_active_ms", number(2_000)?)])?;
    let mut charge = PolicySchemaCharge::new(limits);
    let plan = allocator(
        gate,
        (ty, Some(&stored)),
        (Raw::Nil, Raw::Map(&patch)),
        (2_000, b"q"),
        limits,
        &mut charge,
    )?;
    accepted(&plan, 3, 4, &key, &next_bytes)?;
    assert_eq!(
        stored,
        expected(3, ty, &[("max_active_ms", number(1_000)?)])?
    );
    Ok(())
}

fn assert_invalid_generation_recipe(gate: GateView<'_>, limits: PolicySchemaLimits) -> TestResult {
    //889's error is a source pattern observation, not a new direct-ledger assertion.
    let ty = b"invalid-expected-policy-generation";
    let patch = [
        (Raw::Atom("max_active_ms"), Raw::Integer(1_000)),
        (Raw::Atom("states"), Raw::Map(&[])),
        (Raw::Atom("type"), Raw::Binary(ty)),
    ];
    let mut charge = PolicySchemaCharge::new(limits);
    let plan = allocator(
        gate,
        (ty, None),
        (
            Raw::Integer(i128::from(MAX_POLICY_GENERATION) + 1),
            Raw::Map(&patch),
        ),
        (1_000, b"q"),
        limits,
        &mut charge,
    )?;
    assert!(matches!(
        plan.decision(),
        super::DispatchDecision::Fresh((None, _))
    ));
    assert!(
        matches!(retained(&plan)?.status(), RetainedStatus::PolicyRejected(reason)
        if reason.error() == PolicyError::InvalidGeneration)
    );
    assert_eq!(
        retained(&plan)?.response(),
        b"ERR invalid flow policy generation"
    );
    Ok(())
}

pub(super) fn assert_wide_versions(gate: GateView<'_>, limits: PolicySchemaLimits) -> TestResult {
    let marks = BTreeMap::new();
    let empty = BTreeMap::new();
    for width in [9, 255, 256] {
        let mut digits = vec![0; width];
        digits[width - 1] = 1;
        let mut padded = digits.clone();
        padded.extend_from_slice(&[0, 0]);
        let patch = [(
            Raw::Atom("version"),
            Raw::BigInteger {
                negative: false,
                little_endian: &padded,
            },
        )];
        let bytes = frame(Raw::Nil, Raw::Map(&patch))?;
        let expected1 = expected(1, b"r", &[("version", big(&digits)?)])?;
        let expected2 = expected(2, b"r", &[("version", big(&digits)?)])?;
        assert_eq!(
            expected1.len(),
            if width == 9 {
                72
            } else {
                width + if width == 255 { 63 } else { 66 }
            }
        );
        let mut charge = PolicySchemaCharge::new(limits);
        let first = apply(
            &view(gate, &empty, &marks, &marks, safety()),
            &bytes,
            0,
            limits,
            AdmissionLimits::default(),
            &mut charge,
        )?;
        accepted(&first, 0, 1, KEY, &expected1)?;
        let (first, _) = take(first)?;
        let first = first.0.ok_or("wide first")?;
        let policies = BTreeMap::from([(b"r".to_vec(), ReadValue::Binary(first.encoded()))]);
        let second = apply(
            &view(gate, &policies, &marks, &marks, safety()),
            &bytes,
            0,
            limits,
            AdmissionLimits::default(),
            &mut charge,
        )?;
        accepted(&second, 1, 2, KEY, &expected2)?;
        assert_eq!(first.encoded(), expected1);
    }
    Ok(())
}

pub(super) fn assert_own_output(gate: GateView<'_>, limits: PolicySchemaLimits) -> TestResult {
    let names = (0..1_000)
        .map(|i| format!("x{i:03}").into_bytes())
        .collect::<Vec<_>>();
    let values = names
        .iter()
        .map(|name| Raw::Binary(name))
        .collect::<Vec<_>>();
    let effects = [
        (Raw::Atom("allowed"), Raw::List(&values)),
        (Raw::Atom("denied"), Raw::List(&values)),
    ];
    let approvals = [
        (Raw::Atom("assignees"), Raw::List(&values)),
        (Raw::Atom("effects"), Raw::List(&values)),
        (Raw::Atom("states"), Raw::List(&values)),
    ];
    let governance = [
        (Raw::Atom("approvals"), Raw::Map(&approvals)),
        (Raw::Atom("effects"), Raw::Map(&effects)),
    ];
    let patch = [(Raw::Atom("governance"), Raw::Map(&governance))];
    let leaf_list = list(&names)?;
    let governance = map(&[
        (
            atom(b"approvals")?,
            map(&[
                (atom(b"assignees")?, leaf_list.clone()),
                (atom(b"effects")?, leaf_list.clone()),
                (atom(b"states")?, leaf_list.clone()),
            ])?,
        ),
        (
            atom(b"effects")?,
            map(&[
                (atom(b"allowed")?, leaf_list.clone()),
                (atom(b"denied")?, leaf_list),
            ])?,
        ),
    ])?;
    let expected1 = expected(1, b"r", &[("governance", governance.clone())])?;
    let expected2 = expected(2, b"r", &[("governance", governance)])?;
    assert_eq!((expected1.len(), expected2.len()), (45_173, 45_173));
    let bytes = frame(Raw::Nil, Raw::Map(&patch))?;
    let empty = BTreeMap::new();
    let marks = BTreeMap::new();
    let mut charge = PolicySchemaCharge::new(limits);
    let first = apply(
        &view(gate, &empty, &marks, &marks, safety()),
        &bytes,
        0,
        limits,
        AdmissionLimits::default(),
        &mut charge,
    )?;
    accepted(&first, 0, 1, KEY, &expected1)?;
    let (first, _) = take(first)?;
    let first = first.0.ok_or("large original output")?;
    let mut public_charge = PolicySchemaCharge::new(limits);
    assert_eq!(
        decode_original_complete_policy_entry(
            first.encoded(),
            &safety(),
            limits,
            &mut public_charge
        )
        .err(),
        Some(EncodingError::ItemLimit)
    );
    let policies = BTreeMap::from([(b"r".to_vec(), ReadValue::Binary(first.encoded()))]);
    let second = apply(
        &view(gate, &policies, &marks, &marks, safety()),
        &bytes,
        0,
        limits,
        AdmissionLimits::default(),
        &mut charge,
    )?;
    accepted(&second, 1, 2, KEY, &expected2)?;
    assert_eq!(first.encoded(), expected1);
    assert!(charge.p() >= 322_496);
    Ok(())
}
