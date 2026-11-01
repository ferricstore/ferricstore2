use super::{TestResult, fixtures::*};

#[test]
fn codec_positive_resources_have_distinct_owned_and_work_refusals() -> TestResult {
    use wf_ports::log::{CodecError, prepare_workflow_metadata_batch};
    let data = metadata(7);
    let bytes = encode(&request(b"i", b"create", 100)?, &[Some(&data)])?;
    let mut cap = codec_limits();
    cap.maximum_work_units = 1;
    assert!(matches!(
        prepare_workflow_metadata_batch(bytes.as_bytes(), cap),
        Err(CodecError::WorkLimit)
    ));
    let mut cap = codec_limits();
    cap.maximum_owned_bytes = 1;
    assert!(matches!(
        prepare_workflow_metadata_batch(bytes.as_bytes(), cap),
        Err(CodecError::OwnedLimit)
    ));
    Ok(())
}

#[test]
fn missing_birth_capture_aborts_every_native_row_and_can_retry() -> TestResult {
    use wf_storage::transaction::apply_workflow_metadata_batch_transaction;
    let directory = crate::fixtures::directory("metadata-native-missing-birth")?;
    let path = directory.join("state");
    drop(fresh(&path)?);
    let before = crate::fixtures::raw_rows(&path)?;
    let manifest = super::f::raw_manifest(&path)?;
    let mut store = crate::fixtures::open(&path)?;
    let batch = request(b"i", b"create", 100)?;
    let bytes = encode(&batch, &[None])?;
    assert!(
        apply_workflow_metadata_batch_transaction(
            &mut store,
            &crate::fixtures::scope()?,
            bytes.as_bytes(),
            crate::fixtures::log(0),
            super::f::limits()
        )
        .is_err()
    );
    drop(store);
    assert_eq!(crate::fixtures::raw_rows(&path)?, before);
    assert_eq!(super::f::raw_manifest(&path)?, manifest);
    let mut store = crate::fixtures::open(&path)?;
    let data = metadata(7);
    apply(&mut store, &encode(&batch, &[Some(&data)])?, 0)?;
    drop(store);
    std::fs::remove_dir_all(directory)?;
    Ok(())
}

#[test]
fn late_metadata_output_refusal_rolls_back_staged_core_history_and_metadata() -> TestResult {
    use wf_storage::transaction::apply_workflow_metadata_batch_transaction;
    let directory = crate::fixtures::directory("metadata-native-late-output")?;
    let path = directory.join("state");
    drop(fresh(&path)?);
    let before = crate::fixtures::raw_rows(&path)?;
    let mut store = crate::fixtures::open(&path)?;
    let mut batch = request(b"i", b"create", 100)?;
    if let wf_ports::consensus::PreparedOperation::Batch(items) = &mut batch.operation {
        let wf_domain::workflow::Operation::Create(create) = &mut items[0].command.operation else {
            return Err("missing create".into());
        };
        create.payload = None;
    }
    let data = metadata(7);
    let bytes = encode(&batch, &[Some(&data)])?;
    let mut cap = super::f::limits();
    cap.maximum_value_rows = 1;
    assert!(
        apply_workflow_metadata_batch_transaction(
            &mut store,
            &crate::fixtures::scope()?,
            bytes.as_bytes(),
            crate::fixtures::log(0),
            cap
        )
        .is_err()
    );
    drop(store);
    assert_eq!(crate::fixtures::raw_rows(&path)?, before);
    let mut store = crate::fixtures::open(&path)?;
    apply(&mut store, &bytes, 0)?;
    drop(store);
    assert_eq!(history_ids(&path)?.len(), 1);
    std::fs::remove_dir_all(directory)?;
    Ok(())
}

