use super::*;
use MutationFieldValue::{Bytes, Integer, Nil, Unsupported};

fn defaults_and_presence() -> TestResult {
    let fields = [("type", Bytes(b"r"))];
    let caller = normalize("create", b"i", &fields)?;
    assert_eq!(caller.id(), b"i");
    assert_eq!(caller.partition(), None);
    assert_eq!(caller.requested_now_ms(), None);
    let CoreMutation::Create {
        workflow_type,
        state,
        priority,
        run_at_ms,
        payload,
    } = caller.operation()
    else {
        return Err("not create".into());
    };
    assert_eq!(*workflow_type, b"r");
    assert_eq!(*state, b"queued");
    assert_eq!(*priority, 0);
    assert_eq!(*run_at_ms, None);
    assert_eq!(*payload, None);
    let fields = [
        ("type", Bytes(b"r")),
        ("now_ms", Integer(0)),
        ("payload", Bytes(b"")),
    ];
    let caller = normalize("create", b"i", &fields)?;
    assert_eq!(caller.requested_now_ms(), Some(0));
    assert!(matches!(
        caller.operation(),
        CoreMutation::Create {
            run_at_ms: Some(0),
            payload: Some(b""),
            ..
        }
    ));
    let fields = [
        ("from", Bytes(b"q")),
        ("to", Bytes(b"z")),
        ("fencing_token", Integer(0)),
        ("now_ms", Integer(7)),
    ];
    assert!(matches!(
        normalize("transition", b"i", &fields)?.operation(),
        CoreMutation::Transition {
            run_at_ms: Some(7),
            ..
        }
    ));
    let fields = [("lease_token", Bytes(b"l")), ("fencing_token", Integer(1))];
    assert!(matches!(
        normalize("extend_lease", b"i", &fields)?.operation(),
        CoreMutation::ExtendLease {
            lease_ms: 30_000,
            ..
        }
    ));
    for operation in ["complete", "fail", "retry"] {
        assert!(normalize(operation, b"i", &fields).is_ok());
    }
    assert!(normalize("cancel", b"i", &[("fencing_token", Integer(0))]).is_ok());
    Ok(())
}

fn partition_and_overrides() -> TestResult {
    let common = [
        ("lease_token", Bytes(b"base")),
        ("fencing_token", Integer(0)),
    ];
    let fields = [("lease_token", Bytes(b"item"))];
    let caller = normalize_core_mutation(
        MutationInput {
            operation: "complete",
            id: b"i",
            fields: &fields,
        },
        &common,
        Some(b"p"),
        AdmissionLimits::default(),
        RAW,
    )?;
    assert_eq!(caller.partition(), Some(b"p".as_slice()));
    assert!(matches!(
        caller.operation(),
        CoreMutation::Complete {
            lease_token: b"item",
            fence: 0,
            ..
        }
    ));
    for (partition, expected) in [
        (Nil, MutationCallerError::InvalidField("partition_key")),
        (Bytes(b"other"), MutationCallerError::PartitionMismatch),
    ] {
        let fields = [("partition_key", partition)];
        assert!(matches!(normalize_core_mutation(
            MutationInput { operation: "complete", id: b"i", fields: &fields },
            &common, Some(b"p"), AdmissionLimits::default(), RAW,
        ), Err(actual) if actual == expected));
    }
    let fields = [("partition_key", Nil)];
    let caller = normalize_core_mutation(
        MutationInput {
            operation: "complete",
            id: b"i",
            fields: &fields,
        },
        &common,
        None,
        AdmissionLimits::default(),
        RAW,
    )?;
    assert_eq!(caller.partition(), None);
    Ok(())
}

