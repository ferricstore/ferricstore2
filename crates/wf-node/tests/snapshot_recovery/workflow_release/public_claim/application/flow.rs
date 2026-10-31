use super::{fixtures::*, *};

#[test]
fn original_reply_binding_checks_the_actual_three_phase_schedule_and_its_unused_credit()
-> TestResult {
    let dir = f::directory("claim-app-three-phase-binding")?;
    let mut port = ClaimApplicationFixture::new(&dir)?;
    let (scope, id) = (f::scope()?, id()?);
    let mut input = ClaimDueInput::new(b"r", b"w");
    input.states = ClaimStatesInput::One(b"q");
    input.partitions = ClaimPartitionsInput::One(b"p");
    input.limit = Some(3);
    input.lease_ms = Some(100);
    input.reclaim_expired = Some(true);
    input.governance_limit_scope = Some(b"global");
    input.governance_shard_id = Some(0);
    let caller = normalize_claim_due(input, ClaimCallerLimits::default())?;
    let R::Retained(outcome) = submit_public_claim_call(
        Request {
            scope: &scope,
            request: &id,
            caller: &caller,
        },
        &mut port,
        Limits::default(),
    )?
    else {
        return Err("three-phase original reply".into());
    };
    let reply = wf_ports::log::inspect_public_claim_reply_for_call(
        outcome.response(),
        outcome.original_time(),
        &caller,
        Limits::default().reply,
    )?;
    let claim = reply.claim().ok_or("actual phased claim")?;
    assert_eq!((claim.count(), claim.phase_count()), (2, 3));
    assert_eq!(claim.used_indices().collect::<Vec<_>>(), [0, 1]);
    let lease = owner(&port)?;
    assert_eq!(
        (
            lease.lease(0).ok_or("lease")?.available,
            lease.lease(0).ok_or("lease")?.in_use
        ),
        (1, 2)
    );
    drop(port);
    std::fs::remove_dir_all(dir)?;
    Ok(())
}

