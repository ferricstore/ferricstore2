//! Expected ETF assembly over explicitly supplied source-normalized fields.

use super::{Gov, Raw, Reason, Retention, TestResult, encode, limits, normalize};
use super::{PolicySchemaCharge, PolicySchemaLimits};
use wf_domain::policy::complete::{
    RawPolicyRetryValue as RetryRaw, normalize_complete_policy_indexed_input,
    normalize_complete_policy_retention_override_input, normalize_complete_policy_retry_input,
    normalize_complete_policy_retry_override_input,
};

pub(super) fn atom(value: &str) -> TestResult<Vec<u8>> {
    let mut bytes = vec![119, u8::try_from(value.len())?];
    bytes.extend_from_slice(value.as_bytes());
    Ok(bytes)
}

pub(super) fn binary(value: &[u8]) -> TestResult<Vec<u8>> {
    let mut bytes = vec![109];
    bytes.extend_from_slice(&u32::try_from(value.len())?.to_be_bytes());
    bytes.extend_from_slice(value);
    Ok(bytes)
}

pub(super) fn number(value: u128) -> TestResult<Vec<u8>> {
    if value <= 255 {
        return Ok(vec![97, u8::try_from(value)?]);
    }
    if value <= 2_147_483_647 {
        let mut bytes = vec![98];
        bytes.extend_from_slice(&i32::try_from(value)?.to_be_bytes());
        return Ok(bytes);
    }
    let magnitude = value.to_le_bytes();
    let length = magnitude
        .iter()
        .rposition(|&byte| byte != 0)
        .ok_or("positive magnitude")?
        + 1;
    let mut bytes = vec![110, u8::try_from(length)?, 0];
    bytes.extend_from_slice(&magnitude[..length]);
    Ok(bytes)
}

pub(super) fn list(values: &[Vec<u8>]) -> TestResult<Vec<u8>> {
    if values.is_empty() {
        return Ok(vec![106]);
    }
    let mut bytes = vec![108];
    bytes.extend_from_slice(&u32::try_from(values.len())?.to_be_bytes());
    for value in values {
        bytes.extend_from_slice(value);
    }
    bytes.push(106);
    Ok(bytes)
}

pub(super) fn map(values: &[(&str, Vec<u8>)]) -> TestResult<Vec<u8>> {
    let rows = values
        .iter()
        .map(|(name, value)| Ok((atom(name)?, value.clone())))
        .collect::<TestResult<Vec<_>>>()?;
    encoded_map(&rows)
}

pub(super) fn named_map(values: &[(Vec<u8>, Vec<u8>)]) -> TestResult<Vec<u8>> {
    let mut rows = values.to_vec();
    rows.sort_by(|left, right| left.0.cmp(&right.0));
    let rows = rows
        .into_iter()
        .map(|(name, value)| Ok((binary(&name)?, value)))
        .collect::<TestResult<Vec<_>>>()?;
    encoded_map(&rows)
}

fn encoded_map(values: &[(Vec<u8>, Vec<u8>)]) -> TestResult<Vec<u8>> {
    let mut bytes = vec![116];
    bytes.extend_from_slice(&u32::try_from(values.len())?.to_be_bytes());
    for (key, value) in values {
        bytes.extend_from_slice(key);
        bytes.extend_from_slice(value);
    }
    Ok(bytes)
}

pub(super) fn snapshot(workflow_type: &[u8], fields: &[(&str, Vec<u8>)]) -> TestResult<Vec<u8>> {
    let mut fields = fields.to_vec();
    if !fields.iter().any(|(name, _)| *name == "states") {
        fields.push(("states", map(&[])?));
    }
    fields.push(("type", binary(workflow_type)?));
    fields.sort_by(|left, right| left.0.cmp(right.0));
    let mut bytes = vec![131];
    bytes.extend_from_slice(&map(&fields)?);
    Ok(bytes)
}

pub(super) fn success(input: &Raw<'_>) -> TestResult<Vec<u8>> {
    let profile = limits(128 * 1_024 * 1_024)?;
    let mut charge = PolicySchemaCharge::new(profile);
    normalize(&Raw::Binary(b"r"), input, 1_000_000, profile, &mut charge)?
        .map_err(|reason| format!("unexpected input reason {reason:?}").into())
}

