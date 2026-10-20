//! Original intent2 receipts gate every changed-current and publication phase.

use super::{TestResult, fixtures as f};
use std::path::Path;
use wf_domain::policy::complete::CompleteCorePolicyPatchApplyError;
use wf_domain::policy::definition::{FieldPatch, PolicyPatch, StateMode};
use wf_domain::policy::installation::{MAX_POLICY_GENERATION, PolicyError};
use wf_domain::retention::RetentionOverride;
use wf_domain::workflow::request::{
    PolicyRejection, RequestScope, RetainedOutcome, RetainedStatus,
};
use wf_ports::state::{GroupApplyStore, StateTransactionError};
use wf_storage::transaction::{
    CorePolicyTransactionError, RecordBatchTransactionOutcome,
    apply_complete_core_policy_patch_transaction,
};
use wf_types::version::TimestampMillis;

fn accepted(path: &Path, s: &RequestScope) -> TestResult<(f::Cohort, Box<RetainedOutcome>)> {
    drop(f::create(path, s, &[b"p"], &[])?);
    let frame = f::frame(
        s,
        b"original",
        100,
        b"job",
        Some(0),
        f::mode_patch(&[(b"queued", Some(StateMode::Fifo))]),
    )?;
    let response = f::policy_wire(b"job", 1, &[(b"queued", StateMode::Fifo)], false)?;
    let mut expected = f::cohort(path)?;
    f::rows_mut(&mut expected)?.insert(f::key(6, &[b"g", b"z", b"t", b"job"]), response.clone());
    let metadata = f::metadata(
        s,
        b"original",
        0,
        100,
        f::frame_intent(frame.as_bytes(), 0)?,
        RetainedStatus::Accepted,
    )?;
    f::finish(
        &mut expected,
        s,
        frame.as_bytes(),
        f::log(0),
        (f::clock(100)?, f::clock(100)?),
        Some((&metadata, 1, &response)),
    )?;
    f::upgrade5(
        &mut expected,
        s,
        f::catalog(s, [None, None], None, None, None)?,
    )?;
    let original = f::apply(path, s, frame.as_bytes(), 0, &expected, &response)?;
    assert_eq!(original.original_time(), TimestampMillis::new(100));
    assert_eq!(original.expires_at(), TimestampMillis::new(1100));
    Ok((expected, original))
}

#[test]
fn retained_core_patch_gate_skips_current_and_reindex_work() -> TestResult {
    let dir = f::directory("policy-retained")?;
    let path = dir.join("state");
    let s = f::scope()?;
    let (mut expected, original) = accepted(&path, &s)?;
    let replay = f::frame(
        &s,
        b"original",
        200,
        b"job",
        Some(0),
        f::mode_patch(&[(b"queued", Some(StateMode::Fifo))]),
    )?;
    // Original CAS is now stale. Fresh decode/current/reindex cannot fit this W.
    let mut caps = f::limits();
    let owner = 2048 + 216 * 57;
    assert_eq!(original.response().len(), 57);
    caps.maximum_read_bytes = 1024 + owner + 512 + 11 + 57;
    caps.maximum_staging_bytes = 1024 + owner;
    let live = caps.maximum_read_bytes + caps.maximum_staging_bytes + caps.maximum_adapter_bytes;
    caps.workspace_bytes =
        live + usize::try_from(caps.schema.max_work_bytes())? + 2048 + 3 * replay.as_bytes().len();
    f::finish(
        &mut expected,
        &s,
        replay.as_bytes(),
        f::log(1),
        (f::clock(200)?, f::clock(200)?),
        None,
    )?;
    let mut store = f::open(&path, &s)?;
    let RecordBatchTransactionOutcome::Retained { outcome, .. } =
        apply_complete_core_policy_patch_transaction(
            &mut store,
            &s,
            replay.as_bytes(),
            f::log(1),
            caps,
        )?
    else {
        return Err("retained policy did not replay under header-only W".into());
    };
    assert_eq!(*outcome, *original);
    drop(store);
    assert_eq!(f::cohort(&path)?, expected);
    conflict_expiry(&path, &s, &mut expected, caps)?;
    let fresh = f::frame(&s, b"fresh", 1101, b"job", Some(1), PolicyPatch::default())?;
    let before = f::cohort(&path)?;
    let mut store = f::open(&path, &s)?;
    assert!(matches!(
        apply_complete_core_policy_patch_transaction(
            &mut store,
            &s,
            fresh.as_bytes(),
            f::log(4),
            caps
        ),
        Err(CorePolicyTransactionError::Apply(
            CompleteCorePolicyPatchApplyError::Workspace { .. }
        ))
    ));
    drop(store.begin_apply(
        s.group(),
        s.recovery(),
        caps.codec.admission(),
        100,
        1_000_000,
        1_000_000,
    )?);
    drop(store);
    assert_eq!(f::cohort(&path)?, before);
    drop(f::open(&path, &s)?);
    std::fs::remove_dir_all(dir)?;
    Ok(())
}

