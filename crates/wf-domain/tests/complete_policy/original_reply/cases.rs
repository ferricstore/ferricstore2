//! Generic decoded terms, exact map overlays and eager source exceptions.

use super::{
    EncodingError, PolicySchemaCharge, ReplyError, TestResult, accepted, atom, big, binary,
    context, envelope, field, integer, list, map, profile, set_reply, tuple,
};

#[test]
fn original_reply_retains_generic_values_and_shared_exact_key_overlays() -> TestResult {
    assert_raw_versions()?;
    assert_signed_max_active()?;
    assert_retry_and_retention()?;
    assert_governance_truthiness()?;
    Ok(())
}

#[test]
fn original_reply_preserves_enumerable_callbacks_and_independent_indexed_fallbacks() -> TestResult {
    assert_indexed_fields()?;
    assert_list_states()?;
    assert_source_exceptions()?;
    assert_struct_boundaries()?;
    assert_cons_windows()?;
    Ok(())
}

fn assert_raw_versions() -> TestResult {
    let mut float = vec![70];
    float.extend_from_slice(&1.25_f64.to_be_bytes());
    let mut export = vec![113];
    export.extend(atom("erlang")?);
    export.extend(atom("length")?);
    export.extend(integer(1)?);
    for version in [
        float.clone(),
        tuple(&[atom("opaque")?, binary(b"v")?])?,
        map(&[(atom("__struct__")?, atom("Elixir.Dummy")?)])?,
        export,
        vec![77, 0, 0, 0, 1, 3, 0b1010_0000],
    ] {
        let bytes = policy(&[(atom("version")?, version.clone())])?;
        let limits = profile(1_000_000_000)?;
        let mut charge = PolicySchemaCharge::new(limits);
        let result = accepted(set_reply(
            b"r",
            &bytes,
            None,
            context(),
            limits,
            &mut charge,
        ))?;
        let value = field(result.root(), "version", &mut charge)?;
        assert_eq!(value.original_encoding(), Some(version.as_slice()));
        if version == float {
            assert_eq!(value.float(), Some(1.25));
        }
        if version.first() == Some(&77) {
            assert_eq!(value.bitstring(), Some((&[0b1010_0000][..], 3)));
            assert!(value.binary_bytes().is_none());
        }
    }
    Ok(())
}

fn assert_signed_max_active() -> TestResult {
    let mut magnitude = vec![0; 20];
    magnitude[19] = 1;
    for number in [integer(-7)?, big(true, &magnitude)?] {
        let bytes = policy(&[(atom("max_active_ms")?, number.clone())])?;
        let limits = profile(1_000_000_000)?;
        let mut charge = PolicySchemaCharge::new(limits);
        let result = accepted(set_reply(
            b"r",
            &bytes,
            Some(b"queued"),
            context(),
            limits,
            &mut charge,
        ))?;
        let actual = field(result.root(), "max_active_ms", &mut charge)?;
        assert_eq!(actual.original_encoding(), Some(number.as_slice()));
        if number.first() == Some(&98) {
            assert_eq!(actual.integer(), Some(-7));
        } else {
            assert_eq!(actual.big_integer(), Some((true, magnitude.as_slice())));
        }
    }
    let bytes = policy(&[(atom("max_active_ms")?, binary(b"infinity")?)])?;
    let limits = profile(1_000_000_000)?;
    let mut charge = PolicySchemaCharge::new(limits);
    let result = accepted(set_reply(
        b"r",
        &bytes,
        None,
        context(),
        limits,
        &mut charge,
    ))?;
    assert_eq!(
        field(result.root(), "max_active_ms", &mut charge)?.atom_bytes(),
        Some(&b"nil"[..])
    );
    Ok(())
}