#[test]
fn legacy_capture_is_unavailable_and_new_commands_cannot_upgrade_it() -> TestResult {
    use wf_storage::transaction::apply_workflow_metadata_batch_transaction;
    use wf_types::identity::{PartitionId, RecordId};
    let directory = crate::fixtures::directory("metadata-native-legacy")?;
    let path = directory.join("state");
    let mut store = legacy_fresh(&path)?;
    let scope = crate::fixtures::scope()?;
    assert!(
        store
            .workflow_record_metadata(
                &scope,
                &PartitionId::new(b"p".to_vec())?,
                &RecordId::new(b"i".to_vec())?,
                None,
                super::f::read_limits()
            )
            .is_err()
    );
    let data = metadata(7);
    let bytes = encode(&request(b"i", b"create", 100)?, &[Some(&data)])?;
    assert!(
        apply_workflow_metadata_batch_transaction(
            &mut store,
            &scope,
            bytes.as_bytes(),
            crate::fixtures::log(0),
            super::f::limits()
        )
        .is_err()
    );
    drop(store);
    assert!(
        !crate::fixtures::raw_rows(&path)?
            .keys()
            .any(|key| key.get(6) == Some(&1))
    );
    std::fs::remove_dir_all(directory)?;
    Ok(())
}

#[test]
fn complete_outer_and_inner_work_share_one_inclusive_account() -> TestResult {
    use wf_ports::log::{CodecError, prepare_workflow_metadata_batch};
    let data = metadata(7);
    let bytes = encode(&request(b"i", b"create", 100)?, &[Some(&data)])?;
    let (_, spent) = prepare_workflow_metadata_batch(bytes.as_bytes(), codec_limits())?;
    let mut cap = codec_limits();
    cap.maximum_work_units = spent;
    assert_eq!(
        prepare_workflow_metadata_batch(bytes.as_bytes(), cap)?.1,
        spent
    );
    cap.maximum_work_units -= 1;
    assert!(matches!(
        prepare_workflow_metadata_batch(bytes.as_bytes(), cap),
        Err(CodecError::WorkLimit)
    ));
    Ok(())
}

#[test]
fn caller_digest_corruption_cannot_be_hidden_by_resealing_or_trailing_bytes() -> TestResult {
    use wf_ports::log::prepare_workflow_metadata_batch;
    let data = metadata(7);
    let bytes = encode(&request(b"i", b"create", 100)?, &[Some(&data)])?.into_bytes();
    for end in 0..bytes.len() {
        assert!(prepare_workflow_metadata_batch(&bytes[..end], codec_limits()).is_err());
    }
    let mut changed = bytes.clone();
    let at = changed.len() - 64;
    changed[at] ^= 1;
    reseal(&mut changed);
    assert!(prepare_workflow_metadata_batch(&changed, codec_limits()).is_err());
    let mut changed = bytes;
    changed.push(0);
    assert!(prepare_workflow_metadata_batch(&changed, codec_limits()).is_err());
    Ok(())
}

#[test]
fn large_metadata_read_has_an_independent_exact_copied_byte_limit() -> TestResult {
    use wf_ports::state::StateTransactionError;
    use wf_types::identity::{PartitionId, RecordId};
    let directory = crate::fixtures::directory("metadata-native-large-read")?;
    let path = directory.join("state");
    let mut store = fresh(&path)?;
    let mut data = metadata(7);
    data.truncate(data.len() - 2);
    data.push(109);
    data.extend_from_slice(&131_072_u32.to_be_bytes());
    data.resize(data.len() + 131_072, 0xa5);
    apply(
        &mut store,
        &encode(&request(b"i", b"create", 100)?, &[Some(&data)])?,
        0,
    )?;
    let scope = crate::fixtures::scope()?;
    let p = PartitionId::new(b"p".to_vec())?;
    let id = RecordId::new(b"i".to_vec())?;
    let mut cap = super::f::read_limits();
    cap.maximum_stored_bytes = data.len();
    assert_eq!(
        store.workflow_record_metadata(&scope, &p, &id, None, cap)?,
        Some(data.clone())
    );
    cap.maximum_stored_bytes -= 1;
    assert!(matches!(
        store.workflow_record_metadata(&scope, &p, &id, None, cap),
        Err(StateTransactionError::Limit)
    ));
    for field in 0..3 {
        let mut cap = super::f::read_limits();
        match field {
            0 => cap.maximum_work = 1,
            1 => cap.maximum_temporary_bytes = 1,
            _ => cap.maximum_references = 0,
        }
        assert!(matches!(
            store.workflow_record_metadata(&scope, &p, &id, None, cap),
            Err(StateTransactionError::Limit)
        ));
    }
    drop(store);
    drop(crate::fixtures::open(&path)?);
    std::fs::remove_dir_all(directory)?;
    Ok(())
}
