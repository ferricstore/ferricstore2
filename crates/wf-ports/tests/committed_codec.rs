//! Independent byte/hash and adversarial private committed-codec controls.

#![forbid(unsafe_code)]

use std::error::Error;

use wf_domain::claim::Claim;
use wf_domain::lease::{ExtendLease, LeaseGuard, LeaseToken};
use wf_domain::policy::{Backoff, BackoffKind, RetryOverride};
use wf_domain::retry::{JitterSample, PolicyRetry, Retry, RetryTime};
use wf_domain::workflow::admission::AdmissionLimits;
use wf_domain::workflow::request::{ClientIntent, RequestScope};
use wf_domain::workflow::{Cancel, CommittedCommand, Create, Operation, Terminal, Transition};
use wf_ports::consensus::{PreparedBatchItem, PreparedOperation, PreparedRequest};
use wf_ports::log::{CodecError, CodecLimits, decode_prepared, encode_prepared};
use wf_types::identity::{GroupId, PartitionId, RecordId, RecoveryId, RequestId, TenantId};
use wf_types::version::{
    AppliedPosition, CommittedTime, Fence, HlcTimestamp, LogIdentity, TimestampMillis,
};

type TestResult<T = ()> = Result<T, Box<dyn Error>>;

fn guard() -> TestResult<LeaseGuard> {
    Ok(LeaseGuard {
        token: LeaseToken::new(b"tok".to_vec())?,
        fence: Fence::new(7),
    })
}

fn claim(state: &[u8]) -> TestResult<Claim> {
    Ok(Claim {
        expected_state: state.to_vec(),
        worker: b"w\xff".to_vec(),
        token: LeaseToken::new(b"tok".to_vec())?,
        lease_ms: 30,
    })
}

fn create(empty: bool) -> Operation {
    Operation::Create(Create {
        workflow_type: b"t".to_vec(),
        state: b"ready".to_vec(),
        run_at: empty.then_some(TimestampMillis::new(0)),
        payload: empty.then(Vec::new),
    })
}

fn operation(name: &str) -> TestResult<Operation> {
    Ok(match name {
        "create_absent" | "batch_forward" | "batch_reverse" => create(false),
        "create_empty" => create(true),
        "claim" => Operation::Claim(claim(b"ready")?),
        "reclaim" => Operation::Reclaim(claim(b"running")?),
        "transition" => Operation::Transition(Transition {
            from: b"running".to_vec(),
            to: b"next".to_vec(),
            fence: Fence::new(u64::MAX),
            lease_token: Some(LeaseToken::new(b"tok".to_vec())?),
            run_at: Some(TimestampMillis::new(0)),
            payload: Some(vec![0, 255]),
        }),
        "complete" => Operation::Complete(Terminal {
            guard: guard()?,
            payload: None,
            value: Some(Vec::new()),
        }),
        "fail" => Operation::Fail(Terminal {
            guard: guard()?,
            payload: Some(b"x".to_vec()),
            value: Some(vec![255]),
        }),
        "retry_explicit" | "retry_resolved" => Operation::Retry(Retry {
            guard: guard()?,
            run_at: if name == "retry_explicit" {
                RetryTime::Explicit(TimestampMillis::new(0))
            } else {
                RetryTime::PolicyResolved(TimestampMillis::new(11))
            },
            payload: (name == "retry_resolved").then(Vec::new),
            error: (name == "retry_resolved").then(|| b"err".to_vec()),
        }),
        "retry_policy" => Operation::RetryWithPolicy(PolicyRetry {
            guard: guard()?,
            run_at: None,
            policy_override: RetryOverride {
                max_retries: Some(0),
                backoff: Some(Backoff {
                    kind: BackoffKind::Fixed,
                    base_ms: 0,
                    max_ms: 7,
                    jitter_pct: 100,
                }),
                exhausted_to: Some(b"failed".to_vec()),
            },
            jitter: JitterSample::new(u32::MAX),
            payload: Some(Vec::new()),
            error: Some(Vec::new()),
        }),
        "extend_lease" => Operation::ExtendLease(ExtendLease {
            guard: guard()?,
            lease_ms: u64::MAX,
        }),
        "cancel_absent" => Operation::Cancel(Cancel {
            fence: Fence::new(0),
            lease_token: None,
            expected_state: None,
            expected_version: None,
            error: None,
        }),
        "cancel_full" => Operation::Cancel(Cancel {
            fence: Fence::new(7),
            lease_token: Some(LeaseToken::new(b"tok".to_vec())?),
            expected_state: Some(b"ready".to_vec()),
            expected_version: Some(0),
            error: Some(Vec::new()),
        }),
        _ => return Err("unknown independent fixture".into()),
    })
}

