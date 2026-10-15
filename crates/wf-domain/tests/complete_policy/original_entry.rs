//! Original stored-entry clauses; raw ETF is distinct from normalized V2 content.

use wf_domain::policy::complete::{
    CompletePolicyEncodingError as Error, OriginalEtfLocalFun, OriginalEtfSafety,
    OriginalPolicyReadError as ReadError, OriginalPolicyReadValue as ReadValue, PolicySchemaCharge,
    PolicySchemaError, decode_original_complete_policy_entry as decode,
    read_original_complete_policy_entry as read,
};

use super::{TestResult, limits};
use cases::{
    COMPRESSED, MIN, SOURCE_603, TAG, TYPE_T, atom, binary, check, envelope, hex, integer, list,
    map, tuple,
};

mod cases;
mod identity;
mod refusals;
mod runtime;

#[test]
fn original_entry_accepts_generation_zero_and_arbitrary_maps_without_admission() -> TestResult {
    let minimal_safety = OriginalEtfSafety::new(&[TAG], &[], None, &[]);
    let profile = limits(100_000_000)?;
    let mut charge = PolicySchemaCharge::new(profile);
    let empty = hex(MIN)?;
    let entry = decode(&empty, &minimal_safety, profile, &mut charge)?.ok_or("missing gen0 map")?;
    assert_eq!(entry.generation(), 0);
    assert_eq!(entry.encoded(), empty);
    assert_eq!(entry.policy_bytes(), &[116, 0, 0, 0, 0]);
    assert_eq!(entry.workflow_type(), None);
    assert_eq!(
        (charge.g(), charge.p(), charge.total()),
        (3_736, 256, 3_992)
    );
    assert!(std::ptr::eq(entry.encoded().as_ptr(), empty.as_ptr()));
    assert!(std::ptr::eq(
        entry.policy_bytes().as_ptr(),
        empty[21..].as_ptr()
    ));
    let safety = OriginalEtfSafety::new(&[TAG, "type", "version", "max_active_ms"], &[], None, &[]);
    let policy = map(&[
        (atom("type")?, binary(b"foreign-policy-type")?),
        (atom("max_active_ms")?, integer(9_000)?),
        (atom("version")?, list(&[integer(-1)?], &[97, 3])?),
    ])?;
    for generation in [0, 1] {
        let bytes = envelope(&policy, &integer(generation)?)?;
        let entry =
            decode(&bytes, &safety, profile, &mut charge)?.ok_or("arbitrary map rejected")?;
        assert_eq!(entry.generation(), u64::try_from(generation)?);
        assert_eq!(entry.policy_bytes(), policy);
        assert_eq!(entry.workflow_type(), Some(&b"foreign-policy-type"[..]));
    }
    assert_encoded_limit_and_snapshot_distinction()?;
    assert_transparent_wrappers()?;
    Ok(())
}

fn assert_encoded_limit_and_snapshot_distinction() -> TestResult {
    let safety = OriginalEtfSafety::new(&[TAG, "version", "type"], &[], None, &[]);
    for length in [263_128, 263_129] {
        let payload = vec![b'x'; length];
        let policy = map(&[(atom("version")?, binary(&payload)?)])?;
        let bytes = envelope(&policy, &[97, 0])?;
        let profile = limits(100_000_000)?;
        let mut charge = PolicySchemaCharge::new(profile);
        let entry = decode(&bytes, &safety, profile, &mut charge)?;
        assert_eq!(bytes.len(), 40 + length);
        if length == 263_128 {
            let entry = entry.ok_or("encoded-only boundary rejected")?;
            assert_eq!(entry.generation(), 0);
            assert_eq!(entry.policy_bytes(), policy);
            assert_eq!(entry.workflow_type(), None);
            assert_eq!(policy.len() + 1, 263_148);
        } else {
            assert!(entry.is_none());
            assert_eq!((charge.g(), charge.p()), (128, 0));
        }
        assert!(payload.iter().all(|&byte| byte == b'x'));
    }
    let payload = vec![b'x'; 262_096];
    let policy = map(&[
        (atom("type")?, binary(b"snapshot-boundary")?),
        (atom("version")?, binary(&payload)?),
    ])?;
    let bytes = envelope(&policy, &[97, 1])?;
    assert_eq!((policy.len() + 1, bytes.len()), (262_144, 262_164));
    check(&bytes, &safety, true)?;
    Ok(())
}

