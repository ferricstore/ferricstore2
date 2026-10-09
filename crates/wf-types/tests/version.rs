//! Explicit committed values and arithmetic checks independent of wall-clock effects.

#![forbid(unsafe_code)]

use wf_types::version::{Fence, TimestampMillis};

#[test]
fn explicit_timestamps_and_fences_preserve_zero_and_full_u64_values() {
    for value in [0, 1, 255, 65_535, u64::MAX - 1, u64::MAX] {
        assert_eq!(TimestampMillis::new(value).get(), value);
        assert_eq!(Fence::new(value).get(), value);
    }
    assert!(TimestampMillis::new(0) < TimestampMillis::new(1));
    assert!(Fence::new(u64::MAX - 1) < Fence::new(u64::MAX));
}

#[test]
fn timestamp_addition_accepts_the_last_representable_deadline_and_rejects_overflow() {
    let committed = TimestampMillis::new(u64::MAX - 10);
    assert_eq!(committed.checked_add(0), Some(committed));
    assert_eq!(
        committed.checked_add(10),
        Some(TimestampMillis::new(u64::MAX))
    );
    assert_eq!(committed.checked_add(11), None);
    assert_eq!(
        TimestampMillis::new(0).checked_add(u64::MAX),
        Some(TimestampMillis::new(u64::MAX))
    );
    assert_eq!(TimestampMillis::new(u64::MAX).checked_add(1), None);
    assert_eq!(committed.get(), u64::MAX - 10);
}

#[test]
fn exhaustive_low_and_high_timestamp_arithmetic_matches_a_wider_integer_oracle() {
    for base in 0..=255_u64 {
        for time in [base, u64::MAX - base] {
            for delta in 0..=255_u64 {
                let expected = u64::try_from(u128::from(time) + u128::from(delta))
                    .ok()
                    .map(TimestampMillis::new);
                assert_eq!(TimestampMillis::new(time).checked_add(delta), expected);
            }
        }
    }
}

#[test]
fn fencing_advances_exactly_once_and_never_wraps() {
    for current in 0..=255_u64 {
        assert_eq!(
            Fence::new(current).checked_next(),
            Some(Fence::new(current + 1))
        );
    }
    assert_eq!(
        Fence::new(u64::MAX - 1).checked_next(),
        Some(Fence::new(u64::MAX))
    );
    let exhausted = Fence::new(u64::MAX);
    assert_eq!(exhausted.checked_next(), None);
    assert_eq!(exhausted.get(), u64::MAX);
}
