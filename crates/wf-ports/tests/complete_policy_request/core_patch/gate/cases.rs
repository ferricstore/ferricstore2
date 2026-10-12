//! Decision, authority priority and continuous-account scenarios for the gate.

use std::collections::BTreeMap;

use sha2::{Digest, Sha256};
use wf_domain::policy::complete::{
    PolicySchemaCharge, PolicySchemaError, PolicySchemaLimits, restore_complete_policy,
};
use wf_domain::policy::installation::PolicyError;
use wf_domain::workflow::admission::AdmissionLimits;
use wf_domain::workflow::batch::ApplyError;
use wf_domain::workflow::control::{ControlState, EntryIdentity, EntryStamp, PreparedDigest};
use wf_domain::workflow::request::gate::{
    ChargedRequestGateError, GateDecision, GateView, inspect_complete_core_policy_patch_gate,
    inspect_request,
};
use wf_domain::workflow::request::{ClientIntent, PolicyRejection, RetainedStatus};
use wf_ports::log::verify_complete_core_policy_patch_frame;
use wf_types::identity::RequestId;
use wf_types::version::{AppliedPosition, LogIdentity};

use super::super::super::{TestResult, minimal_data, profile};
use super::super::prepare::cases::{core_patch_prepare_literal_frame, decode_core_patch_literal};
use super::super::prepare::literals::FROZEN_CORE_PATCH_FRAMES;
use super::{
    assert_core_patch_gate_frame, assert_core_patch_gate_replay, core_patch_gate_log,
    core_patch_gate_retained, core_patch_gate_scope, core_patch_gate_stamp,
};

pub(super) const CORE_PATCH_GATE_PHASES: [(u64, u64); 9] = [
    (4_551, 4_423),
    (4_685, 4_551),
    (4_813, 4_685),
    (4_942, 4_813),
    (5_076, 4_942),
    (5_206, 5_076),
    (5_334, 5_206),
    (5_530, 5_334),
    (5_658, 5_530),
];

pub(super) fn assert_core_patch_gate_control_decisions(
    limits: PolicySchemaLimits,
    charge: &mut PolicySchemaCharge,
) -> TestResult {
    let encoded = decode_core_patch_literal(FROZEN_CORE_PATCH_FRAMES[0].0)?;
    let scope = core_patch_gate_scope([b"t", b"g", b"r"])?;
    let stamp = core_patch_gate_stamp(0, 0, 0)?;
    let admission = AdmissionLimits::default();
    let empty = BTreeMap::new();
    let before = charge.g();
    let verified = verify_complete_core_policy_patch_frame(&encoded, limits, admission, charge)?;
    let header = verified.header(core_patch_gate_log(0));
    let view = GateView {
        scope: &scope,
        control: ControlState::initial(stamp),
        outcomes: &empty,
    };
    let fresh = inspect_complete_core_policy_patch_gate(view, header, limits, admission, charge)?;
    assert_eq!(fresh, inspect_request(view, header, admission)?);
    assert_eq!(fresh.decision(), GateDecision::Fresh);
    assert!(!fresh.control().is_log_replay());
    assert_eq!(fresh.control().next().last_entry(), Some(header.identity));
    assert_eq!(fresh.control().next().position().get(), 0);
    assert_eq!(fresh.control().next().time(), stamp);
    assert_eq!(fresh.control().execution_time().get(), 0);
    assert_eq!(view.control.last_entry(), None);
    assert!(empty.is_empty());
    assert_core_patch_gate_frame(&verified, &encoded, FROZEN_CORE_PATCH_FRAMES[0].1, stamp)?;
    assert_eq!((charge.g(), charge.p()), (before + 4_942, 0));
    let outcomes = BTreeMap::from([(
        RequestId::new(b"q".to_vec())?,
        core_patch_gate_retained(header.intent, 3, RetainedStatus::Accepted)?,
    )]);
    let previous = fresh.control().next();
    let before = charge.g();
    let verified_again =
        verify_complete_core_policy_patch_frame(&encoded, limits, admission, charge)?;
    let view = GateView {
        scope: &scope,
        control: previous,
        outcomes: &outcomes,
    };
    let header = verified_again.header(core_patch_gate_log(0));
    let replay = inspect_complete_core_policy_patch_gate(view, header, limits, admission, charge)?;
    assert_eq!(replay, inspect_request(view, header, admission)?);
    assert_eq!(replay.decision(), GateDecision::LogReplay);
    assert!(replay.control().is_log_replay());
    assert_eq!(replay.control().next(), previous);
    assert_core_patch_gate_frame(
        &verified_again,
        &encoded,
        FROZEN_CORE_PATCH_FRAMES[0].1,
        stamp,
    )?;
    assert_eq!((charge.g(), charge.p()), (before + 4_813, 0));
    let retained = outcomes
        .get(&RequestId::new(b"q".to_vec())?)
        .ok_or("missing deliberately future-position retained fixture")?;
    assert_eq!(retained.original_position(), core_patch_gate_log(1));
    assert_eq!(retained.original_time().get(), 0);
    assert_eq!(retained.expires_at().get(), 100);
    assert_eq!(retained.response(), b"accepted-original\0\xff");
    assert_eq!(retained.status(), RetainedStatus::Accepted);
    Ok(())
}

