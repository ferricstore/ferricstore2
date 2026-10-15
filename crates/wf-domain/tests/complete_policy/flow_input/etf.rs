//! Preproduction original-source ETF goldens and both late business size gates.

use wf_domain::policy::complete::{
    FlowPolicyInputRejection as Reason, GovernancePolicyInputRejection as Gov, OriginalEtfSafety,
    PolicySchemaCharge, PolicySchemaLimits, RawFlowPolicyValue as Raw,
    decode_original_complete_policy_entry as decode,
    normalize_original_complete_policy_input as normalize,
};

use super::super::{TestResult, limits};

fn hex(value: &str) -> TestResult<Vec<u8>> {
    if !value.len().is_multiple_of(2) {
        return Err("odd source golden".into());
    }
    value
        .as_bytes()
        .as_chunks::<2>()
        .0
        .iter()
        .map(|pair| Ok(u8::from_str_radix(std::str::from_utf8(pair)?, 16)?))
        .collect()
}

fn version_snapshot(version: &[u8]) -> TestResult<Vec<u8>> {
    let mut expected = hex(concat!(
        "837400000003770673746174657374000000007704747970656d0000000172",
        "770776657273696f6e"
    ))?;
    expected.extend_from_slice(version);
    Ok(expected)
}

fn assert_version(value: Raw<'_>, term: &[u8]) -> TestResult {
    let profile = PolicySchemaLimits::default();
    let mut charge = PolicySchemaCharge::new(profile);
    let attrs = [(Raw::Atom("version"), value)];
    let expected = version_snapshot(term)?;
    assert_eq!(
        normalize(
            &Raw::Binary(b"r"),
            &Raw::Map(&attrs),
            17,
            profile,
            &mut charge
        )?,
        Ok(expected)
    );
    Ok(())
}

#[test]
fn original_flow_input_preserves_arbitrary_customer_versions() -> TestResult {
    for (value, encoded) in [
        (0, "6100"),
        (255, "61ff"),
        (256, "6200000100"),
        (2_147_483_647, "627fffffff"),
        (2_147_483_648, "6e040000000080"),
        (18_446_744_073_709_551_615, "6e0800ffffffffffffffff"),
        (18_446_744_073_709_551_616, "6e0900000000000000000001"),
        (i128::MAX, "6e1000ffffffffffffffffffffffffffffff7f"),
    ] {
        assert_version(Raw::Integer(value), &hex(encoded)?)?;
    }
    for (negative, magnitude, encoded) in [
        (false, &[][..], "6100"),
        (true, &[0, 0][..], "6100"),
        (false, &[255, 0, 0][..], "61ff"),
        (false, &[0, 1, 0][..], "6200000100"),
        (
            false,
            &[0, 0, 0, 0, 0, 0, 0, 0, 1, 0][..],
            "6e0900000000000000000001",
        ),
    ] {
        assert_version(
            Raw::BigInteger {
                negative,
                little_endian: magnitude,
            },
            &hex(encoded)?,
        )?;
    }
    assert_big_widths()?;
    assert_version(Raw::Binary(&[255, 0, 128]), &hex("6d00000003ff0080")?)?;
    let profile = limits(10_000_000)?;
    for value in [
        Raw::Integer(-1),
        Raw::Integer(i128::MIN),
        Raw::Binary(b""),
        Raw::Atom("1"),
        Raw::Map(&[]),
        Raw::List(&[]),
        Raw::Other,
        Raw::BigInteger {
            negative: true,
            little_endian: &[0, 1, 0],
        },
    ] {
        let attrs = [(Raw::Atom("version"), value)];
        let mut charge = PolicySchemaCharge::new(profile);
        assert_eq!(
            normalize(
                &Raw::Binary(b"r"),
                &Raw::Map(&attrs),
                17,
                profile,
                &mut charge
            )?,
            Err(Reason::Version)
        );
    }
    assert_native_wide_version()?;
    Ok(())
}

