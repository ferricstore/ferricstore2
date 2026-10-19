//! Independent physical encodings and observations; no production seed capability.

use redb::{Durability, ReadableDatabase, ReadableTable};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::path::Path;
use wf_domain::workflow::control::{ControlState, EntryIdentity, EntryStamp, plan_control};
use wf_ports::consensus::{RaftMembership, RaftMembershipLimits};
use wf_ports::log::{PreparedBytes, encode_raft_blank, encode_raft_membership};
use wf_ports::state::{
    GroupApplyStore, GroupApplyTransaction, GroupControlChanges, GroupControlTransaction,
};
use wf_storage::state::{RedbStateStore, key, value};
use wf_types::version::{CommittedTime, HlcTimestamp, LogIdentity};

use super::{TestResult, f};

pub(super) const MEMBER_KEY: &[u8] = b"membership";
pub(super) const MEMBER: &str = "46574d42000100016700017a000000000300000002000000000000000100000000000000020000000200000000000000020000000000000003000000020000000000000001000000000000000200000004000000000000000100000000000000020000000000000003000000000000000400000001610000000000000002780000000002ceb2";
pub(super) const NONE: &str =
    "46574d56000100016700017a00ac790aa1bf45989cda350d1a756194a344fee55f0cf4169b6e72190759ebeecc";
pub(super) const SOME: &str = "46574d56000100016700017a01000000000000000100000000000000070000000000000000000000000000008646574d42000100016700017a000000000300000002000000000000000100000000000000020000000200000000000000020000000000000003000000020000000000000001000000000000000200000004000000000000000100000000000000020000000000000003000000000000000400000001610000000000000002780000000002ceb27492a63cb751990c91dc46ef3153f12a155c4b4d5490148e6bc916b947cbad10";

pub(super) fn hex(s: &str) -> TestResult<Vec<u8>> {
    s.as_bytes()
        .as_chunks::<2>()
        .0
        .iter()
        .map(|p| Ok(u8::from_str_radix(std::str::from_utf8(p)?, 16)?))
        .collect()
}

pub(super) fn stamp(p: u64, l: u16, w: u64) -> TestResult<EntryStamp> {
    Ok(EntryStamp::Stamped(CommittedTime::new(
        HlcTimestamp::new(p, l)?,
        w,
    )?))
}

pub(super) fn membership() -> TestResult<RaftMembership> {
    Ok(RaftMembership::new(
        vec![vec![1, 2], vec![2, 3], vec![1, 2]],
        vec![
            (1, "a".into()),
            (2, String::new()),
            (3, "x\0".into()),
            (4, "β".into()),
        ],
        RaftMembershipLimits::default(),
    )?)
}

pub(super) fn frame(member: bool, stamp: EntryStamp) -> TestResult<PreparedBytes> {
    let scope = f::scope()?;
    let limits = RaftMembershipLimits::default();
    Ok(if member {
        encode_raft_membership(
            scope.group(),
            scope.recovery(),
            stamp,
            &membership()?,
            limits,
            16 * f::MIB,
        )?
    } else {
        encode_raft_blank(scope.group(), scope.recovery(), stamp, limits, 16 * f::MIB)?
    })
}

pub(super) fn row(log: Option<LogIdentity>, body: Option<&[u8]>) -> TestResult<Vec<u8>> {
    let mut out = b"FWMV\0\x01\0\x01g\0\x01z".to_vec();
    out.push(u8::from(log.is_some()));
    match (log, body) {
        (Some(log), Some(body)) => {
            out.extend_from_slice(&log.term().to_be_bytes());
            out.extend_from_slice(&log.leader_node().to_be_bytes());
            out.extend_from_slice(&log.index().get().to_be_bytes());
            out.extend_from_slice(&u64::try_from(body.len())?.to_be_bytes());
            out.extend_from_slice(body);
        }
        (None, None) => {}
        _ => return Err("invalid independent membership row pair".into()),
    }
    let sha = Sha256::digest(&out);
    out.extend_from_slice(&sha);
    Ok(out)
}

pub(super) fn raw_meta(path: &Path) -> TestResult<BTreeMap<Vec<u8>, Vec<u8>>> {
    let db = redb::Builder::new().open(path)?;
    let tx = db.begin_read()?;
    tx.open_table(f::META)?
        .iter()?
        .map(|r| {
            let (k, v) = r?;
            Ok((k.value().to_vec(), v.value().to_vec()))
        })
        .collect()
}

