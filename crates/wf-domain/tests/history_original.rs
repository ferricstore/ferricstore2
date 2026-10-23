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

// Pinned HistoryEventTest at 66586c596e271408d9184af32eb31cd7218c126e.
// Source async:true/flow, with no local setup; five observations in source order.
use wf_domain::history::original::original_history_event_ms;

#[test]
fn parses_millisecond_prefix_from_generated_event_ids() {
    let actual = original_history_event_ms(Raw::Binary(b"12345-7"));
    assert_eq!(
        (actual.is_negative(), actual.digits()),
        (false, b"12345".as_slice())
    );
}

#[test]
fn parses_bare_millisecond_event_ids() {
    let actual = original_history_event_ms(Raw::Binary(b"98765"));
    assert_eq!(
        (actual.is_negative(), actual.digits()),
        (false, b"98765".as_slice())
    );
}

#[test]
fn returns_zero_for_malformed_and_non_binary_event_ids() {
    let actual = original_history_event_ms(Raw::Binary(b"abc-1"));
    assert_eq!(
        (actual.is_negative(), actual.digits()),
        (false, b"0".as_slice())
    );
    let actual = original_history_event_ms(Raw::Binary(b"123abc"));
    assert_eq!(
        (actual.is_negative(), actual.digits()),
        (false, b"0".as_slice())
    );
    let actual = original_history_event_ms(Raw::Nil);
    assert_eq!(
        (actual.is_negative(), actual.digits()),
        (false, b"0".as_slice())
    );
}

#[test]
fn history_millis_preserves_sign_zero_and_arbitrary_integer_width() {
    for (input, negative, digits) in [
        (b"+00012-4".as_slice(), false, b"12".as_slice()),
        (b"-00012".as_slice(), true, b"12".as_slice()),
        (b"00012--x".as_slice(), false, b"12".as_slice()),
        (b"0".as_slice(), false, b"0".as_slice()),
        (b"-0".as_slice(), false, b"0".as_slice()),
        (b"+000".as_slice(), false, b"0".as_slice()),
        (b"-000-1".as_slice(), false, b"0".as_slice()),
    ] {
        let actual = original_history_event_ms(Raw::Binary(input));
        assert_eq!(
            (actual.is_negative(), actual.digits()),
            (negative, digits),
            "{input:?}"
        );
    }

    let magnitude = [b'9'; 96];
    let positive = original_history_event_ms(Raw::Binary(&magnitude));
    assert_eq!(
        (positive.is_negative(), positive.digits()),
        (false, magnitude.as_slice())
    );
    let mut signed = [b'9'; 97];
    signed[0] = b'-';
    let negative = original_history_event_ms(Raw::Binary(&signed));
    assert_eq!(
        (negative.is_negative(), negative.digits()),
        (true, &signed[1..])
    );
    assert_eq!(magnitude, [b'9'; 96]);
    assert_eq!(&signed[1..], magnitude.as_slice());
}

#[test]
fn history_millis_accepts_only_source_remainders_and_binary_inputs() {
    for (input, negative) in [
        (b"12-".as_slice(), false),
        (b"-12--x".as_slice(), true),
        (b"+0012-\xff".as_slice(), false),
        (b"12- \0".as_slice(), false),
    ] {
        let actual = original_history_event_ms(Raw::Binary(input));
        assert_eq!(
            (actual.is_negative(), actual.digits()),
            (negative, b"12".as_slice()),
            "{input:?}"
        );
    }

    for input in [
        b"".as_slice(),
        b"+",
        b"-",
        b"--1",
        b"++1",
        b"+-1",
        b"-+1",
        b" 12",
        b"12 ",
        b"\t12",
        b"12\t",
        b"12.0",
        b"1_2",
        b"12e1",
        b"0x12",
        b"\xd9\xa1",
        b"\xef\xbc\x91",
        b"12+suffix",
        b"12\xff",
        b"\xff-12",
    ] {
        let actual = original_history_event_ms(Raw::Binary(input));
        assert_eq!(
            (actual.is_negative(), actual.digits()),
            (false, b"0".as_slice()),
            "{input:?}"
        );
    }

    let magnitude = [0xff; 17];
    let nested = [Raw::Binary(b"12345")];
    let nil = Raw::Nil;
    for input in [
        Raw::Integer(12_345),
        Raw::BigInteger {
            negative: true,
            little_endian: &magnitude,
        },
        Raw::Atom("12345"),
        Raw::Nil,
        Raw::Map(&[]),
        Raw::List(&nested),
        Raw::Tuple(&nested),
        Raw::ImproperList {
            prefix: &nested,
            tail: &nil,
        },
        Raw::Other,
    ] {
        let actual = original_history_event_ms(input);
        assert_eq!(
            (actual.is_negative(), actual.digits()),
            (false, b"0".as_slice())
        );
    }
}

#[test]
fn history_millis_borrows_only_normalized_nonzero_digit_prefix() {
    let backing = *b"-00012345-\xff";
    let expected_digits = &backing[4..9];
    let actual = {
        let descriptor = Raw::Binary(&backing);
        original_history_event_ms(descriptor)
    };
    assert_eq!(
        (actual.is_negative(), actual.digits()),
        (true, b"12345".as_slice())
    );
    assert!(std::ptr::eq(actual.digits(), expected_digits));
    assert_eq!(backing, *b"-00012345-\xff");
}
