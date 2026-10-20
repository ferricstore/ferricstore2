//! Independent original ETF owners, configured workspace and cumulative reads.
mod consumed;
use super::{TestResult, fixtures as f};
use std::path::Path;
use wf_domain::policy::complete::{
    OriginalCompletePolicyReplaceApplyError as RawError, PolicySchemaCharge,
    RawFlowPolicyValue as Raw,
};
use wf_domain::workflow::request::{RetainedOutcome, RetainedStatus};
use wf_ports::state::{
    GroupApplyStore, GroupCorePolicyTransaction, GroupOriginalPolicyTransaction,
    StateTransactionError,
};
use wf_storage::transaction::{
    OriginalPolicyTransactionError, RecordBatchTransactionLimits, RecordBatchTransactionOutcome,
    apply_original_policy_set_transaction, with_original_policy_get_reply,
};
use wf_types::identity::RequestId;

pub(super) fn assert_original_hard_length_corrupt(path: &Path, expected: &f::Cohort) -> TestResult {
    use wf_domain::policy::complete::PolicySchemaLimits;
    use wf_storage::state::{RedbStateStore, StateStoreLimits};

    let normal = f::store_limits(64)?;
    let temporary = 32 * 1_048_576;
    let narrow = StateStoreLimits::new(
        normal.admission(),
        PolicySchemaLimits::new(normal.schema().policy_limits(), u64::try_from(temporary)?)?,
        (
            normal.maximum_genesis_rows(),
            normal.maximum_genesis_bytes(),
        ),
        (
            normal.maximum_recovery_rows(),
            normal.maximum_recovery_bytes(),
            normal.maximum_recovery_steps(),
        ),
        temporary,
        normal.workspace_bytes(),
    )?
    .with_history_max_events_limit(64)?
    .with_original_policy_reply_defaults(normal.original_policy_reply_defaults())?;
    let s = f::scope()?;
    // Healthy raw authority is supported under these SAME schema/C limits.
    drop(RedbStateStore::open(
        path,
        s.group().clone(),
        s.recovery().clone(),
        narrow,
    )?);
    let mut invalid = expected.clone();
    let mut oversized = f::etf_policy(0, b"job", &[(b"states", &f::etf_map(&[])?)])?;
    oversized.resize(263_169, 0);
    assert_eq!(65_536 + 256 * oversized.len(), 67_436_800);
    assert!(67_436_800 > temporary);
    f::rows_mut(&mut invalid)?.insert(f::key(6, &[b"g", b"z", b"t", b"job"]), oversized);
    f::upgrade6(
        &mut invalid,
        &s,
        f::catalog(&s, [None, None], None, None, None)?,
    )?;
    f::replace(path, &invalid)?;
    let error = RedbStateStore::open(path, s.group().clone(), s.recovery().clone(), narrow).err();
    assert_eq!(f::cohort(path)?, invalid);
    f::replace(path, expected)?;
    drop(RedbStateStore::open(
        path,
        s.group().clone(),
        s.recovery().clone(),
        narrow,
    )?);
    assert_eq!(error, Some(StateTransactionError::Corrupt));
    assert_eq!(f::cohort(path)?, *expected);
    Ok(())
}

fn input() -> TestResult<(Vec<u8>, Vec<u8>)> {
    let caps = f::limits();
    let s = f::scope()?;
    Ok((
        f::public_frame(
            &s,
            &RequestId::new(b"q".to_vec())?,
            25,
            b"r",
            Raw::Map(&[]),
            Raw::List(&[]),
            &mut PolicySchemaCharge::new(caps.schema),
        )?,
        f::etf_policy(1, b"r", &[(b"states", &f::etf_map(&[])?)])?,
    ))
}

fn expected(
    before: &f::Cohort,
    frame: &[u8],
    at: u64,
    response: &[u8],
) -> TestResult<(f::Cohort, RetainedOutcome)> {
    let mut rows = before.clone();
    let s = f::scope()?;
    let caps = f::limits();
    let metadata = f::metadata(
        &s,
        b"q",
        at,
        25,
        f::frame_intent(frame, at)?,
        RetainedStatus::Accepted,
    )?;
    f::rows_mut(&mut rows)?.insert(f::key(6, &[b"g", b"z", b"t", b"r"]), response.to_vec());
    f::finish(
        &mut rows,
        &s,
        frame,
        f::log(at),
        (f::clock(25)?, f::clock(25)?),
        Some((&metadata, 1, response)),
    )?;
    f::upgrade6(
        &mut rows,
        &s,
        f::catalog(&s, [None, None], None, None, None)?,
    )?;
    Ok((
        rows,
        RetainedOutcome::new(metadata, 1, response.to_vec(), caps.codec.admission())?,
    ))
}

