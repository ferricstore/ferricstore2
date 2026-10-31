use super::*;

#[test]
fn catalog_collection_skips_missing_records_but_propagates_late_loader_errors() -> TestResult {
    let fixture = CatalogFixture::new(300)?;
    let mut loaded = 0;
    let result = collect_governance_catalog(
        &fixture,
        fixture.request(1),
        |key, limits| {
            loaded += 1;
            if loaded == 290 {
                return Err(S::Corrupt);
            }
            if loaded % 2 == 0 {
                return Ok(None);
            }
            CatalogFixture::load(key, limits)
        },
        Ord::cmp,
        Limits::default(),
    );
    assert!(matches!(result, Err(E::Source(S::Corrupt))));
    assert_eq!(loaded, 290);
    assert_eq!(*fixture.calls.borrow(), vec![256, 256]);
    Ok(())
}

#[test]
fn catalog_collection_refuses_changed_repeated_unsorted_and_invalid_full_pages() -> TestResult {
    for (fault, expected) in [
        (Fault::DeleteAnchor, E::Source(S::CatalogChanged)),
        (Fault::RepeatedPage, E::InvalidPage),
        (Fault::Unsorted, E::InvalidPage),
        (Fault::ShortFull, E::InvalidPage),
        (Fault::Unavailable, E::Source(S::Unavailable)),
    ] {
        let fixture = CatalogFixture::new(if fault == Fault::ShortFull { 3 } else { 300 })?;
        fixture.fault.set(fault);
        let result = collect_governance_catalog(
            &fixture,
            fixture.request(5),
            CatalogFixture::load,
            Ord::cmp,
            Limits::default(),
        );
        assert!(
            matches!(result, Err(actual) if actual == expected),
            "{fault:?}"
        );
    }
    Ok(())
}

#[test]
fn catalog_collection_admits_before_reads_and_checks_complete_loader_result_ownership() -> TestResult
{
    let fixture = CatalogFixture::new(3)?;
    let limits = Limits {
        workspace_bytes: 1,
        ..Limits::default()
    };
    let result = collect_governance_catalog(
        &fixture,
        fixture.request(5),
        CatalogFixture::load,
        Ord::cmp,
        limits,
    );
    assert!(matches!(result, Err(E::Limit)));
    assert_eq!(fixture.calls.borrow().len(), 0);
    let result = collect_governance_keys(
        &[b"a"],
        1,
        |_, _| {
            Ok(Some(Loaded {
                record: vec![1_u8],
                owned_bytes: 0,
            }))
        },
        Ord::cmp,
        Limits::default(),
    );
    assert!(matches!(result, Err(E::InvalidLoader)));
    let limits = Limits {
        maximum_result_bytes: 1,
        ..Limits::default()
    };
    let result = collect_governance_keys(&[b"a"], 1, CatalogFixture::load, Ord::cmp, limits);
    assert!(matches!(result, Err(E::Limit)));
    Ok(())
}
