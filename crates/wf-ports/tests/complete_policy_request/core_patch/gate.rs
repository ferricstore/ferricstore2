//! Header-only charged gate controls with immutable original frame oracles.

use std::collections::BTreeMap;

use sha2::{Digest, Sha256};

use wf_domain::policy::complete::{PolicySchemaCharge, PolicySchemaError};
use wf_domain::policy::installation::PolicyError;
use wf_domain::workflow::admission::AdmissionLimits;
use wf_domain::workflow::control::{ControlState, EntryIdentity, EntryStamp, PreparedDigest};
use wf_domain::workflow::request::gate::{
    ChargedRequestGateError, GateDecision, GateView, RequestGate,
    inspect_complete_core_policy_patch_gate, inspect_request,
};
use wf_domain::workflow::request::{
    ClientIntent, PolicyRejection, RequestScope, RetainedMetadata, RetainedOutcome, RetainedStatus,
};
use wf_ports::log::{
    VerifiedCompleteCorePolicyPatchFrame, verify_complete_core_policy_patch_frame,
};
use wf_types::identity::{GroupId, RecoveryId, RequestId, TenantId};
use wf_types::version::{
    AppliedPosition, CommittedTime, HlcTimestamp, LogIdentity, TimestampMillis,
};

use super::super::{TestResult, profile};
use super::prepare::cases::decode_core_patch_literal;
use super::prepare::literals::FROZEN_CORE_PATCH_FRAMES;

mod cases;

use cases::{
    CORE_PATCH_GATE_PHASES, assert_core_patch_gate_account,
    assert_core_patch_gate_control_decisions, assert_core_patch_gate_header_priority,
    assert_core_patch_gate_retained_decisions, assert_core_patch_gate_retained_priority,
};

#[test]
fn core_patch_gate_outlives_temporary_verified_header() -> TestResult {
    let limits = profile(20_000_000)?;
    let admission = AdmissionLimits::default();
    let mut charge = PolicySchemaCharge::new(limits);
    let encoded = decode_core_patch_literal(FROZEN_CORE_PATCH_FRAMES[0].0)?;
    let scope = core_patch_gate_scope([b"t", b"g", b"r"])?;
    let stamp = core_patch_gate_stamp(0, 0, 0)?;
    let status =
        RetainedStatus::PolicyRejected(PolicyRejection::new(PolicyError::InvalidStateMode)?);
    let mut outcomes = BTreeMap::new();
    let (gate, legacy, previous, identity) = {
        let verified =
            verify_complete_core_policy_patch_frame(&encoded, limits, admission, &mut charge)?;
        let original = verified.header(core_patch_gate_log(0));
        let previous =
            ControlState::restored(stamp, original.identity, EntryStamp::Stamped(stamp))?;
        outcomes.insert(
            RequestId::new(b"q".to_vec())?,
            core_patch_gate_retained(original.intent, 0, status)?,
        );
        let header = verified.header(core_patch_gate_log(1));
        let view = GateView {
            scope: &scope,
            control: previous,
            outcomes: &outcomes,
        };
        let gate =
            inspect_complete_core_policy_patch_gate(view, header, limits, admission, &mut charge)?;
        let legacy = inspect_request(view, header, admission)?;
        assert_core_patch_gate_frame(&verified, &encoded, FROZEN_CORE_PATCH_FRAMES[0].1, stamp)?;
        (gate, legacy, previous, header.identity)
    };
    assert_eq!(gate, legacy);
    assert_eq!(identity.log(), core_patch_gate_log(1));
    let retained = outcomes
        .get(&RequestId::new(b"q".to_vec())?)
        .ok_or("missing retained fixture")?;
    assert_core_patch_gate_replay(gate, retained, status, stamp, identity);
    assert_eq!(previous.position().get(), 0);
    assert_eq!(previous.time(), stamp);
    assert_eq!(outcomes.len(), 1);
    assert_eq!((charge.g(), charge.p(), charge.total()), (5_658, 0, 5_658));
    let GateDecision::Replay(replayed) = gate.decision() else {
        return Err("expected replay after temporary Verified dropped".into());
    };
    assert!(std::ptr::eq(replayed, retained));
    assert_eq!(replayed.response().as_ptr(), retained.response().as_ptr());
    Ok(())
}

