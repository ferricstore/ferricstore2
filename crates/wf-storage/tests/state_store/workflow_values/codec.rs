//! Independent outer-eight grammar, corruption and ownership controls.

use std::mem::size_of;

use sha2::{Digest, Sha256};
use wf_domain::lease::{ExtendLease, LeaseGuard, LeaseToken};
use wf_domain::policy::RetryOverride;
use wf_domain::retention::RetentionOverride;
use wf_domain::retry::{JitterSample, PolicyRetry};
use wf_domain::workflow::active::{LifecycleOverrides, MaxActiveOverride};
use wf_domain::workflow::admission::AdmissionLimits;
use wf_domain::workflow::caller::PublicMutationEvidence;
use wf_domain::workflow::{Create, Operation, StartAndClaim};
use wf_ports::consensus::{
    PreparedBatchItem, PreparedOperation, PreparedWorkflowValueBatch, WorkflowValueItemInput,
    WorkflowValuePolicyWitness, WorkflowValueTargetGuard,
};
use wf_ports::log::{
    CodecError, CodecLimits, DecodedRequest, decode_workflow_value_batch,
    encode_workflow_value_batch, prepare_workflow_value_batch,
};
use wf_types::version::Fence;

use super::{TestResult, fixtures as f};

#[test]
fn value_binary_codec_rejects_truncation_and_trailing_bytes() -> TestResult {
    let request = codec_request()?;
    let limits = f::limits().record_batch.codec;
    let encoded = encode_workflow_value_batch(&request, limits);
    assert!(encoded.is_ok(), "{:?}", encoded.as_ref().err());
    let encoded = encoded?;
    let raw = encoded.as_bytes();
    assert_literal_value_wire(raw, &encoded);
    for length in 0..raw.len() {
        assert!(
            decode_workflow_value_batch(&raw[..length], limits).is_err(),
            "prefix {length}"
        );
    }
    let mut trailing = raw.to_vec();
    trailing.push(0);
    assert!(matches!(
        decode_workflow_value_batch(&trailing, limits),
        Err(CodecError::TrailingBytes)
    ));
    let short = CodecLimits::new(AdmissionLimits::new(1_024, 285, 1)?);
    assert!(matches!(
        encode_workflow_value_batch(&request, short),
        Err(CodecError::ByteLimit)
    ));
    assert!(matches!(
        decode_workflow_value_batch(raw, short),
        Err(CodecError::ByteLimit)
    ));
    let exact = CodecLimits::new(AdmissionLimits::new(1_024, 286, 1)?);
    let exact_encoded = encode_workflow_value_batch(&request, exact);
    assert!(exact_encoded.is_ok(), "{:?}", exact_encoded.as_ref().err());
    assert_eq!(exact_encoded?.as_bytes(), raw);
    let exact_decoded = decode_workflow_value_batch(raw, exact);
    assert!(exact_decoded.is_ok(), "{:?}", exact_decoded.as_ref().err());
    Ok(())
}

#[test]
fn value_binary_codec_rejects_footer_and_caller_intent_tampering() -> TestResult {
    let request = codec_request()?;
    let limits = f::limits().record_batch.codec;
    let encoded = encode_workflow_value_batch(&request, limits);
    assert!(encoded.is_ok(), "{:?}", encoded.as_ref().err());
    let encoded = encoded?;
    let mut damaged = encoded.as_bytes().to_vec();
    damaged[285] ^= 1;
    assert!(matches!(
        decode_workflow_value_batch(&damaged, limits),
        Err(CodecError::Malformed)
    ));
    for offset in [32, 236, 11] {
        let mut changed = encoded.as_bytes().to_vec();
        changed[offset] ^= 1;
        reseal(&mut changed)?;
        assert!(
            matches!(
                decode_workflow_value_batch(&changed, limits),
                Err(CodecError::Malformed)
            ),
            "offset {offset}"
        );
    }
    for server_change in 0..4 {
        let mut changed = encoded.as_bytes().to_vec();
        match server_change {
            0 => {
                changed[64..72].copy_from_slice(&101_u64.to_be_bytes());
                changed[74..82].copy_from_slice(&101_u64.to_be_bytes());
            }
            1 => changed[164] ^= 1,
            2 => changed[85] = 0,
            _ => {
                changed[84] = 0;
                changed.remove(85);
            }
        }
        reseal(&mut changed)?;
        assert_eq!(&changed[32..64], &encoded.as_bytes()[32..64]);
        assert_ne!(
            Sha256::digest(&changed).as_slice(),
            encoded.digest().as_bytes()
        );
        let decoded = decode_workflow_value_batch(&changed, limits);
        assert!(
            decoded.is_ok(),
            "server {server_change}: {:?}",
            decoded.as_ref().err()
        );
        let decoded = decoded?;
        match server_change {
            0 => assert_eq!(decoded.stamp, crate::fixtures::clock(101)?),
            1 => assert_ne!(decoded.policies[0].digest, request.policies[0].digest),
            2 => assert_eq!(decoded.policy_references_captured, Some(false)),
            _ => assert_eq!(decoded.policy_references_captured, None),
        }
    }
    Ok(())
}

