//! Original governance cases, complete fields and source-ordered diagnoses.

use super::cases::{complete_governance, complete_governance_bytes, gov_reason};
use super::{Gov, Raw, Reason, TestResult, atom, binary, list, map, named_map};
use super::{PolicySchemaCharge, limits, normalize, number, snapshot, success};
mod cases;

use cases::{assert_rule_error, assert_rule_scalars, assert_shapes_and_enums};

use wf_domain::policy::complete::{
    CircuitTrackingFailure as Tracking, GovernanceField as Field, GovernanceRuleKind as Kind,
};

#[test]
fn original_flow_input_preserves_complete_governance_model() -> TestResult {
    // Original c718031a8b58adbb: every limits/budgets/circuits/approval field.
    let expected = snapshot(b"r", &[("governance", complete_governance_bytes()?)])?;
    let rows = [(Raw::Atom("governance"), complete_governance())];
    assert_eq!(success(&Raw::Map(&rows))?, expected);
    for value in [Raw::Map(&[]), Raw::List(&[])] {
        assert_eq!(
            success(&Raw::Map(&[(Raw::Atom("governance"), value)]))?,
            snapshot(b"r", &[("governance", map(&[])?)])?
        );
    }
    for boolean in [false, true] {
        let text = if boolean { "true" } else { "false" };
        let governance = [
            (
                Raw::Atom("effects"),
                Raw::Map(&[(Raw::Atom("require_idempotency_key"), Raw::Atom(text))]),
            ),
            (
                Raw::Atom("audit"),
                Raw::Map(&[
                    (Raw::Atom("effect_events"), Raw::Atom(text)),
                    (Raw::Atom("denials"), Raw::Binary(b"SAMPLED")),
                ]),
            ),
        ];
        let expected = map(&[
            (
                "audit",
                map(&[
                    ("denials", atom("sampled")?),
                    ("effect_events", atom(text)?),
                ])?,
            ),
            ("effects", map(&[("require_idempotency_key", atom(text)?)])?),
        ])?;
        assert_eq!(
            success(&Raw::Map(&[(
                Raw::Atom("governance"),
                Raw::Map(&governance)
            )]))?,
            snapshot(b"r", &[("governance", expected)])?
        );
    }
    assert_optional_nil_groups()?;
    Ok(())
}

fn assert_optional_nil_groups() -> TestResult {
    for field in [
        "mode",
        "effects",
        "audit",
        "limits",
        "budgets",
        "circuits",
        "approvals",
    ] {
        for value in [Raw::Nil, Raw::Atom("nil")] {
            let group = [(Raw::Atom(field), value)];
            assert_eq!(
                success(&Raw::Map(&[(Raw::Atom("governance"), Raw::Map(&group))]))?,
                snapshot(b"r", &[("governance", map(&[])?)])?
            );
        }
    }
    for field in [
        "effects",
        "audit",
        "limits",
        "budgets",
        "circuits",
        "approvals",
    ] {
        let group = [(Raw::Atom(field), Raw::List(&[]))];
        assert_eq!(
            success(&Raw::Map(&[(Raw::Atom("governance"), Raw::Map(&group))]))?,
            snapshot(b"r", &[("governance", map(&[(field, map(&[])?)])?)])?
        );
    }
    Ok(())
}

#[test]
fn original_flow_input_preserves_governance_scalar_and_group_errors() -> TestResult {
    assert_shapes_and_enums()?;
    assert_boolean_classes()?;
    assert_rule_scalars()?;
    cases::assert_big_integers()?;
    cases::assert_group_order()?;
    cases::assert_rule_order()?;
    super::cases::assert_circuit_scalar_order()?;
    assert_circuit_order()?;
    assert_governance_shadowing()?;
    Ok(())
}