pub(super) fn assert_core_patch_gate_retained_decisions(
    limits: PolicySchemaLimits,
    charge: &mut PolicySchemaCharge,
) -> TestResult {
    let minimal = decode_core_patch_literal(FROZEN_CORE_PATCH_FRAMES[0].0)?;
    let guarded = decode_core_patch_literal(FROZEN_CORE_PATCH_FRAMES[1].0)?;
    let scope = core_patch_gate_scope([b"t", b"g", b"r"])?;
    let stamp = core_patch_gate_stamp(0, 0, 0)?;
    let admission = AdmissionLimits::default();
    let original = verify_complete_core_policy_patch_frame(&minimal, limits, admission, charge)?;
    assert_core_patch_gate_frame(&original, &minimal, FROZEN_CORE_PATCH_FRAMES[0].1, stamp)?;
    let original_header = original.header(core_patch_gate_log(0));
    let previous =
        ControlState::restored(stamp, original_header.identity, EntryStamp::Stamped(stamp))?;
    for status in [
        RetainedStatus::Accepted,
        RetainedStatus::PolicyRejected(PolicyRejection::new(PolicyError::InvalidStateMode)?),
    ] {
        let outcomes = BTreeMap::from([(
            RequestId::new(b"q".to_vec())?,
            core_patch_gate_retained(original_header.intent, 0, status)?,
        )]);
        for (case, hash, verifier_fee, gate_fee) in [
            (0, FROZEN_CORE_PATCH_FRAMES[0].1, 4_423, 1_235),
            (
                1,
                "0a4203c8bd713947ce012d2ae6c08edbb1897e12db7797547e1cf0450452a692",
                4_575,
                1_107,
            ),
            (
                2,
                "b6e78e042599efcb904e86cacdc0852d34be0263da46078af8b841362e30d7f2",
                4_423,
                1_235,
            ),
        ] {
            let incoming_stamp = core_patch_gate_stamp(
                if case == 0 { 0 } else { 100 },
                0,
                if case == 0 { 0 } else { 100 },
            )?;
            let intent = if case == 1 {
                &guarded[83..]
            } else {
                &minimal[83..]
            };
            let encoded =
                core_patch_prepare_literal_frame(intent, [b"t", b"g", b"r", b"q"], incoming_stamp)?;
            assert_eq!(
                <[u8; 32]>::from(Sha256::digest(&encoded)).as_slice(),
                decode_core_patch_literal(hash)?.as_slice()
            );
            assert_core_patch_gate_retained_case(
                GateView {
                    scope: &scope,
                    control: previous,
                    outcomes: &outcomes,
                },
                &encoded,
                status,
                incoming_stamp,
                (case, hash, verifier_fee, gate_fee),
                limits,
                charge,
            )?;
        }
    }
    Ok(())
}

