//! Scalar/rule fixtures and ordered governance branch controls.

use super::super::cases::gov_reason;
use super::super::{Gov, Raw, TestResult, map, named_map};
use super::super::{number, snapshot, success};
use super::{Field, Kind};

pub(super) fn assert_rule_scalars() -> TestResult {
    for (name, kind, body) in [
        (
            "limits",
            Kind::Limits,
            b"ERR flow governance limit rule must be a map or keyword list".as_slice(),
        ),
        (
            "budgets",
            Kind::Budgets,
            b"ERR flow governance budget rule must be a map or keyword list".as_slice(),
        ),
        (
            "circuits",
            Kind::Circuits,
            b"ERR flow governance circuit rule must be a map or keyword list".as_slice(),
        ),
    ] {
        let rules = [(Raw::Binary(b"scope"), Raw::Other)];
        gov_reason(
            Raw::Map(&[(Raw::Atom(name), Raw::Map(&rules))]),
            Gov::RuleShape(kind),
            body,
        )?;
    }
    for bad in [
        Raw::Nil,
        Raw::Atom("nil"),
        Raw::Integer(-1),
        Raw::Integer(9_007_199_254_740_992),
        Raw::Map(&[]),
        Raw::Other,
    ] {
        assert_rule_error(
            "limits",
            &[(Raw::Atom("limit"), bad)],
            Gov::Limit,
            b"ERR flow governance limit must be a non-negative integer",
        )?;
    }
    assert_positive_fields()?;
    for bad in [Raw::Integer(0), Raw::Integer(101), Raw::Binary(b"50")] {
        let rule = [
            (Raw::Atom("failure_threshold"), Raw::Integer(1)),
            (Raw::Atom("open_ms"), Raw::Integer(1)),
            (Raw::Atom("failure_rate_pct"), bad),
        ];
        assert_rule_error(
            "circuits",
            &rule,
            Gov::Percent,
            b"ERR flow governance failure_rate_pct must be an integer from 1 to 100",
        )?;
    }
    let too_wide = vec![b'u'; 65_536];
    for bad in [Raw::Binary(b""), Raw::Binary(&too_wide), Raw::Other] {
        let rule = [
            (Raw::Atom("limit"), Raw::Integer(1)),
            (Raw::Atom("window_ms"), Raw::Integer(1)),
            (Raw::Atom("unit"), bad),
        ];
        assert_rule_error(
            "budgets",
            &rule,
            Gov::Unit,
            b"ERR flow governance unit must be a non-empty string of at most 65535 bytes",
        )?;
    }
    Ok(())
}

fn assert_positive(name: &str, field: Field, bad: Raw<'_>, body: &[u8]) -> TestResult {
    if name == "timeout_ms" {
        return gov_reason(
            Raw::Map(&[(Raw::Atom("approvals"), Raw::Map(&[(Raw::Atom(name), bad)]))]),
            Gov::Positive(field),
            body,
        );
    }
    let group = if name == "lease_size" {
        "limits"
    } else if name == "window_ms" {
        "budgets"
    } else {
        "circuits"
    };
    let mut rule = if group == "circuits" {
        vec![
            (Raw::Atom("failure_threshold"), Raw::Integer(1)),
            (Raw::Atom("open_ms"), Raw::Integer(1)),
        ]
    } else {
        vec![(Raw::Atom("limit"), Raw::Integer(1))]
    };
    if let Some(row) = rule
        .iter_mut()
        .find(|row| matches!(row.0, Raw::Atom(key) if key == name))
    {
        row.1 = bad;
    } else {
        rule.push((Raw::Atom(name), bad));
    }
    assert_rule_error(group, &rule, Gov::Positive(field), body)
}

