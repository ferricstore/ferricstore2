use super::*;

fn captured(operation: &str) -> Vec<(&'static str, MutationFieldValue<'static>)> {
    use MutationFieldValue::{Bytes, Integer};
    match operation {
        "create" => vec![("type", Bytes(b"r"))],
        "transition" => vec![
            ("from", Bytes(b"q")),
            ("to", Bytes(b"r")),
            ("fencing_token", Integer(1)),
            ("lease_token", Bytes(b"l")),
        ],
        "complete" | "fail" | "retry" | "extend_lease" => {
            vec![("fencing_token", Integer(1)), ("lease_token", Bytes(b"l"))]
        }
        _ => vec![("fencing_token", Integer(1))],
    }
}

fn arguments<'a>(
    operation: &str,
    partition: &'a [(&'a str, IntentValue<'a>)],
) -> Vec<(&'a str, IntentValue<'a>)> {
    use IntentValue::{Bytes, Map, Unsigned};
    let mut args = vec![("workflow_id", Bytes(b"i")), ("partition", Map(partition))];
    args.extend(match operation {
        "create" => vec![
            ("workflow_type", Bytes(b"r")),
            ("initial_state", Bytes(b"queued")),
            ("priority", Unsigned(0)),
        ],
        "transition" => vec![
            ("from_state", Bytes(b"q")),
            ("to_state", Bytes(b"r")),
            ("fencing_token", Unsigned(1)),
            ("lease_token", Bytes(b"l")),
        ],
        "extend_lease" => vec![
            ("fencing_token", Unsigned(1)),
            ("lease_token", Bytes(b"l")),
            ("lease_ms", Unsigned(30_000)),
        ],
        "cancel" => vec![("fencing_token", Unsigned(1))],
        _ => vec![("fencing_token", Unsigned(1)), ("lease_token", Bytes(b"l"))],
    });
    args
}

pub(super) fn generic_vectors() -> TestResult {
    let scope = scope()?;
    let partition = [
        ("mode", IntentValue::String("only")),
        ("key", IntentValue::Bytes(b"p")),
    ];
    for (operation, golden) in [
        (
            "create",
            "c7b61cef117cb2ed893f132ebd9430a96433285504a319eff2ae2cf21c71bf3d",
        ),
        (
            "transition",
            "823764ea25530a514783c34dd2dd8d5d090c0781fb2f2315d690d34d15cb0941",
        ),
        (
            "complete",
            "ad27fb04f2617b292c86a42ad3e31ef6346e62938f4586d81588f1c76ad58466",
        ),
        (
            "fail",
            "fdc477f785df08ff0c707c61e103365e6af87d0f6199a2767739287ea7c15017",
        ),
        (
            "retry",
            "2d10f8a994cf6d657c20ca699f37757254697c2ed633687f282fc2db415c29bd",
        ),
        (
            "extend_lease",
            "ce791aad1fbb8bfbafc568ac9975bedded0790c0f4ed6fc1aebeb92b9955fd8a",
        ),
        (
            "cancel",
            "61e854d9fc33e99f96d2156c6447dc5f7b55988bdbf2e69f86da32c419b7bd57",
        ),
    ] {
        let fields = captured(operation);
        let caller = normalize_core_mutation(
            MutationInput {
                operation,
                id: b"i",
                fields: &fields,
            },
            &[],
            Some(b"p"),
            AdmissionLimits::default(),
            16 * MIB,
        )?;
        let args = arguments(operation, &partition);
        let name = format!("workflow.{operation}");
        let reference = prepare_intent(
            IntentInput {
                tenant: scope.tenant(),
                operation: &name,
                arguments: &args,
            },
            IntentLimits::new(2 * MIB, 128, 8)?,
        )?;
        let actual = prepare_public_mutation_intent(scope.tenant(), &caller, 16 * MIB + 8_192)?;
        assert_eq!(actual, reference);
        assert_eq!(actual.digest().as_slice(), hex(golden)?);
    }
    Ok(())
}