fn assert_transparent_wrappers() -> TestResult {
    let safety = OriginalEtfSafety::new(&[TAG, "type"], &[], None, &[]);
    let wrapped_policy = list(&[], &map(&[(atom("type")?, list(&[], &binary(b"t")?)?)])?)?;
    let wrapped_tag = list(&[], &atom(TAG)?)?;
    let wrapped_generation = list(&[], &integer(0)?)?;
    let mut bytes = vec![131];
    bytes.extend_from_slice(&list(
        &[],
        &tuple(&[wrapped_tag, wrapped_generation, wrapped_policy.clone()])?,
    )?);
    let profile = limits(1_000_000)?;
    let mut charge = PolicySchemaCharge::new(profile);
    let entry =
        decode(&bytes, &safety, profile, &mut charge)?.ok_or("transparent aliases rejected")?;
    assert_eq!(entry.generation(), 0);
    assert_eq!(entry.workflow_type(), Some(&b"t"[..]));
    assert_eq!(entry.policy_bytes(), wrapped_policy);
    assert_eq!(entry.encoded(), bytes);
    Ok(())
}

#[test]
fn original_entry_preserves_original_shape_generation_and_consumption_errors() -> TestResult {
    let safety = OriginalEtfSafety::new(
        &[
            TAG,
            "type",
            "version",
            "retry",
            "states",
            "max_attempts",
            "wrong",
        ],
        &[],
        None,
        &[],
    );
    let valid = hex(TYPE_T)?;
    for end in 0..valid.len() {
        check(&valid[..end], &safety, false)?;
    }
    for suffix in [vec![0], vec![131], vec![106], vec![0, 0, 0]] {
        let mut trailing = valid.clone();
        trailing.extend_from_slice(&suffix);
        check(&trailing, &safety, false)?;
    }
    check(&hex(COMPRESSED)?, &safety, false)?;
    check(
        &hex("836802770e666c6f775f706f6c6963795f76317400000000")?,
        &safety,
        false,
    )?;
    let source = hex(SOURCE_603)?;
    assert_eq!(source.len(), 133);
    assert_eq!(&source[1..3], &[104, 2]);
    // Frozen a16aec6cceb6253a:603 is generationless, not a deprecated-key result.
    check(&source, &safety, false)?;
    let source_limits = limits(1_000_000)?;
    let mut source_charge = PolicySchemaCharge::new(source_limits);
    assert!(decode(&source, &safety, source_limits, &mut source_charge)?.is_none());
    for generation in [
        integer(-1)?,
        integer(9_007_199_254_740_992)?,
        cases::float(0),
    ] {
        check(&envelope(&map(&[])?, &generation)?, &safety, false)?;
    }
    let maximum = hex(concat!(
        "836803770e666c6f775f706f6c6963795f76316e0700ffffffffffff1f7400000002",
        "7704747970656d0000001367656e65726174696f6e2d626f756e646172797707",
        "76657273696f6e6d000000097075626c69632d7631"
    ))?;
    let profile = limits(1_000_000)?;
    let mut charge = PolicySchemaCharge::new(profile);
    let entry = decode(&maximum, &safety, profile, &mut charge)?.ok_or("MAX rejected")?;
    assert_eq!(entry.generation(), 9_007_199_254_740_991);
    assert_eq!(entry.workflow_type(), Some(&b"generation-boundary"[..]));
    assert_eq!(entry.encoded(), maximum);
    assert_eq!(entry.policy_bytes(), &maximum[29..]);
    let mut excessive = maximum.clone();
    excessive[22..29].copy_from_slice(&[0, 0, 0, 0, 0, 0, 32]);
    check(&excessive, &safety, false)?;
    let mut excessive_charge = PolicySchemaCharge::new(profile);
    assert!(decode(&excessive, &safety, profile, &mut excessive_charge)?.is_none());
    let mut wrong = vec![131];
    wrong.extend_from_slice(&tuple(&[atom("wrong")?, integer(0)?, map(&[])?])?);
    check(&wrong, &safety, false)?;
    check(&envelope(&[106], &[97, 0])?, &safety, false)?;
    Ok(())
}