fn assert_positive_neighbor(name: &str) -> TestResult {
    let rows = [(Raw::Atom(name), Raw::Integer(1))];
    let group = if name == "timeout_ms" {
        map(&[("approvals", map(&[(name, number(1)?)])?)])?
    } else if name == "lease_size" {
        map(&[(
            "limits",
            named_map(&[(
                b"scope".to_vec(),
                map(&[("lease_size", number(1)?), ("limit", number(1)?)])?,
            )])?,
        )])?
    } else if name == "window_ms" {
        map(&[(
            "budgets",
            named_map(&[(
                b"scope".to_vec(),
                map(&[("limit", number(1)?), ("window_ms", number(1)?)])?,
            )])?,
        )])?
    } else {
        let mut fields = vec![("failure_threshold", number(1)?), ("open_ms", number(1)?)];
        if let Some(field) = fields.iter_mut().find(|field| field.0 == name) {
            field.1 = number(1)?;
        } else {
            fields.push((name, number(1)?));
        }
        fields.sort_by_key(|field| field.0);
        map(&[(
            "circuits",
            named_map(&[(b"scope".to_vec(), map(&fields)?)])?,
        )])?
    };
    let mut rule = vec![
        (Raw::Atom("failure_threshold"), Raw::Integer(1)),
        (Raw::Atom("open_ms"), Raw::Integer(1)),
    ];
    if !matches!(name, "failure_threshold" | "open_ms") {
        rule.push((Raw::Atom(name), Raw::Integer(1)));
    }
    let limit = [(Raw::Atom("limit"), Raw::Integer(1)), rows[0]];
    let approvals = [(Raw::Atom("approvals"), Raw::Map(&rows))];
    let limit_rules = [(Raw::Binary(b"scope"), Raw::Map(&limit))];
    let limits = [(Raw::Atom("limits"), Raw::Map(&limit_rules))];
    let budgets = [(Raw::Atom("budgets"), Raw::Map(&limit_rules))];
    let circuit_rules = [(Raw::Binary(b"scope"), Raw::Map(&rule))];
    let circuits = [(Raw::Atom("circuits"), Raw::Map(&circuit_rules))];
    let input = Raw::Map(match name {
        "timeout_ms" => &approvals,
        "lease_size" => &limits,
        "window_ms" => &budgets,
        _ => &circuits,
    });
    assert_eq!(
        success(&Raw::Map(&[(Raw::Atom("governance"), input)]))?,
        snapshot(b"r", &[("governance", group)])?
    );
    Ok(())
}

pub(super) fn assert_rule_error(
    group: &str,
    rule: &[(Raw<'_>, Raw<'_>)],
    reason: Gov,
    body: &[u8],
) -> TestResult {
    let rules = [(Raw::Binary(b"scope"), Raw::Map(rule))];
    gov_reason(
        Raw::Map(&[(Raw::Atom(group), Raw::Map(&rules))]),
        reason,
        body,
    )
}

pub(super) fn assert_shapes_and_enums() -> TestResult {
    gov_reason(
        Raw::Other,
        Gov::Shape,
        b"ERR flow governance policy must be a map or keyword list",
    )?;
    for (name, reason, body) in [
        (
            "effects",
            Gov::EffectsShape,
            b"ERR flow governance effects policy must be a map or keyword list".as_slice(),
        ),
        (
            "audit",
            Gov::AuditShape,
            b"ERR flow governance audit policy must be a map or keyword list".as_slice(),
        ),
        (
            "approvals",
            Gov::ApprovalsShape,
            b"ERR flow governance approvals policy must be a map or keyword list".as_slice(),
        ),
        (
            "limits",
            Gov::RuleMapShape(Kind::Limits),
            b"ERR flow governance limits policy must be a map or keyword list".as_slice(),
        ),
        (
            "budgets",
            Gov::RuleMapShape(Kind::Budgets),
            b"ERR flow governance budgets policy must be a map or keyword list".as_slice(),
        ),
        (
            "circuits",
            Gov::RuleMapShape(Kind::Circuits),
            b"ERR flow governance circuits policy must be a map or keyword list".as_slice(),
        ),
    ] {
        for bad in [Raw::Other, Raw::Integer(1), Raw::List(&[Raw::Atom("bad")])] {
            gov_reason(Raw::Map(&[(Raw::Atom(name), bad)]), reason, body)?;
        }
    }
    for bad in [Raw::Atom("unknown"), Raw::Integer(42), Raw::Map(&[])] {
        gov_reason(
            Raw::Map(&[(Raw::Atom("mode"), bad)]),
            Gov::Mode,
            b"ERR invalid flow governance mode",
        )?;
        let audit = [(Raw::Atom("denials"), bad)];
        gov_reason(
            Raw::Map(&[(Raw::Atom("audit"), Raw::Map(&audit))]),
            Gov::Denials,
            b"ERR invalid flow governance denials audit mode",
        )?;
    }
    for bad in [
        Raw::Atom("strict_local"),
        Raw::Atom("shard_local"),
        Raw::Atom("async_audit"),
        Raw::Binary(b"strict_local"),
    ] {
        let rule = [
            (Raw::Atom("limit"), Raw::Integer(10)),
            (Raw::Atom("enforcement"), bad),
        ];
        assert_rule_error(
            "limits",
            &rule,
            Gov::Enforcement,
            b"ERR invalid flow governance enforcement mode",
        )?;
    }
    super::super::cases::assert_governance_enum_neighbors()
}

