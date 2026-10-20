//! Independent whole VALUE, original-intent and retained-metadata recipes.

use super::*;

pub(super) const CORE_PATCH_PARENT_MINIMAL: &[u8] =
    b"FWPV\0\x02\0\0\0\0\0\0\0\x01\0\0\0\x16FWPS\0\x02\0\0\0\x01r\0\0\0\0\0\0\0\0\0\0\0";

pub(super) const CORE_PATCH_PARENT_VALUES: &[(&str, &str)] = &[
    (
        "46575056000200000000000000010000016c4657505300020000000172010100000000000000000100000000000000010101000001030000000000000001000000000000000200010000000178010100000000000000010100000001000000010000000173010101000000010100000000000000010100000001010000000000000001000000020000000161000000017a01000000016d0101020101000000020000000161000000017a0100000000010001010001020100000001000000016c00000000000000000100010000000000000001010000000100000001620000000000000001000000000000000101010100000000000000010100000001750100000001000000016300000000000000010000000000000001010000000000000001010000000000000001013201000000000000000101000000000000000101000000000000000101000000020000000161000000017a01010000000100000001610100000001000000017301000000010000000175010000000000000001",
        "0f61166723f2a7466e67d447059dc25128c8b5aec270a9a626e3edbeeb03b07f",
    ),
    (
        "46575056000200000000000000020000016c4657505300020000000172010100000000000000000100000000000000010101000001030000000000000001000000000000000200010000000178010100000000000000010100000001000000010000000173010101000000010100000000000000010100000001010000000000000001000000020000000161000000017a01000000016d0101020101000000020000000161000000017a0100000000010001010001020100000001000000016c00000000000000000100010000000000000001010000000100000001620000000000000001000000000000000101010100000000000000010100000001750100000001000000016300000000000000010000000000000001010000000000000001010000000000000001013201000000000000000101000000000000000101000000000000000101000000020000000161000000017a01010000000100000001610100000001000000017301000000010000000175010000000000000001",
        "312cba420877fa192c422d52331953ad1fb85d48df8ed97584a1211d7971f94b",
    ),
    (
        "465750560002001fffffffffffff0000016c4657505300020000000172010100000000000000000100000000000000010101000001030000000000000001000000000000000200010000000178010100000000000000010100000001000000010000000173010101000000010100000000000000010100000001010000000000000001000000020000000161000000017a01000000016d0101020101000000020000000161000000017a0100000000010001010001020100000001000000016c00000000000000000100010000000000000001010000000100000001620000000000000001000000000000000101010100000000000000010100000001750100000001000000016300000000000000010000000000000001010000000000000001010000000000000001013201000000000000000101000000000000000101000000000000000101000000020000000161000000017a01010000000100000001610100000001000000017301000000010000000175010000000000000001",
        "5295745e92df515f8c929753e44732b72b16cd30aa9e64f22e83f8befedc01cb",
    ),
    (
        "465750560002000000000000000200000194465750530002000000017201010000000000000000010000000000000001010100000103000000000000000100000000000000020001000000017801010000000000000001010000000100000003000000067175657565640001000001000000046465616400000000000672657669657701010000000000000173010101000000010100000000000000010100000001010000000000000001000000020000000161000000017a01000000016d0101020101000000020000000161000000017a0100000000010001010001020100000001000000016c00000000000000000100010000000000000001010000000100000001620000000000000001000000000000000101010100000000000000010100000001750100000001000000016300000000000000010000000000000001010000000000000001010000000000000001013201000000000000000101000000000000000101000000000000000101000000020000000161000000017a01010000000100000001610100000001000000017301000000010000000175010000000000000001",
        "545afed8ccda9c829f9cf4f04ddc94e3d3b1222008b03be20de35ac9c79e7492",
    ),
];

