//! Actual consensus-neutral entries share the existing workflow control/writer.

use super::{TestResult, fixtures as f};
use wf_domain::workflow::admission::AdmissionLimits;
use wf_domain::workflow::control::{ControlState, EntryStamp};
use wf_domain::workflow::request::RetainedStatus;
use wf_ports::consensus::RaftMembershipLimits;
use wf_ports::state::{
    GroupApplyStore, GroupControlChanges, GroupControlTransaction, StateTransactionError,
};
use wf_storage::state::{key, value};

mod faults;
pub(super) mod fixtures;
mod limits;
mod recovery;

#[test]
fn consensus_entries_publish_only_same_control_and_membership() -> TestResult {
    let dir = f::directory("control-publication")?;
    let path = dir.join("state.redb");
    let mut store = f::create(&path)?;
    let mut previous = ControlState::initial(f::clock(0)?);
    drop(store);
    let mut rows = f::raw_rows(&path)?;
    let mut meta = fixtures::raw_meta(&path)?;
    assert_eq!(meta.len(), 3);
    assert_eq!(
        meta.get(fixtures::MEMBER_KEY).ok_or("required member")?,
        &fixtures::hex(fixtures::NONE)?
    );
    assert_eq!(
        meta.get(b"manifest".as_slice()).ok_or("manifest")?[4..6],
        [0, 3]
    );
    let blank = fixtures::frame(false, EntryStamp::Unstamped)?;
    let stamp = fixtures::stamp(20, 3, 19)?;
    let member = fixtures::frame(true, stamp)?;
    store = f::open(&path)?;
    for (index, body, stamp) in [(0, &blank, EntryStamp::Unstamped), (1, &member, stamp)] {
        previous = fixtures::next(previous, body, f::log(index), stamp)?;
        assert_eq!(fixtures::apply(&mut store, body, f::log(index))?, previous);
        drop(store);
        fixtures::set_control(&mut rows, previous)?;
        if index == 1 {
            meta.insert(
                fixtures::MEMBER_KEY.to_vec(),
                fixtures::row(Some(f::log(index)), Some(body.as_bytes()))?,
            );
        }
        fixtures::assert_pair(&path, &rows, &meta)?;
        store = f::open(&path)?;
        fixtures::assert_applied(
            &mut store,
            previous,
            if index == 1 {
                Some((&member, f::log(1), stamp))
            } else {
                None
            },
        )?;
    }
    drop(store);
    let normal = f::frame(b"q", 100, vec![(b"i", f::create_op())])?;
    f::step(&mut rows, b"i", 100, f::create_op(), 2_u128 << 32, 1)?;
    rows = f::finish(
        rows,
        normal.as_bytes(),
        f::log(2),
        5,
        vec![0],
        RetainedStatus::Accepted,
    )?;
    drop(super::apply_and_check(&path, &normal, 2, 5, &rows)?);
    fixtures::assert_pair(&path, &rows, &meta)?;
    let scope = f::scope()?;
    previous = value::decode_control(
        rows.get(&key::encode_control_key(
            scope.group(),
            scope.recovery(),
            f::MIB,
        )?)
        .ok_or("control")?,
        scope.group(),
        scope.recovery(),
        f::MIB,
    )?;
    store = f::open(&path)?;
    fixtures::assert_applied(&mut store, previous, Some((&member, f::log(1), stamp)))?;
    drop(store);
    finish_clamped(&path, rows, meta, previous)?;
    std::fs::remove_dir_all(dir)?;
    Ok(())
}

fn finish_clamped(
    path: &std::path::Path,
    mut rows: std::collections::BTreeMap<Vec<u8>, Vec<u8>>,
    mut meta: std::collections::BTreeMap<Vec<u8>, Vec<u8>>,
    mut previous: ControlState,
) -> TestResult {
    let low = fixtures::stamp(10, 9, 9)?;
    let member = fixtures::frame(true, low)?;
    let high = fixtures::stamp(101, 1, 100)?;
    let blank = fixtures::frame(false, high)?;
    let mut store = f::open(path)?;
    for (index, body, stamp) in [(3, &member, low), (4, &blank, high)] {
        let next = fixtures::next(previous, body, f::log(index), stamp)?;
        if index == 3 {
            assert_eq!(next.time(), previous.time());
        }
        assert_eq!(fixtures::apply(&mut store, body, f::log(index))?, next);
        previous = next;
        drop(store);
        fixtures::set_control(&mut rows, previous)?;
        if index == 3 {
            meta.insert(
                fixtures::MEMBER_KEY.to_vec(),
                fixtures::row(Some(f::log(index)), Some(body.as_bytes()))?,
            );
        }
        fixtures::assert_pair(path, &rows, &meta)?;
        store = f::open(path)?;
        fixtures::assert_applied(&mut store, previous, Some((&member, f::log(3), low)))?;
    }
    assert_eq!(fixtures::apply(&mut store, &blank, f::log(4))?, previous);
    let changed = fixtures::frame(false, fixtures::stamp(102, 1, 100)?)?;
    assert!(
        matches!(fixtures::apply(&mut store,&changed,f::log(4)),Err(e) if e.downcast_ref::<StateTransactionError>()==Some(&StateTransactionError::InvalidChanges))
    );
    stale_and_foreign(&mut store, previous)?;
    drop(store);
    fixtures::assert_pair(path, &rows, &meta)
}

fn stale_and_foreign(
    store: &mut wf_storage::state::RedbStateStore,
    previous: ControlState,
) -> TestResult {
    let s = f::scope()?;
    let blank = fixtures::frame(false, EntryStamp::Unstamped)?;
    let changes = GroupControlChanges::new(
        s.group(),
        s.recovery(),
        ControlState::initial(f::clock(0)?),
        f::log(0),
        blank.as_bytes(),
        RaftMembershipLimits::default(),
    )?;
    let tx = store.begin_apply(
        s.group(),
        s.recovery(),
        AdmissionLimits::default(),
        100,
        1024,
        65536,
    )?;
    assert_eq!(
        tx.commit_control(&changes),
        Err(StateTransactionError::KnownAborted)
    );
    let foreign = wf_types::identity::GroupId::new(b"other".to_vec())?;
    let body = wf_ports::log::encode_raft_blank(
        &foreign,
        s.recovery(),
        EntryStamp::Unstamped,
        RaftMembershipLimits::default(),
        4096,
    )?;
    let changes = GroupControlChanges::new(
        &foreign,
        s.recovery(),
        previous,
        f::log(5),
        body.as_bytes(),
        RaftMembershipLimits::default(),
    )?;
    let tx = store.begin_apply(
        s.group(),
        s.recovery(),
        AdmissionLimits::default(),
        100,
        1024,
        65536,
    )?;
    assert_eq!(
        tx.commit_control(&changes),
        Err(StateTransactionError::InvalidChanges)
    );
    assert_eq!(fixtures::current(store)?, previous);
    Ok(())
}
