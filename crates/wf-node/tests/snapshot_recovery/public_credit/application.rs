//! Public caller sequencing through actual committed Node/state operations.
use super::{TestResult, f};
use wf_application::governance::{
    PublicCreditCallError as E, PublicCreditCallLimits as Limits,
    PublicCreditCallRequest as Request, PublicCreditCallResult as R, retry_public_credit_call,
    submit_public_credit_call,
};
use wf_domain::governance::caller::{
    CreditCallKind as K, CreditOptionValue as V, normalize_credit_call,
};
mod fixtures;
mod ports;
use fixtures::{
    PublicCreditFault as Fault, PublicCreditFixture as Fixture, PublicCreditTrace as Trace,
};
#[test]
fn public_credit_application_commits_owner_before_synchronous_catalog_registration() -> TestResult {
    let mut port = Fixture::new()?;
    let opts = [
        ("shard_id", V::Integer(0)),
        ("amount", V::Integer(2)),
        ("limit", V::Integer(2)),
        ("ttl_ms", V::Integer(1000)),
    ];
    let result = port.call(K::Lease, b"new-owner", &opts)??;
    assert!(matches!(result, R::Retained(_)));
    assert!(port.owner()?.is_some());
    assert!(port.member()?);
    let trace = port.trace.borrow();
    let credit = trace
        .iter()
        .position(|v| *v == Trace::Submit)
        .ok_or("no credit")?;
    let registration = trace
        .iter()
        .position(|v| *v == Trace::Register(true))
        .ok_or("no actual registered owner")?;
    assert!(credit < registration);
    drop(trace);
    port.cleanup()
}
#[test]
fn public_credit_application_repairs_uncataloged_owner_before_spend_and_entropy() -> TestResult {
    let mut port = Fixture::new()?;
    port.seed()?;
    port.now = 1001;
    assert!(!port.member()?);
    let result = port.call(
        K::Spend,
        b"spend",
        &[("shard_id", V::Integer(0)), ("amount", V::Integer(1))],
    )??;
    assert!(matches!(result, R::Retained(_)));
    assert!(port.member()?);
    assert_eq!(port.in_use()?, 1);
    let trace = port.trace.borrow();
    let registration = trace
        .iter()
        .position(|v| *v == Trace::Register(true))
        .ok_or("no registration")?;
    let entropy = trace
        .iter()
        .position(|v| *v == Trace::Entropy)
        .ok_or("no entropy")?;
    let credit = trace
        .iter()
        .position(|v| *v == Trace::Submit)
        .ok_or("no spend")?;
    assert!(registration < entropy && entropy < credit);
    drop(trace);
    port.cleanup()
}
#[test]
fn public_credit_application_unknown_or_refused_registration_prevents_dependent_spend() -> TestResult
{
    for fault in [Fault::RefuseRegister, Fault::UnknownRegister] {
        let mut port = Fixture::new()?;
        port.seed()?;
        port.now = 1001;
        port.fault.set(fault);
        let result = port.call(
            K::Spend,
            b"spend",
            &[("shard_id", V::Integer(0)), ("amount", V::Integer(1))],
        )?;
        assert!(matches!(result, Err(E::Repair(_))));
        assert_eq!(port.in_use()?, 0);
        assert!(
            !port
                .trace
                .borrow()
                .iter()
                .any(|v| matches!(v, Trace::Prepare | Trace::Entropy | Trace::Submit))
        );
        port.cleanup()?;
    }
    Ok(())
}
#[test]
fn public_credit_application_missing_spend_get_and_cleanup_never_register_ghosts() -> TestResult {
    let mut port = Fixture::new()?;
    assert!(matches!(
        port.call(
            K::Spend,
            b"spend",
            &[("shard_id", V::Integer(0)), ("amount", V::Integer(1))]
        )?,
        Err(E::NotFound)
    ));
    assert!(matches!(port.call(K::Get, b"get", &[])??, R::Missing));
    assert!(matches!(
        port.call(K::Cleanup, b"cleanup", &[])??,
        R::EmptyCleanup
    ));
    assert!(!port.member()?);
    assert!(port.owner()?.is_none());
    assert!(
        !port
            .trace
            .borrow()
            .iter()
            .any(|v| matches!(v, Trace::Maintenance | Trace::Prepare | Trace::Submit))
    );
    port.cleanup()
}
#[test]
fn public_credit_application_lost_spend_preserves_original_ids_and_retry_frame() -> TestResult {
    let mut port = Fixture::new()?;
    port.seed()?;
    port.now = 1001;
    port.fault.set(Fault::LosePublic);
    let opts = [("shard_id", V::Integer(0)), ("amount", V::Integer(2))];
    let R::Unknown { original, .. } = port.call(K::Spend, b"spend", &opts)?? else {
        return Err("no original unknown context".into());
    };
    let view = original.unknown_spend().ok_or("no compensation IDs")?;
    assert_eq!(view.code, "FLOW_LIMIT_SPEND_UNKNOWN_OUTCOME");
    assert_eq!(view.scope, b"global");
    assert_eq!(view.shard_id, 0);
    assert_eq!(view.reservation_ids.len(), 2);
    let ids = view.reservation_ids.to_vec();
    assert_eq!(port.in_use()?, 2);
    port.trace.borrow_mut().clear();
    port.fault.set(Fault::OwnerRead);
    let R::Retained(outcome) = port.call(K::Spend, b"spend", &opts)?? else {
        return Err("no lookup original".into());
    };
    assert_eq!(fixtures::decoded(&outcome)?.reservation_ids, ids);
    assert_eq!(*port.trace.borrow(), vec![Trace::Lookup]);
    port.trace.borrow_mut().clear();
    let R::Retained(retried) = retry_public_credit_call(original, &mut port, Limits::default())?
    else {
        return Err("no original retry".into());
    };
    assert_eq!(retried, outcome);
    assert_eq!(*port.trace.borrow(), vec![Trace::Submit]);
    assert!(std::sync::Arc::ptr_eq(
        &port.frames[0],
        port.frames.last().ok_or("no retried frame")?
    ));
    port.cleanup()
}
#[test]
fn public_credit_application_wrong_original_id_order_is_unknown_and_safely_retryable() -> TestResult
{
    let mut port = Fixture::new()?;
    port.seed()?;
    port.now = 1001;
    port.fault.set(Fault::WrongPublicIds);
    let R::Unknown { reason, original } = port.call(
        K::Spend,
        b"spend",
        &[("shard_id", V::Integer(0)), ("amount", V::Integer(2))],
    )??
    else {
        return Err("wrong IDs accepted".into());
    };
    assert_eq!(reason, wf_ports::consensus::MutationUnknown::InvalidReply);
    assert_eq!(port.in_use()?, 2);
    let ids = original.spend_ids().to_vec();
    let R::Retained(outcome) = retry_public_credit_call(original, &mut port, Limits::default())?
    else {
        return Err("no retry original".into());
    };
    assert_eq!(fixtures::decoded(&outcome)?.reservation_ids, ids);
    assert_eq!(port.in_use()?, 2);
    port.cleanup()
}
#[test]
fn public_credit_application_publication_failure_keeps_committed_owner_and_repairable_outbox()
-> TestResult {
    let mut port = Fixture::new()?;
    port.fault.set(Fault::RefuseRegister);
    let opts = [
        ("shard_id", V::Integer(0)),
        ("amount", V::Integer(2)),
        ("limit", V::Integer(2)),
        ("ttl_ms", V::Integer(1000)),
    ];
    assert!(matches!(
        port.call(K::Lease, b"lease", &opts)??,
        R::PublicationUnconfirmed { .. }
    ));
    assert!(port.owner()?.is_some());
    assert!(!port.member()?);
    assert_eq!(port.pending()?, 1);
    let source = port.scope.clone();
    let stats = wf_application::governance::publish_credit_catalog(
        wf_application::governance::CreditCatalogPublicationRequest {
            sources: std::slice::from_ref(&source),
            source_cursor: 0,
            target: &source,
            catalog: b"limit",
        },
        &mut port,
        wf_application::governance::CreditCatalogPublicationLimits::default(),
    )?;
    assert_eq!((stats.published, stats.errors), (1, 0));
    assert!(port.member()?);
    assert_eq!(port.pending()?, 0);
    port.cleanup()
}
#[test]
fn public_credit_application_unknown_lookup_and_bad_workspace_stop_before_owner_reads() -> TestResult
{
    let mut port = Fixture::new()?;
    port.fault.set(Fault::LookupUnknown);
    assert!(matches!(
        port.call(
            K::Spend,
            b"spend",
            &[("shard_id", V::Integer(0)), ("amount", V::Integer(1))]
        )??,
        R::Unresolved(_)
    ));
    assert_eq!(*port.trace.borrow(), vec![Trace::Lookup]);
    port.trace.borrow_mut().clear();
    let scope = port.scope.clone();
    let id = wf_types::identity::RequestId::new(b"get".to_vec())?;
    let caller = normalize_credit_call(K::Get, b"global", 4, &[], 65535)?;
    let result = submit_public_credit_call(
        Request {
            source: &scope,
            target: &scope,
            catalog: b"limit",
            request: &id,
            caller,
        },
        &mut port,
        Limits {
            workspace_bytes: 1,
            ..Limits::default()
        },
    );
    assert!(matches!(result, Err(E::Limit)));
    assert!(port.trace.borrow().is_empty());
    port.cleanup()
}

