//! Raw occurrence admission, external leaf ownership and invalid caller values.

use super::TestResult;
use wf_domain::claim::caller::{
    ClaimCallerError, ClaimCallerLimits, ClaimDueInput, ClaimPartitionsInput, ClaimStatesInput,
    ClaimValuesInput, normalize_claim_due,
};
use wf_domain::workflow::admission::MAX_INLINE_BYTES;

#[test]
fn raw_counts_and_occurrence_bytes_precede_dedup() -> TestResult {
    let limits = ClaimCallerLimits::default();
    assert_eq!(
        (
            limits.max_components(),
            limits.max_component_bytes(),
            limits.max_raw_bytes()
        ),
        (100_000, 1_048_576, 16_777_216)
    );
    for (count, component, aggregate) in [
        (0, 1, 1),
        (1, 0, 1),
        (1, 1, 0),
        (100_001, 1, 1),
        (1, MAX_INLINE_BYTES + 1, 1),
    ] {
        assert_eq!(
            ClaimCallerLimits::new(count, component, aggregate),
            Err(ClaimCallerError::InvalidLimits)
        );
    }
    assert_eq!(
        ClaimCallerLimits::new(100_000, MAX_INLINE_BYTES, usize::MAX)?.max_raw_bytes(),
        usize::MAX
    );
    let names = [b"".as_slice(), b""];
    let tiny = ClaimCallerLimits::new(1, 1, 1)?;
    for input in [
        ClaimDueInput {
            states: ClaimStatesInput::Many(&names),
            ..ClaimDueInput::new(b"too large", b"")
        },
        ClaimDueInput {
            states: ClaimStatesInput::One(b"too large"),
            partitions: ClaimPartitionsInput::Many(&names),
            ..ClaimDueInput::new(b"", b"")
        },
        ClaimDueInput {
            states: ClaimStatesInput::One(b"too large"),
            values: ClaimValuesInput::Names(&names),
            ..ClaimDueInput::new(b"", b"")
        },
    ] {
        assert_eq!(
            normalize_claim_due(input, tiny).err(),
            Some(ClaimCallerError::ComponentCount)
        );
    }
    let empty_occurrences = vec![b"".as_slice(); 100_001];
    let exact = normalize_claim_due(
        ClaimDueInput {
            values: ClaimValuesInput::Names(&empty_occurrences[..100_000]),
            ..ClaimDueInput::new(b"", b"")
        },
        limits,
    )?;
    assert_eq!(exact.values().1, [b"".as_slice()]);
    assert_eq!(
        normalize_claim_due(
            ClaimDueInput {
                values: ClaimValuesInput::Names(&empty_occurrences),
                ..ClaimDueInput::new(b"", b"")
            },
            limits
        )
        .err(),
        Some(ClaimCallerError::ComponentCount)
    );
    let states = [b"qq".as_slice(), b"qq"];
    let keys = [b"pp".as_slice(), b"pp"];
    let values = [b"v".as_slice(), b"v"];
    for (maximum, expected) in [
        (21, None),
        (
            20,
            Some(ClaimCallerError::RawBytes {
                bytes: 21,
                maximum: 20,
            }),
        ),
    ] {
        let value = normalize_claim_due(
            ClaimDueInput {
                states: ClaimStatesInput::Many(&states),
                partitions: ClaimPartitionsInput::Many(&keys),
                values: ClaimValuesInput::Names(&values),
                governance_limit_scope: Some(b"g"),
                ..ClaimDueInput::new(b"type", b"worker")
            },
            ClaimCallerLimits::new(2, 6, maximum)?,
        );
        assert_eq!(value.err(), expected);
    }
    Ok(())
}

