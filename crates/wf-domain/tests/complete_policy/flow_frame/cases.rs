//! Payload identity and malformed-wire controls independent of the implementation.

use super::{
    AdmissionLimits, Error, HlcTimestamp, PolicySchemaCharge, Raw, RequestError, TestResult,
    assert_alias, assert_distinct, assert_malformed, digest, fixture, input, input_with, intent,
    prepare, prepare_original_flow_policy_replace_frame_bytes, profile, put_u32, restore,
    restore_original_flow_policy_replace_frame_header, scan_original_flow_policy_replace_frame,
    zero_stamp,
};
use wf_types::version::CommittedTime;

pub(super) fn assert_integer_and_atom_aliases() -> TestResult {
    assert_alias(Raw::Nil, Raw::Atom("nil"))?;
    for value in [i128::MIN, -4_294_967_296, -1, 0, 1, 255, 256, i128::MAX] {
        let magnitude = value.unsigned_abs().to_le_bytes();
        let mut padded = magnitude.to_vec();
        padded.extend_from_slice(&[0, 0, 0]);
        assert_alias(
            Raw::Integer(value),
            Raw::BigInteger {
                negative: value < 0,
                little_endian: &padded,
            },
        )?;
    }
    for magnitude in [&[][..], &[0][..], &[0, 0, 0][..]] {
        for negative in [false, true] {
            assert_alias(
                Raw::Integer(0),
                Raw::BigInteger {
                    negative,
                    little_endian: magnitude,
                },
            )?;
        }
    }
    for digits in [255, 256] {
        let mut magnitude = vec![17; digits];
        let mut padded = magnitude.clone();
        padded.extend_from_slice(&[0, 0]);
        assert_alias(
            Raw::BigInteger {
                negative: false,
                little_endian: &magnitude,
            },
            Raw::BigInteger {
                negative: false,
                little_endian: &padded,
            },
        )?;
        assert_distinct(
            Raw::BigInteger {
                negative: false,
                little_endian: &magnitude,
            },
            Raw::BigInteger {
                negative: true,
                little_endian: &magnitude,
            },
        )?;
        magnitude[digits - 1] = 18;
        assert_distinct(
            Raw::BigInteger {
                negative: false,
                little_endian: &magnitude,
            },
            Raw::BigInteger {
                negative: false,
                little_endian: &padded,
            },
        )?;
    }
    for (left, right) in [
        (Raw::Nil, Raw::Integer(0)),
        (Raw::Atom(""), Raw::Binary(b"")),
        (Raw::Atom("nil"), Raw::Binary(b"nil")),
        (Raw::Atom("A"), Raw::Atom("a")),
        (Raw::Binary(b"\xff\0"), Raw::Binary(b"\xff")),
    ] {
        assert_distinct(left, right)?;
    }
    assert_alias(Raw::Atom("\u{212a}"), Raw::Atom("\u{212a}"))?;
    Ok(())
}

