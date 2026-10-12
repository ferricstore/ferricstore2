//! Separate policy byte, encoded-envelope and finite occurrence work budgets.
#![forbid(unsafe_code)]

use wf_domain::policy::installation::{PolicyError, validate_snapshot_batch};

#[test]
fn repeated_empty_snapshots_cannot_bypass_finite_occurrence_work() {
    let empty: &[u8] = &[];
    let occurrences = vec![empty; 100_001];
    assert_eq!(
        validate_snapshot_batch(&occurrences),
        Err(PolicyError::SnapshotBatchItems)
    );
}

#[test]
fn actual_policy_snapshot_batch_has_inclusive_four_mib_boundary() {
    let exact = vec![0; 4 * 1_024 * 1_024];
    assert_eq!(validate_snapshot_batch(&[&exact]), Ok(()));
    assert_eq!(
        validate_snapshot_batch(&[&exact, &[1]]),
        Err(PolicyError::SnapshotBatchTooLarge)
    );
}
