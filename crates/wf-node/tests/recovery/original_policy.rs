//! Actual original public frames join bounded audit and mixed committed replay.

use super::{
    CommittedRecoveryError as Error, CommittedRecoveryLimits, TestResult, fixtures as f,
    recover_committed,
};
use redb::{ReadableDatabase, ReadableTable, TableDefinition, TableHandle};
use sha2::{Digest, Sha256};
use std::path::Path;
use wf_domain::claim::apply::ClaimWorkLimits;
use wf_domain::policy::complete::{
    OriginalPolicySetFrameInput, OriginalPolicySetMapOrders, PolicySchemaCharge,
    RawFlowPolicyValue as Raw, prepare_original_policy_set_request,
};
use wf_domain::workflow::admission::AdmissionLimits;
use wf_domain::workflow::caller::{
    MutationFieldValue as V, MutationInput, normalize_core_mutation,
};
use wf_domain::workflow::control::{EntryIdentity, EntryStamp};
use wf_domain::workflow::request::{RequestScope, RetainedStatus};
use wf_ports::consensus::RaftMembershipLimits;
use wf_ports::log::{RaftLogStore, encode_public_mutation, prepare_original_policy_set_frame};
use wf_ports::state::{
    GroupApplyStore, GroupApplyTransaction, StoredPolicyValue, StoredWorkflowRecord,
};
use wf_storage::state::RedbStateStore;
use wf_storage::transaction::{
    ClaimTransactionLimits, ClaimTransactionOutcome, RecordBatchTransactionLimits,
    RecordBatchTransactionOutcome, apply_claim_transaction, apply_original_policy_set_transaction,
    apply_public_mutation_transaction,
};
use wf_types::identity::{PartitionId, RecordId, RequestId};

mod audit;

fn normal() -> RecordBatchTransactionLimits {
    let mut value = f::driver_limits();
    value.workspace_bytes = 512 * f::MIB;
    value
}

fn caps(driver: RecordBatchTransactionLimits) -> TestResult<CommittedRecoveryLimits> {
    Ok(CommittedRecoveryLimits::new(
        RaftMembershipLimits::default(),
        (10_000, 64 * f::MIB, 64 * f::MIB, 32 * f::MIB),
        driver,
        (1000, 64 * f::MIB),
        16 * f::MIB,
        1024 * f::MIB,
    )?
    .with_claim_work_limits(ClaimWorkLimits::default())
    .with_original_policy_sets())
}

fn policy(scope: &RequestScope, request: &[u8], now: u64, replace: bool) -> TestResult<Vec<u8>> {
    const PATCH: &[u8] = &[0, 0, 0, 17, 4, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 17];
    let profile = normal();
    let mut charge = PolicySchemaCharge::new(profile.schema);
    let replace = [
        Raw::Atom("replace"),
        if replace {
            Raw::Atom("true")
        } else {
            Raw::Atom("false")
        },
    ];
    let opts = [Raw::Tuple(&replace)];
    let prepared = prepare_original_policy_set_request(
        &Raw::Binary(b"u"),
        &Raw::List(&opts),
        profile.schema,
        &mut charge,
    )?
    .map_err(|_| "original public preparation refused")?;
    // Only the committed empty PATCH map is supplied; no current/merged donor.
    let orders = OriginalPolicySetMapOrders::new(
        Sha256::digest(PATCH).into(),
        None,
        &[(0, &[], &[])],
        &[],
        &[],
    );
    Ok(prepare_original_policy_set_frame(
        OriginalPolicySetFrameInput::new(
            scope.clone(),
            RequestId::new(request.to_vec())?,
            f::clock(now, 0, now)?,
            &prepared,
            &orders,
        ),
        profile.schema,
        profile.codec.admission(),
        &mut charge,
    )?
    .as_bytes()
    .to_vec())
}

fn expected(generation: u8) -> TestResult<Vec<u8>> {
    let mut raw = f::hex(
        "836803770e666c6f775f706f6c6963795f763161017400000002770673746174657374000000007704747970656d0000000175",
    )?;
    raw[20] = generation;
    Ok(raw)
}

