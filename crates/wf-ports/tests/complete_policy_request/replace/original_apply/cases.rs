//! Independent original ETF, private VALUE, frame and coherent-view fixtures.

use super::{
    AdmissionLimits, ApplyError, BTreeMap, CompletePolicyData, ControlState, DispatchDecision,
    EncodingError, ExistingError, OriginalEtfSafety, PolicySchemaCharge, PolicySchemaError,
    PolicySchemaLimits, RawError, RawView, ReadError, ReadValue, RequestError, RequestScope,
    RetainedOutcome, RetainedStatus, TestResult, apply, minimal_data, profile,
};
use sha2::{Digest, Sha256};
use wf_domain::workflow::control::{EntryIdentity, EntryStamp, PreparedDigest};
use wf_domain::workflow::request::gate::GateView;
use wf_types::identity::{GroupId, RecoveryId, TenantId};
use wf_types::version::{AppliedPosition, CommittedTime, HlcTimestamp, LogIdentity};

pub(super) fn scope() -> TestResult<RequestScope> {
    Ok(RequestScope::new(
        TenantId::new(b"t".to_vec())?,
        GroupId::new(b"g".to_vec())?,
        RecoveryId::new(b"r".to_vec())?,
    ))
}

pub(super) fn stamp(time: u64) -> TestResult<CommittedTime> {
    Ok(CommittedTime::new(HlcTimestamp::new(time, 0)?, time)?)
}

pub(super) const fn log(index: u64) -> LogIdentity {
    LogIdentity::new(1, 7, AppliedPosition::new(index))
}

pub(super) fn view<'v, 'r, 'c>(
    gate: GateView<'v>,
    policies: &'r BTreeMap<Vec<u8>, ReadValue<'r>>,
    marks: &'r BTreeMap<Vec<u8>, Result<u64, ApplyError>>,
    safety: OriginalEtfSafety<'c>,
) -> RawView<'v, 'r, 'c> {
    RawView {
        gate,
        revision: 7,
        policies,
        active_migrations: marks,
        completed_migrations: marks,
        history_max_events_limit: Ok(1_000_000),
        outcome_retention_ms: Ok(100),
        safety,
    }
}

pub(super) const fn safety() -> OriginalEtfSafety<'static> {
    OriginalEtfSafety::new(
        &[
            "flow_policy_v1",
            "type",
            "version",
            "states",
            "max_active_ms",
            "retry",
            "max_attempts",
        ],
        &[],
        None,
        &[],
    )
}

pub(super) const fn k1_safety() -> OriginalEtfSafety<'static> {
    OriginalEtfSafety::new(&["flow_policy_v1", "type"], &[], None, &[])
}

pub(super) fn frame(
    full: bool,
    guard: Option<u64>,
    request: &[u8],
    time: u64,
) -> TestResult<Vec<u8>> {
    let fields = if full {
        super::super::super::PRESENT_INTENT_BYTES
    } else {
        super::super::super::MINIMAL_INTENT_BYTES
    };
    let mut intent = fields[..35].to_vec();
    if let Some(value) = guard {
        intent.push(1);
        intent.extend_from_slice(&value.to_be_bytes());
    } else {
        intent.push(0);
    }
    intent.extend_from_slice(&fields[36..]);
    super::super::literals::replace_frame_literal(
        &intent,
        [b"t", b"g", b"r", request],
        stamp(time)?,
    )
}

pub(super) fn input_frame(data: CompletePolicyData) -> TestResult<Vec<u8>> {
    let limits = profile(200_000_000)?;
    let mut setup = PolicySchemaCharge::new(limits);
    Ok(wf_ports::log::prepare_complete_policy_replace_frame(
        super::super::super::input(data, None)?,
        limits,
        AdmissionLimits::default(),
        &mut setup,
    )?
    .as_bytes()
    .to_vec())
}

pub(super) fn value_bytes(generation: u64, full: bool) -> TestResult<Vec<u8>> {
    let fields = if full {
        super::super::super::PRESENT_INTENT_BYTES
    } else {
        super::super::super::MINIMAL_INTENT_BYTES
    };
    let mut snapshot = b"FWPS\0\x02".to_vec();
    snapshot.extend_from_slice(&fields[36..]);
    let mut value = b"FWPV\0\x02".to_vec();
    value.extend_from_slice(&generation.to_be_bytes());
    value.extend_from_slice(&u32::try_from(snapshot.len())?.to_be_bytes());
    value.extend_from_slice(&snapshot);
    assert_eq!(value.len(), if full { 382 } else { 40 });
    if generation == 1 {
        let digest = if full {
            "0f61166723f2a7466e67d447059dc25128c8b5aec270a9a626e3edbeeb03b07f"
        } else {
            "cce55bbc22a124e63173e9b7c60e5e8c8c99a1f6847aa8a07750f02e0ff350df"
        };
        assert_eq!(
            <[u8; 32]>::from(Sha256::digest(&value)).as_slice(),
            super::super::literals::replace_frame_hex(digest)?
        );
    }
    Ok(value)
}

