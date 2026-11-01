use super::{TestResult, fixtures::*};
use wf_domain::workflow::request::RequestScope;
use wf_storage::state::value::{encode_record_capture, scan_record_capture};
use wf_types::identity::{GroupId, PartitionId, RecordId, RecoveryId, TenantId};

#[test]
fn independently_packed_complete_record_capture_matches_encoder() -> TestResult {
    let f = Fixture::new()?;
    let actual = encode_record_capture(f.input(), limits())?;
    assert_eq!(actual, independent_wire()?);
    Ok(())
}
#[test]
fn exact_point_birth_and_version_borrow_from_one_donor() -> TestResult {
    let f = Fixture::new()?;
    let bytes = independent_wire()?;
    let scanned = scan_record_capture(&bytes, f.point(), limits())?;
    assert_eq!(scanned.core().version(), 1);
    assert_eq!(scanned.incarnation(), u128::MAX - 7);
    assert_eq!(scanned.metadata_bytes(), f.metadata.as_stored_bytes());
    assert_eq!(scanned.core_bytes().as_ptr(), bytes[26..].as_ptr());
    let start = scanned.metadata_bytes().as_ptr() as usize;
    assert!(start >= bytes.as_ptr() as usize);
    assert!(start + scanned.metadata_bytes().len() <= bytes.as_ptr() as usize + bytes.len());
    Ok(())
}
#[test]
fn debug_does_not_disclose_record_or_original_fields() -> TestResult {
    let f = Fixture::new()?;
    let bytes = independent_wire()?;
    let scanned = scan_record_capture(&bytes, f.point(), limits())?;
    assert_eq!(format!("{scanned:?}"), "ScannedRecordCapture(<redacted>)");
    Ok(())
}

#[test]
fn every_identity_component_and_exact_birth_version_must_match() -> TestResult {
    let f = Fixture::new()?;
    let bytes = independent_wire()?;
    let bad_scopes = [
        RequestScope::new(
            TenantId::new(b"other".to_vec())?,
            f.scope.group().clone(),
            f.scope.recovery().clone(),
        ),
        RequestScope::new(
            f.scope.tenant().clone(),
            GroupId::new(b"other".to_vec())?,
            f.scope.recovery().clone(),
        ),
        RequestScope::new(
            f.scope.tenant().clone(),
            f.scope.group().clone(),
            RecoveryId::new(b"other".to_vec())?,
        ),
    ];
    for scope in &bad_scopes {
        let mut point = f.point();
        point.scope = scope;
        assert!(scan_record_capture(&bytes, point, limits()).is_err());
    }
    let partition = PartitionId::new(b"other".to_vec())?;
    let mut point = f.point();
    point.partition = &partition;
    assert!(scan_record_capture(&bytes, point, limits()).is_err());
    let id = RecordId::new(b"other".to_vec())?;
    let mut point = f.point();
    point.id = &id;
    assert!(scan_record_capture(&bytes, point, limits()).is_err());
    let mut point = f.point();
    point.incarnation -= 1;
    assert!(scan_record_capture(&bytes, point, limits()).is_err());
    for version in [0, 2, u64::MAX] {
        let mut point = f.point();
        point.version = version;
        assert!(scan_record_capture(&bytes, point, limits()).is_err());
    }
    Ok(())
}

#[test]
fn every_truncation_trailing_bytes_and_changed_byte_refuse() -> TestResult {
    let f = Fixture::new()?;
    let original = independent_wire()?;
    for end in 0..original.len() {
        assert!(
            scan_record_capture(&original[..end], f.point(), limits()).is_err(),
            "truncation {end}"
        );
    }
    for at in 0..original.len() {
        let mut bytes = original.clone();
        bytes[at] ^= 1;
        assert!(
            scan_record_capture(&bytes, f.point(), limits()).is_err(),
            "changed byte {at}"
        );
    }
    let mut bytes = original;
    bytes.push(0);
    assert!(scan_record_capture(&bytes, f.point(), limits()).is_err());
    Ok(())
}

#[test]
fn resealed_malformed_core_lengths_metadata_and_version_refuse() -> TestResult {
    let f = Fixture::new()?;
    let original = independent_wire()?;
    let mut changes = Vec::new();
    let mut bytes = original.clone();
    bytes[5] = 2;
    changes.push(bytes);
    let mut bytes = original.clone();
    bytes[22..26].copy_from_slice(&u32::MAX.to_be_bytes());
    changes.push(bytes);
    let mut bytes = original.clone();
    bytes[31] = 1;
    changes.push(bytes); // legacy FWSR1
    let mut bytes = original.clone();
    bytes[60] = 3;
    changes.push(bytes); // invalid core priority
    let metadata_at = 26 + 147 + 4;
    let mut bytes = original.clone();
    bytes[metadata_at] = b'X';
    changes.push(bytes);
    let mut bytes = original.clone();
    bytes[metadata_at + 4] = 1;
    changes.push(bytes);
    let mut bytes = original.clone();
    bytes[metadata_at - 4..metadata_at].copy_from_slice(&0_u32.to_be_bytes());
    changes.push(bytes);
    for mut bytes in changes {
        bytes.truncate(bytes.len() - 32);
        seal(&mut bytes);
        assert!(scan_record_capture(&bytes, f.point(), limits()).is_err());
    }
    Ok(())
}

