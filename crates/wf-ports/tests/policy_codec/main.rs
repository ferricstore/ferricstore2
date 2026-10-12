//! A-N8 independent policy bytes, value binding and fresh/replay dispatch controls.
#![forbid(unsafe_code)]

mod dispatch;

use std::collections::BTreeMap;

use wf_domain::policy::definition::{
    BackoffPatch, FieldPatch, PolicyPatch, PolicyVersion, RetryPatch, StateChange, StateMode,
    StatePolicy, StatePolicyPatch,
};
use wf_domain::policy::installation::{
    InstallCommand, InstallMode, MAX_POLICY_GENERATION, PolicyLimits, prepare_install,
};
use wf_domain::policy::restoration::{PolicySnapshotData, restore_candidate};
use wf_domain::policy::{Backoff, BackoffKind, RetryOverride};
use wf_domain::retention::RetentionOverride;
use wf_domain::workflow::admission::AdmissionLimits;
use wf_domain::workflow::request::{ClientIntent, RequestScope};
use wf_ports::consensus::{PreparedOperation, PreparedRequest};
use wf_ports::log::{CodecError, CodecLimits, decode_prepared, encode_prepared, prepare_policy};
use wf_types::identity::{GroupId, RecoveryId, RequestId, TenantId};
use wf_types::version::{AppliedPosition, CommittedTime, HlcTimestamp, LogIdentity};

type TestError = Box<dyn std::error::Error>;
type TestResult<T = ()> = Result<T, TestError>;

fn timing(kind: BackoffKind, base: u64, maximum: u64, jitter: u8) -> BackoffPatch {
    BackoffPatch {
        kind: Some(kind),
        base_ms: Some(base),
        max_ms: Some(maximum),
        jitter_pct: Some(jitter),
    }
}

fn rich() -> InstallCommand {
    InstallCommand {
        workflow_type: vec![b't', 255],
        expected_generation: Some(0),
        mode: InstallMode::Replace,
        patch: PolicyPatch {
            version: FieldPatch::Set(PolicyVersion::Bytes(vec![0, 255])),
            max_active_ms: FieldPatch::Set(31_536_000_000),
            retry: FieldPatch::Set(RetryPatch {
                max_retries: Some(0),
                backoff: Some(timing(BackoffKind::Fixed, 0, 7, 100)),
                exhausted_to: Some(vec![255]),
            }),
            retention: FieldPatch::Set(RetentionOverride {
                ttl_ms: Some(1),
                history_max_events: Some(1),
            }),
            states: BTreeMap::from([
                (
                    b"ready".to_vec(),
                    StateChange::Patch(StatePolicyPatch {
                        mode: FieldPatch::Set(StateMode::Parallel),
                        retry: FieldPatch::Set(RetryPatch {
                            max_retries: Some(1_000),
                            backoff: Some(timing(BackoffKind::None, 0, 0, 0)),
                            exhausted_to: Some(b"done".to_vec()),
                        }),
                        retention: FieldPatch::Set(RetentionOverride {
                            ttl_ms: Some(31_536_000_000),
                            history_max_events: Some(1_000_000),
                        }),
                    }),
                ),
                (
                    vec![0, 255],
                    StateChange::Patch(StatePolicyPatch {
                        mode: FieldPatch::Set(StateMode::Fifo),
                        retention: FieldPatch::Set(RetentionOverride::default()),
                        ..StatePolicyPatch::default()
                    }),
                ),
            ]),
        },
    }
}

fn command(name: &str) -> TestResult<InstallCommand> {
    let mut command = InstallCommand::low_level(vec![b't', 255], PolicyPatch::default());
    match name {
        "policy_empty" => {}
        "policy_replace" => return Ok(rich()),
        "policy_number_zero" | "policy_number_max" => {
            command.expected_generation = Some(0);
            command.mode = InstallMode::Replace;
            command.patch.version =
                FieldPatch::Set(PolicyVersion::Number(if name == "policy_number_zero" {
                    0
                } else {
                    u64::MAX
                }));
        }
        "policy_clear" => {
            command.expected_generation = Some(1);
            command.patch = PolicyPatch {
                version: FieldPatch::Clear,
                max_active_ms: FieldPatch::Clear,
                retry: FieldPatch::Clear,
                retention: FieldPatch::Clear,
                states: BTreeMap::from([
                    (b"ready".to_vec(), StateChange::Remove),
                    (
                        vec![0, 255],
                        StateChange::Patch(StatePolicyPatch {
                            mode: FieldPatch::Clear,
                            retry: FieldPatch::Set(RetryPatch {
                                backoff: Some(BackoffPatch::default()),
                                ..RetryPatch::default()
                            }),
                            retention: FieldPatch::Clear,
                        }),
                    ),
                ]),
            };
        }
        "policy_linear" | "policy_exponential" => {
            command.patch.retry = FieldPatch::Set(RetryPatch {
                backoff: Some(timing(
                    if name == "policy_linear" {
                        BackoffKind::Linear
                    } else {
                        BackoffKind::Exponential
                    },
                    11,
                    100,
                    0,
                )),
                ..RetryPatch::default()
            });
        }
        _ => return Err("unknown independent policy fixture".into()),
    }
    Ok(command)
}

