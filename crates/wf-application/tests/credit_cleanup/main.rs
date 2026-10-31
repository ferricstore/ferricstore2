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
    let limits = Limits {
        workspace_bytes: 1,
        ..Limits::default()
    };
    assert_eq!(
        run_credit_cleanup_tick(request, &mut port, Some(1), limits).err(),
        Some(E::Limit)
    );
    assert!(port.calls.borrow().is_empty());
    let limits = Limits {
        execution_workspace_bytes: usize::MAX,
        ..Limits::default()
    };
    assert_eq!(
        run_credit_cleanup_tick(request, &mut port, Some(1), limits).err(),
        Some(E::Limit)
    );
    assert!(port.calls.borrow().is_empty());
    Ok(())
}

#[test]
fn cleanup_workspace_cutoff_is_inclusive_and_refuses_before_effects_one_byte_below() -> TestResult {
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
    let mut low = 1;
    let mut high = Limits::default().workspace_bytes;
    while low < high {
        let mid = low + (high - low) / 2;
        port.calls.borrow_mut().clear();
        let limits = Limits {
            workspace_bytes: mid,
            ..Limits::default()
        };
        match run_credit_cleanup_tick(request, &mut port, Some(1), limits) {
            Ok(result) => {
                assert!(result.caught_up);
                high = mid;
            }
            Err(E::Limit) => {
                assert!(port.calls.borrow().is_empty());
                low = mid + 1;
            }
            Err(other) => return Err(other.into()),
        }
    }
    let limits = Limits {
        workspace_bytes: low,
        ..Limits::default()
    };
    assert!(run_credit_cleanup_tick(request, &mut port, Some(1), limits)?.caught_up);
    port.calls.borrow_mut().clear();
    let limits = Limits {
        workspace_bytes: low - 1,
        ..limits
    };
    assert_eq!(
        run_credit_cleanup_tick(request, &mut port, Some(1), limits).err(),
        Some(E::Limit)
    );
    assert!(port.calls.borrow().is_empty());
    Ok(())
}

#[test]
fn cleanup_empty_page_underpaid_owner_fee_cannot_claim_caught_up() -> TestResult {
    let mut port = BudgetFixture::new()?;
    let scope = port.scope.clone();
    let sources = [Source {
        scope: &scope,
        shard_count: 4,
    }];
    let limits = Limits {
        read: wf_ports::state::CreditCatalogReadLimits {
            maximum_bytes: 1,
            ..Limits::default().read
        },
        ..Limits::default()
    };
    let result = run_credit_cleanup_tick(
        Request {
            target: &scope,
            sources: &sources,
            catalog: b"limit",
            now_ms: 1006,
        },
        &mut port,
        Some(1),
        limits,
    )?;
    assert_eq!((result.commands, result.errors), (0, 8));
    assert!(!result.caught_up);
    Ok(())
}

#[test]
fn cleanup_missing_owner_default_freshness_refusal_never_prepares_prune() -> TestResult {
    let mut port = BudgetFixture::new()?;
    port.fault = BudgetFault::MissingOwner;
    let scope = port.scope.clone();
    let sources = [Source {
        scope: &scope,
        shard_count: 4,
    }];
    let step = run_credit_cleanup_once(
        Request {
            target: &scope,
            sources: &sources,
            catalog: b"limit",
            now_ms: 1006,
        },
        &mut port,
        Limits::default(),
    )?;
    assert_eq!(step.commands, 0);
    assert_eq!(
        step.reply.err(),
        Some(wf_application::governance::CreditCleanupFailure::Read(
            wf_ports::state::StateTransactionError::UnsupportedPolicyProfile,
        ))
    );
    assert!(
        !port
            .calls
            .borrow()
            .iter()
            .any(|call| matches!(call, BudgetCall::Prepare | BudgetCall::Submit))
    );
    Ok(())
}

#[test]
fn cleanup_member_revision_pair_must_be_coherent_before_owner_or_submit() -> TestResult {
    for fault in [BudgetFault::MissingRevision, BudgetFault::EmptyWithRevision] {
        let mut port = BudgetFixture::new()?;
        port.fault = fault;
        let scope = port.scope.clone();
        let sources = [Source {
            scope: &scope,
            shard_count: 4,
        }];
        let step = run_credit_cleanup_once(
            Request {
                target: &scope,
                sources: &sources,
                catalog: b"limit",
                now_ms: 1006,
            },
            &mut port,
            Limits::default(),
        )?;
        assert_eq!(step.commands, 0);
        assert_eq!(
            step.reply.err(),
            Some(wf_application::governance::CreditCleanupFailure::InvalidObservation)
        );
        assert_eq!(
            *port.calls.borrow(),
            vec![BudgetCall::ReadProgress, BudgetCall::Page]
        );
    }
    Ok(())
}
