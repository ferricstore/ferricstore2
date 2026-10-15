//! Bound source traversal, trivial-map shortcuts and mathematical fixture rows.

use super::{
    EncodingError, ExistingError, OriginalPolicySetPatchView as View, PolicySchemaCharge,
    PolicySchemaError, Raw, RawError, Reason, RequestError, TestResult, accepted, current,
    invoke_captured, profile, rejected,
};

#[test]
fn original_public_merge_uses_server_bound_source_traversal() -> TestResult {
    assert_current_hamt_order()?;
    assert_trivial_and_missing()?;
    assert_struct_boundary()?;
    Ok(())
}

fn assert_current_hamt_order() -> TestResult {
    // Pinned OTP28.3.1/Elixir1.19.5 optimized64LE reduce/maps.to_list order,
    // independently captured before the previous flow implementation existed.
    // ETF/key-index order remains s01..s33; these facts are not inferred from it.
    let forward = [
        0, 2, 21, 18, 5, 22, 15, 26, 4, 20, 6, 32, 31, 27, 23, 10, 9, 19, 30, 14, 17, 1, 13, 29, 8,
        24, 3, 25, 11, 7, 16, 12, 28,
    ];
    let inverse = inverse(&forward)?;
    let names = (1..=33).map(|n| format!("s{n:02}")).collect::<Vec<_>>();
    for pair in forward.windows(2) {
        for swap in [false, true] {
            let earlier = usize::try_from(pair[0])?;
            let later = usize::try_from(pair[1])?;
            let mut configs = vec![Vec::new(); 33];
            configs[earlier].push(if swap {
                (Raw::Atom("max_active_ms"), Raw::Integer(1))
            } else {
                (Raw::Atom("mode"), Raw::Integer(7))
            });
            configs[later].push(if swap {
                (Raw::Atom("mode"), Raw::Integer(7))
            } else {
                (Raw::Atom("max_active_ms"), Raw::Integer(1))
            });
            let states = names
                .iter()
                .zip(&configs)
                .map(|(name, config)| (Raw::Binary(name.as_bytes()), Raw::Map(config)))
                .collect::<Vec<_>>();
            let fields = [
                (Raw::Atom("states"), Raw::Map(&states)),
                (Raw::Atom("type"), Raw::Binary(b"r")),
            ];
            let bytes = current::envelope(1, Raw::Map(&fields))?;
            assert_order_paths(&bytes, &states, (&forward, &inverse), swap)?;
        }
    }
    Ok(())
}

fn assert_order_paths(
    bytes: &[u8],
    states: &[(Raw<'_>, Raw<'_>)],
    order: (&[u32], &[u32]),
    swap: bool,
) -> TestResult {
    let native = order
        .0
        .iter()
        .map(|&rank| usize::try_from(rank).map(|i| states[i]))
        .collect::<Result<Vec<_>, _>>()?;
    let frame_pair = [Raw::Atom("states"), Raw::Map(&native)];
    let frame_opts = [Raw::Tuple(&frame_pair)];
    let no_op = [(Raw::Binary(b"s01"), Raw::Map(&[]))];
    let merged_pair = [Raw::Atom("states"), Raw::Map(&no_op)];
    let merged_opts = [Raw::Tuple(&merged_pair)];
    let current_records = [(5, order.0, order.1)];
    let merged_records = [(1, order.0, order.1)];
    let limits = profile(2_000_000_000)?;
    for path in 0..3 {
        let opts = match path {
            0 => Raw::List(&[]),
            1 => Raw::List(&frame_opts),
            _ => Raw::List(&merged_opts),
        };
        let mut charge = PolicySchemaCharge::new(limits);
        let result = invoke_captured(
            (b"r", (path != 1).then_some(bytes), 1_000_000),
            opts,
            if path == 0 { &current_records } else { &[] },
            if path == 2 { &merged_records } else { &[] },
            (limits, &mut charge),
        )?;
        let (reason, body) = if swap {
            (
                Reason::StateMaxActive,
                b"ERR flow max_active_ms is type-level only".as_slice(),
            )
        } else {
            (
                Reason::StateMode,
                b"ERR flow state mode must be parallel or fifo".as_slice(),
            )
        };
        rejected(&result, reason, body);
    }
    Ok(())
}

