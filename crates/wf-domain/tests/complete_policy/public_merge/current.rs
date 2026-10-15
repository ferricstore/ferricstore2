//! Actual original ETF fixtures and physical-span versus virtual-list controls.

use super::{
    PolicySchemaCharge, Raw, Reason, TestResult, accepted, invoke, invoke_for_type, profile,
    rejected,
};

#[test]
fn original_public_merge_retains_original_terms_without_string_expansion() -> TestResult {
    assert_two_strings()?;
    assert_invalid_current_rescue()?;
    assert_opaque_current()?;
    assert_customer_versions()?;
    assert_zero_head_current()?;
    assert_bitstring_key_identity()?;
    assert_large_own_output()?;
    assert_many_normalized_rules()?;
    Ok(())
}

fn assert_many_normalized_rules() -> TestResult {
    // PostinitialRED source-profile regression: physical Nc6012, deprecated
    // map descriptors6009. Old public4096 remains; only the new parent expands.
    let names = (0..1_000).map(|n| format!("r{n:03}")).collect::<Vec<_>>();
    let rule = [
        (Raw::Atom("enforcement"), Raw::Atom("strict_global")),
        (Raw::Atom("limit"), Raw::Integer(1)),
    ];
    let rules = names
        .iter()
        .map(|name| (Raw::Binary(name.as_bytes()), Raw::Map(&rule)))
        .collect::<Vec<_>>();
    let governance = [(Raw::Atom("limits"), Raw::Map(&rules))];
    let fields = [
        (Raw::Atom("governance"), Raw::Map(&governance)),
        (Raw::Atom("states"), Raw::Map(&[])),
        (Raw::Atom("type"), Raw::Binary(b"r")),
    ];
    let bytes = envelope(1, Raw::Map(&fields))?;
    let expected = minimal(2)?;
    assert_eq!(bytes.len(), 51_081);
    let replace = [Raw::Atom("replace"), Raw::Atom("true")];
    accepted(
        &invoke(Some(&bytes), Raw::List(&[Raw::Tuple(&replace)]), 1_000_000)?,
        1,
        2,
        &expected,
    )
}

fn assert_zero_head_current() -> TestResult {
    // This constructs CURRENT ETF LIST_EXT(0,Binary("v")), whose mathematical
    // term is Binary("v"). Empty-prefix Raw is never admitted as incoming data.
    let tail = Raw::Binary(b"v");
    let fields = [
        (Raw::Atom("states"), Raw::Map(&[])),
        (Raw::Atom("type"), Raw::Binary(b"r")),
        (
            Raw::Atom("version"),
            Raw::ImproperList {
                prefix: &[],
                tail: &tail,
            },
        ),
    ];
    let bytes = envelope(1, Raw::Map(&fields))?;
    let fields = [
        (Raw::Atom("states"), Raw::Map(&[])),
        (Raw::Atom("type"), Raw::Binary(b"r")),
        (Raw::Atom("version"), Raw::Binary(b"v")),
    ];
    let expected = envelope(2, Raw::Map(&fields))?;
    accepted(
        &invoke(Some(&bytes), Raw::List(&[]), 1_000_000)?,
        1,
        2,
        &expected,
    )
}

fn assert_customer_versions() -> TestResult {
    let old = [0, 0, 0, 0, 0, 0, 0, 0, 1];
    let new = [1, 0, 0, 0, 0, 0, 0, 0, 1];
    let old_value = Raw::BigInteger {
        negative: false,
        little_endian: &old,
    };
    let new_value = Raw::BigInteger {
        negative: false,
        little_endian: &new,
    };
    let fields = |version| {
        [
            (Raw::Atom("states"), Raw::Map(&[])),
            (Raw::Atom("type"), Raw::Binary(b"r")),
            (Raw::Atom("version"), version),
        ]
    };
    let bytes = envelope(1, Raw::Map(&fields(old_value)))?;
    let unchanged = envelope(2, Raw::Map(&fields(old_value)))?;
    let changed = envelope(2, Raw::Map(&fields(new_value)))?;
    accepted(
        &invoke(Some(&bytes), Raw::List(&[]), 1_000_000)?,
        1,
        2,
        &unchanged,
    )?;
    let pair = [Raw::Atom("version"), new_value];
    accepted(
        &invoke(Some(&bytes), Raw::List(&[Raw::Tuple(&pair)]), 1_000_000)?,
        1,
        2,
        &changed,
    )?;
    let replace = [Raw::Atom("replace"), Raw::Atom("true")];
    accepted(
        &invoke(Some(&bytes), Raw::List(&[Raw::Tuple(&replace)]), 1_000_000)?,
        1,
        2,
        &minimal(2)?,
    )
}