#[test]
fn value_binary_codec_checks_metadata_shapes_and_bounds() -> TestResult {
    let request = codec_request()?;
    let limits = f::limits().record_batch.codec;
    let encoded = encode_workflow_value_batch(&request, limits);
    assert!(encoded.is_ok(), "{:?}", encoded.as_ref().err());
    let encoded = encoded?;
    let corruptions: &[(usize, &[u8], CodecError)] = &[
        (82, &[2], CodecError::UnsupportedVersion),
        (83, &[128], CodecError::UnsupportedTag),
        (84, &[2], CodecError::InvalidOptionality),
        (85, &[2], CodecError::InvalidOptionality),
        (86, &[0; 8], CodecError::Malformed),
        (114, &[255], CodecError::Malformed),
        (146, &[0; 4], CodecError::ItemLimit),
        (146, &[255; 4], CodecError::ItemLimit),
        (155, &[3], CodecError::UnsupportedTag),
        (196, &[0; 4], CodecError::ItemLimit),
        (196, &[255; 4], CodecError::ItemLimit),
        (219, &[11], CodecError::UnsupportedTag),
        (230, &[2], CodecError::InvalidOptionality),
        (238, &[0; 4], CodecError::Malformed),
        (242, &[0, 0, 0, 1], CodecError::Malformed),
        (246, &[0, 0, 0, 1], CodecError::Malformed),
        (250, &[3], CodecError::UnsupportedTag),
        (251, &[2], CodecError::InvalidOptionality),
        (252, &[2], CodecError::InvalidOptionality),
        (253, &[2], CodecError::InvalidOptionality),
    ];
    for &(offset, replacement, expected) in corruptions {
        let mut changed = encoded.as_bytes().to_vec();
        changed[offset..offset + replacement.len()].copy_from_slice(replacement);
        reseal(&mut changed)?;
        assert!(
            matches!(decode_workflow_value_batch(&changed, limits), Err(error) if error == expected),
            "offset {offset}: {expected:?}"
        );
    }
    for workflow_type in [b"r", b"q"] {
        let mut duplicate_or_unsorted = codec_request()?;
        duplicate_or_unsorted
            .policies
            .push(f::witness(workflow_type, None)?);
        assert!(matches!(
            encode_workflow_value_batch(&duplicate_or_unsorted, limits),
            Err(CodecError::Malformed)
        ));
    }
    let mut two_policies = codec_request()?;
    two_policies.policies.push(f::witness(b"s", None)?);
    let count_one = CodecLimits::new(AdmissionLimits::new(1_024, 4_096, 1)?);
    assert!(matches!(
        encode_workflow_value_batch(&two_policies, count_one),
        Err(CodecError::ItemLimit)
    ));
    let sorted = encode_workflow_value_batch(&two_policies, limits);
    assert!(sorted.is_ok(), "{:?}", sorted.as_ref().err());
    let sorted = sorted?;
    let decoded = decode_workflow_value_batch(sorted.as_bytes(), limits);
    assert!(decoded.is_ok(), "{:?}", decoded.as_ref().err());
    assert_eq!(decoded?.policies.len(), 2);
    for workflow_type in *b"rq" {
        let mut changed = sorted.as_bytes().to_vec();
        changed[200] = workflow_type;
        reseal(&mut changed)?;
        assert!(matches!(
            decode_workflow_value_batch(&changed, limits),
            Err(CodecError::Malformed)
        ));
    }
    assert!(matches!(
        decode_workflow_value_batch(sorted.as_bytes(), count_one),
        Err(CodecError::ItemLimit)
    ));
    let mut unaligned = codec_request()?;
    unaligned.inputs.clear();
    assert!(matches!(
        encode_workflow_value_batch(&unaligned, limits),
        Err(CodecError::Malformed)
    ));
    let mut bad_defaults = codec_request()?;
    bad_defaults.defaults.lifecycle.ttl_ms = 0;
    assert!(matches!(
        encode_workflow_value_batch(&bad_defaults, limits),
        Err(CodecError::Malformed)
    ));
    Ok(())
}

