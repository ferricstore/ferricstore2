//! Original `PolicyCommand` byte references, separate from public Policy absence.

use std::collections::BTreeMap;

use sha2::{Digest, Sha256};
use wf_domain::policy::complete::{
    CompletePolicyEncodingError as Error, OriginalEtfSafety, OriginalPolicyReadError as ReadError,
    OriginalPolicyReadValue as ReadValue, PolicySchemaCharge, PolicySchemaError,
    read_original_complete_policy_entry as read, restore_complete_policy,
};
use wf_domain::workflow::control::PreparedDigest;
use wf_ports::log::{
    OriginalPolicyReference, prepare_original_complete_policy_reference as prepare,
};

use super::{TestResult, minimal_data, profile};

const TAG: &str = "flow_policy_v1";
const TYPE_T: &str = "836803770e666c6f775f706f6c6963795f7631610074000000017704747970656d0000000174";
const TYPE_T_SHA: &str = "0509159fb87c1a2ae9f989ad1e2051fccb21d37434af9ecfe721d088f78afb5c";

#[test]
fn original_reference_distinguishes_public_absence_from_stamping_fallback() -> TestResult {
    let limits = profile(100_000)?;
    let poisoned = OriginalEtfSafety::new(&["duplicate", "duplicate"], &[], None, &[]);
    let mut charge = PolicySchemaCharge::new(limits);
    assert!(
        read(
            ReadValue::Absent,
            b"checkout",
            &poisoned,
            limits,
            &mut charge
        )?
        .is_none()
    );
    assert_eq!((charge.g(), charge.p()), (128, 0));
    let expected = hex(concat!(
        "836803770e666c6f775f706f6c6963795f7631610074000000017704747970656d",
        "00000008636865636b6f7574"
    ))?;
    let expected_hash = "4b2a4354b30b9b8ef3b7aa0c35f074bf353113b4a492173067e2528b8f2431e4";
    assert_eq!(expected.len(), 45);
    assert_eq!(Sha256::digest(&expected).as_slice(), hex(expected_hash)?);
    let query = b"checkout".to_vec();
    let value = prepare(ReadValue::Absent, &query, &poisoned, limits, &mut charge)?;
    observe(&value, &expected, expected_hash, 0, &query)?;
    assert_eq!((charge.g(), charge.p()), (2_356, 0));
    let (workflow_type, generation, prepared) = value.into_parts();
    assert_eq!(workflow_type, b"checkout");
    assert_eq!(generation, 0);
    assert_eq!(prepared.as_bytes(), expected);
    assert_eq!(prepared.digest().as_bytes().as_slice(), hex(expected_hash)?);
    assert!(std::ptr::eq(workflow_type.as_ptr(), query.as_ptr()));
    for requested in [b"".as_slice(), &[0, 255][..]] {
        let expected = fallback(requested)?;
        let digest: [u8; 32] = Sha256::digest(&expected).into();
        let mut account = PolicySchemaCharge::new(limits);
        let value = prepare(
            ReadValue::Absent,
            requested,
            &poisoned,
            limits,
            &mut account,
        )?;
        assert_eq!(value.workflow_type(), requested);
        assert_eq!(value.generation(), 0);
        assert_eq!(value.encoded(), expected);
        assert_eq!(value.digest(), PreparedDigest::new(digest));
        assert_eq!(
            (account.g(), account.p()),
            (2_048 + 4 * u64::try_from(expected.len())?, 0)
        );
    }
    Ok(())
}