fn assert_retry_and_retention() -> TestResult {
    let type_backoff = map(&[
        (atom("base_ms")?, integer(300)?),
        (binary(b"base_ms")?, integer(400)?),
        (atom("x")?, tuple(&[integer(1)?, integer(2)?])?),
        (atom("__struct__")?, atom("Elixir.Dummy")?),
    ])?;
    let local_backoff = map(&[
        (atom("base_ms")?, integer(-5)?),
        (binary(b"x")?, atom("false")?),
        (atom("kind")?, atom("nil")?),
    ])?;
    let type_retry = map(&[
        (atom("backoff")?, type_backoff),
        (atom("max_retries")?, binary(b"raw retry")?),
        (atom("exhausted_to")?, list(&[integer(9)?], None)?),
    ])?;
    let local_retry = map(&[(atom("backoff")?, local_backoff)])?;
    let retention = map(&[
        (atom("ttl_ms")?, tuple(&[atom("opaque")?, integer(-1)?])?),
        (
            atom("history_max_events")?,
            binary(b"unbounded source value")?,
        ),
        (atom("history_hot_max_events")?, map(&[])?),
    ])?;
    let local = map(&[(atom("retry")?, local_retry)])?;
    let bytes = policy(&[
        (atom("retry")?, type_retry),
        (atom("retention")?, retention),
        (atom("states")?, map(&[(binary(b"queued")?, local)])?),
    ])?;
    let limits = profile(1_000_000_000)?;
    let mut charge = PolicySchemaCharge::new(limits);
    let result = accepted(set_reply(
        b"r",
        &bytes,
        Some(b"queued"),
        context(),
        limits,
        &mut charge,
    ))?;
    let retry = field(result.root(), "retry", &mut charge)?;
    assert_eq!(
        field(retry, "max_retries", &mut charge)?.binary_bytes(),
        Some(&b"raw retry"[..])
    );
    assert_eq!(
        field(retry, "exhausted_to", &mut charge)?
            .cons()
            .ok_or("target list")?
            .0
            .integer(),
        Some(9)
    );
    let backoff = field(retry, "backoff", &mut charge)?;
    assert_eq!(backoff.map_len(), Some(8));
    assert_eq!(field(backoff, "base_ms", &mut charge)?.integer(), Some(-5));
    assert_eq!(
        field(backoff, "kind", &mut charge)?.atom_bytes(),
        Some(&b"nil"[..])
    );
    assert_eq!(field(backoff, "x", &mut charge)?.tuple_len(), Some(2));
    let mut rows = backoff.map_iter().ok_or("backoff iterator")?;
    let mut observed = (0, false, false);
    while let Some((key, value)) = rows.next(&mut charge)? {
        observed.0 += 1;
        if key.binary_bytes() == Some(&b"base_ms"[..]) {
            observed.1 = value.integer() == Some(400);
        }
        if key.binary_bytes() == Some(&b"x"[..]) {
            observed.2 = value.atom_bytes() == Some(&b"false"[..]);
        }
    }
    assert_eq!(observed, (8, true, true));
    let retention = field(result.root(), "retention", &mut charge)?;
    assert_eq!(
        field(retention, "ttl_ms", &mut charge)?.tuple_len(),
        Some(2)
    );
    assert_eq!(
        field(retention, "history_max_events", &mut charge)?.binary_bytes(),
        Some(&b"unbounded source value"[..])
    );
    assert!(
        retention
            .atom_field("history_hot_max_events", &mut charge)?
            .is_none()
    );
    Ok(())
}

fn assert_governance_truthiness() -> TestResult {
    for (governance, fallback) in [
        (atom("nil")?, true),
        (atom("false")?, true),
        (list(&[], None)?, false),
        (integer(0)?, false),
        (map(&[])?, false),
    ] {
        let config = map(&[(atom("governance")?, governance.clone())])?;
        let bytes = policy(&[
            (atom("governance")?, map(&[(atom("x")?, integer(7)?)])?),
            (atom("states")?, map(&[(binary(b"queued")?, config)])?),
        ])?;
        let limits = profile(1_000_000_000)?;
        let mut charge = PolicySchemaCharge::new(limits);
        let all = accepted(set_reply(
            b"r",
            &bytes,
            None,
            context(),
            limits,
            &mut charge,
        ))?;
        let states = field(all.root(), "states", &mut charge)?;
        let row = states
            .map_iter()
            .ok_or("states iterator")?
            .next(&mut charge)?
            .ok_or("state row")?
            .1;
        assert_eq!(
            field(row, "governance", &mut charge)?.original_encoding(),
            Some(governance.as_slice())
        );
        drop(all);
        let selected = accepted(set_reply(
            b"r",
            &bytes,
            Some(b"queued"),
            context(),
            limits,
            &mut charge,
        ))?;
        let actual = field(selected.root(), "governance", &mut charge)?;
        if fallback {
            assert_eq!(field(actual, "x", &mut charge)?.integer(), Some(7));
        } else {
            assert_eq!(actual.original_encoding(), Some(governance.as_slice()));
        }
    }
    Ok(())
}

