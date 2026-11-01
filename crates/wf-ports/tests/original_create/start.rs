//! Original atomic birth fields and the complete source deadline regression.
use super::*;
use wf_ports::projection::query_row::prepare_original_start_and_claim as prepare;

fn start<'r, 'a>(options: I<'r, 'a>) -> TestResult<PreparedOriginalCreate<'r, 'a>> {
    match prepare(
        I::Binary(b"flow-1"),
        I::Binary(b"email"),
        I::Binary(b"queued"),
        options,
        OriginalCreateContext::default(),
        limits(),
    )? {
        Outcome::Accepted { value, .. } => Ok(value),
        other => Err(format!("expected atomic start: {other:?}").into()),
    }
}

#[test]
fn original_start_and_claim_preserves_birth_shape_and_omission() -> TestResult {
    let pairs = [[I::Atom("worker"), I::Binary(b"worker")]];
    let opts = pairs.iter().map(|p| I::Tuple(p)).collect::<Vec<_>>();
    let value = start(I::List(&opts))?;
    assert!(matches!(value.field("id"), Some(I::Binary(b"flow-1"))));
    assert!(matches!(value.field("type"), Some(I::Binary(b"email"))));
    assert!(matches!(value.field("state"), Some(I::Binary(b"running"))));
    assert!(matches!(
        value.field("run_state"),
        Some(I::Binary(b"queued"))
    ));
    assert!(matches!(value.field("worker"), Some(I::Binary(b"worker"))));
    assert!(matches!(value.field("lease_ms"), Some(I::Integer(30_000))));
    assert!(matches!(
        value.field("partition_key"),
        Some(I::Binary(b"__flow_auto__:253"))
    ));
    assert!(matches!(
        value.field("partition_key_explicit"),
        Some(I::Boolean(false))
    ));
    for name in [
        "now_ms",
        "run_at_ms",
        "idempotent",
        "root_flow_id",
        "payload",
        "schedule_metadata",
    ] {
        assert!(value.field(name).is_none(), "{name}");
    }
    assert_eq!(value.field_count(), 9);
    assert_eq!(value.state_key(), b"f:{fa:253}:s:flow-1");
    assert!(value.admission_metadata_bytes() > 0);
    Ok(())
}

#[test]
fn source_lease_mutations_reject_deadlines_outside_exact_timestamp_range() -> TestResult {
    let max_exact_integer = 9_007_199_254_740_991;
    for (now, lease, expected) in [
        (
            0,
            max_exact_integer + 1,
            "ERR flow lease_ms exceeds maximum 9007199254740991",
        ),
        (
            max_exact_integer,
            1,
            "ERR flow lease_ms deadline exceeds maximum 9007199254740991",
        ),
    ] {
        let pairs = [
            [I::Atom("worker"), I::Binary(b"worker")],
            [I::Atom("now_ms"), I::Integer(now)],
            [I::Atom("lease_ms"), I::Integer(lease)],
        ];
        let opts = pairs.iter().map(|p| I::Tuple(p)).collect::<Vec<_>>();
        let outcome = prepare(
            I::Binary(b"flow-1"),
            I::Binary(b"email"),
            I::Binary(b"queued"),
            I::List(&opts),
            OriginalCreateContext::default(),
            limits(),
        )?;
        assert!(matches!(outcome, Outcome::Rejected {message, ..} if message == expected));
    }
    Ok(())
}

#[test]
fn atomic_start_preserves_normalized_metadata_and_original_value_presence() -> TestResult {
    let attributes = [(I::Atom("enabled"), I::Boolean(false))];
    let state_meta = [(I::Atom("attempt"), I::Integer(3))];
    let values = [I::Binary(b"order")];
    let value_refs = [(I::Atom("reservation"), I::Binary(b"ref-1"))];
    let pairs = [
        [I::Atom("worker"), I::Binary(b"worker")],
        [I::Atom("attributes"), I::Map(&attributes)],
        [I::Atom("state_meta"), I::Map(&state_meta)],
        [I::Atom("parent_flow_id"), I::Nil],
        [I::Atom("root_flow_id"), I::Binary(b"flow-1")],
        [I::Atom("correlation_id"), I::Binary(b"")],
        [I::Atom("partition_key"), I::Binary(b"p1")],
        [I::Atom("payload"), I::Nil],
        [I::Atom("payload_ref"), I::Binary(b"")],
        [I::Atom("values"), I::List(&values)],
        [I::Atom("value_refs"), I::Map(&value_refs)],
        [I::Atom("max_active_ms"), I::Nil],
        [I::Atom("now_ms"), I::Integer(7)],
        [I::Atom("lease_ms"), I::Integer(10)],
    ];
    let opts = pairs.iter().map(|p| I::Tuple(p)).collect::<Vec<_>>();
    let value = start(I::List(&opts))?;
    assert!(matches!(
        value.field("state_meta_state"),
        Some(I::Binary(b"queued"))
    ));
    assert!(matches!(
        value.caller().state_metadata_update(),
        [(
            "attempt",
            wf_domain::metadata::OriginalMetadataScalar::Integer(3)
        )]
    ));
    assert!(matches!(
        value.caller().attributes().entries.as_slice(),
        [(
            "enabled",
            wf_domain::metadata::OriginalAttributeValue::Scalar(
                wf_domain::metadata::OriginalMetadataScalar::Boolean(false)
            )
        )]
    ));
    assert!(matches!(value.field("payload"), Some(I::Nil)));
    assert!(matches!(value.field("payload_ref"), Some(I::Binary(b""))));
    assert!(
        matches!(value.field("values"), Some(I::List(v)) if std::ptr::eq(v, values.as_slice()))
    );
    assert!(
        matches!(value.field("value_refs"), Some(I::Map(v)) if std::ptr::eq(v, value_refs.as_slice()))
    );
    check_compact_links(&value);
    assert!(matches!(
        value.field("max_active_ms"),
        Some(I::Atom("infinity"))
    ));
    assert!(matches!(value.field("now_ms"), Some(I::Integer(7))));
    assert!(value.field("run_at_ms").is_none());
    Ok(())
}