#[test]
fn original_reference_hashes_exact_present_encoding_without_default_expansion() -> TestResult {
    let expected = hex(TYPE_T)?;
    assert_eq!(expected.len(), 38);
    assert_eq!(Sha256::digest(&expected).as_slice(), hex(TYPE_T_SHA)?);
    let query = b"t".to_vec();
    let limits = profile(100_000)?;
    let mut charge = PolicySchemaCharge::new(limits);
    let value = {
        let owned_atoms = [TAG.to_owned(), "type".to_owned()];
        let atoms = owned_atoms.iter().map(String::as_str).collect::<Vec<_>>();
        let safety = OriginalEtfSafety::new(&atoms, &[], None, &[]);
        let source_owner = expected.clone();
        prepare(
            ReadValue::Binary(&source_owner),
            &query,
            &safety,
            limits,
            &mut charge,
        )?
    };
    // Source and safety owners are dropped; the only borrowed output is the query.
    observe(&value, &expected, TYPE_T_SHA, 0, &query)?;
    assert_eq!(
        (charge.g(), charge.p(), charge.total()),
        (7_496, 448, 7_944)
    );
    let (workflow_type, generation, prepared) = value.into_parts();
    assert_eq!(workflow_type, query);
    assert_eq!(generation, 0);
    assert_eq!(prepared.as_bytes(), expected);
    assert_eq!(prepared.digest().as_bytes().as_slice(), hex(TYPE_T_SHA)?);
    assert!(std::ptr::eq(workflow_type.as_ptr(), query.as_ptr()));
    assert_noncanonical_present_encoding()?;
    assert_generation_boundary_reference()?;
    Ok(())
}

fn assert_noncanonical_present_encoding() -> TestResult {
    // LARGE_TUPLE, Latin1 atom tag/type, padded generation: retain exact wire form.
    let expected = hex(concat!(
        "83690000000364000e666c6f775f706f6c6963795f76316e020000007400000001",
        "640004747970656d0000000174"
    ))?;
    let digest: [u8; 32] = Sha256::digest(&expected).into();
    let safety = OriginalEtfSafety::new(&[TAG, "type"], &[], None, &[]);
    let limits = profile(100_000)?;
    let mut charge = PolicySchemaCharge::new(limits);
    let value = prepare(
        ReadValue::Binary(&expected),
        b"t",
        &safety,
        limits,
        &mut charge,
    )?;
    assert_eq!(value.encoded(), expected);
    assert_eq!(value.generation(), 0);
    assert_eq!(value.workflow_type(), b"t");
    assert_eq!(value.digest(), PreparedDigest::new(digest));
    assert_ne!(value.encoded(), hex(TYPE_T)?);
    assert!(expected.starts_with(&[131, 105, 0, 0, 0, 3]));
    assert!(!std::ptr::eq(value.encoded().as_ptr(), expected.as_ptr()));
    Ok(())
}

fn assert_generation_boundary_reference() -> TestResult {
    let expected = hex(concat!(
        "836803770e666c6f775f706f6c6963795f76316e0700ffffffffffff1f7400000002",
        "7704747970656d0000001367656e65726174696f6e2d626f756e646172797707",
        "76657273696f6e6d000000097075626c69632d7631"
    ))?;
    let digest = "a58e405da3872edcefc54b4aefa173d2974f19f6f6d0c4eb477a27bcff950228";
    assert_eq!(
        (expected.len(), Sha256::digest(&expected).as_slice()),
        (87, hex(digest)?.as_slice())
    );
    let limits = profile(100_000)?;
    let mut charge = PolicySchemaCharge::new(limits);
    let safety = OriginalEtfSafety::new(&[TAG, "type", "version"], &[], None, &[]);
    let value = prepare(
        ReadValue::Binary(&expected),
        b"generation-boundary",
        &safety,
        limits,
        &mut charge,
    )?;
    observe(
        &value,
        &expected,
        digest,
        9_007_199_254_740_991,
        b"generation-boundary",
    )?;
    assert_eq!(&value.encoded()[29..], &expected[29..]);
    Ok(())
}

#[test]
fn original_reference_preserves_raw_failure_and_continuous_copy_hash_refusals() -> TestResult {
    assert_present_phase_refusals()?;
    assert_absent_repeats_and_prior_peak()?;
    assert_read_failures_and_output_cap()?;
    Ok(())
}