fn assert_bitstring_key_identity() -> TestResult {
    let deletion = [(Raw::Binary(b"a"), Raw::Nil)];
    let pair = [Raw::Atom("states"), Raw::Map(&deletion)];
    let opts = [Raw::Tuple(&pair)];
    let expected = minimal(2)?;
    for (bits, byte) in [(8, b'a'), (3, 0b0110_0000)] {
        let mut bytes = vec![131, 104, 3];
        term(Raw::Atom("flow_policy_v1"), &mut bytes)?;
        term(Raw::Integer(1), &mut bytes)?;
        bytes.extend_from_slice(&[116, 0, 0, 0, 2]);
        term(Raw::Atom("states"), &mut bytes)?;
        bytes.extend_from_slice(&[116, 0, 0, 0, 1, 77, 0, 0, 0, 1, bits, byte]);
        term(Raw::Map(&[]), &mut bytes)?;
        term(Raw::Atom("type"), &mut bytes)?;
        term(Raw::Binary(b"r"), &mut bytes)?;
        let result = invoke(Some(&bytes), Raw::List(&opts), 1_000_000)?;
        if bits == 8 {
            accepted(&result, 1, 2, &expected)?;
        } else {
            rejected(
                &result,
                Reason::StateName,
                b"ERR flow policy state must be a non-empty string",
            );
        }
    }
    Ok(())
}

fn assert_large_own_output() -> TestResult {
    let names = (0..1_000).map(|n| format!("x{n:03}")).collect::<Vec<_>>();
    let values = names
        .iter()
        .map(|name| Raw::Binary(name.as_bytes()))
        .collect::<Vec<_>>();
    let effects = [
        (Raw::Atom("allowed"), Raw::List(&values)),
        (Raw::Atom("denied"), Raw::List(&values)),
    ];
    let approvals = [
        (Raw::Atom("assignees"), Raw::List(&values)),
        (Raw::Atom("effects"), Raw::List(&values)),
        (Raw::Atom("states"), Raw::List(&values)),
    ];
    let governance = [
        (Raw::Atom("approvals"), Raw::Map(&approvals)),
        (Raw::Atom("effects"), Raw::Map(&effects)),
    ];
    let fields = [
        (Raw::Atom("governance"), Raw::Map(&governance)),
        (Raw::Atom("states"), Raw::Map(&[])),
        (Raw::Atom("type"), Raw::Binary(b"r")),
    ];
    let expected_first = envelope(1, Raw::Map(&fields))?;
    let expected_second = envelope(2, Raw::Map(&fields))?;
    assert_eq!(expected_first.len(), 45_173);
    let pair = [Raw::Atom("governance"), Raw::Map(&governance)];
    let first = invoke(None, Raw::List(&[Raw::Tuple(&pair)]), 1_000_000)?;
    accepted(&first, 0, 1, &expected_first)?;
    let actual = first.0.as_ref().ok_or("large first install")?.encoded();
    accepted(
        &invoke(Some(actual), Raw::List(&[]), 1_000_000)?,
        1,
        2,
        &expected_second,
    )
}

fn assert_two_strings() -> TestResult {
    let bytes = two_strings()?;
    assert_eq!(bytes.len(), 131_133);
    let expected = minimal(1)?;
    assert_eq!(expected.len(), 51);
    for work in [33_554_432, 134_217_728] {
        let limits = profile(work)?;
        let mut charge = PolicySchemaCharge::new(limits);
        let result = invoke_for_type(
            b"r",
            Some(&bytes),
            Raw::List(&[]),
            1_000_000,
            limits,
            &mut charge,
        )?;
        accepted(&result, 0, 1, &expected)?;
        // Nc12/Kc4/Fc0, R3=1024/R4=224 and B45 keep the entire
        // declared physical-based stage below1MiB. Eager131080-cell Layout
        // violates this bound and cannot fit the separate32MiB account.
        assert!(charge.p() < 1_048_576);
        assert_eq!(bytes.len(), 131_133);
    }
    Ok(())
}

