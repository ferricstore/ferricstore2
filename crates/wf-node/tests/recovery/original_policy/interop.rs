//! Real mixed-format receipts survive bounded historical audit and suffix replay.

use super::{
    CommittedRecoveryLimits, Error, TestResult, cohort, expected, f, normal, policy,
    recover_committed, reference, row,
};
use std::path::Path;
use wf_domain::claim::apply::ClaimWorkLimits;
use wf_domain::policy::complete::{
    CompleteCorePolicyPatchInput, CompleteCorePolicyPatchRequestInput, CompletePolicyEncodingError,
    CompletePolicyRequestError, PolicySchemaCharge,
};
use wf_domain::policy::definition::{FieldPatch, PolicyPatch, PolicyVersion};
use wf_domain::workflow::control::EntryIdentity;
use wf_domain::workflow::request::{RequestScope, RetainedOutcome, RetainedStatus};
use wf_ports::consensus::RaftMembershipLimits;
use wf_ports::log::{RaftLogError, RaftLogStore, prepare_complete_core_policy_patch_frame};
use wf_ports::state::{GroupApplyStore, GroupApplyTransaction, StoredPolicyValue};
use wf_storage::state::RedbStateStore;
use wf_storage::transaction::{
    ClaimTransactionLimits, CorePolicyTransactionError, RecordBatchTransactionOutcome,
    apply_claim_transaction, apply_complete_core_policy_patch_transaction,
    apply_original_policy_set_transaction, apply_public_mutation_transaction,
};
use wf_types::identity::RequestId;

const UNCOMMITTED: &[u8] = b"uncommitted interop suffix is never decoded";
const COMPLETE_V: &[u8] = b"FWPV\0\x02\0\0\0\0\0\0\0\x01\0\0\0\x16\
    FWPS\0\x02\0\0\0\x01v\0\0\0\0\0\0\0\0\0\0\0";

fn original_u(generation: u8) -> TestResult<Vec<u8>> {
    let mut raw = expected(generation)?;
    assert_eq!(raw.len(), 51);
    assert_eq!(&raw[21..26], &[116, 0, 0, 0, 2]);
    raw[25] = 3;
    raw.extend_from_slice(b"\x77\x07version\x61\x07");
    assert_eq!(raw.len(), 62);
    Ok(raw)
}

fn core(scope: &RequestScope, request: &[u8], now: u64, original: bool) -> TestResult<Vec<u8>> {
    let limits = normal();
    let patch = PolicyPatch {
        version: if original {
            FieldPatch::Set(PolicyVersion::Number(7))
        } else {
            FieldPatch::Unchanged
        },
        ..PolicyPatch::default()
    };
    Ok(prepare_complete_core_policy_patch_frame(
        CompleteCorePolicyPatchRequestInput::new(
            scope.clone(),
            RequestId::new(request.to_vec())?,
            f::clock(now, 0, now)?,
            CompleteCorePolicyPatchInput {
                workflow_type: if original { b"u" } else { b"v" }.to_vec(),
                expected_generation: Some(if original { 2 } else { 0 }),
                patch,
            },
        ),
        limits.schema,
        limits.codec.admission(),
        &mut PolicySchemaCharge::new(limits.schema),
    )?
    .as_bytes()
    .to_vec())
}

fn assert_reply(
    outcome: &RetainedOutcome,
    request: &[u8],
    index: u64,
    time: u64,
    versions: (u16, u16),
    response: &[u8],
) -> TestResult {
    assert_eq!(outcome.scope(), &f::scope()?);
    assert_eq!(outcome.request().as_bytes(), request);
    assert_eq!(outcome.status(), RetainedStatus::Accepted);
    assert_eq!(
        (outcome.intent().version(), outcome.response_version()),
        versions
    );
    assert_eq!(outcome.response(), response);
    assert_eq!(
        (
            outcome.original_position(),
            outcome.original_time().get(),
            outcome.expires_at().get()
        ),
        (f::at(index), time, time + 1000)
    );
    Ok(())
}

fn apply(state: &mut RedbStateStore, index: usize, frame: &[u8]) -> TestResult {
    let scope = f::scope()?;
    let at = f::at(u64::try_from(index)?);
    let limits = normal();
    match index {
        0 | 6 => {
            f::apply_control(state, u64::try_from(index)?, frame)?;
        }
        1 => drop(f::apply_normal(state, 1, frame, 5)?),
        3 => drop(apply_claim_transaction(
            state,
            &scope,
            frame,
            at,
            ClaimTransactionLimits {
                base: limits,
                work: ClaimWorkLimits::default(),
            },
        )?),
        4 => drop(apply_public_mutation_transaction(
            state, &scope, frame, at, limits,
        )?),
        2 | 5 | 9 | 11 => {
            let result = apply_original_policy_set_transaction(
                state,
                &scope,
                frame,
                at,
                limits,
                &mut PolicySchemaCharge::new(limits.schema),
            )?;
            if index == 9 || index == 11 {
                let RecordBatchTransactionOutcome::Retained { outcome, .. } = result else {
                    return Err("original request failed to retain/replay".into());
                };
                assert_reply(&outcome, b"merge", 9, 17, (4, 1), &original_u(4)?)?;
            }
        }
        7 | 8 | 10 => {
            let RecordBatchTransactionOutcome::Retained { control, outcome } =
                apply_complete_core_policy_patch_transaction(state, &scope, frame, at, limits)?
            else {
                return Err("Core request failed to retain/replay".into());
            };
            assert_eq!(control.last_entry().map(EntryIdentity::log), Some(at));
            if index == 7 {
                assert_reply(&outcome, b"core-v", 7, 15, (2, 1), COMPLETE_V)?;
            } else {
                assert_reply(&outcome, b"core-u", 8, 16, (2, 2), &original_u(3)?)?;
            }
        }
        _ => return Err("unselected committed fixture index".into()),
    }
    Ok(())
}

