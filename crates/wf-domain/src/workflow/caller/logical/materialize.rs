//! Fallible scalar ownership and direct moves of complete stored value buffers.

use wf_types::identity::RecordId;
use wf_types::version::{CommittedTime, Fence, TimestampMillis};

use crate::lease::{ExtendLease, LeaseGuard, LeaseToken};
use crate::policy::RetryOverride;
use crate::retry::{JitterSample, PolicyRetry};
use crate::workflow::caller::logical::budget::Budget;
use crate::workflow::caller::logical::{LogicalMutationError as E, PreparedLogicalMutation};
use crate::workflow::caller::{
    CoreMutation as C, MutationCallerError, NormalizedMutation, PublicMutationEvidence,
};
use crate::workflow::{Cancel, CommittedCommand, Create, Operation, Terminal, Transition};

pub(super) fn finish(
    caller: &NormalizedMutation<'_>,
    values: [Option<Vec<u8>>; 2],
    stamp: CommittedTime,
    budget: &Budget,
) -> Result<PreparedLogicalMutation, E> {
    let priority = if let C::Create { priority, .. } = caller.operation() {
        *priority
    } else {
        0
    };
    let operation = operation(caller.operation(), values, budget)?;
    let id =
        RecordId::new(budget.copy(caller.id())?).map_err(|_| MutationCallerError::InvalidId)?;
    let partition = caller
        .partition()
        .map(|bytes| budget.copy(bytes))
        .transpose()?;
    Ok(PreparedLogicalMutation {
        command: CommittedCommand {
            id,
            now: TimestampMillis::new(
                caller
                    .requested_now_ms()
                    .unwrap_or(stamp.hlc().physical().get()),
            ),
            operation,
        },
        partition,
        priority,
        evidence: PublicMutationEvidence {
            requested_now_ms: caller.requested_now_ms(),
            auto_partition: caller.partition().is_none(),
            retry_override_present: matches!(caller.operation(), C::Retry { retry: Some(_), .. }),
        },
        work: budget.work,
        temporary: budget.peak,
    })
}

fn operation(
    caller: &C<'_>,
    values: [Option<Vec<u8>>; 2],
    budget: &Budget,
) -> Result<Operation, E> {
    let [payload, value] = values;
    Ok(match caller {
        C::Create {
            workflow_type,
            state,
            run_at_ms,
            ..
        } => Operation::Create(Create {
            workflow_type: budget.copy(workflow_type)?,
            state: budget.copy(state)?,
            run_at: run_at_ms.map(TimestampMillis::new),
            payload,
        }),
        C::Transition {
            from,
            to,
            fence,
            lease_token,
            run_at_ms,
            ..
        } => Operation::Transition(Transition {
            from: budget.copy(from)?,
            to: budget.copy(to)?,
            fence: Fence::new(*fence),
            lease_token: lease_token.map(|bytes| token(bytes, budget)).transpose()?,
            run_at: run_at_ms.map(TimestampMillis::new),
            payload,
        }),
        C::Complete {
            lease_token, fence, ..
        }
        | C::Fail {
            lease_token, fence, ..
        } => {
            let terminal = Terminal {
                guard: guard(lease_token, *fence, budget)?,
                payload,
                value,
            };
            if matches!(caller, C::Complete { .. }) {
                Operation::Complete(terminal)
            } else {
                Operation::Fail(terminal)
            }
        }
        C::Retry {
            lease_token,
            fence,
            run_at_ms,
            retry,
            ..
        } => Operation::RetryWithPolicy(PolicyRetry {
            guard: guard(lease_token, *fence, budget)?,
            run_at: run_at_ms.map(TimestampMillis::new),
            policy_override: owned_retry(*retry, budget)?,
            jitter: JitterSample::new(0),
            payload,
            error: value,
        }),
        C::ExtendLease {
            lease_token,
            fence,
            lease_ms,
        } => Operation::ExtendLease(ExtendLease {
            guard: guard(lease_token, *fence, budget)?,
            lease_ms: *lease_ms,
        }),
        C::Cancel {
            fence,
            lease_token,
            expected_state,
            expected_version,
            ..
        } => Operation::Cancel(Cancel {
            fence: Fence::new(*fence),
            lease_token: lease_token.map(|bytes| token(bytes, budget)).transpose()?,
            expected_state: expected_state.map(|bytes| budget.copy(bytes)).transpose()?,
            expected_version: *expected_version,
            error: value,
        }),
    })
}

fn token(bytes: &[u8], budget: &Budget) -> Result<LeaseToken, E> {
    LeaseToken::new(budget.copy(bytes)?)
        .map_err(|_| MutationCallerError::InvalidField("lease_token").into())
}

fn guard(bytes: &[u8], fence: u64, budget: &Budget) -> Result<LeaseGuard, E> {
    Ok(LeaseGuard {
        token: token(bytes, budget)?,
        fence: Fence::new(fence),
    })
}

fn owned_retry(value: Option<&RetryOverride>, budget: &Budget) -> Result<RetryOverride, E> {
    let Some(value) = value else {
        return Ok(RetryOverride::default());
    };
    Ok(RetryOverride {
        max_retries: value.max_retries,
        backoff: value.backoff,
        exhausted_to: value
            .exhausted_to
            .as_ref()
            .map(|bytes| budget.copy(bytes))
            .transpose()?,
    })
}
