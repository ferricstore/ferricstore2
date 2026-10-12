//! Captured source safety, function-entry normalization and runtime distinctions.

use super::cases::{
    TAG, atom, binary, check, envelope, export, fun, hex, integer, map, paired, pid, reference,
};
use super::{
    Error, OriginalEtfLocalFun, OriginalEtfSafety, PolicySchemaCharge, TestResult, decode, limits,
};

#[test]
fn original_entry_applies_branch_specific_safety_and_runtime_normalization() -> TestResult {
    let atoms = [TAG, "m", "f", "n", "l", "é"];
    let active = [("m", "f", 1)];
    let safety = OriginalEtfSafety::new(&atoms, &active, Some(("l", 0)), &[]);
    let exported = export(&integer(1)?)?;
    let bytes = envelope(&map(&[(binary(b"ignored")?, exported.clone())])?, &[97, 0])?;
    check(&bytes, &safety, true)?;
    let unavailable = OriginalEtfSafety::new(&atoms, &[], Some(("l", 0)), &[]);
    check(&bytes, &unavailable, false)?;
    // Safe exports normalize source-small arity to unsigned32 before membership.
    paired(
        export(&integer(1)?)?,
        export(&integer(4_294_967_297)?)?,
        &safety,
        true,
    )?;
    for arity in [integer(-1)?, hex("6e08000000000000000010")?] {
        check(
            &envelope(&map(&[(binary(b"x")?, export(&arity)?)])?, &[97, 0])?,
            &safety,
            false,
        )?;
    }
    let hidden = map(&[(binary(b"ignored")?, atom("unknown_atom")?)])?;
    check(&envelope(&hidden, &[97, 0])?, &safety, false)?;
    let bad_utf8_atom = hex("7701ff")?;
    check(
        &envelope(&map(&[(binary(b"x")?, bad_utf8_atom)])?, &[97, 0])?,
        &safety,
        false,
    )?;
    assert_function_entry_identity(&safety)?;
    assert_source_small_boundaries(&safety)?;
    assert_source_environment_bound(&safety)?;
    assert_captured_function_precedence(&atoms)?;
    assert_insert_before_environment(&safety)?;
    assert_runtime_syntax_and_unavailable_context(&safety)?;
    assert_atom_index_runtime_exclusions(&safety)?;
    assert_zero_reference_runtime_exclusions(&safety)?;
    assert_pid_and_port_identity(&safety)?;
    assert_source_pid_port_bounds(&safety)?;
    assert_source_reference_bounds(&safety)?;
    Ok(())
}

fn assert_function_entry_identity(safety: &OriginalEtfSafety<'_>) -> TestResult {
    let creator = pid("n", 0, 0, 0)?;
    let changed_creator = pid("n", 1, 0, 0)?;
    let first = fun((0, 0, 0, 7), [0; 16], &creator, &[], 0)?;
    let alternate = fun((0, 0, 99, 9), [0; 16], &changed_creator, &[], u32::MAX)?;
    // Same lookup reuses the first canonical entry; creator/Size/old-index are not identity.
    paired(first.clone(), alternate, safety, true)?;
    paired(
        first.clone(),
        fun((1, 0, 0, 7), [1; 16], &creator, &[], 1)?,
        safety,
        true,
    )?;
    paired(
        first,
        fun((1, 0, 0, 9), [1; 16], &creator, &[], 1)?,
        safety,
        false,
    )?;
    paired(
        fun((0, -1, -1, -1), [0; 16], &creator, &[], 0)?,
        fun(
            (1, -1, 4_294_967_295, 4_294_967_295),
            [1; 16],
            &creator,
            &[],
            0,
        )?,
        safety,
        true,
    )?;
    paired(
        fun((0, 0, 0, 7), [0; 16], &creator, &[integer(1)?], 0)?,
        fun((1, 0, 0, 7), [1; 16], &creator, &[integer(2)?], 0)?,
        safety,
        false,
    )?;
    let ignored_environment = fun(
        (0, 0, 0, 7),
        [0; 16],
        &creator,
        &[atom("not_in_capture")?],
        0,
    )?;
    check(
        &envelope(&map(&[(binary(b"x")?, ignored_environment)])?, &[97, 0])?,
        safety,
        false,
    )?;
    let bad_creator = fun((0, 0, 0, 7), [0; 16], &[106], &[], 0)?;
    check(
        &envelope(&map(&[(binary(b"x")?, bad_creator)])?, &[97, 0])?,
        safety,
        false,
    )?;
    let outside_small = hex("6e08000000000000000010")?;
    let mut bad_old = fun((0, 0, 0, 7), [0; 16], &creator, &[], 0)?;
    // Fixed NEW_FUN header30 + module3, then the source-small OLD index term.
    bad_old.splice(33..35, outside_small);
    check(
        &envelope(&map(&[(binary(b"x")?, bad_old)])?, &[97, 0])?,
        safety,
        false,
    )?;
    Ok(())
}

