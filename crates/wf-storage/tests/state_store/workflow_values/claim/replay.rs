use super::{fixtures as s, *};
use wf_domain::lease::{LeaseGuard, LeaseToken};
use wf_domain::retry::{Retry, RetryTime};
use wf_domain::workflow::Operation;
use wf_ports::consensus::PreparedOperation;
use wf_types::version::{Fence, TimestampMillis};

#[test]
fn named_value_claim_replays_original_before_changed_current_and_all_gate_kinds() -> TestResult {
    let dir = f::directory("named-value-claim-replay")?;
    let path = dir.join("state");
    let mut store = s::fresh(&path, false)?;
    s::create(&mut store, b"i", b"q", (0, 0, 0), Some(b"old"))?;
    let frame = s::phase(b"claim", 2, b"q", b"w", 1)?;
    let original = s::retained(s::apply(&mut store, &frame, 1)?)?;
    let mutation = v::batch(
        5,
        b"retry",
        3,
        PreparedOperation::Batch(vec![v::item(
            b"i",
            3,
            Operation::Retry(Retry {
                guard: LeaseGuard {
                    token: LeaseToken::new(b"w:2:1".to_vec())?,
                    fence: Fence::new(1),
                },
                run_at: RetryTime::Explicit(TimestampMillis::new(10)),
                payload: None,
                error: Some(b"again".to_vec()),
            }),
        )?]),
        vec![v::input(0, 0)],
        vec![v::witness(b"r", None)?],
    )?;
    assert_eq!(
        s::retained_record(v::apply_value(&mut store, &f::scope()?, &mutation, 2)?)?.status(),
        RetainedStatus::Accepted
    );
    let after = s::record(&mut store, b"i")?;
    assert_eq!(after.record.version(), 3);
    assert!(after.record.lease().is_none());
    let replay = s::phase(b"claim", 4, b"q", b"w", 1)?;
    assert_eq!(
        s::retained(s::apply(&mut store, &replay, 3)?)?.as_ref(),
        original.as_ref()
    );
    assert_eq!(s::record(&mut store, b"i")?, after);
    let conflict = s::phase_with_intent(b"claim", 5, b"q", b"x", 1, [43; 32])?;
    assert!(matches!(
        s::apply(&mut store, &conflict, 4)?,
        ClaimTransactionOutcome::IntentConflict(_)
    ));
    let expired = s::phase(b"claim", 1002, b"q", b"w", 1)?;
    assert!(matches!(
        s::apply(&mut store, &expired, 5)?,
        ClaimTransactionOutcome::Expired(_)
    ));
    assert!(matches!(
        s::apply(&mut store, &expired, 5)?,
        ClaimTransactionOutcome::LogReplay(_)
    ));
    assert_eq!(s::record(&mut store, b"i")?, after);
    assert_eq!(s::history(&store, b"i")?.entries.len(), 3);
    drop(store);
    drop(f::open(&path)?);
    std::fs::remove_dir_all(dir)?;
    Ok(())
}

#[test]
fn named_value_claim_empty_and_rejected_results_publish_no_value_bindings() -> TestResult {
    let dir = f::directory("named-value-claim-empty")?;
    let path = dir.join("state");
    let mut store = s::fresh(&path, false)?;
    let empty = s::retained(s::apply(
        &mut store,
        &s::phase(b"empty", 1, b"q", b"w", 1)?,
        0,
    )?)?;
    assert_eq!(s::summary(&empty)?, (RetainedStatus::Accepted, Some(0)));
    let rejected = s::retained(s::apply(
        &mut store,
        &s::phase(b"bad", 2, b"q", b"w", 0)?,
        1,
    )?)?;
    assert_eq!(
        s::summary(&rejected)?,
        (
            RetainedStatus::ClaimRejected(wf_domain::claim::apply::ClaimRejection::InvalidBudget),
            None
        )
    );
    drop(store);
    assert!(
        s::cohort(&path)?[1]
            .keys()
            .all(|key| !matches!(key.get(6), Some(1 | 5 | 8..=11)))
    );
    drop(f::open(&path)?);
    std::fs::remove_dir_all(dir)?;
    Ok(())
}
