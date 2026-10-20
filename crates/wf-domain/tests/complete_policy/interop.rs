//! Same-type compatibility: independent full authority and exact new owner cuts.
use super::{TestResult, candidate, data, limits};
use std::collections::BTreeMap;
use wf_domain::policy::complete::{
    CompleteCorePolicyPatchInput, CompletePolicyEncodingError as Error, PolicySchemaCharge,
    PolicySchemaError, PolicySchemaLimits, StoredPolicyValue,
    encode_original_policy_from_complete as encode, original_policy_from_complete_bytes as size,
    prepare_stored_core_policy_patch as patch,
};
use wf_domain::policy::definition::{
    BackoffPatch, FieldPatch, PolicyPatch, RetryPatch, StateChange, StateMode, StatePolicyPatch,
};
use wf_domain::policy::installation::{MAX_POLICY_GENERATION, PolicyError, PolicyLimits};
use wf_domain::policy::{Backoff, BackoffKind, RetryOverride};
use wf_domain::retention::{RetentionError, RetentionOverride};
mod accounting;
mod fixtures;
use accounting::{assert_frontier, prime};
use fixtures as f;

#[test]
fn v2_projection_preserves_every_complete_field_and_presence() -> TestResult {
    let profile = limits(2_000_000_000)?;
    let value = f::value(7, f::rich_data(), profile)?;
    let saved = value.encoded().to_vec();
    let expected = f::rich_wire(7);
    let mut charge = PolicySchemaCharge::new(profile);
    assert_eq!(size(&value, profile, &mut charge, 0)?.0, expected.len());
    assert_eq!(
        encode(&value, profile, &mut charge, 0, expected.len())?,
        expected
    );
    assert_eq!(value.encoded(), saved);
    assert!(value.data() == &f::rich_data());
    assert_variant_wires(profile)?;
    assert!(matches!(
        encode(&value, profile, &mut charge, 0, expected.len() - 1),
        Err(Error::ByteLimit)
    ));
    let other = limits(1_999_999_999)?;
    assert!(matches!(
        size(&value, other, &mut charge, 0),
        Err(Error::Schema(PolicySchemaError::ProfileMismatch))
    ));
    let mut raw = saved.clone();
    let last = raw.last_mut().ok_or("nonempty V2")?;
    *last ^= 1;
    let invalid = candidate(7, f::rich_data(), profile, &mut charge)?.bind_encoded_snapshot(
        0..1,
        raw,
        profile,
        &mut charge,
    )?;
    assert!(matches!(
        encode(&invalid, profile, &mut charge, 0, expected.len()),
        Err(Error::EncodingMismatch)
    ));
    Ok(())
}

fn assert_variant_wires(profile: PolicySchemaLimits) -> TestResult {
    use wf_domain::policy::definition::PolicyVersion;
    let bytes = b"\xff\0version";
    let value = f::value(
        2,
        data(
            b"r".to_vec(),
            Some(PolicyVersion::Bytes(bytes.to_vec())),
            BTreeMap::new(),
        ),
        profile,
    )?;
    let expected = f::envelope(
        2,
        &f::fields(&[
            ("states", f::map(&[])),
            ("type", f::binary(b"r")),
            ("version", f::binary(bytes)),
        ]),
    );
    assert_eq!(
        encode(
            &value,
            profile,
            &mut PolicySchemaCharge::new(profile),
            0,
            expected.len()
        )?,
        expected
    );
    for ledger in [false, true] {
        let (value, expected) = f::enum_variant(profile, ledger)?;
        assert_eq!(
            encode(
                &value,
                profile,
                &mut PolicySchemaCharge::new(profile),
                0,
                expected.len()
            )?,
            expected
        );
    }
    for (kind, label) in [
        (BackoffKind::None, "none"),
        (BackoffKind::Fixed, "fixed"),
        (BackoffKind::Linear, "linear"),
        (BackoffKind::Exponential, "exponential"),
    ] {
        let mut input = data(b"r".to_vec(), None, BTreeMap::new());
        input.retry = Some(RetryOverride {
            backoff: Some(Backoff {
                kind,
                base_ms: 0,
                max_ms: 0,
                jitter_pct: 0,
            }),
            ..RetryOverride::default()
        });
        let expected = f::envelope(
            1,
            &f::fields(&[
                (
                    "retry",
                    f::fields(&[(
                        "backoff",
                        f::fields(&[
                            ("base_ms", f::integer(0)),
                            ("jitter_pct", f::integer(0)),
                            ("kind", f::atom(label)),
                            ("max_ms", f::integer(0)),
                        ]),
                    )]),
                ),
                ("states", f::map(&[])),
                ("type", f::binary(b"r")),
            ]),
        );
        let value = f::value(1, input, profile)?;
        assert_eq!(
            encode(
                &value,
                profile,
                &mut PolicySchemaCharge::new(profile),
                0,
                expected.len()
            )?,
            expected
        );
    }
    Ok(())
}

