//! Supported receipt families cannot publish while received DATA awaits log ACK.

use super::{
    CorePolicyTransactionError, RecordBatchTransactionOutcome, SnapshotLimits, SnapshotTransition,
    StateSnapshotTransitionStore, StateTransactionError, TestResult, artifact, cohort, f, normal,
    received, seed,
};
use sha2::{Digest, Sha256};
use std::path::Path;
use wf_domain::policy::complete::{
    CompleteCorePolicyPatchInput, CompleteCorePolicyPatchRequestInput, OriginalPolicySetFrameInput,
    OriginalPolicySetMapOrders, PolicySchemaCharge, RawFlowPolicyValue as Raw,
    prepare_original_policy_set_request,
};
use wf_domain::policy::definition::{FieldPatch, PolicyPatch};
use wf_ports::log::{
    RaftLogStore, prepare_complete_core_policy_patch_frame, prepare_original_policy_set_frame,
};
use wf_storage::transaction::{
    OriginalPolicyTransactionError, apply_complete_core_policy_patch_transaction,
    apply_original_policy_set_transaction,
};
use wf_types::identity::RequestId;

pub(super) fn assert_requests() -> TestResult {
    let source = f::directory("interop-supported-receipt-source")?;
    let (_, wire, position) = seed(&source, true, true)?;
    assert_eq!(position, 2);
    let mut observations = Vec::new();
    for original in [false, true] {
        for class in 0..4 {
            let dir = f::directory("interop-supported-receipt-pending")?;
            let (same_cohort, same_logs, refused) = one(&dir, &wire, original, class)?;
            observations.push((original, class, same_cohort, same_logs, refused));
            std::fs::remove_dir_all(dir)?;
        }
    }
    std::fs::remove_dir_all(source)?;
    assert!(
        observations
            .iter()
            .all(|(_, _, same_cohort, same_logs, refused)| *same_cohort && *same_logs && *refused),
        "pending receipt publication (original,class,same_cohort,same_logs,InvalidChanges): {observations:?}"
    );
    Ok(())
}

fn one(dir: &Path, wire: &[u8], original: bool, class: u8) -> TestResult<(bool, bool, bool)> {
    let (mut log, mut state, old, _) = received::target(dir)?;
    let before_log = log.state()?;
    let transition =
        SnapshotTransition::received(before_log, f::at(2), Sha256::digest(wire).into())?;
    let candidate = received::stage(dir, wire)?;
    state.prepare_received_snapshot(candidate, transition, SnapshotLimits::default())?;
    artifact(&mut state, &old)?;
    artifact(&mut state, wire)?;
    assert_eq!(
        state.snapshot_transition(SnapshotLimits::default())?,
        Some(transition)
    );
    drop((log, state));
    let before = cohort(dir)?;
    let logs = (f::log_rows(dir)?, f::log_meta(dir)?);
    let body = frame(original, class)?;
    let (log, mut state) = f::open_pair(dir)?;
    let limits = normal();
    let result: TestResult<RecordBatchTransactionOutcome> = if original {
        apply_original_policy_set_transaction(
            &mut state,
            &f::scope()?,
            &body,
            f::at(3),
            limits,
            &mut PolicySchemaCharge::new(limits.schema),
        )
        .map_err(Into::into)
    } else {
        apply_complete_core_policy_patch_transaction(
            &mut state,
            &f::scope()?,
            &body,
            f::at(3),
            limits,
        )
        .map_err(Into::into)
    };
    drop((log, state));
    let same_cohort = cohort(dir)? == before;
    let same_logs = (f::log_rows(dir)?, f::log_meta(dir)?) == logs;
    let refused = result.as_ref().err().is_some_and(|error| {
        if original {
            error.downcast_ref::<OriginalPolicyTransactionError>()
                == Some(&OriginalPolicyTransactionError::Transaction(
                    StateTransactionError::InvalidChanges,
                ))
        } else {
            error.downcast_ref::<CorePolicyTransactionError>()
                == Some(&CorePolicyTransactionError::Transaction(
                    StateTransactionError::InvalidChanges,
                ))
        }
    });
    Ok((same_cohort, same_logs, refused))
}

fn frame(original: bool, class: u8) -> TestResult<Vec<u8>> {
    assert!(class < 4);
    if original {
        return if class == 3 {
            stale_original()
        } else {
            super::super::policy(
                &f::scope()?,
                b"set",
                if class == 2 { 1012 } else { 12 },
                class == 1,
            )
        };
    }
    let limits = normal();
    let now = if class == 2 { 1011 } else { 11 };
    let request = if class == 3 {
        b"fresh-core".as_slice()
    } else {
        b"core"
    };
    Ok(prepare_complete_core_policy_patch_frame(
        CompleteCorePolicyPatchRequestInput::new(
            f::scope()?,
            RequestId::new(request.to_vec())?,
            f::clock(now, 0, now)?,
            CompleteCorePolicyPatchInput {
                workflow_type: b"r".to_vec(),
                expected_generation: Some(7),
                patch: PolicyPatch {
                    max_active_ms: if class == 1 {
                        FieldPatch::Set(1)
                    } else {
                        FieldPatch::Unchanged
                    },
                    ..PolicyPatch::default()
                },
            },
        ),
        limits.schema,
        limits.codec.admission(),
        &mut PolicySchemaCharge::new(limits.schema),
    )?
    .as_bytes()
    .to_vec())
}

fn stale_original() -> TestResult<Vec<u8>> {
    const PATCH: &[u8] = &[0, 0, 0, 17, 4, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 17];
    let limits = normal();
    let mut charge = PolicySchemaCharge::new(limits.schema);
    let expected = [Raw::Atom("expected_generation"), Raw::Integer(0)];
    let opts = [Raw::Tuple(&expected)];
    let prepared = prepare_original_policy_set_request(
        &Raw::Binary(b"u"),
        &Raw::List(&opts),
        limits.schema,
        &mut charge,
    )?
    .map_err(|reason| format!("stale fixture refused: {reason:?}"))?;
    let orders = OriginalPolicySetMapOrders::new(
        Sha256::digest(PATCH).into(),
        None,
        &[(0, &[], &[])],
        &[],
        &[],
    );
    Ok(prepare_original_policy_set_frame(
        OriginalPolicySetFrameInput::new(
            f::scope()?,
            RequestId::new(b"fresh-set".to_vec())?,
            f::clock(12, 0, 12)?,
            &prepared,
            &orders,
        ),
        limits.schema,
        limits.codec.admission(),
        &mut charge,
    )?
    .as_bytes()
    .to_vec())
}