fn assert_captured_function_precedence(atoms: &[&str]) -> TestResult {
    let captured = [OriginalEtfLocalFun::new("m", 0, 0, [0; 16], 17)];
    let safety = OriginalEtfSafety::new(atoms, &[], Some(("l", 0)), &captured);
    let absent = OriginalEtfSafety::new(atoms, &[], Some(("l", 0)), &[]);
    let creator = pid("n", 0, 0, 0)?;
    let incoming = fun((0, 0, 0, 7), [0; 16], &creator, &[], 0)?;
    let independent = fun((1, 0, 0, 17), [1; 16], &creator, &[], 0)?;
    paired(incoming.clone(), independent.clone(), &safety, true)?;
    paired(incoming, independent, &absent, false)?;
    // Capture lookup includes NEW arity/uniq; equality uses its canonical signed fields.
    let negative = [OriginalEtfLocalFun::new("m", -1, 0, [0; 16], -1)];
    let safety = OriginalEtfSafety::new(atoms, &[], Some(("l", 0)), &negative);
    paired(
        fun((0, -1, 0, 123), [0; 16], &creator, &[], 0)?,
        fun((1, -1, 0, -1), [1; 16], &creator, &[], 0)?,
        &safety,
        true,
    )?;
    Ok(())
}

fn assert_source_small_boundaries(safety: &OriginalEtfSafety<'_>) -> TestResult {
    paired(
        export(&integer(1)?)?,
        export(&integer(576_460_748_008_456_193)?)?,
        safety,
        true,
    )?;
    let too_large_export = export(&integer(576_460_752_303_423_489)?)?;
    check(
        &envelope(&map(&[(binary(b"x")?, too_large_export)])?, &[97, 0])?,
        safety,
        false,
    )?;
    let creator = pid("n", 0, 0, 0)?;
    for (value, accepted) in [
        (-576_460_752_303_423_488, true),
        (576_460_752_303_423_487, true),
        (-576_460_752_303_423_489, false),
        (576_460_752_303_423_488, false),
    ] {
        for fields in [(0, 0, value, 7), (0, 0, 0, value)] {
            let original = fun(fields, [0; 16], &creator, &[], 0)?;
            let raw = envelope(&map(&[(binary(b"x")?, original)])?, &[97, 0])?;
            check(&raw, safety, accepted)?;
        }
    }
    Ok(())
}

fn assert_source_environment_bound(safety: &OriginalEtfSafety<'_>) -> TestResult {
    let creator = pid("n", 0, 0, 0)?;
    for count in [255, 256] {
        let environment = vec![vec![106]; count];
        let original = fun((0, 0, 0, 7), [0; 16], &creator, &environment, 0)?;
        let raw = envelope(&map(&[(binary(b"x")?, original)])?, &[97, 0])?;
        check(&raw, safety, count == 255)?;
        assert_eq!(environment.len(), count);
        assert!(environment.iter().all(|term| term == &[106]));
    }
    Ok(())
}

fn assert_insert_before_environment(safety: &OriginalEtfSafety<'_>) -> TestResult {
    let creator = pid("n", 0, 0, 0)?;
    let nested = fun((0, 0, 0, 9), [0; 16], &creator, &[], 0)?;
    let outer = fun((0, 0, 0, 7), [0; 16], &creator, &[nested], 0)?;
    for (anchor_old_uniq, accepted) in [(7, false), (9, true)] {
        let direct = fun((0, 0, 0, 0), [0; 16], &creator, &[], 0)?;
        let anchor = fun((1, 0, 0, anchor_old_uniq), [1; 16], &creator, &[], 0)?;
        let policy = map(&[
            (binary(b"first")?, outer.clone()),
            (direct, vec![106]),
            (anchor, integer(0)?),
        ])?;
        // First outer stub must exist before its same-lookup nested child is visited.
        check(&envelope(&policy, &[97, 0])?, safety, accepted)?;
    }
    Ok(())
}

