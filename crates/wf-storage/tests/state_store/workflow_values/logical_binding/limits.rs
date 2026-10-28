use wf_ports::log::{
    CodecError as E, LogicalMutationFrameLimits as L, encode_logical_mutation_frame,
};

use super::{TestResult, fixtures as f};

#[test]
fn logical_binding_enforces_owned_work_and_capacity_bounds() -> TestResult {
    f::automatic(b"abc")?;
    let limits = f::limits()?;
    for (profile, error) in [
        (
            L {
                maximum_owned: 1,
                ..limits
            },
            E::OwnedLimit,
        ),
        (
            L {
                maximum_work: 1,
                ..limits
            },
            E::WorkLimit,
        ),
        (
            L {
                maximum_owned: 0,
                ..limits
            },
            E::Malformed,
        ),
        (
            L {
                maximum_work: 0,
                ..limits
            },
            E::Malformed,
        ),
    ] {
        let actual = encode_logical_mutation_frame(
            f::caller("create", b"abc", None, None)?,
            f::context(None, 100)?,
            profile,
        );
        assert!(matches!(actual, Err(found) if found == error));
    }
    let small = L {
        maximum_owned: 65_536,
        ..limits
    };
    let mut context = f::context(None, 100)?;
    let mut large = Vec::with_capacity(262_144);
    large.extend_from_slice(b"r");
    context.policies[0].workflow_type = large;
    assert!(matches!(
        encode_logical_mutation_frame(f::caller("create", b"abc", None, None)?, context, small),
        Err(E::OwnedLimit)
    ));
    let mut context = f::context(None, 100)?;
    let mut oversized = Vec::with_capacity(4_096);
    oversized.append(&mut context.policies);
    context.policies = oversized;
    assert!(matches!(
        encode_logical_mutation_frame(f::caller("create", b"abc", None, None)?, context, small),
        Err(E::OwnedLimit)
    ));
    exact_owned_boundary(limits)?;
    Ok(())
}

fn exact_owned_boundary(limits: L) -> TestResult {
    let mut low = 1;
    let mut high = limits.maximum_owned;
    while low < high {
        let middle = low + (high - low) / 2;
        let limited = L {
            maximum_owned: middle,
            ..limits
        };
        match encode_logical_mutation_frame(
            f::caller("create", b"abc", None, None)?,
            f::context(None, 100)?,
            limited,
        ) {
            Ok(_) => high = middle,
            Err(E::OwnedLimit) => low = middle + 1,
            Err(error) => return Err(format!("owned boundary: {error:?}").into()),
        }
    }
    assert!(
        encode_logical_mutation_frame(
            f::caller("create", b"abc", None, None)?,
            f::context(None, 100)?,
            L {
                maximum_owned: low,
                ..limits
            }
        )
        .is_ok()
    );
    assert!(matches!(
        encode_logical_mutation_frame(
            f::caller("create", b"abc", None, None)?,
            f::context(None, 100)?,
            L {
                maximum_owned: low - 1,
                ..limits
            }
        ),
        Err(E::OwnedLimit)
    ));
    Ok(())
}
