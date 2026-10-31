//! Actual owner loading and retained repair, without a simulated commit flag.
use super::fixtures::PublicationFault as Fault;
use super::*;
use wf_application::governance::{CreditCatalogRepairError, CreditCatalogRepairStatus as Status};

#[test]
fn catalog_collection_loads_actual_owners_skips_missing_members_and_changes_no_authority()
-> TestResult {
    let mut fixture = PublicationFixture::combined()?;
    assert_eq!(fixture.run(0, Limits::default())?.published, 3);
    fixture.remove_owner(0, b"b")?;
    let before = fixture.cohort(0)?;
    let result = fixture.collect_owners(2)?;
    assert_eq!(
        result
            .records()
            .map(|owner| owner.view().scope.as_slice())
            .collect::<Vec<_>>(),
        vec![b"a".as_slice(), b"c"]
    );
    drop(result);
    assert_eq!(fixture.cohort(0)?, before);
    fixture.verify_all()?;
    fixture.cleanup()?;
    Ok(())
}

#[test]
fn catalog_collection_routes_only_configured_source_owners_and_exact_keys_bypass_page_failures()
-> TestResult {
    let mut fixture = PublicationFixture::separate()?;
    assert_eq!(fixture.run(1, Limits::default())?.published, 3);
    let result = fixture.collect_owners(2)?;
    assert_eq!(
        result
            .records()
            .map(|owner| owner.view().scope.as_slice())
            .collect::<Vec<_>>(),
        vec![b"a".as_slice(), b"b"]
    );
    drop(result);
    fixture.fault = Fault::UnavailablePage;
    assert!(fixture.collect_owners(3).is_err());
    let before = fixture.cohort(1)?;
    let result = fixture.collect_exact_owners(1, &[b"c", b"ghost", b"a"], 3)?;
    assert_eq!(
        result
            .into_records()
            .map(|owner| owner.into_data().scope)
            .collect::<Vec<_>>(),
        vec![b"a".to_vec(), b"c".to_vec()]
    );
    assert_eq!(fixture.cohort(1)?, before);
    fixture.verify_all()?;
    fixture.cleanup()?;
    Ok(())
}

#[test]
fn catalog_repair_confirms_actual_existing_owner_membership_without_ghosts_or_publication_ack()
-> TestResult {
    let mut fixture = PublicationFixture::combined()?;
    assert_eq!(
        fixture.repair(0, b"a", Limits::default())?,
        Status::Registered
    );
    assert_eq!(fixture.members()?.len(), 1);
    assert_eq!(fixture.calls, vec![Call::Membership]);
    assert_eq!(fixture.repair(0, b"a", Limits::default())?, Status::Present);
    assert_eq!(
        fixture.repair(0, b"ghost", Limits::default())?,
        Status::Missing
    );
    assert_eq!(fixture.calls, vec![Call::Membership]);
    assert_eq!(fixture.pending(0)?.entries.len(), 3);
    fixture.snapshot_roundtrip()?;
    fixture.verify_all()?;
    fixture.cleanup()?;
    Ok(())
}

#[test]
fn catalog_repair_refuses_unknown_or_foreign_replies_and_retries_actual_membership() -> TestResult {
    for fault in [
        Fault::RefuseMembership,
        Fault::LoseMembershipReply,
        Fault::InvalidMembershipReply,
    ] {
        let mut fixture = PublicationFixture::combined()?;
        fixture.fault = fault;
        assert_eq!(
            fixture.repair(0, b"a", Limits::default()),
            Err(CreditCatalogRepairError::Unconfirmed)
        );
        assert_eq!(fixture.calls, vec![Call::Membership]);
        assert_eq!(fixture.pending(0)?.entries.len(), 3);
        let retry = fixture.repair(0, b"a", Limits::default())?;
        assert!(matches!(retry, Status::Registered | Status::Present));
        assert_eq!(fixture.members()?.len(), 1);
        assert_eq!(fixture.pending(0)?.entries.len(), 3);
        fixture.verify_all()?;
        fixture.cleanup()?;
    }
    Ok(())
}

#[test]
fn catalog_repair_workspace_refuses_before_any_submission_or_table_prefix() -> TestResult {
    let mut fixture = PublicationFixture::combined()?;
    let before = fixture.cohort(0)?;
    let limits = Limits {
        workspace_bytes: 1,
        ..Limits::default()
    };
    assert!(matches!(
        fixture.repair(0, b"a", limits),
        Err(CreditCatalogRepairError::Preflight(_))
    ));
    assert_eq!(fixture.calls.len(), 0);
    assert_eq!(fixture.cohort(0)?, before);
    fixture.cleanup()?;
    Ok(())
}