pub(super) const CORE_PATCH_PARENT_BUSINESS: &[(PolicyError, &[u8])] = &[
    (
        PolicyError::InvalidVersion,
        b"ERR flow policy version must be a non-empty string or non-negative integer",
    ),
    (
        PolicyError::InvalidMaxActive,
        b"ERR flow max_active_ms must be between 1 and 31536000000 or infinity",
    ),
    (
        PolicyError::InvalidStateMode,
        b"ERR flow state mode must be parallel or fifo",
    ),
    (
        PolicyError::InvalidStatePolicy,
        b"ERR flow state policy must be a map or keyword list",
    ),
    (
        PolicyError::InvalidGeneration,
        b"ERR invalid flow policy generation",
    ),
    (
        PolicyError::StaleGeneration,
        b"ERR stale flow policy generation",
    ),
    (
        PolicyError::GenerationExhausted,
        b"ERR flow policy generation exhausted",
    ),
    (
        PolicyError::Retention(RetentionError::Ttl),
        b"ERR flow retention ttl_ms must be between 1 and 31536000000",
    ),
    (
        PolicyError::Retention(RetentionError::History),
        b"ERR flow retention history_max_events is outside configured bounds",
    ),
    (
        PolicyError::Domain(DomainError::EmptyValue(InputField::Type)),
        b"ERR flow type must be a non-empty string",
    ),
    (
        PolicyError::Domain(DomainError::InvalidRetryPolicy(
            RetryPolicyField::MaxRetries,
        )),
        b"ERR flow retry max_retries must be between 0 and 1000",
    ),
    (
        PolicyError::Domain(DomainError::InvalidRetryPolicy(
            RetryPolicyField::BaseMillis,
        )),
        b"ERR flow retry base_ms must be between 0 and 2592000000",
    ),
    (
        PolicyError::Domain(DomainError::InvalidRetryPolicy(RetryPolicyField::MaxMillis)),
        b"ERR flow retry max_ms must be between 0 and 2592000000",
    ),
    (
        PolicyError::Domain(DomainError::InvalidRetryPolicy(
            RetryPolicyField::JitterPercent,
        )),
        b"ERR flow retry jitter_pct must be between 0 and 100",
    ),
    (
        PolicyError::Domain(DomainError::InvalidRetryPolicy(
            RetryPolicyField::ExhaustedTo,
        )),
        b"ERR flow retry exhausted_to must be a non-empty string",
    ),
    (
        PolicyError::Domain(DomainError::InvalidRetryPolicy(
            RetryPolicyField::RunningExhaustion,
        )),
        b"ERR flow retry exhausted_to cannot be running",
    ),
    (
        PolicyError::Domain(DomainError::InvalidRetryPolicy(RetryPolicyField::State)),
        b"ERR flow policy state must be a non-empty string",
    ),
    (
        PolicyError::Domain(DomainError::InvalidRetryPolicy(
            RetryPolicyField::RunningState,
        )),
        b"ERR flow policy state cannot be running",
    ),
];

pub(super) fn core_patch_parent_scope() -> TestResult<RequestScope> {
    Ok(RequestScope::new(
        TenantId::new(b"t".to_vec())?,
        GroupId::new(b"g".to_vec())?,
        RecoveryId::new(b"r".to_vec())?,
    ))
}

pub(super) fn core_patch_parent_stamp(time: u64) -> TestResult<CommittedTime> {
    Ok(CommittedTime::new(HlcTimestamp::new(time, 0)?, time)?)
}

pub(super) const fn core_patch_parent_log(index: u64) -> LogIdentity {
    LogIdentity::new(1, 7, AppliedPosition::new(index))
}

pub(super) fn core_patch_parent_view<'a>(
    scope: &'a RequestScope,
    control: ControlState,
    outcomes: &'a BTreeMap<RequestId, RetainedOutcome>,
    policies: &'a BTreeMap<Vec<u8>, Result<StoredPolicyValue, ApplyError>>,
    active: &'a BTreeMap<Vec<u8>, Result<u64, ApplyError>>,
    done: &'a BTreeMap<Vec<u8>, Result<u64, ApplyError>>,
) -> CompleteCorePolicyPatchApplyView<'a> {
    CompleteCorePolicyPatchApplyView {
        gate: GateView {
            scope,
            control,
            outcomes,
        },
        revision: 7,
        policies,
        active_migrations: active,
        completed_migrations: done,
        history_max_events_limit: Ok(1_000_000),
        outcome_retention_ms: Ok(100),
    }
}