fn input(generation: Option<u64>, change: PolicyPatch) -> CompleteCorePolicyPatchInput {
    CompleteCorePolicyPatchInput {
        workflow_type: b"r".to_vec(),
        expected_generation: generation,
        patch: change,
    }
}
fn fifo() -> PolicyPatch {
    PolicyPatch {
        states: BTreeMap::from([(
            b"queued".to_vec(),
            StateChange::Patch(StatePolicyPatch {
                mode: FieldPatch::Set(StateMode::Fifo),
                ..StatePolicyPatch::default()
            }),
        )]),
        ..PolicyPatch::default()
    }
}
fn original(value: &StoredPolicyValue) -> TestResult<&[u8]> {
    match value {
        StoredPolicyValue::Original(raw) => Ok(raw),
        StoredPolicyValue::Complete(_) => Err("lost original authority".into()),
    }
}

#[test]
fn core_patch_retains_original_generation_zero_big_integer_and_raw_leaves() -> TestResult {
    let profile = limits(2_000_000_000)?;
    let mut magnitude = vec![1];
    magnitude.extend_from_slice(&[0; 15]);
    magnitude.extend_from_slice(&[1, 0, 0]);
    for generation in [0, 7] {
        let raw = f::original_wire(generation, &magnitude, b"\0\xffopaque");
        let old = StoredPolicyValue::Original(raw.clone());
        let mut charge = PolicySchemaCharge::new(profile);
        let new = patch(
            Some(&old),
            0,
            input(Some(generation), fifo()),
            64,
            profile,
            &mut charge,
            raw.len(),
        )?;
        assert_eq!(
            original(&new)?,
            f::fifo_wire(generation + 1, &magnitude, b"\0\xffopaque")
        );
        assert_eq!(original(&old)?, raw);
        let higher = patch(
            Some(&old),
            17,
            input(Some(generation), fifo()),
            64,
            profile,
            &mut charge,
            raw.len(),
        )?;
        assert_eq!(
            original(&higher)?,
            f::fifo_wire(18, &magnitude, b"\0\xffopaque")
        );
    }
    Ok(())
}

fn partial_change() -> PolicyPatch {
    PolicyPatch {
        version: FieldPatch::Clear,
        max_active_ms: FieldPatch::Set(3),
        retry: FieldPatch::Set(RetryPatch {
            backoff: Some(BackoffPatch {
                base_ms: Some(9),
                ..BackoffPatch::default()
            }),
            ..RetryPatch::default()
        }),
        retention: FieldPatch::Set(RetentionOverride {
            ttl_ms: Some(200),
            history_max_events: None,
        }),
        states: BTreeMap::from([(
            b"queued".to_vec(),
            StateChange::Patch(StatePolicyPatch {
                retry: FieldPatch::Set(RetryPatch {
                    max_retries: Some(2),
                    ..RetryPatch::default()
                }),
                retention: FieldPatch::Clear,
                ..StatePolicyPatch::default()
            }),
        )]),
    }
}
fn assert_policy_error(
    old: &StoredPolicyValue,
    input: CompleteCorePolicyPatchInput,
    history: u32,
    high: u64,
    expected: PolicyError,
) -> TestResult {
    let profile = limits(2_000_000_000)?;
    let result = patch(
        Some(old),
        high,
        input,
        history,
        profile,
        &mut PolicySchemaCharge::new(profile),
        0,
    );
    assert!(
        matches!(result, Err(Error::Schema(PolicySchemaError::Policy(error))) if error == expected)
    );
    Ok(())
}

