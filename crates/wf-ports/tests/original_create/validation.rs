//! Supplemental source-order, field-shape and normalization controls.
use super::*;
use wf_domain::metadata::{OriginalAttributeValue as V, OriginalMetadataScalar as S};

#[test]
fn keyword_first_occurrence_unknown_options_and_false_omission_remain_exact() -> TestResult {
    let pairs = [
        [I::Atom("type"), I::Binary(b"email")],
        [I::Atom("type"), I::Nil],
        [I::Atom("returning"), I::Other],
        [I::Atom("worker"), I::Other],
        [I::Atom("idempotent"), I::Boolean(false)],
        [I::Atom("priority"), I::Integer(0)],
        [I::Atom("root_flow_id"), I::Binary(b"flow-1")],
    ];
    let options = pairs.iter().map(|p| I::Tuple(p)).collect::<Vec<_>>();
    let attrs = accepted(b"flow-1", I::List(&options))?;
    assert!(matches!(attrs.field("type"), Some(I::Binary(b"email"))));
    assert!(matches!(
        attrs.field("partition_key_explicit"),
        Some(I::Boolean(false))
    ));
    assert!(attrs.field("idempotent").is_none());
    assert!(attrs.field("priority").is_none());
    assert!(attrs.field("root_flow_id").is_none());
    assert!(attrs.field("worker").is_none());
    assert!(attrs.field("returning").is_none());
    assert!(attrs.caller().attributes().entries.is_empty());
    assert!(attrs.caller().state_metadata_update().is_empty());
    assert_eq!(attrs.field_count(), 6);
    Ok(())
}

#[test]
fn create_normalizes_dynamic_aliases_lists_and_a_scalar_only_state_update() -> TestResult {
    let values = [I::Atom("blue"), I::Binary(b"red"), I::Binary(b"blue")];
    let attrs = [
        (I::Atom("color"), I::Binary(b"old")),
        (I::Binary(b" color "), I::List(&values)),
        (I::Atom("flag"), I::Boolean(false)),
    ];
    let state = [
        (I::Atom("count"), I::Integer(1)),
        (I::Binary(b" count "), I::Integer(2)),
        (I::Atom("empty"), I::Nil),
    ];
    let pairs = [
        [I::Atom("type"), I::Binary(b"email")],
        [I::Atom("attributes"), I::Map(&attrs)],
        [I::Atom("state_meta"), I::Map(&state)],
        [I::Atom("state"), I::Binary(b"q")],
        [I::Atom("partition_key"), I::Binary(b"p")],
    ];
    let options = pairs.iter().map(|p| I::Tuple(p)).collect::<Vec<_>>();
    let attrs = accepted(b"i", I::List(&options))?;
    let actual = &attrs.caller().attributes().entries;
    assert_eq!(actual.len(), 2);
    assert_eq!(actual[0].0, "color");
    assert!(matches!(&actual[0].1,V::List(v) if *v==[&b"red"[..],&b"blue"[..]]));
    assert!(matches!(&actual[1].1, V::Scalar(S::Boolean(false))));
    let state = attrs.caller().state_metadata_update();
    assert_eq!(state.len(), 2);
    assert_eq!(state[0].0, "count");
    assert!(matches!(state[0].1, S::Integer(2)));
    assert_eq!(state[1].0, "empty");
    assert!(matches!(state[1].1, S::Nil));
    assert!(matches!(
        attrs.field("state_meta_state"),
        Some(I::Binary(b"q"))
    ));
    assert!(attrs.field("partition_key_explicit").is_none());
    assert!(attrs.admission_metadata_bytes() > 200);
    Ok(())
}

#[test]
fn create_error_order_matches_keyword_identity_scalar_metadata_and_schedule_checks() -> TestResult {
    rejected(b"", I::Nil, "ERR flow opts must be a keyword list")?;
    let return_pair = [I::Atom("return"), I::Nil];
    let opts = [I::Tuple(&return_pair)];
    rejected(
        b"",
        I::List(&opts),
        "ERR flow return option is not supported",
    )?;
    let empty = [];
    rejected(
        b"__ferricstore_schedule__:x",
        I::List(&empty),
        "ERR flow id is reserved for internal use",
    )?;
    rejected(b"i", I::List(&empty), "ERR flow type is required")?;
    let pairs = [
        [I::Atom("type"), I::Binary(b"email")],
        [I::Atom("state"), I::Binary(b"running")],
        [I::Atom("attributes"), I::Other],
        [I::Atom("ttl_ms"), I::Other],
    ];
    let opts = pairs.iter().map(|p| I::Tuple(p)).collect::<Vec<_>>();
    rejected(
        b"i",
        I::List(&opts),
        "ERR flow running state is only entered by FLOW.CLAIM_DUE",
    )?;
    let pairs = [
        [I::Atom("type"), I::Binary(b"email")],
        [I::Atom("attributes"), I::Other],
        [I::Atom("ttl_ms"), I::Other],
    ];
    let opts = pairs.iter().map(|p| I::Tuple(p)).collect::<Vec<_>>();
    rejected(
        b"i",
        I::List(&opts),
        "ERR flow ttl_ms was renamed to retention_ttl_ms",
    )?;
    let pairs = [
        [I::Atom("type"), I::Binary(b"email")],
        [I::Atom("attributes"), I::Other],
        [I::Atom("state_meta"), I::Other],
        [I::Atom("schedule_metadata"), I::Nil],
    ];
    let opts = pairs.iter().map(|p| I::Tuple(p)).collect::<Vec<_>>();
    rejected(b"i", I::List(&opts), "ERR flow attributes must be a map")?;
    Ok(())
}

