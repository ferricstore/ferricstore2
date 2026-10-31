//! Source cleanup budgets through actual logs, canonical state and snapshots.
use super::{TestResult, f};
use wf_application::governance::{
    CreditCleanupLimits, CreditCleanupRequest, CreditCleanupSource, CreditCleanupStats as Stats,
    run_credit_cleanup_tick,
};
use wf_domain::workflow::request::RequestScope;
use wf_ports::state::{
    GovernanceCatalogProgress, GovernanceCatalogProgressPort, GovernanceCatalogReadLimits,
};
mod fixtures;
mod ports;
use fixtures::{CleanupCall, CleanupFault, CleanupFixture};

#[test]
fn credit_cleaner_actual_600_ids_obey_two_command_budget_then_delete_remaining_88() -> TestResult {
    let mut fixture = CleanupFixture::new()?;
    let ids = fixture.seed(b"a", 600)?;
    assert_eq!(ids.len(), 600);
    let first = fixture.run(2)?;
    assert_eq!((first.commands, first.deleted, first.errors), (2, 512, 0));
    assert_eq!(
        fixture.remaining(b"a")?,
        ids[512..].iter().cloned().collect()
    );
    assert_eq!(fixture.progress()?, GovernanceCatalogProgress::Missing);
    let second = fixture.run(2)?;
    assert_eq!((second.commands, second.deleted, second.errors), (1, 88, 0));
    assert!(second.caught_up);
    assert!(fixture.remaining(b"a")?.is_empty());
    fixture.snapshot()?;
    fixture.cleanup()
}
#[test]
fn credit_cleaner_actual_busy_300_then_later_owner_respects_fairness_and_cursor_budget()
-> TestResult {
    let mut fixture = CleanupFixture::new()?;
    let busy = fixture.seed(b"a", 300)?;
    let later = fixture.seed(b"b", 1)?;
    let first = fixture.run(2)?;
    assert_eq!((first.commands, first.deleted, first.errors), (2, 300, 0));
    assert!(fixture.remaining(b"a")?.is_empty());
    assert_eq!(busy.len(), 300);
    assert_eq!(fixture.remaining(b"b")?, later.into_iter().collect());
    assert_eq!(fixture.progress()?, GovernanceCatalogProgress::Missing);
    let second = fixture.run(2)?;
    assert_eq!((second.commands, second.deleted, second.errors), (2, 1, 0));
    assert!(fixture.remaining(b"b")?.is_empty());
    fixture.cleanup()
}
#[test]
fn credit_cleaner_actual_empty_catalog_issues_no_command_and_changes_neither_store() -> TestResult {
    let mut fixture = CleanupFixture::new()?;
    assert_eq!(fixture.progress()?, GovernanceCatalogProgress::Missing);
    let before = fixture.cohort()?;
    let result = fixture.run(1)?;
    assert_eq!((result.commands, result.deleted, result.errors), (0, 0, 0));
    assert!(result.caught_up);
    assert!(fixture.calls.borrow().is_empty());
    assert_eq!(fixture.cohort()?, before);
    assert_eq!(fixture.progress()?, GovernanceCatalogProgress::Missing);
    fixture.cleanup()
}
#[test]
fn credit_cleaner_actual_invalid_hint_reset_uses_whole_tick_budget_before_cleanup() -> TestResult {
    let mut fixture = CleanupFixture::new()?;
    let ids = fixture.seed(b"a", 1)?;
    fixture.set_progress(Some(b"bad"))?;
    fixture.damage_hint()?;
    assert_eq!(fixture.progress()?, GovernanceCatalogProgress::Invalid);
    let first = fixture.run(1)?;
    assert_eq!((first.commands, first.deleted, first.errors), (1, 0, 0));
    assert_eq!(fixture.progress()?, GovernanceCatalogProgress::Missing);
    assert_eq!(fixture.remaining(b"a")?, ids.into_iter().collect());
    let second = fixture.run(1)?;
    assert_eq!((second.commands, second.deleted, second.errors), (1, 1, 0));
    fixture.cleanup()
}
#[test]
fn credit_cleaner_actual_missing_owner_prunes_member_once_without_cursor_delete() -> TestResult {
    let mut fixture = CleanupFixture::new()?;
    fixture.register(b"ghost")?;
    let result = fixture.run(1)?;
    assert_eq!((result.commands, result.deleted, result.errors), (1, 0, 0));
    assert_eq!(*fixture.calls.borrow(), vec![CleanupCall::Prune]);
    let before = fixture.cohort()?;
    assert_eq!(fixture.run(1)?.commands, 0);
    assert_eq!(fixture.cohort()?, before);
    fixture.cleanup()
}
#[test]
fn credit_cleaner_actual_exhausted_cursor_budget_preserves_old_hint_and_wrap_follows_persistence()
-> TestResult {
    let mut fixture = CleanupFixture::new()?;
    fixture.seed(b"a", 1)?;
    fixture.seed(b"b", 1)?;
    let first = fixture.run(1)?;
    assert_eq!((first.commands, first.deleted), (1, 1));
    assert!(!first.wrapped);
    assert!(!first.caught_up);
    assert_eq!(fixture.progress()?, GovernanceCatalogProgress::Missing);
    let second = fixture.run(2)?;
    assert_eq!((second.commands, second.deleted), (2, 1));
    assert!(!second.wrapped);
    assert_eq!(
        fixture.progress()?,
        GovernanceCatalogProgress::Cursor(fixture.member(b"a")?)
    );
    fixture.snapshot()?;
    let third = fixture.run(1)?;
    assert_eq!((third.commands, third.deleted), (1, 0));
    assert!(third.caught_up);
    assert_eq!(fixture.progress()?, GovernanceCatalogProgress::Missing);
    fixture.cleanup()
}
#[test]
fn credit_cleaner_actual_failed_cleanup_and_cursor_write_share_budget_and_do_not_starve_later_owner()
-> TestResult {
    let mut fixture = CleanupFixture::new()?;
    fixture.seed(b"a", 1)?;
    fixture.seed(b"b", 1)?;
    fixture.fault.set(CleanupFault::RefuseCleanup);
    let first = fixture.run(2)?;
    assert_eq!((first.commands, first.deleted, first.errors), (2, 0, 1));
    assert_eq!(
        fixture.progress()?,
        GovernanceCatalogProgress::Cursor(fixture.member(b"a")?)
    );
    assert_eq!(fixture.remaining(b"a")?.len(), 1);
    let second = fixture.run(2)?;
    assert_eq!((second.commands, second.deleted, second.errors), (2, 1, 0));
    assert!(fixture.remaining(b"b")?.is_empty());
    fixture.cleanup()
}
#[test]
fn credit_cleaner_actual_lost_progress_reply_never_claims_wrap_and_next_step_reloads_durable_cursor()
-> TestResult {
    let mut fixture = CleanupFixture::new()?;
    fixture.seed(b"a", 1)?;
    fixture.seed(b"b", 1)?;
    fixture.fault.set(CleanupFault::LoseProgress);
    let first = fixture.run(2)?;
    assert_eq!((first.commands, first.deleted, first.errors), (2, 1, 1));
    assert!(!first.wrapped);
    assert!(!first.caught_up);
    assert_eq!(
        fixture.progress()?,
        GovernanceCatalogProgress::Cursor(fixture.member(b"a")?)
    );
    let second = fixture.run(2)?;
    assert_eq!((second.commands, second.deleted, second.errors), (2, 1, 0));
    fixture.cleanup()
}