fn assert_boolean_classes() -> TestResult {
    for bad in [
        Raw::Integer(1),
        Raw::Binary(b"true"),
        Raw::Other,
        Raw::List(&[]),
    ] {
        let effects = [(Raw::Atom("require_idempotency_key"), bad)];
        gov_reason(
            Raw::Map(&[(Raw::Atom("effects"), Raw::Map(&effects))]),
            Gov::RequireIdempotencyBoolean,
            b"ERR flow governance require_idempotency_key must be boolean",
        )?;
        let audit = [(Raw::Atom("effect_events"), bad)];
        gov_reason(
            Raw::Map(&[(Raw::Atom("audit"), Raw::Map(&audit))]),
            Gov::EffectEventsBoolean,
            b"ERR flow governance effect_events must be boolean",
        )?;
    }
    Ok(())
}

fn assert_circuit_order() -> TestResult {
    for (minimum, threshold, rate, reason, body) in [
        (
            Some(65),
            65,
            None,
            Tracking::MinCallsAbove64,
            b"ERR flow governance circuit min_calls must be <= 64".as_slice(),
        ),
        (
            None,
            65,
            None,
            Tracking::ThresholdAbove64WithoutRate,
            b"ERR flow governance circuit failure_threshold must be <= 64 without failure_rate_pct"
                .as_slice(),
        ),
        (
            None,
            65,
            Some(50),
            Tracking::MissingMinCallsForLargeThreshold,
            b"ERR flow governance circuit min_calls is required when failure_threshold exceeds 64"
                .as_slice(),
        ),
    ] {
        let mut rows = vec![
            (Raw::Atom("failure_threshold"), Raw::Integer(threshold)),
            (Raw::Atom("open_ms"), Raw::Integer(30_000)),
        ];
        if let Some(value) = minimum {
            rows.push((Raw::Atom("min_calls"), Raw::Integer(value)));
        }
        if let Some(value) = rate {
            rows.push((Raw::Atom("failure_rate_pct"), Raw::Integer(value)));
        }
        assert_rule_error("circuits", &rows, Gov::Tracking(reason), body)?;
    }
    let bad = [
        (Raw::Atom("failure_threshold"), Raw::Integer(10)),
        (Raw::Atom("open_ms"), Raw::Integer(30_000)),
        (Raw::Atom("min_calls"), Raw::Integer(65)),
        (Raw::Atom("failure_rate_pct"), Raw::Integer(50)),
    ];
    assert_rule_error(
        "circuits",
        &bad,
        Gov::Tracking(Tracking::MinCallsAbove64),
        b"ERR flow governance circuit min_calls must be <= 64",
    )?;
    let rows = [
        (Raw::Atom("failure_threshold"), Raw::Integer(65)),
        (Raw::Atom("open_ms"), Raw::Integer(1)),
        (Raw::Atom("min_calls"), Raw::Integer(1)),
        (Raw::Atom("failure_rate_pct"), Raw::Integer(50)),
        (Raw::Atom("latency_threshold_ms"), Raw::Integer(0)),
        (Raw::Atom("error_classes"), Raw::Other),
    ];
    assert_rule_error(
        "circuits",
        &rows,
        Gov::Positive(Field::LatencyMillis),
        b"ERR flow governance latency_threshold_ms must be a positive integer",
    )?;
    let rows = [
        (Raw::Atom("failure_threshold"), Raw::Integer(65)),
        (Raw::Atom("open_ms"), Raw::Integer(1)),
        (Raw::Atom("failure_rate_pct"), Raw::Integer(50)),
        (Raw::Atom("error_classes"), Raw::Other),
        (Raw::Atom("half_open_max_probes"), Raw::Integer(0)),
    ];
    assert_rule_error(
        "circuits",
        &rows,
        Gov::StringList(Field::ErrorClasses),
        b"ERR flow governance error_classes must be a list of strings",
    )?;
    assert_tracking_neighbors()
}