fn refusal(
    path: &Path,
    frame: &[u8],
    at: u64,
    caps: RecordBatchTransactionLimits,
) -> TestResult<OriginalPolicyTransactionError> {
    let before = f::cohort(path)?;
    let s = f::scope()?;
    let mut store = f::open(path, 64)?;
    let result = apply_original_policy_set_transaction(
        &mut store,
        &s,
        frame,
        f::log(at),
        caps,
        &mut PolicySchemaCharge::new(caps.schema),
    );
    let Err(error) = result else {
        return Err("prospective refusal was acknowledged".into());
    };
    let ample = f::limits();
    drop(store.begin_apply(
        s.group(),
        s.recovery(),
        ample.codec.admission(),
        100,
        1_000_000,
        1_000_000,
    )?);
    drop(store);
    assert_eq!(f::cohort(path)?, before);
    drop(f::open(path, 64)?);
    Ok(error)
}

#[test]
fn exact_header_etf_and_reply_owners_are_admitted_before_copy() -> TestResult {
    let dir = f::directory("original-exact-owners")?;
    let path = dir.join("state");
    drop(f::create(&path, &[], 64)?);
    let (frame, response) = input()?;
    assert_eq!(response.len(), 51);
    let mut caps = f::limits();
    let live = caps.maximum_read_bytes + caps.maximum_staging_bytes + caps.maximum_adapter_bytes;
    let pg = usize::try_from(caps.schema.max_work_bytes())?;
    let dh = 2048 + 3 * frame.len();
    let first = live + pg + 65_536 + dh;
    let late = first + 2 * 263_168 + 65_535 + 1024;
    for (maximum, bytes) in [(first - 1, first), (first, late), (late - 1, late)] {
        caps.workspace_bytes = maximum;
        assert_eq!(
            refusal(&path, &frame, 0, caps)?,
            OriginalPolicyTransactionError::Original(RawError::Workspace { bytes, maximum })
        );
    }
    caps = f::limits();
    caps.maximum_staging_bytes = 1024 + 512 + 1 - 1;
    assert_eq!(
        refusal(&path, &frame, 0, caps)?,
        OriginalPolicyTransactionError::Transaction(StateTransactionError::Limit)
    );
    let x = 65_536 + 256 * 51;
    assert_eq!(x, 78_592);
    caps.maximum_staging_bytes = 1024 + 512 + 1 + x - 1;
    assert_eq!(
        refusal(&path, &frame, 0, caps)?,
        OriginalPolicyTransactionError::Transaction(StateTransactionError::Limit)
    );
    caps.maximum_staging_bytes += 1;
    assert_eq!(caps.maximum_staging_bytes, 80_129);
    let live = caps.maximum_read_bytes + caps.maximum_staging_bytes + caps.maximum_adapter_bytes;
    caps.workspace_bytes = live + pg + 65_536 + dh + 2 * 263_168 + 65_535 + 1024;
    let before = f::cohort(&path)?;
    let (wanted, original) = expected(&before, &frame, 0, &response)?;
    let mut store = f::open(&path, 64)?;
    assert!(
        matches!(apply_original_policy_set_transaction(&mut store,&f::scope()?,&frame,f::log(0),caps,&mut PolicySchemaCharge::new(caps.schema))?,
        RecordBatchTransactionOutcome::Retained{outcome,..}if *outcome==original)
    );
    drop(store);
    assert_eq!(f::cohort(&path)?, wanted);
    current_owner(&path, &response)?;
    get_reply_workspace(&path)?;
    std::fs::remove_dir_all(dir)?;
    Ok(())
}