#[test]
fn value_binary_codec_owned_and_generic_decoders_preserve_distinct_ownership() -> TestResult {
    let request = codec_request()?;
    let limits = f::limits().record_batch.codec;
    let encoded = encode_workflow_value_batch(&request, limits);
    assert!(encoded.is_ok(), "{:?}", encoded.as_ref().err());
    let encoded = encoded?;
    let owned = decode_workflow_value_batch(encoded.as_bytes(), limits);
    assert!(owned.is_ok(), "{:?}", owned.as_ref().err());
    let owned = owned?;
    let maximum_owned = size_of::<PreparedWorkflowValueBatch>()
        + size_of::<DecodedRequest<'static>>()
        + size_of::<PreparedBatchItem>()
        + size_of::<WorkflowValueItemInput>()
        + size_of::<WorkflowValuePolicyWitness>()
        + 2 * 286
        + size_of::<Vec<u8>>()
        + 62;
    assert!(matches!(
        prepare_workflow_value_batch(encoded.as_bytes(), limits, maximum_owned - 1, 111_232),
        Err(CodecError::OwnedLimit)
    ));
    assert!(matches!(
        prepare_workflow_value_batch(encoded.as_bytes(), limits, maximum_owned, 111_231),
        Err(CodecError::WorkLimit)
    ));
    let generic = prepare_workflow_value_batch(encoded.as_bytes(), limits, maximum_owned, 111_232);
    assert!(generic.is_ok(), "{:?}", generic.as_ref().err());
    let (generic, spent) = generic?;
    assert_eq!(spent, 111_232);
    assert_eq!(generic.request().intent.version(), 8);
    assert_eq!(generic.request().scope, request.scope);
    assert_eq!(generic.request().request.as_bytes(), b"wire");
    assert_eq!(generic.digest(), encoded.digest());
    assert_eq!(generic.encoded().as_ptr(), encoded.as_bytes().as_ptr());
    assert_eq!(generic.encoded(), encoded.as_bytes());
    assert_generic_metadata(&generic, &request)?;
    let generic_items = generic
        .request()
        .operation
        .mutation_items()
        .ok_or("generic items")?;
    let owned_items = owned.operation.mutation_items().ok_or("owned items")?;
    let Operation::Create(generic_create) = &generic_items[0].command.operation else {
        return Err("generic Create".into());
    };
    let Operation::Create(owned_create) = &owned_items[0].command.operation else {
        return Err("owned Create".into());
    };
    let generic_payload = generic_create.payload.as_deref().ok_or("generic payload")?;
    let owned_payload = owned_create.payload.as_deref().ok_or("owned payload")?;
    assert_ne!(generic_payload.as_ptr(), owned_payload.as_ptr());
    assert_eq!(generic_payload, [0, 255]);
    assert_eq!(
        generic.value_bytes(0),
        Some([Some([0, 255].as_slice()), None])
    );
    drop(generic);
    drop((encoded, request));
    let items = owned
        .operation
        .mutation_items()
        .ok_or("owned post-drop items")?;
    assert_eq!(owned.scope.tenant().as_bytes(), b"t");
    assert_eq!(items[0].partition.as_bytes(), b"p");
    assert_eq!(items[0].command.id.as_bytes(), b"i");
    let Operation::Create(create) = &items[0].command.operation else {
        return Err("owned post-drop Create".into());
    };
    assert_eq!(
        (create.workflow_type.as_slice(), create.state.as_slice()),
        (b"r".as_slice(), b"q".as_slice())
    );
    assert_eq!(create.payload.as_deref(), Some([0, 255].as_slice()));
    assert_eq!(owned.policies[0].workflow_type, b"r");
    Ok(())
}

