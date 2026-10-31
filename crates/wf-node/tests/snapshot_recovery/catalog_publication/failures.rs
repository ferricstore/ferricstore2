use super::fixtures::PublicationFault as Fault;

#[test]
fn catalog_publisher_creation_after_missing_observation_keeps_a_fresh_publication_intent()
-> TestResult {
    let mut fixture = PublicationFixture::combined()?;
    fixture.remove_owner(0, b"c")?;
    fixture.fault = Fault::CreateTailOwnerAfterRead;
    let result = fixture.run(0, Limits::default())?;
    assert_eq!(
        (result.published, result.errors),
        (3, 0),
        "{:?}",
        fixture.last_error
    );
    assert_eq!(fixture.members()?.len(), 2);
    let pending = fixture.pending(0)?;
    assert_eq!(
        (
            pending.meta.head(),
            pending.meta.tail(),
            pending.entries.len()
        ),
        (4, 4, 1)
    );
    assert_eq!(pending.entries[0].credit_scope, b"c");
    assert_eq!(pending.entries[0].sequence, 4);
    let retry = fixture.run(0, Limits::default())?;
    assert_eq!((retry.published, retry.errors), (1, 0));
    assert_eq!(fixture.members()?.len(), 3);
    assert_eq!(fixture.pending(0)?.entries.len(), 0);
    fixture.verify_all()?;
    fixture.cleanup()?;
    Ok(())
}
use super::*;

#[test]
fn catalog_publisher_lost_membership_reply_never_acks_and_real_retry_is_idempotent() -> TestResult {
    let mut fixture = PublicationFixture::combined()?;
    let before = fixture.pending(0)?;
    fixture.fault = Fault::LoseMembershipReply;
    let result = fixture.run(0, Limits::default())?;
    assert_eq!(
        (result.published, result.errors, result.write_batches),
        (0, 1, 1)
    );
    assert_eq!(fixture.pending(0)?, before);
    assert_eq!(fixture.members()?.len(), 3);
    assert!(!fixture.calls.contains(&Call::Ack));
    fixture.calls.clear();
    let retry = fixture.run(0, Limits::default())?;
    assert_eq!((retry.published, retry.errors), (3, 0));
    assert_eq!(fixture.members()?.len(), 3);
    assert_eq!(fixture.pending(0)?.entries.len(), 0);
    fixture.verify_all()?;
    fixture.cleanup()?;
    Ok(())
}

#[test]
fn catalog_publisher_ack_loss_and_refusals_preserve_committed_members_and_pending_progress()
-> TestResult {
    for fault in [
        Fault::RefuseMembership,
        Fault::RefuseAck,
        Fault::LoseAckReply,
        Fault::InvalidMembershipReply,
    ] {
        let mut fixture = PublicationFixture::combined()?;
        fixture.fault = fault;
        let result = fixture.run(0, Limits::default())?;
        assert_eq!(
            (result.published, result.errors),
            (0, 1),
            "{fault:?}: {:?}",
            fixture.last_error
        );
        assert_eq!(
            fixture.members()?.len(),
            if fault == Fault::RefuseMembership {
                0
            } else {
                3
            }
        );
        assert_eq!(
            fixture.pending(0)?.entries.len(),
            if fault == Fault::LoseAckReply { 0 } else { 3 }
        );
        if matches!(
            fault,
            Fault::RefuseMembership | Fault::InvalidMembershipReply
        ) {
            assert!(!fixture.calls.contains(&Call::Ack));
        }
        let retry = fixture.run(0, Limits::default())?;
        assert_eq!(
            retry.published,
            if fault == Fault::LoseAckReply { 0 } else { 3 }
        );
        assert_eq!(retry.errors, 0);
        assert_eq!(fixture.members()?.len(), 3);
        assert_eq!(fixture.pending(0)?.entries.len(), 0);
        fixture.verify_all()?;
        fixture.cleanup()?;
    }
    Ok(())
}

#[test]
fn catalog_publisher_valid_prefix_and_exact_batch_shape_control_actual_ack_range() -> TestResult {
    let mut fixture = PublicationFixture::combined()?;
    fixture.fault = Fault::CorruptMiddle;
    let result = fixture.run(0, Limits::default())?;
    assert_eq!((result.published, result.errors), (1, 1));
    assert_eq!(fixture.members()?.len(), 1);
    let page = fixture.pending(0)?;
    assert_eq!((page.meta.head(), page.entries.len()), (2, 2));
    fixture.fault = Fault::ShortOwners;
    let before = fixture.cohort(0)?;
    let result = fixture.run(0, Limits::default())?;
    assert_eq!(
        (
            result.published,
            result.errors,
            result.read_batches,
            result.write_batches
        ),
        (0, 1, 2, 0)
    );
    assert_eq!(fixture.cohort(0)?, before);
    let retry = fixture.run(0, Limits::default())?;
    assert_eq!((retry.published, retry.errors), (2, 0));
    assert_eq!(fixture.members()?.len(), 3);
    fixture.verify_all()?;
    fixture.cleanup()?;
    Ok(())
}

#[test]
fn catalog_publisher_preflights_workspace_and_advances_only_one_source_after_read_error()
-> TestResult {
    let mut fixture = PublicationFixture::separate()?;
    let before = fixture.cohort(1)?;
    let limits = Limits {
        workspace_bytes: 1,
        ..Limits::default()
    };
    assert!(fixture.run(1, limits).is_err());
    assert_eq!(fixture.calls.len(), 0);
    assert_eq!(fixture.cohort(1)?, before);
    fixture.fault = Fault::UnavailablePage;
    let result = fixture.run(0, Limits::default())?;
    assert_eq!(
        (
            result.published,
            result.errors,
            result.sources_scanned,
            result.next_source
        ),
        (0, 1, 1, 1)
    );
    assert_eq!(fixture.calls, vec![Call::Page]);
    fixture.calls.clear();
    let retry = fixture.run(result.next_source, Limits::default())?;
    assert_eq!((retry.published, retry.errors), (3, 0));
    assert_eq!(fixture.pending(1)?.entries.len(), 0);
    fixture.cleanup()?;
    Ok(())
}
