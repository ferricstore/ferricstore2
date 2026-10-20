//! Real complete policy replay and full historical verification on both owners.

use super::{
    CommittedRecoveryError as Error, CommittedRecoveryLimits, TestResult, fixtures as f,
    recover_committed,
};
use redb::{ReadableDatabase, ReadableTable, TableDefinition, TableHandle};
use sha2::{Digest, Sha256};
use std::path::Path;
use wf_domain::claim::apply::ClaimWorkLimits;
use wf_domain::policy::complete::{
    CompleteCorePolicyPatchApplyError, CompletePolicyEncodingError, CompletePolicyRequestError,
    PolicySchemaCharge, PolicySchemaError, PolicySchemaLimits,
};
use wf_domain::workflow::admission::AdmissionLimits;
use wf_domain::workflow::control::{EntryIdentity, EntryStamp};
use wf_domain::workflow::request::RetainedStatus;
use wf_ports::consensus::RaftMembershipLimits;
use wf_ports::log::RaftLogStore;
use wf_ports::state::{GroupApplyStore, GroupApplyTransaction, StateTransactionError};
use wf_storage::state::RedbStateStore;
use wf_storage::transaction::{
    ClaimTransactionLimits, CorePolicyTransactionError, RecordBatchTransactionLimits,
    RecordBatchTransactionOutcome, apply_claim_transaction,
    apply_complete_core_policy_patch_transaction,
};
use wf_types::identity::{PartitionId, RecordId, RequestId};

