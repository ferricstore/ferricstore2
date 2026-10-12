//! Opaque namespace and committed-clock boundary acceptance.
#![forbid(unsafe_code)]

use wf_types::identity::{GroupId, PartitionId, RecoveryId, RequestId, TenantId};
use wf_types::version::{AppliedPosition, CommittedTime, HlcTimestamp, Revision};

type TestResult = Result<(), Box<dyn std::error::Error>>;

#[test]
fn opaque_scope_identities_preserve_bytes_and_reject_empty_or_oversize() -> TestResult {
    let bytes = vec![0, 255, b':'];
    assert_eq!(TenantId::new(bytes.clone())?.as_bytes(), bytes);
    assert_eq!(GroupId::new(bytes.clone())?.as_bytes(), bytes);
    assert_eq!(RecoveryId::new(bytes.clone())?.as_bytes(), bytes);
    assert_eq!(RequestId::new(bytes.clone())?.as_bytes(), bytes);
    assert_eq!(PartitionId::new(bytes.clone())?.as_bytes(), bytes);
    assert!(TenantId::new(vec![]).is_err());
    assert!(GroupId::new(vec![]).is_err());
    assert!(RecoveryId::new(vec![]).is_err());
    assert!(RequestId::new(vec![]).is_err());
    assert!(PartitionId::new(vec![]).is_err());
    assert!(GroupId::new(vec![1; 257]).is_err());
    assert!(RequestId::new(vec![1; 257]).is_err());
    assert!(RecoveryId::new(vec![1; 257]).is_err());
    assert!(TenantId::new(vec![1; 8_193]).is_err());
    assert!(PartitionId::new(vec![1; 65_536]).is_err());
    assert!(GroupId::new(vec![1; 256]).is_ok());
    assert!(TenantId::new(vec![1; 8_192]).is_ok());
    assert!(PartitionId::new(vec![1; 65_535]).is_ok());
    assert!(RequestId::new(vec![1])? < RequestId::new(vec![255])?);
    Ok(())
}

#[test]
fn validated_committed_metadata_preserves_source_clock_bounds() -> TestResult {
    let clock = HlcTimestamp::new(0, 0)?;
    assert_eq!(CommittedTime::new(clock, 0)?.wall().get(), 0);
    let maximum = HlcTimestamp::new((1_u64 << 48) - 1, u16::MAX)?;
    assert_eq!(maximum.physical().get(), (1_u64 << 48) - 1);
    assert_eq!(maximum.logical(), u16::MAX);
    assert!(HlcTimestamp::new(1_u64 << 48, 0).is_err());
    assert!(CommittedTime::new(clock, 1).is_err());
    assert!(HlcTimestamp::new(10, 1)? > HlcTimestamp::new(10, 0)?);
    assert!(HlcTimestamp::new(11, 0)? > HlcTimestamp::new(10, u16::MAX)?);
    Ok(())
}

#[test]
fn distinct_position_and_revision_counters_never_wrap() {
    assert_eq!(
        AppliedPosition::new(0).checked_next(),
        Some(AppliedPosition::new(1))
    );
    assert_eq!(Revision::new(0).checked_next(), Some(Revision::new(1)));
    assert_eq!(AppliedPosition::new(u64::MAX).checked_next(), None);
    assert_eq!(Revision::new(u64::MAX).checked_next(), None);
}