pub(super) fn core_patch_parent_frame(
    case: usize,
    request: &[u8],
    time: u64,
) -> TestResult<Vec<u8>> {
    let original = decode_core_patch_literal(FROZEN_CORE_PATCH_FRAMES[case].0)?;
    core_patch_prepare_literal_frame(
        &original[83..],
        [b"t", b"g", b"r", request],
        core_patch_parent_stamp(time)?,
    )
}

pub(super) fn core_patch_parent_value(
    case: usize,
    limits: PolicySchemaLimits,
    charge: &mut PolicySchemaCharge,
) -> TestResult<StoredPolicyValue> {
    let expected = decode_core_patch_literal(CORE_PATCH_PARENT_VALUES[case].0)?;
    let hash = decode_core_patch_literal(CORE_PATCH_PARENT_VALUES[case].1)?;
    assert_eq!(
        <[u8; 32]>::from(Sha256::digest(&expected)).as_slice(),
        hash.as_slice()
    );
    let value = decode_complete_policy_value_v2(&expected, limits, charge)?;
    let generation = match case {
        0 => 1,
        1 | 3 => 2,
        2 => MAX_POLICY_GENERATION,
        _ => return Err("unknown VALUE literal".into()),
    };
    let data = if case == 3 {
        core_patch_parent_source8_data()
    } else {
        present_data()
    };
    assert_eq!(value.generation(), generation);
    assert!(value.data() == &data);
    assert_eq!(value.encoded(), expected.as_slice());
    Ok(StoredPolicyValue::Complete(Box::new(value)))
}

pub(super) fn core_patch_parent_rows(
    generation: u64,
    rows: &BTreeMap<Vec<u8>, Vec<u8>>,
) -> TestResult<Vec<u8>> {
    let anchor = decode_core_patch_literal(CORE_PATCH_PARENT_VALUES[0].0)?;
    let mut expected = anchor[..92].to_vec();
    expected[6..14].copy_from_slice(&generation.to_be_bytes());
    expected.extend_from_slice(&u32::try_from(rows.len())?.to_be_bytes());
    for row in rows.values() {
        expected.extend_from_slice(row);
    }
    expected.extend_from_slice(&anchor[130..]);
    let snapshot_len = u32::try_from(expected.len() - 18)?;
    expected[14..18].copy_from_slice(&snapshot_len.to_be_bytes());
    Ok(expected)
}

pub(super) fn assert_core_patch_parent_control(
    plan: &CompleteCorePolicyPatchRequestPlan<'_>,
    view: CompleteCorePolicyPatchApplyView<'_>,
    frame: &[u8],
    index: u64,
    time: u64,
    replay: bool,
) -> TestResult {
    let stamp = core_patch_parent_stamp(time)?;
    let identity = EntryIdentity::new(
        core_patch_parent_log(index),
        PreparedDigest::new(Sha256::digest(frame).into()),
    );
    let expected = if replay {
        view.gate.control
    } else {
        let time = CommittedTime::new(
            view.gate.control.time().hlc().max(stamp.hlc()),
            view.gate.control.time().wall().max(stamp.wall()).get(),
        )?;
        ControlState::restored(time, identity, EntryStamp::Stamped(stamp))?
    };
    assert_eq!(plan.expected_revision(), view.revision);
    assert_eq!(plan.base_control(), view.gate.control);
    assert_eq!(plan.control().next(), expected);
    assert_eq!(plan.control().is_log_replay(), replay);
    assert_eq!(
        plan.control().execution_time(),
        expected.time().hlc().physical()
    );
    Ok(())
}

pub(super) fn assert_core_patch_parent_outcome(
    outcome: &RetainedOutcome,
    expected: &[u8],
    status: RetainedStatus,
    intent: &[u8],
    metadata: (&[u8], u64, u64),
) -> TestResult {
    let (request, time, index) = metadata;
    let deadline = time.checked_add(100).ok_or("invalid expected expiry")?;
    assert_eq!(outcome.scope().tenant().as_bytes(), b"t");
    assert_eq!(outcome.scope().group().as_bytes(), b"g");
    assert_eq!(outcome.scope().recovery().as_bytes(), b"r");
    assert_eq!(outcome.request().as_bytes(), request);
    assert_eq!(outcome.intent().version(), 2);
    assert_eq!(
        outcome.intent().digest(),
        &<[u8; 32]>::from(Sha256::digest(intent))
    );
    assert_eq!(outcome.response_version(), 1);
    assert_eq!(outcome.status(), status);
    assert_eq!(outcome.response(), expected);
    assert_eq!(outcome.original_position(), core_patch_parent_log(index));
    assert_eq!(outcome.original_time().get(), time);
    assert_eq!(outcome.expires_at().get(), deadline);
    Ok(())
}