fn assert_indexed_fields() -> TestResult {
    for attributes in [integer(7)?, list(&[integer(7)?], None)?] {
        let bytes = policy(&[
            (atom("indexed_attributes")?, attributes),
            (atom("indexed_state_meta")?, binary(b" meta ")?),
        ])?;
        let limits = profile(1_000_000_000)?;
        let mut charge = PolicySchemaCharge::new(limits);
        let value = accepted(set_reply(
            b"r",
            &bytes,
            None,
            context(),
            limits,
            &mut charge,
        ))?;
        assert!(field(value.root(), "indexed_attributes", &mut charge)?.is_empty_list());
        assert_eq!(
            field(value.root(), "indexed_state_meta", &mut charge)?.binary_bytes(),
            Some(&b"meta"[..])
        );
    }
    let bytes = policy(&[
        (
            atom("indexed_attributes")?,
            list(&[binary(b" customer ")?, atom("x")?, binary(b"x")?], None)?,
        ),
        (
            atom("indexed_state_meta")?,
            list(&[binary(b"a")?, binary(b"b")?], None)?,
        ),
    ])?;
    let limits = profile(1_000_000_000)?;
    let mut charge = PolicySchemaCharge::new(limits);
    let result = accepted(set_reply(
        b"r",
        &bytes,
        None,
        context(),
        limits,
        &mut charge,
    ))?;
    let names = field(result.root(), "indexed_attributes", &mut charge)?;
    let (first, rest) = names.cons().ok_or("first indexed name")?;
    let (second, end) = rest.cons().ok_or("second indexed name")?;
    assert_eq!(first.binary_bytes(), Some(&b"customer"[..]));
    assert_eq!(second.binary_bytes(), Some(&b"x"[..]));
    assert!(end.is_empty_list());
    assert_eq!(
        field(result.root(), "indexed_state_meta", &mut charge)?.atom_bytes(),
        Some(&b"nil"[..])
    );
    Ok(())
}

fn assert_list_states() -> TestResult {
    let fifo = map(&[(atom("mode")?, atom("fifo")?)])?;
    let padded = big(false, &[1, 0, 0])?;
    let pairs = list(
        &[
            tuple(&[binary(b"queued")?, fifo.clone()])?,
            tuple(&[integer(1)?, map(&[])?])?,
            tuple(&[
                padded,
                map(&[(atom("__struct__")?, atom("Elixir.Dummy")?)])?,
            ])?,
        ],
        None,
    )?;
    let bytes = policy(&[(atom("states")?, pairs)])?;
    let limits = profile(1_000_000_000)?;
    let mut charge = PolicySchemaCharge::new(limits);
    let result = accepted(set_reply(
        b"r",
        &bytes,
        None,
        context(),
        limits,
        &mut charge,
    ))?;
    let states = field(result.root(), "states", &mut charge)?;
    assert_eq!(states.map_len(), Some(2));
    let mut rows = states.map_iter().ok_or("list states iterator")?;
    let mut count = 0;
    while let Some((_, row)) = rows.next(&mut charge)? {
        count += 1;
        assert_eq!(
            field(row, "mode", &mut charge)?.atom_bytes(),
            Some(&b"parallel"[..])
        );
    }
    assert_eq!(count, 2);
    drop(result);
    for mode in [binary(b"fifo")?, atom("fifo")?] {
        let cfg = map(&[(atom("mode")?, mode.clone())])?;
        let bytes = policy(&[(atom("states")?, map(&[(binary(b"queued")?, cfg)])?)])?;
        let result = accepted(set_reply(
            b"r",
            &bytes,
            Some(b"queued"),
            context(),
            limits,
            &mut charge,
        ))?;
        let expected = if mode.first() == Some(&109) {
            &b"parallel"[..]
        } else {
            &b"fifo"[..]
        };
        assert_eq!(
            field(result.root(), "mode", &mut charge)?.atom_bytes(),
            Some(expected)
        );
    }
    let cfg = map(&[(binary(b"mode")?, atom("fifo")?)])?;
    let bytes = policy(&[(atom("states")?, map(&[(binary(b"queued")?, cfg)])?)])?;
    let result = accepted(set_reply(
        b"r",
        &bytes,
        Some(b"queued"),
        context(),
        limits,
        &mut charge,
    ))?;
    assert_eq!(
        field(result.root(), "mode", &mut charge)?.atom_bytes(),
        Some(&b"parallel"[..])
    );
    Ok(())
}

fn assert_source_exceptions() -> TestResult {
    let valid = tuple(&[binary(b"queued")?, map(&[])?])?;
    for states in [
        list(&[valid.clone(), integer(7)?], None)?,
        list(&[valid], Some(&integer(7)?))?,
        vec![107, 0, 1, 7],
    ] {
        let bytes = policy(&[(atom("states")?, states)])?;
        assert_error(&bytes, ReplyError::FunctionClause)?;
    }
    let bytes = policy(&[(atom("states")?, atom("nil")?)])?;
    assert_error(&bytes, ReplyError::Enumerable)?;
    let limits = profile(1_000_000_000)?;
    let mut charge = PolicySchemaCharge::new(limits);
    let selected = accepted(set_reply(
        b"r",
        &bytes,
        Some(b"q"),
        context(),
        limits,
        &mut charge,
    ))?;
    assert_eq!(
        field(selected.root(), "mode", &mut charge)?.atom_bytes(),
        Some(&b"parallel"[..])
    );
    let improper = list(&[binary(b"a")?], Some(&integer(7)?))?;
    for name in ["indexed_attributes", "indexed_state_meta"] {
        let bytes = policy(&[(atom(name)?, improper.clone())])?;
        assert_error(&bytes, ReplyError::FunctionClause)?;
    }
    Ok(())
}