pub(super) fn assert_list_and_recursive_map_identity() -> TestResult {
    let heads = [Raw::Integer(1), Raw::Binary(b"x"), Raw::Nil];
    let end = Raw::List(&heads[2..]);
    let middle = Raw::ImproperList {
        prefix: &heads[1..2],
        tail: &end,
    };
    assert_alias(
        Raw::List(&heads),
        Raw::ImproperList {
            prefix: &heads[..1],
            tail: &middle,
        },
    )?;
    assert_alias(
        Raw::List(&heads),
        Raw::ImproperList {
            prefix: &heads,
            tail: &Raw::List(&[]),
        },
    )?;
    let list_element = [Raw::List(&heads[..1]), Raw::Binary(b"x"), Raw::Nil];
    assert_distinct(Raw::List(&heads), Raw::List(&list_element))?;
    assert_distinct(Raw::List(&heads), Raw::Tuple(&heads))?;
    assert_distinct(Raw::List(&[]), Raw::Nil)?;
    assert_distinct(
        Raw::ImproperList {
            prefix: &heads,
            tail: &Raw::Nil,
        },
        Raw::List(&heads),
    )?;
    assert_alias(
        Raw::ImproperList {
            prefix: &heads,
            tail: &Raw::Nil,
        },
        Raw::ImproperList {
            prefix: &heads,
            tail: &Raw::Atom("nil"),
        },
    )?;
    let first = [
        (Raw::Binary(b"b"), Raw::Integer(2)),
        (Raw::Binary(b"a"), Raw::Integer(1)),
    ];
    let second = [first[1], first[0]];
    let left = [
        (Raw::Tuple(&heads), Raw::Map(&first)),
        (Raw::Map(&first), Raw::List(&heads)),
    ];
    let right = [
        (Raw::Map(&second), Raw::List(&heads)),
        (Raw::Tuple(&heads), Raw::Map(&second)),
    ];
    assert_alias(Raw::Map(&left), Raw::Map(&right))?;
    assert_distinct(Raw::Map(&first), Raw::List(&[]))?;
    let limits = profile(1_000_000)?;
    let mut charge = PolicySchemaCharge::new(limits);
    assert_eq!(
        prepare_original_flow_policy_replace_frame_bytes(
            input(
                Raw::ImproperList {
                    prefix: &[],
                    tail: &Raw::Nil
                },
                Raw::Map(&[])
            )?,
            limits,
            AdmissionLimits::default(),
            &mut charge,
        )
        .err(),
        Some(RequestError::Encoding(Error::Malformed))
    );
    Ok(())
}

pub(super) fn assert_duplicate_semantic_keys() -> TestResult {
    let magnitude = [1, 0, 0];
    assert_key_alias(
        Raw::Integer(1),
        Raw::BigInteger {
            negative: false,
            little_endian: &magnitude,
        },
    )?;
    assert_key_alias(Raw::Nil, Raw::Atom("nil"))?;
    let heads = [Raw::Binary(b"x")];
    assert_key_alias(
        Raw::List(&heads),
        Raw::ImproperList {
            prefix: &heads,
            tail: &Raw::List(&[]),
        },
    )?;
    let rows = [
        (Raw::Binary(b"b"), Raw::Integer(2)),
        (Raw::Binary(b"a"), Raw::Integer(1)),
    ];
    let reversed = [rows[1], rows[0]];
    assert_key_alias(Raw::Map(&rows), Raw::Map(&reversed))?;
    assert_key_alias(Raw::Tuple(&[Raw::Nil]), Raw::Tuple(&[Raw::Atom("nil")]))?;
    // Unlike map keys, keyword tuple repetitions are represented list content.
    let pair = [Raw::Atom("x"), Raw::Integer(1)];
    let keywords = [Raw::Tuple(&pair), Raw::Tuple(&pair)];
    let _ = prepare(Raw::Nil, Raw::List(&keywords))?;
    for value in [Raw::Other, Raw::Tuple(&[Raw::Other, Raw::Integer(-7)])] {
        let limits = profile(1_000_000)?;
        let mut charge = PolicySchemaCharge::new(limits);
        let ignored = [(Raw::Binary(b"ignored"), value)];
        assert_eq!(
            prepare_original_flow_policy_replace_frame_bytes(
                input(Raw::Nil, Raw::Map(&ignored))?,
                limits,
                AdmissionLimits::default(),
                &mut charge,
            )
            .err(),
            Some(RequestError::Encoding(Error::UnsupportedTag))
        );
    }
    Ok(())
}

fn assert_key_alias(left: Raw<'_>, right: Raw<'_>) -> TestResult {
    // Each operand must first be accepted alone; two malformed keys cannot prove equality.
    for key in [left, right] {
        let _ = intent(Raw::Nil, Raw::Map(&[(key, Raw::Nil)]))?;
    }
    let duplicate = [(left, Raw::Nil), (right, Raw::Integer(1))];
    let limits = profile(1_000_000_000)?;
    let mut charge = PolicySchemaCharge::new(limits);
    assert_eq!(
        prepare_original_flow_policy_replace_frame_bytes(
            input(Raw::Nil, Raw::Map(&duplicate))?,
            limits,
            AdmissionLimits::default(),
            &mut charge,
        )
        .err(),
        Some(RequestError::Encoding(Error::Malformed))
    );
    Ok(())
}