fn assert_tracking_neighbors() -> TestResult {
    for threshold in [64, 65] {
        let rule = [
            (Raw::Atom("failure_threshold"), Raw::Integer(threshold)),
            (Raw::Atom("open_ms"), Raw::Integer(1)),
            (Raw::Atom("min_calls"), Raw::Integer(64)),
            (Raw::Atom("failure_rate_pct"), Raw::Integer(100)),
        ];
        let expected = map(&[
            ("failure_rate_pct", number(100)?),
            ("failure_threshold", number(u128::try_from(threshold)?)?),
            ("min_calls", number(64)?),
            ("open_ms", number(1)?),
        ])?;
        assert_eq!(
            success(&Raw::Map(&[(
                Raw::Atom("governance"),
                Raw::Map(&[(
                    Raw::Atom("circuits"),
                    Raw::Map(&[(Raw::Binary(b"scope"), Raw::Map(&rule))])
                )])
            )]))?,
            snapshot(
                b"r",
                &[(
                    "governance",
                    map(&[("circuits", named_map(&[(b"scope".to_vec(), expected)])?)])?
                )]
            )?
        );
    }
    Ok(())
}

fn assert_governance_shadowing() -> TestResult {
    for reversed in [false, true] {
        let mut fields = [
            (Raw::Atom("mode"), Raw::Binary(b"FULL")),
            (Raw::Binary(b"mode"), Raw::Other),
        ];
        if reversed {
            fields.reverse();
        }
        assert_eq!(
            success(&Raw::Map(&[(Raw::Atom("governance"), Raw::Map(&fields))]))?,
            snapshot(b"r", &[("governance", map(&[("mode", atom("full")?)])?)])?
        );
        let mut fields = [
            (Raw::Atom("mode"), Raw::Nil),
            (Raw::Binary(b"mode"), Raw::Binary(b"FULL")),
        ];
        if reversed {
            fields.reverse();
        }
        assert_eq!(
            success(&Raw::Map(&[(Raw::Atom("governance"), Raw::Map(&fields))]))?,
            snapshot(b"r", &[("governance", map(&[])?)])?
        );
    }
    Ok(())
}

#[test]
fn original_flow_input_preserves_governance_lists_and_rule_bounds() -> TestResult {
    let leaves = [
        Raw::Binary(b"email"),
        Raw::Binary(b"webhook"),
        Raw::Binary(b"email"),
        Raw::Binary(b"sms"),
        Raw::Binary(b"webhook"),
    ];
    let effects = [(Raw::Atom("allowed"), Raw::List(&leaves))];
    assert_eq!(
        success(&Raw::Map(&[(
            Raw::Atom("governance"),
            Raw::Map(&[(Raw::Atom("effects"), Raw::Map(&effects))])
        )]))?,
        snapshot(
            b"r",
            &[(
                "governance",
                map(&[(
                    "effects",
                    map(&[(
                        "allowed",
                        list(&[binary(b"email")?, binary(b"webhook")?, binary(b"sms")?])?
                    )])?
                )])?
            )]
        )?
    );
    assert_lists()?;
    assert_rule_bounds()?;
    super::cases::assert_rule_name_neighbor()?;
    Ok(())
}

fn assert_lists() -> TestResult {
    for (name, field) in [
        ("allowed", Field::Allowed),
        ("denied", Field::Denied),
        ("effects", Field::ApprovalEffects),
        ("states", Field::ApprovalStates),
        ("assignees", Field::Assignees),
        ("error_classes", Field::ErrorClasses),
    ] {
        for bad in [Raw::Other, Raw::Binary(b"x")] {
            list_reason(
                name,
                bad,
                Gov::StringList(field),
                format!("ERR flow governance {name} must be a list of strings").as_bytes(),
            )?;
        }
        for bad in [Raw::Nil, Raw::Integer(1), Raw::Binary(b"")] {
            list_reason(
                name,
                Raw::List(&[bad]),
                Gov::ListValue,
                b"ERR flow governance list values must be non-empty strings",
            )?;
        }
        let width = if name == "error_classes" { 256 } else { 65_535 };
        let too_wide = vec![b'x'; width + 1];
        list_reason(
            name,
            Raw::List(&[Raw::Binary(&too_wide)]),
            Gov::ListBytes(field),
            format!("ERR flow governance {name} values must be at most {width} bytes").as_bytes(),
        )?;
    }
    let originals = (1..=1_001)
        .map(|i| format!("effect-{i}"))
        .collect::<Vec<_>>();
    let leaves = originals
        .iter()
        .map(|s| Raw::Binary(s.as_bytes()))
        .collect::<Vec<_>>();
    list_reason(
        "allowed",
        Raw::List(&leaves),
        Gov::ListCount,
        b"ERR flow governance list must contain at most 1000 values",
    )?;
    assert_list_neighbors()?;
    let duplicates = vec![Raw::Binary(b"x"); 1_001];
    list_reason(
        "allowed",
        Raw::List(&duplicates),
        Gov::ListCount,
        b"ERR flow governance list must contain at most 1000 values",
    )?;
    let mut bad_last = duplicates.clone();
    bad_last[1_000] = Raw::Other;
    list_reason(
        "allowed",
        Raw::List(&bad_last),
        Gov::ListCount,
        b"ERR flow governance list must contain at most 1000 values",
    )?;
    Ok(())
}

