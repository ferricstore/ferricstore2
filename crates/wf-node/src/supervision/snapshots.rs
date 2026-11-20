//! The snapshot thread (ADR-013): on a fixed interval it runs the node's
//! snapshot check, which takes a snapshot when one is due.

use std::sync::mpsc::{Receiver, RecvTimeoutError};
use std::time::Duration;

/// Calls `tick` every `interval` until `stop` is closed or sent to.
pub(crate) fn run(stop: &Receiver<()>, interval: Duration, mut tick: impl FnMut()) {
    while let Err(RecvTimeoutError::Timeout) = stop.recv_timeout(interval) {
        tick();
    }
}