#[test]
fn scalar_nil_empty_and_source_option_presence_are_not_collapsed() -> TestResult {
    let pairs = [
        [I::Atom("type"), I::Binary(b"email")],
        [I::Atom("parent_flow_id"), I::Binary(b"")],
        [I::Atom("correlation_id"), I::Binary(b"")],
        [I::Atom("payload"), I::Nil],
        [I::Atom("payload_ref"), I::Nil],
        [I::Atom("values"), I::Nil],
        [I::Atom("drop_values"), I::List(&[])],
        [I::Atom("override_values"), I::Boolean(false)],
        [I::Atom("max_active_ms"), I::Nil],
    ];
    let opts = pairs.iter().map(|p| I::Tuple(p)).collect::<Vec<_>>();
    let attrs = accepted(b"i", I::List(&opts))?;
    for name in ["payload", "payload_ref", "values"] {
        assert!(matches!(attrs.field(name), Some(I::Nil)));
    }
    assert!(matches!(attrs.field("drop_values"),Some(I::List(v)) if v.is_empty()));
    assert!(matches!(
        attrs.field("override_values"),
        Some(I::Boolean(false))
    ));
    assert!(matches!(
        attrs.field("parent_flow_id"),
        Some(I::Binary(b""))
    ));
    assert!(matches!(
        attrs.field("correlation_id"),
        Some(I::Binary(b""))
    ));
    assert!(matches!(
        attrs.field("max_active_ms"),
        Some(I::Atom("infinity"))
    ));
    Ok(())
}

#[test]
fn arbitrary_retention_and_hot_history_integers_keep_mathematical_bounds() -> TestResult {
    let max = [0_u8, 0, 0, 0, 0, 0, 0, 0, 1];
    let greater = [1_u8, 0, 0, 0, 0, 0, 0, 0, 1];
    let wide = I::BigInteger {
        negative: false,
        little_endian: &max,
    };
    let context = OriginalCreateContext {
        history_hot_maximum: wide,
        ..OriginalCreateContext::default()
    };
    let pairs = [
        [I::Atom("type"), I::Binary(b"email")],
        [I::Atom("retention_ttl_ms"), wide],
        [I::Atom("history_hot_max_events"), wide],
    ];
    let opts = pairs.iter().map(|p| I::Tuple(p)).collect::<Vec<_>>();
    let Outcome::Accepted { value: attrs, .. } =
        prepare_original_create(I::Binary(b"i"), I::List(&opts), context, limits())?
    else {
        return Err("wide caller integer".into());
    };
    assert!(
        matches!(attrs.field("retention_ttl_ms"),Some(I::BigInteger{little_endian,..}) if little_endian==max)
    );
    assert!(
        matches!(attrs.field("history_hot_max_events"),Some(I::BigInteger{little_endian,..}) if little_endian==max)
    );
    let pairs = [
        [I::Atom("type"), I::Binary(b"email")],
        [
            I::Atom("history_hot_max_events"),
            I::BigInteger {
                negative: false,
                little_endian: &greater,
            },
        ],
    ];
    let opts = pairs.iter().map(|p| I::Tuple(p)).collect::<Vec<_>>();
    assert!(
        matches!(prepare_original_create(I::Binary(b"i"),I::List(&opts),context,limits())?,Outcome::Rejected{message,..} if message=="ERR flow history_hot_max_events exceeds maximum 18446744073709551616")
    );
    Ok(())
}

#[test]
fn internal_looking_caller_options_do_not_supply_the_captured_grant() -> TestResult {
    let pairs = [
        [I::Atom("type"), I::Binary(b"__ferricstore_schedule")],
        [I::Atom("__ferricstore_internal__"), I::Boolean(true)],
    ];
    let opts = pairs.iter().map(|p| I::Tuple(p)).collect::<Vec<_>>();
    rejected(
        b"i",
        I::List(&opts),
        "ERR flow type is reserved for internal use",
    )?;
    rejected(
        b"__ferricstore_schedule__:x",
        I::List(&opts),
        "ERR flow id is reserved for internal use",
    )?;
    Ok(())
}