fn raw(out: &mut Vec<u8>, bytes: &[u8]) -> TestResult {
    out.extend_from_slice(&u32::try_from(bytes.len())?.to_be_bytes());
    out.extend_from_slice(bytes);
    Ok(())
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
    raw(&mut out, b"u")?;
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

fn row(state: &mut RedbStateStore) -> TestResult<StoredWorkflowRecord> {
    let scope = f::scope()?;
    let tx = state.begin_apply(
        scope.group(),
        scope.recovery(),
        AdmissionLimits::default(),
        1000,
        64 * f::MIB,
        32 * f::MIB,
    )?;
    Ok(tx
        .record(
            scope.tenant(),
            &PartitionId::new(b"p".to_vec())?,
            &RecordId::new(b"i".to_vec())?,
        )?
        .ok_or("actual claimed/completed record missing")?)
}

fn complete(owner: &StoredWorkflowRecord) -> TestResult<Vec<u8>> {
    let lease = owner.record.lease().ok_or("real Claim lease required")?;
    assert_eq!(lease.token.as_bytes(), b"w:12:1");
    assert_eq!(owner.record.fence().get(), 1);
    let fields = [
        ("lease_token", V::Bytes(lease.token.as_bytes())),
        ("fencing_token", V::Integer(1)),
        ("now_ms", V::Integer(13)),
    ];
    let caller = normalize_core_mutation(
        MutationInput {
            operation: "complete",
            id: owner.record.id().as_bytes(),
            fields: &fields,
        },
        &[],
        Some(owner.partition.as_bytes()),
        AdmissionLimits::default(),
        normal().workspace_bytes,
    )?;
    Ok(encode_public_mutation(
        &f::scope()?,
        &RequestId::new(b"complete".to_vec())?,
        &caller,
        f::clock(13, 0, 13)?,
        normal().codec,
        normal().workspace_bytes,
    )?
    .0
    .as_bytes()
    .to_vec())
}

fn publish(
    state: &mut RedbStateStore,
    scope: &RequestScope,
    index: u64,
    body: &[u8],
    generation: u8,
) -> TestResult {
    let profile = normal();
    let mut charge = PolicySchemaCharge::new(profile.schema);
    let RecordBatchTransactionOutcome::Retained { control, outcome } =
        apply_original_policy_set_transaction(
            state,
            scope,
            body,
            f::at(index),
            profile,
            &mut charge,
        )?
    else {
        return Err("original SET not retained".into());
    };
    let time = if generation == 1 { 11 } else { 14 };
    assert_eq!(
        control.last_entry().map(EntryIdentity::log),
        Some(f::at(index))
    );
    assert_eq!(outcome.response(), expected(generation)?);
    assert_eq!(outcome.status(), RetainedStatus::Accepted);
    assert_eq!(
        (outcome.intent().version(), outcome.response_version()),
        (4, 1)
    );
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
    let names = read
        .list_tables()?
        .map(|table| table.name().to_owned())
        .collect::<Vec<_>>();
    assert!(names.iter().all(|name| tables.contains(&name.as_str())));
    assert!(
        tables[..4]
            .iter()
            .all(|name| names.iter().any(|actual| actual == name))
    );
    assert_eq!(
        names.iter().any(|s| s == tables[4]),
        names.iter().any(|s| s == tables[5])
    );
    assert_eq!(read.list_multimap_tables()?.count(), 0);
    let mut result = std::array::from_fn(|_| None);
    for (slot, name) in result.iter_mut().zip(tables) {
        if !names.iter().any(|actual| actual == name) {
            continue;
        }
        let table = read.open_table(TableDefinition::<&[u8], &[u8]>::new(name))?;
        *slot = Some(
            table
                .iter()?
                .map(|row| {
                    let (k, v) = row?;
                    Ok((k.value().to_vec(), v.value().to_vec()))
                })
                .collect::<Result<f::Rows, redb::StorageError>>()?,
        );
    }
    Ok(result)
}

fn reference(dir: &Path) -> TestResult<Vec<Vec<u8>>> {
    let (mut log, mut state) = f::create_pair(dir)?;
    let scope = f::scope()?;
    let member = f::member(EntryStamp::Unstamped, 4)?;
    f::append(&mut log, 0, member.as_bytes())?;
    f::apply_control(&mut state, 0, member.as_bytes())?;
    let mut create = f::create_op();
    let wf_domain::workflow::Operation::Create(value) = &mut create else {
        return Err("Create fixture changed".into());
    };
    value.workflow_type = b"u".to_vec();
    let create = f::frame(5, b"create", 10, vec![(b"i", create)])?;
    f::append(&mut log, 1, create.as_bytes())?;
    drop(f::apply_normal(&mut state, 1, create.as_bytes(), 5)?);
    let first = policy(&scope, b"set", 11, false)?;
    f::append(&mut log, 2, &first)?;
    publish(&mut state, &scope, 2, &first, 1)?;
    let claim = claim()?;
    f::append(&mut log, 3, &claim)?;
    let ClaimTransactionOutcome::Retained { outcome, .. } = apply_claim_transaction(
        &mut state,
        &scope,
        &claim,
        f::at(3),
        ClaimTransactionLimits {
            base: normal(),
            work: ClaimWorkLimits::default(),
        },
    )?
    else {
        return Err("real Claim not retained".into());
    };
    assert_eq!(outcome.status(), RetainedStatus::Accepted);
    let complete = complete(&row(&mut state)?)?;
    f::append(&mut log, 4, &complete)?;
    let RecordBatchTransactionOutcome::Retained { outcome, .. } =
        apply_public_mutation_transaction(&mut state, &scope, &complete, f::at(4), normal())?
    else {
        return Err("real Complete not retained".into());
    };
    assert_eq!(outcome.status(), RetainedStatus::Accepted);
    let replace = policy(&scope, b"replace", 14, true)?;
    f::append(&mut log, 5, &replace)?;
    publish(&mut state, &scope, 5, &replace, 2)?;
    let blank = f::blank(EntryStamp::Unstamped)?;
    f::append(&mut log, 6, blank.as_bytes())?;
    f::apply_control(&mut state, 6, blank.as_bytes())?;
    log.save_committed(f::at(6))?;
    f::append(&mut log, 7, b"uncommitted original policy suffix")?;
    Ok(vec![
        member.as_bytes().to_vec(),
        create.as_bytes().to_vec(),
        first,
        claim,
        complete,
        replace,
        blank.as_bytes().to_vec(),
    ])
}

fn pending(dir: &Path, frames: &[Vec<u8>]) -> TestResult {
    let (mut log, mut state) = f::create_pair(dir)?;
    for (index, frame) in frames.iter().enumerate() {
        f::append(&mut log, u64::try_from(index)?, frame)?;
    }
    f::apply_control(&mut state, 0, &frames[0])?;
    log.save_committed(f::at(6))?;
    f::append(&mut log, 7, b"uncommitted original policy suffix")?;
    Ok(())
}

fn inspect(state: &mut RedbStateStore) -> TestResult {
    let scope = f::scope()?;
    let profile = normal();
    let tx = state.begin_apply(
        scope.group(),
        scope.recovery(),
        profile.codec.admission(),
        1000,
        64 * f::MIB,
        32 * f::MIB,
    )?;
    let current = tx
        .stored_policy_value(
            scope.tenant(),
            b"u",
            profile.schema,
            &mut PolicySchemaCharge::new(profile.schema),
        )?
        .ok_or("original raw policy missing")?;
    let StoredPolicyValue::Original(current) = current else {
        return Err("original ETF authority replaced by V2".into());
    };
    assert_eq!(current, expected(2)?);
    for (request, index, time, generation) in
        [(b"set".as_slice(), 2, 11, 1), (b"replace", 5, 14, 2)]
    {
        let receipt = tx
            .outcome(scope.tenant(), &RequestId::new(request.to_vec())?)?
            .ok_or("original receipt missing")?;
        assert_eq!(receipt.response(), expected(generation)?);
        assert_eq!(receipt.intent().version(), 4);
        assert_eq!(
            (
                receipt.original_position(),
                receipt.original_time().get(),
                receipt.expires_at().get()
            ),
            (f::at(index), time, time + 1000)
        );
    }
    drop(tx);
    let row = row(state)?;
    assert_eq!(
        (
            row.record.workflow_type(),
            row.record.state(),
            row.record.version(),
            row.record.fence().get()
        ),
        (b"u".as_slice(), b"completed".as_slice(), 3, 1)
    );
    assert_eq!(row.record.payload(), Some([0, 255].as_slice()));
    assert!(row.record.lease().is_none());
    assert_eq!(
        (row.ordering.priority, row.ordering.state_enter_seq),
        (0, 1_u128 << 32)
    );
    Ok(())
}

#[test]
fn mixed_original_public_frames_replay_original_outputs() -> TestResult {
    let source = f::directory("original-recovery-reference")?;
    let frames = reference(&source)?;
    let expected = cohort(&source)?;
    assert!(expected.iter().all(Option::is_some));
    let meta = expected[0].as_ref().ok_or("metadata")?;
    assert_eq!(
        &meta.get(b"manifest".as_slice()).ok_or("manifest")?[..6],
        b"FWSS\0\x06"
    );
    assert_eq!(
        &meta.get(b"origin".as_slice()).ok_or("origin")?[..6],
        b"FWSS\0\x03"
    );
    let dir = f::directory("original-recovery-pending")?;
    pending(&dir, &frames)?;
    let logs = (f::log_rows(&dir)?, f::log_meta(&dir)?);
    let scope = f::scope()?;
    for _ in 0..2 {
        let (log, state) = f::open_pair(&dir)?;
        let (log, mut state) =
            recover_committed(log, state, scope.group(), scope.recovery(), caps(normal())?)?;
        inspect(&mut state)?;
        assert_eq!(
            f::applied(&mut state)?
                .0
                .last_entry()
                .map(EntryIdentity::log),
            Some(f::at(6))
        );
        drop((log, state));
        assert_eq!(cohort(&dir)?, expected);
        assert_eq!((f::log_rows(&dir)?, f::log_meta(&dir)?), logs);
    }
    let disabled = super::limits()?.with_claim_work_limits(ClaimWorkLimits::default());
    assert!(!disabled.original_policy_sets());
    let (log, state) = f::open_pair(&source)?;
    assert_eq!(
        recover_committed(log, state, scope.group(), scope.recovery(), disabled).err(),
        Some(Error::UnsupportedEntry)
    );
    drop(f::open_pair(&source)?);
    assert_eq!(cohort(&source)?, expected);
    std::fs::remove_dir_all(dir)?;
    std::fs::remove_dir_all(source)?;
    Ok(())
}
