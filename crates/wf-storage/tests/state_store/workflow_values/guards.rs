//! Actual policy, target and resource refusals leave durable authority unchanged.

use std::path::Path;

use wf_domain::policy::definition::{FieldPatch, PolicyPatch};
use wf_domain::workflow::control::PreparedDigest;
use wf_domain::workflow::request::{ClientIntent, RequestScope, RetainedStatus};
use wf_domain::workflow::{Cancel, Create, Operation, Transition};
use wf_ports::blob::WorkflowValueBindingKnowledge;
use wf_ports::consensus::{
    PreparedOperation, PreparedRequest, PreparedWorkflowValueBatch, WorkflowValueTargetGuard,
};
use wf_ports::log::{CodecError, encode_record_batch, encode_workflow_value_batch};
use wf_ports::state::StateTransactionError;
use wf_storage::state::{RedbStateStore, StateStoreGenesis, StateStoreLimits};
use wf_storage::transaction::{
    RecordBatchTransactionError, RecordBatchTransactionOutcome, WorkflowValueTransactionError,
    WorkflowValueTransactionLimits, apply_record_batch_transaction,
    apply_workflow_value_batch_transaction,
};
use wf_types::identity::{PartitionId, RecordId, RequestId};
use wf_types::version::Fence;

use super::{TestResult, fixtures as f};

#[test]
fn value_binary_policy_witnesses_guard_births_and_targets() -> TestResult {
    let directory = crate::fixtures::directory("value-policy-guards")?;
    let path = directory.join("state");
    let fresh = f::fresh(&path)?;
    assert!(fresh.is_ok(), "{:?}", fresh.as_ref().err());
    let Ok(mut store) = fresh else {
        return Err("expected fresh value capability".into());
    };
    let scope = crate::fixtures::scope()?;
    let first = PolicyPatch {
        max_active_ms: FieldPatch::Set(5_000),
        ..Default::default()
    };
    let second = PolicyPatch {
        max_active_ms: FieldPatch::Set(6_000),
        ..Default::default()
    };
    f::install_complete(&mut store, &scope, first, None, 0, 10)?;
    f::install_complete(&mut store, &scope, second, Some(1), 1, 20)?;
    let installed = f::load_policy(&mut store, &scope)?;
    let policy = f::witness(b"r", installed.as_ref())?;
    assert_eq!(policy.profile, 1);
    assert_eq!(policy.generation, 2);
    let create = Operation::Create(Create {
        workflow_type: b"r".to_vec(),
        state: b"q".to_vec(),
        run_at: None,
        payload: Some(b"keep".to_vec()),
    });
    let operation = PreparedOperation::Batch(vec![f::item(b"guard", 30, create)?]);
    let mut birth = f::input(0, 0);
    birth.target = WorkflowValueTargetGuard::Absent;
    let inputs = vec![birth];
    let policies = vec![policy];
    let prepared = f::batch(5, b"birth", 30, operation, inputs, policies)?;
    f::apply_value(&mut store, &scope, &prepared, 2)?;
    drop(prepared);
    drop(store);
    for case in 0..8 {
        let mut witness = f::witness(b"r", installed.as_ref())?;
        match case {
            0 => witness.generation = 3,
            1 => witness.generation = 1,
            2 => witness.digest = PreparedDigest::new([0xa5; 32]),
            7 => witness.workflow_type = b"wrong".to_vec(),
            _ => {}
        }
        let mut input = f::input(0, 0);
        let incarnation = (2_u128 << 32) + u128::from(case == 6);
        input.target = if case == 5 {
            WorkflowValueTargetGuard::Absent
        } else {
            WorkflowValueTargetGuard::Present { incarnation }
        };
        let transition = Operation::Transition(Transition {
            from: b"q".to_vec(),
            to: b"waiting".to_vec(),
            fence: Fence::new(0),
            lease_token: None,
            run_at: None,
            payload: Some(b"replace".to_vec()),
        });
        let operation = PreparedOperation::Batch(vec![f::item(b"guard", 40, transition)?]);
        let request = format!("guard-{case}");
        let inputs = vec![input];
        let policies = vec![witness];
        let mut prepared = f::batch(5, request.as_bytes(), 40, operation, inputs, policies)?;
        prepared.policy_references_captured = match case {
            3 | 5..=7 => None,
            4 => Some(false),
            _ => Some(true),
        };
        let error = refused_value_batch(&path, &scope, &prepared, 3, f::limits())?;
        assert!(matches!(
            (case, error),
            (0, WorkflowValueTransactionError::PolicyGenerationNotApplied)
                | (1, WorkflowValueTransactionError::StalePolicyGeneration)
                | (
                    2,
                    WorkflowValueTransactionError::ConflictingPolicyGeneration
                )
                | (3, WorkflowValueTransactionError::PolicyReferenceRequired)
                | (4, WorkflowValueTransactionError::InvalidInput)
                | (5..=7, WorkflowValueTransactionError::StalePolicyTarget)
        ));
    }
    std::fs::remove_dir_all(directory)?;
    Ok(())
}