#[test]
fn public_credit_application_retry_refusals_keep_original_and_exact_id_budget_is_sufficient()
-> TestResult {
    let mut port = Fixture::new()?;
    port.seed()?;
    port.now = 1001;
    port.fault.set(Fault::LosePublic);
    let R::Unknown { original, .. } = port.call(
        K::Spend,
        b"spend",
        &[("shard_id", V::Integer(0)), ("amount", V::Integer(2))],
    )??
    else {
        return Err("no original context".into());
    };
    let frame = original.frame().as_ptr();
    let ids = original.spend_ids().to_vec();
    port.trace.borrow_mut().clear();
    let refusal = match retry_public_credit_call(
        original,
        &mut port,
        Limits {
            workspace_bytes: 1,
            ..Limits::default()
        },
    ) {
        Err(error) => error,
        Ok(_) => return Err("bad workspace admitted".into()),
    };
    assert_eq!(refusal.error(), E::Limit);
    assert_eq!(port.trace.borrow().len(), 0);
    let original = refusal.into_original();
    assert_eq!(original.frame().as_ptr(), frame);
    assert_eq!(original.spend_ids(), ids);
    port.fault.set(Fault::RefusePublic);
    let refusal = match retry_public_credit_call(original, &mut port, Limits::default()) {
        Err(error) => error,
        Ok(_) => return Err("retry proposal refusal lost".into()),
    };
    assert_eq!(
        refusal.error(),
        E::Preparation(wf_ports::consensus::MutationNotSubmitted::Unavailable)
    );
    let original = refusal.into_original();
    assert_eq!(original.frame().as_ptr(), frame);
    assert_eq!(original.spend_ids(), ids);
    let maximum_ids_bytes = 128
        + ids
            .iter()
            .map(|id| id.len() + std::mem::size_of::<Vec<u8>>())
            .sum::<usize>();
    let R::Retained(outcome) = retry_public_credit_call(
        original,
        &mut port,
        Limits {
            maximum_ids_bytes,
            ..Limits::default()
        },
    )?
    else {
        return Err("exact original ID budget refused".into());
    };
    assert_eq!(fixtures::decoded(&outcome)?.reservation_ids, ids);
    assert_eq!(port.in_use()?, 2);
    port.cleanup()
}

