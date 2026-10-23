//! Committed startup recovery on separate real databases and complete cohorts.

#![forbid(unsafe_code)]

mod atomic_start;
mod claim;
mod core_policy;
mod faults;
mod fixtures;
mod history;
mod limits;
mod original_policy;
mod replay;

type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;

use wf_node::recovery::{CommittedRecoveryError, CommittedRecoveryLimits, recover_committed};
use wf_ports::consensus::RaftMembershipLimits;

fn limits() -> TestResult<CommittedRecoveryLimits> {
    Ok(CommittedRecoveryLimits::new(
        RaftMembershipLimits::default(),
        (
            10_000,
            64 * fixtures::MIB,
            64 * fixtures::MIB,
            32 * fixtures::MIB,
        ),
        fixtures::driver_limits(),
        (1_000, 64 * fixtures::MIB),
        16 * fixtures::MIB,
        512 * fixtures::MIB,
    )?)
}

fn assert_refusal(
    dir: &std::path::Path,
    cap: CommittedRecoveryLimits,
    expected: CommittedRecoveryError,
) -> TestResult {
    let rows = fixtures::state_rows(dir)?;
    let meta = fixtures::state_meta(dir)?;
    let logs = fixtures::log_rows(dir)?;
    let log_meta = fixtures::log_meta(dir)?;
    let (log, state) = fixtures::open_pair(dir)?;
    let s = fixtures::scope()?;
    let Err(error) = recover_committed(log, state, s.group(), s.recovery(), cap) else {
        return Err("recovery accepted inconsistent authority".into());
    };
    assert_eq!(error, expected);
    // The consuming error must have released BOTH exclusive database owners.
    drop(fixtures::open_pair(dir)?);
    assert_eq!(fixtures::state_rows(dir)?, rows);
    assert_eq!(fixtures::state_meta(dir)?, meta);
    assert_eq!(fixtures::log_rows(dir)?, logs);
    assert_eq!(fixtures::log_meta(dir)?, log_meta);
    Ok(())
}
