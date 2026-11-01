use super::TestResult;
use sha2::{Digest, Sha256};
use wf_domain::metadata::record::{
    PreparedOriginalRecordMetadata, prepare_original_record_metadata,
};
use wf_domain::value::{OriginalValueEncodeLimits, OriginalValueInput, OriginalValueNode as Node};
use wf_domain::workflow::admission::AdmissionLimits;
use wf_domain::workflow::indexed::OrderingMetadata;
use wf_domain::workflow::request::RequestScope;
use wf_domain::workflow::{CommittedCommand, Create, Operation, WorkflowRecord, reduce};
use wf_storage::state::value::{RecordCaptureInput, RecordCaptureLimits, RecordCapturePoint};
use wf_types::identity::{GroupId, PartitionId, RecordId, RecoveryId, TenantId};
use wf_types::version::TimestampMillis;

pub(super) struct Fixture {
    pub scope: RequestScope,
    pub partition: PartitionId,
    pub record: WorkflowRecord,
    pub ordering: OrderingMetadata,
    pub metadata: PreparedOriginalRecordMetadata,
}
impl Fixture {
    pub fn new() -> TestResult<Self> {
        let plan = reduce(
            None,
            &CommittedCommand {
                id: RecordId::new(b"i\0\xff".to_vec())?,
                now: TimestampMillis::new(1),
                operation: Operation::Create(Create {
                    workflow_type: b"r\0\xff".to_vec(),
                    state: b"q\0\xff".to_vec(),
                    run_at: Some(TimestampMillis::new(7)),
                    payload: Some(Vec::new()),
                }),
            },
        )?;
        Ok(Self {
            scope: RequestScope::new(
                TenantId::new(b"t\0\xff".to_vec())?,
                GroupId::new(b"g\0\xff".to_vec())?,
                RecoveryId::new(b"z\0\xff".to_vec())?,
            ),
            partition: PartitionId::new(b"p\0\xff".to_vec())?,
            record: plan.into_parts().1.ok_or("missing create")?.record,
            ordering: OrderingMetadata {
                priority: 2,
                state_enter_seq: u128::MAX - 7,
            },
            metadata: metadata()?,
        })
    }
    pub fn input(&self) -> RecordCaptureInput<'_> {
        RecordCaptureInput {
            scope: &self.scope,
            partition: &self.partition,
            record: &self.record,
            ordering: self.ordering,
            incarnation: self.ordering.state_enter_seq,
            metadata: &self.metadata,
        }
    }
    pub fn point(&self) -> RecordCapturePoint<'_> {
        RecordCapturePoint {
            scope: &self.scope,
            partition: &self.partition,
            id: self.record.id(),
            incarnation: self.ordering.state_enter_seq,
            version: self.record.version(),
        }
    }
}
pub(super) fn limits() -> RecordCaptureLimits {
    RecordCaptureLimits {
        admission: AdmissionLimits::default(),
        maximum_encoded_bytes: 4_194_304,
        maximum_metadata_bytes: 2_097_152,
        maximum_work_units: 1024 * 1024 * 1024,
        maximum_temporary_bytes: 8_388_608,
    }
}
fn metadata() -> TestResult<PreparedOriginalRecordMetadata> {
    let bytes = b"wf.record.metadataattributes";
    let nodes = [
        Node::Binary { bytes: 0..18 },
        Node::Integer(1),
        Node::Binary { bytes: 18..28 },
        Node::Integer(7),
        Node::Map { entries: 0..2 },
        Node::Tuple { children: 2..5 },
    ];
    let edges = [2, 3, 0, 1, 4];
    let limits = OriginalValueEncodeLimits {
        max_input_bytes: 1024,
        max_nodes: 32,
        max_edges: 32,
        max_depth: 16,
        max_encoded_bytes: 1024,
        max_work_units: 1_048_576,
        max_temporary_bytes: 1_048_576,
    };
    Ok(prepare_original_record_metadata(
        OriginalValueInput {
            bytes,
            nodes: &nodes,
            edges: &edges,
            root: 5,
        },
        None,
        limits,
    )?)
}

pub(super) fn large_metadata(length: usize) -> TestResult<PreparedOriginalRecordMetadata> {
    let mut bytes = b"wf.record.metadataattributes".to_vec();
    bytes.resize(bytes.len() + length, 0xa5);
    let nodes = [
        Node::Binary { bytes: 0..18 },
        Node::Integer(1),
        Node::Binary { bytes: 18..28 },
        Node::Binary {
            bytes: 28..bytes.len(),
        },
        Node::Map { entries: 0..2 },
        Node::Tuple { children: 2..5 },
    ];
    let edges = [2, 3, 0, 1, 4];
    Ok(prepare_original_record_metadata(
        OriginalValueInput {
            bytes: &bytes,
            nodes: &nodes,
            edges: &edges,
            root: 5,
        },
        None,
        OriginalValueEncodeLimits {
            max_input_bytes: 1_048_576,
            max_nodes: 32,
            max_edges: 32,
            max_depth: 16,
            max_encoded_bytes: 2_097_152,
            max_work_units: 134_217_728,
            max_temporary_bytes: 33_554_432,
        },
    )?)
}
pub(super) fn independent_wire() -> TestResult<Vec<u8>> {
    // Independently packed core names, present-empty payload, priority and sequence.
    let hex = "465753520002000000036700ff000000037a00ff000000037400ff000000037000ff02fffffffffffffffffffffffffffffff8000000036900ff000000037200ff000000037100ff000000000000000100000000000000000000000000000000000000000000000100000000000000010100000000000000070000010000000000000000000000000000010000000000000000";
    let core: Vec<u8> = hex
        .as_bytes()
        .as_chunks::<2>()
        .0
        .iter()
        .map(|p| Ok(u8::from_str_radix(std::str::from_utf8(p)?, 16)?))
        .collect::<TestResult<_>>()?;
    let mut metadata = b"FSV2\x02\x83\x68\x03\x6d\0\0\0\x12wf.record.metadata\x61\x01\x74\0\0\0\x01\x6d\0\0\0\x0aattributes\x61\x07".to_vec();
    let mut bytes = b"FWMC\0\x01".to_vec();
    bytes.extend_from_slice(&(u128::MAX - 7).to_be_bytes());
    bytes.extend_from_slice(&u32::try_from(core.len())?.to_be_bytes());
    bytes.extend(core);
    bytes.extend_from_slice(&u32::try_from(metadata.len())?.to_be_bytes());
    bytes.append(&mut metadata);
    seal(&mut bytes);
    Ok(bytes)
}
pub(super) fn seal(bytes: &mut Vec<u8>) {
    let digest: [u8; 32] = Sha256::digest(&bytes).into();
    bytes.extend_from_slice(&digest);
}
