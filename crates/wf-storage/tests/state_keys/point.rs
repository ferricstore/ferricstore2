use super::{TestResult, hex, scope, standard_scope};
use std::collections::BTreeSet;
use wf_storage::state::key::{encode_control_key, encode_outcome_key, encode_record_key};
use wf_storage::state::value::StateValueError;
use wf_types::identity::{GroupId, PartitionId, RecordId, RecoveryId, RequestId};

// Packed from FWPK/u16BE1/family and escaped components before any codec existed.
const CONTROL: &str = "4657504b0001006700007a0000";
const RECORD: &str = "4657504b0001016700007a0000740000700000690000";
const OUTCOME: &str = "4657504b0001026700007a0000740000710000";

#[test]
fn point_key_vectors_bind_global_and_tenant_identity() -> TestResult {
    let original = standard_scope()?;
    let partition = PartitionId::new(b"p".to_vec())?;
    let id = RecordId::new(b"i".to_vec())?;
    let request = RequestId::new(b"q".to_vec())?;
    assert_eq!(
        encode_control_key(original.group(), original.recovery(), 13)?,
        hex(CONTROL)?
    );
    assert_eq!(
        encode_record_key(&original, &partition, &id, 22)?,
        hex(RECORD)?
    );
    assert_eq!(encode_outcome_key(&original, &request, 19)?, hex(OUTCOME)?);
    let foreign_tenant = scope(b"g", b"z", b"other")?;
    assert_eq!(
        encode_control_key(foreign_tenant.group(), foreign_tenant.recovery(), 13)?,
        hex(CONTROL)?
    );
    assert_ne!(
        encode_record_key(&foreign_tenant, &partition, &id, 100)?,
        hex(RECORD)?
    );
    assert_ne!(
        encode_outcome_key(&foreign_tenant, &request, 100)?,
        hex(OUTCOME)?
    );
    for foreign in [scope(b"h", b"z", b"t")?, scope(b"g", b"y", b"t")?] {
        assert_ne!(
            encode_control_key(foreign.group(), foreign.recovery(), 100)?,
            hex(CONTROL)?
        );
        assert_ne!(
            encode_record_key(&foreign, &partition, &id, 100)?,
            hex(RECORD)?
        );
        assert_ne!(encode_outcome_key(&foreign, &request, 100)?, hex(OUTCOME)?);
    }
    assert_ne!(hex(CONTROL)?, hex(RECORD)?);
    assert_ne!(hex(RECORD)?, hex(OUTCOME)?);
    Ok(())
}

fn assert_point_caps() -> TestResult {
    let original = standard_scope()?;
    let partition = PartitionId::new(b"p".to_vec())?;
    let id = RecordId::new(b"i".to_vec())?;
    let request = RequestId::new(b"q".to_vec())?;
    assert_eq!(
        encode_control_key(original.group(), original.recovery(), 12),
        Err(StateValueError::Limit {
            bytes: 13,
            maximum: 12
        })
    );
    assert_eq!(
        encode_record_key(&original, &partition, &id, 21),
        Err(StateValueError::Limit {
            bytes: 22,
            maximum: 21
        })
    );
    assert_eq!(
        encode_outcome_key(&original, &request, 18),
        Err(StateValueError::Limit {
            bytes: 19,
            maximum: 18
        })
    );
    assert_eq!(
        encode_control_key(original.group(), original.recovery(), 0),
        Err(StateValueError::InvalidValue)
    );
    assert_eq!(
        encode_record_key(&original, &partition, &id, 0),
        Err(StateValueError::InvalidValue)
    );
    assert_eq!(
        encode_outcome_key(&original, &request, 0),
        Err(StateValueError::InvalidValue)
    );
    Ok(())
}

fn assert_maximum_nul_components() -> TestResult {
    let original = scope(&[0; 256], &[0; 256], &[0; 8_192])?;
    let partition = PartitionId::new(vec![0; 65_535])?;
    let id = RecordId::new(vec![0; 65_535])?;
    let request = RequestId::new(vec![0; 256])?;
    assert_eq!(
        encode_control_key(original.group(), original.recovery(), 1_035)?.len(),
        1_035
    );
    assert_eq!(
        encode_record_key(&original, &partition, &id, 279_565)?.len(),
        279_565
    );
    assert_eq!(
        encode_outcome_key(&original, &request, 17_935)?.len(),
        17_935
    );
    assert_eq!(
        encode_record_key(&original, &partition, &id, 65_535),
        Err(StateValueError::Limit {
            bytes: 279_565,
            maximum: 65_535
        })
    );
    Ok(())
}

#[test]
fn point_key_collisions_and_byte_limits_fail_closed() -> TestResult {
    assert_point_caps()?;
    assert_maximum_nul_components()?;
    let partition = PartitionId::new(b"p".to_vec())?;
    let id = RecordId::new(b"i".to_vec())?;
    let mut keys = BTreeSet::new();
    // The first two scopes have identical naïve concatenation, as do NUL splits.
    for namespace in [
        scope(b"a", b"bc", b"t")?,
        scope(b"ab", b"c", b"t")?,
        scope(b"g\0", b"z", b"t")?,
        scope(b"g", b"\0z", b"t")?,
        scope(b"g", b"z", b"t\xff")?,
    ] {
        assert!(keys.insert(encode_record_key(&namespace, &partition, &id, 100)?));
    }
    let original = standard_scope()?;
    for raw_partition in [b"p".as_slice(), b"p\0", b"p\xff"] {
        for raw_id in [b"a".as_slice(), b"a\0", b"a\0\xff", b"aa", b"\xff"] {
            let partition = PartitionId::new(raw_partition.to_vec())?;
            let id = RecordId::new(raw_id.to_vec())?;
            assert!(keys.insert(encode_record_key(&original, &partition, &id, 100)?));
        }
    }
    assert_eq!(keys.len(), 20);
    let group = GroupId::new(b"g".to_vec())?;
    let recovery = RecoveryId::new(b"z".to_vec())?;
    assert_eq!(encode_control_key(&group, &recovery, 13)?, hex(CONTROL)?);
    Ok(())
}
