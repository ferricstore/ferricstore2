//! Pinned `HistoryEntryTest` at 66586c596e271408d9184af32eb31cd7218c126e.
//! Source: `apps/ferricstore/test/ferricstore/flow/history_entry_test.exs`.
//! Source async:true/flow, with no local setup. The first four cases retain all
//! four complete source observations, in order. Public history and full profiles
//! remain separate from these already captured flat proper entry views.

#![forbid(unsafe_code)]

use wf_domain::history::original::{OriginalHistoryInput as Input, fields_to_map, to_tuple};
use wf_domain::policy::complete::RawFlowPolicyValue as Raw;

#[test]
fn converts_tuple_history_entries_to_event_id_and_fields_map() {
    let event_id = Raw::Binary(b"1-0");
    let fields = [
        Raw::Binary(b"event"),
        Raw::Binary(b"created"),
        Raw::Binary(b"state"),
        Raw::Binary(b"queued"),
    ];
    let actual = to_tuple(Input::Tuple {
        event_id: &event_id,
        fields: &fields,
    });
    assert!(actual.is_some_and(|entry| {
        matches!(entry.event_id, Raw::Binary(b"1-0"))
            && entry.fields.len() == 2
            && matches!(
                entry.fields.get(b"event".as_slice()),
                Some(Raw::Binary(b"created"))
            )
            && matches!(
                entry.fields.get(b"state".as_slice()),
                Some(Raw::Binary(b"queued"))
            )
    }));
}

#[test]
fn converts_list_history_entries_to_event_id_and_fields_map() {
    let flat = [
        Raw::Binary(b"2-0"),
        Raw::Binary(b"event"),
        Raw::Binary(b"completed"),
        Raw::Binary(b"state"),
        Raw::Binary(b"completed"),
    ];
    let actual = to_tuple(Input::List(&flat));
    assert!(actual.is_some_and(|entry| {
        matches!(entry.event_id, Raw::Binary(b"2-0"))
            && entry.fields.len() == 2
            && matches!(
                entry.fields.get(b"event".as_slice()),
                Some(Raw::Binary(b"completed"))
            )
            && matches!(
                entry.fields.get(b"state".as_slice()),
                Some(Raw::Binary(b"completed"))
            )
    }));
}

#[test]
fn last_duplicate_field_wins() {
    let fields = [
        Raw::Binary(b"event"),
        Raw::Binary(b"created"),
        Raw::Binary(b"event"),
        Raw::Binary(b"retry"),
    ];
    let actual = fields_to_map(&fields);
    assert!(
        actual.len() == 1 && matches!(actual.get(b"event".as_slice()), Some(Raw::Binary(b"retry")))
    );
}

#[test]
fn malformed_stored_fields_do_not_crash_history_reads() {
    let fields = [
        Raw::Atom("invalid_key"),
        Raw::Binary(b"ignored"),
        Raw::Binary(b"event"),
        Raw::Binary(b"created"),
        Raw::Binary(b"dangling"),
    ];
    let actual = fields_to_map(&fields);
    assert!(
        actual.len() == 1
            && matches!(
                actual.get(b"event".as_slice()),
                Some(Raw::Binary(b"created"))
            )
    );
}

#[test]
fn borrowed_history_values_preserve_opaque_descriptors_and_binary_key_equality() {
    let first_backing = [0xff, 0];
    let last_backing = [7, 0xff, 0];
    let first_key = first_backing.as_slice();
    let last_key = &last_backing[1..];
    assert_eq!(first_key, last_key);
    assert_ne!(first_key.as_ptr(), last_key.as_ptr());
    let tuple = [Raw::Atom("retained"), Raw::Integer(9)];
    let fields = [
        Raw::Binary(first_key),
        Raw::Other,
        Raw::Binary(b""),
        Raw::Map(&[]),
        Raw::Binary(last_key),
        Raw::Tuple(&tuple),
    ];
    let actual = fields_to_map(&fields);
    assert_eq!(actual.len(), 2);
    assert!(actual.contains_key(first_key));
    assert!(actual.contains_key(b"".as_slice()));
    assert!(actual.get_key_value(last_key).is_some_and(|(stored, _)| {
        std::ptr::eq(*stored, first_key) || std::ptr::eq(*stored, last_key)
    }));
    assert!(
        actual
            .get(last_key)
            .is_some_and(|value| std::ptr::eq(*value, &raw const fields[5]))
    );
    assert!(
        actual
            .get(b"".as_slice())
            .is_some_and(|value| std::ptr::eq(*value, &raw const fields[3]))
    );
    drop(actual);
    assert!(matches!(fields[5], Raw::Tuple(value) if std::ptr::eq(value, tuple.as_slice())));
    assert_eq!(first_key, [0xff, 0]);
    assert_eq!(last_key, [0xff, 0]);
}

#[test]
fn history_input_views_preserve_nonbinary_ids_and_flat_list_boundaries() {
    assert!(to_tuple(Input::List(&[])).is_none());
    let event_id = Raw::Other;
    let empty_tuple = to_tuple(Input::Tuple {
        event_id: &event_id,
        fields: &[],
    });
    assert!(empty_tuple.is_some_and(|entry| {
        std::ptr::eq(entry.event_id, &raw const event_id) && entry.fields.is_empty()
    }));
    let singleton = [Raw::Integer(42)];
    assert!(to_tuple(Input::List(&singleton)).is_some_and(|entry| {
        std::ptr::eq(entry.event_id, &raw const singleton[0]) && entry.fields.is_empty()
    }));
    let invalid = [
        Raw::Atom("invalid"),
        Raw::Binary(b"must-not-become-a-key"),
        Raw::Binary(b"dangling"),
    ];
    assert!(
        to_tuple(Input::Tuple {
            event_id: &event_id,
            fields: &invalid
        })
        .is_some_and(|entry| {
            std::ptr::eq(entry.event_id, &raw const event_id) && entry.fields.is_empty()
        })
    );
    let nested = [Raw::Binary(b"event"), Raw::Binary(b"created")];
    let flat = [Raw::Atom("nonbinary-id"), Raw::List(&nested)];
    assert!(to_tuple(Input::List(&flat)).is_some_and(|entry| {
        std::ptr::eq(entry.event_id, &raw const flat[0]) && entry.fields.is_empty()
    }));
    let fields = [
        Raw::Atom("nonbinary-id"),
        Raw::Binary(b"event"),
        Raw::List(&nested),
    ];
    assert!(to_tuple(Input::List(&fields)).is_some_and(|entry| {
        std::ptr::eq(entry.event_id, &raw const fields[0])
            && entry.fields.len() == 1
            && entry
                .fields
                .get(b"event".as_slice())
                .is_some_and(|value| std::ptr::eq(*value, &raw const fields[2]))
    }));
}
