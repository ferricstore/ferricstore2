//! Shared real producers with the original no-snapshot metadata oracle.

use super::TestResult;
use std::path::Path;

mod shared;
use shared::bytes_table;
pub(super) use shared::{
    MIB, Rows, append, applied, apply_control, apply_normal, at, blank, cancel_op, clock,
    create_op, create_pair, create_pair_for, directory, driver_limits, frame, frame_for, hex,
    log_meta, log_rows, member, open_pair, open_pair_for, scope, scope_for, state_rows,
};

pub(super) fn state_meta(dir: &Path) -> TestResult<Rows> {
    let rows = bytes_table(&dir.join("state.redb"), "wf_state_meta_v1")?;
    assert_eq!(rows.len(), 3);
    assert!(rows.contains_key(b"manifest".as_slice()));
    assert!(rows.contains_key(b"membership".as_slice()));
    let descriptor = rows
        .get(b"snapshot".as_slice())
        .ok_or("missing snapshot descriptor")?;
    assert_eq!(descriptor.len(), 39);
    assert_eq!(&descriptor[..7], b"FWSD\0\x01\0");
    for name in ["wf_state_snapshot_meta_v1", "wf_state_snapshot_rows_v1"] {
        assert_eq!(bytes_table(&dir.join("state.redb"), name)?.len(), 0);
    }
    Ok(rows)
}
