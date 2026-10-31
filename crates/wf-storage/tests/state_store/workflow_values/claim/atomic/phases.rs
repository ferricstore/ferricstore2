use super::super::fixtures as leaf;
use super::{fixtures as a, *};
use wf_ports::log::inspect_atomic_claim_reply;
use wf_storage::transaction::apply_atomic_claim_transaction;

#[test]
fn three_reclaim_phases_publish_one_parent_and_observe_prior_staged_leases() -> TestResult {
    let dir = f::directory("atomic-claim-three-phases")?;
    let path = dir.join("state");
    let mut store = a::filled(&path)?;
    let before = [
        leaf::record(&mut store, b"r1")?,
        leaf::record(&mut store, b"q1")?,
        leaf::record(&mut store, b"r2")?,
        leaf::record(&mut store, b"r3")?,
    ];
    let frame = a::frame(100)?;
    let outcome = leaf::retained(apply_atomic_claim_transaction(
        &mut store,
        &f::scope()?,
        frame.as_bytes(),
        f::log(5),
        a::limits(),
    )?)?;
    assert_eq!(
        (
            outcome.intent().version(),
            outcome.response_version(),
            outcome.original_position()
        ),
        (20, 17, f::log(5))
    );
    let reply = inspect_atomic_claim_reply(
        outcome.response(),
        outcome.original_time(),
        v::limits().record_batch.codec.admission(),
    )?;
    assert_eq!(
        (reply.status(), reply.count(), reply.phase_count()),
        (RetainedStatus::Accepted, 4, 3)
    );
    let mut phases = reply.phases();
    let mut actual = Vec::new();
    while !phases.is_empty() {
        let kind = phases[0];
        let length = usize::try_from(u32::from_be_bytes(phases[1..5].try_into()?))?;
        let body = &phases[5..5 + length];
        let count = u32::from_be_bytes(body[17..21].try_into()?);
        actual.push((kind, count));
        phases = &phases[5 + length..];
    }
    assert_eq!(actual, [(2, 1), (1, 1), (2, 2)]);
    for (id, old) in [b"r1", b"q1", b"r2", b"r3"].into_iter().zip(before) {
        let current = leaf::record(&mut store, id)?;
        assert_eq!(current.record.version(), old.record.version() + 1);
        assert_eq!(current.record.fence().get(), old.record.fence().get() + 1);
        assert_eq!(current.record.lease().ok_or("lease")?.worker, b"new");
        assert_eq!(current.ordering, old.ordering);
        assert_eq!(
            leaf::history(&store, id)?.entries.len(),
            usize::try_from(current.record.version())?
        );
    }
    drop(store);
    let mut store = f::open(&path)?;
    let retried = leaf::retained(apply_atomic_claim_transaction(
        &mut store,
        &f::scope()?,
        frame.as_bytes(),
        f::log(6),
        a::limits(),
    )?)?;
    assert_eq!(retried, outcome);
    assert_eq!(leaf::record(&mut store, b"r1")?.record.version(), 3);
    drop(store);
    std::fs::remove_dir_all(dir)?;
    Ok(())
}
