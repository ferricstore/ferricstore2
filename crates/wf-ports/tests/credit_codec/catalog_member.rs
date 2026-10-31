use super::TestResult;
use wf_domain::workflow::request::RequestScope;
use wf_ports::state::{decode_credit_catalog_member, encode_credit_catalog_members};
use wf_types::identity::{GroupId, RecoveryId, TenantId};

fn source() -> TestResult<RequestScope> {
    Ok(RequestScope::new(
        TenantId::new(b"t".to_vec())?,
        GroupId::new(b"g".to_vec())?,
        RecoveryId::new(b"r".to_vec())?,
    ))
}

#[test]
fn credit_catalog_member_matches_independent_bytes_and_keeps_group_recovery_scope() -> TestResult {
    let encoded = encode_credit_catalog_members(&source()?, &[b"a\0b", b"z"], 65535, 65535)?;
    let members = encoded.members().collect::<Vec<_>>();
    assert_eq!(members[0], b"FWGL\0\x01\0\0\0\x03a\0b\0\x01g\0\x01r");
    assert_eq!(members[1], b"FWGL\0\x01\0\0\0\x01z\0\x01g\0\x01r");
    let decoded = decode_credit_catalog_member(members[0], 65535)?;
    assert_eq!(
        (decoded.group, decoded.recovery, decoded.credit_scope),
        (b"g".as_slice(), b"r".as_slice(), b"a\0b".as_slice())
    );
    assert!(std::ptr::eq(
        decoded.credit_scope.as_ptr(),
        members[0][10..].as_ptr()
    ));
    assert_eq!(
        members[1].as_ptr(),
        members[0].as_ptr().wrapping_add(members[0].len())
    );
    Ok(())
}

#[test]
fn credit_catalog_member_rejects_truncation_trailing_invalid_counts_and_exact_owned_boundary()
-> TestResult {
    let source = source()?;
    let encoded = encode_credit_catalog_members(&source, &[b"a"], 65535, 65535)?;
    let owned = encoded.owned_bytes();
    assert!(encode_credit_catalog_members(&source, &[b"a"], 65535, owned).is_ok());
    assert!(encode_credit_catalog_members(&source, &[b"a"], 65535, owned - 1).is_err());
    let bytes = encoded.members().next().ok_or("missing member")?;
    for cut in 0..bytes.len() {
        assert!(
            decode_credit_catalog_member(&bytes[..cut], 65535).is_err(),
            "cut{cut}"
        );
    }
    let mut extra = bytes.to_vec();
    extra.push(0);
    assert!(decode_credit_catalog_member(&extra, 65535).is_err());
    assert!(encode_credit_catalog_members(&source, &[b""], 65535, 65535).is_err());
    assert!(encode_credit_catalog_members(&source, &[b"a".as_slice(); 257], 65535, 65535).is_err());
    assert_eq!(
        encode_credit_catalog_members(&source, &[], 65535, 65535)?
            .members()
            .len(),
        0
    );
    Ok(())
}
