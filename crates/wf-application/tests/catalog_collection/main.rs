//! Original all-page ordering, exact-key bypass, ownership and failure contracts.
#![forbid(unsafe_code)]
use wf_application::governance::{
    GovernanceCatalogCollectionError as E, GovernanceCatalogCollectionLimits as Limits,
    GovernanceCatalogCollectionRequest as Request, collect_governance_catalog,
    collect_governance_keys,
};
use wf_domain::workflow::request::RequestScope;
use wf_ports::state::{
    GovernanceCatalogReadLimits as ReadLimits, LoadedGovernanceRecord as Loaded,
    StateTransactionError as S,
};
type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;
mod failures;
mod fixtures;
use fixtures::{CatalogFault as Fault, CatalogFixture, TrackedRecord};

#[test]
fn catalog_collection_keeps_global_top_results_across_every_page() -> TestResult {
    let fixture = CatalogFixture::new(600)?;
    let result = collect_governance_catalog(
        &fixture,
        fixture.request(5),
        CatalogFixture::load,
        |a, b| b.cmp(a),
        Limits::default(),
    )?;
    assert_eq!(
        result.into_records().collect::<Vec<_>>(),
        (595..600_u32)
            .rev()
            .map(|n| n.to_be_bytes().to_vec())
            .collect::<Vec<_>>()
    );
    assert_eq!(*fixture.calls.borrow(), vec![256, 256, 256]);
    Ok(())
}

#[test]
fn catalog_collection_preserves_reversed_arrival_ties_and_exact_full_page_tail() -> TestResult {
    let fixture = CatalogFixture::new(512)?;
    let result = collect_governance_catalog(
        &fixture,
        fixture.request(3),
        CatalogFixture::load,
        |_, _| std::cmp::Ordering::Equal,
        Limits::default(),
    )?;
    assert_eq!(
        result.into_records().collect::<Vec<_>>(),
        (509..512_u32)
            .rev()
            .map(|n| n.to_be_bytes().to_vec())
            .collect::<Vec<_>>()
    );
    assert_eq!(*fixture.calls.borrow(), vec![256, 256, 256]);
    Ok(())
}

#[test]
fn catalog_collection_anchors_after_concurrent_insertion_before_the_cursor() -> TestResult {
    let fixture = CatalogFixture::new(300)?;
    fixture.fault.set(Fault::InsertBeforeAnchor);
    let result = collect_governance_catalog(
        &fixture,
        fixture.request(300),
        CatalogFixture::load,
        Ord::cmp,
        Limits::default(),
    )?;
    assert_eq!(
        result.into_records().collect::<Vec<_>>(),
        (0..300_u32)
            .map(|n| n.to_be_bytes().to_vec())
            .collect::<Vec<_>>()
    );
    Ok(())
}

#[test]
fn catalog_collection_direct_keys_preserve_duplicates_and_need_no_catalog() -> TestResult {
    let keys = [b"c".as_slice(), b"a", b"c", b"b"];
    let result =
        collect_governance_keys(&keys, 4, CatalogFixture::load, Ord::cmp, Limits::default())?;
    assert_eq!(
        result.into_records().collect::<Vec<_>>(),
        vec![b"a".to_vec(), b"b".to_vec(), b"c".to_vec(), b"c".to_vec()]
    );
    let empty = collect_governance_keys::<Vec<u8>>(
        &[],
        5,
        CatalogFixture::load,
        Ord::cmp,
        Limits::default(),
    )?;
    assert!(empty.is_empty());
    Ok(())
}

#[test]
fn catalog_collection_moves_records_and_holds_only_top_n_plus_one_candidate() -> TestResult {
    let fixture = CatalogFixture::new(600)?;
    let live = std::rc::Rc::new(std::cell::Cell::new(0));
    let peak = std::cell::Cell::new(0);
    let result = collect_governance_catalog(
        &fixture,
        fixture.request(5),
        |key, _| Ok(Some(TrackedRecord::load(key, &live, &peak))),
        |a, b| b.key.cmp(&a.key),
        Limits::default(),
    )?;
    assert_eq!(result.len(), 5);
    assert_eq!(
        result.records().next().map(|r| r.key.as_slice()),
        Some(599_u32.to_be_bytes().as_slice())
    );
    assert_eq!(live.get(), 5);
    assert_eq!(peak.get(), 6);
    assert_eq!(
        result.owned_bytes(),
        5 * (std::mem::size_of::<TrackedRecord>() + 4)
    );
    drop(result);
    assert_eq!(live.get(), 0);
    Ok(())
}