#[test]
fn conflicting_original_and_actual_committed_expiry_never_generate_another_nonce_or_spend()
-> TestResult {
    use wf_domain::workflow::control::EntryStamp;
    let dir = f::directory("claim-app-original-dispositions")?;
    let mut port = ClaimApplicationFixture::new(&dir)?;
    let (scope, id, caller) = (f::scope()?, id()?, caller()?);
    assert!(matches!(
        submit_public_claim_call(
            Request {
                scope: &scope,
                request: &id,
                caller: &caller
            },
            &mut port,
            Limits::default()
        )?,
        R::Retained(_)
    ));
    let mut input = ClaimDueInput::new(b"r", b"other-worker");
    input.states = ClaimStatesInput::One(b"q");
    input.partitions = ClaimPartitionsInput::One(b"p");
    input.limit = Some(3);
    input.lease_ms = Some(100);
    input.reclaim_expired = Some(false);
    input.governance_limit_scope = Some(b"global");
    input.governance_shard_id = Some(0);
    let changed = normalize_claim_due(input, ClaimCallerLimits::default())?;
    port.trace.clear();
    assert!(matches!(
        submit_public_claim_call(
            Request {
                scope: &scope,
                request: &id,
                caller: &changed
            },
            &mut port,
            Limits::default()
        )?,
        R::IntentConflict
    ));
    assert_eq!(port.trace, [ClaimApplicationTrace::Lookup]);
    let blank = f::blank(EntryStamp::Stamped(f::clock(2000, 0, 2000)?))?;
    f::append(&mut port.log, 4, blank.as_bytes())?;
    port.log.save_committed(f::at(4))?;
    f::apply_control(&mut port.state, 4, blank.as_bytes())?;
    port.trace.clear();
    assert!(matches!(
        submit_public_claim_call(
            Request {
                scope: &scope,
                request: &id,
                caller: &caller
            },
            &mut port,
            Limits::default()
        )?,
        R::Expired
    ));
    assert_eq!(port.trace, [ClaimApplicationTrace::Lookup]);
    assert_eq!(port.frames.len(), 1);
    assert_eq!(port.log.state()?.last_log(), Some(f::at(4)));
    assert_eq!(
        owner(&port)?
            .lease(0)
            .ok_or("original unchanged credits")?
            .in_use,
        2
    );
    drop(port);
    std::fs::remove_dir_all(dir)?;
    Ok(())
}
#[test]
fn actual_application_funds_claims_refunds_and_reopens_its_original_without_new_preparation()
-> TestResult {
    let dir = f::directory("claim-app-actual-flow")?;
    let mut port = ClaimApplicationFixture::new(&dir)?;
    let (scope, id, caller) = (f::scope()?, id()?, caller()?);
    let R::Retained(outcome) = submit_public_claim_call(
        Request {
            scope: &scope,
            request: &id,
            caller: &caller,
        },
        &mut port,
        Limits::default(),
    )?
    else {
        return Err("actual retained Claim".into());
    };
    let reply = wf_ports::log::inspect_public_claim_reply(
        outcome.response(),
        outcome.original_time(),
        Limits::default().reply.admission,
    )?;
    let claim = reply.claim().ok_or("actual claim")?;
    assert_eq!(claim.count(), 2);
    assert_eq!(claim.payment().ok_or("original paid batch")?.count(), 3);
    assert_eq!(claim.used_indices().collect::<Vec<_>>(), [0, 1]);
    let owner = owner(&port)?;
    let lease = owner.lease(0).ok_or("lease")?;
    assert_eq!(
        (owner.view().free, lease.available, lease.in_use),
        (0, 1, 2)
    );
    assert_eq!(
        port.trace,
        [
            ClaimApplicationTrace::Lookup,
            ClaimApplicationTrace::Prepare,
            ClaimApplicationTrace::Submit,
            ClaimApplicationTrace::Commit
        ]
    );
    port.trace.clear();
    let R::Retained(replayed) = submit_public_claim_call(
        Request {
            scope: &scope,
            request: &id,
            caller: &caller,
        },
        &mut port,
        Limits::default(),
    )?
    else {
        return Err("original inquiry".into());
    };
    assert_eq!(replayed, outcome);
    assert_eq!(port.trace, [ClaimApplicationTrace::Lookup]);
    drop(port);
    let before = p::cohort(&dir)?;
    let (log, state) = f::open_pair(&dir)?;
    drop(wf_node::recovery::recover_committed(
        log,
        state,
        scope.group(),
        scope.recovery(),
        super::super::limits()?,
    )?);
    assert_eq!(p::cohort(&dir)?, before);
    std::fs::remove_dir_all(dir)?;
    Ok(())
}
#[test]
fn concurrent_earlier_winner_with_different_nonce_and_time_remains_the_original() -> TestResult {
    let dir = f::directory("claim-app-earlier-winner")?;
    let mut port = ClaimApplicationFixture::new(&dir)?;
    port.fault = ClaimApplicationFault::Race;
    let (scope, id, caller) = (f::scope()?, id()?, caller()?);
    let R::Retained(outcome) = submit_public_claim_call(
        Request {
            scope: &scope,
            request: &id,
            caller: &caller,
        },
        &mut port,
        Limits::default(),
    )?
    else {
        return Err("earlier original winner".into());
    };
    assert_eq!(outcome.original_time().get(), 103);
    let own = wf_ports::log::prepare_public_claim_request(
        port.frames[0].as_bytes(),
        Limits::default().read.codec,
        ClaimCallerLimits::default(),
        32 * f::MIB,
    )?;
    assert_eq!(own.prepared().funding_batch(), Some([0x7a; 16]));
    let reply = wf_ports::log::inspect_public_claim_reply(
        outcome.response(),
        outcome.original_time(),
        Limits::default().reply.admission,
    )?;
    let paid = reply
        .claim()
        .ok_or("claim")?
        .payment()
        .ok_or("original payment")?;
    let winner = wf_ports::log::generate_public_credit_ids(
        3,
        wf_ports::log::CreditSpendPreparation {
            lease_epoch: 1,
            batch: [0x33; 16],
        },
        f::MIB,
    )?;
    assert_eq!(
        paid.reservation_ids().collect::<Result<Vec<_>, _>>()?,
        winner.iter().map(Vec::as_slice).collect::<Vec<_>>()
    );
    assert_eq!(port.log.state()?.last_log(), Some(f::at(3)));
    drop(own);
    drop(port);
    std::fs::remove_dir_all(dir)?;
    Ok(())
}