#[test]
fn stored_patch_preserves_partial_clear_remove_and_reached_error_order() -> TestResult {
    let profile = limits(2_000_000_000)?;
    assert_keyword_groups(profile)?;
    let old = StoredPolicyValue::Original(f::partial_wire(7, false));
    let changed = patch(
        Some(&old),
        0,
        input(Some(7), partial_change()),
        64,
        profile,
        &mut PolicySchemaCharge::new(profile),
        0,
    )?;
    assert_eq!(original(&changed)?, f::partial_wire(8, true));
    assert_eq!(original(&old)?, f::partial_wire(7, false));
    assert_policy_error(
        &old,
        input(Some(8), partial_change()),
        1,
        MAX_POLICY_GENERATION + 1,
        PolicyError::StaleGeneration,
    )?;
    assert_policy_error(
        &old,
        input(Some(7), partial_change()),
        32,
        0,
        PolicyError::Retention(RetentionError::History),
    )?;
    let invalid = PolicyPatch {
        states: BTreeMap::from([(
            b"queued".to_vec(),
            StateChange::Patch(StatePolicyPatch {
                mode: FieldPatch::Clear,
                ..StatePolicyPatch::default()
            }),
        )]),
        ..PolicyPatch::default()
    };
    assert_policy_error(
        &old,
        input(Some(7), invalid),
        64,
        MAX_POLICY_GENERATION + 1,
        PolicyError::InvalidStateMode,
    )?;
    let exhausted = StoredPolicyValue::Original(f::envelope(
        MAX_POLICY_GENERATION,
        &f::fields(&[("states", f::map(&[])), ("type", f::binary(b"r"))]),
    ));
    assert_policy_error(
        &exhausted,
        input(None, PolicyPatch::default()),
        64,
        0,
        PolicyError::GenerationExhausted,
    )?;
    assert_state_replacement_and_complete(profile)?;
    assert_reserved_state_removals(profile)?;
    Ok(())
}

fn assert_reserved_state_removals(profile: PolicySchemaLimits) -> TestResult {
    use wf_domain::value::{DomainError, RetryPolicyField};
    let old = StoredPolicyValue::Original(f::states_wire(7, &[b"", b"running", b"queued"]));
    let one = PolicySchemaLimits::new(PolicyLimits::new(1)?, profile.max_work_bytes())?;
    let change = PolicyPatch {
        states: BTreeMap::from([
            (Vec::new(), StateChange::Remove),
            (b"running".to_vec(), StateChange::Remove),
        ]),
        ..PolicyPatch::default()
    };
    let new = patch(
        Some(&old),
        0,
        input(Some(7), change),
        64,
        one,
        &mut PolicySchemaCharge::new(one),
        0,
    )?;
    assert_eq!(original(&new)?, f::states_wire(8, &[b"queued"]));
    assert_eq!(
        original(&old)?,
        f::states_wire(7, &[b"", b"running", b"queued"])
    );
    for (name, field) in [
        (b"".as_slice(), RetryPolicyField::State),
        (b"running", RetryPolicyField::RunningState),
    ] {
        let old = StoredPolicyValue::Original(f::states_wire(7, &[name, b"queued"]));
        let error = PolicyError::Domain(DomainError::InvalidRetryPolicy(field));
        assert_policy_error(&old, input(Some(7), PolicyPatch::default()), 64, 0, error)?;
        let change = PolicyPatch {
            states: BTreeMap::from([(
                name.to_vec(),
                StateChange::Patch(StatePolicyPatch::default()),
            )]),
            ..PolicyPatch::default()
        };
        assert_policy_error(&old, input(Some(7), change), 64, 0, error)?;
    }
    let absent = PolicyPatch {
        states: BTreeMap::from([(Vec::new(), StateChange::Remove)]),
        ..PolicyPatch::default()
    };
    assert!(matches!(
        patch(
            None,
            0,
            input(None, absent),
            64,
            profile,
            &mut PolicySchemaCharge::new(profile),
            0
        ),
        Err(Error::Schema(PolicySchemaError::Policy(
            PolicyError::InvalidStatePolicy
        )))
    ));
    Ok(())
}