#[test]
fn terminal_many_normalizes_all_before_effects() -> TestResult {
    let scope = scope()?;
    let x = RequestId::new(b"x".to_vec())?;
    let y = RequestId::new(b"y".to_vec())?;
    let valid = fields();
    let bad = [
        ("lease_token", MutationFieldValue::Bytes(b"")),
        ("fencing_token", MutationFieldValue::Integer(1)),
    ];
    let items = [item(&scope, &x, b"i", &valid), item(&scope, &y, b"j", &bad)];
    let mut port = probe();
    assert!(
        submit_independent_mutations(
            many(&items, MutationReplyMode::Items),
            &mut port,
            MutationCallLimits::default()
        )
        .is_err()
    );
    assert!(port.events.is_empty());
    assert!(port.frames.is_empty());
    assert!(matches!(
        submit_independent_mutations(
            many(&[], MutationReplyMode::Items),
            &mut port,
            MutationCallLimits::default()
        ),
        Err(MutationCallerError::EmptyMany)
    ));
    assert!(port.events.is_empty());
    unsupported_routes(&scope, &x)?;
    let rich = [
        ("lease_token", MutationFieldValue::Bytes(b"l")),
        ("fencing_token", MutationFieldValue::Integer(1)),
        ("result", MutationFieldValue::Unsupported),
    ];
    let items = [
        item(&scope, &x, b"i", &valid),
        item(&scope, &y, b"j", &rich),
    ];
    assert!(
        submit_independent_mutations(
            many(&items, MutationReplyMode::Items),
            &mut port,
            MutationCallLimits::default()
        )
        .is_err()
    );
    assert!(port.events.is_empty());
    assert_eq!(port.frames.len(), 0);
    Ok(())
}

fn unsupported_routes(scope: &RequestScope, request: &RequestId) -> TestResult {
    for operation in ["create", "transition", "extend_lease"] {
        let fields = captured(operation);
        let items = [MutationItemInput {
            scope,
            request,
            input: MutationInput {
                operation,
                id: b"i",
                fields: &fields,
            },
        }];
        let mut port = probe();
        let reply = submit_independent_mutations(
            IndependentMutationCall::TerminalPipeline { items: &items },
            &mut port,
            MutationCallLimits::default(),
        )?;
        assert_eq!(reply.items()[0].intent(), None);
        assert!(matches!(
            reply.items()[0].status(),
            MutationItemStatus::LocalError(MutationCallerError::UnsupportedFeature(_))
        ));
        assert!(port.events.is_empty());
        let call = IndependentMutationCall::TerminalMany {
            operation,
            base_partition: Some(b"p"),
            common: &[],
            items: &items,
            reply: MutationReplyMode::Items,
        };
        assert!(
            submit_independent_mutations(call, &mut port, MutationCallLimits::default()).is_err()
        );
        assert!(port.events.is_empty());
    }
    let fields = fields();
    let items = [MutationItemInput {
        scope,
        request,
        input: MutationInput {
            operation: "fail",
            id: b"i",
            fields: &fields,
        },
    }];
    let mut port = probe();
    assert!(
        submit_independent_mutations(
            many(&items, MutationReplyMode::Items),
            &mut port,
            MutationCallLimits::default()
        )
        .is_err()
    );
    assert!(port.events.is_empty());
    Ok(())
}

fn limits_precede_effects() -> TestResult {
    let scope = scope()?;
    let x = RequestId::new(b"x".to_vec())?;
    let y = RequestId::new(b"y".to_vec())?;
    let fields = fields();
    let items = [
        item(&scope, &x, b"i", &fields),
        item(&scope, &y, b"j", &fields),
    ];
    for limits in [
        MutationCallLimits {
            maximum_items: 1,
            ..MutationCallLimits::default()
        },
        MutationCallLimits {
            maximum_result_bytes: 1_031,
            ..MutationCallLimits::default()
        },
        MutationCallLimits {
            workspace_bytes: 2_055,
            ..MutationCallLimits::default()
        },
        MutationCallLimits {
            workspace_bytes: 268_437_511,
            ..MutationCallLimits::default()
        },
        MutationCallLimits {
            maximum_raw_bytes: 123,
            ..MutationCallLimits::default()
        },
    ] {
        let mut port = probe();
        assert!(
            submit_independent_mutations(many(&items, MutationReplyMode::Items), &mut port, limits)
                .is_err()
        );
        assert!(port.events.is_empty());
        assert!(port.frames.is_empty());
    }
    let mut port = probe();
    let limits = MutationCallLimits {
        maximum_raw_bytes: 124,
        maximum_result_bytes: 1_032,
        workspace_bytes: 268_437_512,
        ..MutationCallLimits::default()
    };
    let reply =
        submit_independent_mutations(many(&items, MutationReplyMode::Items), &mut port, limits)?;
    assert_eq!(reply.items().len(), 2);
    assert_eq!(port.events, ["prepare", "submit", "prepare", "submit"]);
    assert_eq!(port.frames[0].len(), 117);
    assert_eq!(port.frames[1].len(), 117);
    Ok(())
}