pub(super) fn two_strings() -> TestResult<Vec<u8>> {
    let mut out = vec![131, 104, 3];
    term(Raw::Atom("flow_policy_v1"), &mut out)?;
    term(Raw::Integer(0), &mut out)?;
    out.extend_from_slice(&[116, 0, 0, 0, 4]);
    for name in ["a", "b"] {
        term(Raw::Atom(name), &mut out)?;
        out.push(107);
        out.extend_from_slice(&65_535_u16.to_be_bytes());
        out.resize(out.len() + 65_535, 1);
    }
    term(Raw::Atom("states"), &mut out)?;
    term(Raw::Map(&[]), &mut out)?;
    term(Raw::Atom("type"), &mut out)?;
    term(Raw::Binary(b"r"), &mut out)?;
    Ok(out)
}

fn assert_invalid_current_rescue() -> TestResult {
    let fields = [
        (Raw::Atom("max_active_ms"), Raw::Integer(0)),
        (Raw::Atom("states"), Raw::Map(&[])),
        (Raw::Atom("type"), Raw::Binary(b"r")),
    ];
    let bytes = envelope(1, Raw::Map(&fields))?;
    rejected(
        &invoke(Some(&bytes), Raw::List(&[]), 1_000_000)?,
        Reason::MaxActive,
        b"ERR flow max_active_ms must be between 1 and 31536000000 or infinity",
    );
    let max = [Raw::Atom("max_active_ms"), Raw::Integer(2_000)];
    let opts = [Raw::Tuple(&max)];
    accepted(
        &invoke(Some(&bytes), Raw::List(&opts), 1_000_000)?,
        1,
        2,
        &super::cases::policy_with_max(2_000)?,
    )?;
    let replace = [Raw::Atom("replace"), Raw::Atom("true")];
    accepted(
        &invoke(Some(&bytes), Raw::List(&[Raw::Tuple(&replace)]), 1_000_000)?,
        1,
        2,
        &minimal(2)?,
    )
}

fn assert_opaque_current() -> TestResult {
    // The current reader owns genuine float/bitstring terms. No Raw::Other
    // unit marker certifies their hidden payload or incoming identity.
    let mut bytes = vec![131, 104, 3];
    term(Raw::Atom("flow_policy_v1"), &mut bytes)?;
    term(Raw::Integer(1), &mut bytes)?;
    bytes.extend_from_slice(&[116, 0, 0, 0, 4]);
    term(Raw::Atom("a"), &mut bytes)?;
    bytes.push(70);
    bytes.extend_from_slice(&1.25_f64.to_bits().to_be_bytes());
    term(Raw::Atom("b"), &mut bytes)?;
    bytes.extend_from_slice(&[77, 0, 0, 0, 1, 3, 0b1010_0000]);
    term(Raw::Atom("states"), &mut bytes)?;
    term(Raw::Map(&[]), &mut bytes)?;
    term(Raw::Atom("type"), &mut bytes)?;
    term(Raw::Binary(b"r"), &mut bytes)?;
    accepted(
        &invoke(Some(&bytes), Raw::List(&[]), 1_000_000)?,
        1,
        2,
        &minimal(2)?,
    )?;
    let before = bytes.clone();
    let field = [Raw::Atom("max_active_ms"), Raw::Integer(1_000)];
    let result = invoke(Some(&bytes), Raw::List(&[Raw::Tuple(&field)]), 1_000_000)?;
    accepted(&result, 1, 2, &super::cases::policy_with_max(1_000)?)?;
    assert_eq!(bytes, before);
    Ok(())
}

pub(super) fn minimal(generation: u64) -> TestResult<Vec<u8>> {
    let fields = [
        (Raw::Atom("states"), Raw::Map(&[])),
        (Raw::Atom("type"), Raw::Binary(b"r")),
    ];
    envelope(generation, Raw::Map(&fields))
}

pub(super) fn envelope(generation: u64, policy: Raw<'_>) -> TestResult<Vec<u8>> {
    let mut out = vec![131, 104, 3];
    term(Raw::Atom("flow_policy_v1"), &mut out)?;
    term(Raw::Integer(i128::from(generation)), &mut out)?;
    term(policy, &mut out)?;
    Ok(out)
}

