//! Source-derived ETF fixtures and actual owned original-parent observations.

use super::{
    AdmissionLimits, ApplyError, BTreeMap, ControlState, DispatchDecision, EncodingError,
    ExistingError, FlowFresh, Input, OriginalEtfSafety, Plan, PolicySchemaCharge,
    PolicySchemaError, PolicySchemaLimits, Raw, RawError, RawView, ReadError, ReadValue,
    RequestError, RequestScope, RetainedOutcome, RetainedStatus, TestResult, apply, profile,
};
use sha2::{Digest, Sha256};
use wf_domain::policy::installation::{MAX_POLICY_GENERATION, PolicyError};
use wf_domain::workflow::control::{EntryIdentity, EntryStamp, PreparedDigest};
use wf_domain::workflow::request::gate::GateView;
use wf_ports::log::prepare_original_flow_policy_replace_frame;
use wf_types::identity::{GroupId, RecoveryId, RequestId, TenantId};
use wf_types::version::{AppliedPosition, CommittedTime, HlcTimestamp, LogIdentity};

pub(super) const KEY: &[u8] = b"f:{f}:policy:r";

pub(super) fn scope() -> TestResult<RequestScope> {
    Ok(RequestScope::new(
        TenantId::new(b"a".to_vec())?,
        GroupId::new(b"b".to_vec())?,
        RecoveryId::new(b"c".to_vec())?,
    ))
}

pub(super) fn stamp(time: u64) -> TestResult<CommittedTime> {
    Ok(CommittedTime::new(HlcTimestamp::new(time, 0)?, time)?)
}

pub(super) const fn log(index: u64) -> LogIdentity {
    LogIdentity::new(1, 7, AppliedPosition::new(index))
}

pub(super) fn frame(expected: Raw<'_>, patch: Raw<'_>) -> TestResult<Vec<u8>> {
    frame_with(KEY, expected, patch, b"q", 0)
}

pub(super) fn frame_with(
    key: &[u8],
    expected: Raw<'_>,
    patch: Raw<'_>,
    request: &[u8],
    time: u64,
) -> TestResult<Vec<u8>> {
    let limits = profile(2_000_000_000)?;
    let input = Input::new(
        scope()?,
        RequestId::new(request.to_vec())?,
        stamp(time)?,
        key,
        expected,
        patch,
    );
    let mut setup = PolicySchemaCharge::new(limits);
    Ok(prepare_original_flow_policy_replace_frame(
        input,
        limits,
        AdmissionLimits::default(),
        &mut setup,
    )?
    .as_bytes()
    .to_vec())
}

pub(super) fn view<'v, 'r, 'c>(
    gate: GateView<'v>,
    policies: &'r BTreeMap<Vec<u8>, ReadValue<'r>>,
    active: &'r BTreeMap<Vec<u8>, Result<u64, ApplyError>>,
    done: &'r BTreeMap<Vec<u8>, Result<u64, ApplyError>>,
    safety: OriginalEtfSafety<'c>,
) -> RawView<'v, 'r, 'c> {
    RawView {
        gate,
        revision: 7,
        policies,
        active_migrations: active,
        completed_migrations: done,
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
            "states",
            "version",
            "max_active_ms",
            "mode",
            "fifo",
            "unknown",
            "retry",
            "max_attempts",
            "governance",
            "effects",
            "allowed",
            "denied",
            "approvals",
            "assignees",
            "retention",
            "history_max_events",
            "backoff",
            "kind",
            "base_ms",
            "max_ms",
            "jitter_pct",
            "exponential",
        ],
        &[],
        None,
        &[],
    )
}

pub(super) const fn k1_safety() -> OriginalEtfSafety<'static> {
    OriginalEtfSafety::new(&["flow_policy_v1", "type"], &[], None, &[])
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

pub(super) fn number(value: u64) -> TestResult<Vec<u8>> {
    if let Ok(small) = u8::try_from(value) {
        return Ok(vec![97, small]);
    }
    if let Ok(small) = i32::try_from(value) {
        let mut out = vec![98];
        out.extend_from_slice(&small.to_be_bytes());
        return Ok(out);
    }
    let digits = value.to_le_bytes();
    let length = 8 - value.leading_zeros() as usize / 8;
    big(&digits[..length])
}