fn check_compact_links(value: &PreparedOriginalCreate<'_, '_>) {
    assert!(value.field("parent_flow_id").is_none());
    assert!(value.field("root_flow_id").is_none());
    assert!(matches!(
        value.field("correlation_id"),
        Some(I::Binary(b""))
    ));
    assert!(value.field("partition_key_explicit").is_none());
}

#[test]
fn start_borrows_large_worker_and_ignores_helper_only_unknown_options() -> TestResult {
    let worker = vec![b'w'; 65_536];
    let pairs = [
        [I::Atom("worker"), I::Binary(&worker)],
        [I::Atom("worker"), I::Nil],
        [I::Atom("return"), I::Other],
        [I::Atom("type"), I::Nil],
        [I::Atom("state"), I::Binary(b"running")],
        [I::Atom("schedule_metadata"), I::Nil],
    ];
    let opts = pairs.iter().map(|p| I::Tuple(p)).collect::<Vec<_>>();
    let value = start(I::List(&opts))?;
    assert!(
        matches!(value.field("worker"), Some(I::Binary(v)) if v.as_ptr() == worker.as_ptr() && v.len() == worker.len())
    );
    assert!(matches!(value.field("type"), Some(I::Binary(b"email"))));
    assert!(matches!(
        value.field("run_state"),
        Some(I::Binary(b"queued"))
    ));
    assert!(value.caller().schedule_metadata().is_none());
    assert!(value.owned_bytes() < worker.len());
    Ok(())
}

#[test]
fn start_resource_boundaries_are_inclusive_and_cover_all_explicit_arguments() -> TestResult {
    let state = vec![b's'; 128];
    let ignored = vec![0; 20_000];
    let pairs = [
        [I::Atom("worker"), I::Binary(b"worker")],
        [I::Atom("unknown"), I::Binary(&ignored)],
    ];
    let opts = pairs.iter().map(|p| I::Tuple(p)).collect::<Vec<_>>();
    let call = |cap| {
        prepare(
            I::Binary(b"flow-1"),
            I::Binary(b"email"),
            I::Binary(&state),
            I::List(&opts),
            OriginalCreateContext::default(),
            cap,
        )
    };
    let Outcome::Accepted {
        value,
        work_units,
        temporary_bytes,
    } = call(limits())?
    else {
        return Err("resource baseline".into());
    };
    for (dimension, amount) in [value.owned_bytes(), work_units, temporary_bytes]
        .into_iter()
        .enumerate()
    {
        let mut cap = limits();
        match dimension {
            0 => cap.maximum_output_bytes = amount,
            1 => cap.maximum_work_units = amount,
            _ => cap.maximum_temporary_bytes = amount,
        }
        assert!(matches!(call(cap), Ok(Outcome::Accepted { .. })));
        match dimension {
            0 => cap.maximum_output_bytes -= 1,
            1 => cap.maximum_work_units -= 1,
            _ => cap.maximum_temporary_bytes -= 1,
        }
        assert!(call(cap).is_err());
    }
    let mut cap = limits();
    cap.maximum_input_bytes = 20_000;
    assert!(call(cap).is_err());
    cap = limits();
    cap.maximum_input_bytes = 0;
    assert!(call(cap).is_err());
    Ok(())
}

#[test]
fn start_raises_on_improper_metadata_and_refuses_oversized_query_run_state() -> TestResult {
    let tail = I::Nil;
    let prefix = [I::Binary(b"bad")];
    let attrs = [(
        I::Atom("bad"),
        I::ImproperList {
            prefix: &prefix,
            tail: &tail,
        },
    )];
    let pairs = [
        [I::Atom("worker"), I::Binary(b"worker")],
        [I::Atom("attributes"), I::Map(&attrs)],
    ];
    let opts = pairs.iter().map(|p| I::Tuple(p)).collect::<Vec<_>>();
    assert!(matches!(
        prepare(
            I::Binary(b"i"),
            I::Binary(b"email"),
            I::Binary(b"queued"),
            I::List(&opts),
            OriginalCreateContext::default(),
            limits(),
        )?,
        Outcome::RaisedArgumentError { .. }
    ));
    let worker = [I::Atom("worker"), I::Binary(b"worker")];
    let opts = [I::Tuple(&worker)];
    let state = vec![b's'; 32_768];
    assert!(matches!(prepare(
        I::Binary(b"i"), I::Binary(b"email"), I::Binary(&state),
        I::List(&opts), OriginalCreateContext::default(), limits(),
    )?, Outcome::Rejected {message, ..} if message == "ERR flow query metadata exceeds 32629 bytes"));
    Ok(())
}
