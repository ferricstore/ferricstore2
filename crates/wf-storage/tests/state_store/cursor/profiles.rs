//! Constructor and consumed-publication profile boundaries on a real backend.

use std::collections::BTreeMap;
use std::path::Path;

use wf_domain::claim::ScopedRecordId;
use wf_domain::policy::complete::{
    PolicySchemaCharge, PolicySchemaLimits, decode_complete_policy_value_v2,
};
use wf_domain::policy::installation::PolicyLimits;
use wf_domain::workflow::admission::AdmissionLimits;
use wf_domain::workflow::control::ControlState;
use wf_domain::workflow::indexed::TouchedIndexedGroupView;
use wf_domain::workflow::request::gate::PointGateView;
use wf_domain::workflow::request::{RequestScope, RetainedStatus};
use wf_ports::log::{
    RecordBatchInspection, decide_fresh_independent_record_batch_touched,
    inspect_independent_record_batch_point, prepare_independent_record_batch,
    record_batch_retained_summary,
};
use wf_ports::state::{
    GroupApplyChanges, GroupApplyStore, GroupApplyTransaction, StateTransactionError,
};
use wf_storage::state::{RedbStateStore, StateStoreGenesis, StateStoreLimits};
use wf_types::identity::PartitionId;

use super::super::{TestResult, fixtures};

pub(super) fn genesis_policy_shape_profile(dir: &Path) -> TestResult {
    let scope = fixtures::scope()?;
    let schema = PolicySchemaLimits::default();
    let mut wire = b"FWPV\0\x02".to_vec();
    wire.extend_from_slice(&7_u64.to_be_bytes());
    wire.extend_from_slice(&40_u32.to_be_bytes());
    wire.extend_from_slice(b"FWPS\0\x02");
    wire.extend_from_slice(&1_u32.to_be_bytes());
    wire.push(b'r');
    wire.extend_from_slice(&[0; 4]);
    wire.extend_from_slice(&2_u32.to_be_bytes());
    for state in *b"ab" {
        wire.extend_from_slice(&1_u32.to_be_bytes());
        wire.push(state);
        wire.extend_from_slice(&[0; 4]);
    }
    wire.extend_from_slice(&[0; 3]);
    assert_eq!(wire.len(), 58);
    let policy =
        decode_complete_policy_value_v2(&wire, schema, &mut PolicySchemaCharge::new(schema))?;
    assert_eq!(policy.data().states.len(), 2);
    let policies = [(scope.tenant().clone(), policy)];
    let schema = PolicySchemaLimits::new(PolicyLimits::new(1)?, schema.max_work_bytes())?;
    let limits = StateStoreLimits::new(
        AdmissionLimits::default(),
        schema,
        (1_000, 128 * fixtures::MIB),
        (1_000_000, 1_024 * fixtures::MIB, 10_000_000),
        128 * fixtures::MIB,
        256 * fixtures::MIB,
    )?;
    let path = dir.join("policy-shape.refused");
    assert!(matches!(
        RedbStateStore::create(
            &path,
            scope.group().clone(),
            scope.recovery().clone(),
            StateStoreGenesis {
                initial_time: fixtures::clock(0)?,
                placements: &[],
                policies: &policies
            },
            limits,
        ),
        Err(StateTransactionError::Limit)
    ));
    assert!(!path.exists());
    Ok(())
}

pub(super) fn outcome_count_profile(dir: &Path) -> TestResult {
    let scope = fixtures::scope()?;
    let path = dir.join("outcome-count.redb");
    let admission = AdmissionLimits::new(fixtures::MIB, 16 * fixtures::MIB, 1)?;
    let limits = StateStoreLimits::new(
        admission,
        PolicySchemaLimits::default(),
        (1_000, 128 * fixtures::MIB),
        (1_000_000, 1_024 * fixtures::MIB, 10_000_000),
        128 * fixtures::MIB,
        256 * fixtures::MIB,
    )?;
    let placements = fixtures::placements()?;
    drop(RedbStateStore::create(
        &path,
        scope.group().clone(),
        scope.recovery().clone(),
        StateStoreGenesis {
            initial_time: fixtures::clock(0)?,
            placements: &placements,
            policies: &[],
        },
        limits,
    )?);
    let rows = fixtures::raw_rows(&path)?;
    let manifest = fixtures::raw_manifest(&path)?;
    let changes = counted_changes(&scope)?;
    let mut store = RedbStateStore::open(
        &path,
        scope.group().clone(),
        scope.recovery().clone(),
        limits,
    )?;
    let tx = store.begin_apply(
        scope.group(),
        scope.recovery(),
        admission,
        100,
        fixtures::MIB,
        fixtures::MIB,
    )?;
    assert_eq!(tx.commit(&changes), Err(StateTransactionError::Limit));
    drop(store);
    assert_eq!(fixtures::raw_rows(&path)?, rows);
    assert_eq!(fixtures::raw_manifest(&path)?, manifest);
    drop(fixtures::open(&path)?);
    Ok(())
}

fn counted_changes(scope: &RequestScope) -> TestResult<GroupApplyChanges<'_, 'static>> {
    let limits = fixtures::driver_limits();
    let frame = fixtures::independent_frame(
        b"counted-output",
        1,
        vec![
            (b"missing-a", fixtures::cancel_op(1)),
            (b"missing-b", fixtures::cancel_op(1)),
        ],
    )?;
    let previous = ControlState::initial(fixtures::clock(0)?);
    let decoded =
        prepare_independent_record_batch(frame.as_bytes(), limits.codec, limits.workspace_bytes)?;
    let inspection = inspect_independent_record_batch_point(
        decoded,
        fixtures::log(0),
        PointGateView {
            scope,
            control: previous,
            outcome: None,
            retained_nonempty: false,
        },
        limits.codec,
        limits.workspace_bytes,
    )?;
    let RecordBatchInspection::Fresh(fresh) = inspection else {
        return Err("expected fresh counted plan".into());
    };
    let placements = BTreeMap::from([(PartitionId::new(b"p".to_vec())?, scope.group().clone())]);
    let current = BTreeMap::from([
        (
            ScopedRecordId {
                partition_key: b"p",
                id: b"missing-a",
            },
            None,
        ),
        (
            ScopedRecordId {
                partition_key: b"p",
                id: b"missing-b",
            },
            None,
        ),
    ]);
    let plan = decide_fresh_independent_record_batch_touched(
        fresh,
        TouchedIndexedGroupView {
            group: scope.group(),
            placements: &placements,
            current: &current,
            parallel: &BTreeMap::new(),
            fifo_members: &BTreeMap::new(),
            fifo_heads: &BTreeMap::new(),
            policies: &BTreeMap::new(),
            retry_policies: &BTreeMap::new(),
        },
        0,
        1_000,
    )?;
    let changes = GroupApplyChanges::new(scope, previous, plan)?;
    let outcome = changes.outcome().ok_or("missing counted outcome")?;
    assert_eq!(
        record_batch_retained_summary(
            outcome.intent(),
            outcome.response_version(),
            outcome.response(),
        )?,
        (RetainedStatus::Accepted, Some(2))
    );
    let business = changes
        .independent_business()
        .ok_or("missing independent plan")?;
    assert_eq!(business.results().len(), 2);
    assert!(business.results().iter().all(Result::is_err));
    assert_eq!(business.final_indices().len(), 0);
    assert_eq!(business.history_plans().count(), 0);
    Ok(changes)
}
