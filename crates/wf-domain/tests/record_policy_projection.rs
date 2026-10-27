//! Runtime projections retain complete policy backing and independent owners.

#![forbid(unsafe_code)]

use wf_domain::policy::complete::{
    CompletePolicyData, CompletePolicyEncodingError, CompleteStatePolicyData, PolicySchemaCharge,
    PolicySchemaError, PolicySchemaLimits, decode_complete_policy_value_v2,
    prepare_complete_policy_value_v2, project_record_batch_policy,
    record_batch_policy_projection_bytes, restore_complete_policy,
};
use wf_domain::policy::definition::{PolicyVersion, StateMode};
use wf_domain::policy::{Backoff, BackoffKind, RetryOverride, resolve};

// Independently packed FWPV2/FWPS2, generation7. No candidate codec produced it.
const RICH: &str = "46575056000200000000000000070000007446575053000200000001720101ffffffffffffffff010000000757b12c00010100000101000000000000000a000000000000001e0001000000066661696c656401000000000001000000017101010100000001000001000000000000000100000000010000000570686173650100000000000000";

fn hex(value: &str) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    value
        .as_bytes()
        .as_chunks::<2>()
        .0
        .iter()
        .map(|pair| Ok(u8::from_str_radix(std::str::from_utf8(pair)?, 16)?))
        .collect()
}

#[test]
fn record_policy_projection_preserves_mode_retry_and_generation()
-> Result<(), Box<dyn std::error::Error>> {
    let encoded = hex(RICH)?;
    assert_eq!(encoded.len(), 134);
    let limits = PolicySchemaLimits::default();
    let mut charge = PolicySchemaCharge::new(limits);
    let value = decode_complete_policy_value_v2(&encoded, limits, &mut charge)?;
    let data_address = std::ptr::from_ref(value.data());
    let bytes_address = value.encoded().as_ptr();
    assert_eq!(record_batch_policy_projection_bytes(&value)?, 1_290);
    let previous_work = charge.g();
    let (mode, retry) = project_record_batch_policy(&value, limits, &mut charge, 1_290)?;
    assert!(charge.g() > previous_work);
    assert_eq!(mode.generation(), 7);
    assert_eq!(mode.policy().workflow_type(), b"r");
    assert_eq!(mode.policy().states().len(), 1);
    assert_eq!(mode.policy().state_mode(b"q"), StateMode::Fifo);
    assert_eq!(mode.policy().state_mode(b"absent"), StateMode::Parallel);
    assert_eq!(retry.workflow_type(), b"r");
    assert_eq!(retry.state_retry(b"q"), Some(&RetryOverride::default()));
    assert_eq!(retry.state_retry(b"absent"), None);
    assert_eq!(retry.type_retry().max_retries, Some(0));
    assert_eq!(
        retry.type_retry().exhausted_to.as_deref(),
        Some(b"failed".as_slice())
    );
    assert_eq!(
        retry.type_retry().backoff,
        Some(Backoff {
            kind: BackoffKind::Fixed,
            base_ms: 10,
            max_ms: 30,
            jitter_pct: 0,
        })
    );
    // The mode projection is temporary, never a replacement complete snapshot.
    assert_eq!(mode.policy().version(), None);
    assert_eq!(mode.policy().max_active_ms(), None);
    assert_eq!(std::ptr::from_ref(value.data()), data_address);
    assert_eq!(value.encoded().as_ptr(), bytes_address);
    assert_eq!(value.encoded(), encoded);
    assert_rich_backing(value.data())?;
    drop(value);
    drop(encoded);
    assert_eq!(mode.policy().state_mode(b"q"), StateMode::Fifo);
    let resolved = resolve(Some(&retry), b"r", b"q", &RetryOverride::default())?;
    assert_eq!(resolved.max_retries, 0);
    assert_eq!(resolved.exhausted_to, b"failed");
    assert_eq!(resolved.backoff.base_ms, 10);
    all_states()?;
    Ok(())
}