fn assert_struct_boundaries() -> TestResult {
    let structure = map(&[(atom("__struct__")?, atom("Elixir.Dummy")?)])?;
    for name in ["retry", "retention", "states"] {
        let bytes = policy(&[(atom(name)?, structure.clone())])?;
        assert_error(&bytes, ReplyError::Encoding(EncodingError::UnsupportedTag))?;
        if name == "states" {
            let limits = profile(1_000_000_000)?;
            let mut charge = PolicySchemaCharge::new(limits);
            let value = accepted(set_reply(
                b"r",
                &bytes,
                Some(b"q"),
                context(),
                limits,
                &mut charge,
            ))?;
            assert_eq!(
                field(value.root(), "mode", &mut charge)?.atom_bytes(),
                Some(&b"parallel"[..])
            );
        }
    }
    for marker in [
        (atom("__struct__")?, integer(0)?),
        (binary(b"__struct__")?, atom("Elixir.Dummy")?),
    ] {
        let atom_key = marker.0 == atom("__struct__")?;
        let bytes = policy(&[(atom("states")?, map(&[marker])?)])?;
        let limits = profile(1_000_000_000)?;
        let mut charge = PolicySchemaCharge::new(limits);
        let result = accepted(set_reply(
            b"r",
            &bytes,
            None,
            context(),
            limits,
            &mut charge,
        ))?;
        assert_eq!(
            field(result.root(), "states", &mut charge)?.map_len(),
            Some(1)
        );
        let row =
            field(result.root(), "states", &mut charge)?.atom_field("__struct__", &mut charge)?;
        assert_eq!(row.is_some(), atom_key);
        if let Some(row) = row {
            assert_eq!(
                field(row, "mode", &mut charge)?.atom_bytes(),
                Some(&b"parallel"[..])
            );
        }
    }
    Ok(())
}

fn assert_cons_windows() -> TestResult {
    let two = [integer(1)?, integer(2)?];
    for (version, proper) in [
        (list(&two, None)?, true),
        (list(&two, Some(&binary(b"tail")?))?, false),
        (vec![107, 0, 2, 1, 2], true),
    ] {
        let bytes = policy(&[(atom("version")?, version)])?;
        let limits = profile(1_000_000_000)?;
        let mut charge = PolicySchemaCharge::new(limits);
        let result = accepted(set_reply(
            b"r",
            &bytes,
            None,
            context(),
            limits,
            &mut charge,
        ))?;
        let value = field(result.root(), "version", &mut charge)?;
        let (one, rest) = value.cons().ok_or("first cons")?;
        let (two, tail) = rest.cons().ok_or("second cons")?;
        assert_eq!((one.integer(), two.integer()), (Some(1), Some(2)));
        assert!(rest.original_encoding().is_none());
        if proper {
            assert!(tail.is_empty_list());
        } else {
            assert_eq!(tail.binary_bytes(), Some(&b"tail"[..]));
        }
    }
    let alias = list(&[], Some(&binary(b"v")?))?;
    let bytes = policy(&[(atom("version")?, alias)])?;
    let limits = profile(1_000_000_000)?;
    let mut charge = PolicySchemaCharge::new(limits);
    let result = accepted(set_reply(
        b"r",
        &bytes,
        None,
        context(),
        limits,
        &mut charge,
    ))?;
    assert_eq!(
        field(result.root(), "version", &mut charge)?.binary_bytes(),
        Some(&b"v"[..])
    );
    Ok(())
}

fn assert_error(bytes: &[u8], expected: ReplyError) -> TestResult {
    let limits = profile(1_000_000_000)?;
    let mut charge = PolicySchemaCharge::new(limits);
    let result = set_reply(b"r", bytes, None, context(), limits, &mut charge);
    assert!(matches!(result, Err(actual) if actual == expected));
    Ok(())
}

fn policy(extra: &[(Vec<u8>, Vec<u8>)]) -> TestResult<Vec<u8>> {
    let mut fields = vec![(atom("type")?, binary(b"r")?)];
    let states = atom("states")?;
    if !extra.iter().any(|(key, _)| key == &states) {
        fields.push((atom("states")?, map(&[])?));
    }
    fields.extend_from_slice(extra);
    envelope(1, &map(&fields)?)
}