pub(super) fn atom(name: &[u8]) -> TestResult<Vec<u8>> {
    let mut out = vec![119, u8::try_from(name.len())?];
    out.extend_from_slice(name);
    Ok(out)
}

pub(super) fn binary(bytes: &[u8]) -> TestResult<Vec<u8>> {
    let mut out = vec![109];
    out.extend_from_slice(&u32::try_from(bytes.len())?.to_be_bytes());
    out.extend_from_slice(bytes);
    Ok(out)
}

pub(super) fn map(rows: &[(Vec<u8>, Vec<u8>)]) -> TestResult<Vec<u8>> {
    let mut out = vec![116];
    out.extend_from_slice(&u32::try_from(rows.len())?.to_be_bytes());
    for (key, value) in rows {
        out.extend_from_slice(key);
        out.extend_from_slice(value);
    }
    Ok(out)
}

pub(super) fn envelope(generation: u64, policy: &[u8]) -> TestResult<Vec<u8>> {
    let mut out = vec![131, 104, 3];
    out.extend_from_slice(&atom(b"flow_policy_v1")?);
    if generation < 256 {
        out.extend_from_slice(&[97, u8::try_from(generation)?]);
    } else {
        let digits = generation.to_le_bytes();
        let length = 8 - generation.leading_zeros() as usize / 8;
        out.extend_from_slice(&[110, u8::try_from(length)?, 0]);
        out.extend_from_slice(&digits[..length]);
    }
    out.extend_from_slice(policy);
    Ok(out)
}

pub(super) fn current(generation: u64) -> TestResult<Vec<u8>> {
    let out = envelope(generation, &map(&[(atom(b"type")?, binary(b"r")?)])?)?;
    if generation == 0 {
        assert_eq!(
            out,
            super::super::literals::replace_frame_hex(
                "836803770e666c6f775f706f6c6963795f7631610074000000017704747970656d0000000172"
            )?
        );
        assert_eq!(
            <[u8; 32]>::from(Sha256::digest(&out)).as_slice(),
            super::super::literals::replace_frame_hex(
                "6e9978636a6017b6e2abf81647a5aab1b71f003b16e23ad931a1beda2d62b9a9"
            )?
        );
    }
    Ok(out)
}

pub(super) fn corrupt() -> RawError {
    RawError::Current(ReadError::Corrupt)
}
pub(super) fn request(error: EncodingError) -> RawError {
    RawError::Replace(ExistingError::Request(RequestError::Encoding(error)))
}
pub(super) fn schema(error: PolicySchemaError) -> RawError {
    request(EncodingError::Schema(error))
}

pub(super) fn outcome(out: &RetainedOutcome, frame: &[u8], body: &[u8], time: u64, index: u64) {
    assert_eq!(out.scope().tenant().as_bytes(), b"t");
    assert_eq!(out.scope().group().as_bytes(), b"g");
    assert_eq!(out.scope().recovery().as_bytes(), b"r");
    assert_eq!(out.request().as_bytes(), &frame[26..27]);
    assert_eq!(out.intent().version(), 2);
    assert_eq!(
        *out.intent().digest(),
        <[u8; 32]>::from(Sha256::digest(&frame[83..]))
    );
    assert_eq!(out.original_position(), log(index));
    assert_eq!(out.original_time().get(), time);
    assert_eq!(out.expires_at().get(), time + 100);
    assert_eq!(out.response_version(), 1);
    assert_eq!(out.response(), body);
}

pub(super) fn control(
    plan: &super::CompletePolicyReplaceRequestPlan<'_>,
    base: ControlState,
    frame: &[u8],
    time: u64,
    index: u64,
    replay: bool,
) -> TestResult {
    let expected = if replay {
        base
    } else {
        ControlState::restored(
            stamp(time)?,
            EntryIdentity::new(
                log(index),
                PreparedDigest::new(Sha256::digest(frame).into()),
            ),
            EntryStamp::Stamped(stamp(time)?),
        )?
    };
    assert_eq!(plan.expected_revision(), 7);
    assert_eq!(plan.base_control(), base);
    assert_eq!(plan.control().next(), expected);
    assert_eq!(plan.control().is_log_replay(), replay);
    assert_eq!(
        plan.control().execution_time(),
        expected.time().hlc().physical()
    );
    Ok(())
}

