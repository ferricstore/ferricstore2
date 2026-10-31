//! Reload durable hints per step and move exact cursor/member owners.
use super::{
    CreditCleanupFailure as F, CreditCleanupLimits as L, CreditCleanupReply as Reply,
    CreditCleanupRequest as R, CreditCleanupStep as Step, dispatch, plan,
};
use wf_ports::{
    consensus::CreditCatalogPublicationPort,
    state::{
        CreditOwnerReadPort, GovernanceCatalogProgress, GovernanceCatalogProgressPort,
        GovernanceCatalogPruneReadPort, GovernanceCatalogRead, GovernanceCatalogReadPort,
        StateTransactionError as S,
    },
};

struct CleanupTraversal {
    cursor: Option<Vec<u8>>,
    step: Step,
}

pub(super) fn run<P>(
    port: &mut P,
    request: R<'_>,
    limits: L,
    budget: u16,
    streak: Option<Vec<u8>>,
    streak_pages: u8,
    burst: u8,
) -> Step
where
    P: CreditCatalogPublicationPort
        + CreditOwnerReadPort
        + GovernanceCatalogReadPort
        + GovernanceCatalogPruneReadPort
        + GovernanceCatalogProgressPort,
{
    let mut state = CleanupTraversal {
        cursor: None,
        step: Step {
            reply: Ok(Reply {
                deleted: 0,
                pending: false,
                pruned: false,
                next_cursor: None,
            }),
            commands: 0,
            deleted: 0,
            errors: 0,
            wrapped: false,
            streak,
            streak_pages,
        },
    };
    match port.governance_catalog_progress(request.target, request.catalog, plan::read(limits)) {
        Ok(GovernanceCatalogProgress::Missing) => (),
        Ok(GovernanceCatalogProgress::Cursor(cursor)) => {
            if cursor.len() > 65535
                || cursor
                    .len()
                    .checked_add(152)
                    .is_none_or(|n| n > limits.read.maximum_bytes)
            {
                return fail(state.step, F::InvalidObservation);
            }
            state.cursor = Some(cursor);
        }
        Ok(GovernanceCatalogProgress::Invalid) => {
            state.step.commands += 1;
            if !dispatch::progress(port, request, None, limits) {
                return fail(state.step, F::ProgressUnconfirmed);
            }
        }
        Err(error) => return fail(state.step, F::Read(error)),
    }
    if state.step.commands == budget {
        return state.step;
    }
    let candidate = match port.governance_catalog_prune_page(
        GovernanceCatalogRead {
            scope: request.target,
            catalog: request.catalog,
            cursor: state.cursor.as_deref(),
            limit: 1,
        },
        plan::read(limits),
    ) {
        Ok(page) if plan::valid_candidate(&page, state.cursor.as_deref(), limits) => page,
        Ok(_) => return fail(state.step, F::InvalidObservation),
        Err(S::CatalogChanged) => return finish(port, request, limits, budget, state, None, true),
        Err(error) => return fail(state.step, F::Read(error)),
    };
    let mut page = candidate.page;
    let Some(member) = page.members.pop() else {
        return finish(port, request, limits, budget, state, None, true);
    };
    let Some(revision) = candidate.revision else {
        return fail(state.step, F::InvalidObservation);
    };
    state.step.reply = work(
        port,
        request,
        limits,
        &member,
        revision,
        &mut state.step.commands,
    );
    state.step.errors = u16::from(state.step.reply.is_err());
    if let Ok(reply) = &state.step.reply {
        state.step.deleted = reply.deleted;
    }
    let pending = state.step.reply.as_ref().is_ok_and(|reply| reply.pending);
    let pages = if state.step.streak.as_deref() == Some(member.as_slice()) {
        state.step.streak_pages + 1
    } else {
        1
    };
    if pending && pages < burst {
        state.step.streak = Some(member);
        state.step.streak_pages = pages;
        if let Ok(reply) = &mut state.step.reply {
            reply.next_cursor = state.cursor;
        }
        return state.step;
    }
    state.step.streak = None;
    state.step.streak_pages = 0;
    let target = if page.full && keep_cursor(port, request, limits, &member) {
        Some(member)
    } else {
        None
    };
    let wrapped = target.is_none();
    finish(port, request, limits, budget, state, target, wrapped)
}
fn work<P>(
    port: &mut P,
    request: R<'_>,
    limits: L,
    member: &[u8],
    revision: wf_domain::workflow::control::EntryIdentity,
    commands: &mut u16,
) -> Result<Reply, F>
where
    P: CreditCatalogPublicationPort + CreditOwnerReadPort,
{
    let (source, name) = plan::source(request, member)?;
    let owner = port
        .credit_catalog_owner(source.scope, name, limits.read)
        .map_err(F::Read)?;
    let owner = match owner {
        Some(owner) => Some(owner),
        None => port
            .credit_catalog_owner_for_prune(source.scope, name, limits.read)
            .map_err(F::Read)?,
    };
    let Some(owner) = owner else {
        *commands += 1;
        let pruned =
            dispatch::prune(port, request, member, revision, limits).ok_or(F::PruneUnconfirmed)?;
        return Ok(Reply {
            deleted: 0,
            pending: false,
            pruned,
            next_cursor: None,
        });
    };
    let view = owner.view();
    if view.scope != name || view.leases.len() > limits.read.credit.max_leases() {
        return Err(F::InvalidObservation);
    }
    let due = view.cleanup_head <= view.cleanup_tail
        || view
            .leases
            .iter()
            .any(|lease| lease.expires_at_ms <= request.now_ms && lease.reservation_page > 0);
    drop(owner);
    if !due {
        return Ok(Reply {
            deleted: 0,
            pending: false,
            pruned: false,
            next_cursor: None,
        });
    }
    *commands += 1;
    let (deleted, pending) =
        dispatch::cleanup(port, request, source, name, limits).ok_or(F::CleanupUnconfirmed)?;
    Ok(Reply {
        deleted,
        pending,
        pruned: false,
        next_cursor: None,
    })
}
fn keep_cursor<P: GovernanceCatalogReadPort>(
    port: &P,
    request: R<'_>,
    limits: L,
    member: &[u8],
) -> bool {
    match port.governance_catalog_page(
        GovernanceCatalogRead {
            scope: request.target,
            catalog: request.catalog,
            cursor: Some(member),
            limit: 1,
        },
        plan::read(limits),
    ) {
        Ok(page) if plan::valid_page(&page, Some(member), limits) => !page.members.is_empty(),
        Err(S::CatalogChanged) => false,
        _ => true,
    }
}
fn finish<P: CreditCatalogPublicationPort>(
    port: &mut P,
    request: R<'_>,
    limits: L,
    budget: u16,
    mut state: CleanupTraversal,
    target: Option<Vec<u8>>,
    wrapped: bool,
) -> Step {
    let mut reached = state.cursor == target;
    if !reached && state.step.commands < budget {
        state.step.commands += 1;
        if !dispatch::progress(port, request, target.as_deref(), limits) {
            return fail(state.step, F::ProgressUnconfirmed);
        }
        state.cursor = target;
        reached = true;
    }
    state.step.wrapped = wrapped && reached;
    if let Ok(reply) = &mut state.step.reply {
        reply.next_cursor = state.cursor;
    }
    state.step
}
fn fail(mut step: Step, error: F) -> Step {
    step.reply = Err(error);
    step.errors = 1;
    step.wrapped = false;
    step
}
