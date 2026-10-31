//! Boundary and preparer contracts supplement real committed Node scenarios.
#![forbid(unsafe_code)]
use wf_application::governance::{
    CreditCleanupError as E, CreditCleanupLimits as Limits, CreditCleanupRequest as Request,
    CreditCleanupSource as Source, run_credit_cleanup_once, run_credit_cleanup_tick,
};
use wf_ports::consensus::{CreditCatalogPublicationPort, MutationNotSubmitted};
type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;
mod fixtures;
use fixtures::{BudgetCall, BudgetFault, BudgetFixture};

#[test]
fn cleanup_default_preparer_preserves_original_identity_only_at_its_trusted_time_floor()
-> TestResult {
    let mut port = BudgetFixture::new()?;
    let scope = port.scope.clone();
    let (id, stamp) = port
        .prepare_catalog_command_at(&scope, 1006)
        .map_err(|e| format!("{e:?}"))?;
    assert_eq!(id.as_bytes(), b"prepare-1");
    assert_eq!(stamp.wall().get(), 1006);
    assert_eq!(
        port.prepare_catalog_command_at(&scope, 1007).err(),
        Some(MutationNotSubmitted::Preparation)
    );
    assert_eq!(
        *port.calls.borrow(),
        vec![BudgetCall::Prepare, BudgetCall::Prepare]
    );
    Ok(())
}
#[test]
fn cleanup_default_budget_and_run_once_empty_catalog_do_not_prepare_or_submit() -> TestResult {
    let mut port = BudgetFixture::new()?;
    let scope = port.scope.clone();
    let sources = [Source {
        scope: &scope,
        shard_count: 4,
    }];
    let request = Request {
        target: &scope,
        sources: &sources,
        catalog: b"limit",
        now_ms: 1006,
    };
    let result = run_credit_cleanup_tick(request, &mut port, None, Limits::default())?;
    assert_eq!((result.commands, result.deleted, result.errors), (0, 0, 0));
    assert!(result.caught_up);
    assert_eq!(
        *port.calls.borrow(),
        vec![BudgetCall::ReadProgress, BudgetCall::Page]
    );
    port.calls.borrow_mut().clear();
    let step = run_credit_cleanup_once(request, &mut port, Limits::default())?;
    assert_eq!(step.commands, 0);
    assert!(step.wrapped);
    assert_eq!(step.reply?.next_cursor, None);
    assert_eq!(
        *port.calls.borrow(),
        vec![BudgetCall::ReadProgress, BudgetCall::Page]
    );
    Ok(())
}
#[test]
fn cleanup_zero_command_errors_and_malformed_pages_use_the_exact_attempt_bound() -> TestResult {
    for fault in [BudgetFault::UnavailableProgress, BudgetFault::MalformedPage] {
        let mut port = BudgetFixture::new()?;
        port.fault = fault;
        let scope = port.scope.clone();
        let sources = [Source {
            scope: &scope,
            shard_count: 4,
        }];
        let result = run_credit_cleanup_tick(
            Request {
                target: &scope,
                sources: &sources,
                catalog: b"limit",
                now_ms: 1006,
            },
            &mut port,
            Some(2),
            Limits::default(),
        )?;
        assert_eq!((result.commands, result.errors), (0, 12));
        assert!(!result.caught_up);
        assert_eq!(
            port.calls
                .borrow()
                .iter()
                .filter(|c| **c == BudgetCall::ReadProgress)
                .count(),
            12
        );
    }
    Ok(())
}
#[test]
fn cleanup_workspace_has_an_exact_pre_effect_boundary_and_checked_overflow() -> TestResult {
    let mut port = BudgetFixture::new()?;
    let scope = port.scope.clone();
    let sources = [Source {
        scope: &scope,
        shard_count: 4,
    }];
    let request = Request {
        target: &scope,
        sources: &sources,
        catalog: b"limit",
        now_ms: 1006,
    };
    let mut limits = Limits::default();
    limits.workspace_bytes = 1;
    assert_eq!(
        run_credit_cleanup_tick(request, &mut port, Some(1), limits).err(),
        Some(E::Limit)
    );
    assert!(port.calls.borrow().is_empty());
    limits = Limits::default();
    limits.execution_workspace_bytes = usize::MAX;
    assert_eq!(
        run_credit_cleanup_tick(request, &mut port, Some(1), limits).err(),
        Some(E::Limit)
    );
    assert!(port.calls.borrow().is_empty());
    Ok(())
}