fn current_owner(path: &Path, response: &[u8]) -> TestResult {
    let s = f::scope()?;
    let caps = f::limits();
    let x = 65_536 + 256 * 51;
    let raw = 512 + 51;
    assert_eq!((x, raw, x + raw), (78_592, 563, 79_155));
    let before = f::cohort(path)?;
    for maximum in [x - 1, x, x + raw - 1, x + raw] {
        let mut store = f::open(path, 64)?;
        let mut charge = PolicySchemaCharge::new(caps.schema);
        {
            let tx = store.begin_apply(
                s.group(),
                s.recovery(),
                caps.codec.admission(),
                1000,
                maximum,
                1_000_000,
            )?;
            let result = tx.original_policy_current(s.tenant(), b"r", caps.schema, &mut charge);
            if maximum == x + raw {
                assert_eq!(result?, Some(response.to_vec()));
                assert!(charge.g() > 0);
            } else {
                assert_eq!(result, Err(StateTransactionError::Limit));
            }
        }
        if maximum < x {
            assert_eq!(charge.g(), 0);
        }
        drop(store.begin_apply(
            s.group(),
            s.recovery(),
            caps.codec.admission(),
            100,
            1_000_000,
            1_000_000,
        )?);
        drop(store);
        assert_eq!(f::cohort(path)?, before);
    }
    Ok(())
}

fn get_reply_workspace(path: &Path) -> TestResult {
    let s = f::scope()?;
    let before = f::cohort(path)?;
    let mut caps = f::limits();
    let live = caps.maximum_read_bytes + caps.maximum_staging_bytes + caps.maximum_adapter_bytes;
    let pg = usize::try_from(caps.schema.max_work_bytes())?;
    // Independent P51/n8/k2/depth1, no native state ranks. Raw owner563,
    // index640/catalog192, capacity1 reply metadata520+1024+64=1608.
    let reply = 520 + 1024 + 64;
    let owners = 563 + 64 * (8 + 2) + (128 + 8 * 8) + reply;
    assert_eq!((reply, owners), (1608, 3003));
    let required = live + pg + 65_536 + owners;
    for maximum in [required - 1, required] {
        caps.workspace_bytes = maximum;
        let mut store = f::open(path, 64)?;
        let mut charge = PolicySchemaCharge::new(caps.schema);
        let mut visited = false;
        let result = with_original_policy_get_reply(
            &mut store,
            &s,
            &Raw::Binary(b"r"),
            &Raw::List(&[]),
            caps,
            &mut charge,
            |reply, account| {
                visited = true;
                let reply = reply.map_err(|_| {
                    wf_domain::policy::complete::OriginalPolicyReplyError::Encoding(
                        wf_domain::policy::complete::CompletePolicyEncodingError::Malformed,
                    )
                })?;
                assert!(reply.root().atom_field("states", account)?.is_some());
                Ok(())
            },
        );
        if maximum == required - 1 {
            assert_eq!(
                result,
                Err(OriginalPolicyTransactionError::Original(
                    RawError::Workspace {
                        bytes: required,
                        maximum
                    }
                ))
            );
            assert!(!visited);
        } else {
            result?;
            assert!(visited);
        }
        drop(store);
        assert_eq!(f::cohort(path)?, before);
    }
    Ok(())
}

#[test]
fn actual_replay_temporary_etf_validation_and_output_overlap() -> TestResult {
    let dir = f::directory("original-retained-profile")?;
    let path = dir.join("state");
    drop(f::create(&path, &[], 64)?);
    let (frame, response) = input()?;
    let (mut wanted, original) = expected(&f::cohort(&path)?, &frame, 0, &response)?;
    let caps = f::limits();
    let s = f::scope()?;
    let mut store = f::open(&path, 64)?;
    assert!(
        matches!(f::apply(&mut store,&frame,0,caps,&mut PolicySchemaCharge::new(caps.schema))?,
        RecordBatchTransactionOutcome::Retained{outcome,..}if *outcome==original)
    );
    drop(store);
    assert_eq!(f::cohort(&path)?, wanted);
    let x = 65_536 + 256 * 51;
    let owner = 512 + 1 + 1 + 1 + 1 + 51;
    assert_eq!((x, owner, x + owner), (78_592, 567, 79_159));
    for maximum in [x - 1, x, x + owner - 1, x + owner] {
        let mut store = f::open(&path, 64)?;
        let mut charge = PolicySchemaCharge::new(caps.schema);
        {
            let tx = store.begin_apply(
                s.group(),
                s.recovery(),
                caps.codec.admission(),
                1000,
                maximum,
                1_000_000,
            )?;
            let result = tx.core_policy_outcome(
                s.tenant(),
                &RequestId::new(b"q".to_vec())?,
                caps.schema,
                &mut charge,
            );
            if maximum == x + owner {
                assert_eq!(result?, Some(original.clone()));
            } else {
                assert_eq!(result, Err(StateTransactionError::Limit));
            }
        }
        if maximum < x {
            assert_eq!(charge.g(), 0);
        } else {
            assert!(charge.g() > 0);
        }
        drop(store);
        assert_eq!(f::cohort(&path)?, wanted);
        drop(f::open(&path, 64)?);
    }
    replay_overlap(&path, &frame, &original, &mut wanted)?;
    cross_operation(&path, &mut wanted)?;
    std::fs::remove_dir_all(dir)?;
    Ok(())
}