fn mixed_reference(dir: &Path) -> TestResult<Vec<Vec<u8>>> {
    let mut frames = reference(dir)?;
    let scope = f::scope()?;
    frames.push(core(&scope, b"core-v", 15, false)?);
    frames.push(core(&scope, b"core-u", 16, true)?);
    frames.push(policy(&scope, b"merge", 17, false)?);
    frames.push(frames[8].clone());
    frames.push(frames[9].clone());
    let (mut log, mut state) = f::open_pair(dir)?;
    log.truncate_from(7)?;
    for (index, body) in frames.iter().enumerate().skip(7) {
        f::append(&mut log, u64::try_from(index)?, body)?;
        apply(&mut state, index, body)?;
    }
    log.save_committed(f::at(11))?;
    f::append(&mut log, 12, UNCOMMITTED)?;
    inspect(&mut state)?;
    Ok(frames)
}

fn pending(dir: &Path, frames: &[Vec<u8>], cut: usize) -> TestResult {
    assert_eq!(frames.len(), 12);
    let (mut log, mut state) = f::create_pair(dir)?;
    for (index, frame) in frames.iter().enumerate() {
        f::append(&mut log, u64::try_from(index)?, frame)?;
        if index <= cut {
            apply(&mut state, index, frame)?;
        }
    }
    log.save_committed(f::at(11))?;
    f::append(&mut log, 12, UNCOMMITTED)?;
    Ok(())
}

fn inspect(state: &mut RedbStateStore) -> TestResult {
    let scope = f::scope()?;
    let limits = normal();
    let tx = state.begin_apply(
        scope.group(),
        scope.recovery(),
        limits.codec.admission(),
        1000,
        64 * f::MIB,
        32 * f::MIB,
    )?;
    let mut charge = PolicySchemaCharge::new(limits.schema);
    let original = tx
        .stored_policy_value(scope.tenant(), b"u", limits.schema, &mut charge)?
        .ok_or("original current missing")?;
    let StoredPolicyValue::Original(raw) = original else {
        return Err("Core rewrote original authority as V2".into());
    };
    assert_eq!(raw, original_u(4)?);
    let complete = tx
        .stored_policy_value(scope.tenant(), b"v", limits.schema, &mut charge)?
        .ok_or("complete current missing")?;
    let StoredPolicyValue::Complete(value) = complete else {
        return Err("Complete authority changed format".into());
    };
    assert_eq!(value.encoded(), COMPLETE_V);
    for (request, index, time, versions, raw) in [
        (b"set".as_slice(), 2, 11, (4, 1), expected(1)?),
        (b"replace", 5, 14, (4, 1), expected(2)?),
        (b"core-v", 7, 15, (2, 1), COMPLETE_V.to_vec()),
        (b"core-u", 8, 16, (2, 2), original_u(3)?),
        (b"merge", 9, 17, (4, 1), original_u(4)?),
    ] {
        let receipt = tx
            .outcome(scope.tenant(), &RequestId::new(request.to_vec())?)?
            .ok_or("mixed original receipt missing")?;
        assert_reply(&receipt, request, index, time, versions, &raw)?;
    }
    drop(tx);
    let record = row(state)?;
    assert_eq!(
        (record.record.workflow_type(), record.record.state()),
        (b"u".as_slice(), b"completed".as_slice())
    );
    assert_eq!(
        (record.record.version(), record.record.fence().get()),
        (3, 1)
    );
    assert_eq!(record.record.payload(), Some([0, 255].as_slice()));
    assert!(record.record.lease().is_none());
    assert_eq!(
        (record.ordering.priority, record.ordering.state_enter_seq),
        (0, 1_u128 << 32)
    );
    assert_eq!(
        f::applied(state)?.0.last_entry().map(EntryIdentity::log),
        Some(f::at(11))
    );
    Ok(())
}

fn limits(frames: &[Vec<u8>], points: usize, bytes: usize) -> TestResult<CommittedRecoveryLimits> {
    let maximum = frames
        .iter()
        .map(Vec::len)
        .max()
        .ok_or("committed bodies")?;
    Ok(CommittedRecoveryLimits::new(
        RaftMembershipLimits::default(),
        (10_000, 64 * f::MIB, 64 * f::MIB, 32 * f::MIB),
        normal(),
        (points, bytes),
        512 + maximum,
        1024 * f::MIB,
    )?
    .with_claim_work_limits(ClaimWorkLimits::default())
    .with_core_policy_patches()
    .with_original_policy_sets())
}