pub(super) fn rejection(input: &Raw<'_>, reason: Reason, body: &[u8]) -> TestResult {
    let profile = limits(128 * 1_024 * 1_024)?;
    let mut charge = PolicySchemaCharge::new(profile);
    assert_eq!(
        normalize(&Raw::Binary(b"r"), input, 17, profile, &mut charge)?,
        Err(reason)
    );
    assert_body(reason, body, profile, &mut charge)
}

pub(super) fn assert_body(
    reason: Reason,
    body: &[u8],
    profile: PolicySchemaLimits,
    charge: &mut PolicySchemaCharge,
) -> TestResult {
    let before = charge.g();
    let owned = encode(reason, profile, charge)?;
    assert_eq!(owned, body);
    let digits = if matches!(
        reason,
        Reason::Retention(
            wf_domain::policy::complete::PolicyRetentionInputRejection::History { .. }
        )
    ) {
        body.len().checked_sub(60).ok_or("history prefix")?
    } else {
        0
    };
    assert_eq!(
        charge.g(),
        before + 256 + u64::try_from(body.len() + digits)?
    );
    Ok(())
}

pub(super) fn gov_reason(input: Raw<'_>, reason: Gov, body: &[u8]) -> TestResult {
    let rows = [(Raw::Atom("governance"), input)];
    rejection(&Raw::Map(&rows), Reason::Governance(reason), body)
}

pub(super) fn assert_indexed_state_nil_shadowing(
    name: &str,
    good: Raw<'_>,
    reversed: bool,
) -> TestResult {
    let good = if name == "indexed_attributes" {
        Raw::List(&[Raw::Binary(b"owner")])
    } else if name == "states" {
        Raw::Map(&[(Raw::Binary(b"queued"), Raw::Map(&[]))])
    } else {
        good
    };
    let mut rows = [
        (Raw::Atom(name), Raw::Nil),
        (Raw::Binary(name.as_bytes()), good),
    ];
    if reversed {
        rows.reverse();
    }
    if name == "states" {
        rejection(
            &Raw::Map(&rows),
            Reason::StatesShape,
            b"ERR flow policy states must be a map or keyword list",
        )?;
    } else {
        let fields = if name == "indexed_attributes" {
            vec![("indexed_attributes", list(&[])?)]
        } else {
            Vec::new()
        };
        assert_eq!(success(&Raw::Map(&rows))?, snapshot(b"r", &fields)?);
    }
    Ok(())
}

pub(super) fn complete_governance() -> Raw<'static> {
    Raw::Map(&[
        (Raw::Atom("mode"), Raw::Binary(b"full")),
        (
            Raw::Atom("limits"),
            Raw::Map(&[(
                Raw::Binary(b"running"),
                Raw::Map(&[
                    (Raw::Atom("limit"), Raw::Integer(100)),
                    (Raw::Atom("enforcement"), Raw::Binary(b"strict_global")),
                    (Raw::Atom("lease_size"), Raw::Integer(20)),
                ]),
            )]),
        ),
        (
            Raw::Atom("budgets"),
            Raw::Map(&[(
                Raw::Binary(b"llm:gpt"),
                Raw::Map(&[
                    (Raw::Atom("limit"), Raw::Integer(10_000)),
                    (Raw::Atom("window_ms"), Raw::Integer(60_000)),
                    (Raw::Atom("unit"), Raw::Binary(b"tokens")),
                ]),
            )]),
        ),
        (
            Raw::Atom("circuits"),
            Raw::Map(&[(
                Raw::Binary(b"effect:email.send"),
                Raw::Map(&[
                    (Raw::Atom("failure_threshold"), Raw::Integer(5)),
                    (Raw::Atom("open_ms"), Raw::Integer(30_000)),
                    (Raw::Atom("window_ms"), Raw::Integer(60_000)),
                    (Raw::Atom("min_calls"), Raw::Integer(10)),
                    (Raw::Atom("failure_rate_pct"), Raw::Integer(50)),
                    (Raw::Atom("latency_threshold_ms"), Raw::Integer(2_000)),
                    (
                        Raw::Atom("error_classes"),
                        Raw::List(&[Raw::Binary(b"TimeoutError")]),
                    ),
                    (Raw::Atom("half_open_max_probes"), Raw::Integer(3)),
                    (Raw::Atom("half_open_success_threshold"), Raw::Integer(2)),
                ]),
            )]),
        ),
        (
            Raw::Atom("approvals"),
            Raw::Map(&[
                (
                    Raw::Atom("effects"),
                    Raw::List(&[Raw::Binary(b"stripe.refund")]),
                ),
                (
                    Raw::Atom("states"),
                    Raw::List(&[Raw::Binary(b"payment_review")]),
                ),
                (
                    Raw::Atom("assignees"),
                    Raw::List(&[Raw::Binary(b"finance")]),
                ),
            ]),
        ),
    ])
}

