//! Real committed-prefix publication, original responses and safe dependent ACK.

use super::{TestResult, f};
use wf_application::governance::{
    CreditCatalogPublicationLimits as Limits, CreditCatalogPublicationStats as Stats,
};
use wf_ports::state::decode_credit_catalog_member;

mod collection;
mod failures;
mod fixtures;
use fixtures::{PublicationCall as Call, PublicationFixture};

#[test]
fn catalog_publisher_selects_one_source_and_batches_real_membership_before_ack() -> TestResult {
    let mut fixture = PublicationFixture::separate()?;
    let empty = fixture.run(0, Limits::default())?;
    assert_eq!(
        empty,
        Stats {
            published: 0,
            errors: 0,
            read_batches: 0,
            write_batches: 0,
            sources_scanned: 1,
            next_source: 1
        }
    );
    assert_eq!(fixture.calls, vec![Call::Page]);
    assert_eq!(fixture.members()?.len(), 0);
    fixture.calls.clear();
    let result = fixture.run(1, Limits::default())?;
    assert_eq!(
        result,
        Stats {
            published: 3,
            errors: 0,
            read_batches: 2,
            write_batches: 1,
            sources_scanned: 1,
            next_source: 0
        }
    );
    assert_eq!(
        fixture.calls,
        vec![Call::Page, Call::Owners, Call::Membership, Call::Ack]
    );
    assert_eq!(fixture.pending(1)?.entries.len(), 0);
    let members = fixture.members()?;
    assert_eq!(members.len(), 3);
    for (member, scope) in members.iter().zip([b"a", b"b", b"c"]) {
        let decoded = decode_credit_catalog_member(member, 65535)?;
        assert_eq!(decoded.credit_scope, scope);
        assert_eq!(decoded.group, fixture.sources[1].group().as_bytes());
        assert_eq!(decoded.recovery, fixture.sources[1].recovery().as_bytes());
    }
    fixture.verify_all()?;
    fixture.cleanup()?;
    Ok(())
}

#[test]
fn catalog_publisher_same_group_combined_profile_discards_missing_owner_and_survives_snapshot()
-> TestResult {
    let mut fixture = PublicationFixture::combined()?;
    fixture.remove_owner(0, b"b")?;
    let result = fixture.run(0, Limits::default())?;
    assert_eq!(
        (
            result.published,
            result.errors,
            result.read_batches,
            result.write_batches
        ),
        (3, 0, 2, 1)
    );
    assert_eq!(fixture.pending(0)?.entries.len(), 0);
    let members = fixture.members()?;
    assert_eq!(members.len(), 2);
    let names = members
        .iter()
        .map(|bytes| decode_credit_catalog_member(bytes, 65535).map(|m| m.credit_scope))
        .collect::<Result<Vec<_>, _>>()?;
    assert_eq!(names, vec![b"a".as_slice(), b"c"]);
    fixture.snapshot_roundtrip()?;
    fixture.verify_all()?;
    fixture.cleanup()?;
    Ok(())
}

#[test]
fn catalog_publisher_missing_only_prefix_acks_progress_without_a_membership_write() -> TestResult {
    let mut fixture = PublicationFixture::combined()?;
    for name in [b"a", b"b", b"c"] {
        fixture.remove_owner(0, name)?;
    }
    let result = fixture.run(0, Limits::default())?;
    assert_eq!(
        (
            result.published,
            result.errors,
            result.read_batches,
            result.write_batches
        ),
        (3, 0, 2, 0)
    );
    assert_eq!(fixture.calls, vec![Call::Page, Call::Owners, Call::Ack]);
    assert_eq!(fixture.members()?.len(), 0);
    assert_eq!(fixture.pending(0)?.entries.len(), 0);
    fixture.verify_all()?;
    fixture.cleanup()?;
    Ok(())
}
