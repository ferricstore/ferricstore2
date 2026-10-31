//! Real ACK replay, retained originals and both pre/post-ACK snapshot custody.

use super::*;
use wf_domain::governance::{
    catalog::{
        CreditCatalogAckReply as Reply, CreditCatalogAckRequest as Request,
        CreditCatalogMeta as Meta,
    },
    identified::{CreditConfiguration, IdentifiedCreditMutation, IdentifiedCreditRequest},
};
use wf_ports::log::{
    PreparedBytes, decode_catalog_ack_reply, encode_catalog_ack_request, encode_credit_request,
};
use wf_storage::transaction::apply_catalog_ack_transaction;
use wf_types::identity::RequestId;

fn frames() -> TestResult<Vec<PreparedBytes>> {
    let mut frames = c::frames()?;
    let scope = f::scope()?;
    let request = RequestId::new(b"ack".to_vec())?;
    let command = Request {
        expected_head: 1,
        up_to: 1,
    };
    frames.push(encode_catalog_ack_request(
        &scope,
        &request,
        f::clock(1006, 0, 1006)?,
        command,
        f::driver_limits().codec,
        16 * f::MIB,
    )?);
    frames.push(encode_credit_request(
        &scope,
        &RequestId::new(b"later".to_vec())?,
        f::clock(1007, 0, 1007)?,
        IdentifiedCreditRequest {
            scope: b"later",
            shard_count: 4,
            configuration: Some(CreditConfiguration {
                limit: 1,
                config_version: Some(1),
                policy_fingerprint: None,
            }),
            mutation: IdentifiedCreditMutation::Lease {
                shard_id: 0,
                amount: 1,
                ttl_ms: 1000,
            },
        },
        f::driver_limits().codec,
        16 * f::MIB,
    )?);
    frames.push(encode_catalog_ack_request(
        &scope,
        &request,
        f::clock(1006, 0, 1006)?,
        command,
        f::driver_limits().codec,
        16 * f::MIB,
    )?);
    Ok(frames)
}

fn append(log: &mut RedbLogStore, frames: &[PreparedBytes], bad: bool) -> TestResult {
    for (index, frame) in frames.iter().enumerate() {
        let mut bytes = frame.as_bytes().to_vec();
        if bad && index == 6 {
            bytes.push(0);
        }
        f::append(log, u64::try_from(index)?, &bytes)?;
    }
    f::append(log, 9, b"opaque uncommitted ACK suffix")?;
    log.save_committed(f::at(8))?;
    Ok(())
}

fn check(state: &mut RedbStateStore) -> TestResult {
    verify(state)?;
    let page = state.read_credit_catalog(&f::scope()?, 256, 10_000, 64 * f::MIB)?;
    assert_eq!(
        (page.meta.head(), page.meta.tail(), page.entries.len()),
        (2, 2, 1)
    );
    assert_eq!(
        (
            page.entries[0].sequence,
            page.entries[0].credit_scope.as_slice()
        ),
        (2, b"later".as_slice())
    );
    let original = c::outcome(state, b"ack")?;
    assert_eq!(
        (
            original.intent().version(),
            original.response_version(),
            original.original_position(),
            original.original_time().get()
        ),
        (13, 11, f::at(6), 1006)
    );
    assert_eq!(
        decode_catalog_ack_reply(original.response(), 1024, 1024)?,
        Reply::Accepted {
            meta: Meta::restore(2, 1)?,
            deleted: 1
        }
    );
    Ok(())
}

#[test]
fn catalog_ack_node_replays_once_preserves_original_reply_and_keeps_generic_profiles_closed()
-> TestResult {
    let dir = f::directory("catalog-ack-node-replay")?;
    let (mut log, mut state) = pair(&dir)?;
    let frames = frames()?;
    append(&mut log, &frames, false)?;
    c::apply(&mut state, &frames[0], 0)?;
    c::apply(&mut state, &frames[1], 1)?;
    drop((log, state));
    let before = c::cohort(&dir)?;
    let log_before = f::log_rows(&dir)?;
    let scope = f::scope()?;
    let (log, state) = f::open_pair(&dir)?;
    assert_eq!(
        recover_committed(log, state, scope.group(), scope.recovery(), caps()?).err(),
        Some(E::UnsupportedEntry)
    );
    assert_eq!(c::cohort(&dir)?, before);
    assert_eq!(f::log_rows(&dir)?, log_before);
    let (log, state) = f::open_pair(&dir)?;
    let (log, mut state) =
        recover_committed(log, state, scope.group(), scope.recovery(), selected()?)?;
    check(&mut state)?;
    drop((log, state));
    let before = c::cohort(&dir)?;
    let (log, state) = f::open_pair(&dir)?;
    drop(recover_committed(
        log,
        state,
        scope.group(),
        scope.recovery(),
        selected()?,
    )?);
    assert_eq!(c::cohort(&dir)?, before);
    assert_eq!(f::log_rows(&dir)?, log_before);
    std::fs::remove_dir_all(dir)?;
    Ok(())
}

