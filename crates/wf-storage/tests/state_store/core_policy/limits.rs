//! Independent owner/workspace equations and cumulative real physical traversal.

mod consumed;

use super::{TestResult, fixtures as f};
use std::path::Path;
use wf_domain::policy::complete::{
    CompleteCorePolicyPatchApplyError, PolicySchemaCharge, PolicySchemaError, PolicySchemaLimits,
};
use wf_domain::policy::definition::StateMode;
use wf_domain::policy::installation::PolicyLimits;
use wf_domain::workflow::admission::AdmissionLimits;
use wf_domain::workflow::request::{RequestScope, RetainedStatus};
use wf_ports::snapshot::{SnapshotError, SnapshotLimits, StateSnapshotStore};
use wf_ports::state::{
    GroupApplyStore, GroupApplyTransaction, GroupCorePolicyTransaction, StateTransactionError,
};
use wf_storage::snapshot::RedbSnapshotStage;
use wf_storage::state::{RedbStateStore, StateStoreLimits};
use wf_storage::transaction::{
    ClaimTransactionLimits, ClaimTransactionOutcome, CorePolicyTransactionError,
    RecordBatchTransactionLimits, RecordBatchTransactionOutcome, apply_claim_transaction,
    apply_complete_core_policy_patch_transaction, apply_record_batch_transaction,
};
use wf_types::identity::{RequestId, TenantId};

fn refusal(
    path: &Path,
    frame: &[u8],
    at: u64,
    caps: RecordBatchTransactionLimits,
) -> TestResult<CorePolicyTransactionError> {
    let s = f::scope()?;
    let before = f::cohort(path)?;
    let mut store = f::open(path, &s)?;
    let Err(error) =
        apply_complete_core_policy_patch_transaction(&mut store, &s, frame, f::log(at), caps)
    else {
        return Err("prospective resource refusal was acknowledged".into());
    };
    drop(store.begin_apply(
        s.group(),
        s.recovery(),
        f::limits().codec.admission(),
        100,
        1_000_000,
        1_000_000,
    )?);
    drop(store);
    assert_eq!(f::cohort(path)?, before);
    drop(f::open(path, &s)?);
    Ok(error)
}

fn expected(path: &Path, s: &RequestScope, frame: &[u8], at: u64) -> TestResult<f::Cohort> {
    let mut expected = f::cohort(path)?;
    let response = f::policy_wire(b"job", 1, &[(b"q", StateMode::Fifo)], false)?;
    f::rows_mut(&mut expected)?.insert(f::key(6, &[b"g", b"z", b"t", b"job"]), response.clone());
    let metadata = f::metadata(
        s,
        b"q",
        at,
        25,
        f::frame_intent(frame, at)?,
        RetainedStatus::Accepted,
    )?;
    f::finish(
        &mut expected,
        s,
        frame,
        f::log(at),
        (f::clock(25)?, f::clock(25)?),
        Some((&metadata, 1, &response)),
    )?;
    f::upgrade5(
        &mut expected,
        s,
        f::catalog(s, [None, None], None, None, None)?,
    )?;
    Ok(expected)
}

#[test]
fn canonical_and_catalog_work_limits_abort_complete_publication() -> TestResult {
    let dir = f::directory("policy-physical-limits")?;
    let path = dir.join("canonical");
    let s = f::scope()?;
    let mut store = f::create(&path, &s, &[b"p"], &[])?;
    let mut owners = Vec::new();
    for ordinal in 0_u32..81 {
        let id = format!("r-{ordinal:03}");
        let mut data = f::data(id.as_bytes(), b"foreign", b"q", 0, Some(0))?;
        if ordinal == 80 {
            data.payload = Some(vec![0x5a; 131_072]);
        }
        owners.push(f::owner(&s, b"p", data, 0, u128::from(ordinal))?);
    }
    f::seed(&mut store, &s, &owners, 0, 0)?;
    drop(store);
    let frame = f::frame(
        &s,
        b"q",
        25,
        b"job",
        Some(0),
        f::mode_patch(&[(b"q", Some(StateMode::Fifo))]),
    )?;
    // All earlier 80 canonical rows cost 80*(256+K26+R527)=64,720.
    // The final unrelated record alone owns 512+t1+p1+id5+type7+state1+payload131072.
    // A narrow type is not permission to skip that real canonical body.
    let final_owner = 512 + 1 + 1 + 5 + 7 + 1 + 131_072;
    assert_eq!(final_owner, 131_599);
    let mut caps = f::limits();
    caps.maximum_read_bytes = final_owner - 1;
    assert_eq!(
        refusal(&path, frame.as_bytes(), 1, caps)?,
        CorePolicyTransactionError::Transaction(StateTransactionError::Limit)
    );
    caps = f::limits();
    caps.maximum_read_steps = 12;
    assert_eq!(
        refusal(&path, frame.as_bytes(), 1, caps)?,
        CorePolicyTransactionError::Transaction(StateTransactionError::Limit)
    );
    let wanted = expected(&path, &s, frame.as_bytes(), 1)?;
    f::apply(
        &path,
        &s,
        frame.as_bytes(),
        1,
        &wanted,
        &f::policy_wire(b"job", 1, &[(b"q", StateMode::Fifo)], false)?,
    )?;
    catalog_passes(&dir)?;
    std::fs::remove_dir_all(dir)?;
    Ok(())
}

