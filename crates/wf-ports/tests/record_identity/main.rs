//! Both complete `RecordIdentityTest` cases at original 66586c596e27.
//! Source IDs: 9589f7a84bac8e16 and 05cd1ba469691b9f.
//! Source async:true; no local setup or flow profile tag. Fixture metadata
//! mirrors `scope_metadata/1`. Every original assertion is retained in order.
#![forbid(unsafe_code)]
use wf_domain::metadata::OriginalMetadataInput as I;
use wf_domain::metadata::scope::{OriginalScopeLimits, original_physical_partition};
use wf_ports::blob::{
    OriginalValuePartition as P, owns_original_state_key as owns, render_original_state_key as key,
};
mod bounds;

fn limits() -> OriginalScopeLimits {
    OriginalScopeLimits {
        maximum_output_bytes: 100_000,
        maximum_work_units: 10_000_000,
        maximum_temporary_bytes: 1_000_000,
    }
}

#[test]
fn accepts_unscoped_and_sealed_shared_state_ownership() -> Result<(), String> {
    let unscoped_key =
        key(b"plain", P::Explicit(b"tenant-a"), 65_535).map_err(|error| format!("{error:?}"))?;
    assert!(
        owns(
            Some(I::Binary(b"plain")),
            Some(I::Binary(b"tenant-a")),
            None,
            I::Binary(&unscoped_key),
            limits()
        )
        .map_err(|error| format!("{error:?}"))?
        .value
    );
    let scope = 42_u64.to_be_bytes();
    let physical = original_physical_partition(b"tenant-a", Some(&scope), limits());
    assert!(physical.is_ok());
    let physical = physical.map_err(|error| format!("{error:?}"))?.value;
    let sealed = [
        I::Integer(1),
        I::Atom("uint64"),
        I::Atom("isolation_scope"),
        I::Integer(42),
    ];
    let metadata = [(I::Integer(0x8001), I::Tuple(&sealed))];
    let state_key =
        key(b"shared", P::Explicit(&physical), 65_535).map_err(|error| format!("{error:?}"))?;
    assert!(
        owns(
            Some(I::Binary(b"shared")),
            Some(I::Binary(&physical)),
            Some(I::Map(&metadata)),
            I::Binary(&state_key),
            limits()
        )
        .map_err(|error| format!("{error:?}"))?
        .value
    );
    Ok(())
}

#[test]
fn rejects_foreign_sealed_scope_and_malformed_identity_without_raising() -> Result<(), String> {
    let scope = 42_u64.to_be_bytes();
    let physical = original_physical_partition(b"tenant-a", Some(&scope), limits());
    assert!(physical.is_ok());
    let physical = physical.map_err(|error| format!("{error:?}"))?.value;
    let state_key =
        key(b"shared", P::Explicit(&physical), 65_535).map_err(|error| format!("{error:?}"))?;
    let sealed = [
        I::Integer(1),
        I::Atom("uint64"),
        I::Atom("isolation_scope"),
        I::Integer(99),
    ];
    let metadata = [(I::Integer(0x8001), I::Tuple(&sealed))];
    let forged = Some(I::Map(&metadata));
    assert!(
        !owns(
            Some(I::Binary(b"shared")),
            Some(I::Binary(&physical)),
            forged,
            I::Binary(&state_key),
            limits()
        )
        .map_err(|error| format!("{error:?}"))?
        .value
    );
    assert!(
        !owns(
            Some(I::Binary(b"shared")),
            Some(I::Map(&[])),
            None,
            I::Binary(&state_key),
            limits()
        )
        .map_err(|error| format!("{error:?}"))?
        .value
    );
    assert!(
        !owns(
            None,
            Some(I::Binary(b"tenant-a")),
            None,
            I::Binary(&state_key),
            limits()
        )
        .map_err(|error| format!("{error:?}"))?
        .value
    );
    assert!(
        !owns(
            Some(I::Binary(b"shared")),
            Some(I::Binary(&physical)),
            forged,
            I::Atom("not_a_key"),
            limits()
        )
        .map_err(|error| format!("{error:?}"))?
        .value
    );
    Ok(())
}