pub(super) fn complete_governance_bytes() -> TestResult<Vec<u8>> {
    map(&[
        (
            "approvals",
            map(&[
                ("assignees", list(&[binary(b"finance")?])?),
                ("effects", list(&[binary(b"stripe.refund")?])?),
                ("states", list(&[binary(b"payment_review")?])?),
            ])?,
        ),
        (
            "budgets",
            named_map(&[(
                b"llm:gpt".to_vec(),
                map(&[
                    ("limit", number(10_000)?),
                    ("unit", binary(b"tokens")?),
                    ("window_ms", number(60_000)?),
                ])?,
            )])?,
        ),
        (
            "circuits",
            named_map(&[(
                b"effect:email.send".to_vec(),
                map(&[
                    ("error_classes", list(&[binary(b"TimeoutError")?])?),
                    ("failure_rate_pct", number(50)?),
                    ("failure_threshold", number(5)?),
                    ("half_open_max_probes", number(3)?),
                    ("half_open_success_threshold", number(2)?),
                    ("latency_threshold_ms", number(2_000)?),
                    ("min_calls", number(10)?),
                    ("open_ms", number(30_000)?),
                    ("window_ms", number(60_000)?),
                ])?,
            )])?,
        ),
        (
            "limits",
            named_map(&[(
                b"running".to_vec(),
                map(&[
                    ("enforcement", atom("strict_global")?),
                    ("lease_size", number(20)?),
                    ("limit", number(100)?),
                ])?,
            )])?,
        ),
        ("mode", atom("full")?),
    ])
}

pub(super) fn assert_captured_history() -> TestResult {
    for (maximum, expected, fee) in [
        (
            1,
            b"ERR flow retention history_max_events must be between 1 and 1".as_slice(),
            318,
        ),
        (
            17,
            b"ERR flow retention history_max_events must be between 1 and 17",
            320,
        ),
        (
            1_000_000,
            b"ERR flow retention history_max_events must be between 1 and 1000000",
            330,
        ),
    ] {
        let profile = limits(10_000_000)?;
        let mut charge = PolicySchemaCharge::new(profile);
        let retention = [(
            Raw::Atom("history_max_events"),
            Raw::Integer(i128::from(maximum) + 1),
        )];
        let actual = normalize(
            &Raw::Binary(b"r"),
            &Raw::Map(&[(Raw::Atom("retention"), Raw::Map(&retention))]),
            maximum,
            profile,
            &mut charge,
        )?;
        let reason = Reason::Retention(Retention::History { maximum });
        assert_eq!(actual, Err(reason));
        let future_maximum = 17;
        let before = charge.g();
        let held = encode(reason, profile, &mut charge)?;
        assert_eq!(held, expected);
        assert_eq!(charge.g(), before + fee);
        assert_eq!(future_maximum, 17);
        assert_eq!(encode(reason, profile, &mut charge)?, expected);
    }
    Ok(())
}

pub(super) fn assert_old_primitive_fees() -> TestResult {
    let profile = limits(10_000)?;
    for (full, expected) in [(false, 512), (true, 646)] {
        let mut charge = PolicySchemaCharge::new(profile);
        if full {
            assert_eq!(
                normalize_complete_policy_retry_input(&RetryRaw::Nil, profile, &mut charge)?
                    .map(|value| (value.max_retries, value.exhausted_to)),
                Ok((3, b"failed".to_vec()))
            );
        } else {
            assert_eq!(
                normalize_complete_policy_retry_override_input(
                    &RetryRaw::Nil,
                    profile,
                    &mut charge
                )?,
                Ok(None)
            );
        }
        assert_eq!((charge.g(), charge.p()), (expected, 0));
    }
    let mut charge = PolicySchemaCharge::new(profile);
    assert_eq!(
        normalize_complete_policy_retention_override_input(
            &RetryRaw::Nil,
            17,
            profile,
            &mut charge
        )?,
        Ok(None)
    );
    assert_eq!((charge.g(), charge.p()), (512, 0));
    let mut charge = PolicySchemaCharge::new(profile);
    assert_eq!(
        normalize_complete_policy_indexed_input(None, None, profile, &mut charge)?
            .map_err(|reason| format!("indexed {reason:?}"))?,
        (None, None)
    );
    assert_eq!((charge.g(), charge.p()), (384, 0));
    Ok(())
}