#[test]
fn metadata_argument_raises_and_query_admission_refusals_keep_their_origin() -> TestResult {
    let prefix = [I::Binary(b"x")];
    let attrs = [(
        I::Binary(b"list"),
        I::ImproperList {
            prefix: &prefix,
            tail: &I::Integer(1),
        },
    )];
    let pairs = [
        [I::Atom("type"), I::Binary(b"email")],
        [I::Atom("attributes"), I::Map(&attrs)],
    ];
    let opts = pairs.iter().map(|p| I::Tuple(p)).collect::<Vec<_>>();
    assert!(
        matches!(prepare_original_create(I::Binary(b"i"),I::List(&opts),OriginalCreateContext::default(),limits())?,Outcome::RaisedArgumentError{work_units,..} if work_units>0)
    );
    let large = vec![b'x'; 40_000];
    let pair = [I::Atom("type"), I::Binary(&large)];
    let opts = [I::Tuple(&pair)];
    rejected(b"i", I::List(&opts), "ERR invalid flow query metadata")?;
    let maximum_value = vec![b'x'; 32_768];
    let pair = [I::Atom("type"), I::Binary(&maximum_value)];
    let opts = [I::Tuple(&pair)];
    rejected(
        b"i",
        I::List(&opts),
        "ERR flow query metadata exceeds 32629 bytes",
    )?;
    let id = vec![b'i'; 65_535];
    let pair = [I::Atom("type"), I::Binary(b"email")];
    let opts = [I::Tuple(&pair)];
    rejected(&id, I::List(&opts), "ERR key too large (max 65535 bytes)")?;
    Ok(())
}

#[test]
fn public_create_validates_all_scalar_option_classes_and_history_configuration() -> TestResult {
    let cases = [
        ("state", I::Nil, "ERR flow state must be a non-empty string"),
        (
            "now_ms",
            I::Nil,
            "ERR flow now_ms must be a non-negative integer",
        ),
        (
            "run_at_ms",
            I::Nil,
            "ERR flow run_at_ms must be a non-negative integer",
        ),
        (
            "retention_ttl_ms",
            I::Integer(0),
            "ERR flow retention_ttl_ms must be a positive integer",
        ),
        (
            "max_active_ms",
            I::Integer(0),
            "ERR flow max_active_ms must be between 1 and 31536000000 or infinity",
        ),
        (
            "history_hot_max_events",
            I::Integer(-1),
            "ERR flow history_hot_max_events must be a non-negative integer",
        ),
        (
            "history_hot_max_events",
            I::Integer(10_001),
            "ERR flow history_hot_max_events exceeds maximum 10000",
        ),
        (
            "history_max_events",
            I::Integer(0),
            "ERR flow history_max_events must be a positive integer",
        ),
        (
            "history_max_events",
            I::Integer(1_000_001),
            "ERR flow history_max_events exceeds maximum 1000000",
        ),
        (
            "priority",
            I::Boolean(false),
            "ERR flow priority must be between 0 and 2",
        ),
        (
            "priority",
            I::Integer(3),
            "ERR flow priority must be between 0 and 2",
        ),
        (
            "idempotent",
            I::Nil,
            "ERR flow idempotent must be a boolean",
        ),
        (
            "partition_key",
            I::Binary(b""),
            "ERR flow partition_key must be a non-empty string or :global",
        ),
    ];
    for (name, value, message) in cases {
        let pairs = [
            [I::Atom("type"), I::Binary(b"email")],
            [I::Atom(name), value],
        ];
        let opts = pairs.iter().map(|p| I::Tuple(p)).collect::<Vec<_>>();
        rejected(b"i", I::List(&opts), message)?;
    }
    Ok(())
}

#[test]
fn public_create_history_configuration_keeps_fallback_clamp_and_pair_order() -> TestResult {
    let pairs = [
        [I::Atom("type"), I::Binary(b"email")],
        [I::Atom("history_hot_max_events"), I::Integer(2)],
        [I::Atom("history_max_events"), I::Integer(1)],
    ];
    let opts = pairs.iter().map(|p| I::Tuple(p)).collect::<Vec<_>>();
    rejected(
        b"i",
        I::List(&opts),
        "ERR flow history_max_events must be greater than or equal to history_hot_max_events",
    )?;
    let pairs = [
        [I::Atom("type"), I::Binary(b"email")],
        [I::Atom("history_hot_max_events"), I::Integer(8)],
        [I::Atom("history_max_events"), I::Integer(9)],
    ];
    let opts = pairs.iter().map(|p| I::Tuple(p)).collect::<Vec<_>>();
    let context = OriginalCreateContext {
        history_hot_maximum: I::Integer(7),
        history_maximum: I::Integer(8),
        ..OriginalCreateContext::default()
    };
    assert!(
        matches!(prepare_original_create(I::Binary(b"i"),I::List(&opts),context,limits())?,Outcome::Rejected{message,..} if message=="ERR flow history_hot_max_events exceeds maximum 7")
    );
    let context = OriginalCreateContext {
        history_hot_maximum: I::Nil,
        history_maximum: I::Integer(2_000_000),
        ..OriginalCreateContext::default()
    };
    assert!(matches!(
        prepare_original_create(I::Binary(b"i"), I::List(&opts), context, limits())?,
        Outcome::Accepted { .. }
    ));
    Ok(())
}