pub(super) fn assert_core_patch_gate_header_priority(
    limits: PolicySchemaLimits,
    charge: &mut PolicySchemaCharge,
) -> TestResult {
    let encoded = decode_core_patch_literal(FROZEN_CORE_PATCH_FRAMES[0].0)?;
    let scope = core_patch_gate_scope([b"t", b"g", b"r"])?;
    let foreign = core_patch_gate_scope([b"u", b"g", b"r"])?;
    let stamp = core_patch_gate_stamp(0, 0, 0)?;
    let oversized_scope = vec![0; 8_193];
    for (case, expected, fee) in [
        (0, ApplyError::InvalidAuthority, 128),
        (1, ApplyError::InvalidAuthority, 262),
        (2, ApplyError::InvalidAuthority, 262),
        (3, ApplyError::InvalidAuthority, 262),
        (4, ApplyError::InvalidAuthority, 262),
        (5, ApplyError::PositionConflict, 390),
        (6, ApplyError::PositionConflict, 390),
        (7, ApplyError::PositionConflict, 390),
        (8, ApplyError::PositionConflict, 390),
    ] {
        let before = charge.g();
        let verified = verify_complete_core_policy_patch_frame(
            &encoded,
            limits,
            AdmissionLimits::default(),
            charge,
        )?;
        let original = verified.header(core_patch_gate_log(0));
        let mut previous =
            ControlState::restored(stamp, original.identity, EntryStamp::Stamped(stamp))?;
        let mut header = verified.header(core_patch_gate_log(1));
        let mut outcomes = BTreeMap::new();
        let admission = if case == 2 {
            AdmissionLimits::new(1_048_576, 134, 1_000)?
        } else {
            AdmissionLimits::default()
        };
        if case == 4 {
            previous = ControlState::initial(stamp);
            outcomes.insert(
                RequestId::new(b"q".to_vec())?,
                core_patch_gate_retained(original.intent, 0, RetainedStatus::Accepted)?,
            );
        }
        core_patch_gate_mutate_header(&mut header, case, &oversized_scope)?;
        let view = GateView {
            scope: if case == 1 { &foreign } else { &scope },
            control: previous,
            outcomes: &outcomes,
        };
        assert_eq!(
            inspect_request(view, header, admission)
                .err()
                .ok_or("expected legacy authority refusal")?,
            expected
        );
        assert_eq!(
            inspect_complete_core_policy_patch_gate(view, header, limits, admission, charge,)
                .err()
                .ok_or("expected same charged authority refusal")?,
            ChargedRequestGateError::Apply(expected)
        );
        assert_eq!((charge.g(), charge.p()), (before + 4_423 + fee, 0));
        assert_eq!(view.control, previous);
        assert_eq!(outcomes.len(), usize::from(case == 4));
        assert_core_patch_gate_frame(&verified, &encoded, FROZEN_CORE_PATCH_FRAMES[0].1, stamp)?;
    }
    Ok(())
}

pub(super) fn assert_core_patch_gate_retained_priority(
    limits: PolicySchemaLimits,
    charge: &mut PolicySchemaCharge,
) -> TestResult {
    let encoded = decode_core_patch_literal(FROZEN_CORE_PATCH_FRAMES[0].0)?;
    let scope = core_patch_gate_scope([b"t", b"g", b"r"])?;
    let stamp = core_patch_gate_stamp(0, 0, 0)?;
    for (case, fee) in [(1, 653), (2, 783), (3, 911), (4, 911), (5, 911)] {
        let before = charge.g();
        let verified = verify_complete_core_policy_patch_frame(
            &encoded,
            limits,
            AdmissionLimits::default(),
            charge,
        )?;
        let header = verified.header(core_patch_gate_log(1));
        let previous = ControlState::restored(
            stamp,
            verified.header(core_patch_gate_log(0)).identity,
            EntryStamp::Stamped(stamp),
        )?;
        // The different intent cannot mask any earlier retained authority error.
        let outcomes = BTreeMap::from([(
            RequestId::new(b"q".to_vec())?,
            core_patch_gate_retained(
                ClientIntent::new(1, [0; 32])?,
                case,
                RetainedStatus::Accepted,
            )?,
        )]);
        let admission = AdmissionLimits::new(1_048_576, 135, 1_000)?;
        let view = GateView {
            scope: &scope,
            control: previous,
            outcomes: &outcomes,
        };
        assert_eq!(
            inspect_request(view, header, admission)
                .err()
                .ok_or("expected legacy corrupt-retained refusal")?,
            ApplyError::InvalidAuthority
        );
        assert_eq!(
            inspect_complete_core_policy_patch_gate(view, header, limits, admission, charge,)
                .err()
                .ok_or("expected retained authority before intent conflict")?,
            ChargedRequestGateError::Apply(ApplyError::InvalidAuthority)
        );
        assert_eq!((charge.g(), charge.p()), (before + 4_423 + fee, 0));
        assert_eq!(previous.time(), stamp);
        assert_eq!(outcomes.len(), 1);
        let retained = outcomes
            .get(&RequestId::new(b"q".to_vec())?)
            .ok_or("missing unchanged corrupt fixture")?;
        assert_eq!(
            retained.original_position(),
            core_patch_gate_log(u64::from(case == 3))
        );
        assert_eq!(retained.original_time().get(), u64::from(case == 4));
        assert_eq!(retained.expires_at().get(), 100);
        assert_eq!(retained.response().len(), if case == 5 { 136 } else { 19 });
        assert_eq!(retained.status(), RetainedStatus::Accepted);
        assert_eq!(retained.response_version(), 1);
        if case == 5 {
            assert!(retained.response().iter().all(|&byte| byte == 0));
        } else {
            assert_eq!(retained.response(), b"accepted-original\0\xff");
        }
        assert_core_patch_gate_frame(&verified, &encoded, FROZEN_CORE_PATCH_FRAMES[0].1, stamp)?;
    }
    Ok(())
}