pub(super) fn assert_governance_enum_neighbors() -> TestResult {
    for (value, expected) in [
        (Raw::Atom("approximate_global"), "approximate_global"),
        (Raw::Binary(b"APPROXIMATE_GLOBAL"), "approximate_global"),
        (Raw::Atom("strict_global"), "strict_global"),
        (Raw::Binary(b"STRICT_GLOBAL"), "strict_global"),
    ] {
        let rule = [
            (Raw::Atom("limit"), Raw::Integer(1)),
            (Raw::Atom("enforcement"), value),
        ];
        let expected = named_map(&[(
            b"scope".to_vec(),
            map(&[("enforcement", atom(expected)?), ("limit", number(1)?)])?,
        )])?;
        assert_eq!(
            success(&Raw::Map(&[(
                Raw::Atom("governance"),
                Raw::Map(&[(
                    Raw::Atom("limits"),
                    Raw::Map(&[(Raw::Binary(b"scope"), Raw::Map(&rule))])
                )])
            )]))?,
            snapshot(b"r", &[("governance", map(&[("limits", expected)])?)])?
        );
    }
    let modes = [
        ("mode", &["minimal", "ledger", "full"][..]),
        ("denials", &["off", "sampled", "all"][..]),
    ]
    .into_iter()
    .flat_map(|(name, accepted)| accepted.iter().map(move |&value| (name, value)));
    for (name, value) in modes {
        let uppercase = value.to_ascii_uppercase();
        for raw in [Raw::Atom(value), Raw::Binary(uppercase.as_bytes())] {
            let fields = [(Raw::Atom(name), raw)];
            let audit = [(Raw::Atom("audit"), Raw::Map(&fields))];
            let group = if name == "mode" {
                Raw::Map(&fields)
            } else {
                Raw::Map(&audit)
            };
            let expected = if name == "mode" {
                map(&[("mode", atom(value)?)])?
            } else {
                map(&[("audit", map(&[("denials", atom(value)?)])?)])?
            };
            assert_eq!(
                success(&Raw::Map(&[(Raw::Atom("governance"), group)]))?,
                snapshot(b"r", &[("governance", expected)])?
            );
        }
    }
    for bad in [
        b"\xffFULL".as_slice(),
        "\u{212a}full".as_bytes(),
        "ſampled".as_bytes(),
        "İ".as_bytes(),
        b" full",
        b"FULL ",
    ] {
        gov_reason(
            Raw::Map(&[(Raw::Atom("mode"), Raw::Binary(bad))]),
            Gov::Mode,
            b"ERR invalid flow governance mode",
        )?;
    }
    Ok(())
}

pub(super) fn assert_order_tariff() -> TestResult {
    let first = [Raw::Binary(b"s01"), Raw::Map(&[])];
    let last = [Raw::Binary(b"s02"), Raw::Map(&[])];
    let leaves = [Raw::Tuple(&first), Raw::Tuple(&last)];
    let rows = [(Raw::Atom("states"), Raw::List(&leaves))];
    let mut events = vec![(128, 0), (128, 0), (1, 0), (128, 0), (128, 0), (6, 0)];
    events.extend([138, 141, 147, 139, 143, 152, 152, 144, 140].map(|fee| (fee, 0)));
    events.extend([(128, 0), (128, 0)]); // membership, list descriptor
    for _ in 0..2 {
        events.extend([128, 128, 3, 128].map(|fee| (fee, 0)));
    }
    events.extend([(0, 9_664), (7_296, 0), (1_920, 0), (448, 0)]);
    events.push((128, 0));
    for (query, validator) in [
        (4, 0),
        (7, 128),
        (13, 128),
        (5, 0),
        (9, 0),
        (18, 128),
        (18, 128),
        (10, 0),
        (6, 0),
    ] {
        selector_events(&mut events, 6, query);
        if validator != 0 {
            events.push((validator, 0));
        }
    }
    // Keyword false on first binary key, then all two state-policy pairs.
    events.extend(std::iter::repeat_n((128, 0), 5));
    // Source order: cached compare, exact order, and atomic work-slot swaps.
    events.extend([128, 128, 131, 128, 128, 131, 16, 128, 134, 256, 256].map(|fee| (fee, 0)));
    for _ in 0..2 {
        events.extend([128, 138, 128, 128, 128, 128, 128, 128].map(|fee| (fee, 0)));
        // child Map, name cell, then the prospective whole-slot update.
        events.extend(std::iter::repeat_n((128, 0), 3));
    }
    events.extend(std::iter::repeat_n((128, 0), 4)); // deferred parent pairs
    events.push((128, 0)); // enclosing normalized states Map
    events.extend([134, 512, 512].map(|fee| (fee, 0))); // paired-edge sort
    events.extend(std::iter::repeat_n((128, 0), 8));
    let writer = [1, 133, 136, 133, 136, 133, 136, 133, 134, 134];
    events.extend(writer.map(|fee| (fee, 0)));
    events.push((185, 0));
    events.extend(writer.map(|fee| (fee, 0)));
    events.push((128, 0));
    let expected = snapshot(
        b"r",
        &[(
            "states",
            named_map(&[(b"s01".to_vec(), map(&[])?), (b"s02".to_vec(), map(&[])?)])?,
        )],
    )?;
    assert_eq!(expected.len(), 57);
    super::refusals::assert_stream(&Raw::Map(&rows), &events, &expected, (27_396, 9_664))
}

