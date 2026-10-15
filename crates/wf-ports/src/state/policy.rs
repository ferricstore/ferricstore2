//! Original public policy query preparation, one read and borrowed projection.

use wf_domain::policy::complete::{
    CompletePolicyEncodingError, OriginalPolicyGetRejection, OriginalPolicyReadValue,
    OriginalPolicyReply, OriginalPolicyReplyContext, OriginalPolicyReplyError,
    OriginalPolicyReplyRejection, PolicySchemaCharge, PolicySchemaLimits, RawFlowPolicyValue,
    prepare_original_policy_get_request, present_original_policy_get_reply,
};

/// Policy-specific source compatibility read capability.
///
/// The caller derives the fixed allocator shard and exact policy key. Adapters
/// must supply their caller/tenant scope and suppress cache tracking. This trait
/// selects no blocking I/O, strong-read barrier or physical adapter. Its result
/// borrows the reader independently of the temporary key's lifetime.
pub trait OriginalPolicyGetReader {
    /// Reads once using the supplied source allocator and policy key.
    fn read_policy_value_without_cache_tracking<'read>(
        &'read mut self,
        shard: u32,
        key: &[u8],
    ) -> OriginalPolicyReadValue<'read>;
}

/// Prepares fresh public controls, reads allocator zero once and projects GET.
///
/// Preparation prospectively pays the one read invocation. The temporary key
/// drops before the unchanged projector admits its selected live owners. Raw
/// payloads, reader storage and existing caller owners remain external inputs;
/// the supplied context carries its own additive live reservation unchanged.
///
/// # Errors
/// Preserves operational/raised projector errors separately from exact public
/// input/read diagnoses. Input rejection returns before invoking the reader.
pub fn read_original_policy_get_reply<'input, R: OriginalPolicyGetReader>(
    reader: &'input mut R,
    workflow_type: &RawFlowPolicyValue<'input>,
    opts: &RawFlowPolicyValue<'input>,
    context: OriginalPolicyReplyContext<'input>,
    limits: PolicySchemaLimits,
    charge: &mut PolicySchemaCharge,
) -> Result<Result<OriginalPolicyReply<'input>, OriginalPolicyGetRejection>, OriginalPolicyReplyError>
{
    let request = match prepare_original_policy_get_request(workflow_type, opts, limits, charge)? {
        Ok(request) => request,
        Err(reason) => return Ok(Err(reason)),
    };
    let workflow_type = request.workflow_type();
    let state = request.response_state();
    let value = reader.read_policy_value_without_cache_tracking(0, request.policy_key());
    drop(request);
    match present_original_policy_get_reply(workflow_type, value, state, context, limits, charge)? {
        Ok(reply) => Ok(Ok(reply)),
        Err(OriginalPolicyReplyRejection::Corrupt) => Ok(Err(OriginalPolicyGetRejection::Corrupt)),
        Err(OriginalPolicyReplyRejection::ShardUnavailable) => {
            Ok(Err(OriginalPolicyGetRejection::ShardUnavailable))
        }
        Err(OriginalPolicyReplyRejection::ReadFailed) => {
            Ok(Err(OriginalPolicyGetRejection::ReadFailed))
        }
        // GET never returns the SET-only allocation diagnosis.
        Err(OriginalPolicyReplyRejection::AllocationFailed) => {
            Err(CompletePolicyEncodingError::Malformed.into())
        }
    }
}
