//! Executed stored counterparts for original 300-member catalog scenarios.
use super::*;
use wf_application::governance::GovernanceCatalogCollectionError;
use wf_domain::{
    governance::catalog::CatalogMembershipAction as Action, workflow::request::RetainedStatus,
};
use wf_ports::state::StateTransactionError;

#[test]
fn source_catalog_collection_keeps_global_top_results_across_real_committed_pages() -> TestResult {
    let fixture = PublicationFixture::combined()?;
    let prefix = b"catalog-page-test-rust";
    let members = (0..300)
        .map(|index| format!("catalog-page-test-rust:{index:03}").into_bytes())
        .collect::<Vec<_>>();
    let keys = members.iter().map(Vec::as_slice).collect::<Vec<_>>();
    let registration = fixture.change_catalog_members(b"budget", Action::Register, &keys)?;
    assert_eq!(registration, vec![RetainedStatus::Accepted; 2]);
    assert_eq!(
        fixture.change_catalog_members(b"budget", Action::Register, &[b"00-noise", b"zz-noise"])?,
        vec![RetainedStatus::Accepted]
    );
    let before = fixture.cohort(0)?;
    let result = fixture.collect_raw_catalog(b"budget", prefix, 5, |_| Ok(()))?;
    assert_eq!(result.len(), 5);
    let expected = members.iter().rev().take(5).cloned().collect::<Vec<_>>();
    assert_eq!(result.into_records().collect::<Vec<_>>(), expected);
    assert_eq!(fixture.cohort(0)?, before);
    fixture.snapshot_roundtrip()?;
    let restored = fixture.collect_raw_catalog(b"budget", prefix, 5, |_| Ok(()))?;
    assert_eq!(restored.into_records().collect::<Vec<_>>(), expected);
    fixture.verify_all()?;
    fixture.cleanup()?;
    Ok(())
}

#[test]
fn source_catalog_paging_anchors_on_the_real_last_member_during_committed_insertion() -> TestResult
{
    let fixture = PublicationFixture::combined()?;
    let prefix = b"catalog-anchor-test-rust";
    let inserted = b"catalog-anchor-test-rust:000-before";
    let members = (1..=300)
        .map(|index| format!("catalog-anchor-test-rust:{index:03}").into_bytes())
        .collect::<Vec<_>>();
    let keys = members.iter().map(Vec::as_slice).collect::<Vec<_>>();
    assert_eq!(
        fixture.change_catalog_members(b"approval", Action::Register, &keys)?,
        vec![RetainedStatus::Accepted; 2]
    );
    let mut did_insert = false;
    let seen = fixture.collect_raw_catalog(b"approval", prefix, 300, |_| {
        if !did_insert {
            let reply = fixture
                .change_catalog_members(b"approval", Action::Register, &[inserted])
                .map_err(|_| StateTransactionError::Unavailable)?;
            assert_eq!(reply, vec![RetainedStatus::Accepted]);
            did_insert = true;
        }
        Ok(())
    })?;
    assert_eq!(seen.len(), 300);
    assert_eq!(seen.into_records().rev().collect::<Vec<_>>(), members);
    assert!(did_insert);
    fixture.verify_all()?;
    fixture.cleanup()?;
    Ok(())
}

#[test]
fn catalog_collection_refuses_a_real_deleted_cursor_without_returning_a_partial_result()
-> TestResult {
    let fixture = PublicationFixture::combined()?;
    let members = (0..300)
        .map(|index| format!("catalog-delete-test:{index:03}").into_bytes())
        .collect::<Vec<_>>();
    let keys = members.iter().map(Vec::as_slice).collect::<Vec<_>>();
    assert_eq!(
        fixture.change_catalog_members(b"budget", Action::Register, &keys)?,
        vec![RetainedStatus::Accepted; 2]
    );
    let mut deleted = false;
    let result = fixture.collect_raw_catalog(b"budget", b"catalog-delete-test", 5, |_| {
        if !deleted {
            let reply = fixture
                .change_catalog_members(b"budget", Action::Unregister, &[&members[255]])
                .map_err(|_| StateTransactionError::Unavailable)?;
            assert_eq!(reply, vec![RetainedStatus::Accepted]);
            deleted = true;
        }
        Ok(())
    });
    assert!(matches!(
        result,
        Err(GovernanceCatalogCollectionError::Source(
            StateTransactionError::CatalogChanged
        ))
    ));
    assert!(deleted);
    fixture.verify_all()?;
    fixture.cleanup()?;
    Ok(())
}