pub(super) fn assert_big_integers() -> TestResult {
    let one = Raw::BigInteger {
        negative: false,
        little_endian: &[1, 0, 0],
    };
    let zero = Raw::BigInteger {
        negative: true,
        little_endian: &[0, 0],
    };
    let hundred = Raw::BigInteger {
        negative: false,
        little_endian: &[100, 0],
    };
    let limit = [(Raw::Atom("limit"), zero), (Raw::Atom("lease_size"), one)];
    let budget = [(Raw::Atom("limit"), one), (Raw::Atom("window_ms"), one)];
    let circuit = [
        (Raw::Atom("failure_threshold"), one),
        (Raw::Atom("open_ms"), one),
        (Raw::Atom("failure_rate_pct"), hundred),
    ];
    let input = [
        (
            Raw::Atom("limits"),
            Raw::Map(&[(Raw::Binary(b"scope"), Raw::Map(&limit))]),
        ),
        (
            Raw::Atom("budgets"),
            Raw::Map(&[(Raw::Binary(b"scope"), Raw::Map(&budget))]),
        ),
        (
            Raw::Atom("circuits"),
            Raw::Map(&[(Raw::Binary(b"scope"), Raw::Map(&circuit))]),
        ),
        (
            Raw::Atom("approvals"),
            Raw::Map(&[(Raw::Atom("timeout_ms"), one)]),
        ),
    ];
    let expected = map(&[
        ("approvals", map(&[("timeout_ms", number(1)?)])?),
        (
            "budgets",
            named_map(&[(
                b"scope".to_vec(),
                map(&[("limit", number(1)?), ("window_ms", number(1)?)])?,
            )])?,
        ),
        (
            "circuits",
            named_map(&[(
                b"scope".to_vec(),
                map(&[
                    ("failure_rate_pct", number(100)?),
                    ("failure_threshold", number(1)?),
                    ("open_ms", number(1)?),
                ])?,
            )])?,
        ),
        (
            "limits",
            named_map(&[(
                b"scope".to_vec(),
                map(&[("lease_size", number(1)?), ("limit", number(0)?)])?,
            )])?,
        ),
    ])?;
    assert_eq!(
        success(&Raw::Map(&[(Raw::Atom("governance"), Raw::Map(&input))]))?,
        snapshot(b"r", &[("governance", expected)])?
    );
    assert_big_integer_errors(zero)
}

pub(super) fn assert_group_order() -> TestResult {
    assert_effect_audit_order()?;
    let invalid = [
        (
            "mode",
            Gov::Mode,
            b"ERR invalid flow governance mode".as_slice(),
        ),
        (
            "effects",
            Gov::EffectsShape,
            b"ERR flow governance effects policy must be a map or keyword list",
        ),
        (
            "audit",
            Gov::AuditShape,
            b"ERR flow governance audit policy must be a map or keyword list",
        ),
        (
            "limits",
            Gov::RuleMapShape(Kind::Limits),
            b"ERR flow governance limits policy must be a map or keyword list",
        ),
        (
            "budgets",
            Gov::RuleMapShape(Kind::Budgets),
            b"ERR flow governance budgets policy must be a map or keyword list",
        ),
        (
            "circuits",
            Gov::RuleMapShape(Kind::Circuits),
            b"ERR flow governance circuits policy must be a map or keyword list",
        ),
        (
            "approvals",
            Gov::ApprovalsShape,
            b"ERR flow governance approvals policy must be a map or keyword list",
        ),
    ];
    for first in 0..invalid.len() {
        let fields = invalid[first..]
            .iter()
            .map(|(name, _, _)| (Raw::Atom(name), Raw::Other))
            .collect::<Vec<_>>();
        gov_reason(Raw::Map(&fields), invalid[first].1, invalid[first].2)?;
    }
    let invalid = [
        ("effects", Field::ApprovalEffects),
        ("states", Field::ApprovalStates),
        ("assignees", Field::Assignees),
    ];
    for first in 0..invalid.len() {
        let mut fields = invalid[first..]
            .iter()
            .map(|(name, _)| (Raw::Atom(name), Raw::Other))
            .collect::<Vec<_>>();
        fields.push((Raw::Atom("timeout_ms"), Raw::Integer(0)));
        let body = format!(
            "ERR flow governance {} must be a list of strings",
            invalid[first].0
        );
        gov_reason(
            Raw::Map(&[(Raw::Atom("approvals"), Raw::Map(&fields))]),
            Gov::StringList(invalid[first].1),
            body.as_bytes(),
        )?;
    }
    Ok(())
}

fn assert_effect_audit_order() -> TestResult {
    for first in ["allowed", "denied"] {
        let mut effects = vec![
            (Raw::Atom(first), Raw::Other),
            (Raw::Atom("require_idempotency_key"), Raw::Other),
        ];
        if first == "allowed" {
            effects.push((Raw::Atom("denied"), Raw::Other));
        }
        let field = if first == "allowed" {
            Field::Allowed
        } else {
            Field::Denied
        };
        let body = format!("ERR flow governance {first} must be a list of strings");
        gov_reason(
            Raw::Map(&[(Raw::Atom("effects"), Raw::Map(&effects))]),
            Gov::StringList(field),
            body.as_bytes(),
        )?;
    }
    let audit = [
        (Raw::Atom("effect_events"), Raw::Other),
        (Raw::Atom("denials"), Raw::Other),
    ];
    gov_reason(
        Raw::Map(&[(Raw::Atom("audit"), Raw::Map(&audit))]),
        Gov::EffectEventsBoolean,
        b"ERR flow governance effect_events must be boolean",
    )
}