fn item(operation: Operation, id: &[u8]) -> TestResult<PreparedBatchItem> {
    let create_priority = if matches!(operation, Operation::Create(_)) {
        2
    } else {
        0
    };
    Ok(PreparedBatchItem {
        partition: PartitionId::new(b"p\x00".to_vec())?,
        command: CommittedCommand {
            id: RecordId::new(id.to_vec())?,
            now: TimestampMillis::new(11),
            operation,
        },
        create_priority,
    })
}

fn request(name: &str) -> TestResult<PreparedRequest> {
    let mut items = vec![item(operation(name)?, b"r1")?];
    if name.starts_with("batch_") {
        items.push(item(Operation::Claim(claim(b"ready")?), b"r2")?);
        if name == "batch_reverse" {
            items.reverse();
        }
    }
    Ok(PreparedRequest {
        scope: RequestScope::new(
            TenantId::new(vec![0, 255])?,
            GroupId::new(b"G".to_vec())?,
            RecoveryId::new(b"R".to_vec())?,
        ),
        request: RequestId::new(b"Q".to_vec())?,
        intent: ClientIntent::new(
            1,
            std::array::from_fn(|index| u8::try_from(index).unwrap_or_default()),
        )?,
        stamp: CommittedTime::new(HlcTimestamp::new(11, 2)?, 7)?,
        operation: PreparedOperation::Batch(items),
    })
}

fn bytes(hex: &str) -> TestResult<Vec<u8>> {
    if !hex.len().is_multiple_of(2) {
        return Err("odd fixture hex".into());
    }
    (0..hex.len())
        .step_by(2)
        .map(|index| u8::from_str_radix(&hex[index..index + 2], 16).map_err(Into::into))
        .collect()
}

#[test]
fn every_represented_operation_matches_independent_bytes_and_hash() -> TestResult {
    let mut count = 0;
    for line in include_str!("../../../tests/prepared/v1-vectors.tsv").lines() {
        if line.starts_with('#') {
            continue;
        }
        let mut fields = line.split('\t');
        let name = fields.next().ok_or("missing name")?;
        let encoded = bytes(fields.next().ok_or("missing bytes")?)?;
        let digest = bytes(fields.next().ok_or("missing digest")?)?;
        assert_eq!(fields.next(), None);
        let expected = request(name)?;
        let actual = encode_prepared(&expected, CodecLimits::default())?;
        assert_eq!(actual.as_bytes(), encoded, "{name}");
        assert_eq!(actual.digest().as_bytes().as_slice(), digest, "{name}");
        let decoded = decode_prepared(&encoded, CodecLimits::default())?;
        assert_eq!(decoded.request(), &expected, "{name}");
        assert_eq!(decoded.digest(), actual.digest());
        count += 1;
    }
    assert_eq!(count, 15);
    Ok(())
}

