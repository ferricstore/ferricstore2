//! Original merge/reset recipes and resolved-state deletion/content inheritance.

use super::{
    PolicySchemaCharge, Raw, Reason, TestResult, accepted, current, invoke, invoke_captured,
    invoke_for_type, profile, rejected,
};
use wf_domain::policy::complete::PolicyRetentionInputRejection as Retention;

#[test]
fn original_public_merge_preserves_resolved_states_and_source_fields() -> TestResult {
    assert_resolved_states()?;
    assert_nil_and_empty()?;
    assert_original_independent_patch()?;
    assert_original_string_patch()?;
    assert_original_native_patch()?;
    assert_original_replace()?;
    assert_captured_history()?;
    Ok(())
}

pub(super) fn policy_with_max(number: i128) -> TestResult<Vec<u8>> {
    let fields = [
        (Raw::Atom("max_active_ms"), Raw::Integer(number)),
        (Raw::Atom("states"), Raw::Map(&[])),
        (Raw::Atom("type"), Raw::Binary(b"r")),
    ];
    current::envelope(2, Raw::Map(&fields))
}

pub(super) fn assert_nested_exact_classes() -> TestResult {
    let config = [
        (Raw::Atom("mode"), Raw::Atom("fifo")),
        (Raw::Binary(b"mode"), Raw::Atom("parallel")),
    ];
    let states = [(Raw::Binary(b"queued"), Raw::Map(&config))];
    let fields = [
        (Raw::Atom("states"), Raw::Map(&states)),
        (Raw::Atom("type"), Raw::Binary(b"r")),
    ];
    let stored = current::envelope(1, Raw::Map(&fields))?;
    for (key, value, expected) in [
        (Raw::Binary(b"mode"), Raw::Atom("parallel"), "fifo"),
        (Raw::Atom("mode"), Raw::Atom("parallel"), "parallel"),
    ] {
        let config = [(key, value)];
        let patch = [(Raw::Binary(b"queued"), Raw::Map(&config))];
        let pair = [Raw::Atom("states"), Raw::Map(&patch)];
        let opts = [Raw::Tuple(&pair)];
        let wanted_config = [(Raw::Atom("mode"), Raw::Atom(expected))];
        let wanted_states = [(Raw::Binary(b"queued"), Raw::Map(&wanted_config))];
        let wanted = policy(b"r", None, &wanted_states, 2)?;
        accepted(
            &invoke(Some(&stored), Raw::List(&opts), 1_000_000)?,
            1,
            2,
            &wanted,
        )?;
    }
    Ok(())
}

fn assert_resolved_states() -> TestResult {
    let queued = [(Raw::Atom("mode"), Raw::Atom("fifo"))];
    let review = [(Raw::Atom("mode"), Raw::Atom("parallel"))];
    let states = [
        (Raw::Binary(b"queued"), Raw::Map(&queued)),
        (Raw::Binary(b"review"), Raw::Map(&review)),
    ];
    let remove = [(Raw::Binary(b"queued"), Raw::Nil)];
    let pair = [Raw::Atom("states"), Raw::Map(&remove)];
    let opts = [Raw::Tuple(&pair)];
    for name in [Raw::Atom("states"), Raw::Binary(b"states")] {
        let fields = [
            (name, Raw::Map(&states)),
            (Raw::Atom("type"), Raw::Binary(b"r")),
        ];
        let stored = current::envelope(1, Raw::Map(&fields))?;
        let limits = profile(2_000_000_000)?;
        let mut charge = PolicySchemaCharge::new(limits);
        let mapping = [(1, &[0, 1][..], &[0, 1][..])];
        let result = invoke_captured(
            (b"r", Some(&stored), 1_000_000),
            Raw::List(&opts),
            &[],
            &mapping,
            (limits, &mut charge),
        )?;
        if matches!(name, Raw::Atom(_)) {
            accepted(&result, 1, 2, &policy(b"r", None, &states[1..], 2)?)?;
        } else {
            rejected(
                &result,
                Reason::StatePolicyShape,
                b"ERR flow state policy must be a map or keyword list",
            );
        }
    }
    // Both exact classes exist: atom states wins and the hidden binary key
    // stays hidden rather than being coalesced or independently normalized.
    let fields = [
        (Raw::Atom("states"), Raw::Map(&states[..1])),
        (Raw::Binary(b"states"), Raw::Map(&states[1..])),
        (Raw::Atom("type"), Raw::Binary(b"r")),
    ];
    let stored = current::envelope(1, Raw::Map(&fields))?;
    accepted(
        &invoke(Some(&stored), Raw::List(&opts), 1_000_000)?,
        1,
        2,
        &current::minimal(2)?,
    )
}

