//! Actual policy publication fixtures for same-revision native reads.
#[cfg(test)]
pub(super) mod fixture_data {
    use super::super::super::TestResult;
    use crate::state::RedbStateStore;
    use crate::transaction::{
        RecordBatchTransactionLimits, RecordBatchTransactionOutcome,
        apply_complete_core_policy_patch_transaction, apply_original_policy_set_transaction,
    };
    use sha2::{Digest, Sha256};
    use wf_domain::policy::complete::{
        CompleteCorePolicyPatchInput, CompleteCorePolicyPatchRequestInput,
        OriginalPolicySetFrameInput, OriginalPolicySetMapOrders, PolicySchemaCharge,
        PolicySchemaLimits, RawFlowPolicyValue as Raw, prepare_original_policy_set_frame_bytes,
        prepare_original_policy_set_request,
    };
    use wf_domain::policy::definition::{FieldPatch, PolicyPatch};
    use wf_domain::workflow::{
        admission::AdmissionLimits,
        request::{RequestScope, RetainedStatus},
    };
    use wf_ports::log::{
        CodecLimits, prepare_complete_core_policy_patch_frame, prepare_original_policy_set_frame,
    };
    use wf_ports::state::WorkflowRecordPolicyObservation as Policy;
    use wf_types::{
        identity::RequestId,
        version::{AppliedPosition, CommittedTime, HlcTimestamp, LogIdentity},
    };

    fn limits() -> RecordBatchTransactionLimits {
        RecordBatchTransactionLimits {
            codec: CodecLimits::new(AdmissionLimits::default()),
            schema: PolicySchemaLimits::default(),
            maximum_read_steps: 10_000,
            maximum_read_bytes: 64 * 1_048_576,
            maximum_staging_bytes: 64 * 1_048_576,
            maximum_adapter_bytes: 32 * 1_048_576,
            workspace_bytes: 512 * 1_048_576,
            outcome_ttl_ms: 1_000,
        }
    }
    fn stamp(now: u64) -> TestResult<CommittedTime> {
        Ok(CommittedTime::new(HlcTimestamp::new(now, 0)?, now)?)
    }
    fn accepted(outcome: RecordBatchTransactionOutcome) -> TestResult {
        let RecordBatchTransactionOutcome::Retained { outcome, .. } = outcome else {
            return Err("expected consumed policy publication".into());
        };
        assert_eq!(outcome.status(), RetainedStatus::Accepted);
        Ok(())
    }
    pub(in super::super) fn check_complete(
        policy: Policy,
        generation: u64,
        active: u64,
    ) -> TestResult {
        let Policy::Present(wf_ports::state::StoredPolicyValue::Complete(value)) = policy else {
            return Err("expected actual Complete backing".into());
        };
        assert_eq!(value.generation(), generation);
        assert_eq!(value.data().workflow_type, b"r");
        assert_eq!(value.data().max_active_ms, Some(active));
        Ok(())
    }
    pub(in super::super) fn publish_policy(
        store: &mut RedbStateStore,
        scope: &RequestScope,
        now: u64,
        expected: Option<u64>,
        active: u64,
        kind: &[u8],
    ) -> TestResult {
        let profile = limits();
        let frame = prepare_complete_core_policy_patch_frame(
            CompleteCorePolicyPatchRequestInput::new(
                scope.clone(),
                RequestId::new(now.to_be_bytes().to_vec())?,
                stamp(now)?,
                CompleteCorePolicyPatchInput {
                    workflow_type: kind.to_vec(),
                    expected_generation: expected,
                    patch: PolicyPatch {
                        max_active_ms: FieldPatch::Set(active),
                        ..PolicyPatch::default()
                    },
                },
            ),
            profile.schema,
            profile.codec.admission(),
            &mut PolicySchemaCharge::new(profile.schema),
        )?;
        accepted(apply_complete_core_policy_patch_transaction(
            store,
            scope,
            frame.as_bytes(),
            LogIdentity::new(1, 7, AppliedPosition::new(now - 100)),
            profile,
        )?)
    }
    pub(in super::super) fn publish_original(
        store: &mut RedbStateStore,
        scope: &RequestScope,
        now: u64,
    ) -> TestResult {
        let profile = limits();
        let pair = [Raw::Atom("max_active_ms"), Raw::Integer(1000)];
        let options = [Raw::Tuple(&pair)];
        let mut charge = PolicySchemaCharge::new(profile.schema);
        let prepared = prepare_original_policy_set_request(
            &Raw::Binary(b"r"),
            &Raw::List(&options),
            profile.schema,
            &mut charge,
        )?
        .map_err(|e| format!("original fixture: {e:?}"))?;
        let records: [(u32, &[u32], &[u32]); 1] = [(0, &[0], &[0])];
        let request = RequestId::new(now.to_be_bytes().to_vec())?;
        let orders = OriginalPolicySetMapOrders::new([0; 32], None, &records, &[], &[]);
        let input = OriginalPolicySetFrameInput::new(
            scope.clone(),
            request.clone(),
            stamp(now)?,
            &prepared,
            &orders,
        );
        let (bytes, _, _, range) = prepare_original_policy_set_frame_bytes(
            input,
            profile.schema,
            profile.codec.admission(),
            &mut charge,
        )?;
        let orders = OriginalPolicySetMapOrders::new(
            Sha256::digest(&bytes[range]).into(),
            None,
            &records,
            &[],
            &[],
        );
        let input = OriginalPolicySetFrameInput::new(
            scope.clone(),
            request,
            stamp(now)?,
            &prepared,
            &orders,
        );
        let frame = prepare_original_policy_set_frame(
            input,
            profile.schema,
            profile.codec.admission(),
            &mut charge,
        )?;
        accepted(apply_original_policy_set_transaction(
            store,
            scope,
            frame.as_bytes(),
            LogIdentity::new(1, 7, AppliedPosition::new(now - 100)),
            profile,
            &mut PolicySchemaCharge::new(profile.schema),
        )?)
    }
}