#[test]
fn every_truncation_and_trailing_byte_rejects() -> TestResult {
    for line in include_str!("../../../tests/prepared/v1-vectors.tsv").lines() {
        if line.starts_with('#') {
            continue;
        }
        let name = line.split('\t').next().ok_or("missing fixture name")?;
        let encoded = encode_prepared(&request(name)?, CodecLimits::default())?;
        for end in 0..encoded.as_bytes().len() {
            assert!(
                decode_prepared(&encoded.as_bytes()[..end], CodecLimits::default()).is_err(),
                "{name} truncated at{end}"
            );
        }
        let mut extra = encoded.as_bytes().to_vec();
        extra.push(0);
        assert!(
            matches!(
                decode_prepared(&extra, CodecLimits::default()),
                Err(CodecError::TrailingBytes)
            ),
            "{name}"
        );
    }
    Ok(())
}

#[test]
fn unsupported_version_tag_and_invalid_option_reject() -> TestResult {
    let valid = encode_prepared(&request("create_absent")?, CodecLimits::default())?;
    let mut changed = valid.as_bytes().to_vec();
    changed[5] = 2;
    assert!(matches!(
        decode_prepared(&changed, CodecLimits::default()),
        Err(CodecError::UnsupportedVersion)
    ));
    changed.copy_from_slice(valid.as_bytes());
    changed[6] = 255;
    assert!(matches!(
        decode_prepared(&changed, CodecLimits::default()),
        Err(CodecError::UnsupportedTag)
    ));
    changed.copy_from_slice(valid.as_bytes());
    let last = changed.len() - 1;
    changed[last] = 2;
    assert!(matches!(
        decode_prepared(&changed, CodecLimits::default()),
        Err(CodecError::InvalidOptionality)
    ));
    Ok(())
}

#[test]
fn exact_complete_byte_item_and_individual_field_limits_apply_on_both_paths() -> TestResult {
    let request = request("create_empty")?;
    let encoded = encode_prepared(&request, CodecLimits::default())?;
    let exact = CodecLimits::new(AdmissionLimits::new(5, encoded.as_bytes().len(), 1)?);
    assert_eq!(
        encode_prepared(&request, exact)?.as_bytes(),
        encoded.as_bytes()
    );
    assert_eq!(
        decode_prepared(encoded.as_bytes(), exact)?.request(),
        &request
    );
    let short = CodecLimits::new(AdmissionLimits::new(5, encoded.as_bytes().len() - 1, 1)?);
    assert!(matches!(
        encode_prepared(&request, short),
        Err(CodecError::ByteLimit)
    ));
    assert!(matches!(
        decode_prepared(encoded.as_bytes(), short),
        Err(CodecError::ByteLimit)
    ));
    let field = CodecLimits::new(AdmissionLimits::new(4, encoded.as_bytes().len(), 1)?);
    assert!(matches!(
        encode_prepared(&request, field),
        Err(CodecError::ValueLimit)
    ));
    assert!(matches!(
        decode_prepared(encoded.as_bytes(), field),
        Err(CodecError::ValueLimit)
    ));
    Ok(())
}

#[test]
fn counts_and_length_words_reject_before_materialization() -> TestResult {
    let one = encode_prepared(&request("create_empty")?, CodecLimits::default())?;
    let mut changed = one.as_bytes().to_vec();
    changed[80..84].copy_from_slice(&u32::MAX.to_be_bytes());
    assert!(matches!(
        decode_prepared(&changed, CodecLimits::default()),
        Err(CodecError::ItemLimit)
    ));
    changed.copy_from_slice(one.as_bytes());
    changed[80..84].copy_from_slice(&0_u32.to_be_bytes());
    assert!(matches!(
        decode_prepared(&changed, CodecLimits::default()),
        Err(CodecError::ItemLimit)
    ));
    changed.copy_from_slice(one.as_bytes());
    let length_offset = changed.len() - 4;
    changed[length_offset..].copy_from_slice(&u32::MAX.to_be_bytes());
    assert!(matches!(
        decode_prepared(&changed, CodecLimits::default()),
        Err(CodecError::ValueLimit)
    ));
    let two = encode_prepared(&request("batch_forward")?, CodecLimits::default())?;
    let limited = CodecLimits::new(AdmissionLimits::new(64, 1_024, 1)?);
    assert!(matches!(
        decode_prepared(two.as_bytes(), limited),
        Err(CodecError::ItemLimit)
    ));
    assert!(matches!(
        encode_prepared(&request("batch_forward")?, limited),
        Err(CodecError::ItemLimit)
    ));
    Ok(())
}