fn list_reason(name: &str, value: Raw<'_>, reason: Gov, body: &[u8]) -> TestResult {
    let fields = [(Raw::Atom(name), value)];
    match name {
        "error_classes" => {
            let rule = [
                (Raw::Atom("failure_threshold"), Raw::Integer(1)),
                (Raw::Atom("open_ms"), Raw::Integer(1)),
                fields[0],
            ];
            assert_rule_error("circuits", &rule, reason, body)
        }
        "allowed" | "denied" => gov_reason(
            Raw::Map(&[(Raw::Atom("effects"), Raw::Map(&fields))]),
            reason,
            body,
        ),
        _ => gov_reason(
            Raw::Map(&[(Raw::Atom("approvals"), Raw::Map(&fields))]),
            reason,
            body,
        ),
    }
}

fn assert_rule_bounds() -> TestResult {
    let names = (1..=1_001)
        .map(|i| format!("scope-{i}"))
        .collect::<Vec<_>>();
    let one = [(Raw::Atom("limit"), Raw::Integer(1))];
    let rows = names
        .iter()
        .map(|name| (Raw::Binary(name.as_bytes()), Raw::Map(&one)))
        .collect::<Vec<_>>();
    gov_reason(
        Raw::Map(&[(Raw::Atom("limits"), Raw::Map(&rows))]),
        Gov::RuleCount(Kind::Limits),
        b"ERR flow governance limits policy must contain at most 1000 rules",
    )?;
    let name = vec![b's'; 65_536];
    let rows = [(
        Raw::Binary(&name),
        Raw::Map(&[(Raw::Atom("limit"), Raw::Integer(1))]),
    )];
    gov_reason(
        Raw::Map(&[(Raw::Atom("limits"), Raw::Map(&rows))]),
        Gov::RuleNameBytes(Kind::Limits),
        b"ERR flow governance limits names must be at most 65535 bytes",
    )?;
    for (group, kind) in [
        ("limits", Kind::Limits),
        ("budgets", Kind::Budgets),
        ("circuits", Kind::Circuits),
    ] {
        for name in [Raw::Binary(b""), Raw::Atom("scope"), Raw::Integer(0)] {
            let rows = [(name, Raw::Other)];
            gov_reason(
                Raw::Map(&[(Raw::Atom(group), Raw::Map(&rows))]),
                Gov::RuleName(kind),
                format!("ERR flow governance {group} names must be non-empty strings").as_bytes(),
            )?;
        }
    }
    let duplicate = [Raw::Atom("scope"), Raw::Other];
    let duplicates = vec![Raw::Tuple(&duplicate); 1_001];
    gov_reason(
        Raw::Map(&[(Raw::Atom("limits"), Raw::List(&duplicates))]),
        Gov::RuleName(Kind::Limits),
        b"ERR flow governance limits names must be non-empty strings",
    )?;
    Ok(())
}