#[test]
fn value_binary_fresh_capability_refuses_legacy_and_unimplemented_lifecycle() -> TestResult {
    let directory = crate::fixtures::directory("value-capability")?;
    let path = directory.join("state");
    let fresh = f::fresh(&path)?;
    assert!(fresh.is_ok(), "{:?}", fresh.as_ref().err());
    let Ok(mut store) = fresh else {
        return Err("expected fresh value capability".into());
    };
    let scope = crate::fixtures::scope()?;
    let create = Operation::Create(Create {
        workflow_type: b"r".to_vec(),
        state: b"q".to_vec(),
        run_at: None,
        payload: Some(b"expired".to_vec()),
    });
    let cancel = Operation::Cancel(Cancel {
        fence: Fence::new(0),
        lease_token: None,
        expected_state: Some(b"q".to_vec()),
        expected_version: Some(1),
        error: Some(b"reason".to_vec()),
    });
    let operation = PreparedOperation::Batch(vec![
        f::item(b"expired", 100, create)?,
        f::item(b"expired", 100, cancel)?,
    ]);
    let mut terminal = f::input(1, 0);
    terminal.terminal_ttl_ms = Some(10);
    let inputs = vec![f::input(0, 0), terminal];
    let policies = vec![f::witness(b"r", None)?];
    let birth = f::batch(5, b"terminal", 100, operation, inputs, policies)?;
    f::apply_value(&mut store, &scope, &birth, 0)?;
    drop(store);
    let create = Operation::Create(Create {
        workflow_type: b"r".to_vec(),
        state: b"q".to_vec(),
        run_at: None,
        payload: Some(b"recreate".to_vec()),
    });
    let operation = PreparedOperation::Batch(vec![f::item(b"expired", 200, create)?]);
    let policies = vec![f::witness(b"r", None)?];
    let mut recreation = f::batch(5, b"reuse", 200, operation, vec![f::input(0, 0)], policies)?;
    let error = refused_value_batch(&path, &scope, &recreation, 1, f::limits())?;
    assert!(matches!(
        error,
        WorkflowValueTransactionError::UnsupportedProfile
    ));
    let legacy_create = Operation::Create(Create {
        workflow_type: b"r".to_vec(),
        state: b"q".to_vec(),
        run_at: None,
        payload: None,
    });
    let request = PreparedRequest {
        scope: scope.clone(),
        request: RequestId::new(b"legacy-bypass".to_vec())?,
        intent: ClientIntent::record_batch([0; 32]),
        stamp: crate::fixtures::clock(200)?,
        operation: PreparedOperation::Batch(vec![f::item(b"legacy-bypass", 200, legacy_create)?]),
    };
    let limits = crate::fixtures::driver_limits();
    let legacy = encode_record_batch(request, limits.codec, limits.workspace_bytes)?;
    let before = crate::fixtures::raw_rows(&path)?;
    let manifest = f::raw_manifest(&path)?;
    let mut store = RedbStateStore::open(
        &path,
        scope.group().clone(),
        scope.recovery().clone(),
        StateStoreLimits::default(),
    )?;
    let result = apply_record_batch_transaction(
        &mut store,
        &scope,
        legacy.as_bytes(),
        crate::fixtures::log(1),
        limits,
    );
    assert!(matches!(
        result,
        Err(RecordBatchTransactionError::Transaction(
            StateTransactionError::UnsupportedLayout
        ))
    ));
    drop(store);
    assert_eq!(crate::fixtures::raw_rows(&path)?, before);
    assert_eq!(f::raw_manifest(&path)?, manifest);
    check_legacy_refusals(&directory.join("legacy"), &scope, &recreation)?;
    recreation.profile = 128;
    assert!(matches!(
        encode_workflow_value_batch(&recreation, limits.codec),
        Err(CodecError::UnsupportedTag)
    ));
    std::fs::remove_dir_all(directory)?;
    Ok(())
}

