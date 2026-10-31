//! Whole-cohort rollback, real disk uncertainty and receipt-only capability.
use super::*;
use std::sync::{
    Arc,
    atomic::{AtomicU8, Ordering},
};
use wf_storage::transaction::{CatalogPruneTransactionError as E, apply_catalog_prune_transaction};

#[test]
fn catalog_prune_every_step_cut_preserves_the_entire_previous_cohort() -> TestResult {
    let mut fixture = PruneFixture::new(None, true)?;
    let revision = fixture.register(b"add", b"a", 0)?;
    let frame = PruneFixture::frame(b"cut", b"a", revision)?;
    let PruneFixture { dir, path, store } = fixture;
    drop(store);
    let before = f::cohort(&path)?;
    let mut reached = false;
    for steps in 1..=160 {
        let mut limits = common::driver_limits();
        limits.maximum_read_steps = steps;
        let mut store = common::open(&path)?;
        let result = apply_catalog_prune_transaction(
            &mut store,
            &common::scope()?,
            frame.as_bytes(),
            common::log(1),
            limits,
        );
        drop(store);
        match result {
            Err(E::State(S::Limit)) => assert_eq!(f::cohort(&path)?, before, "cut{steps}"),
            Ok(Outcome::Retained { outcome, .. }) => {
                assert_eq!(reply(&outcome)?, P::Removed);
                reached = true;
                break;
            }
            other => return Err(format!("unexpected prune cut {other:?}").into()),
        }
    }
    assert!(reached);
    assert_ne!(f::cohort(&path)?, before);
    std::fs::remove_dir_all(dir)?;
    Ok(())
}

#[test]
fn catalog_prune_actual_commit_uncertainty_fences_then_recovers_complete_old_or_new() -> TestResult
{
    let mut fixture = PruneFixture::new(None, true)?;
    let revision = fixture.register(b"add", b"a", 0)?;
    let frame = PruneFixture::frame(b"unknown", b"a", revision)?;
    let mut target = PruneFixture::new(None, true)?;
    assert_eq!(target.register(b"add", b"a", 0)?, revision);
    assert_eq!(reply(&*target.apply(&frame, 1)?)?, P::Removed);
    let PruneFixture {
        dir: expected_dir,
        path: expected,
        store,
    } = target;
    drop(store);
    let after = f::cohort(&expected)?;
    let PruneFixture { dir, path, store } = fixture;
    drop(store);
    let before = f::cohort(&path)?;
    assert_ne!(before, after);
    let fault = Arc::new(AtomicU8::new(0));
    let mut store = crate::faults::wrapped(&path, Arc::clone(&fault))?;
    fault.store(4, Ordering::SeqCst);
    assert_eq!(
        apply_catalog_prune_transaction(
            &mut store,
            &common::scope()?,
            frame.as_bytes(),
            common::log(1),
            common::driver_limits(),
        )
        .err(),
        Some(E::State(S::Uncertain))
    );
    assert_eq!(fault.load(Ordering::SeqCst), 0);
    assert_eq!(
        store
            .governance_catalog_prune_page(
                GovernanceCatalogRead {
                    scope: &common::scope()?,
                    catalog: b"limit",
                    cursor: None,
                    limit: 1
                },
                L::default(),
            )
            .err(),
        Some(S::Unavailable)
    );
    drop(store);
    drop(common::open(&path)?);
    let recovered = f::cohort(&path)?;
    assert!(recovered == before || recovered == after);
    let mut store = common::open(&path)?;
    let Outcome::Retained { outcome, .. } = apply_catalog_prune_transaction(
        &mut store,
        &common::scope()?,
        frame.as_bytes(),
        common::log(1 + u64::from(recovered != before)),
        common::driver_limits(),
    )?
    else {
        return Err("missing retry outcome".into());
    };
    assert_eq!(reply(&outcome)?, P::Removed);
    assert_eq!(outcome.original_position(), common::log(1));
    drop(store);
    std::fs::remove_dir_all(dir)?;
    std::fs::remove_dir_all(expected_dir)?;
    Ok(())
}