fn assert_list_neighbors() -> TestResult {
    let duplicates = vec![Raw::Binary(b"x"); 1_000];
    let effects = [(Raw::Atom("allowed"), Raw::List(&duplicates))];
    assert_eq!(
        success(&Raw::Map(&[(
            Raw::Atom("governance"),
            Raw::Map(&[(Raw::Atom("effects"), Raw::Map(&effects))])
        )]))?,
        snapshot(
            b"r",
            &[(
                "governance",
                map(&[("effects", map(&[("allowed", list(&[binary(b"x")?])?)])?)])?
            )]
        )?
    );
    let tail = Raw::Nil;
    for prefix in [&duplicates[..1], &duplicates[..]] {
        list_reason(
            "allowed",
            Raw::ImproperList {
                prefix,
                tail: &tail,
            },
            Gov::ListValue,
            b"ERR flow governance list values must be non-empty strings",
        )?;
    }
    let opaque = vec![255; 256];
    let rule = [
        (Raw::Atom("failure_threshold"), Raw::Integer(1)),
        (Raw::Atom("open_ms"), Raw::Integer(1)),
        (
            Raw::Atom("error_classes"),
            Raw::List(&[Raw::Binary(&opaque)]),
        ),
    ];
    let expected = map(&[
        ("error_classes", list(&[binary(&opaque)?])?),
        ("failure_threshold", number(1)?),
        ("open_ms", number(1)?),
    ])?;
    assert_eq!(
        success(&Raw::Map(&[(
            Raw::Atom("governance"),
            Raw::Map(&[(
                Raw::Atom("circuits"),
                Raw::Map(&[(Raw::Binary(&[255, 0]), Raw::Map(&rule))])
            )])
        )]))?,
        snapshot(
            b"r",
            &[(
                "governance",
                map(&[("circuits", named_map(&[(vec![255, 0], expected)])?)])?
            )]
        )?
    );
    let unit = [
        (Raw::Atom("limit"), Raw::Integer(0)),
        (Raw::Atom("window_ms"), Raw::Integer(1)),
        (Raw::Atom("unit"), Raw::Binary(&[255, 0])),
    ];
    let expected = map(&[
        ("limit", number(0)?),
        ("unit", binary(&[255, 0])?),
        ("window_ms", number(1)?),
    ])?;
    assert_eq!(
        success(&Raw::Map(&[(
            Raw::Atom("governance"),
            Raw::Map(&[(
                Raw::Atom("budgets"),
                Raw::Map(&[(Raw::Binary(&[0, 255]), Raw::Map(&unit))])
            )])
        )]))?,
        snapshot(
            b"r",
            &[(
                "governance",
                map(&[("budgets", named_map(&[(vec![0, 255], expected)])?)])?
            )]
        )?
    );
    Ok(())
}

#[test]
fn original_flow_input_preserves_governance_late_size_priority() -> TestResult {
    let values = (1..=600)
        .map(|i| format!("{}{i}", "e".repeat(2_000)))
        .collect::<Vec<_>>();
    assert_eq!(values.iter().map(String::len).sum::<usize>(), 1_201_692);
    let leaves = values
        .iter()
        .map(|s| Raw::Binary(s.as_bytes()))
        .collect::<Vec<_>>();
    let effects = [(Raw::Atom("allowed"), Raw::List(&leaves))];
    let gov = [(Raw::Atom("effects"), Raw::Map(&effects))];
    let input = [
        (Raw::Atom("governance"), Raw::Map(&gov)),
        (Raw::Atom("states"), Raw::Other),
    ];
    let profile = limits(128 * 1_024 * 1_024)?;
    let mut charge = PolicySchemaCharge::new(profile);
    let actual = normalize(
        &Raw::Binary(b"r"),
        &Raw::Map(&input),
        17,
        profile,
        &mut charge,
    )?;
    assert_eq!(actual, Err(Reason::Governance(Gov::PolicyBytes)));
    assert_eq!(
        super::encode(Reason::Governance(Gov::PolicyBytes), profile, &mut charge)?,
        b"ERR flow governance policy exceeds 1048576-byte durable limit"
    );
    let approvals = [(Raw::Atom("timeout_ms"), Raw::Integer(0))];
    let gov = [
        (Raw::Atom("effects"), Raw::Map(&effects)),
        (Raw::Atom("approvals"), Raw::Map(&approvals)),
    ];
    gov_reason(
        Raw::Map(&gov),
        Gov::Positive(Field::ApprovalTimeout),
        b"ERR flow governance timeout_ms must be a positive integer",
    )?;
    Ok(())
}