pub(super) fn assert_core_patch_parent_accepted(
    plan: &CompleteCorePolicyPatchRequestPlan<'_>,
    data: &CompletePolicyData,
    expected: &[u8],
    stored: u64,
    frame: &[u8],
    metadata: (&[u8], u64, u64),
) -> TestResult {
    let DispatchDecision::Fresh((Some((observed, StoredPolicyValue::Complete(value))), outcome)) =
        plan.decision()
    else {
        return Err("expected complete accepted Fresh owners".into());
    };
    assert_eq!(*observed, stored);
    assert_eq!(
        value.generation(),
        u64::from_be_bytes(expected[6..14].try_into()?)
    );
    assert!(value.data() == data);
    assert_eq!(value.encoded(), expected);
    assert_core_patch_parent_outcome(
        outcome,
        expected,
        RetainedStatus::Accepted,
        &frame[82 + metadata.0.len()..],
        metadata,
    )?;
    Ok(())
}

pub(super) fn core_patch_parent_business_frame(case: usize) -> TestResult<Vec<u8>> {
    if let Some(original) = match case {
        0 => Some(12),
        2 => Some(14),
        3 => Some(5),
        4 => Some(2),
        9 => Some(15),
        14 => Some(13),
        _ => None,
    } {
        return core_patch_parent_frame(original, b"q", 0);
    }
    let minimal = decode_core_patch_literal(FROZEN_CORE_PATCH_FRAMES[0].0)?;
    let mut intent = minimal[83..].to_vec();
    let payload = match case {
        1 => (45, "010000000000000000"),
        7 => (47, "0101000000000000000000"),
        8 => (47, "010001000f4241"),
        10 => (46, "010103e90000"),
        11 => (46, "0100010001000000009a7ec801000000"),
        12 => (46, "010001000001000000009a7ec8010000"),
        13 => (46, "010001000000016500"),
        15 => (46, "010000010000000772756e6e696e67"),
        _ => (0, ""),
    };
    if payload.0 != 0 {
        drop(intent.splice(payload.0..=payload.0, decode_core_patch_literal(payload.1)?));
    } else if case == 5 {
        let guarded = decode_core_patch_literal(FROZEN_CORE_PATCH_FRAMES[1].0)?;
        intent = guarded[83..].to_vec();
        intent[39..47].copy_from_slice(&1_u64.to_be_bytes());
    } else if case == 16 || case == 17 {
        intent[48..52].copy_from_slice(&1_u32.to_be_bytes());
        let name: &[u8] = if case == 16 { b"" } else { b"running" };
        intent.extend_from_slice(&u32::try_from(name.len())?.to_be_bytes());
        intent.extend_from_slice(name);
        intent.extend_from_slice(&[1, 0, 0, 0]);
    } else if case != 6 {
        return Err("unknown business literal recipe".into());
    }
    core_patch_prepare_literal_frame(
        &intent,
        [b"t", b"g", b"r", b"q"],
        core_patch_parent_stamp(0)?,
    )
}