#[test]
fn catalog_prune_receipt_only_missing_and_removed_refuse_healthy_gc2_downgrade() -> TestResult {
    for removed in [false, true] {
        let mut fixture = PruneFixture::new(None, true)?;
        let revision = if removed {
            fixture.register(b"add", b"a", 0)?
        } else {
            EntryIdentity::new(
                common::log(0),
                wf_domain::workflow::control::PreparedDigest::new([7; 32]),
            )
        };
        let frame = PruneFixture::frame(b"prune", b"a", revision)?;
        let result = fixture.apply(&frame, u64::from(removed))?;
        assert_eq!(
            reply(&result)?,
            if removed { P::Removed } else { P::Missing }
        );
        assert_eq!(fixture.page()?.page.members, Vec::<Vec<u8>>::new());
        let PruneFixture { dir, path, store } = fixture;
        drop(store);
        fixtures::downgrade(&path)?;
        let error = common::open(&path)
            .err()
            .ok_or("downgraded prune receipt accepted")?;
        assert_eq!(error.downcast_ref::<S>(), Some(&S::Corrupt));
        std::fs::remove_dir_all(dir)?;
    }
    Ok(())
}

#[test]
fn catalog_prune_malformed_ahead_and_equal_position_wrong_identity_rows_fence() -> TestResult {
    for damage in 0..3 {
        let mut fixture = PruneFixture::new(None, true)?;
        let revision = fixture.register(b"add", b"a", 0)?;
        let mut bytes = wf_ports::state::encode_catalog_member_revision(revision).to_vec();
        match damage {
            0 => bytes = b"FWGM\0\x01".to_vec(),
            1 => bytes[22..30].copy_from_slice(&1_u64.to_be_bytes()),
            _ => bytes[30] ^= 1,
        }
        let PruneFixture { dir, path, store } = fixture;
        drop(store);
        fixtures::damage_member(&path, &bytes)?;
        let before = f::cohort(&path)?;
        let error = common::open(&path)
            .err()
            .ok_or("invalid member revision accepted")?;
        assert_eq!(error.downcast_ref::<S>(), Some(&S::Corrupt));
        assert_eq!(f::cohort(&path)?, before);
        std::fs::remove_dir_all(dir)?;
    }
    Ok(())
}

#[test]
fn catalog_prune_candidate_read_admission_has_exact_byte_cutoff_without_mutation() -> TestResult {
    let mut fixture = PruneFixture::new(None, true)?;
    let revision = fixture.register(b"add", b"a", 0)?;
    let scope = common::scope()?;
    let mut low = 1;
    let mut high = L::default().maximum_bytes;
    while low < high {
        let mid = low + (high - low) / 2;
        let result = fixture.store.governance_catalog_prune_page(
            GovernanceCatalogRead {
                scope: &scope,
                catalog: b"limit",
                cursor: None,
                limit: 1,
            },
            L {
                maximum_bytes: mid,
                ..L::default()
            },
        );
        match result {
            Ok(page) => {
                assert_eq!(page.revision, Some(revision));
                high = mid;
            }
            Err(S::Limit) => low = mid + 1,
            other => return Err(format!("unexpected candidate admission {other:?}").into()),
        }
    }
    let read = |bytes| {
        fixture.store.governance_catalog_prune_page(
            GovernanceCatalogRead {
                scope: &scope,
                catalog: b"limit",
                cursor: None,
                limit: 1,
            },
            L {
                maximum_bytes: bytes,
                ..L::default()
            },
        )
    };
    assert_eq!(read(low)?.revision, Some(revision));
    assert_eq!(read(low - 1).err(), Some(S::Limit));
    let PruneFixture { dir, path, store } = fixture;
    drop(store);
    let before = f::cohort(&path)?;
    drop(common::open(&path)?);
    assert_eq!(f::cohort(&path)?, before);
    std::fs::remove_dir_all(dir)?;
    Ok(())
}