#[test]
fn value_binary_limits_preserve_no_partial_publication() -> TestResult {
    let directory = crate::fixtures::directory("value-atomic-limits")?;
    let path = directory.join("state");
    let fresh = f::fresh(&path)?;
    assert!(fresh.is_ok(), "{:?}", fresh.as_ref().err());
    let Ok(store) = fresh else {
        return Err("expected fresh value capability".into());
    };
    drop(store);
    let scope = crate::fixtures::scope()?;
    let create = Operation::Create(Create {
        workflow_type: b"r".to_vec(),
        state: b"q".to_vec(),
        run_at: None,
        payload: Some(b"value".to_vec()),
    });
    let operation = PreparedOperation::Batch(vec![f::item(b"limited", 100, create)?]);
    let policies = vec![f::witness(b"r", None)?];
    let prepared = f::batch(5, b"limits", 100, operation, vec![f::input(0, 0)], policies)?;
    for case in 0..4 {
        let mut limits = f::limits();
        match case {
            0 => limits.maximum_value_rows = 3,
            1 => limits.maximum_value_bytes = 1,
            2 => limits.maximum_value_work = 1,
            _ => limits.maximum_value_temporary_bytes = 1,
        }
        let error = refused_value_batch(&path, &scope, &prepared, 0, limits)?;
        assert!(matches!(
            error,
            WorkflowValueTransactionError::Record(RecordBatchTransactionError::Transaction(
                StateTransactionError::Limit
            ))
        ));
    }
    let mut store = RedbStateStore::open(
        &path,
        scope.group().clone(),
        scope.recovery().clone(),
        StateStoreLimits::default(),
    )?;
    let encoded = encode_workflow_value_batch(&prepared, f::limits().record_batch.codec);
    assert!(encoded.is_ok(), "{:?}", encoded.as_ref().err());
    let mut limits = f::limits();
    limits.maximum_value_rows = 4;
    let result = apply_workflow_value_batch_transaction(
        &mut store,
        &scope,
        encoded?.as_bytes(),
        crate::fixtures::log(0),
        limits,
    );
    assert!(result.is_ok(), "{:?}", result.as_ref().err());
    let RecordBatchTransactionOutcome::Retained { outcome, .. } = result? else {
        return Err("expected accepted four-row publication".into());
    };
    assert_eq!(outcome.status(), RetainedStatus::Accepted);
    drop(store);
    let rows = crate::fixtures::raw_rows(&path)?;
    for family in [8, 9, 10, 11] {
        let prefix = [b'F', b'W', b'P', b'K', 0, 1, family];
        assert_eq!(
            rows.keys().filter(|key| key.starts_with(&prefix)).count(),
            1
        );
    }
    std::fs::remove_dir_all(directory)?;
    Ok(())
}