#[test]
fn original_entry_integrates_old_key_short_circuit_and_atom_map_preference() -> TestResult {
    let safety = OriginalEtfSafety::new(
        &[TAG, "retry", "states", "max_attempts", "max_retries"],
        &[],
        None,
        &[],
    );
    let old = map(&[(atom("max_attempts")?, vec![106])])?;
    let clean = map(&[(atom("max_retries")?, integer(0)?)])?;
    for reversed in [false, true] {
        for name in ["retry", "states"] {
            let bad = if name == "retry" {
                old.clone()
            } else {
                map(&[(binary(b"queued")?, map(&[(atom("retry")?, old.clone())])?)])?
            };
            let mut rows = [(binary(name.as_bytes())?, bad), (atom(name)?, vec![106])];
            if reversed {
                rows.reverse();
            }
            check(&envelope(&map(&rows)?, &[97, 0])?, &safety, true)?;
        }
        for (atom_retry, binary_retry, accepted) in [
            (vec![106], old.clone(), false),
            (clean.clone(), old.clone(), true),
            (old.clone(), clean.clone(), false),
        ] {
            let mut retry = [
                (atom("retry")?, atom_retry),
                (binary(b"retry")?, binary_retry),
            ];
            if reversed {
                retry.reverse();
            }
            let states = map(&[(binary(b"queued")?, map(&retry)?)])?;
            let policy = map(&[(atom("states")?, states)])?;
            check(&envelope(&policy, &[97, 0])?, &safety, accepted)?;
        }
    }
    for key in [
        atom("max_attempts")?,
        binary(b"max_attempts")?,
        cases::bits(b"max_attempts", 8)?,
    ] {
        for value in [vec![106], map(&[])?, integer(2)?] {
            let policy = map(&[(atom("retry")?, map(&[(key.clone(), value)])?)])?;
            check(&envelope(&policy, &[97, 0])?, &safety, false)?;
        }
    }
    // Full raw validation must inspect ignored fields before semantic short circuit.
    let unsafe_states = map(&[
        (atom("states")?, atom("not_captured")?),
        (atom("retry")?, old),
    ])?;
    check(&envelope(&unsafe_states, &[97, 0])?, &safety, false)?;
    let unsafe_only = map(&[
        (binary(b"ignored")?, atom("not_captured")?),
        (atom("retry")?, clean.clone()),
    ])?;
    check(&envelope(&unsafe_only, &[97, 0])?, &safety, false)?;
    let raw = envelope(&map(&[(atom("retry")?, clean)])?, &[97, 0])?;
    check(&raw, &safety, true)?;
    Ok(())
}

#[test]
fn original_entry_binds_type_and_preserves_read_result_origin() -> TestResult {
    let safety = OriginalEtfSafety::new(&[TAG, "type", "max_active_ms"], &[], None, &[]);
    let profile = limits(1_000_000)?;
    let mut charge = PolicySchemaCharge::new(profile);
    let foreign = envelope(
        &map(&[
            (atom("type")?, binary(b"foreign-policy-type")?),
            (atom("max_active_ms")?, integer(9_000)?),
        ])?,
        &[97, 0],
    )?;
    assert!(decode(&foreign, &safety, profile, &mut charge)?.is_some());
    assert_eq!(
        read(
            ReadValue::Binary(&foreign),
            b"checkout",
            &safety,
            profile,
            &mut charge
        )
        .err(),
        Some(ReadError::Corrupt)
    );
    assert_eq!(ReadError::Corrupt.to_string(), "ERR flow policy is corrupt");
    for (key, value, requested, accepted) in [
        (atom("type")?, binary(b"t")?, &b"t"[..], true),
        (binary(b"type")?, binary(b"t")?, &b"t"[..], false),
        (atom("type")?, atom("type")?, &b"t"[..], false),
        (atom("type")?, binary(b"")?, &b""[..], true),
        (atom("type")?, binary(&[0, 255])?, &[0, 255][..], true),
        (atom("type")?, cases::bits(b"t", 8)?, &b"t"[..], true),
        (atom("type")?, cases::bits(&[], 8)?, &b""[..], false),
        (atom("type")?, cases::bits(b"t", 7)?, &b"t"[..], false),
    ] {
        let raw = envelope(&map(&[(key, value)])?, &[97, 0])?;
        let result = read(
            ReadValue::Binary(&raw),
            requested,
            &safety,
            profile,
            &mut charge,
        );
        if accepted {
            let entry = result?.ok_or("present policy became absence")?;
            assert_eq!(entry.generation(), 0);
            assert_eq!(entry.workflow_type(), Some(requested));
            assert_eq!(entry.encoded(), raw);
        } else {
            assert_eq!(result.err(), Some(ReadError::Corrupt));
        }
    }
    assert_empty_bit_runtime_exclusion(&safety)?;
    let poisoned = OriginalEtfSafety::new(&["duplicate", "duplicate"], &[], None, &[]);
    for (value, error, message) in [
        (
            ReadValue::Other,
            ReadError::Corrupt,
            "ERR flow policy is corrupt",
        ),
        (
            ReadValue::Unavailable,
            ReadError::ShardUnavailable,
            "ERR flow policy shard not available",
        ),
        (
            ReadValue::ReadFailed,
            ReadError::ReadFailed,
            "ERR flow policy read failed",
        ),
    ] {
        let mut account = PolicySchemaCharge::new(profile);
        assert_eq!(
            read(value, b"", &poisoned, profile, &mut account).err(),
            Some(error)
        );
        assert_eq!(error.to_string(), message);
        assert_eq!((account.g(), account.p()), (128, 0));
    }
    let mut account = PolicySchemaCharge::new(profile);
    assert!(read(ReadValue::Absent, b"", &poisoned, profile, &mut account)?.is_none());
    assert_eq!((account.g(), account.p()), (128, 0));
    assert_eq!(
        read(ReadValue::Binary(&[]), b"t", &safety, profile, &mut charge).err(),
        Some(ReadError::Corrupt)
    );
    Ok(())
}