#[test]
fn core_patch_gate_preserves_all_decisions_and_validation_priority() -> TestResult {
    let limits = profile(20_000_000)?;
    let mut charge = PolicySchemaCharge::new(limits);
    assert_core_patch_gate_control_decisions(limits, &mut charge)?;
    assert_core_patch_gate_retained_decisions(limits, &mut charge)?;
    assert_core_patch_gate_header_priority(limits, &mut charge)?;
    assert_core_patch_gate_retained_priority(limits, &mut charge)?;
    assert_core_patch_gate_widths(limits, &mut charge)?;
    Ok(())
}

#[test]
fn core_patch_gate_charges_each_reached_event_before_refusal() -> TestResult {
    let encoded = decode_core_patch_literal(FROZEN_CORE_PATCH_FRAMES[0].0)?;
    let scope = core_patch_gate_scope([b"t", b"g", b"r"])?;
    let stamp = core_patch_gate_stamp(0, 0, 0)?;
    let admission = AdmissionLimits::default();
    assert_core_patch_gate_profile()?;
    for (target, retained_work) in CORE_PATCH_GATE_PHASES {
        let maximum = target - 1;
        let limits = profile(maximum)?;
        let mut charge = PolicySchemaCharge::new(limits);
        let verified =
            verify_complete_core_policy_patch_frame(&encoded, limits, admission, &mut charge)?;
        let original = verified.header(core_patch_gate_log(0));
        let previous =
            ControlState::restored(stamp, original.identity, EntryStamp::Stamped(stamp))?;
        let outcomes = BTreeMap::from([(
            RequestId::new(b"q".to_vec())?,
            core_patch_gate_retained(original.intent, 0, RetainedStatus::Accepted)?,
        )]);
        let result = inspect_complete_core_policy_patch_gate(
            GateView {
                scope: &scope,
                control: previous,
                outcomes: &outcomes,
            },
            verified.header(core_patch_gate_log(1)),
            limits,
            admission,
            &mut charge,
        );
        assert_eq!(
            result
                .err()
                .ok_or("expected distinct prospective gate refusal")?,
            ChargedRequestGateError::Schema(PolicySchemaError::WorkBytes {
                bytes: target,
                maximum,
            })
        );
        assert_eq!(
            (charge.g(), charge.p(), charge.total()),
            (retained_work, 0, retained_work)
        );
        assert_core_patch_gate_frame(&verified, &encoded, FROZEN_CORE_PATCH_FRAMES[0].1, stamp)?;
        assert_eq!(previous.position().get(), 0);
        assert_eq!(previous.time(), stamp);
        assert_eq!(outcomes.len(), 1);
        let original = outcomes
            .get(&RequestId::new(b"q".to_vec())?)
            .ok_or("missing unchanged retained owner")?;
        assert_eq!(original.response(), b"accepted-original\0\xff");
        assert_eq!(original.expires_at().get(), 100);
        assert_eq!(original.original_position(), core_patch_gate_log(0));
        assert_eq!(original.original_time().get(), 0);
        assert_eq!(original.status(), RetainedStatus::Accepted);
    }
    Ok(())
}

#[test]
fn core_patch_gate_keeps_one_account_repeated_outputs_and_prior_peak() -> TestResult {
    for (maximum, target, attempts, peak) in [
        (4_942, 4_942, 1, false),
        (4_941, 4_942, 1, false),
        (9_884, 9_884, 2, false),
        (9_883, 9_884, 2, false),
        (6_934, 6_934, 1, true),
        (6_933, 6_934, 1, true),
        (11_876, 11_876, 2, true),
        (11_875, 11_876, 2, true),
    ] {
        assert_core_patch_gate_account(maximum, target, attempts, peak)?;
    }
    Ok(())
}

