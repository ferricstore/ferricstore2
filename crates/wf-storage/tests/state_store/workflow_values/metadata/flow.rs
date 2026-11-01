use super::{TestResult, f, fixtures::*};
use wf_domain::workflow::request::RetainedStatus;
use wf_ports::log::{prepare_workflow_metadata_batch, prepare_workflow_stored_value_batch};
use wf_storage::transaction::RecordBatchTransactionOutcome;
use wf_types::identity::{PartitionId, RecordId};

#[test]
fn metadata_codec_binds_actual_original_operands_and_rejects_legacy_decoder() -> TestResult {
    let batch = request(b"i", b"create", 100)?;
    let data = metadata(7);
    let bytes = encode(&batch, &[Some(&data)])?;
    let (decoded, work) = prepare_workflow_metadata_batch(bytes.as_bytes(), codec_limits())?;
    assert_eq!(decoded.request().intent.version(), 22);
    assert_eq!(decoded.request().scope, batch.scope);
    assert!(work > 0);
    assert!(
        prepare_workflow_stored_value_batch(
            bytes.as_bytes(),
            codec_limits().codec,
            codec_limits().maximum_owned_bytes,
            codec_limits().maximum_work_units
        )
        .is_err()
    );
    let other = metadata(8);
    let changed = encode(&batch, &[Some(&other)])?;
    let (changed, _) = prepare_workflow_metadata_batch(changed.as_bytes(), codec_limits())?;
    assert_ne!(decoded.request().intent, changed.request().intent);
    Ok(())
}

#[test]
fn metadata_birth_is_atomic_with_real_core_history_values_and_original_retry() -> TestResult {
    let directory = crate::fixtures::directory("metadata-native-birth")?;
    let path = directory.join("state");
    let mut store = fresh(&path)?;
    let batch = request(b"i", b"create", 100)?;
    let data = metadata(7);
    let bytes = encode(&batch, &[Some(&data)])?;
    let RecordBatchTransactionOutcome::Retained {
        outcome: original, ..
    } = apply(&mut store, &bytes, 0)?
    else {
        return Err("expected fresh metadata birth".into());
    };
    assert_eq!(original.intent().version(), 22);
    assert_eq!(original.status(), RetainedStatus::Accepted);
    let scope = crate::fixtures::scope()?;
    assert_eq!(
        store.workflow_record_metadata(
            &scope,
            &PartitionId::new(b"p".to_vec())?,
            &RecordId::new(b"i".to_vec())?,
            None,
            f::read_limits()
        )?,
        Some(data)
    );
    let RecordBatchTransactionOutcome::Retained {
        outcome: replay, ..
    } = apply(&mut store, &bytes, 1)?
    else {
        return Err("expected original retained metadata result".into());
    };
    assert_eq!(replay, original);
    drop(store);
    let rows = crate::fixtures::raw_rows(&path)?;
    for family in [1, 2, 5, 8, 9, 10, 11] {
        assert_eq!(
            rows.keys()
                .filter(|key| key.get(6) == Some(&family))
                .count(),
            1
        );
    }
    std::fs::remove_dir_all(directory)?;
    Ok(())
}

#[test]
fn metadata_retry_conflict_preserves_the_original_capture_and_response() -> TestResult {
    let directory = crate::fixtures::directory("metadata-native-conflict")?;
    let path = directory.join("state");
    let mut store = fresh(&path)?;
    let batch = request(b"i", b"same-request", 100)?;
    let original = metadata(7);
    let changed = metadata(8);
    apply(&mut store, &encode(&batch, &[Some(&original)])?, 0)?;
    assert!(matches!(
        apply(&mut store, &encode(&batch, &[Some(&changed)])?, 1)?,
        RecordBatchTransactionOutcome::IntentConflict(_)
    ));
    assert_eq!(
        store.workflow_record_metadata(
            &crate::fixtures::scope()?,
            &PartitionId::new(b"p".to_vec())?,
            &RecordId::new(b"i".to_vec())?,
            None,
            f::read_limits()
        )?,
        Some(original)
    );
    drop(store);
    assert_eq!(history_ids(&path)?.len(), 1);
    std::fs::remove_dir_all(directory)?;
    Ok(())
}