#[test]
fn actual_namespace_key_values_and_header_are_bound_to_decoded_fields() -> TestResult {
    let encoded = encode_prepared(&request("create_empty")?, CodecLimits::default())?;
    let decoded = decode_prepared(encoded.as_bytes(), CodecLimits::default())?;
    let expected_scope = bytes("4657534300010000000200ff00000001470000000152")?;
    let expected_key = bytes(
        "4657534b0001000000164657534300010000000200ff00000001470000000152000000027000000000027231",
    )?;
    assert_eq!(decoded.isolation_scope(), expected_scope);
    assert_eq!(decoded.state_key(0), Some(expected_key.as_slice()));
    assert_eq!(decoded.state_key(1), None);
    assert_eq!(decoded.value_bytes(0), Some([Some(&b""[..]), None]));
    assert_eq!(decoded.value_bytes(1), None);
    let log = LogIdentity::new(19, 7, AppliedPosition::new(42));
    let header = decoded.header(log);
    assert_eq!(header.identity.log(), log);
    assert_eq!(header.identity.prepared(), encoded.digest());
    assert_eq!(header.scope, &decoded.request().scope);
    assert_eq!(header.request, &decoded.request().request);
    assert_eq!(header.intent, decoded.request().intent);
    assert_eq!(header.stamp, decoded.request().stamp);
    assert_eq!(header.encoded_isolation_scope, expected_scope);
    assert_eq!(header.encoded_entry.as_ptr(), encoded.as_bytes().as_ptr());
    assert_eq!(decoded.encoded().as_ptr(), encoded.as_bytes().as_ptr());
    assert_eq!(
        decoded
            .header(LogIdentity::new(20, 9, AppliedPosition::new(43)))
            .identity
            .prepared(),
        header.identity.prepared()
    );
    Ok(())
}

#[test]
fn malformed_trusted_clock_and_typed_identity_reject() -> TestResult {
    let encoded = encode_prepared(&request("create_absent")?, CodecLimits::default())?;
    let mut changed = encoded.as_bytes().to_vec();
    changed[62..70].copy_from_slice(&(1_u64 << 48).to_be_bytes());
    assert!(matches!(
        decode_prepared(&changed, CodecLimits::default()),
        Err(CodecError::Malformed)
    ));
    changed.copy_from_slice(encoded.as_bytes());
    changed[72..80].copy_from_slice(&12_u64.to_be_bytes());
    assert!(matches!(
        decode_prepared(&changed, CodecLimits::default()),
        Err(CodecError::Malformed)
    ));
    changed.copy_from_slice(encoded.as_bytes());
    changed[7..11].copy_from_slice(&0_u32.to_be_bytes());
    assert!(matches!(
        decode_prepared(&changed, CodecLimits::default()),
        Err(CodecError::Malformed)
    ));
    changed.copy_from_slice(encoded.as_bytes());
    changed[105] = 0;
    assert!(matches!(
        decode_prepared(&changed, CodecLimits::default()),
        Err(CodecError::UnsupportedTag)
    ));
    Ok(())
}