pub(super) fn replace(
    path: &Path,
    rows: &BTreeMap<Vec<u8>, Vec<u8>>,
    meta: &BTreeMap<Vec<u8>, Vec<u8>>,
) -> TestResult {
    let db = redb::Builder::new().open(path)?;
    let mut tx = db.begin_write()?;
    tx.set_durability(Durability::Immediate)?;
    {
        let mut table = tx.open_table(f::ROWS)?;
        table.retain(|_, _| false)?;
        for (k, v) in rows {
            table.insert(k.as_slice(), v.as_slice())?;
        }
        let mut table = tx.open_table(f::META)?;
        table.retain(|_, _| false)?;
        for (k, v) in meta {
            table.insert(k.as_slice(), v.as_slice())?;
        }
    }
    tx.commit()?;
    Ok(())
}

pub(super) fn set_control(
    rows: &mut BTreeMap<Vec<u8>, Vec<u8>>,
    control: ControlState,
) -> TestResult {
    let scope = f::scope()?;
    rows.insert(
        key::encode_control_key(scope.group(), scope.recovery(), f::MIB)?,
        value::encode_control(scope.group(), scope.recovery(), control, f::MIB)?,
    );
    Ok(())
}

pub(super) fn current(state: &mut RedbStateStore) -> TestResult<ControlState> {
    let s = f::scope()?;
    let tx = state.begin_apply(
        s.group(),
        s.recovery(),
        wf_domain::workflow::admission::AdmissionLimits::default(),
        100,
        f::MIB,
        f::MIB,
    )?;
    Ok(tx.control()?)
}

pub(super) fn next(
    previous: ControlState,
    frame: &PreparedBytes,
    log: LogIdentity,
    stamp: EntryStamp,
) -> TestResult<ControlState> {
    Ok(plan_control(previous, EntryIdentity::new(log, frame.digest()), stamp)?.next())
}

pub(super) fn apply(
    state: &mut RedbStateStore,
    frame: &PreparedBytes,
    log: LogIdentity,
) -> TestResult<ControlState> {
    let s = f::scope()?;
    let tx = state.begin_apply(
        s.group(),
        s.recovery(),
        wf_domain::workflow::admission::AdmissionLimits::default(),
        100,
        1024,
        65536,
    )?;
    let changes = GroupControlChanges::new(
        s.group(),
        s.recovery(),
        tx.control()?,
        log,
        frame.as_bytes(),
        RaftMembershipLimits::default(),
    )?;
    let next = changes.control().next();
    tx.commit_control(&changes)?;
    Ok(next)
}

pub(super) fn assert_pair(
    path: &Path,
    rows: &BTreeMap<Vec<u8>, Vec<u8>>,
    meta: &BTreeMap<Vec<u8>, Vec<u8>>,
) -> TestResult {
    assert_eq!(f::raw_rows(path)?, *rows);
    assert_eq!(raw_meta(path)?, *meta);
    drop(f::open(path)?);
    assert_eq!(f::raw_rows(path)?, *rows);
    assert_eq!(raw_meta(path)?, *meta);
    Ok(())
}

pub(super) fn assert_applied(
    state: &mut RedbStateStore,
    expected: ControlState,
    member: Option<(&PreparedBytes, LogIdentity, EntryStamp)>,
) -> TestResult {
    let s = f::scope()?;
    let tx = state.begin_apply(
        s.group(),
        s.recovery(),
        wf_domain::workflow::admission::AdmissionLimits::default(),
        100,
        f::MIB,
        f::MIB,
    )?;
    assert_eq!(tx.owner_limits(), (f::MIB, f::MIB));
    let control = tx.control()?;
    let m = tx.applied_membership(RaftMembershipLimits::default())?;
    assert_eq!(control, expected);
    if let Some((body, id, stamp)) = member {
        assert_eq!(m.entry(), Some(EntryIdentity::new(id, body.digest())));
        assert_eq!(m.log_id(), Some(id));
        assert_eq!(m.stamp(), Some(stamp));
        assert_eq!(m.membership().configs(), membership()?.configs());
        assert_eq!(m.membership().nodes(), membership()?.nodes());
    } else {
        assert!(m.entry().is_none());
        assert!(m.stamp().is_none());
        let configs: &[Vec<u64>] = &[];
        let nodes: &[(u64, String)] = &[];
        assert_eq!(m.membership().configs(), configs);
        assert_eq!(m.membership().nodes(), nodes);
    }
    Ok(())
}