fn assert_state_replacement_and_complete(profile: PolicySchemaLimits) -> TestResult {
    let one = PolicySchemaLimits::new(PolicyLimits::new(1)?, profile.max_work_bytes())?;
    let old = StoredPolicyValue::Original(f::envelope(
        7,
        &f::fields(&[
            (
                "states",
                f::map(&[(
                    f::binary(b"queued"),
                    f::fields(&[("mode", f::atom("fifo"))]),
                )]),
            ),
            ("type", f::binary(b"r")),
        ]),
    ));
    let states = BTreeMap::from([
        (b"queued".to_vec(), StateChange::Remove),
        (
            b"review".to_vec(),
            StateChange::Patch(StatePolicyPatch {
                mode: FieldPatch::Set(StateMode::Parallel),
                ..StatePolicyPatch::default()
            }),
        ),
    ]);
    let new = patch(
        Some(&old),
        0,
        input(
            Some(7),
            PolicyPatch {
                states,
                ..PolicyPatch::default()
            },
        ),
        64,
        one,
        &mut PolicySchemaCharge::new(one),
        0,
    )?;
    let expected = f::envelope(
        8,
        &f::fields(&[
            (
                "states",
                f::map(&[(
                    f::binary(b"review"),
                    f::fields(&[("mode", f::atom("parallel"))]),
                )]),
            ),
            ("type", f::binary(b"r")),
        ]),
    );
    assert_eq!(original(&new)?, expected);
    let complete = f::minimal(profile)?;
    let mut expected = complete.encoded().to_vec();
    expected[6..14].copy_from_slice(&2_u64.to_be_bytes());
    let old = StoredPolicyValue::Complete(Box::new(complete));
    let result = patch(
        Some(&old),
        0,
        input(Some(1), PolicyPatch::default()),
        64,
        profile,
        &mut PolicySchemaCharge::new(profile),
        0,
    )?;
    match result {
        StoredPolicyValue::Complete(value) => assert_eq!(value.encoded(), expected),
        StoredPolicyValue::Original(_) => return Err("Complete path changed output format".into()),
    }
    Ok(())
}

#[test]
fn interop_admits_each_conversion_and_splice_owner_on_one_schema_account() -> TestResult {
    // ARCH CREATE1-P1 + independent PERF ledger3b9c9b30; no SUT-derived caps.
    let profile = limits(30_825)?;
    let value = f::minimal(profile)?;
    let literal =
        b"FWPV\0\x02\0\0\0\0\0\0\0\x01\0\0\0\x16FWPS\0\x02\0\0\0\x01r\0\0\0\0\0\0\0\0\0\0\0";
    assert_eq!(value.encoded(), literal);
    assert_eq!(literal.len(), 40);
    assert_eq!(f::minimal_wire().len(), 51);
    let mut charge = PolicySchemaCharge::new(profile);
    prime(profile, &mut charge)?;
    assert_eq!(size(&value, profile, &mut charge, 64)?, (51, 8_864));
    assert_eq!((charge.g(), charge.p()), (5_932, 64));
    let held = vec![0x5a; 64];
    assert_eq!(
        encode(&value, profile, &mut charge, held.len(), 51)?,
        f::minimal_wire()
    );
    assert_eq!(
        (charge.g(), charge.p(), charge.total()),
        (21_334, 9_491, 30_825)
    );
    assert_eq!(held, [0x5a; 64]);
    for cap in [14_859, 14_860, 25_566, 25_567, 26_820, 26_821] {
        assert_frontier(cap, cap == 26_821)?;
    }
    let short = limits(30_824)?;
    let mut repeated = PolicySchemaCharge::new(short);
    prime(short, &mut repeated)?;
    assert_eq!(size(&value, short, &mut repeated, 64)?, (51, 8_864));
    assert!(matches!(
        encode(&value, short, &mut repeated, 64, 51),
        Err(Error::Schema(PolicySchemaError::WorkBytes {
            bytes: 30_825,
            maximum: 30_824
        }))
    ));
    assert_eq!((repeated.g(), repeated.p()), (21_200, 9_491));
    accounting::assert_raw_accounting()?;
    Ok(())
}