pub(super) fn assert_presence_and_order() -> TestResult {
    let forward = [(Raw::Binary(b"a"), Raw::Nil), (Raw::Binary(b"b"), Raw::Nil)];
    let reverse = [forward[1], forward[0]];
    let (first, _, first_range) = prepare(Raw::Nil, Raw::Map(&forward))?;
    let (second, _, second_range) = prepare(Raw::Atom("nil"), Raw::Map(&reverse))?;
    assert_eq!(&first[first_range.clone()], &second[second_range.clone()]);
    assert_ne!(&first[first_range.end..], &second[second_range.end..]);
    assert_eq!(
        &second[second_range.end + 20..],
        &[0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0]
    );
    for guard in [
        Raw::Integer(-1),
        Raw::Integer(0),
        Raw::Integer(1),
        Raw::Integer(i128::MAX),
        Raw::Binary(b"1"),
    ] {
        assert_distinct(Raw::Nil, guard)?;
    }
    let ab = [Raw::Binary(b"a"), Raw::Binary(b"b")];
    let ba = [ab[1], ab[0]];
    assert_distinct(Raw::List(&ab), Raw::List(&ba))?;
    for patch in [Raw::Nil, Raw::List(&[]), Raw::Tuple(&[]), Raw::Binary(b"")] {
        assert_ne!(intent(Raw::Nil, patch)?, intent(Raw::Nil, Raw::Map(&[]))?);
    }
    Ok(())
}

pub(super) fn assert_header_binding() -> TestResult {
    let baseline = intent(Raw::Nil, Raw::Map(&[]))?;
    for case in 0..6 {
        let ids: [&[u8]; 4] = match case {
            0 | 4 => [b"a", b"b", b"c", b"q"],
            1 => [b"a", b"B", b"c", b"q"],
            2 => [b"a", b"b", b"C", b"q"],
            3 => [b"a", b"b", b"c", b"Q"],
            _ => [b"z", b"b", b"c", b"q"],
        };
        let stamp = if case == 4 {
            CommittedTime::new(HlcTimestamp::new(9, 7)?, 8)?
        } else {
            zero_stamp()?
        };
        let limits = profile(1_000_000)?;
        let mut charge = PolicySchemaCharge::new(limits);
        let (frame, offset, range) = prepare_original_flow_policy_replace_frame_bytes(
            input_with(Raw::Nil, Raw::Map(&[]), super::KEY, ids, stamp)?,
            limits,
            AdmissionLimits::default(),
            &mut charge,
        )?;
        assert_eq!(&frame[offset..offset + 32], &[0; 32]);
        assert_eq!(frame[range.clone()] == baseline, case != 5);
        // Matching mathematical digest bytes are not a ports SHA verification certificate.
        let mut verification = PolicySchemaCharge::new(limits);
        let header = restore(&frame, [0; 32], limits, &mut verification)?;
        assert_eq!(header.scope().tenant().as_bytes(), ids[0]);
        assert_eq!(header.scope().group().as_bytes(), ids[1]);
        assert_eq!(header.scope().recovery().as_bytes(), ids[2]);
        assert_eq!(header.request().as_bytes(), ids[3]);
        assert_eq!(header.stamp(), stamp);
        assert_eq!(header.original_intent(), &frame[range]);
    }
    for key in [&b""[..], &b"f:{f}:policy:other"[..], &b"wrong"[..]] {
        let limits = profile(1_000_000)?;
        let mut charge = PolicySchemaCharge::new(limits);
        let (frame, _, range) = prepare_original_flow_policy_replace_frame_bytes(
            input_with(
                Raw::Nil,
                Raw::Map(&[]),
                key,
                [b"a", b"b", b"c", b"q"],
                zero_stamp()?,
            )?,
            limits,
            AdmissionLimits::default(),
            &mut charge,
        )?;
        assert_ne!(&frame[range], baseline);
    }
    Ok(())
}

