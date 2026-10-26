//! Claim startup decoder, outer-scope and driver budget boundaries.

use super::{
    CommittedRecoveryLimits, Error, TestResult, empty_receipt, f, phase, recover_committed, refusal,
};
use std::path::Path;
use wf_domain::claim::apply::ClaimWorkLimits;
use wf_domain::workflow::admission::AdmissionLimits;
use wf_domain::workflow::control::EntryStamp;
use wf_domain::workflow::request::RequestScope;
use wf_ports::log::{RaftLogError, RaftLogStore, RecordBatchApplyError};
use wf_ports::state::{GroupApplyStore, GroupApplyTransaction};
use wf_storage::transaction::{
    ClaimTransactionError, ClaimTransactionLimits, RecordBatchTransactionLimits,
    apply_claim_transaction,
};
use wf_types::identity::RequestId;

pub(super) fn profile(
    normal: RecordBatchTransactionLimits,
    work: (usize, usize),
    entry: usize,
    workspace: usize,
) -> TestResult<CommittedRecoveryLimits> {
    Ok(CommittedRecoveryLimits::new(
        wf_ports::consensus::RaftMembershipLimits::default(),
        (100, 1536, 1024, 131),
        normal,
        work,
        entry,
        workspace,
    )?
    .with_claim_work_limits(ClaimWorkLimits::default()))
}

fn scope_peak() -> TestResult {
    let dir = f::directory("claim-large-scope")?;
    let s = f::scope_for(&[255; 4096], b"g", b"z")?;
    let (mut log, state) = f::create_pair_for(&dir, &s)?;
    let body = phase(&s, b"empty", 1, b"absent", 1)?;
    f::append(&mut log, 0, &body)?;
    log.save_committed(f::at(0))?;
    drop((log, state));
    let normal = f::driver_limits();
    let q = 512 + 4096 + 1 + 1;
    let exact = 3588 + 512 + q + normal.workspace_bytes;
    for maximum in [exact - 1, exact, with_headers(exact)? - 1] {
        refusal(
            &dir,
            &s,
            &profile(normal, (1, body.len()), 512 + body.len(), maximum)?,
            Error::Limit,
        )?;
    }
    let (log, state) = f::open_pair_for(&dir, &s)?;
    let (log, mut state) = recover_committed(
        log,
        state,
        s.group(),
        s.recovery(),
        profile(
            normal,
            (1, body.len()),
            512 + body.len(),
            with_headers(exact)?,
        )?,
    )?;
    let tx = state.begin_apply(
        s.group(),
        s.recovery(),
        AdmissionLimits::default(),
        100,
        16 * f::MIB,
        16 * f::MIB,
    )?;
    let outcome = tx
        .outcome(s.tenant(), &RequestId::new(b"empty".to_vec())?)?
        .ok_or("large-scope outcome missing")?;
    assert_eq!(outcome.response(), empty_receipt());
    assert_eq!(outcome.scope().tenant(), s.tenant());
    drop(tx);
    drop((log, state));
    drop(f::open_pair_for(&dir, &s)?);
    std::fs::remove_dir_all(dir)?;
    Ok(())
}

