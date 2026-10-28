use super::{I, L, N, P, TestResult, prepare};

pub(super) const fn limits() -> L {
    L {
        max_input_bytes: 1_048_576,
        max_pairs: 1_024,
        max_output_bytes: 131_072,
        max_work_units: 64 * 1_024 * 1_024,
        max_temporary_bytes: 8 * 1_024 * 1_024,
    }
}

pub(super) fn healthy(input: I<'_>, limits: L) -> TestResult<P<'_>> {
    let value = prepare(input, limits);
    assert!(
        value.is_ok(),
        "healthy history metadata: {:?}",
        value.as_ref().err()
    );
    Ok(value?)
}

pub(super) fn ready() -> TestResult {
    let nodes = [
        N::Binary { bytes: 0..5 },
        N::FloatBits(1.5_f64.to_bits()),
        N::Map { entries: 0..2 },
    ];
    healthy(
        I {
            bytes: b"float",
            nodes: &nodes,
            edges: &[0, 1],
            root: 2,
        },
        limits(),
    )?;
    Ok(())
}

pub(super) fn rendered(bytes: &[u8], scalar: N) -> TestResult<Vec<u8>> {
    let nodes = [N::Binary { bytes: 0..1 }, scalar, N::Map { entries: 0..2 }];
    let value = healthy(
        I {
            bytes,
            nodes: &nodes,
            edges: &[0, 1],
            root: 2,
        },
        limits(),
    )?;
    let fields = value.fields();
    assert_eq!(fields.len(), 1);
    Ok(fields[0].value.as_ref().to_vec())
}
