//! Explicit stored-byte command identity, without durable-profile admission.

use sha2::{Digest, Sha256};
use wf_domain::lease::{LeaseGuard, LeaseToken};
use wf_domain::value::{OriginalValueContext, OriginalValueNode, decode_original_flow_value};
use wf_domain::workflow::admission::AdmissionLimits;
use wf_domain::workflow::caller::PublicMutationEvidence;
use wf_domain::workflow::{Cancel, Create, Operation, StartAndClaim, Terminal};
use wf_ports::consensus::{
    PreparedOperation, PreparedWorkflowValueBatch, WorkflowValueTargetGuard,
};
use wf_ports::log::{
    CodecError, CodecLimits, decode_prepared, decode_workflow_stored_value_batch,
    decode_workflow_value_batch, encode_workflow_stored_value_batch, encode_workflow_value_batch,
    normal_recovery_header, prepare_workflow_stored_value_batch, prepare_workflow_value_batch,
};
use wf_types::identity::RequestId;
use wf_types::version::Fence;

use super::{TestResult, fixtures as f};

const TYPED: &[u8] = b"FSV2\x02\x83h\x02a\x07m\0\0\0\x01x";

fn request(mode: u8, payload: Option<&[u8]>) -> TestResult<PreparedWorkflowValueBatch> {
    let operation = if mode == 7 {
        Operation::StartAndClaim(StartAndClaim {
            workflow_type: b"r".to_vec(),
            state: b"q".to_vec(),
            worker: b"w".to_vec(),
            lease_ms: 100,
            payload: payload.map(<[u8]>::to_vec),
        })
    } else {
        Operation::Create(Create {
            workflow_type: b"r".to_vec(),
            state: b"q".to_vec(),
            run_at: None,
            payload: payload.map(<[u8]>::to_vec),
        })
    };
    let item = f::item(b"i", 100, operation)?;
    let operation = if mode == 1 {
        PreparedOperation::PublicMutation {
            item: Box::new(item),
            evidence: PublicMutationEvidence {
                auto_partition: false,
                requested_now_ms: None,
                retry_override_present: false,
            },
        }
    } else {
        PreparedOperation::Batch(vec![item])
    };
    let mut input = f::input(0, 0);
    input.target = WorkflowValueTargetGuard::Absent;
    let request = f::batch(
        mode,
        b"stored-codec",
        100,
        operation,
        vec![input],
        vec![f::witness(b"r", None)?],
    )?;
    let old = encode_workflow_value_batch(&request, f::limits().record_batch.codec);
    assert!(
        old.is_ok(),
        "existing binary encoder premise: {:?}",
        old.as_ref().err()
    );
    Ok(request)
}

fn payload(batch: &PreparedWorkflowValueBatch) -> TestResult<Option<&[u8]>> {
    let items = batch.operation.mutation_items().ok_or("mutation items")?;
    Ok(
        match &items.first().ok_or("first item")?.command.operation {
            Operation::Create(create) => create.payload.as_deref(),
            Operation::StartAndClaim(start) => start.payload.as_deref(),
            _ => return Err("payload fixture operation".into()),
        },
    )
}

fn intent_offset(bytes: &[u8]) -> TestResult<usize> {
    assert_eq!(bytes.get(..7), Some(b"FWCP\0\x01\x01".as_slice()));
    let mut at = 7;
    for _ in 0..4 {
        let width = u32::from_be_bytes(bytes.get(at..at + 4).ok_or("header length")?.try_into()?);
        let width = usize::try_from(width)?;
        at = at
            .checked_add(4)
            .and_then(|n| n.checked_add(width))
            .ok_or("header overflow")?;
    }
    Ok(at)
}

fn reseal(bytes: &mut [u8]) -> TestResult {
    let end = bytes.len().checked_sub(32).ok_or("footer")?;
    let hash = Sha256::digest(&bytes[..end]);
    bytes[end..].copy_from_slice(&hash);
    Ok(())
}

#[test]
fn stored_value_codec_distinguishes_binary_identity_and_preserves_bytes() -> TestResult {
    let request = request(5, Some(TYPED))?;
    let limits = f::limits().record_batch.codec;
    let old = encode_workflow_value_batch(&request, limits)?;
    let first = encode_workflow_stored_value_batch(&request, limits);
    assert!(first.is_ok(), "{:?}", first.as_ref().err());
    let encoded = first?;
    let at = intent_offset(encoded.as_bytes())?;
    assert_eq!(&old.as_bytes()[at..at + 2], 8_u16.to_be_bytes());
    assert_eq!(&encoded.as_bytes()[at..at + 2], 9_u16.to_be_bytes());
    assert_ne!(
        &encoded.as_bytes()[at + 2..at + 34],
        &old.as_bytes()[at + 2..at + 34]
    );
    assert_eq!(encoded.as_bytes().len(), old.as_bytes().len());
    let generic = decode_prepared(encoded.as_bytes(), limits)?;
    assert_eq!(generic.request().intent.version(), 9);
    assert_eq!(generic.value_bytes(0), Some([Some(TYPED), None]));
    let owned = decode_workflow_stored_value_batch(encoded.as_bytes(), limits)?;
    drop(generic);
    drop((request, encoded, old));
    assert_eq!(payload(&owned)?, Some(TYPED));
    Ok(())
}