fn core_patch_gate_scope(ids: [&[u8]; 3]) -> TestResult<RequestScope> {
    Ok(RequestScope::new(
        TenantId::new(ids[0].to_vec())?,
        GroupId::new(ids[1].to_vec())?,
        RecoveryId::new(ids[2].to_vec())?,
    ))
}

fn core_patch_gate_stamp(physical: u64, logical: u16, wall: u64) -> TestResult<CommittedTime> {
    Ok(CommittedTime::new(
        HlcTimestamp::new(physical, logical)?,
        wall,
    )?)
}

fn core_patch_gate_log(index: u64) -> LogIdentity {
    LogIdentity::new(1, 7, AppliedPosition::new(index))
}

// These admitted opaque owners are mathematical gate fixtures, not installed results.
fn core_patch_gate_retained(
    intent: ClientIntent,
    case: usize,
    status: RetainedStatus,
) -> TestResult<RetainedOutcome> {
    let scope = core_patch_gate_scope([if case == 1 { b"u" } else { b"t" }, b"g", b"r"])?;
    let request = RequestId::new((if case == 2 { b"z" } else { b"q" }).to_vec())?;
    let response = if case == 5 {
        vec![0; 136]
    } else if status == RetainedStatus::Accepted {
        b"accepted-original\0\xff".to_vec()
    } else {
        b"ERR flow state mode must be parallel or fifo".to_vec()
    };
    Ok(RetainedOutcome::new(
        RetainedMetadata {
            scope,
            request,
            intent,
            original_position: core_patch_gate_log(u64::from(case == 3)),
            original_time: TimestampMillis::new(u64::from(case == 4)),
            expires_at: TimestampMillis::new(100),
            status,
        },
        1,
        response,
        AdmissionLimits::default(),
    )?)
}

fn assert_core_patch_gate_frame(
    verified: &VerifiedCompleteCorePolicyPatchFrame<'_>,
    expected: &[u8],
    hash: &str,
    stamp: CommittedTime,
) -> TestResult {
    let digest: [u8; 32] = decode_core_patch_literal(hash)?
        .try_into()
        .map_err(|_| "wrong independent frame SHA length")?;
    assert_eq!(<[u8; 32]>::from(Sha256::digest(expected)), digest);
    assert_eq!(verified.encoded(), expected);
    assert_eq!(verified.original_intent(), &expected[83..]);
    assert_eq!(verified.digest(), PreparedDigest::new(digest));
    let isolation = b"FWSC\0\x01\0\0\0\x01t\0\0\0\x01g\0\0\0\x01r";
    assert_eq!(verified.isolation_scope(), isolation);
    let header = verified.header(core_patch_gate_log(1));
    assert_eq!(header.format_version, 1);
    assert_eq!(header.scope.tenant().as_bytes(), b"t");
    assert_eq!(header.scope.group().as_bytes(), b"g");
    assert_eq!(header.scope.recovery().as_bytes(), b"r");
    assert_eq!(header.request.as_bytes(), b"q");
    assert_eq!(header.intent.version(), 2);
    assert_eq!(header.intent.digest().as_slice(), &expected[29..61]);
    assert_eq!(
        header.intent.digest(),
        &<[u8; 32]>::from(Sha256::digest(&expected[83..]))
    );
    assert_eq!(
        header.identity,
        EntryIdentity::new(core_patch_gate_log(1), PreparedDigest::new(digest))
    );
    assert_eq!(header.stamp, stamp);
    assert_eq!(header.encoded_entry, expected);
    assert_eq!(header.encoded_isolation_scope, isolation);
    Ok(())
}

