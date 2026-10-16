//! Scalar independent `CreateMany`; physical publication is tested by the native oracle.

use super::*;
use wf_domain::workflow::Operation;
use wf_ports::consensus::PreparedOperation;

fn create<'a>(
    scope: &'a RequestScope,
    request: &'a RequestId,
    id: &'a [u8],
    fields: &'a [(&'a str, MutationFieldValue<'a>)],
) -> MutationItemInput<'a> {
    MutationItemInput {
        scope,
        request,
        input: MutationInput {
            operation: "create",
            id,
            fields,
        },
    }
}

fn common() -> [(&'static str, MutationFieldValue<'static>); 2] {
    [
        ("type", MutationFieldValue::Bytes(b"r")),
        ("run_at_ms", MutationFieldValue::Integer(1_000)),
    ]
}

fn call<'a>(
    items: &'a [MutationItemInput<'a>],
    common: &'a [(&'a str, MutationFieldValue<'a>)],
    base_partition: Option<&'a [u8]>,
    reply: MutationReplyMode,
) -> IndependentMutationCall<'a> {
    IndependentMutationCall::CreateMany {
        base_partition,
        common,
        items,
        reply,
    }
}

fn refuses(
    call: IndependentMutationCall<'_>,
    limits: MutationCallLimits,
    expected: MutationCallerError,
) {
    let mut port = probe();
    assert_eq!(
        submit_independent_mutations(call, &mut port, limits),
        Err(expected)
    );
    assert_eq!(port.events, [] as [&str; 0]);
    assert_eq!(port.frames, [] as [Vec<u8>; 0]);
}

fn first_errors() -> TestResult {
    let scope = scope()?;
    let ids = [
        RequestId::new(b"x".to_vec())?,
        RequestId::new(b"y".to_vec())?,
    ];
    let common = common();
    refuses(
        call(&[], &common, Some(b"p"), MutationReplyMode::Items),
        MutationCallLimits::default(),
        MutationCallerError::EmptyMany,
    );
    for (field, value, error) in [
        (
            "state",
            MutationFieldValue::Bytes(b""),
            MutationCallerError::InvalidField("state"),
        ),
        (
            "idempotent",
            MutationFieldValue::Bool(true),
            MutationCallerError::UnsupportedFeature("idempotent"),
        ),
        (
            "partition_key",
            MutationFieldValue::Bytes(b"other"),
            MutationCallerError::PartitionMismatch,
        ),
        (
            "partition_key",
            MutationFieldValue::Nil,
            MutationCallerError::InvalidField("partition_key"),
        ),
    ] {
        let bad = [(field, value)];
        let items = [
            create(&scope, &ids[0], b"i", &[]),
            create(&scope, &ids[1], b"j", &bad),
        ];
        refuses(
            call(&items, &common, Some(b"p"), MutationReplyMode::Items),
            MutationCallLimits::default(),
            error,
        );
    }
    let mut items = [
        create(&scope, &ids[0], b"i", &[]),
        create(&scope, &ids[1], b"j", &[]),
    ];
    items[1].input.operation = "complete";
    refuses(
        call(&items, &common, Some(b"p"), MutationReplyMode::Items),
        MutationCallLimits::default(),
        MutationCallerError::InvalidField("operation"),
    );
    let bad = [("state", MutationFieldValue::Bytes(b""))];
    let unsupported = [("idempotent", MutationFieldValue::Bool(true))];
    let items = [
        create(&scope, &ids[0], b"i", &bad),
        create(&scope, &ids[1], b"j", &unsupported),
    ];
    refuses(
        call(&items, &common, Some(b"p"), MutationReplyMode::Items),
        MutationCallLimits::default(),
        MutationCallerError::InvalidField("state"),
    );
    let nine = [("unknown", MutationFieldValue::Unsupported); 9];
    let items = [
        create(&scope, &ids[0], b"i", &bad),
        create(&scope, &ids[1], b"j", &nine),
    ];
    refuses(
        call(&items, &common, Some(b"p"), MutationReplyMode::Items),
        MutationCallLimits::default(),
        MutationCallerError::InvalidOptions,
    );
    Ok(())
}

