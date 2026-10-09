//! One-shot acknowledged coordination for a concrete future adapter fault hook.
//!
//! An acknowledgement proves that the hook executed. It does not prove a disk
//! write, Raft commit, or process crash; those require a real adapter binding.

use std::{
    error::Error,
    fmt,
    sync::mpsc::{self, Receiver, RecvTimeoutError, SyncSender, TrySendError},
    time::Duration,
};

/// Maximum watchdog wait; this wall-time bound has no workflow authority.
pub const MAX_WAIT: Duration = Duration::from_secs(60);

/// Failed barrier coordination is a test failure, never a passing injection.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FaultError {
    /// Bounds must be greater than zero and at most [`MAX_WAIT`].
    InvalidBound,
    /// The hook was not acknowledged within the bound.
    HookTimeout,
    /// The hook was dropped before reaching the fault point.
    HookMissing,
    /// The controller disappeared or the paused hook is no longer available.
    Disconnected,
    /// The acknowledged hook was not released within the bound.
    ReleaseTimeout,
    /// The one-shot signal channel was unexpectedly already full.
    SignalFull,
}

impl fmt::Display for FaultError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::InvalidBound => "fault wait must be in (0, 60 seconds]",
            Self::HookTimeout => "fault hook was not reached before the bound",
            Self::HookMissing => "target ended without reaching the fault hook",
            Self::Disconnected => "fault controller or paused hook disconnected",
            Self::ReleaseTimeout => "acknowledged fault hook was not released before the bound",
            Self::SignalFull => "one-shot fault signal was already full",
        };
        f.write_str(message)
    }
}

impl Error for FaultError {}

/// The test controller can release the target only after an acknowledgement.
#[derive(Debug)]
pub struct FaultController {
    reached: Receiver<()>,
    release: SyncSender<()>,
}

/// A single owned hook placed at a specific adapter durability boundary.
#[derive(Debug)]
pub struct FaultHook {
    reached: SyncSender<()>,
    release: Receiver<()>,
}

/// Proof that this barrier's hook was reached; dropping it cancels the pause.
#[derive(Debug)]
pub struct ReachedFault {
    release: SyncSender<()>,
}

/// Create one isolated barrier with bounded single-message channels.
#[must_use]
pub fn fault_barrier() -> (FaultController, FaultHook) {
    let (reached_sender, reached_receiver) = mpsc::sync_channel(1);
    let (release_sender, release_receiver) = mpsc::sync_channel(1);
    (
        FaultController {
            reached: reached_receiver,
            release: release_sender,
        },
        FaultHook {
            reached: reached_sender,
            release: release_receiver,
        },
    )
}

fn check_bound(bound: Duration) -> Result<(), FaultError> {
    if bound.is_zero() || bound > MAX_WAIT {
        return Err(FaultError::InvalidBound);
    }
    Ok(())
}

fn send_signal(sender: &SyncSender<()>) -> Result<(), FaultError> {
    sender.try_send(()).map_err(|error| match error {
        TrySendError::Full(()) => FaultError::SignalFull,
        TrySendError::Disconnected(()) => FaultError::Disconnected,
    })
}

impl FaultController {
    /// Wait for proof that the target reached the hook.
    ///
    /// # Errors
    /// Rejects invalid bounds, missing hooks and unacknowledged timeout.
    pub fn wait_reached(self, bound: Duration) -> Result<ReachedFault, FaultError> {
        check_bound(bound)?;
        self.reached
            .recv_timeout(bound)
            .map_err(|error| match error {
                RecvTimeoutError::Timeout => FaultError::HookTimeout,
                RecvTimeoutError::Disconnected => FaultError::HookMissing,
            })?;
        Ok(ReachedFault {
            release: self.release,
        })
    }
}

impl FaultHook {
    /// Acknowledge the fault point, then pause for bounded release or cancellation.
    ///
    /// # Errors
    /// Rejects invalid bounds, a missing controller and unreleased timeout.
    pub fn reach(self, bound: Duration) -> Result<(), FaultError> {
        check_bound(bound)?;
        send_signal(&self.reached)?;
        self.release
            .recv_timeout(bound)
            .map_err(|error| match error {
                RecvTimeoutError::Timeout => FaultError::ReleaseTimeout,
                RecvTimeoutError::Disconnected => FaultError::Disconnected,
            })
    }
}

impl ReachedFault {
    /// Resume an acknowledged target once; ownership prevents duplicate release.
    ///
    /// # Errors
    /// Returns an error if the paused hook has ended or the channel is full.
    pub fn release(self) -> Result<(), FaultError> {
        send_signal(&self.release)
    }
}