pub(super) fn assert_core_patch_parent_presence(
    limits: PolicySchemaLimits,
    charge: &mut PolicySchemaCharge,
) -> TestResult {
    let scope = core_patch_parent_scope()?;
    let control = ControlState::initial(core_patch_parent_stamp(0)?);
    let outcomes = BTreeMap::new();
    let migrations = BTreeMap::new();
    let anchor = decode_core_patch_literal(CORE_PATCH_PARENT_VALUES[0].0)?;
    let policies = BTreeMap::from([(
        b"r".to_vec(),
        Ok(core_patch_parent_value(0, limits, charge)?),
    )]);
    let minimal = decode_core_patch_literal(FROZEN_CORE_PATCH_FRAMES[0].0)?;
    for case in 0..11 {
        let mut intent = minimal[83..].to_vec();
        let mut data = present_data();
        let mut rows = BTreeMap::from([(b"s".to_vec(), anchor[96..130].to_vec())]);
        intent = core_patch_parent_presence_intent(case, intent)?;
        if case == 5 {
            rows.remove(b"s".as_slice());
            data.states.remove(b"s".as_slice());
        } else if case == 7 || case == 9 {
            let row = rows.get_mut(b"s".as_slice()).ok_or("missing fixed s row")?;
            let state = data
                .states
                .get_mut(b"s".as_slice())
                .ok_or("missing independent s data")?;
            let range = if case == 7 {
                state.retry = None;
                7..11
            } else {
                state.retention = None;
                11..26
            };
            drop(row.splice(range, [0]));
        }
        let mut expected = core_patch_parent_rows(2, &rows)?;
        if case == 1 {
            data.retry = None;
            drop(expected.splice(48..77, [0]));
        }
        if case == 3 {
            data.retention = None;
            drop(expected.splice(77..92, [0]));
        }
        let length = u32::try_from(expected.len() - 18)?;
        expected[14..18].copy_from_slice(&length.to_be_bytes());
        let frame = core_patch_prepare_literal_frame(
            &intent,
            [b"t", b"g", b"r", b"q"],
            core_patch_parent_stamp(0)?,
        )?;
        let view = core_patch_parent_view(
            &scope,
            control,
            &outcomes,
            &policies,
            &migrations,
            &migrations,
        );
        let plan = core_patch_parent_apply(view, &frame, 0, limits, charge)?;
        assert_core_patch_parent_accepted(&plan, &data, &expected, 1, &frame, (b"q", 0, 0))?;
        assert_core_patch_parent_control(&plan, view, &frame, 0, 0, false)?;
        assert_eq!(
            core_patch_parent_stored(&policies)?.encoded(),
            anchor.as_slice()
        );
        assert!(outcomes.is_empty() && migrations.is_empty());
    }
    Ok(())
}

pub(super) fn core_patch_parent_source8_data() -> CompletePolicyData {
    let mut data = present_data();
    data.states.insert(
        b"queued".to_vec(),
        CompleteStatePolicyData {
            mode: None,
            retry: Some(RetryOverride {
                exhausted_to: Some(b"dead".to_vec()),
                ..RetryOverride::default()
            }),
            retention: None,
            governance: None,
        },
    );
    data.states.insert(
        b"review".to_vec(),
        CompleteStatePolicyData {
            mode: Some(StateMode::Fifo),
            retry: None,
            retention: None,
            governance: None,
        },
    );
    data
}

pub(super) fn assert_core_patch_parent_current_restoration() -> TestResult {
    let limits = profile(11_793)?;
    let mut charge = PolicySchemaCharge::new(limits);
    let current = decode_complete_policy_value_v2(CORE_PATCH_PARENT_MINIMAL, limits, &mut charge)?;
    let current = StoredPolicyValue::Complete(Box::new(current));
    assert_eq!((charge.g(), charge.p()), (2_153, 0));
    let scope = core_patch_parent_scope()?;
    let outcomes = BTreeMap::new();
    let policies = BTreeMap::from([(b"r".to_vec(), Ok(current))]);
    let poison = BTreeMap::from([(b"r".to_vec(), Err(ApplyError::AllocationFailed))]);
    let frame = core_patch_parent_frame(0, b"q", 0)?;
    let mut view = core_patch_parent_view(
        &scope,
        ControlState::initial(core_patch_parent_stamp(0)?),
        &outcomes,
        &policies,
        &poison,
        &poison,
    );
    view.history_max_events_limit = Err(ApplyError::AllocationFailed);
    view.outcome_retention_ms = Err(ApplyError::AllocationFailed);
    assert_eq!(
        core_patch_parent_apply(view, &frame, 0, limits, &mut charge)
            .err()
            .ok_or("expected paid current restoration refusal")?,
        CompleteCorePolicyPatchApplyError::Request(
            PolicySchemaError::WorkBytes {
                bytes: 11_794,
                maximum: 11_793
            }
            .into()
        )
    );
    assert_eq!(
        (charge.g(), charge.p(), charge.total()),
        (11_666, 0, 11_666)
    );
    assert_eq!(
        core_patch_parent_stored(&policies)?.encoded(),
        CORE_PATCH_PARENT_MINIMAL
    );
    assert!(outcomes.is_empty());

    Ok(())
}

