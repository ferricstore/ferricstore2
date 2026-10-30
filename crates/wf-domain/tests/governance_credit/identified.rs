use super::{TestResult, fixtures as f};
use wf_domain::governance::credit::{CreditError, CreditLimits, CreditOwner, restore_credit_owner};
use wf_domain::governance::identified::{
    CreditCleanupTask, CreditPageData, CreditProtocolObservation as Observation,
    CreditProtocolQuery as Query, CreditReservationObservation, CreditReservationStatus as Status,
    IdentifiedCreditDecision as Decision, IdentifiedCreditMutation as Mutation,
    IdentifiedCreditPlan, IdentifiedCreditRejection as Rejection, IdentifiedCreditRequest,
    prepare_identified_credit, resume_identified_credit,
};
use wf_types::version::TimestampMillis;

fn command(mutation: Mutation<'_>) -> IdentifiedCreditRequest<'_> {
    IdentifiedCreditRequest {
        scope: b"global",
        shard_count: 4,
        configuration: None,
        mutation,
    }
}

fn owner() -> TestResult<CreditOwner> {
    f::grant(f::owner(b"global", 3)?, 0, 3, 1000, 1000)
}

fn observed<'a>(ids: &'a [Vec<u8>], statuses: &[Option<Status>], epoch: u64) -> Observation<'a> {
    Observation::Reservations(
        ids.iter()
            .zip(statuses)
            .map(|(id, status)| CreditReservationObservation {
                id,
                shard_id: 0,
                epoch,
                status: *status,
            })
            .collect(),
    )
}

fn spent(ids: &[Vec<u8>]) -> TestResult<IdentifiedCreditPlan<'_>> {
    let request = command(Mutation::Spend {
        shard_id: 0,
        lease_epoch: 1,
        amount: 2,
        ttl_ms: None,
        reservation_ids: ids,
    });
    let pending = prepare_identified_credit(
        Some(owner()?),
        request,
        TimestampMillis::new(1001),
        CreditLimits::default(),
    )?;
    assert!(matches!(
        pending.query(),
        Some(Query::Reservations {
            shard_id: 0,
            epoch: 1,
            ..
        })
    ));
    Ok(resume_identified_credit(pending, observed(ids, &[None, None], 1))?.finish()?)
}

#[test]
fn identified_sparse_spend_release_and_known_tombstone_retry_preserve_credit() -> TestResult {
    let ids = vec![b"a".to_vec(), b"b".to_vec()];
    let plan = spent(&ids)?;
    assert_eq!(plan.decision(), Decision::Spent { shard_id: 0 });
    assert_eq!(plan.response_ids(), ids);
    assert_eq!(plan.reservations().len(), 2);
    let lease = plan
        .owner()
        .ok_or("missing owner")?
        .lease(0)
        .ok_or("missing lease")?;
    assert_eq!(
        (
            lease.available,
            lease.in_use,
            lease.reservation_page,
            lease.reservation_page_fill
        ),
        (1, 2, 1, 2)
    );
    let a = vec![b"a".to_vec()];
    let pending = prepare_identified_credit(
        plan.owner().cloned(),
        command(Mutation::Release {
            shard_id: 0,
            amount: 1,
            reservation_ids: &a,
        }),
        TimestampMillis::new(1002),
        CreditLimits::default(),
    )?;
    let released =
        resume_identified_credit(pending, observed(&a, &[Some(Status::Active)], 1))?.finish()?;
    assert_eq!(released.decision(), Decision::Released);
    assert_eq!(released.reservations()[0].status, Some(Status::Released));
    let prior = released.owner().ok_or("missing owner")?;
    assert_eq!(
        (
            prior.lease(0).ok_or("missing lease")?.in_use,
            prior.view().epoch
        ),
        (1, 1)
    );
    let pending = prepare_identified_credit(
        Some(prior.clone()),
        command(Mutation::Spend {
            shard_id: 0,
            lease_epoch: 1,
            amount: 2,
            ttl_ms: Some(9000),
            reservation_ids: &ids,
        }),
        TimestampMillis::new(1003),
        CreditLimits::default(),
    )?;
    let duplicate = resume_identified_credit(
        pending,
        observed(&ids, &[Some(Status::Released), Some(Status::Active)], 1),
    )?
    .finish()?;
    assert_eq!(duplicate.decision(), Decision::Spent { shard_id: 0 });
    assert_eq!(duplicate.owner(), Some(prior));
    assert_eq!(duplicate.reservations().len(), 0);
    assert_eq!(duplicate.pages().len(), 0);
    Ok(())
}