fn catalog_passes(dir: &Path) -> TestResult {
    let path = dir.join("catalog");
    let s = f::scope()?;
    let kind = vec![b'u'; 131_072];
    let tenant = TenantId::new(b"other".to_vec())?;
    let policy = f::policy(&kind, 1, &[], false)?;
    let wire = policy.encoded().to_vec();
    drop(f::create(&path, &s, &[b"p"], &[(tenant.clone(), policy)])?);
    let physical = f::key(6, &[b"g", b"z", tenant.as_bytes(), &kind]);
    assert_eq!((physical.len(), wire.len()), (131_094, 131_111));
    let frame = f::frame(
        &s,
        b"q",
        25,
        b"job",
        Some(0),
        f::mode_patch(&[(b"q", Some(StateMode::Fifo))]),
    )?;
    let full_wire = physical.len() + wire.len();
    // Each all6/7 pass pays the full escaped K/V inspection and owning grammar
    // decoder buffer before parse. Seek/bounds/metadata/output costs are extra.
    let one_census_floor = full_wire + 256 + physical.len();
    for maximum in [full_wire - 1, 2 * one_census_floor - 1] {
        let mut caps = f::limits();
        caps.maximum_adapter_bytes = maximum;
        assert_eq!(
            refusal(&path, frame.as_bytes(), 0, caps)?,
            CorePolicyTransactionError::Transaction(StateTransactionError::Limit)
        );
    }
    let before = f::cohort(&path)?;
    let wanted = expected(&path, &s, frame.as_bytes(), 0)?;
    f::apply(
        &path,
        &s,
        frame.as_bytes(),
        0,
        &wanted,
        &f::policy_wire(b"job", 1, &[(b"q", StateMode::Fifo)], false)?,
    )?;
    assert_eq!(
        f::rows(&wanted)?.get(&physical),
        f::rows(&before)?.get(&physical)
    );
    Ok(())
}

