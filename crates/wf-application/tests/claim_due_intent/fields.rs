use super::{
    ClaimDueInput, ClaimPartitionsInput, ClaimReplyMode, ClaimStatesInput, ClaimValuesInput,
    IntentLimits, TenantId, TestResult, fixture_input, intent, normalized,
};
use wf_application::commands::prepare_claim_due_intent;
use wf_application::error::PreparationError;
use wf_domain::workflow::request::RequestScope;
use wf_ports::consensus::{
    PreparedClaimPhase, PreparedClaimQuery, PreparedOperation, PreparedRequest,
};
use wf_ports::log::{CodecLimits, decode_prepared, encode_prepared};
use wf_types::identity::{GroupId, RecoveryId, RequestId};
use wf_types::version::{CommittedTime, HlcTimestamp};

#[test]
fn every_caller_field_changes_public_claim_identity() -> TestResult {
    let tenant = TenantId::new(b"\0\xff".to_vec())?;
    let expected = intent(&tenant, fixture_input("defaults")?)?;
    for field in [
        "type",
        "worker",
        "states",
        "partitions",
        "values",
        "lease",
        "limit",
        "priority",
        "reclaim",
        "ratio",
        "jobs",
        "compact",
        "attrs",
        "state",
        "state_attrs",
        "payload",
        "full",
        "cap",
        "governance_scope",
        "governance_shard",
        "requested_now",
    ] {
        let mut input = fixture_input("defaults")?;
        match field {
            "type" => input.workflow_type = b"t\0",
            "worker" => input.worker = b"w\0",
            "states" => input.states = ClaimStatesInput::One(b"waiting"),
            "partitions" => input.partitions = ClaimPartitionsInput::One(b"p"),
            "values" => input.values = ClaimValuesInput::All,
            "lease" => input.lease_ms = Some(1),
            "limit" => input.limit = Some(2),
            "priority" => input.priority = Some(0),
            "reclaim" => input.reclaim_expired = Some(false),
            "ratio" => input.reclaim_ratio = Some(0),
            "jobs" => input.reply_mode = Some(ClaimReplyMode::Jobs),
            "compact" => input.reply_mode = Some(ClaimReplyMode::Compact),
            "attrs" => input.reply_mode = Some(ClaimReplyMode::CompactAttrs),
            "state" => input.reply_mode = Some(ClaimReplyMode::CompactState),
            "state_attrs" => input.reply_mode = Some(ClaimReplyMode::CompactStateAttrs),
            "payload" => input.payload = Some(false),
            "full" => input.full = Some(false),
            "cap" => input.payload_max_bytes = Some(0),
            "governance_scope" => input.governance_limit_scope = Some(b"\0\xff"),
            "governance_shard" => input.governance_shard_id = Some(0),
            "requested_now" => input.requested_now_ms = Some(0),
            _ => return Err("unknown caller field control".into()),
        }
        assert_ne!(intent(&tenant, input)?, expected, "collapsed {field}");
    }
    assert_ne!(
        intent(
            &TenantId::new(b"\xff\0".to_vec())?,
            fixture_input("defaults")?
        )?,
        expected
    );
    Ok(())
}