fn assert_runtime_syntax_and_unavailable_context(safety: &OriginalEtfSafety<'_>) -> TestResult {
    let profile = limits(1_000_000)?;
    for runtime in [vec![121, 0, 0, 0, 0, 106], reference("n", 0, &[1])?] {
        let raw = envelope(&map(&[(binary(b"x")?, runtime.clone())])?, &[97, 0])?;
        let mut charge = PolicySchemaCharge::new(profile);
        let context = if runtime[0] == 121 {
            OriginalEtfSafety::new(&[TAG, "n"], &[], Some(("l", 0)), &[])
        } else {
            OriginalEtfSafety::new(&[TAG, "n"], &[], None, &[])
        };
        assert_eq!(
            decode(&raw, &context, profile, &mut charge).err(),
            Some(Error::UnsupportedTag)
        );
    }
    let mut complete_old_fun = vec![117];
    complete_old_fun.extend_from_slice(&0_u32.to_be_bytes());
    complete_old_fun.extend_from_slice(&pid("n", 0, 0, 0)?);
    complete_old_fun.extend_from_slice(&atom("m")?);
    complete_old_fun.extend_from_slice(&integer(0)?);
    complete_old_fun.extend_from_slice(&integer(7)?);
    let old_fun = envelope(&map(&[(binary(b"x")?, complete_old_fun)])?, &[97, 0])?;
    check(&old_fun, safety, false)?;
    for runtime in [
        pid("n", 0, 0, 0)?,
        reference("n", 0, &[1])?,
        fun((0, 0, 0, 7), [0; 16], &pid("n", 0, 0, 0)?, &[], 0)?,
    ] {
        let raw = envelope(&map(&[(binary(b"x")?, runtime)])?, &[97, 0])?;
        for end in 21..raw.len() {
            check(&raw[..end], safety, false)?;
        }
        check(&raw, safety, true)?;
    }
    Ok(())
}

fn assert_pid_and_port_identity(safety: &OriginalEtfSafety<'_>) -> TestResult {
    let mut old_pid = vec![103];
    old_pid.extend_from_slice(&atom("n")?);
    old_pid.extend_from_slice(&0_u32.to_be_bytes());
    old_pid.extend_from_slice(&0_u32.to_be_bytes());
    old_pid.push(0);
    paired(old_pid, pid("n", 0, 0, 0)?, safety, true)?;
    paired(pid("n", 0, 0, 0)?, pid("n", 1, 0, 0)?, safety, false)?;
    let mut old_port = vec![102];
    old_port.extend_from_slice(&atom("n")?);
    old_port.extend_from_slice(&1_u32.to_be_bytes());
    old_port.push(0);
    let mut v4_port = vec![120];
    v4_port.extend_from_slice(&atom("n")?);
    v4_port.extend_from_slice(&1_u64.to_be_bytes());
    v4_port.extend_from_slice(&0_u32.to_be_bytes());
    paired(old_port, v4_port, safety, true)?;
    Ok(())
}

fn assert_source_pid_port_bounds(safety: &OriginalEtfSafety<'_>) -> TestResult {
    for node in ["n", "l"] {
        for id in [0x0fff_ffff_u32, 0x1000_0000, u32::MAX] {
            let mut old = vec![103];
            old.extend_from_slice(&atom(node)?);
            old.extend_from_slice(&id.to_be_bytes());
            old.extend_from_slice(&u32::MAX.to_be_bytes());
            old.push(0);
            let new = pid(node, id, u32::MAX, 0)?;
            let accepted = node == "n" || id <= 0x0fff_ffff;
            for encoded in [&old, &new] {
                check(
                    &envelope(&map(&[(binary(b"x")?, encoded.clone())])?, &[97, 0])?,
                    safety,
                    accepted,
                )?;
            }
            if accepted {
                paired(old, new, safety, true)?;
            }
        }
        for id in [(1_u64 << 60) - 1, 1 << 60, u64::MAX] {
            let mut port = vec![120];
            port.extend_from_slice(&atom(node)?);
            port.extend_from_slice(&id.to_be_bytes());
            port.extend_from_slice(&0_u32.to_be_bytes());
            check(
                &envelope(&map(&[(binary(b"x")?, port)])?, &[97, 0])?,
                safety,
                node == "n" || id < (1 << 60),
            )?;
        }
    }
    // Same sysname with a different creation is a remote container, not local.
    let stale_node = pid("l", u32::MAX, u32::MAX, 1)?;
    check(
        &envelope(&map(&[(binary(b"x")?, stale_node)])?, &[97, 0])?,
        safety,
        true,
    )?;
    Ok(())
}

