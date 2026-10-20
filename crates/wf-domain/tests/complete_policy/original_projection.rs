//! Raw original authority survives generation-free execution projections.

use super::{TestResult, limits};
use wf_domain::policy::complete::{
    CompletePolicyEncodingError as Error, PolicySchemaCharge, PolicySchemaError,
    decode_original_complete_policy_entry_portable as decode,
    original_record_batch_policy_projection_bytes as measure,
    project_original_record_batch_policy as project,
};
use wf_domain::policy::definition::StateMode;

fn atom(name: &str) -> TestResult<Vec<u8>> {
    let mut bytes = vec![119, u8::try_from(name.len())?];
    bytes.extend_from_slice(name.as_bytes());
    Ok(bytes)
}

fn binary(value: &[u8]) -> TestResult<Vec<u8>> {
    let mut bytes = vec![109];
    bytes.extend_from_slice(&u32::try_from(value.len())?.to_be_bytes());
    bytes.extend_from_slice(value);
    Ok(bytes)
}

fn map(rows: &[(Vec<u8>, Vec<u8>)]) -> TestResult<Vec<u8>> {
    let mut bytes = vec![116];
    bytes.extend_from_slice(&u32::try_from(rows.len())?.to_be_bytes());
    for (key, value) in rows {
        bytes.extend_from_slice(key);
        bytes.extend_from_slice(value);
    }
    Ok(bytes)
}

fn envelope(generation: u8, fields: &[(Vec<u8>, Vec<u8>)]) -> TestResult<Vec<u8>> {
    let mut bytes = vec![131, 104, 3];
    bytes.extend_from_slice(&atom("flow_policy_v1")?);
    bytes.extend_from_slice(&[97, generation]);
    bytes.extend_from_slice(&map(fields)?);
    Ok(bytes)
}

#[test]
fn present_gen0_and_big_integer_remain_raw_authority() -> TestResult {
    let profile = limits(2_000_000_000)?;
    let mut charge = PolicySchemaCharge::new(profile);
    // Customer version 2^128+1 cannot fit PolicyVersion::Number(u64).
    let mut magnitude = vec![110, 17, 0, 1];
    magnitude.extend_from_slice(&[0; 15]);
    magnitude.push(1);
    let retry = map(&[(atom("max_retries")?, vec![97, 7])])?;
    let fifo = map(&[(atom("mode")?, atom("fifo")?)])?;
    let states = map(&[(binary(b"queued")?, fifo)])?;
    let governance = map(&[(atom("effects")?, map(&[])?)])?;
    let fields = [
        (atom("version")?, magnitude.clone()),
        (atom("retry")?, retry),
        (atom("states")?, states),
        (atom("governance")?, governance),
        (atom("type")?, binary(b"job")?),
    ];
    for generation in [0, 1] {
        let raw = envelope(generation, &fields)?;
        let saved = raw.clone();
        let owner = decode(&raw, profile, &mut charge)?.ok_or("original owner")?;
        assert_eq!(owner.generation(), u64::from(generation));
        assert_eq!(owner.workflow_type(), Some(b"job".as_slice()));
        assert!(std::ptr::eq(owner.encoded().as_ptr(), raw.as_ptr()));
        let needed = measure(&raw, b"job", profile, &mut charge)?;
        // Independent owner tariff: 1024 + two type copies + state slot/name copies.
        assert_eq!(needed, 1024 + 2 * 3 + 256 + 2 * 6);
        assert_eq!(needed, 1298);
        let before = charge.g();
        let (snapshot, retries) = project(&raw, b"job", profile, &mut charge, needed)?;
        assert!(charge.g() > before);
        assert_eq!(snapshot.workflow_type(), b"job");
        assert_eq!(snapshot.state_mode(b"queued"), StateMode::Fifo);
        assert_eq!(snapshot.state_mode(b"other"), StateMode::Parallel);
        assert_eq!(retries.type_retry().max_retries, Some(7));
        assert!(snapshot.version().is_none());
        assert!(
            owner
                .policy_bytes()
                .windows(magnitude.len())
                .any(|w| w == magnitude)
        );
        assert_eq!(owner.encoded(), saved);
        drop(snapshot);
        drop(retries);
        assert_eq!(raw, saved);
        assert!(matches!(
            project(&raw, b"job", profile, &mut charge, needed - 1),
            Err(Error::ByteLimit)
        ));
    }
    // There is no numeric-generation test standing in for presence: the
    // genuinely present generation-zero entry above has its exact raw owner.
    Ok(())
}