#[test]
fn policy_owners_lane_slots_schema_and_workspace_are_prospectively_admitted() -> TestResult {
    let dir = f::directory("policy-exact-owners")?;
    let path = dir.join("state");
    let s = f::scope()?;
    drop(f::create(&path, &s, &[b"p"], &[])?);
    let frame = f::frame(
        &s,
        b"q",
        25,
        b"job",
        Some(0),
        f::mode_patch(&[(b"q", Some(StateMode::Fifo))]),
    )?;
    assert_eq!(frame.as_bytes().len(), 155);
    let dh = 2048 + 3 * 155;
    let d = 4096 + 4 * 155 + 2048;
    assert_eq!((dh, d), (2513, 6764));
    let mut caps = f::limits();
    let live = caps.maximum_read_bytes + caps.maximum_staging_bytes + caps.maximum_adapter_bytes;
    let pg = usize::try_from(caps.schema.max_work_bytes())?;
    let late = live + pg + d + 56_846_336 + 263_168 + 1024;
    for (maximum, attempted) in [
        (live + pg + dh - 1, live + pg + dh),
        (live + pg + dh, live + pg + d),
        (live + pg + d, late),
        (late - 1, late),
    ] {
        caps.workspace_bytes = maximum;
        assert_eq!(
            refusal(&path, frame.as_bytes(), 0, caps)?,
            CorePolicyTransactionError::Apply(CompleteCorePolicyPatchApplyError::Workspace {
                bytes: attempted,
                maximum
            })
        );
    }
    caps = f::limits();
    caps.maximum_staging_bytes = 1024 + 512 + 3 - 1; // Query copy must be prepaid.
    assert_eq!(
        refusal(&path, frame.as_bytes(), 0, caps)?,
        CorePolicyTransactionError::Transaction(StateTransactionError::Limit)
    );
    caps.maximum_staging_bytes = 1024 + 512 + 3 + (2048 + 216 * 52) - 1;
    assert_eq!(
        refusal(&path, frame.as_bytes(), 0, caps)?,
        CorePolicyTransactionError::Transaction(StateTransactionError::Limit)
    );
    schema_cut(&path, frame.as_bytes())?;
    // Exact positive S includes query separately from the actual complete-value
    // validation owner; fixed candidate Umax and distinct output Omax stay in W.
    caps.maximum_staging_bytes += 1;
    assert_eq!(caps.maximum_staging_bytes, 14_819);
    let live = caps.maximum_read_bytes + caps.maximum_staging_bytes + caps.maximum_adapter_bytes;
    caps.workspace_bytes = live + pg + d + 56_846_336 + 263_168 + 1024;
    let wanted = expected(&path, &s, frame.as_bytes(), 0)?;
    let mut store = f::open(&path, &s)?;
    assert!(
        matches!(apply_complete_core_policy_patch_transaction(&mut store, &s, frame.as_bytes(), f::log(0), caps)?,
        RecordBatchTransactionOutcome::Retained { outcome, .. } if outcome.response() == f::policy_wire(b"job", 1, &[(b"q", StateMode::Fifo)], false)?)
    );
    drop(store);
    assert_eq!(f::cohort(&path)?, wanted);
    drop(f::open(&path, &s)?);
    consumed::assert_prefix(&dir)?;
    std::fs::remove_dir_all(dir)?;
    Ok(())
}

fn schema_cut(path: &Path, frame: &[u8]) -> TestResult {
    let mut caps = f::limits();
    caps.schema = PolicySchemaLimits::new(PolicyLimits::default(), 1)?;
    assert_eq!(
        refusal(path, frame, 0, caps)?,
        CorePolicyTransactionError::Apply(CompleteCorePolicyPatchApplyError::Request(
            PolicySchemaError::WorkBytes {
                bytes: 128,
                maximum: 1
            }
            .into()
        ))
    );
    Ok(())
}

#[test]
fn intent2_retained_points_refuse_narrow_profiles_without_fencing() -> TestResult {
    let dir = f::directory("policy-retained-profile")?;
    let path = dir.join("state");
    let s = f::scope()?;
    drop(f::create(&path, &s, &[b"p"], &[])?);
    let frame = f::frame(
        &s,
        b"q",
        25,
        b"job",
        Some(0),
        f::mode_patch(&[(b"q", Some(StateMode::Fifo))]),
    )?;
    let mut wanted = expected(&path, &s, frame.as_bytes(), 0)?;
    let response = f::policy_wire(b"job", 1, &[(b"q", StateMode::Fifo)], false)?;
    let original = f::apply(&path, &s, frame.as_bytes(), 0, &wanted, &response)?;
    assert_eq!(response.len(), 52);
    let temporary = 2048 + 216 * 52;
    let retained = 512 + 1 + 1 + 1 + 1 + 52;
    assert_eq!((temporary, retained), (13_280, 568));
    for maximum in [
        temporary - 1,
        temporary,
        temporary + retained - 1,
        temporary + retained,
    ] {
        let mut store = f::open(&path, &s)?;
        let mut charge = PolicySchemaCharge::new(f::limits().schema);
        {
            let tx = store.begin_apply(
                s.group(),
                s.recovery(),
                f::limits().codec.admission(),
                1000,
                maximum,
                1_000_000,
            )?;
            let result = tx.core_policy_outcome(
                s.tenant(),
                &RequestId::new(b"q".to_vec())?,
                f::limits().schema,
                &mut charge,
            );
            if maximum == temporary + retained {
                assert_eq!(result?, Some(*original.clone()));
            } else {
                assert_eq!(result, Err(StateTransactionError::Limit));
            }
        }
        if maximum < temporary {
            assert_eq!(charge.g(), 0);
        } else {
            assert!(charge.g() > 0);
        }
        drop(store.begin_apply(
            s.group(),
            s.recovery(),
            f::limits().codec.admission(),
            100,
            1_000_000,
            1_000_000,
        )?);
        drop(store);
        assert_eq!(f::cohort(&path)?, wanted);
    }
    narrow_read(&path)?;
    cross_operation(&path, &mut wanted)?;
    narrow_snapshot(&dir, &path)?;
    std::fs::remove_dir_all(dir)?;
    Ok(())
}