fn assert_nil_and_empty() -> TestResult {
    let queued = [(Raw::Atom("mode"), Raw::Atom("fifo"))];
    let states = [(Raw::Binary(b"queued"), Raw::Map(&queued))];
    let stored = policy(b"r", None, &states, 1)?;
    for value in [Raw::Nil, Raw::Atom("nil"), Raw::List(&[])] {
        let states = [(Raw::Binary(b"queued"), value)];
        let pair = [Raw::Atom("states"), Raw::Map(&states)];
        let opts = [Raw::Tuple(&pair)];
        let empty_config = [(Raw::Binary(b"queued"), Raw::Map(&[]))];
        let wanted = if matches!(value, Raw::List(_)) {
            policy(b"r", None, &empty_config, 2)?
        } else {
            current::minimal(2)?
        };
        accepted(
            &invoke(Some(&stored), Raw::List(&opts), 1_000_000)?,
            1,
            2,
            &wanted,
        )?;
    }
    Ok(())
}

fn assert_original_independent_patch() -> TestResult {
    // 76ce05532a4084fb/source158–189: both source keyword configs and Map
    // states are preserved; only pure installation/content is claimed here.
    let kind = b"generation-merge-1";
    let retry = [Raw::Atom("max_retries"), Raw::Integer(7)];
    let retry_opts = [Raw::Tuple(&retry)];
    let mode = [Raw::Atom("mode"), Raw::Atom("fifo")];
    let retry_pair = [Raw::Atom("retry"), Raw::List(&retry_opts)];
    let config = [Raw::Tuple(&mode), Raw::Tuple(&retry_pair)];
    let states = [(Raw::Binary(b"queued"), Raw::List(&config))];
    let active = [Raw::Atom("max_active_ms"), Raw::Integer(1_000)];
    let state_pair = [Raw::Atom("states"), Raw::Map(&states)];
    let first = [Raw::Tuple(&active), Raw::Tuple(&state_pair)];
    let normalized_retry = [(Raw::Atom("max_retries"), Raw::Integer(7))];
    let normalized = [
        (Raw::Atom("mode"), Raw::Atom("fifo")),
        (Raw::Atom("retry"), Raw::Map(&normalized_retry)),
    ];
    let expected_states = [(Raw::Binary(b"queued"), Raw::Map(&normalized))];
    let expected_first = policy(kind, Some(1_000), &expected_states, 1)?;
    let limits = profile(2_000_000_000)?;
    let mut charge = PolicySchemaCharge::new(limits);
    let first = invoke_for_type(
        kind,
        None,
        Raw::List(&first),
        1_000_000,
        limits,
        &mut charge,
    )?;
    accepted(&first, 0, 1, &expected_first)?;
    let actual = first.0.as_ref().ok_or("first original install")?.encoded();
    assert_independent_second(kind, actual)
}

fn assert_independent_second(kind: &[u8], actual: &[u8]) -> TestResult {
    let exhausted = [Raw::Atom("exhausted_to"), Raw::Binary(b"dead")];
    let retry = [Raw::Tuple(&exhausted)];
    let retry_pair = [Raw::Atom("retry"), Raw::List(&retry)];
    let queued = [Raw::Tuple(&retry_pair)];
    let mode = [Raw::Atom("mode"), Raw::Atom("fifo")];
    let review = [Raw::Tuple(&mode)];
    let states = [
        (Raw::Binary(b"queued"), Raw::List(&queued)),
        (Raw::Binary(b"review"), Raw::List(&review)),
    ];
    let pair = [Raw::Atom("states"), Raw::Map(&states)];
    let opts = [Raw::Tuple(&pair)];
    let retry = [
        (Raw::Atom("exhausted_to"), Raw::Binary(b"dead")),
        (Raw::Atom("max_retries"), Raw::Integer(7)),
    ];
    let queued = [
        (Raw::Atom("mode"), Raw::Atom("fifo")),
        (Raw::Atom("retry"), Raw::Map(&retry)),
    ];
    let review = [(Raw::Atom("mode"), Raw::Atom("fifo"))];
    let states = [
        (Raw::Binary(b"queued"), Raw::Map(&queued)),
        (Raw::Binary(b"review"), Raw::Map(&review)),
    ];
    let wanted = policy(kind, Some(1_000), &states, 2)?;
    let limits = profile(2_000_000_000)?;
    let mut charge = PolicySchemaCharge::new(limits);
    let merged = [(1, &[0, 1][..], &[0, 1][..])];
    accepted(
        &invoke_captured(
            (kind, Some(actual), 1_000_000),
            Raw::List(&opts),
            &[],
            &merged,
            (limits, &mut charge),
        )?,
        1,
        2,
        &wanted,
    )
}