fn normal() -> RecordBatchTransactionLimits {
    let mut value = f::driver_limits();
    value.workspace_bytes = 512 * f::MIB;
    value
}
fn caps(normal: RecordBatchTransactionLimits) -> TestResult<CommittedRecoveryLimits> {
    Ok(CommittedRecoveryLimits::new(
        RaftMembershipLimits::default(),
        (10_000, 64 * f::MIB, 64 * f::MIB, 32 * f::MIB),
        normal,
        (1_000, 64 * f::MIB),
        16 * f::MIB,
        1024 * f::MIB,
    )?
    .with_claim_work_limits(ClaimWorkLimits::default())
    .with_core_policy_patches())
}
fn raw(out: &mut Vec<u8>, bytes: &[u8]) -> TestResult {
    out.extend_from_slice(&u32::try_from(bytes.len())?.to_be_bytes());
    out.extend_from_slice(bytes);
    Ok(())
}
fn policy(now: u64) -> TestResult<Vec<u8>> {
    let mut intent = b"WF-INTENT\0COMPLETE-CORE-PATCH\0V2\0".to_vec();
    raw(&mut intent, b"t")?;
    intent.push(1);
    intent.extend_from_slice(&7_u64.to_be_bytes());
    raw(&mut intent, b"r")?;
    intent.extend_from_slice(&[0; 4]);
    intent.extend_from_slice(&1_u32.to_be_bytes());
    raw(&mut intent, b"q")?;
    intent.extend_from_slice(&[1, 1, 0, 0, 0]); // Patch(mode=Parallel,retry/retention unchanged).
    let mut out = b"FWCP\0\x01\x05".to_vec();
    for bytes in [b"t".as_slice(), b"g", b"z", b"policy"] {
        raw(&mut out, bytes)?;
    }
    out.extend_from_slice(&2_u16.to_be_bytes());
    out.extend_from_slice(&Sha256::digest(&intent));
    out.extend_from_slice(&now.to_be_bytes());
    out.extend_from_slice(&0_u16.to_be_bytes());
    out.extend_from_slice(&now.to_be_bytes());
    raw(&mut out, &intent)?;
    Ok(out)
}
fn claim() -> TestResult<Vec<u8>> {
    let mut out = b"FWCP\0\x01\x03".to_vec();
    for bytes in [b"t".as_slice(), b"g", b"z", b"claim"] {
        raw(&mut out, bytes)?;
    }
    out.extend_from_slice(&1_u16.to_be_bytes());
    out.extend_from_slice(&[7; 32]);
    out.extend_from_slice(&12_u64.to_be_bytes());
    out.extend_from_slice(&0_u16.to_be_bytes());
    out.extend_from_slice(&12_u64.to_be_bytes());
    raw(&mut out, b"r")?;
    out.extend_from_slice(&[0; 7]);
    raw(&mut out, b"w")?;
    out.extend_from_slice(&50_u64.to_be_bytes());
    for count in [1_u32, 16_384, 4096] {
        out.extend_from_slice(&count.to_be_bytes());
    }
    out.extend_from_slice(&1_u16.to_be_bytes());
    out.extend_from_slice(&u32::MAX.to_be_bytes());
    Ok(out)
}
fn expected_response() -> TestResult<Vec<u8>> {
    let mut value = f::hex(
        "46575056000200000000000000070000007446575053000200000001720101ffffffffffffffff010000000757b12c00010100000101000000000000000a000000000000001e0001000000066661696c656401000000000001000000017101010100000001000001000000000000000100000000010000000570686173650100000000000000",
    )?;
    value[6..14].copy_from_slice(&8_u64.to_be_bytes());
    let marker = b"\0\0\0\x01\0\0\0\x01q\x01\x01";
    let matches = value
        .windows(marker.len())
        .enumerate()
        .filter_map(|(i, part)| (part == marker).then_some(i))
        .collect::<Vec<_>>();
    assert_eq!(matches.len(), 1);
    value[matches[0] + 10] = 0;
    Ok(value)
}
fn publish(state: &mut RedbStateStore, index: u64, body: &[u8]) -> TestResult {
    let result = apply_complete_core_policy_patch_transaction(
        state,
        &f::scope()?,
        body,
        f::at(index),
        normal(),
    )?;
    let RecordBatchTransactionOutcome::Retained { control, outcome } = result else {
        return Err("missing actual policy acknowledgement".into());
    };
    assert_eq!(
        control.last_entry().map(EntryIdentity::log),
        Some(f::at(index))
    );
    assert_eq!(outcome.response(), expected_response()?);
    assert_eq!(outcome.status(), RetainedStatus::Accepted);
    assert_eq!(
        (outcome.intent().version(), outcome.response_version()),
        (2, 1)
    );
    assert_eq!(
        (
            outcome.original_position(),
            outcome.original_time().get(),
            outcome.expires_at().get()
        ),
        (f::at(index), 11, 1011)
    );
    Ok(())
}
fn cohort(dir: &Path) -> TestResult<[Option<f::Rows>; 6]> {
    let tables = [
        "wf_state_meta_v1",
        "wf_state_rows_v1",
        "wf_state_snapshot_meta_v1",
        "wf_state_snapshot_rows_v1",
        "wf_state_snapshot_alt_meta_v1",
        "wf_state_snapshot_alt_rows_v1",
    ];
    let database = redb::Builder::new().open(dir.join("state.redb"))?;
    let read = database.begin_read()?;
    let mut names = read
        .list_tables()?
        .map(|table| table.name().to_owned())
        .collect::<Vec<_>>();
    names.sort();
    let mut expected = tables[..if names.len() == 4 { 4 } else { 6 }]
        .iter()
        .map(|s| (*s).to_owned())
        .collect::<Vec<_>>();
    expected.sort();
    assert_eq!(names, expected);
    assert_eq!(read.list_multimap_tables()?.count(), 0);
    let mut result = std::array::from_fn(|_| None);
    for (slot, name) in result.iter_mut().zip(tables) {
        if !names.iter().any(|actual| actual == name) {
            continue;
        }
        let table = read.open_table(TableDefinition::<&[u8], &[u8]>::new(name))?;
        let mut rows = f::Rows::new();
        for row in table.iter()? {
            let (key, value) = row?;
            assert!(
                rows.insert(key.value().to_vec(), value.value().to_vec())
                    .is_none()
            );
        }
        *slot = Some(rows);
    }
    Ok(result)
}
fn seed(dir: &Path, complete: bool) -> TestResult {
    let (mut log, mut state) = f::create_pair(dir)?;
    let member = f::member(EntryStamp::Unstamped, 4)?;
    f::append(&mut log, 0, member.as_bytes())?;
    f::apply_control(&mut state, 0, member.as_bytes())?;
    let create = f::frame(5, b"create", 10, vec![(b"i", f::create_op())])?;
    let policy = policy(11)?;
    let claim = claim()?;
    let blank = f::blank(EntryStamp::Unstamped)?;
    for (index, body) in [
        (1, create.as_bytes()),
        (2, &policy),
        (3, &claim),
        (4, blank.as_bytes()),
    ] {
        f::append(&mut log, index, body)?;
    }
    if complete {
        drop(f::apply_normal(&mut state, 1, create.as_bytes(), 5)?);
        publish(&mut state, 2, &policy)?;
        drop(apply_claim_transaction(
            &mut state,
            &f::scope()?,
            &claim,
            f::at(3),
            ClaimTransactionLimits {
                base: normal(),
                work: ClaimWorkLimits::default(),
            },
        )?);
        f::apply_control(&mut state, 4, blank.as_bytes())?;
    }
    f::append(&mut log, 5, b"uncommitted opaque policy suffix")?;
    log.save_committed(f::at(4))?;
    Ok(())
}
fn inspect(state: &mut RedbStateStore) -> TestResult {
    let s = f::scope()?;
    let schema = PolicySchemaLimits::default();
    let tx = state.begin_apply(
        s.group(),
        s.recovery(),
        AdmissionLimits::default(),
        1000,
        64 * f::MIB,
        32 * f::MIB,
    )?;
    let outcome = tx
        .outcome(s.tenant(), &RequestId::new(b"policy".to_vec())?)?
        .ok_or("missing retained policy")?;
    assert_eq!(outcome.response(), expected_response()?);
    assert_eq!(
        (
            outcome.original_position(),
            outcome.original_time().get(),
            outcome.expires_at().get()
        ),
        (f::at(2), 11, 1011)
    );
    let current = tx
        .installed_policy(
            s.tenant(),
            b"r",
            schema,
            &mut PolicySchemaCharge::new(schema),
        )?
        .ok_or("missing full current policy")?;
    assert_eq!(current.encoded(), expected_response()?);
    let row = tx
        .record(
            s.tenant(),
            &PartitionId::new(b"p".to_vec())?,
            &RecordId::new(b"i".to_vec())?,
        )?
        .ok_or("missing actual claimed row")?;
    assert_eq!(
        (
            row.record.state(),
            row.record.version(),
            row.record.fence().get()
        ),
        (b"running".as_slice(), 2, 1)
    );
    assert_eq!(
        row.record.lease().ok_or("missing lease")?.token.as_bytes(),
        b"w:12:1"
    );
    assert_eq!(
        (row.ordering.priority, row.ordering.state_enter_seq),
        (0, 1_u128 << 32)
    );
    Ok(())
}