pub(super) fn accepted(
    plan: &super::CompletePolicyReplaceRequestPlan<'_>,
    data: &CompletePolicyData,
    bytes: &[u8],
    stored: u64,
    frame: &[u8],
) -> TestResult {
    accepted_generation(plan, data, bytes, stored, stored + 1, frame)
}

fn accepted_generation(
    plan: &super::CompletePolicyReplaceRequestPlan<'_>,
    data: &CompletePolicyData,
    bytes: &[u8],
    stored: u64,
    generation: u64,
    frame: &[u8],
) -> TestResult {
    let DispatchDecision::Fresh((Some((old, value)), retained)) = plan.decision() else {
        return Err("expected complete Fresh write".into());
    };
    assert_eq!(*old, stored);
    assert!(value.data() == data);
    assert_eq!(value.generation(), generation);
    assert_eq!(value.encoded(), bytes);
    assert_eq!(retained.status(), RetainedStatus::Accepted);
    outcome(retained, frame, bytes, 0, 0);
    assert_ne!(value.encoded().as_ptr(), retained.response().as_ptr());
    Ok(())
}

pub(super) fn assert_dominating_migrations(
    gate: GateView<'_>,
    limits: PolicySchemaLimits,
    charge: &mut PolicySchemaCharge,
) -> TestResult {
    let raw = current(1)?;
    let policies = BTreeMap::from([(b"r".to_vec(), ReadValue::Binary(&raw))]);
    let bytes = frame(false, None, b"q", 0)?;
    for (active, done, generation) in [(7, 3, 8), (3, 9, 10)] {
        let active = BTreeMap::from([(b"r".to_vec(), Ok(active))]);
        let done = BTreeMap::from([(b"r".to_vec(), Ok(done))]);
        let expected = value_bytes(generation, false)?;
        let mut supplied = view(gate, &policies, &active, k1_safety());
        supplied.completed_migrations = &done;
        let plan = apply(&supplied, &bytes, 0, limits, charge)?;
        accepted_generation(&plan, &minimal_data(), &expected, 1, generation, &bytes)?;
        assert_eq!(raw, current(1)?);
    }
    Ok(())
}

pub(super) fn rejected(
    plan: &super::CompletePolicyReplaceRequestPlan<'_>,
    body: &[u8],
    frame: &[u8],
) -> TestResult {
    let DispatchDecision::Fresh((None, retained)) = plan.decision() else {
        return Err("business failure must not install".into());
    };
    outcome(retained, frame, body, 0, 0);
    Ok(())
}

#[test]
fn original_current_replace_preserves_reader_origins_and_profile_refusals() -> TestResult {
    let limits = profile(200_000_000)?;
    let scope = scope()?;
    let outcomes = BTreeMap::new();
    let marks = BTreeMap::new();
    let control = ControlState::initial(stamp(0)?);
    let gate = GateView {
        scope: &scope,
        control,
        outcomes: &outcomes,
    };
    let frame = frame(false, None, b"q", 0)?;
    let poisoned = OriginalEtfSafety::new(&["duplicate", "duplicate"], &[], None, &[]);
    for (class, expected) in [
        (ReadValue::Other, Some(corrupt())),
        (
            ReadValue::Unavailable,
            Some(RawError::Current(ReadError::ShardUnavailable)),
        ),
        (
            ReadValue::ReadFailed,
            Some(RawError::Current(ReadError::ReadFailed)),
        ),
        (ReadValue::Absent, None),
    ] {
        let policies = BTreeMap::from([(b"r".to_vec(), class)]);
        let mut charge = PolicySchemaCharge::new(limits);
        let result = apply(
            &view(gate, &policies, &marks, poisoned),
            &frame,
            0,
            limits,
            &mut charge,
        );
        if let Some(error) = expected {
            assert_eq!(result.err(), Some(error));
            assert_eq!((charge.g(), charge.p()), (6_276, 0));
        } else {
            accepted(
                &result?,
                &minimal_data(),
                &value_bytes(1, false)?,
                0,
                &frame,
            )?;
            assert_eq!((charge.g(), charge.p()), (9_640, 0));
        }
    }
    assert_eq!(corrupt().to_string(), "ERR corrupt flow policy high-water");
    assert_eq!(ReadError::Corrupt.to_string(), "ERR flow policy is corrupt");
    let raw = current(0)?;
    let policies = BTreeMap::from([(b"r".to_vec(), ReadValue::Binary(&raw))]);
    let supplied = view(gate, &policies, &marks, k1_safety());
    let mut charge = PolicySchemaCharge::new(limits);
    assert_eq!(
        apply(&supplied, &frame, 0, profile(199_999_999)?, &mut charge).err(),
        Some(schema(PolicySchemaError::ProfileMismatch))
    );
    assert_eq!((charge.g(), charge.p()), (0, 0));
    assert_current_bounds(gate, &marks, &frame, limits)?;
    super::refusals::assert_current_nodes(gate, &marks, &frame, limits)?;
    Ok(())
}