fn replay_overlap(
    path: &Path,
    frame: &[u8],
    original: &RetainedOutcome,
    wanted: &mut f::Cohort,
) -> TestResult {
    let s = f::scope()?;
    let mut caps = f::limits();
    let x = 65_536 + 256 * 51;
    let owner = 567;
    caps.maximum_read_bytes = 1024 + x + owner - 1;
    caps.maximum_staging_bytes = 1024 + x;
    caps.maximum_adapter_bytes = 1_000_000;
    let live = caps.maximum_read_bytes + caps.maximum_staging_bytes + caps.maximum_adapter_bytes;
    let pg = usize::try_from(caps.schema.max_work_bytes())?;
    let dh = 2048 + 3 * frame.len();
    caps.workspace_bytes = live + pg + 65_536 + dh;
    assert_eq!(
        refusal(path, frame, 1, caps)?,
        OriginalPolicyTransactionError::Transaction(StateTransactionError::Limit)
    );
    caps.maximum_read_bytes += 1;
    caps.workspace_bytes += 1;
    assert_eq!(caps.maximum_read_bytes, 80_183);
    let mut store = f::open(path, 64)?;
    let mut charge = PolicySchemaCharge::new(caps.schema);
    assert!(
        matches!(f::apply(&mut store,frame,1,caps,&mut charge)?,RecordBatchTransactionOutcome::Retained{outcome,..}if *outcome==*original)
    );
    assert!(charge.g() > 0);
    f::finish(
        wanted,
        &s,
        frame,
        f::log(1),
        (f::clock(25)?, f::clock(25)?),
        None,
    )?;
    drop(store);
    assert_eq!(f::cohort(path)?, *wanted);
    drop(f::open(path, 64)?);
    Ok(())
}

fn cross_operation(path: &Path, wanted: &mut f::Cohort) -> TestResult {
    use wf_domain::claim::apply::ClaimWorkLimits;
    use wf_storage::transaction::{
        ClaimTransactionLimits, ClaimTransactionOutcome, apply_claim_transaction,
        apply_record_batch_transaction,
    };
    let s = f::scope()?;
    let item = wf_ports::consensus::PreparedBatchItem {
        partition: wf_types::identity::PartitionId::new(b"p".to_vec())?,
        create_priority: 0,
        command: wf_domain::workflow::CommittedCommand {
            id: wf_types::identity::RecordId::new(b"never".to_vec())?,
            now: wf_types::version::TimestampMillis::new(26),
            operation: wf_domain::workflow::Operation::Create(wf_domain::workflow::Create {
                workflow_type: b"r".to_vec(),
                state: b"q".to_vec(),
                run_at: Some(wf_types::version::TimestampMillis::new(26)),
                payload: None,
            }),
        },
    };
    let batch = f::batch(&s, b"q", 26, vec![item])?;
    let mut store = f::open(path, 64)?;
    assert!(matches!(
        apply_record_batch_transaction(&mut store, &s, batch.as_bytes(), f::log(2), f::limits())?,
        RecordBatchTransactionOutcome::IntentConflict(_)
    ));
    f::finish(
        wanted,
        &s,
        batch.as_bytes(),
        f::log(2),
        (f::clock(26)?, f::clock(26)?),
        None,
    )?;
    drop(store);
    assert_eq!(f::cohort(path)?, *wanted);
    let claim = f::claim_frame(&s, b"q", 27, b"r", b"w", None, None)?;
    store = f::open(path, 64)?;
    assert!(matches!(
        apply_claim_transaction(
            &mut store,
            &s,
            &claim,
            f::log(3),
            ClaimTransactionLimits {
                base: f::limits(),
                work: ClaimWorkLimits::default()
            }
        )?,
        ClaimTransactionOutcome::IntentConflict(_)
    ));
    f::finish(
        wanted,
        &s,
        &claim,
        f::log(3),
        (f::clock(27)?, f::clock(27)?),
        None,
    )?;
    drop(store);
    assert_eq!(f::cohort(path)?, *wanted);
    drop(f::open(path, 64)?);
    Ok(())
}