#[test]
fn catalog_ack_node_audits_full_historical_body_behind_an_applied_last_entry() -> TestResult {
    let dir = f::directory("catalog-ack-node-history")?;
    let (mut log, mut state) = pair(&dir)?;
    let frames = frames()?;
    append(&mut log, &frames, true)?;
    for (index, frame) in frames.iter().enumerate() {
        if frame.as_bytes().get(6) == Some(&11) {
            apply_catalog_ack_transaction(
                &mut state,
                &f::scope()?,
                frame.as_bytes(),
                f::at(u64::try_from(index)?),
                f::driver_limits(),
            )?;
        } else {
            c::apply(&mut state, frame, u64::try_from(index)?)?;
        }
    }
    check(&mut state)?;
    drop((log, state));
    let before = c::cohort(&dir)?;
    let logs = f::log_rows(&dir)?;
    let scope = f::scope()?;
    let (log, state) = f::open_pair(&dir)?;
    assert_eq!(
        recover_committed(log, state, scope.group(), scope.recovery(), selected()?).err(),
        Some(E::Codec(CodecError::TrailingBytes))
    );
    assert_eq!(c::cohort(&dir)?, before);
    assert_eq!(f::log_rows(&dir)?, logs);
    std::fs::remove_dir_all(dir)?;
    Ok(())
}

fn received(path: &Path, wire: &[u8], frames: &[PreparedBytes], expected: &f::Rows) -> TestResult {
    std::fs::create_dir(path)?;
    let mut log = c::log(path)?;
    for (index, frame) in frames.iter().enumerate() {
        f::append(&mut log, u64::try_from(index)?, frame.as_bytes())?;
    }
    f::append(&mut log, 9, b"opaque uncommitted ACK suffix")?;
    let scope = f::scope()?;
    let candidate = RedbSnapshotStage::receive(
        &path.join("state.redb"),
        scope.group().clone(),
        scope.recovery().clone(),
        &mut std::io::Cursor::new(wire),
        StateStoreLimits::default(),
        SnapshotLimits::default(),
    )?;
    let (mut log, state) = restore_received_snapshot(
        log,
        candidate,
        scope.group(),
        scope.recovery(),
        selected()?,
        SnapshotLimits::default(),
    )?;
    log.save_committed(f::at(8))?;
    let (log, mut state) = recover_snapshotted(
        log,
        state,
        scope.group(),
        scope.recovery(),
        selected()?,
        SnapshotLimits::default(),
    )?;
    check(&mut state)?;
    drop((log, state));
    assert_eq!(&f::state_rows(path)?, expected);
    Ok(())
}

#[test]
fn catalog_ack_node_purged_received_suffix_and_post_ack_snapshot_preserve_original_metadata()
-> TestResult {
    let dir = f::directory("catalog-ack-node-snapshot")?;
    let source = dir.join("source");
    std::fs::create_dir(&source)?;
    let (mut log, mut state) = pair(&source)?;
    let frames = frames()?;
    append(&mut log, &frames, false)?;
    c::apply(&mut state, &frames[0], 0)?;
    c::apply(&mut state, &frames[1], 1)?;
    let mut before_ack = Vec::new();
    state.capture_snapshot(&mut before_ack, SnapshotLimits::default())?;
    let scope = f::scope()?;
    let (log, state) = purge_snapshot(
        log,
        state,
        scope.group(),
        scope.recovery(),
        selected()?,
        SnapshotLimits::default(),
    )?;
    let (log, mut state) = recover_snapshotted(
        log,
        state,
        scope.group(),
        scope.recovery(),
        selected()?,
        SnapshotLimits::default(),
    )?;
    check(&mut state)?;
    let mut after_ack = Vec::new();
    let snapshot = state.capture_snapshot(&mut after_ack, SnapshotLimits::default())?;
    assert_eq!(snapshot.control().position(), f::at(8).index());
    drop((log, state));
    let expected = f::state_rows(&source)?;
    received(&dir.join("suffix"), &before_ack, &frames, &expected)?;
    received(&dir.join("checkpoint"), &after_ack, &frames, &expected)?;
    std::fs::remove_dir_all(dir)?;
    Ok(())
}