pub(super) fn assert_outer_and_node_tampering() -> TestResult {
    let original = fixture(0)?;
    let limits = profile(1_000_000)?;
    for end in 0..original.len() {
        let mut charge = PolicySchemaCharge::new(limits);
        assert!(
            scan_original_flow_policy_replace_frame(
                &original[..end],
                limits,
                AdmissionLimits::default(),
                &mut charge,
            )
            .is_err(),
            "truncation {end}"
        );
    }
    let mut trailing = original.clone();
    trailing.push(0);
    let mut charge = PolicySchemaCharge::new(limits);
    assert_eq!(
        scan_original_flow_policy_replace_frame(
            &trailing,
            limits,
            AdmissionLimits::default(),
            &mut charge,
        )
        .err(),
        Some(RequestError::Encoding(Error::TrailingBytes))
    );
    for (offset, byte) in [(0, 0), (11, b'z'), (87, 0), (126, b'z'), (154, 0)] {
        let mut changed = original.clone();
        changed[offset] = byte;
        assert_malformed(&changed)?;
    }
    for (offset, value) in [
        (79, 79),
        (79, 81),
        (83, 19),
        (83, 21),
        (145, 4),
        (145, 6),
        (150, 16),
        (150, 18),
        (155, 1),
        (159, 1),
        (163, 16),
        (163, 18),
        (167, 0),
        (167, 2),
        (171, 11),
        (171, 13),
        (175, 19),
        (175, 21),
        (179, 62),
        (179, 64),
        (183, 1),
    ] {
        let mut changed = original.clone();
        put_u32(&mut changed, offset, value);
        assert_malformed(&changed)?;
    }
    let mut reserved = original.clone();
    reserved[149] = 8;
    let mut charge = PolicySchemaCharge::new(limits);
    assert_eq!(
        scan_original_flow_policy_replace_frame(
            &reserved,
            limits,
            AdmissionLimits::default(),
            &mut charge,
        )
        .err(),
        Some(RequestError::Encoding(Error::UnsupportedTag))
    );
    let mut charge = PolicySchemaCharge::new(limits);
    let scanned = scan_original_flow_policy_replace_frame(
        &original,
        limits,
        AdmissionLimits::default(),
        &mut charge,
    )?;
    assert_eq!(
        restore_original_flow_policy_replace_frame_header(
            scanned,
            [0; 32],
            limits,
            AdmissionLimits::default(),
            &mut charge,
        )
        .err(),
        Some(RequestError::IntentMismatch)
    );
    assert_eq!(charge.g(), 2_253); // L1723 + tenant130 + intentSHA208 + digest192.
    assert_eq!(&original[29..61], &digest(0)?);
    Ok(())
}

pub(super) fn assert_three_cycle_and_nested_key_tampering() -> TestResult {
    let original = fixture(2)?;
    assert_eq!(&original[256..268], &[0, 0, 0, 2, 0, 0, 0, 0, 0, 0, 0, 1]);
    assert_eq!(&original[268..280], &[0, 0, 0, 1, 0, 0, 0, 2, 0, 0, 0, 0]);
    for offset in [256, 260, 264] {
        let mut out_of_range = original.clone();
        put_u32(&mut out_of_range, offset, 3);
        assert_malformed(&out_of_range)?;
    }
    for (offset, value) in [
        (256, 0),
        (260, 2),
        (264, 0),
        (268, 0),
        (272, 1),
        (276, 2),
        (248, 62),
        (252, 2),
    ] {
        let mut changed = original.clone();
        put_u32(&mut changed, offset, value);
        assert_malformed(&changed)?;
    }
    let mut one_swapped = original.clone();
    one_swapped[256..268].copy_from_slice(&original[268..280]);
    assert_malformed(&one_swapped)?;
    let mut both_swapped = one_swapped;
    both_swapped[268..280].copy_from_slice(&original[256..268]);
    let limits = profile(1_000_000)?;
    let mut charge = PolicySchemaCharge::new(limits);
    let alternate = scan_original_flow_policy_replace_frame(
        &both_swapped,
        limits,
        AdmissionLimits::default(),
        &mut charge,
    )?;
    assert_eq!(alternate.original_intent(), &original[87..236]);
    assert_ne!(&both_swapped[236..], &original[236..]);
    let mut duplicate = fixture(1)?;
    duplicate[207] = b'a';
    assert_malformed(&duplicate)?;
    let mut reversed = fixture(1)?;
    reversed[192] = b'b';
    reversed[207] = b'a';
    assert_malformed(&reversed)?;
    for offset in [163, 167, 171, 175, 179, 183, 187] {
        let mut changed = original.clone();
        put_u32(&mut changed, offset, 0);
        assert_malformed(&changed)?;
    }
    let left = [(Raw::Atom("x"), Raw::Integer(1))];
    let right = [(Raw::Atom("x"), Raw::Integer(2))];
    let nested = [(Raw::Map(&left), Raw::Nil), (Raw::Map(&right), Raw::Nil)];
    let (mut frame, _, _) = prepare(Raw::Nil, Raw::Map(&nested))?;
    let needle = [0, 0, 0, 11, 1, 0, 0, 0, 0, 1, 2];
    let position = frame
        .windows(needle.len())
        .position(|bytes| bytes == needle)
        .ok_or("fixed integer two")?;
    frame[position + 10] = 1;
    // Nested map IDs differ; equal mathematical keys must nevertheless be rejected.
    assert_malformed(&frame)?;
    Ok(())
}

