//! Independent aggregate ceilings and borrowed final leaves.
use super::*;

#[test]
fn original_create_preserves_inclusive_output_work_and_temporary_boundaries() -> TestResult {
    let workflow_type = b"email".to_vec();
    let attrs = [(I::Atom("flag"), I::Boolean(false))];
    let pairs = [
        [I::Atom("type"), I::Binary(&workflow_type)],
        [I::Atom("attributes"), I::Map(&attrs)],
        [I::Atom("partition_key"), I::Binary(b"p")],
    ];
    let opts = pairs.iter().map(|p| I::Tuple(p)).collect::<Vec<_>>();
    let Outcome::Accepted {
        value,
        work_units,
        temporary_bytes,
    } = prepare_original_create(
        I::Binary(b"i"),
        I::List(&opts),
        OriginalCreateContext::default(),
        limits(),
    )?
    else {
        return Err("resource baseline".into());
    };
    let used = [value.owned_bytes(), work_units, temporary_bytes];
    for (dimension, amount) in used.into_iter().enumerate() {
        for (at, success) in [(0, false), (amount - 1, false), (amount, true)] {
            let mut cap = limits();
            match dimension {
                0 => cap.maximum_output_bytes = at,
                1 => cap.maximum_work_units = at,
                _ => cap.maximum_temporary_bytes = at,
            }
            let result = prepare_original_create(
                I::Binary(b"i"),
                I::List(&opts),
                OriginalCreateContext::default(),
                cap,
            );
            if success {
                assert!(
                    matches!(result, Ok(Outcome::Accepted { .. })),
                    "{dimension} at {at}: {result:?}"
                );
            } else {
                assert!(result.is_err(), "{dimension} at {at}: {result:?}");
            }
        }
    }
    assert!(matches!(
        value.field("partition_key"),
        Some(I::Binary(b"p"))
    ));
    assert!(matches!(value.field("type"),Some(I::Binary(v)) if v.as_ptr()==workflow_type.as_ptr()));
    Ok(())
}

#[test]
fn original_create_counts_shadowed_and_ignored_input_and_measures_refusals() -> TestResult {
    let ignored = vec![0; 20_000];
    let pairs = [
        [I::Atom("type"), I::Binary(b"email")],
        [I::Atom("type"), I::Binary(&ignored)],
        [I::Atom("unknown"), I::Binary(&ignored)],
    ];
    let opts = pairs.iter().map(|p| I::Tuple(p)).collect::<Vec<_>>();
    let mut cap = limits();
    cap.maximum_input_bytes = 30_000;
    assert!(
        prepare_original_create(
            I::Binary(b"i"),
            I::List(&opts),
            OriginalCreateContext::default(),
            cap
        )
        .is_err()
    );
    let value = accepted(b"i", I::List(&opts))?;
    assert!(matches!(value.field("type"), Some(I::Binary(b"email"))));
    let pairs = [
        [I::Atom("type"), I::Binary(b"email")],
        [I::Atom("state"), I::Binary(b"running")],
    ];
    let opts = pairs.iter().map(|p| I::Tuple(p)).collect::<Vec<_>>();
    let Outcome::Rejected {
        work_units,
        temporary_bytes,
        ..
    } = prepare_original_create(
        I::Binary(b"i"),
        I::List(&opts),
        OriginalCreateContext::default(),
        limits(),
    )?
    else {
        return Err("measured source refusal".into());
    };
    assert!(work_units > 16_384);
    assert!(temporary_bytes >= 16_384);
    let mut cap = limits();
    cap.maximum_work_units = work_units - 1;
    assert!(
        prepare_original_create(
            I::Binary(b"i"),
            I::List(&opts),
            OriginalCreateContext::default(),
            cap
        )
        .is_err()
    );
    cap.maximum_work_units = work_units;
    assert!(matches!(
        prepare_original_create(
            I::Binary(b"i"),
            I::List(&opts),
            OriginalCreateContext::default(),
            cap
        )?,
        Outcome::Rejected { .. }
    ));
    Ok(())
}

#[test]
fn original_create_input_ceiling_has_an_inclusive_boundary() -> TestResult {
    let pair = [I::Atom("type"), I::Binary(b"email")];
    let opts = [I::Tuple(&pair)];
    let mut below = 0;
    let mut above = 4096;
    while below + 1 < above {
        let middle = below + (above - below) / 2;
        let mut cap = limits();
        cap.maximum_input_bytes = middle;
        if prepare_original_create(
            I::Binary(b"i"),
            I::List(&opts),
            OriginalCreateContext::default(),
            cap,
        )
        .is_ok()
        {
            above = middle;
        } else {
            below = middle;
        }
    }
    let mut cap = limits();
    cap.maximum_input_bytes = above;
    assert!(matches!(
        prepare_original_create(
            I::Binary(b"i"),
            I::List(&opts),
            OriginalCreateContext::default(),
            cap
        )?,
        Outcome::Accepted { .. }
    ));
    cap.maximum_input_bytes = below;
    assert!(
        prepare_original_create(
            I::Binary(b"i"),
            I::List(&opts),
            OriginalCreateContext::default(),
            cap
        )
        .is_err()
    );
    Ok(())
}