fn assert_big_widths() -> TestResult {
    for digits in [255, 256] {
        let magnitude = vec![1; digits];
        let mut encoded = hex(if digits == 255 {
            "6eff00"
        } else {
            "6f0000010000"
        })?;
        encoded.extend_from_slice(&magnitude);
        assert_version(
            Raw::BigInteger {
                negative: false,
                little_endian: &magnitude,
            },
            &encoded,
        )?;
        let mut padded = magnitude.clone();
        padded.push(0);
        assert_version(
            Raw::BigInteger {
                negative: false,
                little_endian: &padded,
            },
            &encoded,
        )?;
    }
    Ok(())
}

fn assert_native_wide_version() -> TestResult {
    // Original source9b56a7: exact 63-byte term with customer version2^64.
    let expected = hex(concat!(
        "837400000003770673746174657374000000007704747970656d0000000c",
        "776964652d76657273696f6e770776657273696f6e6e0900000000000000000001"
    ))?;
    let attrs = [(
        Raw::Atom("version"),
        Raw::Integer(18_446_744_073_709_551_616),
    )];
    let profile = limits(10_000_000)?;
    let mut charge = PolicySchemaCharge::new(profile);
    let actual = normalize(
        &Raw::Binary(b"wide-version"),
        &Raw::Map(&attrs),
        17,
        profile,
        &mut charge,
    )?
    .map_err(|_| "wide customer integer rejected")?;
    assert_eq!(actual, expected);
    assert_eq!(actual.len(), 63);
    assert_reader_wrapper(&actual, b"wide-version")?;
    Ok(())
}

fn assert_reader_wrapper(snapshot: &[u8], workflow_type: &[u8]) -> TestResult {
    let mut envelope = hex("836803770e666c6f775f706f6c6963795f76316101")?;
    envelope.extend_from_slice(snapshot.get(1..).ok_or("missing VERSION")?);
    let atoms = [
        "flow_policy_v1",
        "states",
        "type",
        "version",
        "governance",
        "audit",
        "denials",
        "all",
        "effect_events",
        "false",
        "effects",
        "allowed",
        "require_idempotency_key",
        "mode",
        "full",
        "indexed_attributes",
        "indexed_state_meta",
        "max_active_ms",
        "retention",
        "history_max_events",
        "ttl_ms",
        "retry",
        "backoff",
        "base_ms",
        "jitter_pct",
        "kind",
        "fixed",
        "max_ms",
        "exhausted_to",
        "max_retries",
        "fifo",
    ];
    let safety = OriginalEtfSafety::new(&atoms, &[], None, &[]);
    let profile = limits(100_000_000)?;
    let mut charge = PolicySchemaCharge::new(profile);
    let entry =
        decode(&envelope, &safety, profile, &mut charge)?.ok_or("reader rejected golden")?;
    assert_eq!(entry.generation(), 1);
    assert_eq!(entry.workflow_type(), Some(workflow_type));
    assert_eq!(entry.policy_bytes(), &snapshot[1..]);
    assert_eq!(entry.encoded(), envelope);
    Ok(())
}

#[test]
fn original_flow_input_emits_original_deterministic_etf() -> TestResult {
    let profile = limits(100_000_000)?;
    let mut charge = PolicySchemaCharge::new(profile);
    let minimal = hex("837400000002770673746174657374000000007704747970656d0000000172")?;
    assert_eq!(
        normalize(&Raw::Binary(b"r"), &Raw::Map(&[]), 17, profile, &mut charge)?,
        Ok(minimal.clone())
    );
    assert_eq!(minimal.len(), 31);
    assert_reader_wrapper(&minimal, b"r")?;
    assert_full_golden(profile)?;
    Ok(())
}