#[test]
fn actual_legacy_mutation_carries_metadata_and_explicit_mutation_replaces_it() -> TestResult {
    use wf_ports::log::encode_workflow_stored_value_batch;
    use wf_storage::transaction::apply_workflow_stored_value_batch_transaction;
    let directory = crate::fixtures::directory("metadata-native-carry")?;
    let path = directory.join("state");
    let mut store = fresh(&path)?;
    let first = metadata(7);
    let second = metadata(8);
    apply(
        &mut store,
        &encode(&request(b"i", b"create", 100)?, &[Some(&first)])?,
        0,
    )?;
    let next = transition(&store, b"i", b"legacy-transition", 101, b"q", b"next")?;
    let legacy = encode_workflow_stored_value_batch(&next, codec_limits().codec)?;
    apply_workflow_stored_value_batch_transaction(
        &mut store,
        &crate::fixtures::scope()?,
        legacy.as_bytes(),
        crate::fixtures::log(1),
        f::limits(),
    )?;
    assert_eq!(
        store.workflow_record_metadata(
            &crate::fixtures::scope()?,
            &PartitionId::new(b"p".to_vec())?,
            &RecordId::new(b"i".to_vec())?,
            None,
            f::read_limits()
        )?,
        Some(first.clone())
    );
    let next = transition(&store, b"i", b"metadata-transition", 102, b"next", b"last")?;
    apply(&mut store, &encode(&next, &[Some(&second)])?, 2)?;
    drop(store);
    let events = history_ids(&path)?;
    assert_eq!(events.len(), 3);
    let store = crate::fixtures::open(&path)?;
    let scope = crate::fixtures::scope()?;
    let p = PartitionId::new(b"p".to_vec())?;
    let id = RecordId::new(b"i".to_vec())?;
    let old: Vec<_> = events
        .iter()
        .map(|event| store.workflow_record_metadata(&scope, &p, &id, Some(event), f::read_limits()))
        .collect::<Result<_, _>>()?;
    assert_eq!(
        old,
        vec![Some(first.clone()), Some(first), Some(second.clone())]
    );
    assert_eq!(
        store.workflow_record_metadata(&scope, &p, &id, None, f::read_limits())?,
        Some(second)
    );
    drop(store);
    std::fs::remove_dir_all(directory)?;
    Ok(())
}

#[test]
fn codec_caller_identity_excludes_actual_generated_time_and_guard_changes() -> TestResult {
    use wf_ports::consensus::WorkflowValueTargetGuard;
    let mut batch = request(b"i", b"create", 100)?;
    let data = metadata(7);
    let first = encode(&batch, &[Some(&data)])?;
    batch.stamp = crate::fixtures::clock(101)?;
    if let wf_ports::consensus::PreparedOperation::Batch(items) = &mut batch.operation {
        items[0].command.now = wf_types::version::TimestampMillis::new(101);
    }
    batch.inputs[0].target = WorkflowValueTargetGuard::Unspecified;
    let changed = encode(&batch, &[Some(&data)])?;
    let (left, _) = prepare_workflow_metadata_batch(first.as_bytes(), codec_limits())?;
    let (right, _) = prepare_workflow_metadata_batch(changed.as_bytes(), codec_limits())?;
    assert_eq!(left.request().intent, right.request().intent);
    assert_ne!(first.digest(), changed.digest());
    Ok(())
}

#[test]
fn native_metadata_supports_all_four_actual_contained_reducer_profiles() -> TestResult {
    use wf_domain::workflow::caller::PublicMutationEvidence;
    use wf_domain::workflow::{Operation, StartAndClaim};
    use wf_ports::consensus::PreparedOperation;
    let directory = crate::fixtures::directory("metadata-native-profiles")?;
    let path = directory.join("state");
    let mut store = fresh(&path)?;
    let data = metadata(7);
    for (index, profile) in [1_u8, 5, 6, 7].into_iter().enumerate() {
        let id = format!("profile-{profile}");
        let mut batch = request(id.as_bytes(), id.as_bytes(), 100)?;
        batch.profile = profile;
        let PreparedOperation::Batch(mut items) = batch.operation else {
            return Err("batch".into());
        };
        if profile == 7 {
            items[0].command.operation = Operation::StartAndClaim(StartAndClaim {
                workflow_type: b"r".to_vec(),
                state: b"q".to_vec(),
                worker: b"w".to_vec(),
                lease_ms: 100,
                payload: Some(b"data".to_vec()),
            });
        }
        batch.operation = if profile == 1 {
            PreparedOperation::PublicMutation {
                item: Box::new(items.remove(0)),
                evidence: PublicMutationEvidence {
                    auto_partition: false,
                    requested_now_ms: None,
                    retry_override_present: false,
                },
            }
        } else {
            PreparedOperation::Batch(items)
        };
        let RecordBatchTransactionOutcome::Retained { outcome, .. } = apply(
            &mut store,
            &encode(&batch, &[Some(&data)])?,
            u64::try_from(index)?,
        )?
        else {
            return Err("retained".into());
        };
        assert_eq!(outcome.status(), RetainedStatus::Accepted);
        assert_eq!(
            outcome.response_version(),
            if profile == 1 { 8 } else { u16::from(profile) }
        );
        assert_eq!(
            store.workflow_record_metadata(
                &crate::fixtures::scope()?,
                &PartitionId::new(b"p".to_vec())?,
                &RecordId::new(id.into_bytes())?,
                None,
                f::read_limits()
            )?,
            Some(data.clone())
        );
    }
    drop(store);
    drop(crate::fixtures::open(&path)?);
    std::fs::remove_dir_all(directory)?;
    Ok(())
}

