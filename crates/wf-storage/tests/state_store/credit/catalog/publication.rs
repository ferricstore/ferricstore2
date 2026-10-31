use super::{fixtures as catalog, *};
use wf_ports::state::GroupApplyStore;
use wf_ports::state::{
    CreditCatalogOwnerState as O, CreditCatalogReadLimits, CreditCatalogReadPort,
};
use wf_storage::state::{RedbStateStore, StateStoreGenesis, StateStoreLimits};

#[test]
fn credit_catalog_combined_genesis_preserves_actual_independent_owner_and_target_capabilities()
-> TestResult {
    let dir = common::directory("catalog-combined")?;
    let path = dir.join("state");
    let scope = common::scope()?;
    let mut store = RedbStateStore::create_credit_and_governance_catalog_database(
        redb::Builder::new().create(&path)?,
        scope.group().clone(),
        scope.recovery().clone(),
        StateStoreGenesis {
            initial_time: common::clock(0)?,
            placements: &common::placements()?,
            policies: &[],
        },
        credit::defaults(),
        4,
        StateStoreLimits::default(),
    )?;
    let tx = store.begin_apply(
        scope.group(),
        scope.recovery(),
        common::driver_limits().codec.admission(),
        10000,
        64 * common::MIB,
        32 * common::MIB,
    )?;
    assert_eq!(tx.require_identified_credit_profile()?, (2, 4));
    tx.require_governance_catalog()?;
    drop(tx);
    drop(store);
    drop(common::open(&path)?);
    assert_eq!(
        common::raw_rows(&path)?
            .keys()
            .filter(|key| key.get(6) == Some(&18))
            .count(),
        0
    );
    std::fs::remove_dir_all(dir)?;
    Ok(())
}

#[test]
fn credit_catalog_owner_batch_preserves_original_order_missing_intents_and_real_read_limits()
-> TestResult {
    let dir = common::directory("catalog-owner-batch")?;
    let path = dir.join("state");
    drop(catalog::create(&path)?);
    for (index, name) in [b"a", b"b", b"c"].into_iter().enumerate() {
        catalog::lease(&path, name, u64::try_from(index)?)?;
    }
    let owner = common::raw_rows(&path)?
        .into_iter()
        .find(|(key, value)| {
            key.get(6) == Some(&12)
                && wf_ports::log::decode_credit_owner(
                    value,
                    CreditLimits::default(),
                    900_000,
                    32 * common::MIB,
                )
                .is_ok_and(|owner| owner.view().scope == b"b")
        })
        .ok_or("missing original owner")?;
    catalog::replace(&path, &owner.0, None)?;
    let mut store = common::open(&path)?;
    let page =
        store.credit_catalog_page(&common::scope()?, 256, CreditCatalogReadLimits::default())?;
    assert_eq!(page.entries.len(), 3);
    assert_eq!(
        store.credit_catalog_owners(
            &common::scope()?,
            &page.entries,
            CreditCatalogReadLimits::default()
        )?,
        vec![O::Present, O::Missing, O::Present]
    );
    let small = CreditCatalogReadLimits {
        maximum_steps: 1,
        ..CreditCatalogReadLimits::default()
    };
    assert_eq!(
        store
            .credit_catalog_owners(&common::scope()?, &page.entries, small)
            .err(),
        Some(StateTransactionError::Limit)
    );
    let original = common::scope()?;
    let wrong = wf_domain::workflow::request::RequestScope::new(
        original.tenant().clone(),
        wf_types::identity::GroupId::new(b"wrong".to_vec())?,
        original.recovery().clone(),
    );
    assert_eq!(
        store
            .credit_catalog_owners(&wrong, &page.entries, CreditCatalogReadLimits::default())
            .err(),
        Some(StateTransactionError::NamespaceConflict)
    );
    drop(store);
    std::fs::remove_dir_all(dir)?;
    Ok(())
}
