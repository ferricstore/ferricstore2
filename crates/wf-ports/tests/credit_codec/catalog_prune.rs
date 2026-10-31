//! Independent complete guarded-prune and member-revision vectors.
use super::TestResult;
use sha2::{Digest, Sha256};
use wf_domain::{
    governance::catalog::{CatalogPruneReply as P, CatalogPruneRequest as C},
    workflow::{
        admission::AdmissionLimits,
        control::{EntryIdentity, PreparedDigest},
        request::{ClientIntent, RequestScope},
    },
};
use wf_ports::{
    log::{
        CodecError as E, CodecLimits, catalog_prune_recovery_header, catalog_prune_request_intent,
        catalog_prune_retained_summary, encode_catalog_prune_reply, encode_catalog_prune_request,
        prepare_catalog_prune_request,
    },
    state::{encode_catalog_member_revision, inspect_catalog_member_revision},
};
use wf_types::{
    identity::{GroupId, RecoveryId, RequestId, TenantId},
    version::{AppliedPosition, CommittedTime, HlcTimestamp, LogIdentity},
};
fn revision() -> EntryIdentity {
    EntryIdentity::new(
        LogIdentity::new(1, 7, AppliedPosition::new(9)),
        PreparedDigest::new([0xa7; 32]),
    )
}
fn row() -> Vec<u8> {
    let mut bytes = b"FWGM\0\x02".to_vec();
    for n in [1_u64, 7, 9] {
        bytes.extend_from_slice(&n.to_be_bytes());
    }
    bytes.extend_from_slice(&[0xa7; 32]);
    bytes
}
fn scope() -> TestResult<RequestScope> {
    Ok(RequestScope::new(
        TenantId::new(b"t".to_vec())?,
        GroupId::new(b"g".to_vec())?,
        RecoveryId::new(b"r".to_vec())?,
    ))
}
fn vector() -> Vec<u8> {
    let mut body = vec![0, 0, 0, 1, b'c', 0, 0, 0, 3, b'a', 0, 255];
    body.extend_from_slice(&row());
    let mut hash = Sha256::new();
    hash.update(b"wf-governance-catalog-prune-intent-v1\0");
    hash.update(&body);
    let mut bytes = b"FWCP\0\x01\x0e".to_vec();
    for field in *b"tgri" {
        bytes.extend_from_slice(&[0, 0, 0, 1, field]);
    }
    bytes.extend_from_slice(&16_u16.to_be_bytes());
    bytes.extend_from_slice(&hash.finalize());
    bytes.extend_from_slice(&1000_u64.to_be_bytes());
    bytes.extend_from_slice(&0_u16.to_be_bytes());
    bytes.extend_from_slice(&1000_u64.to_be_bytes());
    bytes.extend_from_slice(&body);
    bytes
}
#[test]
fn catalog_prune_domain_preserves_original_binary_member_and_complete_identity() -> TestResult {
    let command = C {
        catalog: b"c",
        member: b"a\0\xff",
        expected: revision(),
    };
    command.validate()?;
    assert_eq!(command.member, b"a\0\xff");
    assert_eq!(command.expected, revision());
    assert!(
        C {
            member: b"",
            ..command
        }
        .validate()
        .is_err()
    );
    let over = vec![0; 65536];
    assert!(
        C {
            catalog: &over,
            ..command
        }
        .validate()
        .is_err()
    );
    Ok(())
}
#[test]
fn catalog_member_revision_matches_fixed_independent_bytes_and_complete_eof() -> TestResult {
    assert_eq!(encode_catalog_member_revision(revision()).as_slice(), row());
    assert_eq!(inspect_catalog_member_revision(&row())?, revision());
    for end in 0..62 {
        assert!(inspect_catalog_member_revision(&row()[..end]).is_err());
    }
    let mut trailing = row();
    trailing.push(0);
    assert!(inspect_catalog_member_revision(&trailing).is_err());
    let mut old = row();
    old[5] = 1;
    assert!(inspect_catalog_member_revision(&old).is_err());
    Ok(())
}
#[test]
fn catalog_prune_codec_matches_independent_bytes_and_borrows_original_member() -> TestResult {
    let bytes = vector();
    let limits = CodecLimits::default();
    let prepared = prepare_catalog_prune_request(&bytes, limits, 65536)?;
    let command = C {
        catalog: b"c",
        member: b"a\0\xff",
        expected: revision(),
    };
    assert_eq!(prepared.command(), command);
    assert_eq!(
        prepared.command().member.as_ptr(),
        bytes[bytes.len() - 65..].as_ptr()
    );
    let stamp = CommittedTime::new(HlcTimestamp::new(1000, 0)?, 1000)?;
    let encoded = encode_catalog_prune_request(
        &scope()?,
        &RequestId::new(b"i".to_vec())?,
        stamp,
        command,
        limits,
        65536,
    )?;
    assert_eq!(encoded.as_bytes(), bytes);
    assert_eq!(
        catalog_prune_recovery_header(&bytes, limits)?,
        (16, [b"t".as_slice(), b"g", b"r"], stamp)
    );
    assert_eq!(catalog_prune_request_intent(command)?.version(), 16);
    for end in 0..bytes.len() {
        assert!(prepare_catalog_prune_request(&bytes[..end], limits, 65536).is_err());
    }
    let owned = prepared.owned_bytes();
    assert!(prepare_catalog_prune_request(&bytes, limits, owned).is_ok());
    assert_eq!(
        prepare_catalog_prune_request(&bytes, limits, owned - 1).err(),
        Some(E::OwnedLimit)
    );
    let mut bad = bytes.clone();
    bad.push(0);
    assert!(prepare_catalog_prune_request(&bad, limits, 65536).is_err());
    Ok(())
}
#[test]
fn catalog_prune_reply_distinguishes_removed_missing_changed_and_refuses_other_profiles()
-> TestResult {
    for (tag, reply) in [(0, P::Removed), (1, P::Missing), (2, P::Changed)] {
        let mut expected = b"FWXR\0\x01".to_vec();
        expected.push(tag);
        assert_eq!(encode_catalog_prune_reply(reply).as_slice(), expected);
        assert_eq!(
            catalog_prune_retained_summary(
                ClientIntent::governance_catalog_prune([1; 32]),
                14,
                &expected,
                AdmissionLimits::default()
            )?,
            reply
        );
        for end in 0..expected.len() {
            assert!(
                catalog_prune_retained_summary(
                    ClientIntent::governance_catalog_prune([1; 32]),
                    14,
                    &expected[..end],
                    AdmissionLimits::default()
                )
                .is_err()
            );
        }
        assert!(
            catalog_prune_retained_summary(
                ClientIntent::governance_catalog_prune([1; 32]),
                13,
                &expected,
                AdmissionLimits::default()
            )
            .is_err()
        );
    }
    Ok(())
}