fn assert_trivial_and_missing() -> TestResult {
    let empty_fields = [
        (Raw::Atom("states"), Raw::Map(&[])),
        (Raw::Atom("type"), Raw::Binary(b"r")),
    ];
    let bytes = current::envelope(1, Raw::Map(&empty_fields))?;
    let poison = [(5, &[99][..], &[99][..])];
    let limits = profile(2_000_000_000)?;
    let mut charge = PolicySchemaCharge::new(limits);
    let expected = current::minimal(2)?;
    accepted(
        &invoke_captured(
            (b"r", Some(&bytes), 1_000_000),
            Raw::List(&[]),
            &poison,
            &[],
            (limits, &mut charge),
        )?,
        1,
        2,
        &expected,
    )?;
    let states = [
        (Raw::Binary(b"a"), Raw::Map(&[])),
        (Raw::Binary(b"b"), Raw::Map(&[])),
    ];
    let fields = [
        (Raw::Atom("states"), Raw::Map(&states)),
        (Raw::Atom("type"), Raw::Binary(b"r")),
    ];
    let bytes = current::envelope(1, Raw::Map(&fields))?;
    for records in [&[][..], &[(5, &[0, 0][..], &[0, 1][..])][..]] {
        let mut charge = PolicySchemaCharge::new(limits);
        assert_eq!(
            invoke_captured(
                (b"r", Some(&bytes), 1_000_000),
                Raw::List(&[]),
                records,
                &[],
                (limits, &mut charge)
            )
            .err()
            .and_then(|error| error.downcast::<RawError>().ok())
            .map(|e| *e),
            Some(authority()),
        );
    }
    let replace = [Raw::Atom("replace"), Raw::Atom("true")];
    let mut charge = PolicySchemaCharge::new(limits);
    accepted(
        &invoke_captured(
            (b"r", Some(&bytes), 1_000_000),
            Raw::List(&[Raw::Tuple(&replace)]),
            &poison,
            &poison,
            (limits, &mut charge),
        )?,
        1,
        2,
        &expected,
    )
}

fn assert_struct_boundary() -> TestResult {
    let structure = [(Raw::Atom("__struct__"), Raw::Atom("Elixir.Dummy"))];
    let fields = [
        (Raw::Atom("states"), Raw::Map(&structure)),
        (Raw::Atom("type"), Raw::Binary(b"r")),
    ];
    let bytes = current::envelope(1, Raw::Map(&fields))?;
    let limits = profile(2_000_000_000)?;
    let mut charge = PolicySchemaCharge::new(limits);
    assert_eq!(
        invoke_captured(
            (b"r", Some(&bytes), 1_000_000),
            Raw::List(&[]),
            &[],
            &[],
            (limits, &mut charge)
        )
        .err()
        .and_then(|error| error.downcast::<RawError>().ok())
        .map(|e| *e),
        Some(authority()),
    );
    // True replacement discards the unsupported current map before Enum.
    let replace = [Raw::Atom("replace"), Raw::Atom("true")];
    let mut charge = PolicySchemaCharge::new(limits);
    accepted(
        &invoke_captured(
            (b"r", Some(&bytes), 1_000_000),
            Raw::List(&[Raw::Tuple(&replace)]),
            &[],
            &[],
            (limits, &mut charge),
        )?,
        1,
        2,
        &current::minimal(2)?,
    )?;
    assert_left_struct_exact_hit()
}

fn assert_left_struct_exact_hit() -> TestResult {
    let config = [
        (Raw::Atom("__struct__"), Raw::Atom("Elixir.Dummy")),
        (Raw::Atom("mode"), Raw::Atom("fifo")),
    ];
    let states = [(Raw::Binary(b"queued"), Raw::Map(&config))];
    let fields = [
        (Raw::Atom("states"), Raw::Map(&states)),
        (Raw::Atom("type"), Raw::Binary(b"r")),
    ];
    let bytes = current::envelope(1, Raw::Map(&fields))?;
    let wanted_config = [(Raw::Atom("mode"), Raw::Atom("parallel"))];
    let wanted_states = [(Raw::Binary(b"queued"), Raw::Map(&wanted_config))];
    let wanted_fields = [
        (Raw::Atom("states"), Raw::Map(&wanted_states)),
        (Raw::Atom("type"), Raw::Binary(b"r")),
    ];
    let expected = current::envelope(2, Raw::Map(&wanted_fields))?;
    for key in [Raw::Atom("mode"), Raw::Binary(b"mode")] {
        let patch = [(key, Raw::Atom("parallel"))];
        let states = [(Raw::Binary(b"queued"), Raw::Map(&patch))];
        let pair = [Raw::Atom("states"), Raw::Map(&states)];
        let result = super::invoke(Some(&bytes), Raw::List(&[Raw::Tuple(&pair)]), 1_000_000);
        if matches!(key, Raw::Atom(_)) {
            accepted(&result?, 1, 2, &expected)?;
        } else {
            assert_eq!(
                result
                    .err()
                    .and_then(|e| e.downcast::<RawError>().ok())
                    .map(|e| *e),
                Some(authority())
            );
        }
    }
    Ok(())
}