fn repeated_common_and_maximum_inline() -> TestResult {
    let scope = scope()?;
    let x = RequestId::new(b"x".to_vec())?;
    let y = RequestId::new(b"y".to_vec())?;
    let fields = fields();
    let items = [
        item(&scope, &x, b"i", &fields),
        item(&scope, &y, b"j", &fields),
    ];
    let payload = vec![0; 256];
    let common = [("payload", MutationFieldValue::Bytes(&payload))];
    for (maximum, succeeds) in [(651, false), (652, true)] {
        let mut port = probe();
        let call = IndependentMutationCall::TerminalMany {
            operation: "complete",
            base_partition: Some(b"p"),
            common: &common,
            items: &items,
            reply: MutationReplyMode::Items,
        };
        let result = submit_independent_mutations(
            call,
            &mut port,
            MutationCallLimits {
                maximum_raw_bytes: maximum,
                ..MutationCallLimits::default()
            },
        );
        assert_eq!(result.is_ok(), succeeds);
        assert_eq!(port.frames.len(), if succeeds { 2 } else { 0 });
        if !succeeds {
            assert!(port.events.is_empty());
        }
    }
    let payload = vec![b'v'; MIB];
    let common = [("payload", MutationFieldValue::Bytes(&payload))];
    let mut port = probe();
    let call = IndependentMutationCall::TerminalMany {
        operation: "complete",
        base_partition: Some(b"p"),
        common: &common,
        items: &items[..1],
        reply: MutationReplyMode::Items,
    };
    let reply = submit_independent_mutations(call, &mut port, MutationCallLimits::default())?;
    assert!(matches!(
        reply.items()[0].status(),
        MutationItemStatus::Success { .. }
    ));
    assert_eq!(port.frames[0].len(), MIB + 121);
    let caller = normalize_core_mutation(
        MutationInput {
            operation: "complete",
            id: b"i",
            fields: &fields,
        },
        &common,
        Some(b"p"),
        AdmissionLimits::default(),
        16 * MIB,
    )?;
    assert_eq!(
        prepare_public_mutation_intent(scope.tenant(), &caller, 16 * MIB + 8_192)?
            .digest()
            .as_slice(),
        hex("15087a2fcdaa8428b0e2aa4e5d6ab99ee663dde4b28244634a0507d21108072f")?
    );
    Ok(())
}

#[test]
fn ingress_whole_call_limits_precede_clock_and_submit() -> TestResult {
    limits_precede_effects()?;
    oversized_raw_options_refuse_before_effects()?;
    repeated_common_and_maximum_inline()
}

fn oversized_raw_options_refuse_before_effects() -> TestResult {
    let scope = scope()?;
    let x = RequestId::new(b"x".to_vec())?;
    let y = RequestId::new(b"y".to_vec())?;
    let fields = fields();
    let over = [("unknown", MutationFieldValue::Unsupported); 9];
    let items = [
        item(&scope, &x, b"i", &fields),
        item(&scope, &y, b"j", &over),
    ];
    let mut port = probe();
    assert!(matches!(
        submit_independent_mutations(
            IndependentMutationCall::TerminalPipeline { items: &items },
            &mut port,
            MutationCallLimits::default()
        ),
        Err(MutationCallerError::InvalidOptions)
    ));
    assert!(port.events.is_empty());
    assert!(port.frames.is_empty());
    Ok(())
}
