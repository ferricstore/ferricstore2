use super::*;

#[test]
fn create_attrs_preserves_default_state_auto_partition_and_named_value_options() -> TestResult {
    let names = [I::Binary(b"order")];
    let refs = [(I::Atom("reservation"), I::Binary(b"ref-1"))];
    let pairs = [
        [I::Atom("type"), I::Binary(b"email")],
        [I::Atom("values"), I::List(&names)],
        [I::Atom("value_refs"), I::Map(&refs)],
    ];
    let options = pairs.iter().map(|p| I::Tuple(p)).collect::<Vec<_>>();
    let result = accepted(b"flow-1", I::List(&options));
    assert!(result.is_ok(), "{result:?}");
    let attrs = result?;
    assert!(matches!(attrs.field("id"), Some(I::Binary(b"flow-1"))));
    assert!(matches!(attrs.field("type"), Some(I::Binary(b"email"))));
    assert!(matches!(attrs.field("state"), Some(I::Binary(b"queued"))));
    assert!(matches!(
        attrs.field("partition_key"),
        Some(I::Binary(b"__flow_auto__:253"))
    ));
    assert!(
        matches!(attrs.field("values"), Some(I::List(v)) if v.len()==1 && matches!(v[0], I::Binary(b"order")))
    );
    assert!(
        matches!(attrs.field("value_refs"), Some(I::Map(v)) if v.len()==1 && matches!(v[0], (I::Atom("reservation"),I::Binary(b"ref-1"))))
    );
    Ok(())
}

#[test]
fn create_attrs_accepts_infinity_while_validating_compact_query_metadata() -> TestResult {
    let pairs = [
        [I::Atom("type"), I::Binary(b"email")],
        [I::Atom("max_active_ms"), I::Atom("infinity")],
    ];
    let options = pairs.iter().map(|p| I::Tuple(p)).collect::<Vec<_>>();
    let result = accepted(b"flow-without-active-timeout", I::List(&options));
    assert!(result.is_ok(), "{result:?}");
    let attrs = result?;
    assert!(matches!(
        attrs.field("max_active_ms"),
        Some(I::Atom("infinity"))
    ));
    Ok(())
}

#[test]
fn create_attrs_rejects_unleased_running_state_before_replicated_mutation_preparation() -> TestResult
{
    let pairs = [
        [I::Atom("type"), I::Binary(b"email")],
        [I::Atom("state"), I::Binary(b"running")],
        [I::Atom("worker"), I::Binary(b"worker")],
        [I::Atom("lease_ms"), I::Integer(30_000)],
    ];
    let options = pairs.iter().map(|p| I::Tuple(p)).collect::<Vec<_>>();
    for end in [2, 4] {
        assert_eq!(
            rejection_message(b"flow-running", I::List(&options[..end]))?,
            "ERR flow running state is only entered by FLOW.CLAIM_DUE",
        );
    }
    Ok(())
}

#[test]
fn create_attrs_rejects_timestamps_that_cannot_be_represented_exactly_by_flow_indexes() -> TestResult
{
    let pairs = [
        [I::Atom("type"), I::Binary(b"email")],
        [I::Atom("now_ms"), I::Integer(9_007_199_254_740_992)],
    ];
    let options = pairs.iter().map(|p| I::Tuple(p)).collect::<Vec<_>>();
    assert_eq!(
        rejection_message(b"flow-1", I::List(&options))?,
        "ERR flow now_ms exceeds maximum 9007199254740991",
    );
    let pairs = [
        [I::Atom("type"), I::Binary(b"email")],
        [I::Atom("now_ms"), I::Integer(1)],
        [I::Atom("run_at_ms"), I::Integer(9_007_199_254_740_992)],
    ];
    let options = pairs.iter().map(|p| I::Tuple(p)).collect::<Vec<_>>();
    assert_eq!(
        rejection_message(b"flow-1", I::List(&options))?,
        "ERR flow run_at_ms exceeds maximum 9007199254740991",
    );
    Ok(())
}

#[test]
fn schedule_discovery_metadata_is_accepted_only_through_the_captured_internal_path() -> TestResult {
    let metadata = [
        (I::Atom("kind"), I::Atom("interval")),
        (I::Atom("target_type"), I::Binary(b"email")),
    ];
    let pairs = [
        [I::Atom("type"), I::Binary(b"email")],
        [I::Atom("schedule_metadata"), I::Map(&metadata)],
    ];
    let opts = pairs.iter().map(|p| I::Tuple(p)).collect::<Vec<_>>();
    rejected(
        b"flow-1",
        I::List(&opts),
        "ERR flow schedule metadata is reserved for internal use",
    )?;
    let pairs = [
        [I::Atom("type"), I::Binary(b"__ferricstore_schedule")],
        [I::Atom("schedule_metadata"), I::Map(&metadata)],
    ];
    let opts = pairs.iter().map(|p| I::Tuple(p)).collect::<Vec<_>>();
    let context = OriginalCreateContext {
        internal_allowed: true,
        ..OriginalCreateContext::default()
    };
    let Outcome::Accepted { value: attrs, .. } = prepare_original_create(
        I::Binary(b"__ferricstore_schedule__:daily"),
        I::List(&opts),
        context,
        limits(),
    )?
    else {
        return Err("internal schedule create".into());
    };
    let schedule = attrs
        .caller()
        .schedule_metadata()
        .ok_or("schedule metadata")?;
    assert_eq!(
        (schedule.kind, schedule.target_type, schedule.timezone),
        ("interval", &b"email"[..], None)
    );
    let invalid = [
        (I::Atom("kind"), I::Atom("cron")),
        (I::Atom("target_type"), I::Binary(b"email")),
    ];
    let pairs = [
        [I::Atom("type"), I::Binary(b"__ferricstore_schedule")],
        [I::Atom("schedule_metadata"), I::Map(&invalid)],
    ];
    let opts = pairs.iter().map(|p| I::Tuple(p)).collect::<Vec<_>>();
    assert!(
        matches!(prepare_original_create(I::Binary(b"__ferricstore_schedule__:invalid"),I::List(&opts),context,limits())?,Outcome::Rejected{message,..} if message=="ERR flow schedule metadata is reserved for internal use")
    );
    Ok(())
}