fn assert_source_reference_bounds(safety: &OriginalEtfSafety<'_>) -> TestResult {
    for node in ["n", "l"] {
        for first in [(1_u32 << 18) - 1, 1 << 18] {
            let mut old = vec![101];
            old.extend_from_slice(&atom(node)?);
            old.extend_from_slice(&first.to_be_bytes());
            old.push(0);
            let mut legacy_new = vec![114, 0, 3];
            legacy_new.extend_from_slice(&atom(node)?);
            legacy_new.push(0);
            for word in [first, 0, 0] {
                legacy_new.extend_from_slice(&word.to_be_bytes());
            }
            for encoded in [old, legacy_new] {
                check(
                    &envelope(&map(&[(binary(b"x")?, encoded)])?, &[97, 0])?,
                    safety,
                    first < (1 << 18),
                )?;
            }
            let newer = reference(node, 0, &[first, 0, 0])?;
            check(
                &envelope(&map(&[(binary(b"x")?, newer)])?, &[97, 0])?,
                safety,
                node == "n" || first < (1 << 18),
            )?;
        }
    }
    let remote_full_word = reference("n", 0, &[u32::MAX, 0, 0])?;
    check(
        &envelope(&map(&[(binary(b"x")?, remote_full_word)])?, &[97, 0])?,
        safety,
        true,
    )?;
    Ok(())
}

fn assert_atom_index_runtime_exclusions(safety: &OriginalEtfSafety<'_>) -> TestResult {
    let profile = limits(1_000_000)?;
    // Existing VM atom indexes cannot be proved by the captured text catalog.
    for term in [vec![73, 0, 1], vec![75, 0, 0, 1]] {
        let mut direct = vec![131];
        direct.extend_from_slice(&term);
        let nested = envelope(&map(&[(binary(b"ignored")?, term)])?, &[97, 0])?;
        for raw in [direct, nested] {
            let mut charge = PolicySchemaCharge::new(profile);
            assert_eq!(
                decode(&raw, safety, profile, &mut charge).err(),
                Some(Error::UnsupportedTag)
            );
            assert_eq!(
                super::read(
                    super::ReadValue::Binary(&raw),
                    b"t",
                    safety,
                    profile,
                    &mut charge
                )
                .err(),
                Some(super::ReadError::Encoding(Error::UnsupportedTag))
            );
        }
    }
    // Distribution-cache and internal heap tags require source decoder context.
    for term in [vec![82, 0], vec![74], vec![76], vec![78]] {
        let mut direct = vec![131];
        direct.extend_from_slice(&term);
        let nested = envelope(&map(&[(binary(b"ignored")?, term)])?, &[97, 0])?;
        for raw in [direct, nested] {
            check(&raw, safety, false)?;
        }
    }
    Ok(())
}

fn assert_zero_reference_runtime_exclusions(safety: &OriginalEtfSafety<'_>) -> TestResult {
    let profile = limits(1_000_000)?;
    for tag in [90, 114] {
        let mut term = vec![tag, 0, 0];
        term.extend_from_slice(&atom("n")?);
        term.extend_from_slice(if tag == 90 { &[0; 4] } else { &[0] });
        let mut direct = vec![131];
        direct.extend_from_slice(&term);
        let nested = envelope(&map(&[(binary(b"ignored")?, term)])?, &[97, 0])?;
        // Source reads an undeclared first word; no native outcome is assumed.
        for raw in [direct, nested] {
            let mut charge = PolicySchemaCharge::new(profile);
            assert_eq!(
                decode(&raw, safety, profile, &mut charge).err(),
                Some(Error::UnsupportedTag)
            );
            assert_eq!(
                super::read(
                    super::ReadValue::Binary(&raw),
                    b"t",
                    safety,
                    profile,
                    &mut charge
                )
                .err(),
                Some(super::ReadError::Encoding(Error::UnsupportedTag))
            );
        }
    }
    Ok(())
}