fn all_states() -> Result<(), Box<dyn std::error::Error>> {
    let states = [
        (b"a".to_vec(), Some(StateMode::Fifo)),
        (b"b".to_vec(), Some(StateMode::Parallel)),
        (b"c".to_vec(), None),
    ]
    .into_iter()
    .map(|(name, mode)| {
        let retry = (name == b"b").then(|| RetryOverride {
            max_retries: Some(5),
            backoff: None,
            exhausted_to: Some(b"rescue".to_vec()),
        });
        (
            name,
            CompleteStatePolicyData {
                mode,
                retry,
                retention: None,
                governance: None,
            },
        )
    })
    .collect();
    let data = CompletePolicyData {
        workflow_type: b"multi".to_vec(),
        version: None,
        max_active_ms: None,
        retry: None,
        retention: None,
        states,
        indexed_attributes: None,
        indexed_state_meta: None,
        governance: None,
    };
    let limits = PolicySchemaLimits::default();
    let mut charge = PolicySchemaCharge::new(limits);
    let candidate = restore_complete_policy(9, data, limits, &mut charge)?;
    let value = prepare_complete_policy_value_v2(candidate, limits, &mut charge)?;
    // Root1024 + two type copies10 + three state pairs(256+2) + target6.
    assert_eq!(record_batch_policy_projection_bytes(&value)?, 1_814);
    let (mode, retry) = project_record_batch_policy(&value, limits, &mut charge, 1_814)?;
    drop(value);
    assert_eq!(mode.generation(), 9);
    assert_eq!(mode.policy().states().len(), 3);
    assert_eq!(mode.policy().state_mode(b"a"), StateMode::Fifo);
    assert_eq!(mode.policy().state_mode(b"b"), StateMode::Parallel);
    assert!(mode.policy().states().contains_key(b"c".as_slice()));
    for state in [b"a", b"b", b"c"] {
        assert!(retry.state_retry(state).is_some());
    }
    assert_eq!(
        retry.state_retry(b"b").ok_or("retry missing")?.max_retries,
        Some(5)
    );
    assert_eq!(
        retry
            .state_retry(b"b")
            .ok_or("retry missing")?
            .exhausted_to
            .as_deref(),
        Some(b"rescue".as_slice())
    );
    Ok(())
}

fn maximum_policy_bytes() -> Vec<u8> {
    // Snapshot6 + type5 + optional-version(1+tag1+length4+262117) + tail10.
    let mut bytes = b"FWPV\0\x02\0\0\0\0\0\0\0\x07\0\x04\0\0FWPS\0\x02\0\0\0\x01r\x01\0".to_vec();
    bytes.extend_from_slice(&262_117_u32.to_be_bytes());
    bytes.resize(bytes.len() + 262_117 + 10, 0);
    bytes
}

#[test]
fn record_policy_projection_admits_exact_bytes_and_preserves_backing()
-> Result<(), Box<dyn std::error::Error>> {
    let limits = PolicySchemaLimits::default();
    let mut charge = PolicySchemaCharge::new(limits);
    let rich = decode_complete_policy_value_v2(&hex(RICH)?, limits, &mut charge)?;
    for cap in [0, 1_289] {
        assert!(matches!(
            project_record_batch_policy(&rich, limits, &mut charge, cap),
            Err(CompletePolicyEncodingError::ByteLimit)
        ));
    }
    let other = PolicySchemaLimits::new(limits.policy_limits(), 1)?;
    assert!(matches!(
        project_record_batch_policy(&rich, other, &mut charge, 1_290),
        Err(CompletePolicyEncodingError::Schema(
            PolicySchemaError::ProfileMismatch
        ))
    ));
    let encoded = maximum_policy_bytes();
    assert_eq!(encoded.len(), 262_162);
    let value = decode_complete_policy_value_v2(&encoded, limits, &mut charge)?;
    assert_eq!(value.encoded(), encoded);
    assert_eq!(record_batch_policy_projection_bytes(&value)?, 1_026);
    let (mode, retry) = project_record_batch_policy(&value, limits, &mut charge, 1_026)?;
    assert_eq!(mode.generation(), 7);
    assert_eq!(mode.policy().workflow_type(), b"r");
    assert!(mode.policy().states().is_empty());
    assert_eq!(retry.type_retry(), &RetryOverride::default());
    assert_eq!(value.encoded(), encoded);
    let Some(PolicyVersion::Bytes(version)) = &value.data().version else {
        return Err("maximum byte version lost".into());
    };
    assert_eq!(version.len(), 262_117);
    assert!(version.iter().all(|byte| *byte == 0));
    Ok(())
}

fn assert_rich_backing(data: &CompletePolicyData) -> Result<(), Box<dyn std::error::Error>> {
    assert_eq!(data.version, Some(PolicyVersion::Number(u64::MAX)));
    assert_eq!(data.max_active_ms, Some(31_536_000_000));
    assert!(data.retention.is_some());
    assert_eq!(data.indexed_attributes.as_deref(), Some([].as_slice()));
    assert_eq!(
        data.indexed_state_meta.as_deref(),
        Some(b"phase".as_slice())
    );
    assert!(data.governance.is_some());
    let state = data.states.get(b"q".as_slice()).ok_or("state missing")?;
    assert!(state.retry.is_some());
    assert!(state.retention.is_some());
    assert!(state.governance.is_some());
    Ok(())
}