fn assert_original_string_patch() -> TestResult {
    // e782ea691ad8e998/source191–215: binary mode/retry/max_retries
    // replace their existing normalized atom equivalents.
    let kind = b"generation-string-patch-1";
    let retry = [(Raw::Atom("max_retries"), Raw::Integer(7))];
    let queued = [
        (Raw::Atom("mode"), Raw::Atom("fifo")),
        (Raw::Atom("retry"), Raw::Map(&retry)),
    ];
    let old_states = [(Raw::Binary(b"queued"), Raw::Map(&queued))];
    let expected_first = policy(kind, None, &old_states, 1)?;
    let first = first_fifo(kind, None, true)?;
    accepted(&first, 0, 1, &expected_first)?;
    let actual = first.0.as_ref().ok_or("first string install")?.encoded();
    let retry = [(Raw::Binary(b"max_retries"), Raw::Integer(9))];
    let config = [
        (Raw::Binary(b"mode"), Raw::Binary(b"parallel")),
        (Raw::Binary(b"retry"), Raw::Map(&retry)),
    ];
    let states = [(Raw::Binary(b"queued"), Raw::Map(&config))];
    let pair = [Raw::Atom("states"), Raw::Map(&states)];
    let opts = [Raw::Tuple(&pair)];
    let retry = [(Raw::Atom("max_retries"), Raw::Integer(9))];
    let config = [
        (Raw::Atom("mode"), Raw::Atom("parallel")),
        (Raw::Atom("retry"), Raw::Map(&retry)),
    ];
    let states = [(Raw::Binary(b"queued"), Raw::Map(&config))];
    let wanted = policy(kind, None, &states, 2)?;
    let limits = profile(2_000_000_000)?;
    let mut charge = PolicySchemaCharge::new(limits);
    accepted(
        &invoke_for_type(
            kind,
            Some(actual),
            Raw::List(&opts),
            1_000_000,
            limits,
            &mut charge,
        )?,
        1,
        2,
        &wanted,
    )
}

fn assert_original_native_patch() -> TestResult {
    // 0f523e8d4fd45612/source217–237: actual review tuple list.
    let kind = b"generation-native-state-patch-1";
    let config = [(Raw::Atom("mode"), Raw::Atom("fifo"))];
    let states = [(Raw::Binary(b"queued"), Raw::Map(&config))];
    let expected_first = policy(kind, None, &states, 1)?;
    let first = first_fifo(kind, None, false)?;
    accepted(&first, 0, 1, &expected_first)?;
    let actual = first.0.as_ref().ok_or("first native install")?.encoded();
    let mode = [Raw::Atom("mode"), Raw::Atom("fifo")];
    let config_opts = [Raw::Tuple(&mode)];
    let state = [Raw::Binary(b"review"), Raw::List(&config_opts)];
    let pairs = [Raw::Tuple(&state)];
    let pair = [Raw::Atom("states"), Raw::List(&pairs)];
    let opts = [Raw::Tuple(&pair)];
    let states = [
        (Raw::Binary(b"queued"), Raw::Map(&config)),
        (Raw::Binary(b"review"), Raw::Map(&config)),
    ];
    let wanted = policy(kind, None, &states, 2)?;
    let merged = [(1, &[0, 1][..], &[0, 1][..])];
    let limits = profile(2_000_000_000)?;
    let mut charge = PolicySchemaCharge::new(limits);
    accepted(
        &invoke_captured(
            (kind, Some(actual), 1_000_000),
            Raw::List(&opts),
            &[],
            &merged,
            (limits, &mut charge),
        )?,
        1,
        2,
        &wanted,
    )
}