fn defaults_and_overrides() -> TestResult {
    let scope = scope()?;
    let ids = [
        RequestId::new(b"x".to_vec())?,
        RequestId::new(b"y".to_vec())?,
    ];
    let common = common();
    let changed = [
        ("type", MutationFieldValue::Bytes(b"other")),
        ("state", MutationFieldValue::Bytes(b"custom")),
        ("priority", MutationFieldValue::Integer(2)),
        ("run_at_ms", MutationFieldValue::Integer(2_500)),
        ("payload", MutationFieldValue::Bytes(b"item")),
        ("partition_key", MutationFieldValue::Bytes(b"p")),
    ];
    let items = [
        create(&scope, &ids[0], b"i", &[]),
        create(&scope, &ids[1], b"j", &changed),
    ];
    let mut port = probe();
    let reply = submit_independent_mutations(
        call(&items, &common, Some(b"p"), MutationReplyMode::Items),
        &mut port,
        MutationCallLimits::default(),
    )?;
    assert_eq!(reply.items().len(), 2);
    assert!(!reply.is_scalar_ok());
    assert_eq!(port.frames.len(), 2);
    assert_eq!(port.frames[0].len(), 127);
    for (index, bytes) in port.frames.iter().enumerate() {
        let decoded = prepare_public_mutation(bytes, CodecLimits::default(), 256 * MIB)?;
        let PreparedOperation::PublicMutation { item, evidence } = &decoded.request().operation
        else {
            return Err("not public mutation".into());
        };
        assert!(!evidence.auto_partition);
        assert_eq!(item.partition.as_bytes(), b"p");
        assert_eq!(item.command.id.as_bytes(), [b"i", b"j"][index]);
        let Operation::Create(effect) = &item.command.operation else {
            return Err("not create".into());
        };
        assert_eq!(
            effect.workflow_type,
            [b"r".as_slice(), b"other".as_slice()][index]
        );
        assert_eq!(
            effect.state,
            [b"queued".as_slice(), b"custom".as_slice()][index]
        );
        assert_eq!(item.create_priority, [0, 2][index]);
        assert_eq!(
            effect.run_at.map(TimestampMillis::get),
            Some([1_000, 2_500][index])
        );
        assert_eq!(
            effect.payload.as_deref(),
            [None, Some(b"item".as_slice())][index]
        );
    }
    Ok(())
}

fn exact_limits() -> TestResult {
    let scope = scope()?;
    let ids = [
        RequestId::new(b"x".to_vec())?,
        RequestId::new(b"y".to_vec())?,
    ];
    let common = common();
    let items = [
        create(&scope, &ids[0], b"i", &[]),
        create(&scope, &ids[1], b"j", &[]),
    ];
    for (raw, output, workspace, succeeds) in [
        (96, 1_032, 268_437_512, true),
        (95, 1_032, 268_437_512, false),
        (96, 1_031, 268_437_512, false),
        (96, 1_032, 268_437_511, false),
    ] {
        let mut port = probe();
        let result = submit_independent_mutations(
            call(&items, &common, Some(b"p"), MutationReplyMode::Items),
            &mut port,
            MutationCallLimits {
                maximum_raw_bytes: raw,
                maximum_result_bytes: output,
                workspace_bytes: workspace,
                ..MutationCallLimits::default()
            },
        );
        assert_eq!(result.is_ok(), succeeds);
        assert_eq!(port.frames.len(), if succeeds { 2 } else { 0 });
        assert_eq!(port.events.len(), if succeeds { 4 } else { 0 });
        if !succeeds {
            assert_eq!(result, Err(MutationCallerError::RawBytes));
        }
    }
    refuses(
        call(&items, &common, Some(b"p"), MutationReplyMode::Items),
        MutationCallLimits {
            maximum_items: 1,
            ..MutationCallLimits::default()
        },
        MutationCallerError::ItemLimit,
    );
    Ok(())
}