fn request(name: &str) -> TestResult<PreparedRequest> {
    let command = command(name)?;
    let limits = CodecLimits::default();
    let high_water = if name == "policy_number_max" {
        MAX_POLICY_GENERATION - 1
    } else {
        0
    };
    let candidate = if name == "policy_clear" {
        // Mathematical/private bytes preserve the raw Clear command. Source
        // admission rejects its mode null; this valid normalized value is
        // explicitly restored, never claimed to result from that installation.
        restore_candidate(
            2,
            PolicySnapshotData {
                workflow_type: command.workflow_type.clone(),
                version: None,
                max_active_ms: None,
                retry: RetryOverride::default(),
                retention: RetentionOverride::default(),
                states: BTreeMap::from([(
                    vec![0, 255],
                    StatePolicy {
                        mode: None,
                        retry: RetryOverride {
                            backoff: Some(Backoff::default()),
                            ..RetryOverride::default()
                        },
                        retention: RetentionOverride::default(),
                    },
                )]),
            },
            limits.policy_limits(),
        )?
    } else {
        prepare_install(None, high_water, &command, limits.policy_limits())?
    };
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
        operation: PreparedOperation::InstallPolicy {
            command,
            value: Box::new(prepare_policy(candidate, limits)?),
        },
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
fn independent_policy_bytes_digests_fields_and_original_owners_match() -> TestResult {
    let mut count = 0;
    for row in include_str!("../../../../tests/prepared/policy-v1-vectors.tsv")
        .lines()
        .filter(|row| !row.starts_with('#'))
    {
        let fields: Vec<_> = row.split('\t').collect();
        assert_eq!(fields.len(), 4);
        let request = request(fields[0])?;
        let expected = bytes(fields[1])?;
        let encoded = encode_prepared(&request, CodecLimits::default())?;
        assert_eq!(encoded.as_bytes(), expected, "{}", fields[0]);
        assert_eq!(
            encoded.digest().as_bytes().as_slice(),
            bytes(fields[2])?,
            "{}",
            fields[0]
        );
        let decoded = decode_prepared(&expected, CodecLimits::default())?;
        assert_eq!(decoded.request(), &request, "{}", fields[0]);
        assert_eq!(decoded.encoded().as_ptr(), expected.as_ptr());
        assert_eq!(decoded.digest(), encoded.digest());
        assert_eq!(decoded.state_key(0), None);
        assert_eq!(decoded.value_bytes(0), None);
        let log = LogIdentity::new(19, 7, AppliedPosition::new(42));
        let envelope = decoded
            .policy_envelope(log)
            .ok_or("missing policy envelope")?;
        assert_eq!(envelope.prepared.encoded(), bytes(fields[3])?);
        assert_eq!(envelope.header.identity.log(), log);
        assert_eq!(envelope.header.identity.prepared(), encoded.digest());
        assert_eq!(envelope.header.encoded_entry.as_ptr(), expected.as_ptr());
        assert_eq!(
            envelope.command.workflow_type,
            envelope.prepared.policy().workflow_type()
        );
        count += 1;
    }
    assert_eq!(count, 7);
    Ok(())
}

#[test]
fn policy_vectors_reject_every_truncation_and_trailing_byte() -> TestResult {
    for row in include_str!("../../../../tests/prepared/policy-v1-vectors.tsv")
        .lines()
        .filter(|row| !row.starts_with('#'))
    {
        let fields: Vec<_> = row.split('\t').collect();
        let encoded = bytes(fields[1])?;
        for length in 0..encoded.len() {
            assert!(
                decode_prepared(&encoded[..length], CodecLimits::default()).is_err(),
                "{} truncation{length}",
                fields[0]
            );
        }
        let mut trailing = encoded;
        trailing.push(0);
        assert!(matches!(
            decode_prepared(&trailing, CodecLimits::default()),
            Err(CodecError::TrailingBytes)
        ));
    }
    Ok(())
}

#[test]
fn complete_request_and_policy_state_profile_boundaries_apply_both_ways() -> TestResult {
    assert_eq!(CodecLimits::default().policy_limits().max_states(), 16_384);
    assert_eq!(
        CodecLimits::new(AdmissionLimits::default()).policy_limits(),
        CodecLimits::default().policy_limits()
    );
    let request = request("policy_replace")?;
    let encoded = encode_prepared(&request, CodecLimits::default())?;
    let exact = CodecLimits::with_policy_limits(
        AdmissionLimits::new(1, encoded.as_bytes().len(), 1)?,
        PolicyLimits::new(2)?,
    );
    assert!(encode_prepared(&request, exact).is_ok());
    assert!(decode_prepared(encoded.as_bytes(), exact).is_ok());
    let bytes_short = CodecLimits::with_policy_limits(
        AdmissionLimits::new(1, encoded.as_bytes().len() - 1, 1)?,
        PolicyLimits::new(2)?,
    );
    assert!(matches!(
        encode_prepared(&request, bytes_short),
        Err(CodecError::ByteLimit)
    ));
    assert!(matches!(
        decode_prepared(encoded.as_bytes(), bytes_short),
        Err(CodecError::ByteLimit)
    ));
    let states_short =
        CodecLimits::with_policy_limits(AdmissionLimits::default(), PolicyLimits::new(1)?);
    assert!(matches!(
        encode_prepared(&request, states_short),
        Err(CodecError::ItemLimit)
    ));
    assert!(matches!(
        decode_prepared(encoded.as_bytes(), states_short),
        Err(CodecError::ItemLimit)
    ));
    Ok(())
}

#[test]
fn legacy_length_binding_cannot_certify_fabricated_policy_bytes() -> TestResult {
    let mut request = request("policy_empty")?;
    let limits = CodecLimits::default();
    let PreparedOperation::InstallPolicy { command, value } = &mut request.operation else {
        return Err("missing policy operation".into());
    };
    let mut forged = value.encoded().to_vec();
    forged[13] = 2;
    **value = prepare_install(None, 0, command, limits.policy_limits())?.bind_bytes(
        b"length-only-snapshot",
        forged,
        limits.policy_limits(),
    )?;
    assert!(matches!(
        encode_prepared(&request, limits),
        Err(CodecError::Malformed)
    ));
    let PreparedOperation::InstallPolicy { command, value } = &mut request.operation else {
        return Err("missing policy operation".into());
    };
    **value = prepare_install(None, 0, command, limits.policy_limits())?.bind_bytes(
        b"length-only-snapshot",
        b"fabricated-policy".to_vec(),
        limits.policy_limits(),
    )?;
    assert!(matches!(
        encode_prepared(&request, limits),
        Err(CodecError::Malformed)
    ));
    Ok(())
}

#[test]
fn malformed_policy_generations_versions_tags_and_counts_reject() -> TestResult {
    let request = request("policy_empty")?;
    let encoded = encode_prepared(&request, CodecLimits::default())?;
    assert_eq!(&encoded.as_bytes()[100..106], b"FWPV\x00\x01");
    assert_eq!(&encoded.as_bytes()[118..124], b"FWPS\x00\x01");
    for generation in [0, MAX_POLICY_GENERATION + 1] {
        let mut changed = encoded.as_bytes().to_vec();
        changed[106..114].copy_from_slice(&generation.to_be_bytes());
        assert!(matches!(
            decode_prepared(&changed, CodecLimits::default()),
            Err(CodecError::Malformed)
        ));
    }
    for (offset, value, expected) in [
        (88, 3, CodecError::UnsupportedTag),
        (105, 2, CodecError::UnsupportedVersion),
        (123, 2, CodecError::UnsupportedVersion),
        (130, 2, CodecError::InvalidOptionality),
    ] {
        let mut changed = encoded.as_bytes().to_vec();
        changed[offset] = value;
        assert!(
            matches!(decode_prepared(&changed, CodecLimits::default()), Err(error) if error == expected)
        );
    }
    for offset in [92, 137] {
        let mut changed = encoded.as_bytes().to_vec();
        changed[offset..offset + 4].copy_from_slice(&u32::MAX.to_be_bytes());
        assert!(matches!(
            decode_prepared(&changed, CodecLimits::default()),
            Err(CodecError::ItemLimit)
        ));
    }
    let mut changed = encoded.as_bytes().to_vec();
    changed[114..118].copy_from_slice(&u32::MAX.to_be_bytes());
    assert!(matches!(
        decode_prepared(&changed, CodecLimits::default()),
        Err(CodecError::ValueLimit)
    ));
    Ok(())
}

#[test]
fn complete_snapshot_limit_counts_prefixes_before_encoding_and_copying() -> TestResult {
    use wf_domain::policy::installation::MAX_POLICY_SNAPSHOT_BYTES;

    for extra in [0, 1] {
        let command = InstallCommand::low_level(
            vec![b't'; MAX_POLICY_SNAPSHOT_BYTES - 21 + extra],
            PolicyPatch::default(),
        );
        let candidate = prepare_install(None, 0, &command, CodecLimits::default().policy_limits())?;
        let value = prepare_policy(candidate, CodecLimits::default());
        if extra == 1 {
            assert!(matches!(value, Err(CodecError::ByteLimit)));
            continue;
        }
        let value = value?;
        assert_eq!(value.encoded().len(), MAX_POLICY_SNAPSHOT_BYTES + 18);
        let mut request = request("policy_empty")?;
        request.operation = PreparedOperation::InstallPolicy {
            command,
            value: Box::new(value),
        };
        let encoded = encode_prepared(&request, CodecLimits::default())?;
        assert_eq!(
            decode_prepared(encoded.as_bytes(), CodecLimits::default())?.request(),
            &request
        );
    }
    Ok(())
}

#[test]
fn out_of_order_and_duplicate_normalized_state_keys_reject() -> TestResult {
    let encoded = encode_prepared(&request("policy_replace")?, CodecLimits::default())?;
    let original = encoded.as_bytes();
    let envelope = original
        .windows(6)
        .position(|bytes| bytes == b"FWPV\x00\x01")
        .ok_or("missing FWPV")?;
    let key = original
        .windows(6)
        .rposition(|bytes| bytes == b"\x00\x00\x00\x02\x00\xff")
        .ok_or("missing state key")?;
    assert!(key > envelope + 18);
    let mut changed = original.to_vec();
    changed[key + 4..key + 6].copy_from_slice(b"zz");
    assert!(matches!(
        decode_prepared(&changed, CodecLimits::default()),
        Err(CodecError::Malformed)
    ));
    let mut duplicate = original.to_vec();
    duplicate.splice(key + 4..key + 6, b"ready".iter().copied());
    duplicate[key..key + 4].copy_from_slice(&5_u32.to_be_bytes());
    for offset in [envelope - 4, envelope + 14] {
        let length = u32::from_be_bytes(original[offset..offset + 4].try_into()?);
        duplicate[offset..offset + 4].copy_from_slice(&(length + 3).to_be_bytes());
    }
    assert!(matches!(
        decode_prepared(&duplicate, CodecLimits::default()),
        Err(CodecError::Malformed)
    ));
    Ok(())
}

#[test]
fn invalid_normalized_scalars_reject_before_materializing_policy_values() -> TestResult {
    let encoded = encode_prepared(&request("policy_replace")?, CodecLimits::default())?;
    let original = encoded.as_bytes();
    let snapshot = original
        .windows(6)
        .position(|bytes| bytes == b"FWPS\x00\x01")
        .ok_or("missing FWPS")?;
    for (offset, bytes) in [
        (6, 0_u32.to_be_bytes().to_vec()),
        (14, 0_u32.to_be_bytes().to_vec()),
        (21, 0_u64.to_be_bytes().to_vec()),
        (30, 1_001_u16.to_be_bytes().to_vec()),
        (34, 2_592_000_001_u64.to_be_bytes().to_vec()),
        (42, 2_592_000_001_u64.to_be_bytes().to_vec()),
        (50, vec![101]),
        (52, 0_u32.to_be_bytes().to_vec()),
        (58, 0_u64.to_be_bytes().to_vec()),
        (67, 0_u32.to_be_bytes().to_vec()),
        (75, 0_u32.to_be_bytes().to_vec()),
    ] {
        let mut changed = original.to_vec();
        changed[snapshot + offset..snapshot + offset + bytes.len()].copy_from_slice(&bytes);
        assert!(
            matches!(
                decode_prepared(&changed, CodecLimits::default()),
                Err(CodecError::Malformed)
            ),
            "normalized field {offset}"
        );
    }
    let mut changed = original.to_vec();
    changed[snapshot + 82] = 2;
    assert!(matches!(
        decode_prepared(&changed, CodecLimits::default()),
        Err(CodecError::UnsupportedTag)
    ));
    Ok(())
}