#[test]
fn credit_cleaner_actual_refused_progress_keeps_old_cursor_and_does_not_claim_wrap() -> TestResult {
    let mut fixture = CleanupFixture::new()?;
    fixture.seed(b"a", 1)?;
    fixture.seed(b"b", 1)?;
    fixture.fault.set(CleanupFault::RefuseProgress);
    let first = fixture.run(2)?;
    assert_eq!((first.commands, first.deleted, first.errors), (2, 1, 1));
    assert!(!first.wrapped);
    assert_eq!(fixture.progress()?, GovernanceCatalogProgress::Missing);
    assert_eq!(fixture.remaining(b"b")?.len(), 1);
    let second = fixture.run(2)?;
    assert_eq!((second.commands, second.deleted, second.errors), (2, 1, 0));
    fixture.cleanup()
}
#[test]
fn credit_cleaner_actual_stale_trusted_stamp_is_refused_before_submission() -> TestResult {
    let mut fixture = CleanupFixture::new()?;
    fixture.seed(b"a", 1)?;
    fixture.seed(b"b", 1)?;
    let before = fixture.cohort()?;
    fixture.fault.set(CleanupFault::StaleTime);
    let result = fixture.run(2)?;
    assert_eq!((result.commands, result.deleted, result.errors), (2, 0, 1));
    assert!(!result.wrapped);
    assert!(fixture.calls.borrow().is_empty());
    assert_eq!(fixture.cohort()?, before);
    fixture.cleanup()
}
#[test]
fn credit_cleaner_actual_repeated_read_failure_stops_at_four_budget_plus_four_attempts()
-> TestResult {
    let mut fixture = CleanupFixture::new()?;
    let before = fixture.cohort()?;
    fixture.fault.set(CleanupFault::ReadProgress);
    let result = fixture.run(1)?;
    assert_eq!((result.commands, result.deleted, result.errors), (0, 0, 8));
    assert!(!result.caught_up);
    assert_eq!(fixture.cohort()?, before);
    fixture.cleanup()
}
#[test]
fn credit_cleaner_actual_invalid_budget_refuses_before_either_store_changes() -> TestResult {
    let mut fixture = CleanupFixture::new()?;
    let before = fixture.cohort()?;
    for budget in [0, 65] {
        assert_eq!(
            fixture
                .run(budget)
                .err()
                .and_then(|e| e
                    .downcast::<wf_application::governance::CreditCleanupError>()
                    .ok())
                .map(|e| *e),
            Some(wf_application::governance::CreditCleanupError::InvalidOptions)
        );
    }
    assert!(fixture.calls.borrow().is_empty());
    assert_eq!(fixture.cohort()?, before);
    fixture.cleanup()
}