fn input_limits() -> TestResult {
    let fields = [("lease_token", Bytes(b"l")), ("fencing_token", Integer(1))];
    // id1 + operation8 + token(1+11+1) + fence(1+13+16) =52.
    assert_eq!(
        measure_core_mutation_input(
            MutationInput {
                operation: "complete",
                id: b"i",
                fields: &fields,
            },
            &[],
            52
        )?,
        52
    );
    assert!(matches!(
        measure_core_mutation_input(
            MutationInput {
                operation: "complete",
                id: b"i",
                fields: &fields,
            },
            &[],
            51
        ),
        Err(MutationCallerError::RawBytes)
    ));
    let excessive: [(&str, MutationFieldValue<'_>); 9] = std::array::from_fn(|_| ("", Nil));
    assert!(matches!(
        measure_core_mutation_input(
            MutationInput {
                operation: "complete",
                id: b"i",
                fields: &excessive,
            },
            &[],
            RAW
        ),
        Err(MutationCallerError::InvalidOptions)
    ));
    assert!(matches!(
        measure_core_mutation_input(
            MutationInput {
                operation: "complete",
                id: b"i",
                fields: &fields,
            },
            &excessive,
            RAW
        ),
        Err(MutationCallerError::InvalidOptions)
    ));
    Ok(())
}

fn schema_errors() {
    let duplicated = [("type", Bytes(b"r")), ("type", Bytes(b"s"))];
    assert!(matches!(
        normalize("create", b"i", &duplicated),
        Err(MutationCallerError::DuplicateField)
    ));
    for fields in [
        vec![("type", Bytes(b"r")), ("attributes", Unsupported)],
        vec![("type", Bytes(b"r")), ("payload", Unsupported)],
    ] {
        assert!(matches!(
            normalize("create", b"i", &fields),
            Err(MutationCallerError::UnsupportedFeature(_))
        ));
    }
    assert!(matches!(
        normalize("create", b"i", &[("type", Bytes(b"r")), ("mystery", Nil)]),
        Err(MutationCallerError::UnknownField)
    ));
    for (name, number) in [
        ("fencing_token", -1),
        ("fencing_token", 9_007_199_254_740_992),
    ] {
        assert!(
            normalize(
                "complete",
                b"i",
                &[("lease_token", Bytes(b"l")), (name, Integer(number))]
            )
            .is_err()
        );
    }
    assert!(normalize("create", b"", &[("type", Bytes(b"r"))]).is_err());
    assert!(
        normalize(
            "create",
            b"__ferricstore_schedule__:x",
            &[("type", Bytes(b"r"))]
        )
        .is_err()
    );
    assert!(
        normalize(
            "create",
            b"i",
            &[("type", Bytes(b"r")), ("state", Bytes(b"running"))]
        )
        .is_err()
    );
    assert!(
        normalize(
            "extend_lease",
            b"i",
            &[
                ("lease_token", Bytes(b"l")),
                ("fencing_token", Integer(1)),
                ("lease_ms", Integer(0))
            ]
        )
        .is_err()
    );
    assert!(matches!(
        normalize(
            "transition",
            b"i",
            &[
                ("from", Bytes(b"q")),
                ("to", Bytes(b"z")),
                ("fencing_token", Integer(0)),
                ("priority", Integer(1))
            ]
        ),
        Err(MutationCallerError::UnsupportedFeature("priority"))
    ));
}

#[test]
fn public_core_caller_defaults_guards_and_feature_refusals() -> TestResult {
    reserved_types();
    defaults_and_presence()?;
    partition_and_overrides()?;
    input_limits()?;
    schema_errors();
    diagnostic_tokens_are_redacted()?;
    terminal_creates_restore()
}

fn reserved_types() {
    let reserved = matches!(
        normalize(
            "create",
            b"i",
            &[("type", Bytes(b"__ferricstore_schedule"))]
        ),
        Err(MutationCallerError::InvalidField("type"))
    );
    let lookalike = normalize(
        "create",
        b"i",
        &[("type", Bytes(b"__ferricstore_schedule__:ordinary"))],
    )
    .is_ok();
    let mut rich = [false; 6];
    for (index, name) in [
        "parent_flow_id",
        "root_flow_id",
        "correlation_id",
        "retention_ttl_ms",
        "history_hot_max_events",
        "state_meta",
    ]
    .into_iter()
    .enumerate()
    {
        rich[index] = matches!(
            normalize(
                "create",
                b"i",
                &[("type", Bytes(b"r")), (name, Unsupported)]
            ),
            Err(MutationCallerError::UnsupportedFeature(_))
        );
    }
    let terminal_fields = [
        ("lease_token", Bytes(b"l")),
        ("fencing_token", Integer(1)),
        ("ttl_ms", Unsupported),
    ];
    let ttl = matches!(
        normalize("complete", b"i", &terminal_fields),
        Err(MutationCallerError::UnsupportedFeature(_))
    );
    assert_eq!(
        (reserved, lookalike, rich, ttl),
        (true, true, [true; 6], true)
    );
}

fn diagnostic_tokens_are_redacted() -> TestResult {
    let secret = b"ferric-token-sentinel".as_slice();
    let fields = [
        ("lease_token", Bytes(secret)),
        ("fencing_token", Integer(1)),
    ];
    let input = MutationInput {
        operation: "complete",
        id: b"i",
        fields: &fields,
    };
    let normalized = normalize(input.operation, input.id, input.fields)?;
    let guard = LeaseGuard {
        token: LeaseToken::new(secret.to_vec())?,
        fence: Fence::new(1),
    };
    let rendered = [
        format!("{:?}", fields[0].1),
        format!("{input:?}"),
        format!("{:?}", normalized.operation()),
        format!("{normalized:?}"),
        format!("{:?}", guard.token),
        format!("{guard:?}"),
    ];
    let ascii = std::str::from_utf8(secret)?;
    let decimal = format!("{secret:?}");
    let exposed = rendered.map(|value| value.contains(ascii) || value.contains(&decimal));
    assert_eq!(exposed, [false; 6]);
    Ok(())
}

fn restore_with_kind(
    record: &WorkflowRecord,
    kind: wf_domain::history::EventKind,
) -> Result<WorkflowRecord, wf_domain::workflow::active::ActiveValueError> {
    restore_record(
        WorkflowRecordData {
            id: record.id().clone(),
            workflow_type: record.workflow_type().to_vec(),
            state: record.state().to_vec(),
            version: record.version(),
            fence: record.fence(),
            attempts: record.attempts(),
            created_at: record.created_at(),
            updated_at: record.updated_at(),
            next_run_at: record.next_run_at(),
            run_state: record.run_state().map(<[u8]>::to_vec),
            lease: record.lease().cloned(),
            payload: record.payload().map(<[u8]>::to_vec),
            result: record.result().map(<[u8]>::to_vec),
            error: record.error().map(<[u8]>::to_vec),
            latest_kind: kind,
            latest_version: record.version(),
            latest_fence: record.fence(),
        },
        AdmissionLimits::default(),
    )
}

fn terminal_creates_restore() -> TestResult {
    use wf_domain::history::EventKind;
    for state in [b"completed".as_slice(), b"failed", b"cancelled"] {
        for run_at in [None, Some(TimestampMillis::new(0))] {
            let record = reduce(
                None,
                &CommittedCommand {
                    id: RecordId::new(b"i".to_vec())?,
                    now: TimestampMillis::new(1_000),
                    operation: Operation::Create(Create {
                        workflow_type: b"r".to_vec(),
                        state: state.to_vec(),
                        run_at,
                        payload: None,
                    }),
                },
            )?
            .into_parts()
            .1
            .ok_or("terminal create absent")?
            .record;
            assert_eq!(
                record.next_run_at(),
                Some(run_at.unwrap_or(TimestampMillis::new(1_000)))
            );
            assert_eq!(with_attempts(&record, 0)?, record);
            assert_eq!(restore_with_kind(&record, EventKind::Created)?, record);
            for kind in [
                EventKind::Completed,
                EventKind::Failed,
                EventKind::Cancelled,
                EventKind::Retry,
            ] {
                assert!(restore_with_kind(&record, kind).is_err());
            }
        }
    }
    Ok(())
}