#[test]
fn original_entry_keeps_original_bytes_after_index_cleanup() -> TestResult {
    let bytes = hex(TYPE_T)?;
    let profile = limits(100_000)?;
    let mut charge = PolicySchemaCharge::new(profile);
    let entry = {
        let owned_catalog = [TAG.to_owned(), "type".to_owned()];
        let atom_names = owned_catalog.iter().map(String::as_str).collect::<Vec<_>>();
        let safety = OriginalEtfSafety::new(&atom_names, &[], None, &[]);
        decode(&bytes, &safety, profile, &mut charge)?.ok_or("entry missing")?
    };
    assert_eq!(entry.encoded(), bytes);
    assert_eq!(entry.generation(), 0);
    assert_eq!(entry.policy_bytes(), &bytes[21..]);
    assert_eq!(entry.workflow_type(), Some(&b"t"[..]));
    assert_eq!(
        (charge.g(), charge.p(), charge.total()),
        (6_778, 448, 7_226)
    );
    let another = hex(MIN)?;
    let safety = OriginalEtfSafety::new(&[TAG], &[], None, &[]);
    let second = decode(&another, &safety, profile, &mut charge)?.ok_or("second missing")?;
    assert_eq!(second.generation(), 0);
    assert_eq!(second.workflow_type(), None);
    assert_eq!((charge.g(), charge.p()), (10_514, 448));
    assert_eq!(entry.encoded(), bytes);
    assert_eq!(entry.policy_bytes(), &bytes[21..]);
    assert!(std::ptr::eq(entry.encoded().as_ptr(), bytes.as_ptr()));
    assert!(std::ptr::eq(
        entry.policy_bytes().as_ptr(),
        bytes[21..].as_ptr()
    ));
    assert!(std::ptr::eq(
        entry.workflow_type().ok_or("type missing")?.as_ptr(),
        bytes[37..].as_ptr()
    ));
    Ok(())
}

fn assert_empty_bit_runtime_exclusion(safety: &OriginalEtfSafety<'_>) -> TestResult {
    let raw = envelope(&map(&[(atom("type")?, cases::bits(&[], 0)?)])?, &[97, 0])?;
    let profile = limits(100_000)?;
    let mut charge = PolicySchemaCharge::new(profile);
    // Source OTP28.3.1 underflows its allocation size for this exact header.
    // The portable Rust profile excludes it operationally; no unsafe VM replay.
    assert_eq!(
        decode(&raw, safety, profile, &mut charge).err(),
        Some(Error::UnsupportedTag)
    );
    assert_eq!(
        read(ReadValue::Binary(&raw), b"", safety, profile, &mut charge).err(),
        Some(ReadError::Encoding(Error::UnsupportedTag))
    );
    Ok(())
}