pub(super) fn assert_rule_order() -> TestResult {
    for group in ["limits", "budgets"] {
        let fields = [
            (
                "limit",
                Gov::Limit,
                b"ERR flow governance limit must be a non-negative integer".as_slice(),
            ),
            (
                "window_ms",
                Gov::Positive(Field::WindowMillis),
                b"ERR flow governance window_ms must be a positive integer",
            ),
            (
                "enforcement",
                Gov::Enforcement,
                b"ERR invalid flow governance enforcement mode",
            ),
            (
                "lease_size",
                Gov::Positive(Field::LeaseSize),
                b"ERR flow governance lease_size must be a positive integer",
            ),
            (
                "unit",
                Gov::Unit,
                b"ERR flow governance unit must be a non-empty string of at most 65535 bytes",
            ),
        ];
        let selected = fields
            .iter()
            .filter(|(name, _, _)| group == "budgets" || !matches!(*name, "window_ms" | "unit"))
            .collect::<Vec<_>>();
        for first in 0..selected.len() {
            let mut rows = vec![
                (Raw::Atom("limit"), Raw::Integer(1)),
                (Raw::Atom("window_ms"), Raw::Integer(1)),
            ];
            for (name, _, _) in &selected[first..] {
                invalidate_rule_field(&mut rows, name);
            }
            assert_rule_error(group, &rows, selected[first].1, selected[first].2)?;
        }
    }
    Ok(())
}

fn assert_positive_fields() -> TestResult {
    for (name, field, label) in [
        ("lease_size", Field::LeaseSize, "lease_size"),
        ("window_ms", Field::WindowMillis, "window_ms"),
        (
            "failure_threshold",
            Field::FailureThreshold,
            "failure_threshold",
        ),
        ("open_ms", Field::OpenMillis, "open_ms"),
        ("min_calls", Field::MinCalls, "min_calls"),
        (
            "latency_threshold_ms",
            Field::LatencyMillis,
            "latency_threshold_ms",
        ),
        (
            "half_open_max_probes",
            Field::HalfOpenProbes,
            "half_open_max_probes",
        ),
        (
            "half_open_success_threshold",
            Field::HalfOpenSuccesses,
            "half_open_success_threshold",
        ),
        ("timeout_ms", Field::ApprovalTimeout, "timeout_ms"),
    ] {
        for value in [0, -1, 9_007_199_254_740_992] {
            let body = format!("ERR flow governance {label} must be a positive integer");
            assert_positive(name, field, Raw::Integer(value), body.as_bytes())?;
        }
        assert_positive_neighbor(name)?;
    }
    Ok(())
}

fn assert_big_integer_errors(zero: Raw<'_>) -> TestResult {
    let one = Raw::BigInteger {
        negative: false,
        little_endian: &[1, 0, 0],
    };
    for bad in [
        Raw::BigInteger {
            negative: true,
            little_endian: &[1],
        },
        Raw::BigInteger {
            negative: false,
            little_endian: &[0, 0, 0, 0, 0, 0, 0, 0, 1],
        },
    ] {
        assert_rule_error(
            "limits",
            &[(Raw::Atom("limit"), bad)],
            Gov::Limit,
            b"ERR flow governance limit must be a non-negative integer",
        )?;
        assert_positive(
            "open_ms",
            Field::OpenMillis,
            bad,
            b"ERR flow governance open_ms must be a positive integer",
        )?;
        let rule = [
            (Raw::Atom("failure_threshold"), one),
            (Raw::Atom("open_ms"), one),
            (Raw::Atom("failure_rate_pct"), bad),
        ];
        assert_rule_error(
            "circuits",
            &rule,
            Gov::Percent,
            b"ERR flow governance failure_rate_pct must be an integer from 1 to 100",
        )?;
    }
    assert_positive(
        "open_ms",
        Field::OpenMillis,
        zero,
        b"ERR flow governance open_ms must be a positive integer",
    )?;
    Ok(())
}

fn invalidate_rule_field<'a>(rows: &mut Vec<(Raw<'a>, Raw<'a>)>, name: &'a str) {
    if let Some(row) = rows
        .iter_mut()
        .find(|row| matches!(row.0, Raw::Atom(key) if key == name))
    {
        row.1 = Raw::Other;
    } else {
        rows.push((Raw::Atom(name), Raw::Other));
    }
}