#[test]
fn value_binary_public_retry_preserves_prepared_jitter_outside_caller_intent() -> TestResult {
    let mut request = codec_request()?;
    request.profile = 1;
    request.operation = PreparedOperation::PublicMutation {
        item: Box::new(f::item(
            b"i",
            100,
            Operation::RetryWithPolicy(PolicyRetry {
                guard: LeaseGuard {
                    token: LeaseToken::new(b"lease".to_vec())?,
                    fence: Fence::new(3),
                },
                run_at: None,
                policy_override: RetryOverride::default(),
                jitter: JitterSample::new(11),
                payload: Some(vec![0, 255]),
                error: Some(b"E".to_vec()),
            }),
        )?),
        evidence: PublicMutationEvidence {
            requested_now_ms: None,
            auto_partition: false,
            retry_override_present: false,
        },
    };
    let limits = f::limits().record_batch.codec;
    let encoded = encode_workflow_value_batch(&request, limits);
    assert!(encoded.is_ok(), "{:?}", encoded.as_ref().err());
    let encoded = encoded?;
    let first = decode_workflow_value_batch(encoded.as_bytes(), limits);
    assert!(first.is_ok(), "{:?}", first.as_ref().err());
    let first = first?;
    let PreparedOperation::PublicMutation { item, .. } = &mut request.operation else {
        return Err("public retry input".into());
    };
    let Operation::RetryWithPolicy(retry) = &mut item.command.operation else {
        return Err("retry input".into());
    };
    retry.jitter = JitterSample::new(22);
    let changed = encode_workflow_value_batch(&request, limits);
    assert!(changed.is_ok(), "{:?}", changed.as_ref().err());
    let changed = changed?;
    assert_eq!(&encoded.as_bytes()[32..64], &changed.as_bytes()[32..64]);
    assert_ne!(encoded.digest(), changed.digest());
    let second = decode_workflow_value_batch(changed.as_bytes(), limits);
    assert!(second.is_ok(), "{:?}", second.as_ref().err());
    let second = second?;
    drop((encoded, changed, request));
    for (decoded, expected) in [(first, 11), (second, 22)] {
        let PreparedOperation::PublicMutation { item, evidence } = decoded.operation else {
            return Err("decoded public retry".into());
        };
        let Operation::RetryWithPolicy(retry) = item.command.operation else {
            return Err("decoded policy retry".into());
        };
        assert_eq!(retry.jitter.get(), expected);
        assert_eq!(retry.guard.token.as_bytes(), b"lease");
        assert_eq!(retry.guard.fence.get(), 3);
        assert_eq!(retry.payload.as_deref(), Some([0, 255].as_slice()));
        assert_eq!(retry.error.as_deref(), Some(b"E".as_slice()));
        assert!(!evidence.retry_override_present);
    }
    Ok(())
}

fn codec_request() -> TestResult<PreparedWorkflowValueBatch> {
    let mut input = f::input(0, 0);
    input.target = WorkflowValueTargetGuard::Absent;
    f::batch(
        5,
        b"wire",
        100,
        PreparedOperation::Batch(vec![f::item(
            b"i",
            100,
            Operation::Create(Create {
                workflow_type: b"r".to_vec(),
                state: b"q".to_vec(),
                run_at: None,
                payload: Some(vec![0, 255]),
            }),
        )?]),
        vec![input],
        vec![f::witness(b"r", None)?],
    )
}

fn reseal(encoded: &mut [u8]) -> TestResult {
    let footer = encoded.len().checked_sub(32).ok_or("missing footer")?;
    let digest = Sha256::digest(&encoded[..footer]);
    encoded[footer..].copy_from_slice(&digest);
    Ok(())
}