fn refused_value_batch(
    path: &Path,
    scope: &RequestScope,
    batch: &PreparedWorkflowValueBatch,
    at: u64,
    limits: WorkflowValueTransactionLimits,
) -> TestResult<WorkflowValueTransactionError> {
    let before = crate::fixtures::raw_rows(path)?;
    let manifest = f::raw_manifest(path)?;
    let mut store = RedbStateStore::open(
        path,
        scope.group().clone(),
        scope.recovery().clone(),
        StateStoreLimits::default(),
    )?;
    let encoded = encode_workflow_value_batch(batch, limits.record_batch.codec);
    assert!(encoded.is_ok(), "{:?}", encoded.as_ref().err());
    let result = apply_workflow_value_batch_transaction(
        &mut store,
        scope,
        encoded?.as_bytes(),
        crate::fixtures::log(at),
        limits,
    );
    assert!(result.is_err());
    let Err(error) = result else {
        return Err("expected operational value refusal".into());
    };
    drop(store);
    assert_eq!(crate::fixtures::raw_rows(path)?, before);
    assert_eq!(f::raw_manifest(path)?, manifest);
    Ok(error)
}

fn check_legacy_refusals(
    path: &Path,
    scope: &RequestScope,
    batch: &PreparedWorkflowValueBatch,
) -> TestResult {
    let placements = crate::fixtures::placements()?;
    let genesis = StateStoreGenesis {
        initial_time: crate::fixtures::clock(0)?,
        placements: &placements,
        policies: &[],
    };
    let mut store = RedbStateStore::create(
        path,
        scope.group().clone(),
        scope.recovery().clone(),
        genesis,
        StateStoreLimits::default(),
    )?;
    let create = Operation::Create(Create {
        workflow_type: b"r".to_vec(),
        state: b"q".to_vec(),
        run_at: None,
        payload: Some(b"inline".to_vec()),
    });
    let request = PreparedRequest {
        scope: scope.clone(),
        request: RequestId::new(b"legacy".to_vec())?,
        intent: ClientIntent::record_batch([0; 32]),
        stamp: crate::fixtures::clock(10)?,
        operation: PreparedOperation::Batch(vec![f::item(b"legacy", 10, create)?]),
    };
    let limits = crate::fixtures::driver_limits();
    let frame = encode_record_batch(request, limits.codec, limits.workspace_bytes)?;
    let result = apply_record_batch_transaction(
        &mut store,
        scope,
        frame.as_bytes(),
        crate::fixtures::log(0),
        limits,
    )?;
    let RecordBatchTransactionOutcome::Retained { outcome, .. } = result else {
        return Err("expected genuine populated legacy store".into());
    };
    assert_eq!(outcome.status(), RetainedStatus::Accepted);
    let binding = store.workflow_value_binding(
        scope,
        &PartitionId::new(b"p".to_vec())?,
        &RecordId::new(b"legacy".to_vec())?,
        None,
        f::read_limits(),
    );
    assert!(binding.is_ok(), "{:?}", binding.as_ref().err());
    assert!(matches!(
        binding?,
        WorkflowValueBindingKnowledge::LegacyUnavailable
    ));
    drop(store);
    let before = crate::fixtures::raw_rows(path)?;
    let manifest = crate::fixtures::raw_manifest(path)?;
    let mut store = RedbStateStore::open(
        path,
        scope.group().clone(),
        scope.recovery().clone(),
        StateStoreLimits::default(),
    )?;
    let encoded = encode_workflow_value_batch(batch, limits.codec);
    assert!(encoded.is_ok(), "{:?}", encoded.as_ref().err());
    let result = apply_workflow_value_batch_transaction(
        &mut store,
        scope,
        encoded?.as_bytes(),
        crate::fixtures::log(1),
        f::limits(),
    );
    assert!(matches!(
        result,
        Err(WorkflowValueTransactionError::UnsupportedProfile)
    ));
    drop(store);
    assert_eq!(crate::fixtures::raw_rows(path)?, before);
    assert_eq!(crate::fixtures::raw_manifest(path)?, manifest);
    let database = redb::Builder::new().open(path)?;
    let refused = RedbStateStore::create_value_database(
        database,
        scope.group().clone(),
        scope.recovery().clone(),
        genesis,
        f::defaults(),
        StateStoreLimits::default(),
    );
    assert!(matches!(
        refused,
        Err(StateTransactionError::NamespaceConflict)
    ));
    assert_eq!(crate::fixtures::raw_rows(path)?, before);
    assert_eq!(crate::fixtures::raw_manifest(path)?, manifest);
    Ok(())
}
