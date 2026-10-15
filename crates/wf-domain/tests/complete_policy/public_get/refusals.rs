//! Prospective ingress work and one-call key scratch; no all-held-owner claim.

use super::{
    EncodingError, PolicySchemaCharge, PolicySchemaError, PolicySchemaLimits, Raw, TestResult,
    accepted, limits, prepare,
};

const MIN: &[(u64, u64)] = &[
    (128, 0),
    (128, 0),
    (128, 0),
    (128, 0),
    (128, 0),
    (128, 0),
    (0, 142),
    (142, 0),
    (128, 0),
    (142, 0),
];

// [(state,nil)] followed by a proper list fragment [(state,Other)].
// The later duplicate pays its key validation but no new selector comparison.
const SEGMENTED: &[(u64, u64)] = &[
    (128, 0),
    (128, 0),
    (128, 0),
    (128, 0),
    (133, 0),
    (138, 0),
    (128, 0),
    (128, 0),
    (133, 0),
    (128, 0),
    (128, 0),
    (128, 0),
    (0, 142),
    (142, 0),
    (128, 0),
    (142, 0),
];

#[test]
fn original_public_get_prepays_each_ingress_operation() -> TestResult {
    assert_events(&Raw::List(&[]), MIN, 1_180)?;
    let first = [Raw::Atom("state"), Raw::Nil];
    let second = [Raw::Atom("state"), Raw::Other];
    let prefix = [Raw::Tuple(&first)];
    let suffix = [Raw::Tuple(&second)];
    let tail = Raw::List(&suffix);
    let opts = Raw::ImproperList {
        prefix: &prefix,
        tail: &tail,
    };
    assert_events(&opts, SEGMENTED, 1_968)?;
    assert_literal_prefixes()?;
    Ok(())
}

#[test]
fn original_public_get_preserves_continuous_work_and_held_requests() -> TestResult {
    assert_mismatched_profile()?;
    assert_held_requests(2_691, true)?;
    assert_held_requests(2_690, false)?;
    Ok(())
}

fn assert_events(opts: &Raw<'_>, events: &[(u64, u64)], total_g: u64) -> TestResult {
    let (mut g, mut p) = (0, 0);
    for (i, &(fee, peak)) in events.iter().enumerate() {
        let next_g = g + fee;
        let next_p = p.max(peak);
        let target = next_g + next_p;
        let tight = limits(target - 1)?;
        let mut charge = PolicySchemaCharge::new(tight);
        assert_work(
            prepare(&Raw::Binary(b"r"), opts, tight, &mut charge),
            target,
            target - 1,
        );
        assert_eq!((charge.g(), charge.p()), (g, p));
        let exact = limits(target)?;
        let mut charge = PolicySchemaCharge::new(exact);
        let result = prepare(&Raw::Binary(b"r"), opts, exact, &mut charge);
        if let Some(&(next_fee, next_peak)) = events.get(i + 1) {
            assert_work(result, next_g + next_fee + next_p.max(next_peak), target);
        } else {
            let value = result?.map_err(|_| "exact ingress refused")?;
            assert_eq!(value.policy_key(), b"f:{f}:policy:r");
            assert_eq!(value.response_state(), None);
            assert!(value.schema_limits() == exact);
        }
        assert_eq!((charge.g(), charge.p()), (next_g, next_p));
        (g, p) = (next_g, next_p);
    }
    assert_eq!((g, p, g + p), (total_g, 142, total_g + 142));
    Ok(())
}

fn assert_literal_prefixes() -> TestResult {
    let state = [Raw::Atom("state"), Raw::Other];
    let prefix = [Raw::Tuple(&state)];
    let tail = Raw::Nil;
    let opts = Raw::ImproperList {
        prefix: &prefix,
        tail: &tail,
    };
    let profile = limits(911)?;
    let mut charge = PolicySchemaCharge::new(profile);
    let Err(reason) = prepare(&Raw::Binary(b""), &opts, profile, &mut charge)? else {
        return Err("improper keyword tail must reject before type/state".into());
    };
    assert_eq!(reason, super::Rejection::Options);
    assert_eq!((charge.g(), charge.p()), (911, 0));
    let profile = limits(910)?;
    let mut charge = PolicySchemaCharge::new(profile);
    assert_work(
        prepare(&Raw::Binary(b""), &opts, profile, &mut charge),
        911,
        910,
    );
    assert_eq!((charge.g(), charge.p()), (783, 0));
    Ok(())
}

fn assert_mismatched_profile() -> TestResult {
    let profile = PolicySchemaLimits::default();
    let mut charge = PolicySchemaCharge::new(profile);
    assert!(matches!(
        prepare(
            &Raw::Binary(b"r"),
            &Raw::List(&[]),
            limits(1_000_000)?,
            &mut charge
        ),
        Err(EncodingError::Schema(PolicySchemaError::ProfileMismatch))
    ));
    assert_eq!((charge.g(), charge.p()), (0, 0));
    Ok(())
}

fn assert_held_requests(maximum: u64, succeeds: bool) -> TestResult {
    let profile = limits(maximum)?;
    let mut charge = PolicySchemaCharge::new(profile);
    let ty = [b'z'; 64];
    // This first key has L77, G1152+2*77=1306 and one-call P205.
    let held = accepted(&Raw::Binary(&ty), &Raw::List(&[]), profile, &mut charge)?;
    assert_eq!((charge.g(), charge.p()), (1_306, 205));
    let result = prepare(&Raw::Binary(b"r"), &Raw::List(&[]), profile, &mut charge);
    if succeeds {
        let second = result?.map_err(|_| "second request refused")?;
        assert_eq!(second.policy_key(), b"f:{f}:policy:r");
        assert_eq!((charge.g(), charge.p()), (2_486, 205));
    } else {
        assert_work(result, 2_691, maximum);
        assert_eq!((charge.g(), charge.p()), (2_344, 205));
    }
    assert_eq!(held.workflow_type(), ty);
    assert_eq!(&held.policy_key()[..13], b"f:{f}:policy:");
    assert_eq!(&held.policy_key()[13..], ty);
    // Preservation/cumulative G/historical P only: this API does not certify
    // the SUM of earlier externally held request owners.
    Ok(())
}

fn assert_work<T>(result: Result<T, EncodingError>, bytes: u64, maximum: u64) {
    assert!(matches!(
        result,
        Err(EncodingError::Schema(PolicySchemaError::WorkBytes { bytes: got, maximum: cap }))
            if got == bytes && cap == maximum
    ));
    drop(result);
}