fn assert_original_replace() -> TestResult {
    // 17d8471407b0c04a/source320–330: complete replacement removes max
    // and queued. The public nil projection is still a required successor.
    let kind = b"generation-replace-1";
    let config = [(Raw::Atom("mode"), Raw::Atom("fifo"))];
    let states = [(Raw::Binary(b"queued"), Raw::Map(&config))];
    let expected_first = policy(kind, Some(1_000), &states, 1)?;
    let first = first_fifo(kind, Some(1_000), false)?;
    accepted(&first, 0, 1, &expected_first)?;
    let actual = first.0.as_ref().ok_or("first replace install")?.encoded();
    let pair = [Raw::Atom("replace"), Raw::Atom("true")];
    let opts = [Raw::Tuple(&pair)];
    let wanted = policy(kind, None, &[], 2)?;
    let limits = profile(2_000_000_000)?;
    let mut charge = PolicySchemaCharge::new(limits);
    accepted(
        &invoke_for_type(
            kind,
            Some(actual),
            Raw::List(&opts),
            1_000_000,
            limits,
            &mut charge,
        )?,
        1,
        2,
        &wanted,
    )
}

fn assert_captured_history() -> TestResult {
    let retention = [(Raw::Atom("history_max_events"), Raw::Integer(50))];
    let fields = [
        (Raw::Atom("retention"), Raw::Map(&retention)),
        (Raw::Atom("states"), Raw::Map(&[])),
        (Raw::Atom("type"), Raw::Binary(b"r")),
    ];
    let actual = current::envelope(1, Raw::Map(&fields))?;
    let still_fifty = current::envelope(2, Raw::Map(&fields))?;
    accepted(
        &invoke(Some(&actual), Raw::List(&[]), 100)?,
        1,
        2,
        &still_fifty,
    )?;
    let body = b"ERR flow retention history_max_events must be between 1 and 10";
    rejected(
        &invoke(Some(&actual), Raw::List(&[]), 10)?,
        Reason::Retention(Retention::History { maximum: 10 }),
        body,
    );
    let x = [Raw::Atom("x"), Raw::Integer(1)];
    rejected(
        &invoke(Some(&actual), Raw::List(&[Raw::Tuple(&x)]), 10)?,
        Reason::Retention(Retention::History { maximum: 10 }),
        body,
    );
    let retention = [(Raw::Atom("history_max_events"), Raw::Integer(10))];
    let pair = [Raw::Atom("retention"), Raw::Map(&retention)];
    let opts = [Raw::Tuple(&pair)];
    let fields = [
        (Raw::Atom("retention"), Raw::Map(&retention)),
        (Raw::Atom("states"), Raw::Map(&[])),
        (Raw::Atom("type"), Raw::Binary(b"r")),
    ];
    let wanted = current::envelope(2, Raw::Map(&fields))?;
    accepted(&invoke(Some(&actual), Raw::List(&opts), 10)?, 1, 2, &wanted)?;
    let replace = [Raw::Atom("replace"), Raw::Atom("true")];
    accepted(
        &invoke(Some(&actual), Raw::List(&[Raw::Tuple(&replace)]), 10)?,
        1,
        2,
        &current::minimal(2)?,
    )
}

fn first_fifo(
    kind: &[u8],
    maximum: Option<i128>,
    with_retry: bool,
) -> TestResult<super::OriginalPolicySetFresh> {
    let mode = [Raw::Atom("mode"), Raw::Atom("fifo")];
    let retries = [Raw::Atom("max_retries"), Raw::Integer(7)];
    let retry_opts = [Raw::Tuple(&retries)];
    let retry = [Raw::Atom("retry"), Raw::List(&retry_opts)];
    let mut config = vec![Raw::Tuple(&mode)];
    if with_retry {
        config.push(Raw::Tuple(&retry));
    }
    let states = [(Raw::Binary(b"queued"), Raw::List(&config))];
    let states_pair = [Raw::Atom("states"), Raw::Map(&states)];
    let active = [
        Raw::Atom("max_active_ms"),
        Raw::Integer(maximum.unwrap_or(1)),
    ];
    let mut opts = vec![Raw::Tuple(&states_pair)];
    if maximum.is_some() {
        opts.push(Raw::Tuple(&active));
    }
    let limits = profile(2_000_000_000)?;
    let mut charge = PolicySchemaCharge::new(limits);
    invoke_for_type(kind, None, Raw::List(&opts), 1_000_000, limits, &mut charge)
}

fn policy(
    kind: &[u8],
    maximum: Option<i128>,
    states: &[(Raw<'_>, Raw<'_>)],
    generation: u64,
) -> TestResult<Vec<u8>> {
    let mut fields = Vec::new();
    if let Some(number) = maximum {
        fields.push((Raw::Atom("max_active_ms"), Raw::Integer(number)));
    }
    fields.push((Raw::Atom("states"), Raw::Map(states)));
    fields.push((Raw::Atom("type"), Raw::Binary(kind)));
    current::envelope(generation, Raw::Map(&fields))
}