#[test]
fn opaque_leaves_borrow_original_owners() -> TestResult {
    let workflow_type = vec![b't', 0, 255];
    let worker = vec![255, 0, b'w'];
    let governance = vec![0, b'g', 255];
    let state_first = vec![255, 0, b's'];
    let state_last = state_first.clone();
    let state_other = vec![b'o'];
    let key_first = vec![255, 0, b'p'];
    let key_last = key_first.clone();
    let value_first = vec![0, 255, b'v'];
    let value_last = value_first.clone();
    let states = [
        state_first.as_slice(),
        state_other.as_slice(),
        state_last.as_slice(),
    ];
    let keys = [key_first.as_slice(), key_last.as_slice()];
    let values = [value_first.as_slice(), value_last.as_slice()];
    let value = normalize_claim_due(
        ClaimDueInput {
            states: ClaimStatesInput::Many(&states),
            partitions: ClaimPartitionsInput::Many(&keys),
            values: ClaimValuesInput::Names(&values),
            governance_limit_scope: Some(&governance),
            ..ClaimDueInput::new(&workflow_type, &worker)
        },
        ClaimCallerLimits::default(),
    )?;
    assert!(std::ptr::eq(
        value.workflow_type(),
        workflow_type.as_slice()
    ));
    assert!(std::ptr::eq(value.worker(), worker.as_slice()));
    assert!(std::ptr::eq(
        value.governance_limit_scope().ok_or("missing scope")?,
        governance.as_slice()
    ));
    assert_eq!(value.states().1.len(), 2);
    assert!(std::ptr::eq(value.states().1[0], state_other.as_slice()));
    assert!(std::ptr::eq(value.states().1[1], state_last.as_slice()));
    assert!(std::ptr::eq(value.partitions().1[0], key_first.as_slice()));
    assert!(std::ptr::eq(value.values().1[0], value_first.as_slice()));
    let singleton = normalize_claim_due(
        ClaimDueInput {
            states: ClaimStatesInput::One(&state_first),
            partitions: ClaimPartitionsInput::One(&key_first),
            ..ClaimDueInput::new(&workflow_type, &worker)
        },
        ClaimCallerLimits::default(),
    )?;
    assert!(std::ptr::eq(
        singleton.states().1[0],
        state_first.as_slice()
    ));
    assert!(std::ptr::eq(
        singleton.partitions().1[0],
        key_first.as_slice()
    ));
    let component = vec![7; 1_048_577];
    let boundary = &component[..1_048_576];
    let exact = normalize_claim_due(
        ClaimDueInput::new(boundary, b""),
        ClaimCallerLimits::default(),
    )?;
    assert!(std::ptr::eq(exact.workflow_type(), boundary));
    assert_eq!(
        normalize_claim_due(
            ClaimDueInput::new(&component, b""),
            ClaimCallerLimits::default()
        )
        .err(),
        Some(ClaimCallerError::ValueTooLarge {
            bytes: 1_048_577,
            maximum: 1_048_576
        })
    );
    Ok(())
}

#[test]
fn zero_max_and_partial_governance_fields_remain_comparable() -> TestResult {
    let empty_names: &[&[u8]] = &[];
    for scalar in [0, u64::MAX] {
        let value = normalize_claim_due(
            ClaimDueInput {
                states: ClaimStatesInput::Many(&[]),
                partitions: ClaimPartitionsInput::Many(&[]),
                values: ClaimValuesInput::Names(&[]),
                lease_ms: Some(scalar),
                limit: Some(scalar),
                priority: Some(scalar),
                reclaim_ratio: Some(scalar),
                reclaim_expired: Some(false),
                payload: Some(false),
                payload_max_bytes: Some(scalar),
                ..ClaimDueInput::new(b"", b"")
            },
            ClaimCallerLimits::default(),
        )?;
        assert_eq!(
            (
                value.lease_ms(),
                value.limit(),
                value.priority(),
                value.reclaim_ratio()
            ),
            (scalar, scalar, Some(scalar), scalar)
        );
        assert!(!value.reclaim_expired());
        assert!(!value.payload_enabled());
        assert_eq!(value.payload_max_bytes(), Some(scalar));
        assert_eq!(value.workflow_type(), b"");
        assert_eq!(value.worker(), b"");
        assert_eq!(value.states(), (false, [].as_slice()));
        assert_eq!(value.partitions().1, empty_names);
        assert_eq!(value.values().1, empty_names);
    }
    for (scope, shard) in [
        (None, None),
        (Some(b"".as_slice()), None),
        (None, Some(0)),
        (Some(b"\0\xff".as_slice()), Some(u64::MAX)),
    ] {
        for now in [None, Some(0), Some(u64::MAX)] {
            for block in [None, Some(0), Some(u64::MAX)] {
                let value = normalize_claim_due(
                    ClaimDueInput {
                        governance_limit_scope: scope,
                        governance_shard_id: shard,
                        requested_now_ms: now,
                        block_ms: block,
                        ..ClaimDueInput::new(b"job", b"worker")
                    },
                    ClaimCallerLimits::default(),
                )?;
                assert_eq!(value.governance_limit_scope(), scope);
                assert_eq!(value.governance_shard_id(), shard);
                assert_eq!(value.requested_now_ms(), now);
                assert_eq!(value.block_ms(), block);
            }
        }
    }
    Ok(())
}
