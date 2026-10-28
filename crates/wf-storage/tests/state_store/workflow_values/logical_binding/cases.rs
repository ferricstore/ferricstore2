use wf_domain::value::{OriginalValueInput as I, OriginalValueNode as N};
use wf_domain::workflow::caller::{LogicalMutationFieldValue as V, MutationFieldValue as S};
use wf_domain::workflow::request::RequestScope;
use wf_ports::log::{
    CodecError as E, decode_prepared, decode_workflow_stored_value_batch,
    encode_logical_mutation_frame, encode_workflow_stored_value_batch,
};
use wf_types::identity::{GroupId, RecoveryId, RequestId, TenantId};
use wf_types::version::TimestampMillis;

use super::super::logical_codec;
use super::{TestResult, fixtures as f};

#[test]
fn logical_binding_resolves_auto_partition_for_all_mutation_kinds() -> TestResult {
    for operation in [
        "create",
        "transition",
        "complete",
        "fail",
        "retry",
        "extend_lease",
        "cancel",
    ] {
        for (id, partition) in [
            (b"case-0".as_slice(), b"__flow_auto__:80".as_slice()),
            (b"case-1", b"__flow_auto__:198"),
            (b"case-23", b"__flow_auto__:3"),
            (b"abc", b"__flow_auto__:194"),
            (b"\0\xff:", b"__flow_auto__:210"),
        ] {
            // Independent stdlib CRC32 %256 goldens, not VM execution evidence.
            let caller = f::caller(operation, id, None, None)?;
            let (encoded, intent) = f::healthy(caller, f::context(None, 100)?, f::limits()?)?;
            let decoded =
                decode_workflow_stored_value_batch(encoded.as_bytes(), f::limits()?.codec)?;
            let items = decoded.operation.mutation_items().ok_or("public items")?;
            assert_eq!(items.len(), 1);
            assert_eq!(items[0].partition.as_bytes(), partition);
            assert_eq!(items[0].command.id.as_bytes(), id);
            assert_eq!(items[0].command.now.get(), 100);
            let generic = decode_prepared(encoded.as_bytes(), f::limits()?.codec)?;
            assert_eq!(intent.version(), 9);
            assert_eq!(generic.request().intent, intent);
            if !matches!(operation, "extend_lease" | "cancel") {
                assert_eq!(
                    generic.value_bytes(0).ok_or("values")?[0],
                    Some(b"FSV2\x01FSV2\x02binary".as_slice())
                );
            }
        }
    }
    Ok(())
}

#[test]
fn logical_binding_keeps_explicit_partition_time_and_retry_evidence() -> TestResult {
    let caller = f::caller("retry", b"i", Some(b"\0\xff:"), Some(80))?;
    let (encoded, intent) = f::healthy(caller, f::context(Some(80), 100)?, f::limits()?)?;
    let decoded = decode_workflow_stored_value_batch(encoded.as_bytes(), f::limits()?.codec)?;
    assert_eq!(intent.version(), 9);
    assert_eq!(
        decoded.inputs[0].requested_at,
        Some(TimestampMillis::new(80))
    );
    let wf_ports::consensus::PreparedOperation::PublicMutation { item, evidence } =
        &decoded.operation
    else {
        return Err("public operation".into());
    };
    assert!(!evidence.auto_partition);
    assert_eq!(evidence.requested_now_ms, Some(80));
    assert_eq!(item.partition.as_bytes(), b"\0\xff:");
    assert_eq!(item.command.now.get(), 80);
    assert!(
        matches!(&item.command.operation, wf_domain::workflow::Operation::RetryWithPolicy(value)
        if value.payload.as_deref() == Some(b"FSV2\x01FSV2\x02binary".as_slice())
        && value.error.as_deref() == Some(b"FSV2\x02\x83w\x03nil".as_slice()))
    );
    Ok(())
}

#[test]
fn logical_binding_preserves_stored_bytes_and_manual_frame_identity() -> TestResult {
    let nodes = [
        N::Integer(7),
        N::Binary { bytes: 0..1 },
        N::Tuple { children: 0..2 },
    ];
    let value = V::Value {
        input: I {
            bytes: b"x",
            nodes: &nodes,
            edges: &[0, 1],
            root: 2,
        },
        init_pid: None,
    };
    for value in [
        value,
        V::Scalar(S::Bytes(b"FSV2\x02\x83h\x02a\x07m\0\0\0\x01x")),
    ] {
        let caller = logical_codec::prepared_create(b"i", value)?;
        let (encoded, intent) = f::healthy(caller, f::context(None, 100)?, f::limits()?)?;
        let manual =
            logical_codec::request(logical_codec::prepared_create(b"i", value)?, b"binding")?;
        let expected = encode_workflow_stored_value_batch(&manual, f::limits()?.codec)?;
        assert_eq!(encoded.as_bytes(), expected.as_bytes());
        let decoded = decode_prepared(expected.as_bytes(), f::limits()?.codec)?;
        assert_eq!(intent, decoded.request().intent);
    }
    Ok(())
}

#[test]
fn logical_binding_rejects_conflicting_metadata_without_repair() -> TestResult {
    f::automatic(b"abc")?;
    for axis in 0..7 {
        let mut context = f::context(None, 100)?;
        match axis {
            0 => context.input.source_index = 1,
            1 => context.input.policy_index = 1,
            2 => context.input.requested_at = Some(TimestampMillis::new(80)),
            3 => context.stamp = crate::fixtures::clock(200)?,
            4 => context.policies[0].profile = 3,
            5 => context.defaults.lifecycle.ttl_ms += 1,
            _ => context.policies[0].workflow_type.clear(),
        }
        let result = encode_logical_mutation_frame(
            f::caller("create", b"abc", None, None)?,
            context,
            f::limits()?,
        );
        assert!(
            matches!(result, Err(E::Malformed | E::UnsupportedTag)),
            "axis{axis}: {:?}",
            result.as_ref().err()
        );
    }
    let mut context = f::context(None, 100)?;
    context.policies.clear();
    assert!(matches!(
        encode_logical_mutation_frame(
            f::caller("create", b"abc", None, None)?,
            context,
            f::limits()?
        ),
        Err(E::ItemLimit)
    ));
    Ok(())
}

#[test]
fn logical_binding_excludes_request_and_generated_stamp_from_intent() -> TestResult {
    let (first, intent) = f::healthy(
        f::caller("create", b"i", None, Some(80))?,
        f::context(Some(80), 100)?,
        f::limits()?,
    )?;
    let mut other = f::context(Some(80), 200)?;
    other.request = RequestId::new(b"another-request".to_vec())?;
    let (second, same) = f::healthy(
        f::caller("create", b"i", None, Some(80))?,
        other,
        f::limits()?,
    )?;
    assert_eq!(same, intent);
    assert_ne!(first.as_bytes(), second.as_bytes());
    let mut other = f::context(Some(80), 100)?;
    other.scope = RequestScope::new(
        TenantId::new(b"other".to_vec())?,
        GroupId::new(b"g".to_vec())?,
        RecoveryId::new(b"z".to_vec())?,
    );
    let (_, changed) = f::healthy(
        f::caller("create", b"i", None, Some(80))?,
        other,
        f::limits()?,
    )?;
    assert_ne!(changed, intent);
    let (_, changed) = f::healthy(
        f::caller("create", b"i", Some(b"literal"), Some(80))?,
        f::context(Some(80), 100)?,
        f::limits()?,
    )?;
    assert_ne!(changed, intent);
    Ok(())
}