#[test]
fn credit_cleaner_actual_cleanup_accepts_monotone_execution_time_above_prepared_floor() -> TestResult
{
    let mut fixture = CleanupFixture::new()?;
    fixture.seed(b"a", 1)?;
    let request = wf_types::identity::RequestId::new(b"clock-advance".to_vec())?;
    let frame = wf_ports::log::encode_catalog_progress_request(
        &fixture.scope,
        &request,
        f::clock(1007, 0, 1007)?,
        wf_domain::governance::catalog::CatalogProgressChange {
            catalog: b"limit",
            cursor: None,
        },
        f::driver_limits().codec,
        16 * f::MIB,
    )?;
    fixture.commit(&request, frame)?;
    let result = fixture.run(1)?;
    assert_eq!((result.commands, result.deleted, result.errors), (1, 1, 0));
    assert!(fixture.remaining(b"a")?.is_empty());
    fixture.cleanup()
}
#[test]
fn credit_cleaner_actual_unknown_cleanup_reply_never_invents_confirmed_deletion_credit()
-> TestResult {
    let mut fixture = CleanupFixture::new()?;
    fixture.seed(b"a", 1)?;
    fixture.seed(b"b", 1)?;
    fixture.fault.set(CleanupFault::LoseCleanup);
    let result = fixture.run(2)?;
    assert_eq!((result.commands, result.deleted, result.errors), (2, 0, 1));
    assert!(fixture.remaining(b"a")?.is_empty());
    assert_eq!(fixture.remaining(b"b")?.len(), 1);
    assert_eq!(
        fixture.progress()?,
        GovernanceCatalogProgress::Cursor(fixture.member(b"a")?)
    );
    assert_eq!(fixture.run(2)?.deleted, 1);
    fixture.cleanup()
}
#[test]
fn credit_cleaner_actual_foreign_cleanup_receipt_is_not_counted_as_accepted_cleanup() -> TestResult
{
    let mut fixture = CleanupFixture::new()?;
    fixture.seed(b"a", 1)?;
    fixture.seed(b"b", 1)?;
    fixture.fault.set(CleanupFault::ForeignCleanup);
    let result = fixture.run(2)?;
    assert_eq!((result.commands, result.deleted, result.errors), (2, 0, 1));
    assert!(fixture.remaining(b"a")?.is_empty());
    assert_eq!(fixture.remaining(b"b")?.len(), 1);
    fixture.cleanup()
}
#[test]
fn credit_cleaner_actual_foreign_progress_receipt_cannot_claim_a_confirmed_cursor_wrap()
-> TestResult {
    let mut fixture = CleanupFixture::new()?;
    fixture.seed(b"a", 1)?;
    fixture.seed(b"b", 1)?;
    fixture.fault.set(CleanupFault::ForeignProgress);
    let result = fixture.run(2)?;
    assert_eq!((result.commands, result.deleted, result.errors), (2, 1, 1));
    assert!(!result.wrapped);
    assert!(!result.caught_up);
    assert_eq!(
        fixture.progress()?,
        GovernanceCatalogProgress::Cursor(fixture.member(b"a")?)
    );
    fixture.cleanup()
}

#[test]
fn credit_cleaner_actual_default_sixteen_command_budget_and_maximum_sixty_four_preserve_all_ids()
-> TestResult {
    let mut fixture = CleanupFixture::new()?;
    for name in [b"a", b"b", b"c", b"d", b"e"] {
        assert_eq!(fixture.seed(name, 1000)?.len(), 1000);
    }
    let scope = fixture.scope.clone();
    let sources = [CreditCleanupSource {
        scope: &scope,
        shard_count: 4,
    }];
    let result = run_credit_cleanup_tick(
        CreditCleanupRequest {
            target: &scope,
            sources: &sources,
            catalog: b"limit",
            now_ms: 1006,
        },
        &mut fixture,
        None,
        fixtures::limits(),
    )?;
    assert_eq!((result.commands, result.errors), (16, 0));
    assert!(!result.caught_up);
    let mut remaining = 0;
    for name in [b"a", b"b", b"c", b"d", b"e"] {
        remaining += fixture.remaining(name)?.len();
    }
    assert_eq!(remaining + usize::try_from(result.deleted)?, 5000);
    let final_tick = fixture.run(64)?;
    assert!(final_tick.commands <= 64);
    assert_eq!(final_tick.errors, 0);
    assert!(final_tick.caught_up);
    for name in [b"a", b"b", b"c", b"d", b"e"] {
        assert!(fixture.remaining(name)?.is_empty());
    }
    fixture.cleanup()
}