pub(super) fn assert_mixed_class_ranks() -> TestResult {
    let rows = [
        (Raw::Map(&[]), Raw::Nil),
        (Raw::List(&[]), Raw::Nil),
        (Raw::Tuple(&[]), Raw::Nil),
        (Raw::Binary(b"a"), Raw::Nil),
        (Raw::Nil, Raw::Nil),
        (Raw::Integer(0), Raw::Nil),
    ];
    let (frame, _, _) = prepare(Raw::Nil, Raw::Map(&rows))?;
    let mut tags = Vec::new();
    for row in 0..6 {
        let offset: [u8; 4] = frame[163 + 8 * row..167 + 8 * row].try_into()?;
        let child = 150 + usize::try_from(u32::from_be_bytes(offset))?;
        tags.push(frame[child + 4]);
    }
    assert_eq!(tags, [1, 0, 3, 6, 5, 4]);
    Ok(())
}

pub(super) fn assert_scalar_canonical_tampering() -> TestResult {
    for (expected, offset, byte) in [
        (Raw::Integer(1), 150, 2),
        (Raw::Integer(1), 155, 0),
        (Raw::Integer(0), 150, 1),
        (Raw::Atom("x"), 154, 255),
    ] {
        let (mut frame, _, _) = prepare(expected, Raw::Map(&[]))?;
        frame[offset] = byte;
        assert_malformed(&frame)?;
    }
    let (mut spelled_nil, _, _) = prepare(Raw::Atom("abc"), Raw::Map(&[]))?;
    spelled_nil[154..157].copy_from_slice(b"nil");
    assert_malformed(&spelled_nil)?;
    let prefix = [Raw::Nil];
    let (mut improper, _, _) = prepare(
        Raw::ImproperList {
            prefix: &prefix,
            tail: &Raw::Tuple(&[]),
        },
        Raw::Map(&[]),
    )?;
    let limits = profile(1_000_000)?;
    let mut positive = PolicySchemaCharge::new(limits);
    assert!(
        scan_original_flow_policy_replace_frame(
            &improper,
            limits,
            AdmissionLimits::default(),
            &mut positive,
        )
        .is_ok()
    );
    assert_eq!(improper[175], 6); // Tuple[] terminal has the same13B width as List[].
    improper[175] = 5;
    assert_malformed(&improper)?;
    for (offset, byte, error) in [
        (5, 2, Error::UnsupportedVersion),
        (6, 7, Error::UnsupportedTag),
        (28, 2, Error::UnsupportedVersion),
    ] {
        let mut frame = fixture(0)?;
        frame[offset] = byte;
        let limits = profile(1_000_000)?;
        let mut charge = PolicySchemaCharge::new(limits);
        assert_eq!(
            scan_original_flow_policy_replace_frame(
                &frame,
                limits,
                AdmissionLimits::default(),
                &mut charge,
            )
            .err(),
            Some(RequestError::Encoding(error))
        );
    }
    Ok(())
}