#[test]
fn committed_core_patch_replays_real_publication_and_audits_all_bodies() -> TestResult {
    let reference = f::directory("policy-replay-reference")?;
    seed(&reference, true)?;
    let expected = cohort(&reference)?;
    assert!(expected.iter().all(Option::is_some));
    let dir = f::directory("policy-replay")?;
    seed(&dir, false)?;
    let logs = (f::log_rows(&dir)?, f::log_meta(&dir)?);
    let s = f::scope()?;
    for _ in 0..2 {
        let (log, state) = f::open_pair(&dir)?;
        let (log, mut state) =
            recover_committed(log, state, s.group(), s.recovery(), caps(normal())?)?;
        inspect(&mut state)?;
        drop((log, state));
        assert_eq!(cohort(&dir)?, expected);
        assert_eq!((f::log_rows(&dir)?, f::log_meta(&dir)?), logs);
    }
    assert_historical_corruption()?;
    let disabled = super::limits()?;
    assert!(!disabled.core_policy_patches());
    let (log, state) = f::open_pair(&reference)?;
    assert_eq!(
        recover_committed(log, state, s.group(), s.recovery(), disabled).err(),
        Some(Error::UnsupportedEntry)
    );
    drop(f::open_pair(&reference)?);
    assert_eq!(cohort(&reference)?, expected);
    std::fs::remove_dir_all(dir)?;
    std::fs::remove_dir_all(reference)?;
    Ok(())
}
fn assert_historical_corruption() -> TestResult {
    for mode in [0, 1] {
        let dir = f::directory("policy-historical-corrupt")?;
        let (mut log, mut state) = f::create_pair(&dir)?;
        let member = f::member(EntryStamp::Unstamped, 4)?;
        f::append(&mut log, 0, member.as_bytes())?;
        f::apply_control(&mut state, 0, member.as_bytes())?;
        let valid = policy(11)?;
        let mut bad = valid.clone();
        if mode == 0 {
            bad.push(0);
        } else {
            bad[34] ^= 1; // Four scope/request raw fields and intent version precede digest.
        }
        f::append(&mut log, 1, &bad)?;
        publish(&mut state, 1, &valid)?;
        let blank = f::blank(EntryStamp::Unstamped)?;
        f::append(&mut log, 2, blank.as_bytes())?;
        f::apply_control(&mut state, 2, blank.as_bytes())?;
        log.save_committed(f::at(2))?;
        drop((log, state));
        let old = cohort(&dir)?;
        let logs = (f::log_rows(&dir)?, f::log_meta(&dir)?);
        let (log, state) = f::open_pair(&dir)?;
        let s = f::scope()?;
        let error = recover_committed(log, state, s.group(), s.recovery(), caps(normal())?)
            .err()
            .ok_or("historical full policy body was not audited")?;
        let expected = if mode == 0 {
            CompletePolicyRequestError::Encoding(CompletePolicyEncodingError::TrailingBytes)
        } else {
            CompletePolicyRequestError::IntentMismatch
        };
        assert_eq!(
            error,
            Error::CorePolicy(CorePolicyTransactionError::Apply(
                CompleteCorePolicyPatchApplyError::Request(expected)
            ))
        );
        drop(f::open_pair(&dir)?);
        assert_eq!(cohort(&dir)?, old);
        assert_eq!((f::log_rows(&dir)?, f::log_meta(&dir)?), logs);
        std::fs::remove_dir_all(dir)?;
    }
    Ok(())
}