fn assert_present_phase_refusals() -> TestResult {
    let bytes = hex(TYPE_T)?;
    let safety = OriginalEtfSafety::new(&[TAG, "type"], &[], None, &[]);
    // Decoder6778 + raw-read root128/type130. Only copy/hash/reference tails follow.
    for (target, retained) in [
        (7_484, 6_906),
        (7_650, 7_036),
        (7_816, 7_202),
        (7_944, 7_368),
    ] {
        let limits = profile(target - 1)?;
        let mut charge = PolicySchemaCharge::new(limits);
        assert_eq!(
            prepare(
                ReadValue::Binary(&bytes),
                b"t",
                &safety,
                limits,
                &mut charge
            )
            .err(),
            Some(ReadError::Encoding(work(target, target - 1)))
        );
        assert_eq!((charge.g(), charge.p()), (retained, 448));
        assert_eq!(bytes, hex(TYPE_T)?);
    }
    for maximum in [7_944, 15_439, 15_440] {
        let limits = profile(maximum)?;
        let mut charge = PolicySchemaCharge::new(limits);
        let first = prepare(
            ReadValue::Binary(&bytes),
            b"t",
            &safety,
            limits,
            &mut charge,
        )?;
        observe(&first, &bytes, TYPE_T_SHA, 0, b"t")?;
        assert_eq!((charge.g(), charge.p()), (7_496, 448));
        if maximum > 7_944 {
            let second = prepare(
                ReadValue::Binary(&bytes),
                b"t",
                &safety,
                limits,
                &mut charge,
            );
            if maximum == 15_439 {
                assert_eq!(
                    second.err(),
                    Some(ReadError::Encoding(work(15_440, maximum)))
                );
                assert_eq!((charge.g(), charge.p()), (14_864, 448));
            } else {
                observe(&second?, &bytes, TYPE_T_SHA, 0, b"t")?;
                assert_eq!((charge.g(), charge.p()), (14_992, 448));
            }
            observe(&first, &bytes, TYPE_T_SHA, 0, b"t")?;
        }
    }
    Ok(())
}

fn assert_absent_repeats_and_prior_peak() -> TestResult {
    let bytes = hex(TYPE_T)?;
    let safety = OriginalEtfSafety::new(&[], &[], None, &[]);
    // Frozen atomic stages: raw128, fixed measure806, reserve166, emit806, SHA166, meta128.
    for (target, prior_g) in [
        (128, 0),
        (934, 128),
        (1_100, 934),
        (1_906, 1_100),
        (2_072, 1_906),
        (2_200, 2_072),
    ] {
        let limits = profile(target - 1)?;
        let mut charge = PolicySchemaCharge::new(limits);
        assert_eq!(
            prepare(ReadValue::Absent, b"t", &safety, limits, &mut charge).err(),
            Some(ReadError::Encoding(work(target, target - 1)))
        );
        assert_eq!((charge.g(), charge.p()), (prior_g, 0));
    }
    for (maximum, attempts, seed, retained) in [
        (2_200, 1, false, 2_200),
        (2_199, 1, false, 2_072),
        (4_400, 2, false, 4_400),
        (4_399, 2, false, 4_272),
        (4_192, 1, true, 4_128),
        (4_191, 1, true, 4_000),
    ] {
        let limits = profile(maximum)?;
        let mut charge = PolicySchemaCharge::new(limits);
        let held = if seed {
            let mut data = minimal_data();
            data.states = BTreeMap::new();
            data.indexed_attributes = Some(vec![b"x".to_vec()]);
            let held = restore_complete_policy(1, data, limits, &mut charge)?;
            assert_eq!((charge.g(), charge.p()), (1_928, 64));
            Some(held)
        } else {
            None
        };
        let mut outputs = Vec::new();
        for attempt in 0..attempts {
            let target = if seed {
                4_192
            } else if attempts == 2 {
                4_400
            } else {
                2_200
            };
            let result = prepare(ReadValue::Absent, b"t", &safety, limits, &mut charge);
            if attempt + 1 == attempts && maximum < target {
                assert_eq!(
                    result.err(),
                    Some(ReadError::Encoding(work(target, maximum)))
                );
            } else {
                outputs.push(result?);
            }
        }
        assert_eq!(
            (charge.g(), charge.p()),
            (retained, if seed { 64 } else { 0 })
        );
        for output in &outputs {
            observe(output, &bytes, TYPE_T_SHA, 0, b"t")?;
        }
        if let Some(held) = held {
            assert_eq!(held.generation(), 1);
            assert_eq!(held.data().indexed_attributes, Some(vec![b"x".to_vec()]));
        }
    }
    Ok(())
}