fn assert_full_golden(profile: PolicySchemaLimits) -> TestResult {
    // Actual source033f0a before production: 509B deterministic OTP28.3.1 ETF.
    let backoff = [
        (Raw::Atom("kind"), Raw::Binary(b"fixed")),
        (Raw::Atom("base_ms"), Raw::Integer(7)),
    ];
    let retry = [
        (Raw::Atom("max_retries"), Raw::Integer(2)),
        (Raw::Atom("exhausted_to"), Raw::Binary(b"dead")),
        (Raw::Atom("backoff"), Raw::Map(&backoff)),
    ];
    let retention = [
        (Raw::Atom("ttl_ms"), Raw::Integer(42)),
        (Raw::Atom("history_max_events"), Raw::Integer(3)),
    ];
    let allowed = [Raw::Binary(b"b"), Raw::Binary(b"a"), Raw::Binary(b"b")];
    let effects = [
        (Raw::Atom("allowed"), Raw::List(&allowed)),
        (Raw::Atom("require_idempotency_key"), Raw::Atom("false")),
    ];
    let audit = [
        (Raw::Atom("effect_events"), Raw::Atom("false")),
        (Raw::Atom("denials"), Raw::Binary(b"ALL")),
    ];
    let governance = [
        (Raw::Atom("mode"), Raw::Binary(b"FULL")),
        (Raw::Atom("effects"), Raw::Map(&effects)),
        (Raw::Atom("audit"), Raw::Map(&audit)),
    ];
    let state_retry = [(Raw::Atom("max_retries"), Raw::Integer(1))];
    let state_z = [
        (Raw::Atom("mode"), Raw::Binary(b"FIFO")),
        (Raw::Atom("retry"), Raw::Map(&state_retry)),
    ];
    let pair_z = [Raw::Binary(b"z"), Raw::Map(&state_z)];
    let pair_a = [Raw::Binary(b"a"), Raw::Map(&[])];
    let states = [Raw::Tuple(&pair_z), Raw::Tuple(&pair_a)];
    let indexed = [Raw::Binary(b" x "), Raw::Binary(b"y")];
    let mut attrs = [
        (Raw::Atom("version"), Raw::Binary(&[255, 0, 128])),
        (Raw::Atom("max_active_ms"), Raw::Integer(1_000)),
        (Raw::Atom("retry"), Raw::Map(&retry)),
        (Raw::Atom("retention"), Raw::Map(&retention)),
        (Raw::Atom("indexed_attributes"), Raw::List(&indexed)),
        (Raw::Atom("indexed_state_meta"), Raw::Binary(b"node")),
        (Raw::Atom("governance"), Raw::Map(&governance)),
        (Raw::Atom("states"), Raw::List(&states)),
    ];
    let expected = full_golden()?;
    assert_eq!(expected.len(), 509);
    for reversed in [false, true] {
        if reversed {
            attrs.reverse();
        }
        let mut charge = PolicySchemaCharge::new(profile);
        let actual = normalize(
            &Raw::Binary(b"r"),
            &Raw::Map(&attrs),
            17,
            profile,
            &mut charge,
        )?
        .map_err(|_| "full original fixture rejected")?;
        assert_eq!(actual, expected);
        assert_reader_wrapper(&actual, b"r")?;
    }
    Ok(())
}

fn full_golden() -> TestResult<Vec<u8>> {
    hex(concat!(
        "837400000009770a676f7665726e616e63657400000003770561756469747400000002",
        "770764656e69616c737703616c6c770d6566666563745f6576656e7473770566616c7365",
        "77076566666563747374000000027707616c6c6f7765646c000000026d00000001626d",
        "00000001616a7717726571756972655f6964656d706f74656e63795f6b657977056661",
        "6c736577046d6f6465770466756c6c7712696e64657865645f61747472696275746573",
        "6c000000026d00000001786d00000001796a7712696e64657865645f73746174655f6d",
        "6574616d000000046e6f6465770d6d61785f6163746976655f6d7362000003e8770972",
        "6574656e74696f6e74000000027712686973746f72795f6d61785f6576656e74736103",
        "770674746c5f6d73612a77057265747279740000000377076261636b6f666674000000",
        "047707626173655f6d736107770a6a69747465725f706374611477046b696e64770566",
        "6978656477066d61785f6d736200007530770c6578686175737465645f746f6d000000",
        "0464656164770b6d61785f726574726965736102770673746174657374000000026d00",
        "0000016174000000006d000000017a740000000277046d6f646577046669666f770572",
        "657472797400000001770b6d61785f7265747269657361017704747970656d00000001",
        "72770776657273696f6e6d00000003ff0080"
    ))
}