pub(super) fn term(value: Raw<'_>, out: &mut Vec<u8>) -> TestResult {
    match value {
        Raw::Nil => term(Raw::Atom("nil"), out)?,
        Raw::Atom(name) => {
            if name.len() <= 255 {
                out.extend_from_slice(&[119, u8::try_from(name.len())?]);
            } else {
                out.push(118);
                out.extend_from_slice(&u16::try_from(name.len())?.to_be_bytes());
            }
            out.extend_from_slice(name.as_bytes());
        }
        Raw::Binary(bytes) => {
            out.push(109);
            out.extend_from_slice(&u32::try_from(bytes.len())?.to_be_bytes());
            out.extend_from_slice(bytes);
        }
        Raw::Integer(number) => integer(number, out)?,
        Raw::BigInteger {
            negative,
            little_endian,
        } => big(negative, little_endian, out)?,
        Raw::Map(rows) => {
            out.push(116);
            out.extend_from_slice(&u32::try_from(rows.len())?.to_be_bytes());
            let mut order = (0..rows.len()).collect::<Vec<_>>();
            if rows.len() > 1 {
                order.sort_by(|&a, &b| key(rows[a].0).cmp(&key(rows[b].0)));
            }
            for row in order {
                term(rows[row].0, out)?;
                term(rows[row].1, out)?;
            }
        }
        Raw::Tuple(items) => {
            if items.len() <= 255 {
                out.extend_from_slice(&[104, u8::try_from(items.len())?]);
            } else {
                out.push(105);
                out.extend_from_slice(&u32::try_from(items.len())?.to_be_bytes());
            }
            for &item in items {
                term(item, out)?;
            }
        }
        Raw::List(items) => list(items, out)?,
        Raw::ImproperList { prefix, tail } => {
            out.push(108);
            out.extend_from_slice(&u32::try_from(prefix.len())?.to_be_bytes());
            for &item in prefix {
                term(item, out)?;
            }
            term(*tail, out)?;
        }
        Raw::Other => return Err("opaque unit has no ETF identity".into()),
    }
    Ok(())
}

fn key(value: Raw<'_>) -> (u8, Vec<u8>) {
    match value {
        Raw::Atom(name) => (0, name.as_bytes().to_vec()),
        Raw::Nil => (0, b"nil".to_vec()),
        Raw::Binary(bytes) => (1, bytes.to_vec()),
        _ => (2, Vec::new()),
    }
}

fn integer(number: i128, out: &mut Vec<u8>) -> TestResult {
    if let Ok(byte) = u8::try_from(number) {
        out.extend_from_slice(&[97, byte]);
    } else if let Ok(word) = i32::try_from(number) {
        out.push(98);
        out.extend_from_slice(&word.to_be_bytes());
    } else {
        let magnitude = number.unsigned_abs().to_le_bytes();
        let length = magnitude.iter().rposition(|&b| b != 0).map_or(0, |n| n + 1);
        big(number < 0, &magnitude[..length], out)?;
    }
    Ok(())
}

fn big(negative: bool, bytes: &[u8], out: &mut Vec<u8>) -> TestResult {
    if bytes.len() < 256 {
        out.extend_from_slice(&[110, u8::try_from(bytes.len())?, u8::from(negative)]);
    } else {
        out.push(111);
        out.extend_from_slice(&u32::try_from(bytes.len())?.to_be_bytes());
        out.push(u8::from(negative));
    }
    out.extend_from_slice(bytes);
    Ok(())
}

fn list(items: &[Raw<'_>], out: &mut Vec<u8>) -> TestResult {
    if items.is_empty() {
        out.push(106);
    } else if items.len() <= 65_535 && items.iter().all(|v| matches!(v, Raw::Integer(0..=255))) {
        out.push(107);
        out.extend_from_slice(&u16::try_from(items.len())?.to_be_bytes());
        for &item in items {
            let Raw::Integer(byte) = item else {
                return Err("byte-list fixture".into());
            };
            out.push(u8::try_from(byte)?);
        }
    } else {
        out.push(108);
        out.extend_from_slice(&u32::try_from(items.len())?.to_be_bytes());
        for &item in items {
            term(item, out)?;
        }
        out.push(106);
    }
    Ok(())
}