fn assert_core_patch_gate_replay(
    gate: RequestGate<'_>,
    expected: &RetainedOutcome,
    status: RetainedStatus,
    stamp: CommittedTime,
    identity: EntryIdentity,
) {
    assert_eq!(gate.decision(), GateDecision::Replay(expected));
    assert!(!gate.control().is_log_replay());
    assert_eq!(gate.control().next().last_entry(), Some(identity));
    assert_eq!(gate.control().next().position(), identity.log().index());
    assert_eq!(gate.control().next().time(), stamp);
    assert_eq!(gate.control().execution_time(), stamp.hlc().physical());
    assert_eq!(expected.original_position(), core_patch_gate_log(0));
    assert_eq!(expected.original_time().get(), 0);
    assert_eq!(expected.expires_at().get(), 100);
    assert_eq!(expected.response_version(), 1);
    assert_eq!(expected.status(), status);
    assert_eq!(
        expected.response(),
        if status == RetainedStatus::Accepted {
            &b"accepted-original\0\xff"[..]
        } else {
            &b"ERR flow state mode must be parallel or fifo"[..]
        }
    );
}

fn assert_core_patch_gate_profile() -> TestResult {
    let limits = profile(20_000_000)?;
    let mut charge = PolicySchemaCharge::new(limits);
    let encoded = decode_core_patch_literal(FROZEN_CORE_PATCH_FRAMES[0].0)?;
    let verified = verify_complete_core_policy_patch_frame(
        &encoded,
        limits,
        AdmissionLimits::default(),
        &mut charge,
    )?;
    let scope = core_patch_gate_scope([b"t", b"g", b"r"])?;
    let empty = BTreeMap::new();
    let before = (charge.g(), charge.p(), charge.total());
    let mut header = verified.header(core_patch_gate_log(0));
    header.format_version = 2;
    let result = inspect_complete_core_policy_patch_gate(
        GateView {
            scope: &scope,
            control: ControlState::initial(core_patch_gate_stamp(0, 0, 0)?),
            outcomes: &empty,
        },
        header,
        profile(20_000_001)?,
        AdmissionLimits::new(1_048_576, 134, 1_000)?,
        &mut charge,
    );
    assert_eq!(
        result
            .err()
            .ok_or("expected profile before malformed header and cap")?,
        ChargedRequestGateError::Schema(PolicySchemaError::ProfileMismatch)
    );
    assert_eq!((charge.g(), charge.p(), charge.total()), before);
    assert_eq!(before, (4_423, 0, 4_423));
    Ok(())
}

fn assert_core_patch_gate_widths(
    limits: wf_domain::policy::complete::PolicySchemaLimits,
    charge: &mut PolicySchemaCharge,
) -> TestResult {
    let (expected, intent, digest) = core_patch_gate_wide_frame()?;
    let stamp = core_patch_gate_stamp(0, 0, 0)?;
    let scope = core_patch_gate_scope([b"tt", b"ggg", b"rrrr"])?;
    let admission = AdmissionLimits::default();
    let empty = BTreeMap::new();
    let before = charge.g();
    let verified = verify_complete_core_policy_patch_frame(&expected, limits, admission, charge)?;
    let header = verified.header(core_patch_gate_log(0));
    let fresh = inspect_complete_core_policy_patch_gate(
        GateView {
            scope: &scope,
            control: ControlState::initial(stamp),
            outcomes: &empty,
        },
        header,
        limits,
        admission,
        charge,
    )?;
    assert_eq!(fresh.decision(), GateDecision::Fresh);
    assert_eq!(
        fresh.control().next().last_entry(),
        Some(EntryIdentity::new(
            core_patch_gate_log(0),
            PreparedDigest::new(digest)
        ),)
    );
    assert_eq!(fresh.control().next().time(), stamp);
    assert_eq!((charge.g(), charge.p()), (before + 5_011, 0));
    let outcomes = BTreeMap::from([(
        RequestId::new(b"qqqqq".to_vec())?,
        RetainedOutcome::new(
            RetainedMetadata {
                scope: core_patch_gate_scope([b"tt", b"ggg", b"rrrr"])?,
                request: RequestId::new(b"qqqqq".to_vec())?,
                intent: header.intent,
                original_position: core_patch_gate_log(0),
                original_time: TimestampMillis::new(0),
                expires_at: TimestampMillis::new(100),
                status: RetainedStatus::Accepted,
            },
            1,
            b"accepted-original\0\xff".to_vec(),
            admission,
        )?,
    )]);
    let verified_again =
        verify_complete_core_policy_patch_frame(&expected, limits, admission, charge)?;
    let view = GateView {
        scope: &scope,
        control: fresh.control().next(),
        outcomes: &outcomes,
    };
    let header = verified_again.header(core_patch_gate_log(1));
    let replay = inspect_complete_core_policy_patch_gate(view, header, limits, admission, charge)?;
    assert_eq!(replay, inspect_request(view, header, admission)?);
    let retained = outcomes
        .get(&RequestId::new(b"qqqqq".to_vec())?)
        .ok_or("missing wider-identity retained fixture")?;
    assert_core_patch_gate_replay(
        replay,
        retained,
        RetainedStatus::Accepted,
        stamp,
        EntryIdentity::new(core_patch_gate_log(1), PreparedDigest::new(digest)),
    );
    for owner in [&verified, &verified_again] {
        assert_core_patch_gate_wide_owner(owner, &expected, &intent, digest, stamp);
    }
    assert_eq!((charge.g(), charge.p()), (before + 10_758, 0));
    Ok(())
}

