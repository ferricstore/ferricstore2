use super::{fixtures as f, *};
use sha2::{Digest, Sha256};

#[test]
fn step_snapshot_preserves_full_core_and_golden_bytes() -> TestResult {
    let expected = f::step(Some(b"\0\xff"))?;
    let mut bytes = f::encoded(&expected)?;
    assert_eq!(bytes, f::hex(f::GOLDEN)?);
    assert_eq!(bytes.len(), 303);
    assert_eq!(
        Sha256::digest(&bytes).as_slice(),
        f::hex("5c715d80baa0c2e50de32c38a032bc8dcb1cd19f45795dafccf3408a01da1fc1")?
    );
    let decoded = f::decoded(&bytes, f::limits()?)?;
    assert_eq!(decoded, expected);
    assert_eq!(
        decoded.postimage.record.run_state(),
        Some(b"charge".as_slice())
    );
    assert_eq!(decoded.event.state, b"running");
    bytes.fill(0);
    drop(bytes);
    assert_eq!(decoded, expected);
    Ok(())
}

#[test]
fn snapshot_retains_internal_logical_state_without_changing_source_fields() -> TestResult {
    use wf_domain::history::compact::{
        OriginalCompactHistoryContext, OriginalCompactHistoryLimits,
        OriginalCompactHistorySnapshot, decode_original_compact_history,
        prepare_original_compact_history,
    };
    use wf_domain::metadata::OriginalMetadataInput as Input;
    let snapshot = f::step(Some(b"\0\xff"))?;
    let bytes = f::encoded(&snapshot)?;
    let decoded = f::decoded(&bytes, f::limits()?)?;
    let record = &decoded.postimage.record;
    let lease = record.lease().ok_or("source field fixture")?;
    let context = OriginalCompactHistoryContext {
        id: Some(record.id().as_bytes()),
        workflow_type: Some(record.workflow_type()),
        ..OriginalCompactHistoryContext::default()
    };
    let capture = OriginalCompactHistorySnapshot {
        event: decoded.event.kind.as_str().as_bytes(),
        version: Input::Integer(i128::from(record.version())),
        at: Input::Integer(2),
        state: Some(record.state()),
        numbers: [
            Input::Integer(i128::from(decoded.postimage.ordering.priority)),
            Input::Integer(i128::from(record.attempts())),
            Input::Integer(i128::from(record.fence().get())),
            Input::Integer(i128::from(record.created_at().get())),
            Input::Integer(i128::from(record.updated_at().get())),
            Input::Integer(30_002),
            Input::Integer(30_002),
        ],
        references: [Some(lease.worker.as_slice()), None, None, None, None],
        context,
        metadata: &[],
    };
    let limits = OriginalCompactHistoryLimits {
        max_input_bytes: 8192,
        max_metadata_pairs: 8,
        max_encoded_bytes: 8192,
        max_output_bytes: 8192,
        max_work_units: WORKSPACE,
        max_temporary_bytes: WORKSPACE,
    };
    let original = prepare_original_compact_history(&capture, limits)?;
    let fields = decode_original_compact_history(original.as_bytes(), context, limits)?;
    assert!(
        fields
            .fields()
            .iter()
            .all(|field| field.key.as_ref() != b"run_state")
    );
    assert_eq!(
        fields
            .fields()
            .iter()
            .find(|field| field.key.as_ref() == b"state")
            .and_then(|field| field.value.as_deref()),
        Some(b"running".as_slice())
    );
    assert_eq!(record.run_state(), Some(b"charge".as_slice()));
    Ok(())
}