fn assert_read_failures_and_output_cap() -> TestResult {
    let limits = profile(100_000_000)?;
    let poisoned = OriginalEtfSafety::new(&["duplicate", "duplicate"], &[], None, &[]);
    for (value, expected) in [
        (ReadValue::Other, ReadError::Corrupt),
        (ReadValue::Unavailable, ReadError::ShardUnavailable),
        (ReadValue::ReadFailed, ReadError::ReadFailed),
    ] {
        let mut charge = PolicySchemaCharge::new(limits);
        assert_eq!(
            prepare(value, b"t", &poisoned, limits, &mut charge).err(),
            Some(expected)
        );
        assert_eq!((charge.g(), charge.p()), (128, 0));
    }
    let safety = OriginalEtfSafety::new(&[TAG, "type", "n"], &[], None, &[]);
    for raw in [
        vec![],
        hex("836802770e666c6f775f706f6c6963795f76317400000000")?,
        hex("836803770e666c6f775f706f6c6963795f7631610074000000016d00000004747970656d0000000174")?,
    ] {
        let mut charge = PolicySchemaCharge::new(limits);
        assert_eq!(
            prepare(ReadValue::Binary(&raw), b"t", &safety, limits, &mut charge).err(),
            Some(ReadError::Corrupt)
        );
    }
    let local =
        hex("836803770e666c6f775f706f6c6963795f7631610074000000016d000000017879000000006a")?;
    let mut charge = PolicySchemaCharge::new(limits);
    assert_eq!(
        prepare(
            ReadValue::Binary(&local),
            b"t",
            &safety,
            limits,
            &mut charge
        )
        .err(),
        Some(ReadError::Encoding(Error::UnsupportedTag))
    );
    for length in [263_131, 263_132] {
        let query = vec![b'x'; length];
        let expected = fallback(&query)?;
        let digest: [u8; 32] = Sha256::digest(&expected).into();
        let mut charge = PolicySchemaCharge::new(limits);
        let result = prepare(ReadValue::Absent, &query, &poisoned, limits, &mut charge);
        if length == 263_131 {
            let value = result?;
            assert_eq!(value.encoded(), expected);
            assert_eq!(value.workflow_type(), query);
            assert_eq!(value.generation(), 0);
            assert_eq!(value.digest(), PreparedDigest::new(digest));
            assert_eq!((charge.g(), charge.p()), (1_054_720, 0));
        } else {
            assert_eq!(result.err(), Some(ReadError::Encoding(Error::ByteLimit)));
        }
        assert_eq!(expected.len(), 37 + length);
    }
    let mut charge = PolicySchemaCharge::new(limits);
    assert_eq!(
        prepare(
            ReadValue::Absent,
            b"t",
            &poisoned,
            profile(99_999_999)?,
            &mut charge
        )
        .err(),
        Some(ReadError::Encoding(Error::Schema(
            PolicySchemaError::ProfileMismatch
        )))
    );
    assert_eq!((charge.g(), charge.p()), (0, 0));
    Ok(())
}

fn hex(value: &str) -> TestResult<Vec<u8>> {
    value
        .as_bytes()
        .as_chunks::<2>()
        .0
        .iter()
        .map(|pair| Ok(u8::from_str_radix(std::str::from_utf8(pair)?, 16)?))
        .collect()
}

fn fallback(query: &[u8]) -> TestResult<Vec<u8>> {
    let mut bytes = hex("836803770e666c6f775f706f6c6963795f7631610074000000017704747970656d")?;
    bytes.extend_from_slice(&u32::try_from(query.len())?.to_be_bytes());
    bytes.extend_from_slice(query);
    Ok(bytes)
}

fn observe(
    value: &OriginalPolicyReference<'_>,
    expected: &[u8],
    hash: &str,
    generation: u64,
    workflow_type: &[u8],
) -> TestResult {
    assert_eq!(value.encoded(), expected);
    assert_eq!(value.generation(), generation);
    assert_eq!(value.workflow_type(), workflow_type);
    assert_eq!(value.digest().as_bytes().as_slice(), hex(hash)?);
    Ok(())
}

fn work(target: u64, maximum: u64) -> Error {
    Error::Schema(PolicySchemaError::WorkBytes {
        bytes: target,
        maximum,
    })
}