#[test]
fn mixed_original_and_core_etf_receipts_recover_under_suffix_budget() -> TestResult {
    let source = f::directory("interop-recovery-reference")?;
    let frames = mixed_reference(&source)?;
    let expected = cohort(&source)?;
    assert!(expected.iter().all(Option::is_some));
    let metadata = expected[0].as_ref().ok_or("live metadata")?;
    assert_eq!(
        &metadata.get(b"manifest".as_slice()).ok_or("manifest")?[..6],
        b"FWSS\0\x06"
    );
    assert_eq!(
        &metadata.get(b"origin".as_slice()).ok_or("origin")?[..6],
        b"FWSS\0\x03"
    );
    let scope = f::scope()?;
    let bytes: usize = frames.iter().map(Vec::len).sum();
    for cut in [0, 7, 8, 11] {
        let dir = f::directory("interop-recovery-cut")?;
        pending(&dir, &frames, cut)?;
        let logs = (f::log_rows(&dir)?, f::log_meta(&dir)?);
        for _ in 0..2 {
            let (log, state) = f::open_pair(&dir)?;
            let (log, mut state) = recover_committed(
                log,
                state,
                scope.group(),
                scope.recovery(),
                limits(&frames, 12, bytes)?,
            )?;
            inspect(&mut state)?;
            drop((log, state));
            assert_eq!(cohort(&dir)?, expected);
            assert_eq!((f::log_rows(&dir)?, f::log_meta(&dir)?), logs);
        }
        std::fs::remove_dir_all(dir)?;
    }
    assert_suffix_budgets(&frames)?;
    assert_historical_body(&frames)?;
    std::fs::remove_dir_all(source)?;
    Ok(())
}

fn assert_suffix_budgets(frames: &[Vec<u8>]) -> TestResult {
    let scope = f::scope()?;
    let bytes: usize = frames.iter().map(Vec::len).sum();
    for points in [11, 12] {
        let dir = f::directory("interop-recovery-narrow")?;
        let reference = f::directory("interop-recovery-prefix")?;
        pending(&dir, frames, 0)?;
        pending(&reference, frames, if points == 11 { 0 } else { 10 })?;
        let prefix = cohort(&reference)?;
        let logs = (f::log_rows(&dir)?, f::log_meta(&dir)?);
        let (log, state) = f::open_pair(&dir)?;
        assert_eq!(
            recover_committed(
                log,
                state,
                scope.group(),
                scope.recovery(),
                limits(frames, points, bytes - 1)?,
            )
            .err(),
            Some(if points == 11 {
                Error::Limit
            } else {
                Error::Log(RaftLogError::Limit)
            })
        );
        drop(f::open_pair(&dir)?);
        assert_eq!(cohort(&dir)?, prefix);
        assert_eq!((f::log_rows(&dir)?, f::log_meta(&dir)?), logs);
        let (log, state) = f::open_pair(&dir)?;
        let (log, mut state) = recover_committed(
            log,
            state,
            scope.group(),
            scope.recovery(),
            limits(frames, 12, bytes)?,
        )?;
        inspect(&mut state)?;
        drop((log, state));
        std::fs::remove_dir_all(dir)?;
        std::fs::remove_dir_all(reference)?;
    }
    Ok(())
}

fn assert_historical_body(frames: &[Vec<u8>]) -> TestResult {
    let dir = f::directory("interop-recovery-historical")?;
    let (mut log, mut state) = f::create_pair(&dir)?;
    for (index, frame) in frames.iter().enumerate() {
        let mut persisted = frame.clone();
        if index == 8 {
            persisted.push(0);
        }
        f::append(&mut log, u64::try_from(index)?, &persisted)?;
        apply(&mut state, index, frame)?;
    }
    log.save_committed(f::at(11))?;
    drop((log, state));
    let old = cohort(&dir)?;
    let logs = (f::log_rows(&dir)?, f::log_meta(&dir)?);
    let scope = f::scope()?;
    let (log, state) = f::open_pair(&dir)?;
    assert_eq!(
        recover_committed(
            log,
            state,
            scope.group(),
            scope.recovery(),
            super::caps(normal())?.with_core_policy_patches(),
        )
        .err(),
        Some(Error::CorePolicy(CorePolicyTransactionError::Apply(
            wf_domain::policy::complete::CompleteCorePolicyPatchApplyError::Request(
                CompletePolicyRequestError::Encoding(CompletePolicyEncodingError::TrailingBytes),
            ),
        )))
    );
    drop(f::open_pair(&dir)?);
    assert_eq!(cohort(&dir)?, old);
    assert_eq!((f::log_rows(&dir)?, f::log_meta(&dir)?), logs);
    std::fs::remove_dir_all(dir)?;
    Ok(())
}