fn narrow_read(path: &Path) -> TestResult {
    let s = f::scope()?;
    let mut store = f::open(path, &s)?;
    let narrow = AdmissionLimits::new(4096, 51, 1000)?;
    {
        let tx = store.begin_apply(s.group(), s.recovery(), narrow, 1000, 1_000_000, 1_000_000)?;
        assert_eq!(
            tx.core_policy_outcome(
                s.tenant(),
                &RequestId::new(b"q".to_vec())?,
                f::limits().schema,
                &mut PolicySchemaCharge::new(f::limits().schema)
            ),
            Err(StateTransactionError::Limit)
        );
        assert!(tx.control()?.last_entry().is_some());
    }
    drop(store);
    Ok(())
}

fn cross_operation(path: &Path, wanted: &mut f::Cohort) -> TestResult {
    let s = f::scope()?;
    let batch = super::super::fixtures::frame(
        b"q",
        26,
        vec![(b"never", super::super::fixtures::create_op())],
    )?;
    f::finish(
        wanted,
        &s,
        batch.as_bytes(),
        f::log(1),
        (f::clock(26)?, f::clock(26)?),
        None,
    )?;
    let mut store = f::open(path, &s)?;
    assert!(matches!(
        apply_record_batch_transaction(&mut store, &s, batch.as_bytes(), f::log(1), f::limits())?,
        RecordBatchTransactionOutcome::IntentConflict(_)
    ));
    drop(store);
    assert_eq!(f::cohort(path)?, *wanted);
    let claim = f::claim_frame(&s, b"q", 27, b"job", b"w")?;
    f::finish(
        wanted,
        &s,
        &claim,
        f::log(2),
        (f::clock(27)?, f::clock(27)?),
        None,
    )?;
    let mut store = f::open(path, &s)?;
    assert!(matches!(
        apply_claim_transaction(
            &mut store,
            &s,
            &claim,
            f::log(2),
            ClaimTransactionLimits {
                base: f::limits(),
                work: wf_domain::claim::apply::ClaimWorkLimits::default()
            }
        )?,
        ClaimTransactionOutcome::IntentConflict(_)
    ));
    drop(store);
    assert_eq!(f::cohort(path)?, *wanted);
    drop(f::open(path, &s)?);
    Ok(())
}

fn narrow_snapshot(dir: &Path, path: &Path) -> TestResult {
    let s = f::scope()?;
    let mut store = f::open(path, &s)?;
    let mut bytes = Vec::new();
    store.capture_snapshot(&mut bytes, SnapshotLimits::default())?;
    drop(store);
    let schema = f::limits().schema;
    let target = StateStoreLimits::new(
        AdmissionLimits::new(4096, 51, 1000)?,
        schema,
        (1000, 128 * 1_048_576),
        (1_000_000, 1_073_741_824, 10_000_000),
        128 * 1_048_576,
        256 * 1_048_576,
    )?;
    assert!(matches!(
        RedbStateStore::open(path, s.group().clone(), s.recovery().clone(), target),
        Err(StateTransactionError::Limit)
    ));
    assert!(matches!(
        RedbSnapshotStage::receive(
            &dir.join("narrow-stage"),
            s.group().clone(),
            s.recovery().clone(),
            &mut std::io::Cursor::new(&bytes),
            target,
            SnapshotLimits::default()
        ),
        Err(SnapshotError::Stage(StateTransactionError::Limit))
    ));
    drop(f::open(path, &s)?);
    Ok(())
}