#[test]
fn birth_sequence_is_checked_even_when_expected_incarnation_and_checksum_agree() -> TestResult {
    let f = Fixture::new()?;
    let mut input = f.input();
    input.incarnation -= 1;
    assert!(encode_record_capture(input, limits()).is_err());
    let mut bytes = independent_wire()?;
    bytes[6..22].copy_from_slice(&input.incarnation.to_be_bytes());
    bytes.truncate(bytes.len() - 32);
    seal(&mut bytes);
    let mut point = f.point();
    point.incarnation = input.incarnation;
    assert!(scan_record_capture(&bytes, point, limits()).is_err());
    Ok(())
}

#[test]
fn borrowed_metadata_requires_its_own_explicit_typed_decode() -> TestResult {
    use wf_domain::metadata::record::{
        OriginalRecordMetadataField as F, decode_original_record_metadata,
    };
    use wf_domain::value::{OriginalValueContext, OriginalValueLimits, OriginalValueNode};
    let f = Fixture::new()?;
    let bytes = independent_wire()?;
    let scanned = scan_record_capture(&bytes, f.point(), limits())?;
    let decoded = decode_original_record_metadata(
        scanned.metadata_bytes().to_vec(),
        &OriginalValueContext {
            safe_atoms: None,
            active_exports: None,
            local_node: None,
            local_functions: None,
        },
        OriginalValueLimits {
            max_encoded_bytes: 1024,
            max_nodes: 32,
            max_edges: 32,
            max_depth: 16,
            max_work_units: 1_048_576,
        },
    )?;
    assert!(matches!(
        decoded.field(F::Attributes),
        Some(OriginalValueNode::Integer(7))
    ));
    assert!(decoded.field(F::Partition).is_none());
    Ok(())
}

#[test]
fn later_core_version_keeps_original_birth_across_changed_ordering() -> TestResult {
    use wf_domain::workflow::{CommittedCommand, Operation, Transition, reduce};
    use wf_types::version::{Fence, TimestampMillis};
    let f = Fixture::new()?;
    let plan = reduce(
        Some(&f.record),
        &CommittedCommand {
            id: f.record.id().clone(),
            now: TimestampMillis::new(8),
            operation: Operation::Transition(Transition {
                from: b"q\0\xff".to_vec(),
                to: b"next".to_vec(),
                fence: Fence::new(0),
                lease_token: None,
                run_at: None,
                payload: None,
            }),
        },
    )?;
    let record = plan.into_parts().1.ok_or("missing transition")?.record;
    let mut input = f.input();
    input.record = &record;
    input.ordering.state_enter_seq += 1;
    let bytes = encode_record_capture(input, limits())?;
    let mut point = f.point();
    point.version = record.version();
    let scanned = scan_record_capture(&bytes, point, limits())?;
    assert_eq!(scanned.core().version(), 2);
    assert_eq!(scanned.incarnation(), f.ordering.state_enter_seq);
    assert_eq!(scanned.metadata_bytes(), f.metadata.as_stored_bytes());
    assert!(scan_record_capture(&bytes, f.point(), limits()).is_err());
    point.incarnation = input.ordering.state_enter_seq;
    assert!(scan_record_capture(&bytes, point, limits()).is_err());
    Ok(())
}

#[test]
fn legacy_reader_cannot_silently_accept_new_capture_as_a_core_record() -> TestResult {
    use wf_storage::state::value::{decode_record, scan_record_snapshot};
    let f = Fixture::new()?;
    let bytes = encode_record_capture(f.input(), limits())?;
    assert!(
        decode_record(
            &bytes,
            f.scope.group(),
            f.scope.recovery(),
            limits().admission,
            limits().maximum_encoded_bytes
        )
        .is_err()
    );
    assert!(
        scan_record_snapshot(
            &bytes,
            &f.scope,
            &f.partition,
            f.record.id(),
            limits().admission,
            limits().maximum_encoded_bytes
        )
        .is_err()
    );
    Ok(())
}

#[test]
fn checksum_and_stored_discriminator_do_not_certify_metadata_schema() -> TestResult {
    use wf_domain::metadata::record::{
        OriginalRecordMetadataError as E, decode_original_record_metadata,
    };
    use wf_domain::value::{OriginalValueContext, OriginalValueLimits};
    let f = Fixture::new()?;
    let mut bytes = independent_wire()?;
    let metadata_at = 26 + 147 + 4;
    bytes[metadata_at + 32] = 2; // Unsupported logical capture schema version.
    bytes.truncate(bytes.len() - 32);
    seal(&mut bytes);
    let scanned = scan_record_capture(&bytes, f.point(), limits())?;
    let decoded = decode_original_record_metadata(
        scanned.metadata_bytes().to_vec(),
        &OriginalValueContext {
            safe_atoms: None,
            active_exports: None,
            local_node: None,
            local_functions: None,
        },
        OriginalValueLimits {
            max_encoded_bytes: 1024,
            max_nodes: 32,
            max_edges: 32,
            max_depth: 16,
            max_work_units: 1_048_576,
        },
    );
    assert!(matches!(decoded, Err(E::UnsupportedVersion)));
    Ok(())
}
