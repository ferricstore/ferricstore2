//! Necessary source postimage bounds; passing is not full metadata/key validation.
use crate::{claim::apply::ClaimQuery, workflow::WorkflowRecord};
use wf_types::{bounds::MAX_API_KEY_BYTES, version::TimestampMillis};

// QueryRowCodec's magic5, CRC4, two maximum10-byte varints and three u16 fields.
const MAX_BINARY: usize = 32 * 1024;
const MAX_METADATA: usize = MAX_BINARY - (5 + 4 + 2 * 10 + 3 * 2) - 104;
// Limits.max_run_id_bytes uses the largest partitioned state-key overhead.
const MAX_ID: usize = MAX_API_KEY_BYTES - (5 + 43 + 4);
// Metadata header11, builtin bitmap2, internal flags1, projection2, attributes1,
// state-meta1. Public projection always adds a partition primitive (minimum1).
const FIXED_METADATA: usize = 11 + 2 + 1 + 2 + 1 + 1 + 1;

pub(super) fn known_rejection(
    record: &WorkflowRecord,
    query: ClaimQuery<'_>,
    now: TimestampMillis,
) -> bool {
    if record.id().as_bytes().len() > MAX_ID || i64::try_from(record.attempts()).is_err() {
        return true;
    }
    let run_state = if record.state() == b"running" {
        record.run_state().unwrap_or(b"queued")
    } else {
        record.state()
    };
    minimum_metadata(
        [record.workflow_type(), run_state, query.worker],
        now,
        query.lease_ms,
    )
    .is_none_or(|bytes| bytes > MAX_METADATA)
}

fn minimum_metadata(binaries: [&[u8]; 3], now: TimestampMillis, lease_ms: u64) -> Option<usize> {
    let deadline = now.checked_add(lease_ms)?;
    let mut bytes = FIXED_METADATA.checked_add(binary_size(b"running")?)?;
    for value in binaries {
        bytes = bytes.checked_add(binary_size(value)?)?;
    }
    bytes = bytes.checked_add(integer_size(now.get())?)?;
    bytes.checked_add(integer_size(deadline.get())?.checked_mul(2)?)
}

fn binary_size(value: &[u8]) -> Option<usize> {
    if value.len() > MAX_BINARY {
        return None;
    }
    let size = varint_size(u64::try_from(value.len()).ok()?);
    value.len().checked_add(size)?.checked_add(1)
}

fn integer_size(value: u64) -> Option<usize> {
    // Nonnegative signed64 source primitives use zigzag followed by u64 varint.
    let zigzag = value.checked_mul(2)?;
    Some(1 + varint_size(zigzag))
}

fn varint_size(mut value: u64) -> usize {
    let mut bytes = 1;
    while value >= 128 {
        value >>= 7;
        bytes += 1;
    }
    bytes
}

#[cfg(test)]
mod tests {
    use super::{binary_size, integer_size, minimum_metadata, varint_size};
    use wf_types::version::TimestampMillis;

    #[test]
    fn unsigned_varint_boundaries_preserve_zero_and_full_u64() {
        for (value, bytes) in [
            (0, 1),
            (127, 1),
            (128, 2),
            (16383, 2),
            (16384, 3),
            (u64::MAX, 10),
        ] {
            assert_eq!(varint_size(value), bytes);
        }
    }

    #[test]
    fn positive_integer_sizes_respect_zigzag_and_signed_ceiling() {
        assert_eq!(integer_size(0), Some(2));
        assert_eq!(integer_size(63), Some(2));
        assert_eq!(integer_size(64), Some(3));
        assert_eq!(integer_size(u64::MAX / 2), Some(11));
        assert_eq!(integer_size(u64::MAX / 2 + 1), None);
    }

    #[test]
    fn primitive_binary_ceiling_includes_tag_and_length_bytes() {
        assert_eq!(binary_size(&[]), Some(2));
        assert_eq!(binary_size(&[0; 127]), Some(129));
        assert_eq!(binary_size(&[0; 128]), Some(131));
        assert_eq!(binary_size(&vec![0; 32768]), Some(32772));
        assert_eq!(binary_size(&vec![0; 32769]), None);
    }

    #[test]
    fn aggregate_lower_bound_has_an_inclusive_ceiling_without_certifying_a_full_row() {
        let now = TimestampMillis::new(3000);
        assert_eq!(
            minimum_metadata([b"r", b"q", &vec![b'w'; 32582]], now, 100),
            Some(32629)
        );
        assert_eq!(
            minimum_metadata([b"r", b"q", &vec![b'w'; 32583]], now, 100),
            Some(32630)
        );
        assert_eq!(
            minimum_metadata([b"r", &vec![b's'; 32000], &vec![b'w'; 1024]], now, 100),
            Some(33071)
        );
        assert_eq!(
            minimum_metadata([b"r", b"q", &vec![b'w'; 1024]], now, 100),
            Some(1070)
        );
        assert_eq!(
            minimum_metadata([b"r", b"q", b"w"], TimestampMillis::new(u64::MAX), 1),
            None
        );
    }

    #[test]
    fn run_id_ceiling_is_independent_of_the_current_native_partition()
    -> Result<(), Box<dyn std::error::Error>> {
        use crate::{
            claim::{
                PartitionFilter,
                apply::{ClaimQuery, ClaimStates},
            },
            history::EventKind,
            workflow::{
                admission::AdmissionLimits,
                restoration::{WorkflowRecordData, restore_record},
            },
        };
        use wf_types::{identity::RecordId, version::Fence};
        let now = TimestampMillis::new(3000);
        let query = ClaimQuery {
            workflow_type: b"r",
            partitions: PartitionFilter::Any,
            states: ClaimStates::Any,
            exclude_states: &[],
            priority: None,
            worker: b"w",
            lease_ms: 100,
            limit: 1,
        };
        for (length, rejected) in [(65483, false), (65484, true)] {
            let record = restore_record(
                WorkflowRecordData {
                    id: RecordId::new(vec![b'i'; length])?,
                    workflow_type: b"r".to_vec(),
                    state: b"q".to_vec(),
                    version: 1,
                    fence: Fence::new(0),
                    attempts: 0,
                    created_at: now,
                    updated_at: now,
                    next_run_at: Some(now),
                    run_state: None,
                    lease: None,
                    payload: None,
                    result: None,
                    error: None,
                    latest_kind: EventKind::Created,
                    latest_version: 1,
                    latest_fence: Fence::new(0),
                },
                AdmissionLimits::default(),
            )?;
            assert_eq!(super::known_rejection(&record, query, now), rejected);
        }
        Ok(())
    }
}