pub(super) fn assert_core_patch_gate_account(
    maximum: u64,
    target: u64,
    attempts: u64,
    peak: bool,
) -> TestResult {
    let limits = profile(maximum)?;
    let mut charge = PolicySchemaCharge::new(limits);
    let mut expected_data = minimal_data();
    expected_data.indexed_attributes = Some(vec![b"a".to_vec()]);
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
    let encoded = decode_core_patch_literal(FROZEN_CORE_PATCH_FRAMES[0].0)?;
    let scope = core_patch_gate_scope([b"t", b"g", b"r"])?;
    let stamp = core_patch_gate_stamp(0, 0, 0)?;
    let outcomes = BTreeMap::new();
    let previous = ControlState::initial(stamp);
    let view = GateView {
        scope: &scope,
        control: previous,
        outcomes: &outcomes,
    };
    let mut owners = Vec::new();
    for attempt in 1..=attempts {
        let verified = verify_complete_core_policy_patch_frame(
            &encoded,
            limits,
            AdmissionLimits::default(),
            &mut charge,
        )?;
        let header = verified.header(core_patch_gate_log(0));
        let result = inspect_complete_core_policy_patch_gate(
            view,
            header,
            limits,
            AdmissionLimits::default(),
            &mut charge,
        );
        let refused = attempt == attempts && maximum < target;
        let g = setup + attempt * 4_942 - if refused { 129 } else { 0 };
        assert_core_patch_gate_account_result(
            result,
            view,
            header,
            refused.then_some((target, maximum)),
            (g, scratch),
            &charge,
        )?;
        assert_core_patch_gate_frame(&verified, &encoded, FROZEN_CORE_PATCH_FRAMES[0].1, stamp)?;
        assert_eq!(view.control, previous);
        assert!(outcomes.is_empty());
        if let Some(held) = &held {
            assert_eq!(held.generation(), 1);
            assert!(held.data() == &expected_data);
        }
        owners.push(verified);
        assert_eq!(u64::try_from(owners.len())?, attempt);
        for owner in &owners {
            assert_core_patch_gate_frame(owner, &encoded, FROZEN_CORE_PATCH_FRAMES[0].1, stamp)?;
        }
    }
    Ok(())
}