pub(super) fn big(digits: &[u8]) -> TestResult<Vec<u8>> {
    let mut out = if let Ok(length) = u8::try_from(digits.len()) {
        vec![110, length, 0]
    } else {
        let mut out = vec![111];
        out.extend_from_slice(&u32::try_from(digits.len())?.to_be_bytes());
        out.push(0);
        out
    };
    out.extend_from_slice(digits);
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

pub(super) fn list(names: &[Vec<u8>]) -> TestResult<Vec<u8>> {
    let mut out = vec![108];
    out.extend_from_slice(&u32::try_from(names.len())?.to_be_bytes());
    for name in names {
        out.extend_from_slice(&binary(name)?);
    }
    out.push(106);
    Ok(out)
}

pub(super) fn policy(type_: &[u8], fields: &[(&str, Vec<u8>)]) -> TestResult<Vec<u8>> {
    let mut rows = vec![("states", map(&[])?), ("type", binary(type_)?)];
    for (name, bytes) in fields {
        if let Some(row) = rows.iter_mut().find(|row| row.0 == *name) {
            row.1.clone_from(bytes);
        } else {
            rows.push((name, bytes.clone()));
        }
    }
    rows.sort_by_key(|row| row.0);
    map(&rows
        .into_iter()
        .map(|(name, value)| Ok((atom(name.as_bytes())?, value)))
        .collect::<TestResult<Vec<_>>>()?)
}

pub(super) fn envelope(generation: u64, policy: &[u8]) -> TestResult<Vec<u8>> {
    let mut out = vec![131, 104, 3];
    out.extend_from_slice(&atom(b"flow_policy_v1")?);
    out.extend_from_slice(&number(generation)?);
    out.extend_from_slice(policy);
    Ok(out)
}

pub(super) fn expected(
    generation: u64,
    type_: &[u8],
    fields: &[(&str, Vec<u8>)],
) -> TestResult<Vec<u8>> {
    envelope(generation, &policy(type_, fields)?)
}

pub(super) fn current(generation: u64, type_: &[u8]) -> TestResult<Vec<u8>> {
    envelope(generation, &map(&[(atom(b"type")?, binary(type_)?)])?)
}

pub(super) fn request(error: EncodingError) -> RawError {
    RawError::Replace(ExistingError::Request(RequestError::Encoding(error)))
}

pub(super) fn schema(error: PolicySchemaError) -> RawError {
    request(EncodingError::Schema(error))
}

pub(super) fn corrupt() -> RawError {
    RawError::Current(ReadError::Corrupt)
}

pub(super) fn retained<'p>(plan: &'p Plan<'_>) -> TestResult<&'p RetainedOutcome> {
    let DispatchDecision::Fresh((_, retained)) = plan.decision() else {
        return Err("expected actual Fresh outcome".into());
    };
    Ok(retained)
}

pub(super) fn outcome(out: &RetainedOutcome, frame: &[u8], body: &[u8]) {
    assert_eq!(out.scope().tenant().as_bytes(), b"a");
    assert_eq!(out.scope().group().as_bytes(), b"b");
    assert_eq!(out.scope().recovery().as_bytes(), b"c");
    assert_eq!(out.request().as_bytes(), &frame[26..27]);
    assert_eq!(out.intent().version(), 3);
    assert_eq!(out.intent().digest().as_slice(), &frame[29..61]);
    assert_eq!(out.original_position(), log(0));
    assert_eq!(out.original_time().get(), 0);
    assert_eq!(out.expires_at().get(), 100);
    assert_eq!(out.response_version(), 1);
    assert_eq!(out.response(), body);
}

pub(super) fn accepted(
    plan: &Plan<'_>,
    stored: u64,
    generation: u64,
    key: &[u8],
    bytes: &[u8],
) -> TestResult {
    let DispatchDecision::Fresh((Some(value), out)) = plan.decision() else {
        return Err("expected actual original ETF installation".into());
    };
    assert_eq!(value.policy_key(), key);
    assert_eq!(value.stored_generation(), stored);
    assert_eq!(value.generation(), generation);
    assert_eq!(value.encoded(), bytes);
    assert_eq!(out.status(), RetainedStatus::Accepted);
    assert_eq!(out.response(), bytes);
    assert_ne!(value.encoded().as_ptr(), out.response().as_ptr());
    Ok(())
}

pub(super) fn rejected(
    plan: &Plan<'_>,
    status: RetainedStatus,
    body: &[u8],
    frame: &[u8],
) -> TestResult {
    let DispatchDecision::Fresh((None, out)) = plan.decision() else {
        return Err("rejection must not install a value".into());
    };
    assert_eq!(out.status(), status);
    outcome(out, frame, body);
    Ok(())
}

pub(super) fn control(
    plan: &Plan<'_>,
    base: ControlState,
    frame: &[u8],
    time: u64,
    index: u64,
    replay: bool,
) -> TestResult {
    let next = if replay {
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
    assert_eq!(plan.control().next(), next);
    assert_eq!(plan.control().is_log_replay(), replay);
    Ok(())
}

pub(super) fn take(plan: Plan<'_>) -> TestResult<(FlowFresh, ControlState)> {
    let (_, _, next, decision) = plan.into_parts();
    let DispatchDecision::Fresh(bundle) = decision else {
        return Err("Fresh bundle".into());
    };
    Ok((bundle, next.next()))
}

pub(super) fn assert_current_and_guard(
    gate: GateView<'_>,
    limits: PolicySchemaLimits,
) -> TestResult {
    let marks = BTreeMap::new();
    let invalid_guard = frame(Raw::Integer(-1), Raw::Map(&[]))?;
    let mut two_tuple = vec![131, 104, 2];
    two_tuple.extend_from_slice(&atom(b"flow_policy_v1")?);
    two_tuple.extend_from_slice(&map(&[])?);
    for bad in [two_tuple, vec![131, 80, 0, 0, 0, 1, 0], vec![131, 106]] {
        let policies = BTreeMap::from([(b"r".to_vec(), ReadValue::Binary(&bad))]);
        let mut charge = PolicySchemaCharge::new(limits);
        assert_eq!(
            apply(
                &view(gate, &policies, &marks, &marks, safety()),
                &invalid_guard,
                0,
                limits,
                AdmissionLimits::default(),
                &mut charge
            )
            .err(),
            Some(corrupt())
        );
    }
    let raw = current(1, b"r")?;
    let policies = BTreeMap::from([(b"r".to_vec(), ReadValue::Binary(&raw))]);
    let wide = [0, 0, 0, 0, 0, 0, 0, 0, 1];
    for guard in [
        Raw::Integer(-1),
        Raw::Integer(i128::from(MAX_POLICY_GENERATION) + 1),
        Raw::Binary(b"1"),
        Raw::List(&[]),
        Raw::Map(&[]),
        Raw::BigInteger {
            negative: false,
            little_endian: &wide,
        },
    ] {
        let bytes = frame(guard, Raw::Map(&[]))?;
        let mut charge = PolicySchemaCharge::new(limits);
        let plan = apply(
            &view(gate, &policies, &marks, &marks, k1_safety()),
            &bytes,
            0,
            limits,
            AdmissionLimits::default(),
            &mut charge,
        )?;
        let out = retained(&plan)?;
        assert!(matches!(out.status(), RetainedStatus::PolicyRejected(r)
            if r.error() == PolicyError::InvalidGeneration));
        outcome(out, &bytes, b"ERR invalid flow policy generation");
    }
    assert_zero_and_arbitrary_current(gate, limits)?;
    assert_current_encoded_bound(gate, limits)?;
    assert_type_binding(gate, limits)?;
    Ok(())
}

fn assert_zero_and_arbitrary_current(gate: GateView<'_>, limits: PolicySchemaLimits) -> TestResult {
    let marks = BTreeMap::new();
    let raws = [
        current(0, b"r")?,
        envelope(
            1,
            &map(&[
                (atom(b"max_active_ms")?, number(0)?),
                (atom(b"states")?, number(7)?),
                (atom(b"type")?, binary(b"r")?),
            ])?,
        )?,
    ];
    for (raw, stored) in raws.iter().zip([0, 1]) {
        let policies = BTreeMap::from([(b"r".to_vec(), ReadValue::Binary(raw))]);
        let bytes = frame(Raw::Integer(i128::from(stored)), Raw::Map(&[]))?;
        let final_bytes = expected(stored + 1, b"r", &[])?;
        let mut charge = PolicySchemaCharge::new(limits);
        let plan = apply(
            &view(gate, &policies, &marks, &marks, safety()),
            &bytes,
            0,
            limits,
            AdmissionLimits::default(),
            &mut charge,
        )?;
        accepted(&plan, stored, stored + 1, KEY, &final_bytes)?;
        outcome(retained(&plan)?, &bytes, &final_bytes);
    }
    Ok(())
}

fn assert_current_encoded_bound(gate: GateView<'_>, limits: PolicySchemaLimits) -> TestResult {
    let payload = vec![0; 263_116];
    let raw = envelope(
        1,
        &map(&[
            (atom(b"type")?, binary(b"r")?),
            (atom(b"version")?, binary(&payload)?),
        ])?,
    )?;
    assert_eq!(raw.len(), 263_168);
    // Source current decoding does NOT import the bare snapshot262144 cap.
    let policies = BTreeMap::from([(b"r".to_vec(), ReadValue::Binary(&raw))]);
    let marks = BTreeMap::new();
    let bytes = frame(Raw::Nil, Raw::Map(&[]))?;
    let final_bytes = expected(2, b"r", &[])?;
    let mut charge = PolicySchemaCharge::new(limits);
    let plan = apply(
        &view(gate, &policies, &marks, &marks, safety()),
        &bytes,
        0,
        limits,
        AdmissionLimits::default(),
        &mut charge,
    )?;
    accepted(&plan, 1, 2, KEY, &final_bytes)?;
    let oversized = vec![0; 263_169];
    let policies = BTreeMap::from([(b"r".to_vec(), ReadValue::Binary(&oversized))]);
    let mut charge = PolicySchemaCharge::new(limits);
    let duplicate = OriginalEtfSafety::new(&["x", "x"], &[], None, &[]);
    assert_eq!(
        apply(
            &view(gate, &policies, &marks, &marks, duplicate),
            &bytes,
            0,
            limits,
            AdmissionLimits::default(),
            &mut charge
        )
        .err(),
        Some(corrupt())
    );
    assert_eq!((charge.g(), charge.p()), (5_857, 0));
    Ok(())
}

fn assert_type_binding(gate: GateView<'_>, limits: PolicySchemaLimits) -> TestResult {
    let marks = BTreeMap::new();
    for type_value in [binary(b"foreign")?, number(1)?] {
        let raw = envelope(1, &map(&[(atom(b"type")?, type_value)])?)?;
        let missing = envelope(1, &map(&[])?)?;
        for raw in [&raw, &missing] {
            let policies = BTreeMap::from([(b"r".to_vec(), ReadValue::Binary(raw))]);
            for guard in [Raw::Integer(0), Raw::Integer(1), Raw::Nil] {
                let bytes = frame(guard, Raw::Map(&[]))?;
                let mut charge = PolicySchemaCharge::new(limits);
                let mut supplied = view(gate, &policies, &marks, &marks, safety());
                supplied.history_max_events_limit = Err(ApplyError::AllocationFailed);
                let result = apply(
                    &supplied,
                    &bytes,
                    0,
                    limits,
                    AdmissionLimits::default(),
                    &mut charge,
                );
                if matches!(guard, Raw::Integer(0)) {
                    let plan = result?;
                    assert!(
                        matches!(retained(&plan)?.status(), RetainedStatus::PolicyRejected(r)
                        if r.error() == PolicyError::StaleGeneration)
                    );
                    outcome(
                        retained(&plan)?,
                        &bytes,
                        b"ERR stale flow policy generation",
                    );
                } else {
                    assert_eq!(result.err(), Some(corrupt()));
                }
            }
        }
    }
    for (class, error) in [
        (ReadValue::Other, ReadError::Corrupt),
        (ReadValue::Unavailable, ReadError::ShardUnavailable),
        (ReadValue::ReadFailed, ReadError::ReadFailed),
    ] {
        let policies = BTreeMap::from([(b"r".to_vec(), class)]);
        let mut charge = PolicySchemaCharge::new(limits);
        assert_eq!(
            apply(
                &view(gate, &policies, &marks, &marks, safety()),
                &frame(Raw::Nil, Raw::Map(&[]))?,
                0,
                limits,
                AdmissionLimits::default(),
                &mut charge
            )
            .err(),
            Some(RawError::Current(error))
        );
    }
    assert_eq!(corrupt().to_string(), "ERR corrupt flow policy high-water");
    Ok(())
}

pub(super) fn assert_highwater(gate: GateView<'_>, limits: PolicySchemaLimits) -> TestResult {
    let mut charge = PolicySchemaCharge::new(limits);
    let empty = BTreeMap::new();
    let no_marks = BTreeMap::new();
    let bytes = frame(Raw::Nil, Raw::Map(&[]))?;
    for (stored, active, done, next) in [
        (1, 7, 3, 8),
        (1, 3, 9, 10),
        (3, 0, 0, 4),
        (MAX_POLICY_GENERATION - 1, 0, 0, MAX_POLICY_GENERATION),
    ] {
        let raw = current(stored, b"r")?;
        let policies = BTreeMap::from([(b"r".to_vec(), ReadValue::Binary(&raw))]);
        let active = BTreeMap::from([(b"r".to_vec(), Ok(active))]);
        let done = BTreeMap::from([(b"r".to_vec(), Ok(done))]);
        let final_bytes = expected(next, b"r", &[])?;
        let plan = apply(
            &view(gate, &policies, &active, &done, k1_safety()),
            &bytes,
            0,
            limits,
            AdmissionLimits::default(),
            &mut charge,
        )?;
        accepted(&plan, stored, next, KEY, &final_bytes)?;
    }
    let exhausted = BTreeMap::from([(b"r".to_vec(), Ok(MAX_POLICY_GENERATION))]);
    let plan = apply(
        &view(gate, &empty, &exhausted, &no_marks, safety()),
        &bytes,
        0,
        limits,
        AdmissionLimits::default(),
        &mut charge,
    )?;
    assert!(
        matches!(retained(&plan)?.status(), RetainedStatus::PolicyRejected(r)
        if r.error() == PolicyError::GenerationExhausted)
    );
    outcome(
        retained(&plan)?,
        &bytes,
        b"ERR flow policy generation exhausted",
    );
    Ok(())
}