fn common_and_late_limits() -> TestResult {
    let scope = scope()?;
    let ids = [
        RequestId::new(b"x".to_vec())?,
        RequestId::new(b"y".to_vec())?,
    ];
    let payload = [0; 256];
    let common = [
        common()[0],
        common()[1],
        ("payload", MutationFieldValue::Bytes(&payload)),
    ];
    let empty = [("payload", MutationFieldValue::Bytes(b""))];
    let explicit_state = [
        common[0],
        common[1],
        ("state", MutationFieldValue::Bytes(b"queued")),
    ];
    for (fields, options, exact) in [
        (&[][..], &explicit_state[..], 120),
        (&[][..], &common[..], 624),
        (&empty[..], &common[..], 640),
    ] {
        let items = [
            create(&scope, &ids[0], b"i", fields),
            create(&scope, &ids[1], b"j", fields),
        ];
        for (raw, succeeds) in [(exact - 1, false), (exact, true)] {
            let mut port = probe();
            let result = submit_independent_mutations(
                call(&items, options, Some(b"p"), MutationReplyMode::Items),
                &mut port,
                MutationCallLimits {
                    maximum_raw_bytes: raw,
                    ..MutationCallLimits::default()
                },
            );
            assert_eq!(result.is_ok(), succeeds);
            assert_eq!(port.events.len(), if succeeds { 4 } else { 0 });
            assert_eq!(port.frames.len(), if succeeds { 2 } else { 0 });
        }
    }
    let payload = [0; 64];
    let later = [("payload", MutationFieldValue::Bytes(&payload))];
    let items = [
        create(&scope, &ids[0], b"i", &[]),
        create(&scope, &ids[1], b"j", &later),
    ];
    refuses(
        call(&items, &common[..2], Some(b"p"), MutationReplyMode::Items),
        MutationCallLimits {
            codec: CodecLimits::new(AdmissionLimits::new(MIB, 150, 1_000)?),
            ..MutationCallLimits::default()
        },
        MutationCallerError::RawBytes,
    );
    Ok(())
}

#[test]
fn create_many_normalizes_all_before_effects() -> TestResult {
    first_errors()?;
    defaults_and_overrides()?;
    exact_limits()?;
    common_and_late_limits()
}

fn scalar_and_owned_identity() -> TestResult {
    let scope = scope()?;
    let ids = [
        RequestId::new(b"x".to_vec())?,
        RequestId::new(b"y".to_vec())?,
    ];
    let common = common();
    let items = [
        create(&scope, &ids[0], b"i", &[]),
        create(&scope, &ids[1], b"i", &[]),
    ];
    let mut port = probe();
    let first = submit_independent_mutations(
        call(&items, &common, Some(b"p"), MutationReplyMode::Items),
        &mut port,
        MutationCallLimits::default(),
    )?;
    let mut port = probe();
    port.time = 2_001;
    let projected = submit_independent_mutations(
        call(&items, &common, Some(b"p"), MutationReplyMode::OkOnSuccess),
        &mut port,
        MutationCallLimits::default(),
    )?;
    assert!(!first.is_scalar_ok());
    assert!(projected.is_scalar_ok());
    assert_eq!(projected.items().len(), 2);
    assert_eq!(port.frames.len(), 2);
    for (index, request) in ids.iter().enumerate() {
        assert_eq!(
            first.items()[index].intent(),
            projected.items()[index].intent()
        );
        assert_eq!(projected.items()[index].request(), request);
        assert_eq!(projected.items()[index].scope(), &scope);
    }
    drop(scope);
    drop(ids);
    assert_eq!(projected.items()[0].request().as_bytes(), b"x");
    assert_eq!(projected.items()[1].request().as_bytes(), b"y");
    assert_eq!(projected.items()[0].scope().tenant().as_bytes(), b"t");
    Ok(())
}