fn assert_core_patch_gate_retained_case(
    view: GateView<'_>,
    encoded: &[u8],
    status: RetainedStatus,
    stamp: wf_types::version::CommittedTime,
    case: (usize, &str, u64, u64),
    limits: PolicySchemaLimits,
    charge: &mut PolicySchemaCharge,
) -> TestResult {
    let (case, hash, verifier_fee, gate_fee) = case;
    let admission = AdmissionLimits::default();
    let before = charge.g();
    let verified = verify_complete_core_policy_patch_frame(encoded, limits, admission, charge)?;
    let header = verified.header(core_patch_gate_log(1));
    let gate = inspect_complete_core_policy_patch_gate(view, header, limits, admission, charge)?;
    assert_eq!(gate, inspect_request(view, header, admission)?);
    let retained = view
        .outcomes
        .get(&RequestId::new(b"q".to_vec())?)
        .ok_or("missing original retained fixture")?;
    match case {
        0 => assert_core_patch_gate_replay(gate, retained, status, stamp, header.identity),
        1 => assert_eq!(gate.decision(), GateDecision::IntentConflict),
        _ => assert_eq!(gate.decision(), GateDecision::Expired),
    }
    assert!(!gate.control().is_log_replay());
    assert_eq!(gate.control().next().last_entry(), Some(header.identity));
    assert_eq!(gate.control().next().position().get(), 1);
    assert_eq!(gate.control().next().time(), stamp);
    assert_eq!(gate.control().execution_time(), stamp.hlc().physical());
    assert_eq!(retained.original_position(), core_patch_gate_log(0));
    assert_eq!(retained.original_time().get(), 0);
    assert_eq!(retained.expires_at().get(), 100);
    assert_eq!(retained.status(), status);
    assert_eq!(retained.response_version(), 1);
    assert_eq!(
        retained.response(),
        if status == RetainedStatus::Accepted {
            &b"accepted-original\0\xff"[..]
        } else {
            &b"ERR flow state mode must be parallel or fifo"[..]
        }
    );
    assert_core_patch_gate_frame(&verified, encoded, hash, stamp)?;
    assert_eq!(
        (charge.g(), charge.p()),
        (before + verifier_fee + gate_fee, 0)
    );
    assert_eq!(view.control.time(), core_patch_gate_stamp(0, 0, 0)?);
    assert_eq!(view.outcomes.len(), 1);
    if case == 0 {
        let GateDecision::Replay(replayed) = gate.decision() else {
            return Err("expected original retained owner".into());
        };
        assert!(std::ptr::eq(replayed, retained));
        assert_eq!(replayed.response().as_ptr(), retained.response().as_ptr());
    }
    Ok(())
}

fn core_patch_gate_mutate_header<'a>(
    header: &mut wf_domain::workflow::request::gate::CommittedHeader<'a>,
    case: usize,
    oversized_scope: &'a [u8],
) -> TestResult {
    let prepared = header.identity.prepared();
    match case {
        0 => header.format_version = 2,
        3 => header.encoded_isolation_scope = oversized_scope,
        5 => header.identity = EntryIdentity::new(core_patch_gate_log(2), prepared),
        6 => {
            let mut digest = *prepared.as_bytes();
            digest[0] ^= 1;
            header.identity =
                EntryIdentity::new(core_patch_gate_log(0), PreparedDigest::new(digest));
        }
        7 => {
            header.identity = EntryIdentity::new(core_patch_gate_log(0), prepared);
            header.stamp = core_patch_gate_stamp(1, 0, 1)?;
        }
        8 => {
            header.identity =
                EntryIdentity::new(LogIdentity::new(0, 7, AppliedPosition::new(1)), prepared);
        }
        _ => {}
    }
    Ok(())
}

fn assert_core_patch_gate_account_result(
    result: Result<wf_domain::workflow::request::gate::RequestGate<'_>, ChargedRequestGateError>,
    view: GateView<'_>,
    header: wf_domain::workflow::request::gate::CommittedHeader<'_>,
    expected_error: Option<(u64, u64)>,
    expected_account: (u64, u64),
    charge: &PolicySchemaCharge,
) -> TestResult {
    if let Some((target, maximum)) = expected_error {
        assert_eq!(
            result
                .err()
                .ok_or("expected final query lookup debit refusal")?,
            ChargedRequestGateError::Schema(PolicySchemaError::WorkBytes {
                bytes: target,
                maximum,
            })
        );
    } else {
        let gate = result?;
        assert_eq!(gate.decision(), GateDecision::Fresh);
        assert!(!gate.control().is_log_replay());
        assert_eq!(gate.control().next().last_entry(), Some(header.identity));
        assert_eq!(gate.control().next().position().get(), 0);
        assert_eq!(
            gate.control().next().time(),
            core_patch_gate_stamp(0, 0, 0)?
        );
        assert_eq!(gate.control().execution_time().get(), 0);
        assert_eq!(
            gate,
            inspect_request(view, header, AdmissionLimits::default())?
        );
    }
    let (g, scratch) = expected_account;
    assert_eq!(
        (charge.g(), charge.p(), charge.total()),
        (g, scratch, g + scratch)
    );
    Ok(())
}