pub(super) fn assert_source_boundary(
    gate: GateView<'_>,
    limits: PolicySchemaLimits,
    charge: &mut PolicySchemaCharge,
) -> TestResult {
    use wf_domain::policy::definition::PolicyVersion;
    use wf_domain::policy::installation::MAX_POLICY_GENERATION;

    let mut data = minimal_data();
    data.workflow_type = b"generation-boundary".to_vec();
    data.version = Some(PolicyVersion::Bytes(b"public-v1".to_vec()));
    let bytes = input_frame(data)?;
    let mut data = minimal_data();
    data.workflow_type = b"generation-boundary".to_vec();
    data.version = Some(PolicyVersion::Bytes(b"public-v1".to_vec()));
    let raw = envelope(
        MAX_POLICY_GENERATION - 1,
        &map(&[
            (atom(b"type")?, binary(b"generation-boundary")?),
            (atom(b"version")?, binary(b"public-v1")?),
        ])?,
    )?;
    let policies = BTreeMap::from([(data.workflow_type.clone(), ReadValue::Binary(&raw))]);
    let marks = BTreeMap::from([(data.workflow_type.clone(), Ok(MAX_POLICY_GENERATION - 1))]);
    let mut snapshot = b"FWPS\0\x02".to_vec();
    snapshot.extend_from_slice(&19_u32.to_be_bytes());
    snapshot.extend_from_slice(b"generation-boundary");
    snapshot.extend_from_slice(&[1, 0]);
    snapshot.extend_from_slice(&9_u32.to_be_bytes());
    snapshot.extend_from_slice(b"public-v1");
    snapshot.extend_from_slice(&super::super::super::MINIMAL_INTENT_BYTES[42..]);
    let mut expected = b"FWPV\0\x02".to_vec();
    expected.extend_from_slice(&MAX_POLICY_GENERATION.to_be_bytes());
    expected.extend_from_slice(&u32::try_from(snapshot.len())?.to_be_bytes());
    expected.extend_from_slice(&snapshot);
    assert_eq!(expected.len(), 72);
    let plan = apply(
        &view(gate, &policies, &marks, safety()),
        &bytes,
        0,
        limits,
        charge,
    )?;
    accepted(&plan, &data, &expected, MAX_POLICY_GENERATION - 1, &bytes)?;
    assert_eq!(
        raw,
        envelope(
            MAX_POLICY_GENERATION - 1,
            &map(&[
                (atom(b"type")?, binary(b"generation-boundary")?),
                (atom(b"version")?, binary(b"public-v1")?),
            ])?
        )?
    );
    Ok(())
}