#[test]
fn committed_policy_profile_and_failed_prefix_release_both_owners() -> TestResult {
    assert_header_only_peak()?;
    for mode in 0..3 {
        let reference = f::directory("policy-prefix-reference")?;
        let dir = f::directory("policy-prefix-failure")?;
        seed_prefix(&reference, true)?;
        seed_prefix(&dir, false)?;
        let prefix = cohort(&reference)?;
        let logs = (f::log_rows(&dir)?, f::log_meta(&dir)?);
        let mut driver = normal();
        let expected = match mode {
            0 => {
                driver.schema = PolicySchemaLimits::new(driver.schema.policy_limits(), 1)?;
                None
            }
            1 => {
                driver.maximum_read_steps = 1;
                Some(StateTransactionError::Limit)
            }
            _ => {
                driver.maximum_adapter_bytes = 1;
                Some(StateTransactionError::Limit)
            }
        };
        let (log, state) = f::open_pair(&dir)?;
        let s = f::scope()?;
        let error = recover_committed(log, state, s.group(), s.recovery(), caps(driver)?)
            .err()
            .ok_or("narrow policy profile accepted")?;
        if let Some(expected) = expected {
            assert_eq!(
                error,
                Error::CorePolicy(CorePolicyTransactionError::Transaction(expected))
            );
        } else {
            assert!(matches!(
                error,
                Error::CorePolicy(CorePolicyTransactionError::Apply(
                    CompleteCorePolicyPatchApplyError::Request(
                        CompletePolicyRequestError::Encoding(CompletePolicyEncodingError::Schema(
                            PolicySchemaError::WorkBytes { maximum: 1, .. }
                        ))
                    )
                ))
            ));
        }
        drop(f::open_pair(&dir)?);
        assert_eq!(cohort(&dir)?, prefix);
        assert_eq!((f::log_rows(&dir)?, f::log_meta(&dir)?), logs);
        let (log, state) = f::open_pair(&dir)?;
        let (log, state) = recover_committed(log, state, s.group(), s.recovery(), caps(normal())?)?;
        drop((log, state));
        let (log, mut state) = f::open_pair(&reference)?;
        publish(&mut state, 2, &policy(11)?)?;
        drop((log, state));
        assert_eq!(cohort(&dir)?, cohort(&reference)?);
        std::fs::remove_dir_all(dir)?;
        std::fs::remove_dir_all(reference)?;
    }
    Ok(())
}
fn seed_prefix(dir: &Path, complete: bool) -> TestResult {
    let (mut log, mut state) = f::create_pair(dir)?;
    let blank = f::blank(EntryStamp::Unstamped)?;
    f::append(&mut log, 0, blank.as_bytes())?;
    f::apply_control(&mut state, 0, blank.as_bytes())?;
    f::append(&mut log, 1, blank.as_bytes())?;
    if complete {
        f::apply_control(&mut state, 1, blank.as_bytes())?;
    }
    f::append(&mut log, 2, &policy(11)?)?;
    log.save_committed(f::at(2))?;
    Ok(())
}
fn assert_header_only_peak() -> TestResult {
    let dir = f::directory("policy-header-peak")?;
    let (mut log, mut state) = f::create_pair(&dir)?;
    let bytes = policy(11)?;
    f::append(&mut log, 0, &bytes)?;
    publish(&mut state, 0, &bytes)?;
    log.save_committed(f::at(0))?;
    drop((log, state));
    let before = cohort(&dir)?;
    let s = f::scope()?;
    let exact = 3588 + 512 + 2048 + 3 * bytes.len();
    for workspace in [exact - 1, exact] {
        let cap = CommittedRecoveryLimits::new(
            RaftMembershipLimits::default(),
            (100, 1536, 1024, 131),
            normal(),
            (1, bytes.len()),
            512 + bytes.len(),
            workspace,
        )?
        .with_core_policy_patches();
        let (log, state) = f::open_pair(&dir)?;
        let result = recover_committed(log, state, s.group(), s.recovery(), cap);
        if workspace == exact {
            drop(result?);
        } else {
            assert_eq!(result.err(), Some(Error::Limit));
        }
        drop(f::open_pair(&dir)?);
        assert_eq!(cohort(&dir)?, before);
    }
    std::fs::remove_dir_all(dir)?;
    Ok(())
}