#[test]
fn legacy_claim_preserves_real_metadata_across_current_and_new_history_binding() -> TestResult {
    use super::super::claim::fixtures as claim;
    let directory = crate::fixtures::directory("metadata-native-claim")?;
    let path = directory.join("state");
    let mut store = fresh(&path)?;
    let data = metadata(7);
    apply(
        &mut store,
        &encode(&request(b"i", b"create", 100)?, &[Some(&data)])?,
        0,
    )?;
    let frame = claim::phase(b"claim", 101, b"q", b"worker", 1)?;
    assert_eq!(
        claim::retained(claim::apply(&mut store, &frame, 1)?)?.status(),
        RetainedStatus::Accepted
    );
    drop(store);
    let events = history_ids(&path)?;
    assert_eq!(events.len(), 2);
    let store = crate::fixtures::open(&path)?;
    let p = PartitionId::new(b"p".to_vec())?;
    let id = RecordId::new(b"i".to_vec())?;
    assert_eq!(
        store.workflow_record_metadata(
            &crate::fixtures::scope()?,
            &p,
            &id,
            None,
            f::read_limits()
        )?,
        Some(data.clone())
    );
    assert_eq!(
        store.workflow_record_metadata(
            &crate::fixtures::scope()?,
            &p,
            &id,
            Some(&events[1]),
            f::read_limits()
        )?,
        Some(data)
    );
    drop(store);
    std::fs::remove_dir_all(directory)?;
    Ok(())
}

#[test]
fn independent_repeated_targets_keep_metadata_through_skips_and_source_positions() -> TestResult {
    use wf_domain::workflow::{Operation, Transition};
    use wf_ports::consensus::PreparedOperation;
    use wf_types::version::Fence;
    let directory = crate::fixtures::directory("metadata-native-independent")?;
    let path = directory.join("state");
    let mut store = fresh(&path)?;
    let mut batch = request(b"i", b"batch", 100)?;
    let PreparedOperation::Batch(mut items) = batch.operation else {
        return Err("items".into());
    };
    for (from, to) in [
        (b"q".as_slice(), b"next".as_slice()),
        (b"wrong", b"bad"),
        (b"next", b"last"),
        (b"last", b"done"),
    ] {
        items.push(f::item(
            b"i",
            100,
            Operation::Transition(Transition {
                from: from.to_vec(),
                to: to.to_vec(),
                fence: Fence::new(0),
                lease_token: None,
                run_at: None,
                payload: None,
            }),
        )?);
    }
    batch.profile = 6;
    batch.operation = PreparedOperation::Batch(items);
    batch.inputs = (0..5)
        .map(|index| {
            let mut input = f::input(index, 0);
            input.target = wf_ports::consensus::WorkflowValueTargetGuard::Absent;
            input
        })
        .collect();
    let first = metadata(7);
    let skipped = metadata(9);
    let last = metadata(8);
    let bytes = encode(
        &batch,
        &[Some(&first), None, Some(&skipped), Some(&last), None],
    )?;
    let RecordBatchTransactionOutcome::Retained { outcome, .. } = apply(&mut store, &bytes, 0)?
    else {
        return Err("retained independent".into());
    };
    assert_eq!(outcome.status(), RetainedStatus::Accepted);
    drop(store);
    let events = history_ids(&path)?;
    assert_eq!(events.len(), 4);
    let store = crate::fixtures::open(&path)?;
    let p = PartitionId::new(b"p".to_vec())?;
    let id = RecordId::new(b"i".to_vec())?;
    let scope = crate::fixtures::scope()?;
    let stored: Vec<_> = events
        .iter()
        .map(|event| store.workflow_record_metadata(&scope, &p, &id, Some(event), f::read_limits()))
        .collect::<Result<_, _>>()?;
    assert_eq!(
        stored,
        vec![
            Some(first.clone()),
            Some(first),
            Some(last.clone()),
            Some(last.clone())
        ]
    );
    assert_eq!(
        store.workflow_record_metadata(
            &crate::fixtures::scope()?,
            &p,
            &id,
            None,
            f::read_limits()
        )?,
        Some(last)
    );
    drop(store);
    std::fs::remove_dir_all(directory)?;
    Ok(())
}