#[test]
fn fixed_defaults_and_source_aliases_compare_equally() -> TestResult {
    let tenant = TenantId::new(b"\0\xff".to_vec())?;
    let expected = intent(&tenant, fixture_input("defaults")?)?;
    for name in ["explicit_fixed_defaults", "auto", "values_omitted"] {
        assert_eq!(intent(&tenant, fixture_input(name)?)?, expected);
    }
    for (alias, name) in [
        (b"aUtO".as_slice(), "auto"),
        (b"aNy", "any"),
        (b"gLoBaL", "global"),
    ] {
        let mut input = fixture_input("defaults")?;
        input.partitions = ClaimPartitionsInput::One(alias);
        assert_eq!(
            intent(&tenant, input)?,
            intent(&tenant, fixture_input(name)?)?
        );
    }
    for states in [
        ClaimStatesInput::One(b"aNy"),
        ClaimStatesInput::Many(&[b"ANY", b"any"]),
    ] {
        let mut input = fixture_input("defaults")?;
        input.states = states;
        assert_eq!(intent(&tenant, input)?, expected);
    }
    let mut input = fixture_input("defaults")?;
    input.states = ClaimStatesInput::Many(&[b"ready" as &[u8], b"queued"]);
    assert_eq!(
        intent(&tenant, input)?,
        intent(&tenant, fixture_input("state_keep_last")?)?
    );
    let mut input = fixture_input("defaults")?;
    input.partitions = ClaimPartitionsInput::Many(&[b"p2", b"p1"]);
    assert_eq!(
        intent(&tenant, input)?,
        intent(&tenant, fixture_input("partition_keep_first")?)?
    );
    let mut duplicated = fixture_input("defaults")?;
    duplicated.values = ClaimValuesInput::Names(&[b"x", b"y", b"x"]);
    let mut unique = fixture_input("defaults")?;
    unique.values = ClaimValuesInput::Names(&[b"x", b"y"]);
    assert_eq!(intent(&tenant, duplicated)?, intent(&tenant, unique)?);
    let mut input = fixture_input("state_keep_last")?;
    input.states = ClaimStatesInput::Many(&[b"queued" as &[u8], b"ready"]);
    assert_ne!(
        intent(&tenant, input)?,
        intent(&tenant, fixture_input("state_keep_last")?)?
    );
    let mut input = fixture_input("partition_keep_first")?;
    input.partitions = ClaimPartitionsInput::Many(&[b"p1", b"p2"]);
    assert_ne!(
        intent(&tenant, input)?,
        intent(&tenant, fixture_input("partition_keep_first")?)?
    );
    let mut input = fixture_input("defaults")?;
    input.values = ClaimValuesInput::Names(&[b"y", b"x"]);
    let mut unique = fixture_input("defaults")?;
    unique.values = ClaimValuesInput::Names(&[b"x", b"y"]);
    assert_ne!(intent(&tenant, input)?, intent(&tenant, unique)?);
    let mut input = fixture_input("defaults")?;
    input.full = Some(false);
    input.payload = Some(true);
    assert_eq!(intent(&tenant, input)?, expected);
    Ok(())
}

#[test]
fn config_absence_empty_zero_and_requested_time_are_distinct() -> TestResult {
    let tenant = TenantId::new(b"\0\xff".to_vec())?;
    let mut seen = vec![intent(&tenant, fixture_input("defaults")?)?];
    for field in [
        "cap",
        "now",
        "priority",
        "governance_shard",
        "governance_scope",
        "lease",
        "limit",
        "ratio",
    ] {
        for value in [0, u64::MAX] {
            let mut input = fixture_input("defaults")?;
            match field {
                "cap" => input.payload_max_bytes = Some(value),
                "now" => input.requested_now_ms = Some(value),
                "priority" => input.priority = Some(value),
                "governance_shard" => input.governance_shard_id = Some(value),
                "governance_scope" => {
                    input.governance_limit_scope =
                        Some([b"".as_slice(), b"\0\xff".as_slice()][usize::from(value != 0)]);
                }
                "lease" => input.lease_ms = Some(value),
                "limit" => input.limit = Some(value),
                "ratio" => input.reclaim_ratio = Some(value),
                _ => return Err("unknown optional distinction control".into()),
            }
            let value = intent(&tenant, input)?;
            assert!(!seen.contains(&value), "collapsed {field}");
            seen.push(value);
        }
    }
    let mut input = fixture_input("defaults")?;
    input.payload_max_bytes = Some(65_536);
    let capped = intent(&tenant, input)?;
    assert!(!seen.contains(&capped));
    for field in [
        "empty_states",
        "empty_state_name",
        "mixed_any",
        "empty_partitions",
        "empty_partition_name",
        "literal_any_partition",
        "empty_values",
        "empty_value_name",
        "all_values",
    ] {
        let mut input = fixture_input("defaults")?;
        match field {
            "empty_states" => input.states = ClaimStatesInput::Many(&[]),
            "empty_state_name" => input.states = ClaimStatesInput::One(b""),
            "mixed_any" => {
                input.states = ClaimStatesInput::Many(&[b"ANY" as &[u8], b"waiting"]);
            }
            "empty_partitions" => input.partitions = ClaimPartitionsInput::Many(&[]),
            "empty_partition_name" => input.partitions = ClaimPartitionsInput::One(b""),
            "literal_any_partition" => input.partitions = ClaimPartitionsInput::Many(&[b"ANY"]),
            "empty_values" => input.values = ClaimValuesInput::Names(&[]),
            "empty_value_name" => input.values = ClaimValuesInput::Names(&[b""]),
            "all_values" => input.values = ClaimValuesInput::All,
            _ => return Err("unknown empty distinction control".into()),
        }
        let value = intent(&tenant, input)?;
        assert!(!seen.contains(&value), "collapsed {field}");
        seen.push(value);
    }
    Ok(())
}