fn core_patch_gate_wide_frame() -> TestResult<(Vec<u8>, Vec<u8>, [u8; 32])> {
    let minimal = decode_core_patch_literal(FROZEN_CORE_PATCH_FRAMES[0].0)?;
    let mut intent = minimal[83..].to_vec();
    drop(intent.splice(33..38, [0, 0, 0, 2, b't', b't']));
    let expected = super::prepare::cases::core_patch_prepare_literal_frame(
        &intent,
        [b"tt", b"ggg", b"rrrr", b"qqqqq"],
        core_patch_gate_stamp(0, 0, 0)?,
    )?;
    let digest: [u8; 32] = decode_core_patch_literal(
        "1ddfd5a2b190f961f9c930080ceb1cc57f63a20665f20264ebf0d88015948cfb",
    )?
    .try_into()
    .map_err(|_| "wrong wider-identity frame SHA length")?;
    assert_eq!((intent.len(), expected.len()), (53, 146));
    assert_eq!(<[u8; 32]>::from(Sha256::digest(&expected)), digest);
    Ok((expected, intent, digest))
}

fn assert_core_patch_gate_wide_owner(
    owner: &VerifiedCompleteCorePolicyPatchFrame<'_>,
    expected: &[u8],
    intent: &[u8],
    digest: [u8; 32],
    stamp: CommittedTime,
) {
    assert_eq!(owner.encoded(), expected);
    assert_eq!(owner.original_intent(), intent);
    assert_eq!(owner.digest(), PreparedDigest::new(digest));
    let isolation = b"FWSC\0\x01\0\0\0\x02tt\0\0\0\x03ggg\0\0\0\x04rrrr";
    assert_eq!(owner.isolation_scope(), isolation);
    let header = owner.header(core_patch_gate_log(1));
    assert_eq!(header.format_version, 1);
    assert_eq!(header.scope.tenant().as_bytes(), b"tt");
    assert_eq!(header.scope.group().as_bytes(), b"ggg");
    assert_eq!(header.scope.recovery().as_bytes(), b"rrrr");
    assert_eq!(header.request.as_bytes(), b"qqqqq");
    assert_eq!(header.intent.version(), 2);
    assert_eq!(
        header.intent.digest(),
        &<[u8; 32]>::from(Sha256::digest(intent))
    );
    assert_eq!(header.intent.digest().as_slice(), &expected[39..71]);
    assert_eq!(header.stamp, stamp);
    assert_eq!(
        header.identity,
        EntryIdentity::new(core_patch_gate_log(1), PreparedDigest::new(digest))
    );
    assert_eq!(header.encoded_entry, expected);
    assert_eq!(header.encoded_isolation_scope, isolation);
}