#[test]
fn value_binary_codec_preserves_reducer_priority_inputs() -> TestResult {
    let limits = f::limits().record_batch.codec;
    for mode in [5, 6, 7] {
        for priority in [3, 255] {
            let mut request = codec_request()?;
            request.profile = mode;
            let PreparedOperation::Batch(items) = &mut request.operation else {
                return Err("priority input batch".into());
            };
            items[0].create_priority = priority;
            if mode == 7 {
                items[0].command.operation = Operation::StartAndClaim(StartAndClaim {
                    workflow_type: b"r".to_vec(),
                    state: b"q".to_vec(),
                    worker: b"w".to_vec(),
                    lease_ms: 100,
                    payload: Some(vec![0, 255]),
                });
            }
            let encoded = encode_workflow_value_batch(&request, limits);
            assert!(
                encoded.is_ok(),
                "mode {mode}, priority {priority}: {:?}",
                encoded.as_ref().err()
            );
            let encoded = encoded?;
            let decoded = decode_workflow_value_batch(encoded.as_bytes(), limits);
            assert!(decoded.is_ok(), "{:?}", decoded.as_ref().err());
            let decoded = decoded?;
            assert_eq!(decoded.profile, mode);
            assert!(decoded.operation == request.operation);
            let items = decoded
                .operation
                .mutation_items()
                .ok_or("priority output")?;
            assert_eq!(items[0].create_priority, priority);
        }
    }
    for mode in [5, 6] {
        let mut request = codec_request()?;
        request.profile = mode;
        let PreparedOperation::Batch(items) = &mut request.operation else {
            return Err("ignored priority batch".into());
        };
        items[0].create_priority = 255;
        items[0].command.operation = Operation::ExtendLease(ExtendLease {
            guard: LeaseGuard {
                token: LeaseToken::new(b"lease".to_vec())?,
                fence: Fence::new(3),
            },
            lease_ms: 100,
        });
        let encoded = encode_workflow_value_batch(&request, limits);
        assert!(encoded.is_ok(), "{:?}", encoded.as_ref().err());
        let encoded = encoded?;
        let decoded = decode_workflow_value_batch(encoded.as_bytes(), limits);
        assert!(decoded.is_ok(), "{:?}", decoded.as_ref().err());
        let decoded = decoded?;
        assert!(decoded.operation == request.operation);
        let items = decoded
            .operation
            .mutation_items()
            .ok_or("ignored priority output")?;
        assert_eq!(items[0].create_priority, 255);
    }
    Ok(())
}

#[test]
fn value_binary_codec_preserves_unresolved_extension_presence() -> TestResult {
    let mut request = codec_request()?;
    let PreparedOperation::Batch(items) = &mut request.operation else {
        return Err("unresolved input batch".into());
    };
    items[0].command.operation = Operation::ExtendLease(ExtendLease {
        guard: LeaseGuard {
            token: LeaseToken::new(b"lease".to_vec())?,
            fence: Fence::new(3),
        },
        lease_ms: 100,
    });
    request.inputs[0].birth = Some(LifecycleOverrides {
        active: MaxActiveOverride::Millis(0),
        retention: RetentionOverride {
            ttl_ms: Some(0),
            history_max_events: Some(0),
        },
        history_hot_max_events: Some(u64::MAX),
    });
    request.inputs[0].terminal_ttl_ms = Some(-1);
    let limits = f::limits().record_batch.codec;
    let encoded = encode_workflow_value_batch(&request, limits);
    assert!(encoded.is_ok(), "{:?}", encoded.as_ref().err());
    let encoded = encoded?;
    let decoded = decode_workflow_value_batch(encoded.as_bytes(), limits);
    assert!(decoded.is_ok(), "{:?}", decoded.as_ref().err());
    let decoded = decoded?;
    assert!(decoded.operation == request.operation);
    assert!(decoded.inputs[0] == request.inputs[0]);
    drop((encoded, request));
    let birth = decoded.inputs[0].birth.ok_or("unresolved birth omitted")?;
    assert!(matches!(birth.active, MaxActiveOverride::Millis(0)));
    assert_eq!(birth.retention.ttl_ms, Some(0));
    assert_eq!(birth.retention.history_max_events, Some(0));
    assert_eq!(birth.history_hot_max_events, Some(u64::MAX));
    assert_eq!(decoded.inputs[0].terminal_ttl_ms, Some(-1));
    Ok(())
}

