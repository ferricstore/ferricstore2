use super::{fixtures as f, *};

#[test]
fn catalog_older_capability_remains_closed_without_changing_its_owner_cohort() -> TestResult {
    let dir = common::directory("catalog-old-capability")?;
    let path = dir.join("state");
    credit::seed(&path)?;
    let before = credit::cohort(&path)?;
    let store = common::open(&path)?;
    assert!(matches!(
        store.read_credit_catalog(&common::scope()?, 1, 10_000, 64 * common::MIB),
        Err(StateTransactionError::UnsupportedPolicyProfile)
    ));
    drop(store);
    assert_eq!(credit::cohort(&path)?, before);
    std::fs::remove_dir_all(dir)?;
    Ok(())
}

#[test]
fn catalog_reopen_rejects_missing_active_entries_metadata_and_noncanonical_values() -> TestResult {
    let dir = common::directory("catalog-corruption")?;
    for variant in 0..5 {
        let path = dir.join(format!("bad-{variant}"));
        drop(f::create(&path)?);
        f::lease(&path, b"global", 0)?;
        let (meta_key, _) = f::row(&path, 16)?;
        let (intent_key, mut value) = f::row(&path, 17)?;
        match variant {
            0 => f::replace(&path, &intent_key, None)?,
            1 => f::replace(&path, &meta_key, None)?,
            2 => f::replace(&path, &meta_key, Some(&f::meta(3, 1)))?,
            3 => {
                value.push(0);
                f::replace(&path, &intent_key, Some(&value))?;
            }
            _ => f::replace(&path, &meta_key, Some(&f::meta(2, 1)))?,
        }
        assert!(
            matches!(common::open(&path),Err(error) if error.downcast_ref::<StateTransactionError>()==Some(&StateTransactionError::Corrupt)),
            "variant {variant}"
        );
    }
    std::fs::remove_dir_all(dir)?;
    Ok(())
}

#[test]
fn catalog_shape_valid_intent_with_missing_owner_is_preserved_for_publisher_discard() -> TestResult
{
    let dir = common::directory("catalog-orphan-owner")?;
    let path = dir.join("state");
    drop(f::create(&path)?);
    f::lease(&path, b"global", 0)?;
    let (key, _) = f::row(&path, 17)?;
    let mut bytes = b"FWCI\0\x01\0\0\0\x07".to_vec();
    bytes.extend_from_slice(b"missing");
    f::replace(&path, &key, Some(&bytes))?;
    let page = f::page(&path, 1)?;
    assert_eq!(page.entries[0].credit_scope, b"missing");
    let store = common::open(&path)?;
    assert!(
        store
            .read_credit_owner(
                &common::scope()?,
                b"missing",
                CreditLimits::default(),
                10_000,
                64 * common::MIB
            )?
            .is_none()
    );
    drop(store);
    std::fs::remove_dir_all(dir)?;
    Ok(())
}

#[test]
fn catalog_only_retained_refusal_cannot_be_reinterpreted_as_an_older_profile() -> TestResult {
    use wf_domain::workflow::MAX_EXACT_MILLIS as MAX;
    let dir = common::directory("catalog-retained-profile")?;
    let path = dir.join("state");
    drop(f::create(&path)?);
    f::lease(&path, b"global", 0)?;
    let (key, _) = f::row(&path, 17)?;
    f::replace(&path, &key, None)?;
    let (meta, _) = f::row(&path, 16)?;
    f::replace(&path, &meta, Some(&f::meta(MAX + 1, MAX)))?;
    let outcome = f::apply(
        &path,
        b"new",
        b"refused",
        1,
        Some(3),
        Mutation::Lease {
            shard_id: 0,
            amount: 1,
            ttl_ms: 1000,
        },
    )?;
    assert_eq!(
        credit::reply(&outcome)?.decision,
        Decision::Rejected(Rejection::CatalogSequenceExhausted)
    );
    f::replace(&path, &meta, None)?;
    f::downgrade_catalog(&path)?;
    // No format2 key remains: the retained response itself requires format2.
    assert_eq!(credit::family_count(&path, 16)?, 0);
    assert_eq!(credit::family_count(&path, 17)?, 0);
    assert!(
        matches!(common::open(&path), Err(error) if error.downcast_ref::<StateTransactionError>() == Some(&StateTransactionError::Corrupt))
    );
    std::fs::remove_dir_all(dir)?;
    Ok(())
}

#[test]
fn catalog_snapshot_cannot_replace_an_older_capability_with_the_same_shard_count() -> TestResult {
    use wf_ports::snapshot::{SnapshotError, SnapshotLimits, StateSnapshotStore};
    use wf_storage::{snapshot::RedbSnapshotStage, state::StateStoreLimits};
    let dir = common::directory("catalog-snapshot-capability")?;
    let path = dir.join("source");
    drop(f::create(&path)?);
    f::lease(&path, b"global", 0)?;
    let mut source = common::open(&path)?;
    let mut wire = Vec::new();
    source.capture_snapshot(&mut wire, SnapshotLimits::default())?;
    drop(source);
    let scope = common::scope()?;
    let stage = RedbSnapshotStage::receive(
        &dir.join("stage"),
        scope.group().clone(),
        scope.recovery().clone(),
        &mut std::io::Cursor::new(&wire),
        StateStoreLimits::default(),
        SnapshotLimits::default(),
    )?;
    let old_path = dir.join("old");
    drop(credit::create(&old_path)?);
    let before = credit::cohort(&old_path)?;
    let mut old = common::open(&old_path)?;
    assert!(matches!(
        old.install_snapshot(stage, SnapshotLimits::default()),
        Err(SnapshotError::GenesisMismatch)
    ));
    drop(old);
    assert_eq!(credit::cohort(&old_path)?, before);
    assert_eq!(f::page(&path, 1)?.entries[0].credit_scope, b"global");
    std::fs::remove_dir_all(dir)?;
    Ok(())
}
