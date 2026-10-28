use super::{Input, Operation, S, TestResult, V, fixtures as f, prepare};

#[test]
fn logical_caller_owns_metadata_and_moves_stored_buffers() -> TestResult {
    let result = {
        let id = b"owned-id".to_vec();
        let kind = b"owned-type".to_vec();
        let partition = b"owned-partition".to_vec();
        let payload = b"FSV2\x02bytes".to_vec();
        let fields = [
            ("type", V::Scalar(S::Bytes(&kind))),
            ("partition_key", V::Scalar(S::Bytes(&partition))),
            ("payload", V::Scalar(S::Bytes(&payload))),
        ];
        let actual = prepare(
            Input {
                operation: "create",
                id: &id,
                fields: &fields,
            },
            &[],
            None,
            f::stamp()?,
            f::limits(),
        );
        assert!(
            actual.is_ok(),
            "healthy owned logical caller: {:?}",
            actual.as_ref().err()
        );
        actual?
    };
    assert_eq!(result.command().id.as_bytes(), b"owned-id");
    assert_eq!(result.partition(), Some(b"owned-partition".as_slice()));
    assert!(
        matches!(&result.command().operation, Operation::Create(value) if value.workflow_type == b"owned-type")
    );
    let payload = f::payload(&result.command().operation).ok_or("owned payload")?;
    assert_eq!(payload, b"FSV2\x01FSV2\x02bytes");
    let pointer = payload.as_ptr();
    let (partition, command, priority, evidence) = result.into_stored_parts();
    assert_eq!(
        f::payload(&command.operation)
            .ok_or("moved payload")?
            .as_ptr(),
        pointer
    );
    assert_eq!(partition.as_deref(), Some(b"owned-partition".as_slice()));
    assert_eq!(priority, 0);
    assert!(!evidence.auto_partition);
    assert_eq!(evidence.requested_now_ms, None);
    Ok(())
}
