//! Exact decoded key identity, including ignored maps and alternate ETF spellings.

use super::cases::{
    TAG, atom, binary, bits, check, envelope, float, hex, integer, legacy_float, list, map, paired,
    reference, string, tuple,
};
use super::{OriginalEtfSafety, TestResult};

#[test]
fn original_entry_checks_semantic_duplicate_keys_across_etf_encodings() -> TestResult {
    let safety = OriginalEtfSafety::new(&[TAG, "é", "n", "l"], &[], Some(("l", 0)), &[]);
    for (left, right, duplicate) in [
        (integer(1)?, hex("6200000001")?, true),
        (integer(1)?, hex("6e0300010000")?, true),
        (integer(0)?, hex("6e02010000")?, true),
        (integer(-1)?, hex("6f0000000301010000")?, true),
        (integer(1)?, float(1.0_f64.to_bits()), false),
        (float(0), float(1_u64 << 63), false),
        (atom("é")?, hex("640001e9")?, true),
        (atom("é")?, hex("7301e9")?, true),
        (atom("é")?, hex("760002c3a9")?, true),
        (binary(&[0, 255])?, bits(&[0, 255], 8)?, true),
        (bits(&[0b1010_0000], 3)?, bits(&[0b1011_1111], 3)?, true),
        (bits(&[0b1010_0000], 3)?, bits(&[0b1010_0000], 4)?, false),
        (vec![106], string(&[])?, true),
        (vec![106], list(&[], &[106])?, true),
        (integer(1)?, list(&[], &integer(1)?)?, true),
        (
            string(b"ab")?,
            list(&[integer(97)?, integer(98)?], &[106])?,
            true,
        ),
        (
            tuple(&[integer(1)?, binary(b"x")?])?,
            hex("690000000261016d0000000178")?,
            true,
        ),
        (legacy_float(b"1,0")?, float(1.0_f64.to_bits()), true),
        (legacy_float(b"-0.0e-999")?, float(1_u64 << 63), true),
    ] {
        paired(left, right, &safety, duplicate)?;
    }
    assert_list_chunks_and_nested_maps(&safety)?;
    assert_float_grammar(&safety)?;
    assert_reference_identity(&safety)?;
    let long = string(&vec![b'a'; 65_535])?;
    paired(long.clone(), long, &safety, true)?;
    // No business UTF8 rule is imposed on arbitrary binary keys/values.
    paired(binary(&[0, 255])?, binary(&[255, 0])?, &safety, false)?;
    Ok(())
}

fn assert_list_chunks_and_nested_maps(safety: &OriginalEtfSafety<'_>) -> TestResult {
    let flat = list(&[integer(1)?, integer(2)?], &[106])?;
    let chunked = list(&[integer(1)?], &list(&[integer(2)?], &[106])?)?;
    paired(flat, chunked, safety, true)?;
    paired(
        list(&[integer(1)?], &integer(2)?)?,
        list(&[integer(1)?], &integer(3)?)?,
        safety,
        false,
    )?;
    let first = map(&[(integer(1)?, binary(b"a")?), (integer(2)?, binary(b"b")?)])?;
    let second = map(&[(integer(2)?, binary(b"b")?), (integer(1)?, binary(b"a")?)])?;
    paired(first.clone(), second, safety, true)?;
    paired(
        first,
        map(&[(integer(1)?, binary(b"a")?), (integer(2)?, binary(b"c")?)])?,
        safety,
        false,
    )?;
    let ignored_bad = map(&[(integer(1)?, vec![106]), (hex("6200000001")?, vec![106])])?;
    let policy = map(&[(binary(b"ignored")?, tuple(&[ignored_bad])?)])?;
    check(&envelope(&policy, &[97, 0])?, safety, false)?;
    let repeated = map(&[(integer(1)?, vec![106]), (integer(1)?, vec![106])])?;
    check(&envelope(&repeated, &[97, 0])?, safety, false)?;
    let mut padded = vec![110, 255, 0, 1];
    padded.resize(258, 0);
    paired(integer(1)?, padded, safety, true)?;
    Ok(())
}

fn assert_float_grammar(safety: &OriginalEtfSafety<'_>) -> TestResult {
    for text in [
        b"1.0".as_slice(),
        b"+1,0e+0",
        b"-1.5E2",
        b"0.0e-999",
        b"-0,0",
    ] {
        let value = legacy_float(text)?;
        check(
            &envelope(&map(&[(binary(b"x")?, value)])?, &[97, 0])?,
            safety,
            true,
        )?;
    }
    let mut after_nul = legacy_float(b"1.0")?;
    after_nul[5..].fill(255);
    paired(after_nul, float(1.0_f64.to_bits()), safety, true)?;
    for text in [
        b"1".as_slice(),
        b".5",
        b"1.",
        b"1.0e",
        b"1.0e+",
        b"NaN",
        b"1.0e999",
    ] {
        check(
            &envelope(&map(&[(binary(b"x")?, legacy_float(text)?)])?, &[97, 0])?,
            safety,
            false,
        )?;
    }
    let mut unterminated = vec![99];
    unterminated.extend_from_slice(&[b'1'; 31]);
    check(
        &envelope(&map(&[(binary(b"x")?, unterminated)])?, &[97, 0])?,
        safety,
        false,
    )?;
    for bits in [
        f64::INFINITY.to_bits(),
        f64::NEG_INFINITY.to_bits(),
        f64::NAN.to_bits(),
    ] {
        check(
            &envelope(&map(&[(binary(b"x")?, float(bits))])?, &[97, 0])?,
            safety,
            false,
        )?;
    }
    Ok(())
}

fn assert_reference_identity(safety: &OriginalEtfSafety<'_>) -> TestResult {
    paired(
        reference("n", 0, &[1])?,
        reference("n", 0, &[1, 0, 0])?,
        safety,
        true,
    )?;
    paired(
        reference("n", 0, &[1])?,
        reference("n", 0, &[1, 0, 1])?,
        safety,
        false,
    )?;
    paired(
        reference("n", 0, &[1])?,
        reference("n", 1, &[1])?,
        safety,
        false,
    )?;
    let mut old = vec![101];
    old.extend_from_slice(&atom("n")?);
    old.extend_from_slice(&1_u32.to_be_bytes());
    old.push(0);
    paired(old, reference("n", 0, &[1, 0, 0])?, safety, true)?;
    // Source ordinary-local fallback drops extra words, even nonzero ones.
    paired(
        reference("l", 0, &[1])?,
        reference("l", 0, &[1, 0, 0, 99])?,
        safety,
        true,
    )?;
    paired(
        reference("l", 0, &[1, 0, 0])?,
        reference("l", 0, &[1, 0, 1])?,
        safety,
        false,
    )?;
    // Local PID-reference marker is static source normalization, no whitelist.
    let pid_words = [1_u32, 1 << 16, 0, 3, 0];
    let encoded = reference("l", 0, &pid_words)?;
    check(
        &envelope(&map(&[(binary(b"x")?, encoded)])?, &[97, 0])?,
        safety,
        true,
    )?;
    paired(
        reference("l", 0, &pid_words)?,
        reference("l", 0, &[1, 1 << 16, 0, 19, 0])?,
        safety,
        false,
    )?;
    let invalid_pid = reference("l", 0, &[1, 1 << 16, 0, 0, 0])?;
    check(
        &envelope(&map(&[(binary(b"x")?, invalid_pid)])?, &[97, 0])?,
        safety,
        false,
    )?;
    Ok(())
}
