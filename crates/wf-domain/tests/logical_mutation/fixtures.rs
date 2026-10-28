use wf_domain::value::OriginalValueEncodeLimits;
use wf_domain::workflow::admission::AdmissionLimits;
use wf_types::version::{CommittedTime, HlcTimestamp};

use super::{Input, L, Operation, P, S, TestResult, V, prepare};

pub(super) fn limits() -> L {
    L {
        admission: AdmissionLimits::default(),
        values: OriginalValueEncodeLimits {
            max_input_bytes: 65_536,
            max_nodes: 4_096,
            max_edges: 8_192,
            max_depth: 128,
            max_encoded_bytes: 65_536,
            max_work_units: 4_194_304,
            max_temporary_bytes: 4_194_304,
        },
        max_raw_bytes: 4_194_304,
        max_work_units: 16_777_216,
        max_temporary_bytes: 16_777_216,
    }
}

pub(super) fn stamp() -> TestResult<CommittedTime> {
    Ok(CommittedTime::new(HlcTimestamp::new(100, 3)?, 99)?)
}

pub(super) fn healthy(operation: &str, fields: &[(&str, V<'_>)]) -> TestResult<P> {
    let actual = prepare(
        Input {
            operation,
            id: b"id",
            fields,
        },
        &[],
        None,
        stamp()?,
        limits(),
    );
    assert!(
        actual.is_ok(),
        "healthy logical caller: {:?}",
        actual.as_ref().err()
    );
    Ok(actual?)
}

pub(super) fn base_fields(operation: &str) -> Vec<(&'static str, V<'static>)> {
    let guard = [
        ("lease_token", V::Scalar(S::Bytes(b"lease"))),
        ("fencing_token", V::Scalar(S::Integer(7))),
    ];
    match operation {
        "create" => vec![("type", V::Scalar(S::Bytes(b"type")))],
        "transition" => vec![
            ("from", V::Scalar(S::Bytes(b"running"))),
            ("to", V::Scalar(S::Bytes(b"waiting"))),
            guard[0],
            guard[1],
        ],
        "cancel" => vec![guard[1]],
        _ => guard.to_vec(),
    }
}

pub(super) fn payload(operation: &Operation) -> Option<&[u8]> {
    match operation {
        Operation::Create(value) => value.payload.as_deref(),
        Operation::Transition(value) => value.payload.as_deref(),
        Operation::Complete(value) | Operation::Fail(value) => value.payload.as_deref(),
        Operation::RetryWithPolicy(value) => value.payload.as_deref(),
        _ => None,
    }
}

pub(super) fn secondary(operation: &Operation) -> Option<&[u8]> {
    match operation {
        Operation::Complete(value) | Operation::Fail(value) => value.value.as_deref(),
        Operation::RetryWithPolicy(value) => value.error.as_deref(),
        Operation::Cancel(value) => value.error.as_deref(),
        _ => None,
    }
}

pub(super) fn order_nodes() -> Vec<super::N> {
    use super::N;
    vec![
        N::Atom {
            bytes: 0..8,
            utf8: true,
        },
        N::Integer(123),
        N::Atom {
            bytes: 8..13,
            utf8: true,
        },
        N::Binary { bytes: 13..17 },
        N::Binary { bytes: 17..20 },
        N::EmptyList,
        N::List {
            heads: 0..2,
            tail: 5,
        },
        N::Map { entries: 2..6 },
    ]
}

pub(super) const ORDER_BYTES: &[u8] = b"order_iditemsbookpen";
pub(super) const ORDER_EDGES: &[usize] = &[3, 4, 0, 1, 2, 6];
pub(super) const ORDER_STORED: &[u8] =
    b"FSV2\x02\x83t\0\0\0\x02w\x05itemsl\0\0\0\x02m\0\0\0\x04bookm\0\0\0\x03penjw\x08order_ida\x7b";