pub(super) fn assert_source_cas_sequence(
    gate: GateView<'_>,
    limits: PolicySchemaLimits,
    charge: &mut PolicySchemaCharge,
) -> TestResult {
    let marks = BTreeMap::new();
    for (stored, old, guard, next) in [
        (0, 0_u32, None, 1_000_u64),
        (1, 1_000, Some(1), 2_000),
        (2, 2_000, Some(1), 3_000),
    ] {
        let mut fields = vec![(atom(b"type")?, binary(b"r")?)];
        if old != 0 {
            let mut value = vec![98];
            value.extend_from_slice(&old.to_be_bytes());
            fields.push((atom(b"max_active_ms")?, value));
        }
        let raw = envelope(stored, &map(&fields)?)?;
        let unchanged = raw.clone();
        let policies = if stored == 0 {
            BTreeMap::new()
        } else {
            BTreeMap::from([(b"r".to_vec(), ReadValue::Binary(&raw))])
        };
        let mut data = minimal_data();
        data.max_active_ms = Some(next);
        let mut setup = PolicySchemaCharge::new(limits);
        let bytes = wf_ports::log::prepare_complete_policy_replace_frame(
            super::super::super::input(data, guard)?,
            limits,
            AdmissionLimits::default(),
            &mut setup,
        )?
        .as_bytes()
        .to_vec();
        let mut expected = value_bytes(stored + 1, false)?;
        expected[14..18].copy_from_slice(&30_u32.to_be_bytes());
        expected[30] = 1;
        expected.splice(31..31, next.to_be_bytes());
        assert_eq!(expected.len(), 48);
        let plan = apply(
            &view(gate, &policies, &marks, safety()),
            &bytes,
            0,
            limits,
            charge,
        )?;
        if stored == 2 {
            rejected(&plan, b"ERR stale flow policy generation", &bytes)?;
            assert_eq!(raw, unchanged);
        } else {
            let mut data = minimal_data();
            data.max_active_ms = Some(next);
            accepted(&plan, &data, &expected, stored, &bytes)?;
        }
    }
    Ok(())
}

fn assert_current_bounds(
    gate: GateView<'_>,
    marks: &BTreeMap<Vec<u8>, Result<u64, ApplyError>>,
    frame: &[u8],
    limits: PolicySchemaLimits,
) -> TestResult {
    for size in [263_116, 263_117] {
        let raw = envelope(
            0,
            &map(&[
                (atom(b"type")?, binary(b"r")?),
                (atom(b"version")?, binary(&vec![b'x'; size])?),
            ])?,
        )?;
        assert_eq!(raw.len(), size + 52);
        let policies = BTreeMap::from([(b"r".to_vec(), ReadValue::Binary(&raw))]);
        let mut charge = PolicySchemaCharge::new(limits);
        let mut supplied = view(gate, &policies, marks, safety());
        if size == 263_117 {
            supplied.outcome_retention_ms = Err(ApplyError::PositionConflict);
        }
        let result = apply(&supplied, frame, 0, limits, &mut charge);
        if size == 263_116 {
            accepted(&result?, &minimal_data(), &value_bytes(1, false)?, 0, frame)?;
            assert_eq!(raw.len() - 20, 263_148);
            assert_eq!(charge.p(), 640);
        } else {
            assert_eq!(result.err(), Some(corrupt()));
            assert_eq!((charge.g(), charge.p()), (6_404, 0));
        }
    }
    for (raw, expected) in [
        (
            vec![131, 121, 0, 0, 0, 0, 106],
            EncodingError::UnsupportedTag,
        ),
        (vec![131, 73, 0, 1], EncodingError::UnsupportedTag),
    ] {
        let policies = BTreeMap::from([(b"r".to_vec(), ReadValue::Binary(&raw))]);
        let mut charge = PolicySchemaCharge::new(limits);
        assert_eq!(
            apply(
                &view(gate, &policies, marks, k1_safety()),
                frame,
                0,
                limits,
                &mut charge
            )
            .err(),
            Some(RawError::Current(ReadError::Encoding(expected)))
        );
    }
    let mut deep = vec![106];
    for _ in 0..63 {
        let mut next = vec![108, 0, 0, 0, 0];
        next.extend_from_slice(&deep);
        deep = next;
    }
    let raw = envelope(0, &map(&[(binary(b"x")?, deep)])?)?;
    let policies = BTreeMap::from([(b"r".to_vec(), ReadValue::Binary(&raw))]);
    let mut charge = PolicySchemaCharge::new(limits);
    assert_eq!(
        apply(
            &view(gate, &policies, marks, k1_safety()),
            frame,
            0,
            limits,
            &mut charge
        )
        .err(),
        Some(RawError::Current(ReadError::Encoding(
            EncodingError::ItemLimit
        )))
    );
    let raw = current(0)?;
    let policies = BTreeMap::from([(b"r".to_vec(), ReadValue::Binary(&raw))]);
    let mut charge = PolicySchemaCharge::new(limits);
    assert_eq!(
        apply(
            &view(
                gate,
                &policies,
                marks,
                OriginalEtfSafety::new(&["flow_policy_v1", "flow_policy_v1"], &[], None, &[])
            ),
            frame,
            0,
            limits,
            &mut charge
        )
        .err(),
        Some(RawError::Current(ReadError::Encoding(
            EncodingError::Malformed
        )))
    );
    Ok(())
}