fn conflict_expiry(
    path: &Path,
    s: &RequestScope,
    expected: &mut f::Cohort,
    caps: wf_storage::transaction::RecordBatchTransactionLimits,
) -> TestResult {
    let conflict = f::frame(
        s,
        b"original",
        201,
        b"job",
        Some(0),
        f::mode_patch(&[(b"queued", Some(StateMode::Parallel))]),
    )?;
    f::finish(
        expected,
        s,
        conflict.as_bytes(),
        f::log(2),
        (f::clock(201)?, f::clock(201)?),
        None,
    )?;
    let mut store = f::open(path, s)?;
    assert!(matches!(
        apply_complete_core_policy_patch_transaction(
            &mut store,
            s,
            conflict.as_bytes(),
            f::log(2),
            caps
        )?,
        RecordBatchTransactionOutcome::IntentConflict(_)
    ));
    drop(store);
    assert_eq!(f::cohort(path)?, *expected);
    let expired = f::frame(
        s,
        b"original",
        1100,
        b"job",
        Some(0),
        f::mode_patch(&[(b"queued", Some(StateMode::Fifo))]),
    )?;
    f::finish(
        expected,
        s,
        expired.as_bytes(),
        f::log(3),
        (f::clock(1100)?, f::clock(1100)?),
        None,
    )?;
    let mut store = f::open(path, s)?;
    assert!(matches!(
        apply_complete_core_policy_patch_transaction(
            &mut store,
            s,
            expired.as_bytes(),
            f::log(3),
            caps
        )?,
        RecordBatchTransactionOutcome::Expired(_)
    ));
    assert!(matches!(
        apply_complete_core_policy_patch_transaction(
            &mut store,
            s,
            expired.as_bytes(),
            f::log(3),
            caps
        )?,
        RecordBatchTransactionOutcome::LogReplay(_)
    ));
    drop(store);
    assert_eq!(f::cohort(path)?, *expected);
    drop(f::open(path, s)?);
    Ok(())
}

#[test]
fn core_patch_business_rejection_advances_only_original_outcome_and_control() -> TestResult {
    let dir = f::directory("policy-rejections")?;
    let path = dir.join("state");
    let s = f::scope()?;
    drop(f::create(&path, &s, &[b"p"], &[])?);
    let mut expected = f::cohort(&path)?;
    let meta = expected[0].clone();
    let stale = f::frame(&s, b"stale", 100, b"job", Some(1), PolicyPatch::default())?;
    let stale_reason = PolicyRejection::new(PolicyError::StaleGeneration)?;
    let metadata = f::metadata(
        &s,
        b"stale",
        0,
        100,
        f::frame_intent(stale.as_bytes(), 0)?,
        RetainedStatus::PolicyRejected(stale_reason),
    )?;
    let body = b"ERR stale flow policy generation";
    f::finish(
        &mut expected,
        &s,
        stale.as_bytes(),
        f::log(0),
        (f::clock(100)?, f::clock(100)?),
        Some((&metadata, 1, body)),
    )?;
    let rejected = f::apply(&path, &s, stale.as_bytes(), 0, &expected, body)?;
    assert_eq!(
        rejected.status(),
        RetainedStatus::PolicyRejected(stale_reason)
    );
    let history = f::frame(
        &s,
        b"history",
        101,
        b"job",
        Some(0),
        PolicyPatch {
            retention: FieldPatch::Set(RetentionOverride {
                ttl_ms: None,
                history_max_events: Some(65),
            }),
            ..PolicyPatch::default()
        },
    )?;
    let reason = PolicyRejection::new(PolicyError::Retention(
        wf_domain::retention::RetentionError::History,
    ))?;
    let metadata = f::metadata(
        &s,
        b"history",
        1,
        101,
        f::frame_intent(history.as_bytes(), 1)?,
        RetainedStatus::PolicyRejected(reason),
    )?;
    let body = b"ERR flow retention history_max_events is outside configured bounds";
    f::finish(
        &mut expected,
        &s,
        history.as_bytes(),
        f::log(1),
        (f::clock(101)?, f::clock(101)?),
        Some((&metadata, 1, body)),
    )?;
    assert_eq!(
        f::apply(&path, &s, history.as_bytes(), 1, &expected, body)?.status(),
        RetainedStatus::PolicyRejected(reason)
    );
    assert_eq!(
        expected[0], meta,
        "business rejection must not upgrade the live manifest"
    );
    assert!(expected[4].is_none() && expected[5].is_none());
    assert!(matches!(
        f::store_limits()?.with_history_max_events_limit(0),
        Err(StateTransactionError::InvalidLimits)
    ));
    generation_exhaustion(&dir)?;
    std::fs::remove_dir_all(dir)?;
    Ok(())
}

fn generation_exhaustion(dir: &Path) -> TestResult {
    let path = dir.join("exhausted");
    let s = f::scope()?;
    drop(f::create(
        &path,
        &s,
        &[b"p"],
        &[(
            s.tenant().clone(),
            f::policy(b"job", MAX_POLICY_GENERATION, &[], false)?,
        )],
    )?);
    let before = f::cohort(&path)?;
    let mut expected = before.clone();
    let frame = f::frame(
        &s,
        b"max",
        100,
        b"job",
        Some(MAX_POLICY_GENERATION),
        PolicyPatch::default(),
    )?;
    let body = b"ERR flow policy generation exhausted";
    let reason = PolicyRejection::new(PolicyError::GenerationExhausted)?;
    let metadata = f::metadata(
        &s,
        b"max",
        0,
        100,
        f::frame_intent(frame.as_bytes(), 0)?,
        RetainedStatus::PolicyRejected(reason),
    )?;
    f::finish(
        &mut expected,
        &s,
        frame.as_bytes(),
        f::log(0),
        (f::clock(100)?, f::clock(100)?),
        Some((&metadata, 1, body)),
    )?;
    f::apply(&path, &s, frame.as_bytes(), 0, &expected, body)?;
    assert_eq!(expected[0], before[0]);
    assert_eq!(
        f::rows(&expected)?.get(&f::key(6, &[b"g", b"z", b"t", b"job"])),
        f::rows(&before)?.get(&f::key(6, &[b"g", b"z", b"t", b"job"]))
    );
    Ok(())
}