pub(super) fn assert_circuit_scalar_order() -> TestResult {
    use wf_domain::policy::complete::GovernanceField as Field;
    let invalid = [
        ("failure_threshold", Gov::Positive(Field::FailureThreshold)),
        ("open_ms", Gov::Positive(Field::OpenMillis)),
        ("window_ms", Gov::Positive(Field::WindowMillis)),
        ("min_calls", Gov::Positive(Field::MinCalls)),
        ("failure_rate_pct", Gov::Percent),
    ];
    for first in 0..invalid.len() {
        let mut rule = vec![
            (Raw::Atom("failure_threshold"), Raw::Integer(1)),
            (Raw::Atom("open_ms"), Raw::Integer(1)),
        ];
        for &(name, _) in &invalid[first..] {
            if let Some(row) = rule
                .iter_mut()
                .find(|row| matches!(row.0, Raw::Atom(key) if key == name))
            {
                row.1 = Raw::Integer(0);
            } else {
                rule.push((Raw::Atom(name), Raw::Integer(0)));
            }
        }
        rule.push((Raw::Atom("latency_threshold_ms"), Raw::Integer(0)));
        let body = if invalid[first].0 == "failure_rate_pct" {
            String::from("ERR flow governance failure_rate_pct must be an integer from 1 to 100")
        } else {
            format!(
                "ERR flow governance {} must be a positive integer",
                invalid[first].0
            )
        };
        let rules = [(Raw::Binary(b"scope"), Raw::Map(&rule))];
        gov_reason(
            Raw::Map(&[(Raw::Atom("circuits"), Raw::Map(&rules))]),
            invalid[first].1,
            body.as_bytes(),
        )?;
    }
    Ok(())
}

pub(super) fn assert_rule_name_neighbor() -> TestResult {
    let name = vec![255; 65_535];
    let rule = [(Raw::Atom("limit"), Raw::Integer(0))];
    let rules = [(Raw::Binary(&name), Raw::Map(&rule))];
    let expected = snapshot(
        b"r",
        &[(
            "governance",
            map(&[(
                "limits",
                named_map(&[(name.clone(), map(&[("limit", number(0)?)])?)])?,
            )])?,
        )],
    )?;
    assert_eq!(
        success(&Raw::Map(&[(
            Raw::Atom("governance"),
            Raw::Map(&[(Raw::Atom("limits"), Raw::Map(&rules))])
        )]))?,
        expected
    );
    Ok(())
}

pub(super) fn assert_nil_keyword_keys(states: bool) -> TestResult {
    for key in [Raw::Nil, Raw::Atom("nil")] {
        let pair = [key, if states { Raw::Map(&[]) } else { Raw::Other }];
        let keyword = [Raw::Tuple(&pair)];
        if states {
            rejection(
                &Raw::Map(&[(Raw::Atom("states"), Raw::List(&keyword))]),
                Reason::StateName,
                b"ERR flow policy state must be a non-empty string",
            )?;
        } else {
            assert_eq!(success(&Raw::List(&keyword))?, snapshot(b"r", &[])?);
        }
    }
    Ok(())
}

pub(super) fn selector_events(events: &mut Vec<(u64, u64)>, key: u64, query: u64) {
    events.extend([(128, 0), (128 + key, 0), (128 + key + query, 0)]);
}