#[test]
fn summed_derived_keys_bound_large_repeated_namespace_amplification() -> TestResult {
    let mut amplified = request("create_absent")?;
    amplified.scope = RequestScope::new(
        TenantId::new(vec![b't'; 2_000])?,
        amplified.scope.group().clone(),
        amplified.scope.recovery().clone(),
    );
    let PreparedOperation::Batch(items) = &mut amplified.operation else {
        return Err("expected batch fixture".into());
    };
    let first = items.first().ok_or("fixture item absent")?.clone();
    items.resize(32, first);
    let encoded = encode_prepared(&amplified, CodecLimits::default())?;
    let limited = CodecLimits::new(AdmissionLimits::new(
        64,
        encoded.as_bytes().len() + 128,
        32,
    )?);
    assert!(encoded.as_bytes().len() < limited.admission().decision_bytes());
    assert!(matches!(
        encode_prepared(&amplified, limited),
        Err(CodecError::ByteLimit)
    ));
    assert!(matches!(
        decode_prepared(encoded.as_bytes(), limited),
        Err(CodecError::ByteLimit)
    ));
    Ok(())
}

#[test]
fn complete_scope_and_state_key_bounds_are_inclusive() -> TestResult {
    let mut scoped = request("create_absent")?;
    scoped.scope = RequestScope::new(
        TenantId::new(vec![b't'; 8_172])?,
        scoped.scope.group().clone(),
        scoped.scope.recovery().clone(),
    );
    let encoded = encode_prepared(&scoped, CodecLimits::default())?;
    assert_eq!(
        decode_prepared(encoded.as_bytes(), CodecLimits::default())?
            .isolation_scope()
            .len(),
        8_192
    );
    scoped.scope = RequestScope::new(
        TenantId::new(vec![b't'; 8_173])?,
        scoped.scope.group().clone(),
        scoped.scope.recovery().clone(),
    );
    assert!(matches!(
        encode_prepared(&scoped, CodecLimits::default()),
        Err(CodecError::ScopeLimit)
    ));
    let mut keyed = request("create_absent")?;
    let PreparedOperation::Batch(items) = &mut keyed.operation else {
        return Err("expected batch fixture".into());
    };
    items[0].partition = PartitionId::new(vec![b'p'; 65_493])?;
    let encoded = encode_prepared(&keyed, CodecLimits::default())?;
    assert_eq!(
        decode_prepared(encoded.as_bytes(), CodecLimits::default())?
            .state_key(0)
            .ok_or("missing key")?
            .len(),
        65_535
    );
    let mut forged = encoded.as_bytes().to_vec();
    forged[84..88].copy_from_slice(&65_494_u32.to_be_bytes());
    forged.insert(88 + 65_493, b'p');
    assert!(matches!(
        decode_prepared(&forged, CodecLimits::default()),
        Err(CodecError::KeyLimit)
    ));
    let PreparedOperation::Batch(items) = &mut keyed.operation else {
        return Err("expected batch fixture".into());
    };
    items[0].partition = PartitionId::new(vec![b'p'; 65_494])?;
    assert!(matches!(
        encode_prepared(&keyed, CodecLimits::default()),
        Err(CodecError::KeyLimit)
    ));
    Ok(())
}

#[test]
fn empty_token_and_unknown_retry_time_and_backoff_tags_reject() -> TestResult {
    let retry = encode_prepared(&request("retry_explicit")?, CodecLimits::default())?;
    let mut changed = retry.as_bytes().to_vec();
    changed[106..110].copy_from_slice(&0_u32.to_be_bytes());
    assert!(matches!(
        decode_prepared(&changed, CodecLimits::default()),
        Err(CodecError::Malformed)
    ));
    changed.copy_from_slice(retry.as_bytes());
    changed[121] = 2;
    assert!(matches!(
        decode_prepared(&changed, CodecLimits::default()),
        Err(CodecError::UnsupportedTag)
    ));
    let policy = encode_prepared(&request("retry_policy")?, CodecLimits::default())?;
    let mut changed = policy.as_bytes().to_vec();
    changed[126] = 4;
    assert!(matches!(
        decode_prepared(&changed, CodecLimits::default()),
        Err(CodecError::UnsupportedTag)
    ));
    Ok(())
}
