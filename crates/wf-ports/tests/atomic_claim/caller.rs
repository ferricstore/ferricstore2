use super::{TestResult, encode, intent, options};
use wf_domain::claim::caller::{
    ClaimDueInput, ClaimPartitionsInput, ClaimReplyMode, ClaimStatesInput, ClaimValuesInput,
};

#[test]
fn full_normalized_caller_fields_change_parent_identity() -> TestResult {
    let base = encode(ClaimDueInput::new(b"job", b"worker"), options(), 100)?;
    let names: [&[u8]; 1] = [b"x\0"];
    for field in 0..16 {
        let mut input = ClaimDueInput::new(b"job", b"worker");
        match field {
            0 => input.workflow_type = b"other",
            1 => input.worker = b"other",
            2 => input.states = ClaimStatesInput::Many(&names),
            3 => input.partitions = ClaimPartitionsInput::Many(&names),
            4 => input.values = ClaimValuesInput::Names(&names),
            5 => input.lease_ms = Some(1),
            6 => input.limit = Some(2),
            7 => input.priority = Some(0),
            8 => input.reclaim_expired = Some(false),
            9 => input.reclaim_ratio = Some(0),
            10 => input.reply_mode = Some(ClaimReplyMode::Jobs),
            11 => input.payload = Some(false),
            12 => input.payload_max_bytes = Some(0),
            13 => input.governance_limit_scope = Some(b"scope"),
            14 => input.governance_shard_id = Some(0),
            _ => input.requested_now_ms = Some(u64::MAX),
        }
        let other = encode(input, options(), 100)?;
        assert_ne!(
            intent(base.as_bytes())?,
            intent(other.as_bytes())?,
            "field {field}"
        );
    }
    Ok(())
}

#[test]
fn normalized_aliases_and_block_attempt_share_original_caller_identity() -> TestResult {
    let baseline = encode(ClaimDueInput::new(b"job", b"worker"), options(), 100)?;
    let mut input = ClaimDueInput::new(b"job", b"worker");
    input.states = ClaimStatesInput::One(b"aNy");
    input.partitions = ClaimPartitionsInput::One(b"aUtO");
    input.lease_ms = Some(30_000);
    input.limit = Some(1);
    input.block_ms = Some(0);
    let other = encode(input, options(), 100)?;
    assert_eq!(baseline.as_bytes(), other.as_bytes());
    Ok(())
}

#[test]
fn canonical_parent_roundtrip_preserves_mixed_any_empty_projection_and_borrowed_bytes() -> TestResult
{
    use wf_domain::claim::caller::{ClaimCallerLimits, ClaimPartitionMode, ClaimValuesMode};
    use wf_ports::log::prepare_atomic_claim_request;
    let names: [&[u8]; 4] = [b"q", b"ANY", b"q", b"z\0"];
    let mut input = ClaimDueInput::new(&[0xff, 0], b"worker");
    input.states = ClaimStatesInput::Many(&names);
    input.partitions = ClaimPartitionsInput::Many(&[]);
    input.values = ClaimValuesInput::Names(&[]);
    input.governance_limit_scope = Some(b"");
    input.requested_now_ms = Some(u64::MAX);
    let frame = encode(input, options(), 100)?;
    let prepared = prepare_atomic_claim_request(
        frame.as_bytes(),
        super::limits(),
        ClaimCallerLimits::default(),
        1024 * 1024,
    )?;
    let call = prepared.caller(ClaimCallerLimits::default())?;
    assert_eq!(call.states(), (true, [b"q".as_slice(), b"z\0"].as_slice()));
    assert_eq!(call.partitions(), (ClaimPartitionMode::Only, [].as_slice()));
    assert_eq!(call.values(), (ClaimValuesMode::Only, [].as_slice()));
    assert_eq!(call.governance_limit_scope(), Some(b"".as_slice()));
    assert_eq!(call.requested_now_ms(), Some(u64::MAX));
    let offset = frame
        .as_bytes()
        .windows(2)
        .position(|bytes| bytes == [0xff, 0])
        .ok_or("type offset")?;
    assert_eq!(
        call.workflow_type().as_ptr(),
        frame.as_bytes()[offset..].as_ptr()
    );
    Ok(())
}

#[test]
fn derived_reclaim_phases_keep_the_same_real_parent_header_and_consumed_budgets() -> TestResult {
    use wf_domain::{
        claim::{
            apply::ClaimStates,
            caller::ClaimCallerLimits,
            coordination::{
                ClaimCoordinationLimits, ClaimPhaseCompletion, advance_claim_schedule,
                plan_claim_schedule,
            },
        },
        workflow::{control::ControlState, request::gate::PointGateView},
    };
    use wf_ports::log::{
        AtomicClaimInspection, inspect_atomic_claim_point, prepare_atomic_claim_request,
    };
    use wf_types::version::{AppliedPosition, CommittedTime, HlcTimestamp, LogIdentity};
    let keys: [&[u8]; 1] = [b"p"];
    let mut input = ClaimDueInput::new(b"job", b"worker");
    input.states = ClaimStatesInput::One(b"q");
    input.partitions = ClaimPartitionsInput::Many(&keys);
    input.limit = Some(4);
    input.requested_now_ms = Some(u64::MAX);
    let frame = encode(input, options(), 100)?;
    let prepared = prepare_atomic_claim_request(
        frame.as_bytes(),
        super::limits(),
        ClaimCallerLimits::default(),
        1024 * 1024,
    )?;
    let call = prepared.caller(ClaimCallerLimits::default())?;
    let initial = ControlState::initial(CommittedTime::new(HlcTimestamp::new(0, 0)?, 0)?);
    let log = LogIdentity::new(1, 7, AppliedPosition::new(0));
    let AtomicClaimInspection::Fresh(fresh) = inspect_atomic_claim_point(
        &prepared,
        log,
        PointGateView {
            scope: prepared.scope(),
            control: initial,
            outcome: None,
            retained_nonempty: false,
        },
        super::limits(),
    )?
    else {
        return Err("fresh parent".into());
    };
    let mut schedule = plan_claim_schedule(&call, ClaimCoordinationLimits::default())?;
    let running = schedule.phase()?.ok_or("first running")?;
    let envelope = fresh.envelope(&running, (0, 0, 0), super::limits())??;
    assert_eq!(envelope.header.intent.version(), 20);
    assert_eq!(envelope.header.identity.prepared(), prepared.digest());
    assert_eq!(envelope.header.encoded_entry, frame.as_bytes());
    assert_eq!(envelope.query.limit, 1);
    assert!(
        matches!(envelope.query.states, ClaimStates::Only(names) if names == [b"running".as_slice()])
    );
    assert_eq!(fresh.control().execution_time().get(), 100);
    advance_claim_schedule(
        &mut schedule,
        ClaimPhaseCompletion::Accepted {
            phase: 0,
            claimed: 1,
        },
    )?;
    let normal = schedule.phase()?.ok_or("normal")?;
    let envelope = fresh.envelope(&normal, (1, 2, 1), super::limits())??;
    assert_eq!(
        (
            envelope.query.limit,
            envelope.scan_budget,
            envelope.head_budget,
            envelope.first_ordinal
        ),
        (3, 19, 8, 1)
    );
    assert_eq!(envelope.query.exclude_states, [b"running".as_slice()]);
    assert_eq!(envelope.header.intent, prepared.intent());
    Ok(())
}