pub(super) fn startup() -> TestResult {
    let dir = f::directory("claim-audit-budget")?;
    let s = f::scope()?;
    let (mut log, mut state) = f::create_pair(&dir)?;
    let body = phase(&s, b"empty", 1, b"absent", 1)?;
    f::append(&mut log, 0, &body)?;
    drop(apply_claim_transaction(
        &mut state,
        &s,
        &body,
        f::at(0),
        ClaimTransactionLimits {
            base: f::driver_limits(),
            work: ClaimWorkLimits::default(),
        },
    )?);
    log.save_committed(f::at(0))?;
    drop((log, state));
    let bytes = body.len();
    let d = 4 * bytes + 2048;
    let exact = 3588 + 512 + d;
    let header = std::mem::size_of::<wf_ports::log::DecodedRequest<'static>>();
    let funded = exact.checked_add(header).ok_or("header budget overflow")?;
    raw_audit_caps(&dir, &s, bytes, d, header)?;
    refusal(
        &dir,
        &s,
        &profile(
            f::driver_limits(),
            (1, bytes),
            512 + bytes,
            with_headers(exact - 1)?,
        )?,
        Error::Claim(ClaimTransactionError::Apply(
            RecordBatchApplyError::Workspace {
                attempted: d,
                maximum: d - 1,
            },
        )),
    )?;
    refusal(
        &dir,
        &s,
        &profile(
            f::driver_limits(),
            (1, bytes),
            512 + bytes,
            with_headers(funded - 1)?,
        )?,
        Error::Claim(ClaimTransactionError::Apply(
            RecordBatchApplyError::Workspace {
                attempted: d + header,
                maximum: d + header - 1,
            },
        )),
    )?;
    for (wire, entry) in [(bytes - 1, 512 + bytes), (bytes, 511 + bytes)] {
        refusal(
            &dir,
            &s,
            &profile(f::driver_limits(), (1, wire), entry, with_headers(funded)?)?,
            Error::Log(RaftLogError::Limit),
        )?;
    }
    let (log, state) = f::open_pair(&dir)?;
    drop(recover_committed(
        log,
        state,
        s.group(),
        s.recovery(),
        profile(
            f::driver_limits(),
            (1, bytes),
            512 + bytes,
            with_headers(funded)?,
        )?,
    )?);
    retained_suffix(&dir, &s, bytes, funded)?;
    std::fs::remove_dir_all(dir)?;
    scope_peak()
}

fn raw_audit_caps(
    dir: &Path,
    scope: &RequestScope,
    bytes: usize,
    d: usize,
    header: usize,
) -> TestResult {
    let exact = 3588 + 512 + d;
    let funded = exact.checked_add(header).ok_or("header budget overflow")?;
    for outer in [exact - 1, funded - 1] {
        let available = outer - with_headers(3588)? - 512;
        let attempted = [d, d + header]
            .into_iter()
            .find(|n| *n > available)
            .ok_or("raw Claim cap must refuse")?;
        refusal(
            dir,
            scope,
            &profile(f::driver_limits(), (1, bytes), 512 + bytes, outer)?,
            Error::Claim(ClaimTransactionError::Apply(
                RecordBatchApplyError::Workspace {
                    attempted,
                    maximum: available,
                },
            )),
        )?;
    }
    Ok(())
}

fn with_headers(bytes: usize) -> TestResult<usize> {
    let fixed = std::mem::size_of::<Option<[usize; 4]>>()
        .checked_add(
            std::mem::size_of::<wf_node::recovery::CommittedRecoveryError>()
                .max(std::mem::size_of::<wf_node::recovery::SnapshotRecoveryError>()),
        )
        .ok_or("fixed recovery header overflow")?;
    Ok(bytes.checked_add(fixed).ok_or("outer profile overflow")?)
}

fn retained_suffix(dir: &Path, s: &RequestScope, bytes: usize, funded: usize) -> TestResult {
    let (mut log, state) = f::open_pair(dir)?;
    let blank = f::blank(EntryStamp::Unstamped)?;
    assert_eq!(blank.as_bytes().len(), 13);
    f::append(&mut log, 1, blank.as_bytes())?;
    log.save_committed(f::at(1))?;
    drop((log, state));
    refusal(
        dir,
        s,
        &profile(
            f::driver_limits(),
            (1, bytes + 13),
            512 + bytes,
            with_headers(funded)?,
        )?,
        Error::Limit,
    )?;
    refusal(
        dir,
        s,
        &profile(
            f::driver_limits(),
            (2, bytes + 12),
            512 + bytes,
            with_headers(funded)?,
        )?,
        Error::Log(RaftLogError::Limit),
    )?;
    let (log, state) = f::open_pair(dir)?;
    drop(recover_committed(
        log,
        state,
        s.group(),
        s.recovery(),
        profile(
            f::driver_limits(),
            (2, bytes + 13),
            512 + bytes,
            with_headers(funded)?,
        )?,
    )?);
    Ok(())
}