#[test]
fn stored_value_codec_roundtrips_all_producer_modes() -> TestResult {
    let limits = f::limits().record_batch.codec;
    let source = request(5, Some(TYPED))?;
    let first = encode_workflow_stored_value_batch(&source, limits);
    assert!(first.is_ok(), "{:?}", first.as_ref().err());
    first?;
    for mode in [1, 5, 6, 7] {
        for bytes in [
            Some(TYPED),
            Some(b"FSV2\x01FSV2raw".as_slice()),
            Some(b"".as_slice()),
            None,
        ] {
            let source = request(mode, bytes)?;
            let encoded = encode_workflow_stored_value_batch(&source, limits)?;
            let decoded = decode_workflow_stored_value_batch(encoded.as_bytes(), limits)?;
            let generic = decode_prepared(encoded.as_bytes(), limits)?;
            assert_eq!(decoded.profile, mode);
            assert!(decoded.operation == source.operation);
            assert_eq!(payload(&decoded)?, bytes);
            assert_eq!(generic.request().intent.version(), 9);
            assert_eq!(
                generic.workflow_value_metadata().ok_or("metadata")?.profile,
                mode
            );
            assert_eq!(generic.value_bytes(0), Some([bytes, None]));
        }
    }
    Ok(())
}

#[test]
fn stored_value_codec_binds_stored_bytes_but_excludes_server_context() -> TestResult {
    let limits = f::limits().record_batch.codec;
    let source = request(5, Some(TYPED))?;
    let first = encode_workflow_stored_value_batch(&source, limits);
    assert!(first.is_ok(), "{:?}", first.as_ref().err());
    let first = first?;
    let original = decode_prepared(first.as_bytes(), limits)?.request().intent;
    for bytes in [Some(b"changed".as_slice()), Some(b"".as_slice()), None] {
        let changed = encode_workflow_stored_value_batch(&request(5, bytes)?, limits)?;
        assert_ne!(
            decode_prepared(changed.as_bytes(), limits)?
                .request()
                .intent,
            original
        );
    }
    let mut server = request(5, Some(TYPED))?;
    server.stamp = crate::fixtures::clock(200)?;
    server.request = RequestId::new(b"another-request".to_vec())?;
    server.policy_references_captured = Some(false);
    server.policies[0].digest = wf_domain::workflow::control::PreparedDigest::new([7; 32]);
    let changed = encode_workflow_stored_value_batch(&server, limits)?;
    assert_ne!(changed.digest(), first.digest());
    assert_eq!(
        decode_prepared(changed.as_bytes(), limits)?
            .request()
            .intent,
        original
    );
    let decoded = decode_workflow_stored_value_batch(changed.as_bytes(), limits)?;
    assert_eq!(decoded.stamp, server.stamp);
    assert_eq!(decoded.request, server.request);
    assert_eq!(decoded.policy_references_captured, Some(false));
    assert_eq!(decoded.policies[0].digest, server.policies[0].digest);
    Ok(())
}

#[test]
fn stored_value_codec_rejects_corruption_and_cross_profile_retagging() -> TestResult {
    let source = request(5, Some(TYPED))?;
    let limits = f::limits().record_batch.codec;
    let first = encode_workflow_stored_value_batch(&source, limits);
    assert!(first.is_ok(), "{:?}", first.as_ref().err());
    let encoded = first?;
    let old = encode_workflow_value_batch(&source, limits)?;
    assert!(matches!(
        decode_workflow_stored_value_batch(old.as_bytes(), limits),
        Err(CodecError::UnsupportedVersion)
    ));
    assert!(matches!(
        decode_workflow_value_batch(encoded.as_bytes(), limits),
        Err(CodecError::UnsupportedVersion)
    ));
    let mut retagged = old.as_bytes().to_vec();
    let at = intent_offset(&retagged)?;
    retagged[at..at + 2].copy_from_slice(&9_u16.to_be_bytes());
    reseal(&mut retagged)?;
    assert!(matches!(
        decode_workflow_stored_value_batch(&retagged, limits),
        Err(CodecError::Malformed)
    ));
    for end in 0..encoded.as_bytes().len() {
        assert!(
            decode_workflow_stored_value_batch(&encoded.as_bytes()[..end], limits).is_err(),
            "prefix {end}"
        );
    }
    let mut bad_footer = encoded.as_bytes().to_vec();
    let last = bad_footer.len() - 1;
    bad_footer[last] ^= 1;
    assert!(matches!(
        decode_workflow_stored_value_batch(&bad_footer, limits),
        Err(CodecError::Malformed)
    ));
    let mut trailing = encoded.as_bytes().to_vec();
    trailing.push(0);
    assert!(matches!(
        decode_workflow_stored_value_batch(&trailing, limits),
        Err(CodecError::TrailingBytes)
    ));
    Ok(())
}