fn keyword(rows: &[(&str, Vec<u8>)]) -> Vec<u8> {
    let mut out = vec![108];
    let count = u32::try_from(rows.len()).unwrap_or_default();
    assert_eq!(u64::from(count), rows.len() as u64);
    out.extend_from_slice(&count.to_be_bytes());
    for (key, value) in rows {
        out.extend_from_slice(&[104, 2]);
        out.extend_from_slice(&f::atom(key));
        out.extend_from_slice(value);
    }
    out.push(106);
    out
}

fn keyword_retry(changed: bool, keywords: bool) -> Vec<u8> {
    let backoff = f::group(
        keywords,
        &[
            ("base_ms", f::integer(if changed { 9 } else { 17 })),
            ("jitter_pct", f::integer(4)),
            ("kind", f::atom("fixed")),
            ("max_ms", f::integer(33)),
            ("version", vec![110, 3, 0, 1, 0, 0]),
        ],
    );
    let mut rows = vec![
        ("backoff", backoff),
        ("exhausted_to", f::binary(b"dead")),
        ("max_retries", f::integer(if changed { 9 } else { 7 })),
        ("type", f::binary(b"\0\xffopaque")),
    ];
    if keywords {
        rows.insert(0, ("max_retries", f::integer(1)));
    }
    f::group(keywords, &rows)
}

fn assert_keyword_groups(profile: PolicySchemaLimits) -> TestResult {
    use wf_domain::policy::complete::decode_original_complete_policy_entry_portable;
    for keywords in [false, true] {
        let raw = f::keyword_wire(7, 0, keywords);
        let mut charge = PolicySchemaCharge::new(profile);
        let entry = decode_original_complete_policy_entry_portable(&raw, profile, &mut charge)
            .map_err(|error| format!("keyword donor={keywords}: {error:?}"))?
            .ok_or("healthy keyword donor did not decode")?;
        assert_eq!(
            (entry.generation(), entry.workflow_type()),
            (7, Some(b"r".as_slice()))
        );
        assert_eq!(entry.encoded(), raw);
        let old = StoredPolicyValue::Original(raw.clone());
        for phase in [1, 2] {
            let mut delta = if phase == 1 {
                PolicyPatch {
                    max_active_ms: FieldPatch::Set(3),
                    ..PolicyPatch::default()
                }
            } else {
                partial_change()
            };
            if phase == 2 {
                let FieldPatch::Set(retry) = &mut delta.retry else {
                    return Err("partial retry fixture".into());
                };
                retry.max_retries = Some(9);
            }
            let changed = patch(
                Some(&old),
                0,
                input(Some(7), delta),
                64,
                profile,
                &mut charge,
                0,
            )
            .map_err(|error| format!("keyword patch={keywords}, phase={phase}: {error:?}"))?;
            assert_eq!(original(&changed)?, f::keyword_wire(8, phase, keywords));
            assert_eq!(original(&old)?, raw);
        }
    }
    Ok(())
}