#[test]
fn public_credit_application_repairs_before_existing_get_renew_release_and_cleanup() -> TestResult {
    use wf_domain::governance::caller::CreditIdList;
    for kind in [K::Get, K::Renew, K::Release, K::Cleanup] {
        let mut port = Fixture::new()?;
        port.seed()?;
        let frames = super::fixtures::frames()?;
        let outcome = port.commit(
            &wf_types::identity::RequestId::new(b"spend".to_vec())?,
            &frames[1],
        )?;
        let ids = fixtures::decoded(&outcome)?.reservation_ids;
        port.now = 1002;
        assert!(!port.member()?);
        let opts = match kind {
            K::Renew => vec![("shard_id", V::Integer(0)), ("ttl_ms", V::Integer(2000))],
            K::Release => vec![
                ("shard_id", V::Integer(0)),
                ("amount", V::Integer(1)),
                ("reservation_ids", V::Ids(CreditIdList::Owned(&ids[..1]))),
            ],
            _ => Vec::new(),
        };
        let R::Retained(outcome) = port.call(kind, b"dependent", &opts)?? else {
            return Err("no original dependent response".into());
        };
        assert_eq!(
            outcome.status(),
            wf_domain::workflow::request::RetainedStatus::Accepted
        );
        assert!(port.member()?);
        assert_eq!(port.in_use()?, if kind == K::Release { 1 } else { 2 });
        let trace = port.trace.borrow();
        let registration = trace
            .iter()
            .position(|v| *v == Trace::Register(true))
            .ok_or("no registration")?;
        let submitted = trace
            .iter()
            .position(|v| *v == Trace::Submit)
            .ok_or("no submission")?;
        assert!(registration < submitted);
        assert!(!trace.contains(&Trace::Entropy));
        assert_eq!(trace.iter().filter(|v| **v == Trace::Owner).count(), 1);
        drop(trace);
        port.cleanup()?;
    }
    Ok(())
}

