//! Original row reading scenarios from pinned66586c596e27.
#![forbid(unsafe_code)]
use wf_domain::metadata::{
    OriginalAttributeValue as A, OriginalMetadataInput as I, OriginalMetadataScalar as S,
};
use wf_ports::projection::query_row::{
    OriginalQueryDecodePlan as Plan, OriginalQueryLocation as Location,
    OriginalQueryLocator as Locator, OriginalQueryRead as Read, OriginalQueryReadLimits as RL,
    OriginalQueryRecord as Record, OriginalQueryRecordError as E, OriginalQueryRecordLimits as L,
    OriginalQuerySelection as Selection, OriginalQueryStateSelection as StateSelection,
    decode_original_query_for_query as query, decode_original_query_reference as reference,
    decode_original_query_row as decode, encode_original_query_row as encode,
    prepare_original_query_internal_record as internal, relocate_original_query_row as relocate,
};
type TestResult<T = ()> = Result<T, String>;
mod adversarial;
mod fixtures;
mod relocation;
mod resources;
mod selectors;
mod source;

fn limits() -> RL {
    RL {
        maximum_input_bytes: 100_000,
        maximum_output_bytes: 100_000,
        maximum_work_units: 20_000_000,
        maximum_temporary_bytes: 2_000_000,
    }
}
fn location(expiry: Option<u64>) -> Location<'static> {
    Location {
        flow_id: b"run-1",
        version: 7,
        raft_index: 99,
        source_tag: 0,
        source_id: 3,
        offset: 128,
        value_size: 512,
        frame_size: None,
        checksum: &[1; 32],
        expire_at_ms: expiry,
        segment_generation: Some(4),
    }
}
fn row(
    encoded: Read<wf_ports::projection::query_row::DecodedOriginalQueryRow<'_>>,
) -> TestResult<wf_ports::projection::query_row::DecodedOriginalQueryRow<'_>> {
    encoded
        .into_row()
        .ok_or_else(|| "unexpected expiry".to_owned())
}
fn scalar_eq(a: I<'_, '_>, b: I<'_, '_>) -> bool {
    match (a, b) {
        (I::Nil, I::Nil) => true,
        (I::Boolean(a), I::Boolean(b)) => a == b,
        (I::Integer(a), I::Integer(b)) => a == b,
        (I::Binary(a), I::Binary(b)) => a == b,
        (I::Float(a), I::Float(b)) => a.to_bits() == b.to_bits(),
        _ => false,
    }
}
fn value_eq(raw: I<'_, '_>, value: &A<'_>) -> bool {
    match value {
        A::Scalar(s) => scalar_eq(
            raw,
            match s {
                S::Nil => I::Nil,
                S::Boolean(v) => I::Boolean(*v),
                S::Integer(v) => I::Integer(i128::from(*v)),
                S::Float(v) => I::Float(*v),
                S::Binary(v) => I::Binary(v),
            },
        ),
        A::List(values) => {
            matches!(raw,I::List(list) if list.len()==values.len() && list.iter().zip(values).all(|(a,b)|matches!(a,I::Binary(a) if a==b)))
        }
    }
}
fn record_eq(record: &Record<'_>, expected: I<'_, '_>) -> bool {
    let I::Map(rows) = expected else {
        return false;
    };
    if record.field_count() != rows.len() {
        return false;
    }
    rows.iter().all(|(key, value)| match key {
        I::Atom("attributes") => attributes_eq(record, *value),
        I::Atom("state_meta") => states_eq(record, *value),
        I::Atom("indexed_attributes") => indexed_eq(record, *value),
        I::Atom("indexed_state_meta") => {
            matches!(value,I::Binary(value) if record.indexed_state_meta()==Some(*value))
        }
        I::Atom(name) => record
            .field(name)
            .is_some_and(|actual| scalar_eq(*value, actual)),
        _ => false,
    })
}
fn attributes_eq(record: &Record<'_>, expected: I<'_, '_>) -> bool {
    let I::Map(values) = expected else {
        return false;
    };
    let Some(actual) = record.attributes() else {
        return false;
    };
    if actual.len() != values.len() {
        return false;
    }
    values.iter().all(|(key,value)|matches!(key,I::Binary(k) if record.attribute(k).is_some_and(|a|value_eq(*value,a))))
}
fn states_eq(record: &Record<'_>, expected: I<'_, '_>) -> bool {
    let I::Map(states) = expected else {
        return false;
    };
    let Some(actual) = record.state_meta() else {
        return false;
    };
    if actual.len() != states.len() {
        return false;
    }
    for (state, values) in states {
        let (I::Binary(state), I::Map(values)) = (state, values) else {
            return false;
        };
        let Some((_, entries)) = actual.iter().find(|s| s.0 == *state) else {
            return false;
        };
        if entries.len() != values.len() {
            return false;
        }
        for (key, value) in *values {
            let I::Binary(key) = key else {
                return false;
            };
            let Some(actual) = record.state_field(state, key) else {
                return false;
            };
            if !value_eq(*value, actual) {
                return false;
            }
        }
    }
    true
}
fn indexed_eq(record: &Record<'_>, expected: I<'_, '_>) -> bool {
    let I::List(values) = expected else {
        return false;
    };
    let Some(names) = record.indexed_attributes() else {
        return false;
    };
    values.len() == names.len()
        && values
            .iter()
            .zip(names)
            .all(|(a, b)| matches!(a,I::Binary(a) if a==b))
}
fn expected_field(name: &str) -> I<'static, 'static> {
    fixtures::RECORD
        .iter()
        .find_map(|&(key, value)| matches!(key,I::Atom(n) if n==name).then_some(value))
        .unwrap_or(I::Other)
}