fn core_patch_parent_presence_intent(case: usize, mut intent: Vec<u8>) -> TestResult<Vec<u8>> {
    let top = match case {
        1 => Some((46, &[2][..])),
        2 => Some((46, &[1, 0, 0, 0][..])),
        3 => Some((47, &[2][..])),
        4 => Some((47, &[1, 0, 0][..])),
        _ => None,
    };
    if let Some((offset, payload)) = top {
        drop(intent.splice(offset..=offset, payload.iter().copied()));
    } else if case >= 5 {
        intent[48..52].copy_from_slice(&1_u32.to_be_bytes());
        intent.extend_from_slice(b"\0\0\0\x01s");
        let payload = match case {
            5 => &[0][..],
            6 => &[1, 0, 0, 0][..],
            7 => &[1, 0, 2, 0][..],
            8 => &[1, 0, 1, 0, 0, 0, 0][..],
            9 => &[1, 0, 0, 2][..],
            10 => &[1, 0, 0, 1, 0, 0][..],
            _ => return Err("unknown state presence".into()),
        };
        intent.extend_from_slice(payload);
    }

    Ok(intent)
}

pub(super) fn core_patch_parent_apply<'view>(
    view: CompleteCorePolicyPatchApplyView<'view>,
    frame: &[u8],
    index: u64,
    limits: PolicySchemaLimits,
    charge: &mut PolicySchemaCharge,
) -> Result<CompleteCorePolicyPatchRequestPlan<'view>, CompleteCorePolicyPatchApplyError> {
    decide_complete_core_policy_patch_frame(
        view,
        frame,
        core_patch_parent_log(index),
        limits,
        AdmissionLimits::default(),
        charge,
    )
}

pub(super) fn core_patch_parent_writer_step(
    view: CompleteCorePolicyPatchApplyView<'_>,
    frame: &[u8],
    expected: (&CompletePolicyData, &[u8], u64),
    metadata: (&[u8], u64),
    limits: PolicySchemaLimits,
    charge: &mut PolicySchemaCharge,
) -> TestResult<(ControlState, CompleteCorePolicyPatchFresh)> {
    let plan = core_patch_parent_apply(view, frame, metadata.1, limits, charge)?;
    assert_core_patch_parent_accepted(
        &plan,
        expected.0,
        expected.1,
        expected.2,
        frame,
        (metadata.0, metadata.1, metadata.1),
    )?;
    assert_core_patch_parent_control(&plan, view, frame, metadata.1, metadata.1, false)?;
    let (revision, base, control, decision) = plan.into_parts();
    assert_eq!((revision, base), (view.revision, view.gate.control));
    let DispatchDecision::Fresh(fresh) = decision else {
        return Err("writer expected complete Fresh bundle".into());
    };
    Ok((control.next(), fresh))
}

pub(super) fn assert_core_patch_parent_full_max(
    limits: PolicySchemaLimits,
    charge: &mut PolicySchemaCharge,
) -> TestResult {
    let scope = core_patch_parent_scope()?;
    let outcomes = BTreeMap::new();
    let policies = BTreeMap::from([(
        b"r".to_vec(),
        Ok(core_patch_parent_value(0, limits, charge)?),
    )]);
    let active = BTreeMap::from([(b"r".to_vec(), Ok(MAX_POLICY_GENERATION - 1))]);
    let done = BTreeMap::new();
    let frame = core_patch_parent_frame(0, b"q", 0)?;
    let expected = decode_core_patch_literal(CORE_PATCH_PARENT_VALUES[2].0)?;
    let view = core_patch_parent_view(
        &scope,
        ControlState::initial(core_patch_parent_stamp(0)?),
        &outcomes,
        &policies,
        &active,
        &done,
    );
    let log = core_patch_parent_log(0);
    let bounds = AdmissionLimits::new(1_048_576, 768, 1_000)?;
    let plan = decide_complete_core_policy_patch_frame(view, &frame, log, limits, bounds, charge)?;
    assert_core_patch_parent_accepted(&plan, &present_data(), &expected, 1, &frame, (b"q", 0, 0))?;
    assert_core_patch_parent_control(&plan, view, &frame, 0, 0, false)?;
    Ok(())
}