#[test]
fn stored_fallback_projection_matches_source_without_new_normalization() -> TestResult {
    let profile = limits(2_000_000_000)?;
    let mut charge = PolicySchemaCharge::new(profile);
    let local_retry = map(&[(atom("max_retries")?, vec![97, 9])])?;
    let fifo = map(&[
        (atom("mode")?, atom("fifo")?),
        (atom("retry")?, local_retry),
    ])?;
    let states = map(&[
        (binary(b"fifo")?, fifo),
        (binary(b"scalar")?, vec![97, 8]),
        (binary(b"unknown")?, map(&[(atom("mode")?, vec![97, 7])])?),
        (binary(b"nil")?, atom("nil")?),
    ])?;
    let retry = map(&[(atom("max_retries")?, vec![97, 3])])?;
    let raw = envelope(
        0,
        &[
            (atom("type")?, binary(b"job")?),
            (atom("states")?, states),
            (atom("retry")?, retry),
        ],
    )?;
    let needed = measure(&raw, b"job", profile, &mut charge)?;
    assert_eq!(needed, 1024 + 2 * 3 + 4 * 256 + 2 * (4 + 6 + 7 + 3));
    assert_eq!(needed, 2094);
    let (snapshot, retries) = project(&raw, b"job", profile, &mut charge, needed)?;
    assert_eq!(snapshot.state_mode(b"fifo"), StateMode::Fifo);
    for name in [b"scalar".as_slice(), b"unknown", b"nil", b"absent"] {
        assert_eq!(snapshot.state_mode(name), StateMode::Parallel);
    }
    assert_eq!(retries.type_retry().max_retries, Some(3));
    assert_eq!(
        retries
            .state_retry(b"fifo")
            .ok_or("local retry")?
            .max_retries,
        Some(9)
    );
    assert!(matches!(
        project(&raw, b"foreign", profile, &mut charge, needed),
        Err(Error::Schema(PolicySchemaError::InvalidAuthority))
    ));
    assert_profile_origin(profile)?;
    Ok(())
}

fn assert_profile_origin(profile: wf_domain::policy::complete::PolicySchemaLimits) -> TestResult {
    let unknown = envelope(
        0,
        &[
            (atom("type")?, binary(b"job")?),
            (atom("version")?, atom("not_in_the_canonical_emitter")?),
        ],
    )?;
    let mut charge = PolicySchemaCharge::new(profile);
    assert!(matches!(
        decode(&unknown, profile, &mut charge),
        Err(Error::UnsupportedPolicyProfile)
    ));
    for cut in [unknown.len() - 1, unknown.len() - 2] {
        assert!(decode(&unknown[..cut], profile, &mut charge)?.is_none());
    }
    let mut trailing = unknown.clone();
    trailing.push(0);
    assert!(decode(&trailing, profile, &mut charge)?.is_none());
    let mut wrong_envelope = unknown;
    wrong_envelope[2] = 2;
    assert!(decode(&wrong_envelope, profile, &mut charge)?.is_none());
    let mut errors = [None, None];
    for (ordinal, name) in [b"".as_slice(), b"running"].into_iter().enumerate() {
        // Original envelopes admit arbitrary binary state keys. Failure to
        // represent them in this execution projection is a healthy capability
        // boundary; it must preserve the valid original raw authority.
        let raw = envelope(
            0,
            &[
                (atom("type")?, binary(b"job")?),
                (atom("states")?, map(&[(binary(name)?, map(&[])?)])?),
            ],
        )?;
        let saved = raw.clone();
        let owner = decode(&raw, profile, &mut charge)?.ok_or("valid arbitrary state map")?;
        assert_eq!(owner.generation(), 0);
        let needed = measure(&raw, b"job", profile, &mut charge)?;
        errors[ordinal] = project(&raw, b"job", profile, &mut charge, needed).err();
        assert_eq!(owner.encoded(), saved);
        assert_eq!(raw, saved);
    }
    assert_eq!(errors, [Some(Error::UnsupportedPolicyProfile); 2]);
    Ok(())
}