#[test]
fn attempt_wait_and_server_preparation_do_not_change_public_identity() -> TestResult {
    let tenant = TenantId::new(b"\0\xff".to_vec())?;
    let expected = intent(&tenant, fixture_input("defaults")?)?;
    for wait in [None, Some(0), Some(1), Some(u64::MAX)] {
        let mut input = fixture_input("defaults")?;
        input.block_ms = wait;
        let request = normalized(input)?;
        assert_eq!(request.block_ms(), wait);
        assert_eq!(
            prepare_claim_due_intent(&tenant, &request, IntentLimits::default())?,
            expected
        );
    }
    let mut input = fixture_input("defaults")?;
    input.states = ClaimStatesInput::One(b"waiting");
    input.partitions = ClaimPartitionsInput::One(b"p");
    let caller = normalized(input)?;
    let public = prepare_claim_due_intent(&tenant, &caller, IntentLimits::default())?;
    let original_stamp = CommittedTime::new(HlcTimestamp::new(11, 2)?, 7)?;
    let changed_stamp = CommittedTime::new(HlcTimestamp::new(22, 3)?, 9)?;
    let mut prepared = PreparedRequest {
        scope: RequestScope::new(
            tenant.clone(),
            GroupId::new(b"G".to_vec())?,
            RecoveryId::new(b"R".to_vec())?,
        ),
        request: RequestId::new(b"Q".to_vec())?,
        intent: public,
        stamp: original_stamp,
        operation: PreparedOperation::ClaimPhase(PreparedClaimPhase {
            query: PreparedClaimQuery {
                workflow_type: caller.workflow_type().to_vec(),
                states: Some(vec![b"waiting".to_vec()]),
                exclude_states: Vec::new(),
                partitions: Some(vec![b"p".to_vec()]),
                priority: None,
                worker: caller.worker().to_vec(),
                lease_ms: caller.lease_ms(),
                limit: 1,
            },
            token_version: 1,
            first_ordinal: 0,
            scan_budget: 4,
            head_budget: 4,
        }),
    };
    let first = encode_prepared(&prepared, CodecLimits::default())?;
    let original = decode_prepared(first.as_bytes(), CodecLimits::default())?;
    prepared.stamp = changed_stamp;
    let PreparedOperation::ClaimPhase(phase) = &mut prepared.operation else {
        return Err("expected prepared claim phase".into());
    };
    phase.token_version = 0;
    phase.first_ordinal = u32::MAX;
    phase.scan_budget = 0;
    phase.head_budget = 0;
    let second = encode_prepared(&prepared, CodecLimits::default())?;
    let changed = decode_prepared(second.as_bytes(), CodecLimits::default())?;
    assert_ne!(first.as_bytes(), second.as_bytes());
    assert_ne!(original.digest(), changed.digest());
    assert_eq!(original.digest(), first.digest());
    assert_eq!(changed.digest(), second.digest());
    assert_eq!(original.request().stamp, original_stamp);
    assert_eq!(changed.request().stamp, changed_stamp);
    assert_eq!(original.request().intent, public);
    assert_eq!(changed.request().intent, public);
    assert_eq!(original.request().scope, changed.request().scope);
    assert_eq!(original.request().request, changed.request().request);
    match (&original.request().operation, &changed.request().operation) {
        (PreparedOperation::ClaimPhase(before), PreparedOperation::ClaimPhase(after)) => {
            assert!(before.query == after.query);
        }
        _ => return Err("decoded operation changed variant".into()),
    }
    assert_eq!(
        prepare_claim_due_intent(&tenant, &caller, IntentLimits::default())?,
        public
    );
    Ok(())
}