#[test]
fn stored_value_codec_preserves_bounded_owned_preparation() -> TestResult {
    let source = request(5, Some(TYPED))?;
    let limits = f::limits().record_batch.codec;
    let first = encode_workflow_stored_value_batch(&source, limits);
    assert!(first.is_ok(), "{:?}", first.as_ref().err());
    let encoded = first?;
    assert!(matches!(
        prepare_workflow_stored_value_batch(encoded.as_bytes(), limits, 1, 64_000_000),
        Err(CodecError::OwnedLimit)
    ));
    assert!(matches!(
        prepare_workflow_stored_value_batch(encoded.as_bytes(), limits, 64_000_000, 1),
        Err(CodecError::WorkLimit)
    ));
    let (decoded, work) =
        prepare_workflow_stored_value_batch(encoded.as_bytes(), limits, 64_000_000, 64_000_000)?;
    assert!(work > encoded.as_bytes().len());
    assert!(work <= 64_000_000);
    assert_eq!(decoded.encoded().as_ptr(), encoded.as_bytes().as_ptr());
    assert_eq!(decoded.value_bytes(0), Some([Some(TYPED), None]));
    assert_eq!(
        decoded.workflow_value_metadata().ok_or("metadata")?.inputs[0].source_index,
        0
    );
    let owned = decode_workflow_stored_value_batch(encoded.as_bytes(), limits)?;
    drop(decoded);
    drop((encoded, source));
    assert_eq!(payload(&owned)?, Some(TYPED));
    let short = CodecLimits::new(AdmissionLimits::new(1_024, 64, 1)?);
    assert!(matches!(
        encode_workflow_stored_value_batch(&request(5, Some(TYPED))?, short),
        Err(CodecError::ByteLimit)
    ));
    Ok(())
}

#[test]
fn stored_value_codec_keeps_typed_content_and_durable_admission_separate() -> TestResult {
    let source = request(5, Some(TYPED))?;
    let limits = f::limits().record_batch.codec;
    let first = encode_workflow_stored_value_batch(&source, limits);
    assert!(first.is_ok(), "{:?}", first.as_ref().err());
    let encoded = first?;
    assert!(matches!(
        normal_recovery_header(encoded.as_bytes(), limits),
        Err(CodecError::UnsupportedVersion)
    ));
    assert!(matches!(
        prepare_workflow_value_batch(encoded.as_bytes(), limits, 64_000_000, 64_000_000),
        Err(CodecError::UnsupportedVersion)
    ));
    let original = decode_workflow_stored_value_batch(encoded.as_bytes(), limits)?;
    let context = OriginalValueContext {
        safe_atoms: None,
        active_exports: None,
        local_node: None,
        local_functions: None,
    };
    let value_limits = wf_domain::value::OriginalValueLimits {
        max_encoded_bytes: 1_024,
        max_nodes: 64,
        max_edges: 64,
        max_depth: 32,
        max_work_units: 1_000_000,
    };
    let document = decode_original_flow_value(
        payload(&original)?.ok_or("typed payload")?.to_vec(),
        &context,
        value_limits,
    )
    .map_err(|error| format!("typed fixture decode: {error:?}"))?;
    assert!(matches!(
        document.root(),
        Some(OriginalValueNode::Tuple { .. })
    ));
    let guard = LeaseGuard {
        token: LeaseToken::new(b"lease".to_vec())?,
        fence: Fence::new(1),
    };
    let operation = PreparedOperation::Batch(vec![
        f::item(
            b"i",
            100,
            Operation::Complete(Terminal {
                guard,
                payload: Some(b"FSV2\x01FSV2binary".to_vec()),
                value: Some(TYPED.to_vec()),
            }),
        )?,
        f::item(
            b"j",
            100,
            Operation::Cancel(Cancel {
                fence: Fence::new(0),
                lease_token: None,
                expected_state: None,
                expected_version: None,
                error: Some(b"FSV2\x02not-an-external-term".to_vec()),
            }),
        )?,
    ]);
    let source = f::batch(
        6,
        b"stored-fields",
        100,
        operation,
        vec![f::input(0, 0), f::input(1, 0)],
        vec![f::witness(b"r", None)?],
    )?;
    let encoded = encode_workflow_stored_value_batch(&source, limits)?;
    let decoded = decode_prepared(encoded.as_bytes(), limits)?;
    assert_eq!(
        decoded.value_bytes(0),
        Some([Some(b"FSV2\x01FSV2binary".as_slice()), Some(TYPED)])
    );
    assert_eq!(
        decoded.value_bytes(1),
        Some([Some(b"FSV2\x02not-an-external-term".as_slice()), None])
    );
    assert!(matches!(
        normal_recovery_header(encoded.as_bytes(), limits),
        Err(CodecError::UnsupportedVersion)
    ));
    Ok(())
}