fn assert_literal_value_wire(raw: &[u8], encoded: &wf_ports::log::PreparedBytes) {
    assert_eq!(raw.len(), 286);
    assert_eq!(
        &raw[..32],
        b"FWCP\0\x01\x01\0\0\0\x01t\0\0\0\x01g\0\0\0\x01z\0\0\0\x04wire\0\x08",
    );
    assert_eq!(&raw[64..72], &100_u64.to_be_bytes());
    assert_eq!(&raw[72..74], &[0, 0]);
    assert_eq!(&raw[74..82], &100_u64.to_be_bytes());
    assert_eq!(&raw[82..86], &[1, 5, 1, 1]);
    let default_scalars = [
        7_000_u64.to_be_bytes().as_slice(),
        &2_u32.to_be_bytes(),
        &10_u32.to_be_bytes(),
        &100_u64.to_be_bytes(),
        &1_000_u32.to_be_bytes(),
    ]
    .concat();
    assert_eq!(&raw[86..114], default_scalars);
    let configuration = [
        b"wf-workflow-value-defaults-v1".as_slice(),
        &default_scalars,
    ]
    .concat();
    assert_eq!(&raw[114..146], Sha256::digest(configuration).as_slice());
    assert_eq!(&raw[146..150], &1_u32.to_be_bytes());
    assert_eq!(&raw[150..156], b"\0\0\0\x01r\0");
    assert_eq!(&raw[156..164], &[0; 8]);
    assert_eq!(
        &raw[164..196],
        &[
            0x6e, 0x99, 0x78, 0x63, 0x6a, 0x60, 0x17, 0xb6, 0xe2, 0xab, 0xf8, 0x16, 0x47, 0xa5,
            0xaa, 0xb1, 0xb7, 0x1f, 0x00, 0x3b, 0x16, 0xe2, 0x3a, 0xd9, 0x31, 0xa1, 0xbe, 0xda,
            0x2d, 0x62, 0xb9, 0xa9,
        ],
    );
    assert_eq!(&raw[196..200], &1_u32.to_be_bytes());
    assert_eq!(
        &raw[200..238],
        b"\0\0\0\x01p\0\0\0\x01i\0\0\0\0\0\0\0\0d\x01\0\0\0\x01r\0\0\0\x01q\0\x01\0\0\0\x02\0\xff",
    );
    assert_eq!(
        &raw[238..254],
        &[0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0]
    );
    let caller = [
        b"WF-WORKFLOW-VALUE\0V8\0".as_slice(),
        b"\0\0\0\x01t\0\0\0\x01g\0\0\0\x01z\x05\0\0\0\x01",
        b"\0\0\0\x01p\0\0\0\x01i\x01\0\0\0\0\x01r\0\0\0\x01q\0\x01\0\0\0\x02\0\xff",
        b"\0\0\0\x01\0\0\0",
    ]
    .concat();
    assert_eq!(&raw[32..64], Sha256::digest(&caller).as_slice());
    assert_eq!(&raw[254..], Sha256::digest(&raw[..254]).as_slice());
    assert_eq!(encoded.digest().as_bytes(), Sha256::digest(raw).as_slice());
}

fn assert_generic_metadata(
    generic: &DecodedRequest<'_>,
    request: &PreparedWorkflowValueBatch,
) -> TestResult {
    let isolation = b"FWSC\0\x01\0\0\0\x01t\0\0\0\x01g\0\0\0\x01z";
    assert_eq!(generic.isolation_scope(), isolation);
    let expected_key = [
        b"FWSK\0\x01\0\0\0\x15".as_slice(),
        isolation,
        b"\0\0\0\x01p\0\0\0\x01i",
    ]
    .concat();
    assert_eq!(generic.state_key(0), Some(expected_key.as_slice()));
    assert_eq!(generic.state_key(1), None);
    let metadata = generic
        .workflow_value_metadata()
        .ok_or("missing value metadata")?;
    assert_eq!(metadata.profile, 5);
    assert_eq!(metadata.policy_references_captured, Some(true));
    assert!(metadata.defaults == f::defaults());
    let (policies, inputs) = (metadata.policies, metadata.inputs);
    assert_eq!(policies.len(), 1);
    assert_eq!(policies[0].workflow_type, b"r");
    assert_eq!((policies[0].profile, policies[0].generation), (0, 0));
    assert_eq!(policies[0].digest, request.policies[0].digest);
    assert_eq!(inputs.len(), 1);
    assert!(inputs[0] == request.inputs[0]);
    Ok(())
}