#[test]
fn normalized_opaque_spines_remain_borrowed_and_budgeted() -> TestResult {
    let tenant = TenantId::new(b"\0\xff".to_vec())?;
    let workflow_type = vec![b't', 255];
    let worker = vec![b'w', 255];
    let first = vec![0, 255];
    let last = vec![0, 255];
    let names = [first.as_slice(), last.as_slice()];
    let mut input = ClaimDueInput::new(&workflow_type, &worker);
    input.states = ClaimStatesInput::Many(&names);
    input.partitions = ClaimPartitionsInput::Many(&names);
    input.values = ClaimValuesInput::Names(&names);
    input.governance_limit_scope = Some(&first);
    let request = normalized(input)?;
    assert_eq!(request.workflow_type().as_ptr(), workflow_type.as_ptr());
    assert_eq!(request.worker().as_ptr(), worker.as_ptr());
    assert_eq!(request.states().1.len(), 1);
    assert_eq!(request.partitions().1.len(), 1);
    assert_eq!(request.values().1.len(), 1);
    assert_eq!(request.states().1[0].as_ptr(), last.as_ptr());
    assert_eq!(request.partitions().1[0].as_ptr(), first.as_ptr());
    assert_eq!(request.values().1[0].as_ptr(), first.as_ptr());
    assert_eq!(
        request
            .governance_limit_scope()
            .ok_or("missing scope")?
            .as_ptr(),
        first.as_ptr()
    );
    let owners: Vec<_> = (0_u64..20).map(u64::to_be_bytes).collect();
    let names: Vec<_> = owners.iter().map(<[u8; 8]>::as_slice).collect();
    let mut input = ClaimDueInput::new(b"t\xff", b"w\xff");
    input.states = ClaimStatesInput::Many(&names);
    input.partitions = ClaimPartitionsInput::Many(&names);
    input.values = ClaimValuesInput::Names(&names);
    let request = normalized(input)?;
    assert_eq!(
        request.states().1.len() + request.partitions().1.len() + request.values().1.len(),
        60
    );
    assert!(
        prepare_claim_due_intent(&tenant, &request, IntentLimits::new(16_384, 105, 5)?).is_ok()
    );
    assert_eq!(
        prepare_claim_due_intent(&tenant, &request, IntentLimits::new(16_384, 104, 5)?),
        Err(PreparationError::ValueLimit)
    );
    let large_worker = vec![7; 1_048_576];
    let mut input = ClaimDueInput::new(b"t\xff", &large_worker);
    input.states = ClaimStatesInput::Many(&names);
    input.partitions = ClaimPartitionsInput::Many(&names);
    input.values = ClaimValuesInput::Names(&names);
    let request = normalized(input)?;
    assert_eq!(
        prepare_claim_due_intent(&tenant, &request, IntentLimits::new(1, 104, 5)?),
        Err(PreparationError::ValueLimit)
    );
    Ok(())
}