#[test]
fn identified_incomplete_and_foreign_observations_expose_no_finished_plan() -> TestResult {
    let ids = vec![b"a".to_vec(), b"b".to_vec()];
    let request = command(Mutation::Spend {
        shard_id: 0,
        lease_epoch: 1,
        amount: 2,
        ttl_ms: None,
        reservation_ids: &ids,
    });
    let pending = prepare_identified_credit(
        Some(owner()?),
        request,
        TimestampMillis::new(1001),
        CreditLimits::default(),
    )?;
    assert_eq!(pending.finish().err(), Some(CreditError::InvalidInput));
    for case in 0..4 {
        let pending = prepare_identified_credit(
            Some(owner()?),
            request,
            TimestampMillis::new(1001),
            CreditLimits::default(),
        )?;
        let mut values = vec![
            CreditReservationObservation {
                id: &ids[0],
                shard_id: 0,
                epoch: 1,
                status: None,
            },
            CreditReservationObservation {
                id: &ids[1],
                shard_id: 0,
                epoch: 1,
                status: None,
            },
        ];
        match case {
            0 => {
                values.pop();
            }
            1 => values[0].id = b"foreign",
            2 => values[0].shard_id = 1,
            _ => values[0].epoch = 2,
        }
        assert_eq!(
            resume_identified_credit(pending, Observation::Reservations(values)).err(),
            Some(CreditError::InvalidState)
        );
    }
    Ok(())
}

#[test]
fn identified_retirement_and_cleanup_move_real_ids_and_preserve_epoch_fencing() -> TestResult {
    let ids = vec![b"a".to_vec(), b"b".to_vec()];
    let plan = spent(&ids)?;
    let pending = prepare_identified_credit(
        plan.owner().cloned(),
        command(Mutation::Release {
            shard_id: 0,
            amount: 2,
            reservation_ids: &ids,
        }),
        TimestampMillis::new(1002),
        CreditLimits::default(),
    )?;
    let released = resume_identified_credit(
        pending,
        observed(&ids, &[Some(Status::Active), Some(Status::Active)], 1),
    )?
    .finish()?;
    let owner = released.owner().ok_or("missing owner")?;
    let lease = owner.lease(0).ok_or("missing lease")?;
    assert_eq!(
        (
            owner.view().epoch,
            lease.epoch,
            lease.available,
            lease.in_use,
            lease.reservation_page
        ),
        (2, 2, 3, 0, 0)
    );
    let task = CreditCleanupTask {
        shard_id: 0,
        epoch: 1,
        next_page: 1,
        last_page: 1,
    };
    assert_eq!(released.cleanup().len(), 1);
    assert_eq!(released.cleanup()[0].task, Some(task));
    let stale = prepare_identified_credit(
        Some(owner.clone()),
        command(Mutation::Spend {
            shard_id: 0,
            lease_epoch: 1,
            amount: 2,
            ttl_ms: None,
            reservation_ids: &ids,
        }),
        TimestampMillis::new(1003),
        CreditLimits::default(),
    )?;
    assert!(stale.query().is_none());
    assert_eq!(
        stale.finish()?.decision(),
        Decision::Rejected(Rejection::LeaseGenerationChanged)
    );
    let pending = prepare_identified_credit(
        Some(owner.clone()),
        command(Mutation::Cleanup),
        TimestampMillis::new(1003),
        CreditLimits::default(),
    )?;
    assert!(matches!(
        pending.query(),
        Some(Query::CleanupTask { sequence: 1 })
    ));
    let pending = resume_identified_credit(pending, Observation::CleanupTask(Some(task)))?;
    assert!(matches!(
        pending.query(),
        Some(Query::Page {
            shard_id: 0,
            epoch: 1,
            page: 1
        })
    ));
    let page = CreditPageData {
        shard_id: 0,
        epoch: 1,
        page: 1,
        reservation_ids: ids.clone(),
    };
    let original_buffer = page.reservation_ids[0].as_ptr();
    let clean = resume_identified_credit(pending, Observation::Page(Some(page)))?.finish()?;
    assert_eq!(
        clean.decision(),
        Decision::Cleaned {
            deleted: 2,
            pending: false
        }
    );
    assert_eq!(clean.reservations()[0].id.as_ptr(), original_buffer);
    assert_eq!(clean.reservations()[0].status, None);
    let owner = clean.owner().ok_or("missing owner")?;
    assert_eq!(
        (owner.view().cleanup_head, owner.view().cleanup_tail),
        (2, 1)
    );
    Ok(())
}

#[test]
fn identified_full_history_refuses_before_page_reads_without_prefix_effects() -> TestResult {
    let mut data = owner()?.into_data();
    data.leases[0].available = 2;
    data.leases[0].in_use = 1;
    data.leases[0].reservation_page = 256;
    data.leases[0].reservation_page_fill = 256;
    let owner = restore_credit_owner(data, CreditLimits::default())?;
    let ids = vec![b"fresh".to_vec()];
    let pending = prepare_identified_credit(
        Some(owner),
        command(Mutation::Spend {
            shard_id: 0,
            lease_epoch: 1,
            amount: 1,
            ttl_ms: None,
            reservation_ids: &ids,
        }),
        TimestampMillis::new(1001),
        CreditLimits::default(),
    )?;
    let pending = resume_identified_credit(pending, observed(&ids, &[None], 1))?;
    assert!(pending.query().is_none());
    let plan = pending.finish()?;
    assert_eq!(
        plan.decision(),
        Decision::Rejected(Rejection::ReservationHistoryFull)
    );
    assert!(plan.changed_owner().is_none());
    assert_eq!(plan.reservations().len(), 0);
    assert_eq!(plan.pages().len(), 0);
    assert_eq!(plan.cleanup().len(), 0);
    Ok(())
}