fn mixed_and_unknown() -> TestResult {
    let scope = scope()?;
    let ids = [
        RequestId::new(b"x".to_vec())?,
        RequestId::new(b"y".to_vec())?,
        RequestId::new(b"z".to_vec())?,
    ];
    let common = common();
    let items = [
        create(&scope, &ids[0], b"i", &[]),
        create(&scope, &ids[1], b"j", &[]),
        create(&scope, &ids[2], b"k", &[]),
    ];
    let rejection = RetainedStatus::IndexedRejected(IndexRejection::Batch(BatchRejection::Domain(
        DomainError::AlreadyExists,
    )));
    let mut port = probe();
    port.replies = vec![(rejection, vec![2, 0, 1])];
    let result = submit_independent_mutations(
        call(&items, &common, Some(b"p"), MutationReplyMode::OkOnSuccess),
        &mut port,
        MutationCallLimits::default(),
    )?;
    assert_eq!(result.items().len(), 3);
    assert!(!result.is_scalar_ok());
    assert!(matches!(
        result.items()[0].status(),
        MutationItemStatus::Rejected {
            reason: IndexRejection::Batch(BatchRejection::Domain(DomainError::AlreadyExists)),
            ..
        }
    ));
    assert!(matches!(
        result.items()[1].status(),
        MutationItemStatus::Success { .. }
    ));
    assert!(matches!(
        result.items()[2].status(),
        MutationItemStatus::Success { .. }
    ));
    assert_eq!(port.frames.len(), 3);
    let mut port = probe();
    port.replies = vec![
        (rejection, vec![2, 0, 1]),
        (RetainedStatus::Accepted, vec![0, 255]),
    ];
    let result = submit_independent_mutations(
        call(&items, &common, Some(b"p"), MutationReplyMode::OkOnSuccess),
        &mut port,
        MutationCallLimits::default(),
    )?;
    assert_eq!(result.items().len(), 3);
    assert!(!result.is_scalar_ok());
    assert!(matches!(
        result.items()[1].status(),
        MutationItemStatus::Unknown(MutationUnknown::InvalidReply)
    ));
    assert!(matches!(
        result.items()[2].status(),
        MutationItemStatus::NotSubmitted(MutationNotSubmitted::PredecessorUnknown)
    ));
    assert_eq!(port.events, ["prepare", "submit", "prepare", "submit"]);
    assert_eq!(port.frames.len(), 2);
    Ok(())
}

fn old_families_stay_terminal() -> TestResult {
    let scope = scope()?;
    let request = RequestId::new(b"x".to_vec())?;
    let common = common();
    let items = [create(&scope, &request, b"i", &common)];
    refuses(
        IndependentMutationCall::TerminalMany {
            operation: "create",
            base_partition: Some(b"p"),
            common: &[],
            items: &items,
            reply: MutationReplyMode::Items,
        },
        MutationCallLimits::default(),
        MutationCallerError::UnsupportedFeature("create"),
    );
    let mut port = probe();
    let result = submit_independent_mutations(
        IndependentMutationCall::TerminalPipeline { items: &items },
        &mut port,
        MutationCallLimits::default(),
    )?;
    assert_eq!(result.items().len(), 1);
    assert_eq!(
        *result.items()[0].status(),
        MutationItemStatus::LocalError(MutationCallerError::UnsupportedFeature("create"))
    );
    assert_eq!(port.events, [] as [&str; 0]);
    Ok(())
}

#[test]
fn create_many_preserves_slots_and_projection() -> TestResult {
    scalar_and_owned_identity()?;
    mixed_and_unknown()?;
    old_families_stay_terminal()
}