#[test]
fn original_flow_input_preserves_both_source_size_gates() -> TestResult {
    let profile = PolicySchemaLimits::default();
    for length in [262_092, 262_093] {
        let payload = vec![b'x'; length];
        let attrs = [(Raw::Atom("version"), Raw::Binary(&payload))];
        let expected = if length == 262_092 {
            let mut bytes = hex(concat!(
                "837400000003770673746174657374000000007704747970656d00000008",
                "626f756e64617279770776657273696f6e6d0003ffcc"
            ))?;
            bytes.extend_from_slice(&payload);
            assert_eq!(bytes.len(), 262_144);
            Ok(bytes)
        } else {
            Err(Reason::SnapshotBytes)
        };
        let mut charge = PolicySchemaCharge::new(profile);
        let actual = normalize(
            &Raw::Binary(b"boundary"),
            &Raw::Map(&attrs),
            17,
            profile,
            &mut charge,
        )?;
        assert_eq!(actual, expected);
        assert!(payload.iter().all(|&byte| byte == b'x'));
    }
    let oversized = vec![b'x'; 262_144];
    let attrs = [(Raw::Atom("version"), Raw::Binary(&oversized))];
    let mut charge = PolicySchemaCharge::new(profile);
    assert_eq!(
        normalize(
            &Raw::Binary(b"oversized-policy"),
            &Raw::Map(&attrs),
            17,
            profile,
            &mut charge
        )?,
        Err(Reason::SnapshotBytes)
    );
    let earlier = [
        (Raw::Atom("version"), Raw::Binary(&oversized)),
        (Raw::Atom("states"), Raw::Integer(0)),
    ];
    assert_eq!(
        normalize(
            &Raw::Binary(b"r"),
            &Raw::Map(&earlier),
            17,
            profile,
            &mut charge
        )?,
        Err(Reason::StatesShape)
    );
    assert_governance_size_boundary(profile)?;
    Ok(())
}

fn assert_governance_size_boundary(profile: PolicySchemaLimits) -> TestResult {
    // Source M = VERSION/maps/allowed-list35 + five bytes per binary + payloads.
    // 15*65535 +65436 +35 +5*16 =1048576. A later state error distinguishes
    // an admitted governance subtree from its own late size rejection.
    for tail_length in [65_436, 65_437] {
        let mut leaves = vec![vec![b'e'; 65_535]; 15];
        leaves.push(vec![b'e'; tail_length]);
        for (index, leaf) in leaves.iter_mut().enumerate() {
            *leaf.last_mut().ok_or("empty boundary leaf")? = b'A' + u8::try_from(index)?;
        }
        assert_eq!(
            leaves.iter().map(Vec::len).sum::<usize>() + 115,
            983_140 + tail_length
        );
        let values = leaves
            .iter()
            .map(|leaf| Raw::Binary(leaf))
            .collect::<Vec<_>>();
        let effects = [(Raw::Atom("allowed"), Raw::List(&values))];
        let governance = [(Raw::Atom("effects"), Raw::Map(&effects))];
        let attrs = [
            (Raw::Atom("governance"), Raw::Map(&governance)),
            (Raw::Atom("states"), Raw::Integer(0)),
        ];
        let mut charge = PolicySchemaCharge::new(profile);
        assert_eq!(
            normalize(
                &Raw::Binary(b"r"),
                &Raw::Map(&attrs),
                17,
                profile,
                &mut charge
            )?,
            Err(if tail_length == 65_436 {
                Reason::StatesShape
            } else {
                Reason::Governance(Gov::PolicyBytes)
            })
        );
        assert_eq!(values.len(), 16);
        assert!(
            leaves
                .iter()
                .all(|leaf| leaf[..leaf.len() - 1].iter().all(|&byte| byte == b'e'))
        );
    }
    Ok(())
}
