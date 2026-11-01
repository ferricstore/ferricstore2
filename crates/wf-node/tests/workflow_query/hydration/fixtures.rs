//! Reuse real metadata publication; the barrier and fault provider is controlled.
use super::*;
use wf_application::queries::{
    StrongWorkflowRecordValueLimits as L, StrongWorkflowRecordValueRequest,
};
use wf_domain::{
    options::normalize_original_payload_return,
    policy::complete::RawFlowPolicyValue as R,
    workflow::{Create, Operation, request::RequestScope},
};
use wf_ports::blob::HistoryValueReadConversionLimits;

pub(super) enum HydrationFault {
    None,
    Deny,
    Unavailable,
    ReadFailure,
    Missing,
    Mismatch,
    UnsafeDrift,
    RevokeAfterRecord,
}
pub(super) struct HydrationFixture {
    pub(super) base: QueryFixture,
    pub(super) value_reads: usize,
    pub(super) fault: HydrationFault,
    pub(super) value_now: Option<TimestampMillis>,
}
impl HydrationFixture {
    pub(super) fn new(tag: &str, payload: Option<&[u8]>) -> TestResult<Self> {
        Self::scoped(tag, payload, None)
    }
    pub(super) fn scoped(
        tag: &str,
        payload: Option<&[u8]>,
        prefix: Option<&[u8]>,
    ) -> TestResult<Self> {
        let mut base = QueryFixture::scoped(tag, prefix)?;
        base.apply(
            Operation::Create(Create {
                workflow_type: b"r".to_vec(),
                state: b"q".to_vec(),
                run_at: None,
                payload: payload.map(<[u8]>::to_vec),
            }),
            0,
            100,
        )?;
        Ok(Self {
            base,
            value_reads: 0,
            fault: HydrationFault::None,
            value_now: None,
        })
    }
    pub(super) fn started(tag: &str, payload: &[u8]) -> TestResult<Self> {
        let mut base = QueryFixture::new(tag)?;
        base.apply_profile(
            Operation::StartAndClaim(wf_domain::workflow::StartAndClaim {
                workflow_type: b"r".to_vec(),
                state: b"q".to_vec(),
                worker: b"w".to_vec(),
                lease_ms: 100,
                payload: Some(payload.to_vec()),
            }),
            7,
            0,
            100,
        )?;
        Ok(Self {
            base,
            value_reads: 0,
            fault: HydrationFault::None,
            value_now: None,
        })
    }
    pub(super) fn finish(self) -> TestResult {
        self.base.finish()
    }
}
pub(super) struct HydrationClock {
    pub(super) now: TimestampMillis,
}
impl wf_ports::clock::WorkflowValueReadClock for HydrationClock {
    fn read_snapshot(
        &mut self,
    ) -> Result<
        (wf_types::version::HlcTimestamp, TimestampMillis),
        wf_ports::state::StateTransactionError,
    > {
        Ok((
            wf_types::version::HlcTimestamp::new(self.now.get(), 0)
                .map_err(|_| wf_ports::state::StateTransactionError::Unavailable)?,
            self.now,
        ))
    }
}
pub(super) fn request_values<'a>(
    scope: &'a RequestScope,
    id: &'a RecordId,
    enabled: bool,
    maximum: u64,
) -> TestResult<StrongWorkflowRecordValueRequest<'a>> {
    Ok(StrongWorkflowRecordValueRequest {
        record: request(scope, id, None),
        value_return: normalize_original_payload_return(
            &[(
                "payload",
                if enabled {
                    R::Atom("true")
                } else {
                    R::Atom("false")
                },
            )],
            false,
            R::Integer(i128::from(maximum)),
        )?,
    })
}
pub(super) fn value_limits() -> L {
    let mib = f::MIB;
    L {
        record: limits(),
        values: wf_ports::blob::WorkflowValueReadLimits {
            maximum_references: 3,
            maximum_stored_bytes: 8 * mib,
            maximum_work: 8 * mib,
            maximum_temporary_bytes: 16 * mib,
        },
        conversion: HistoryValueReadConversionLimits {
            maximum_references: 3,
            maximum_work: mib,
            maximum_temporary_bytes: 16 * mib,
        },
        projection: wf_domain::history::values::HistoryValueLimits {
            maximum_entries: 1,
            maximum_references: 3,
            maximum_read_results: 3,
            maximum_writes: 9,
            maximum_work_units: 32 * mib,
            maximum_requested_bytes: 16 * mib,
            decoder: wf_domain::value::OriginalValueLimits {
                max_encoded_bytes: mib,
                max_nodes: 10000,
                max_edges: 20000,
                max_depth: 64,
                max_work_units: mib,
            },
        },
        maximum_output_bytes: 8 * mib,
        maximum_work_units: 128 * mib,
        maximum_temporary_bytes: 128 * mib,
    }
}