#[test]
fn public_credit_application_unknown_spend_compensation_preserves_sibling_reservations()
-> TestResult {
    use wf_domain::governance::caller::CreditIdList;
    let mut port = Fixture::new()?;
    port.seed()?;
    port.now = 1001;
    port.fault.set(Fault::LosePublic);
    let opts = [("shard_id", V::Integer(0)), ("amount", V::Integer(1))];
    let R::Unknown { original, .. } = port.call(K::Spend, b"unknown", &opts)?? else {
        return Err("no uncertain spend".into());
    };
    let ids = original
        .unknown_spend()
        .ok_or("no original IDs")?
        .reservation_ids
        .to_vec();
    let R::Retained(sibling) = port.call(K::Spend, b"sibling", &opts)?? else {
        return Err("no sibling".into());
    };
    let sibling_ids = fixtures::decoded(&sibling)?.reservation_ids;
    assert_ne!(ids, sibling_ids);
    assert_eq!(port.in_use()?, 2);
    let release = [
        ("shard_id", V::Integer(0)),
        ("amount", V::Integer(1)),
        ("reservation_ids", V::Ids(CreditIdList::Owned(&ids))),
    ];
    for request in [b"compensate".as_slice(), b"repeat-compensation"] {
        let R::Retained(outcome) = port.call(K::Release, request, &release)?? else {
            return Err("no compensation outcome".into());
        };
        assert_eq!(
            outcome.status(),
            wf_domain::workflow::request::RetainedStatus::Accepted
        );
        assert_eq!(port.in_use()?, 1);
    }
    let R::Retained(retained) = port.call(K::Spend, b"sibling", &opts)?? else {
        return Err("no original sibling".into());
    };
    assert_eq!(fixtures::decoded(&retained)?.reservation_ids, sibling_ids);
    assert_eq!(port.in_use()?, 1);
    port.cleanup()
}
