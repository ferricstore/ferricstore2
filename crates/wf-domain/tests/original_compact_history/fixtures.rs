use super::{C, D, L, M, P, S, TestResult, decode, prepare};
use wf_domain::history::metadata::{
    OriginalHistoryMetadataField, OriginalHistoryMetadataLimits, PreparedOriginalHistoryMetadata,
    prepare_original_history_metadata,
};
use wf_domain::value::{OriginalValueInput, OriginalValueNode};

pub(super) fn limits() -> L {
    L {
        max_input_bytes: 1_048_576,
        max_metadata_pairs: 4096,
        max_encoded_bytes: 1_048_576,
        max_output_bytes: 4_194_304,
        max_work_units: 33_554_432,
        max_temporary_bytes: 8_388_608,
    }
}

pub(super) fn context() -> C<'static> {
    C {
        id: Some(b"flow-1"),
        workflow_type: Some(b"email"),
        root_flow_id: Some(b"flow-1"),
        ..C::default()
    }
}

pub(super) fn base<'a>(metadata: &'a [OriginalHistoryMetadataField<'a>]) -> S<'a, 'a> {
    S {
        event: b"123-1",
        version: M::Integer(1),
        at: M::Integer(123),
        state: Some(b"running"),
        numbers: [
            M::Integer(0),
            M::Integer(1),
            M::Integer(7),
            M::Integer(100),
            M::Integer(110),
            M::Nil,
            M::Integer(1000),
        ],
        references: [
            Some(b"worker-1"),
            Some(b"flow/value/a/payload/1"),
            None,
            None,
            None,
        ],
        context: context(),
        metadata,
    }
}

pub(super) fn empty() -> S<'static, 'static> {
    S {
        event: b"e",
        version: M::Integer(1),
        at: M::Integer(0),
        state: Some(b"s"),
        numbers: [
            M::Integer(0),
            M::Integer(0),
            M::Integer(0),
            M::Integer(0),
            M::Integer(0),
            M::Nil,
            M::Integer(0),
        ],
        references: [None; 5],
        context: C::default(),
        metadata: &[],
    }
}

pub(super) fn healthy(snapshot: &S<'_, '_>, limits: L) -> TestResult<P> {
    let result = prepare(snapshot, limits);
    assert!(
        result.is_ok(),
        "healthy compact history encode: {:?}",
        result.as_ref().err()
    );
    Ok(result?)
}

pub(super) fn decoded<'a>(bytes: &'a [u8], context: C<'a>, limits: L) -> TestResult<D<'a>> {
    let result = decode(bytes, context, limits);
    assert!(
        result.is_ok(),
        "healthy compact history decode: {:?}",
        result.as_ref().err()
    );
    Ok(result?)
}

pub(super) fn pairs<'a>(value: &'a D<'_>) -> Vec<(&'a [u8], Option<&'a [u8]>)> {
    value
        .fields()
        .iter()
        .map(|field| (field.key.as_ref(), field.value.as_deref()))
        .collect()
}

pub(super) fn value<'a>(document: &'a D<'_>, key: &[u8]) -> Option<&'a [u8]> {
    document
        .fields()
        .iter()
        .rev()
        .find(|field| field.key.as_ref() == key)
        .and_then(|field| field.value.as_deref())
}

pub(super) fn metadata_nodes(length: usize) -> [OriginalValueNode; 3] {
    [
        OriginalValueNode::Atom {
            bytes: 0..6,
            utf8: true,
        },
        OriginalValueNode::Binary { bytes: 6..length },
        OriginalValueNode::Map { entries: 0..2 },
    ]
}

pub(super) fn normalized<'a>(
    bytes: &'a [u8],
    nodes: &'a [OriginalValueNode],
) -> TestResult<PreparedOriginalHistoryMetadata<'a>> {
    Ok(prepare_original_history_metadata(
        OriginalValueInput {
            bytes,
            nodes,
            edges: &[0, 1],
            root: 2,
        },
        OriginalHistoryMetadataLimits {
            max_input_bytes: 1_048_576,
            max_pairs: 4096,
            max_output_bytes: 1_048_576,
            max_work_units: 33_554_432,
            max_temporary_bytes: 8_388_608,
        },
    )?)
}

pub(super) fn base_fields<'a>() -> Vec<(&'a [u8], Option<&'a [u8]>)> {
    vec![
        (b"event", Some(b"123-1")),
        (b"version", Some(b"1")),
        (b"at", Some(b"123")),
        (b"id", Some(b"flow-1")),
        (b"type", Some(b"email")),
        (b"state", Some(b"running")),
        (b"priority", Some(b"0")),
        (b"attempts", Some(b"1")),
        (b"fencing_token", Some(b"7")),
        (b"created_at_ms", Some(b"100")),
        (b"updated_at_ms", Some(b"110")),
        (b"next_run_at_ms", Some(b"")),
        (b"lease_deadline_ms", Some(b"1000")),
        (b"lease_owner", Some(b"worker-1")),
        (b"payload_ref", Some(b"flow/value/a/payload/1")),
        (b"parent_flow_id", Some(b"")),
        (b"root_flow_id", Some(b"flow-1")),
        (b"correlation_id", Some(b"")),
        (b"result_ref", Some(b"")),
        (b"error_ref", Some(b"")),
        (b"rewound_to_event_id", Some(b"")),
    ]
}

pub(super) fn terminal_wire() -> Vec<u8> {
    b"FSH2\xdf'\x06123-1\x02|\x0acompleted\x02\x08eo\xe9\x07\x09worker-1\x17flow/value/a/payload/1\x16flow/value/a/result/2\x02\x07reason\x03ok".to_vec()
}

pub(super) fn compact_wire() -> Vec<u8> {
    b"FSH2\x8f\x02\x0btransition\x0d\xd1\x0f\x08running\x03\x0a\xe9\x07\x13shared-payload-ref"
        .to_vec()
}

pub(super) fn long_wire() -> Vec<u8> {
    let mut bytes = b"FSH2\xdf#\x0c123456789-1\x02\x96\x9a\xef:\x08running\x02\x08eo\xe9\x07\x09worker-1\x17flow/value/a/payload/1\x02\x07reason\xab\x01".to_vec();
    bytes.extend_from_slice(&[b'r'; 170]);
    bytes
}