fn authority() -> RawError {
    RawError::Replace(ExistingError::Request(RequestError::Encoding(
        EncodingError::Schema(PolicySchemaError::InvalidAuthority),
    )))
}

pub(super) fn inverse(forward: &[u32]) -> TestResult<Vec<u32>> {
    let mut inverse = vec![u32::MAX; forward.len()];
    for (source, &rank) in forward.iter().enumerate() {
        *inverse
            .get_mut(usize::try_from(rank)?)
            .ok_or("rank fixture")? = u32::try_from(source)?;
    }
    Ok(inverse)
}

pub(super) fn patch_records(
    value: View<'_>,
    out: &mut Vec<(u32, Vec<u32>, Vec<u32>)>,
) -> TestResult {
    view_maps(value, out)
}

fn view_maps(value: View<'_>, out: &mut Vec<(u32, Vec<u32>, Vec<u32>)>) -> TestResult {
    if let Some(count) = value.map_len() {
        let rows = (0..count)
            .map(|i| value.map_entry(i).ok_or("patch map fixture"))
            .collect::<Result<Vec<_>, _>>()?;
        let mut sorted = (0..rows.len()).collect::<Vec<_>>();
        if rows.len() > 1 {
            let keys = rows
                .iter()
                .map(|(k, _)| key(k.raw().ok_or("raw key fixture")?))
                .collect::<TestResult<Vec<_>>>()?;
            sorted.sort_by(|&a, &b| keys[a].cmp(&keys[b]));
        }
        record(&sorted, out)?;
        for index in sorted {
            view_maps(rows[index].0, out)?;
            view_maps(rows[index].1, out)?;
        }
    } else if let Some(count) = value.list_len() {
        for index in 0..count {
            view_maps(value.list_item(index).ok_or("patch list fixture")?, out)?;
        }
    } else {
        raw_maps(value.raw().ok_or("patch raw fixture")?, out)?;
    }
    Ok(())
}

fn raw_maps(value: Raw<'_>, out: &mut Vec<(u32, Vec<u32>, Vec<u32>)>) -> TestResult {
    match value {
        Raw::Map(rows) => {
            let mut sorted = (0..rows.len()).collect::<Vec<_>>();
            if rows.len() > 1 {
                let keys = rows
                    .iter()
                    .map(|(k, _)| key(*k))
                    .collect::<TestResult<Vec<_>>>()?;
                sorted.sort_by(|&a, &b| keys[a].cmp(&keys[b]));
            }
            record(&sorted, out)?;
            for index in sorted {
                raw_maps(rows[index].0, out)?;
                raw_maps(rows[index].1, out)?;
            }
        }
        Raw::List(items) | Raw::Tuple(items) => {
            for &item in items {
                raw_maps(item, out)?;
            }
        }
        Raw::ImproperList { prefix, tail } => {
            for &item in prefix {
                raw_maps(item, out)?;
            }
            raw_maps(*tail, out)?;
        }
        _ => {}
    }
    Ok(())
}

fn record(sorted: &[usize], out: &mut Vec<(u32, Vec<u32>, Vec<u32>)>) -> TestResult {
    let q = sorted
        .iter()
        .copied()
        .map(u32::try_from)
        .collect::<Result<Vec<_>, _>>()?;
    let p = inverse(&q)?;
    // These are explicit conditional source-order fixture facts, not a proof
    // that prepared logical atom order derives the runtime atom index.
    out.push((u32::try_from(out.len())?, p, q));
    Ok(())
}

fn key(value: Raw<'_>) -> TestResult<(u8, Vec<u8>)> {
    match value {
        Raw::Nil => Ok((1, b"nil".to_vec())),
        Raw::Atom(name) => Ok((1, name.as_bytes().to_vec())),
        Raw::Binary(bytes) => Ok((2, bytes.to_vec())),
        _ => Err("multi-key fixture needs explicit canonical ordering".into()),
    }
}
