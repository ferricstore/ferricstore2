//! Pure comparison regressions; these do not establish application authority.

#![forbid(unsafe_code)]

use wf_application::commands::{IntentInput, IntentLimits, IntentValue, prepare_intent};
use wf_application::error::PreparationError;
use wf_types::identity::TenantId;

type TestResult = Result<(), Box<dyn std::error::Error>>;

fn hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    bytes
        .iter()
        .flat_map(|byte| {
            [
                char::from(DIGITS[usize::from(byte >> 4)]),
                char::from(DIGITS[usize::from(byte & 15)]),
            ]
        })
        .collect()
}

#[test]
fn independent_minimal_vector_and_exact_limits_include_fixed_root() -> TestResult {
    let tenant = TenantId::new(b"tenant".to_vec())?;
    let input = IntentInput {
        tenant: &tenant,
        operation: "create",
        arguments: &[],
    };
    let vector = include_str!("../../../tests/intent/v1-vectors.tsv")
        .lines()
        .find(|line| line.starts_with("empty\t"))
        .ok_or("missing empty vector")?;
    let fields: Vec<_> = vector.split('\t').collect();
    let bytes = fields[1].len() / 2;
    let nodes = fields[3].parse()?;
    let depth = fields[4].parse()?;
    let limits = IntentLimits::new(bytes, nodes, depth)?;
    let digest = prepare_intent(input, limits)?;
    assert_eq!(digest.version(), 1);
    assert_eq!(hex(digest.digest()), fields[2]);
    for (limits, error) in [
        (
            IntentLimits::new(bytes - 1, nodes, depth)?,
            PreparationError::ByteLimit,
        ),
        (
            IntentLimits::new(bytes, nodes - 1, depth)?,
            PreparationError::ValueLimit,
        ),
        (
            IntentLimits::new(bytes, nodes, depth - 1)?,
            PreparationError::DepthLimit,
        ),
    ] {
        assert_eq!(prepare_intent(input, limits), Err(error));
    }
    Ok(())
}

#[test]
fn unique_map_keys_and_finite_numbers_are_required_before_hashing() -> TestResult {
    let tenant = TenantId::new(b"tenant".to_vec())?;
    let duplicate = [("same", IntentValue::Null), ("same", IntentValue::True)];
    let input = IntentInput {
        tenant: &tenant,
        operation: "create",
        arguments: &duplicate,
    };
    assert_eq!(
        prepare_intent(input, IntentLimits::default()),
        Err(PreparationError::DuplicateKey)
    );
    for value in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        let args = [("number", IntentValue::Float(value))];
        assert_eq!(
            prepare_intent(
                IntentInput {
                    arguments: &args,
                    ..input
                },
                IntentLimits::default()
            ),
            Err(PreparationError::NonFiniteNumber)
        );
    }
    assert_eq!(
        prepare_intent(
            IntentInput {
                operation: "",
                arguments: &[],
                ..input
            },
            IntentLimits::default()
        ),
        Err(PreparationError::InvalidOperation)
    );
    Ok(())
}

#[test]
fn oversize_container_and_key_work_reject_before_sorting() -> TestResult {
    let tenant = TenantId::new(b"tenant".to_vec())?;
    let entries = vec![("", IntentValue::Null); 20_000];
    let input = IntentInput {
        tenant: &tenant,
        operation: "create",
        arguments: &entries,
    };
    assert_eq!(
        prepare_intent(input, IntentLimits::default()),
        Err(PreparationError::ValueLimit)
    );
    let key = "k".repeat(1_048_577);
    let args = [(key.as_str(), IntentValue::Null)];
    assert_eq!(
        prepare_intent(
            IntentInput {
                arguments: &args,
                ..input
            },
            IntentLimits::default()
        ),
        Err(PreparationError::ByteLimit)
    );
    for limits in [(0, 1, 1), (1, 0, 1), (1, 1, 0), (1, 1, 129)] {
        assert_eq!(
            IntentLimits::new(limits.0, limits.1, limits.2),
            Err(PreparationError::InvalidLimits)
        );
    }
    Ok(())
}

fn nested_lists(items: &[IntentValue<'_>], count: usize, visit: &mut dyn FnMut(IntentValue<'_>)) {
    if count == 0 {
        visit(IntentValue::List(items));
    } else {
        let next = [IntentValue::List(items)];
        nested_lists(&next, count - 1, visit);
    }
}

#[test]
fn configured_maximum_depth_is_bounded_and_counts_nested_containers() -> TestResult {
    let tenant = TenantId::new(b"tenant".to_vec())?;
    let limits = IntentLimits::new(8_192, 200, 128)?;
    let mut accepted = false;
    nested_lists(&[], 125, &mut |value| {
        let arguments = [("nested", value)];
        let input = IntentInput {
            tenant: &tenant,
            operation: "create",
            arguments: &arguments,
        };
        accepted = prepare_intent(input, limits).is_ok();
        assert_eq!(
            prepare_intent(input, IntentLimits::default()),
            Err(PreparationError::DepthLimit)
        );
    });
    assert!(accepted);
    nested_lists(&[], 126, &mut |value| {
        let arguments = [("nested", value)];
        assert_eq!(
            prepare_intent(
                IntentInput {
                    tenant: &tenant,
                    operation: "create",
                    arguments: &arguments
                },
                limits
            ),
            Err(PreparationError::DepthLimit)
        );
    });
    Ok(())
}
