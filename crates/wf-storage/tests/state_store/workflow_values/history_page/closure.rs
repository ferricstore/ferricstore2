//! Namespace isolation, owned results and independently bounded observations.

use wf_types::identity::{GroupId, RecoveryId, TenantId};

use super::*;

#[test]
fn history_page_owns_binary_namespaces_and_never_leaks_neighbors() -> TestResult {
    let directory = crate::fixtures::directory("history-ownership")?;
    let mut store = fresh(&directory.join("state"))?;
    let scope = crate::fixtures::scope()?;
    let owners: [&[u8]; 4] = [b"i", b"i\0", b"i\xff", b"j"];
    let items = owners
        .iter()
        .map(|id| create(id, Some(id)))
        .collect::<TestResult<Vec<_>>>()?;
    apply(&mut store, &batch(b"neighbors", items)?, 0)?;
    let mut captured = read(&store, &scope, b"i", query(10))?;
    for id in owners {
        let page = read(&store, &scope, id, query(10))?;
        assert_eq!(ids(&page), ["100-1"]);
        let WorkflowValueBindingKnowledge::Known(Some(binding)) = &page.entries[0].values else {
            return Err("owner binding missing".into());
        };
        assert!(
            matches!(&binding.refs.payload, Some(ActiveValueRef::Versioned { id: actual, .. }) if actual.as_bytes() == id)
        );
    }
    let mut other = batch(b"other-tenant", vec![create(b"i", Some(b"other"))?])?;
    other.scope = RequestScope::new(
        TenantId::new(b"t\0".to_vec())?,
        scope.group().clone(),
        scope.recovery().clone(),
    );
    apply(&mut store, &other, 1)?;
    assert_eq!(
        ids(&read(&store, &other.scope, b"i", query(10))?),
        ["100-1"]
    );
    assert_eq!(ids(&read(&store, &scope, b"i", query(10))?), ["100-1"]);
    let wrong = RequestScope::new(
        scope.tenant().clone(),
        GroupId::new(b"foreign".to_vec())?,
        RecoveryId::new(b"z".to_vec())?,
    );
    assert!(matches!(
        store.read_workflow_history_page(
            &wrong,
            &PartitionId::new(b"p".to_vec())?,
            &RecordId::new(b"i".to_vec())?,
            query(10),
            limits()
        ),
        Err(StateTransactionError::InvalidChanges)
    ));
    captured.entries[0].event.state[0] = b'x';
    assert_eq!(
        read(&store, &scope, b"i", query(10))?.entries[0]
            .event
            .state,
        b"q"
    );
    drop(other);
    drop(scope);
    drop(store);
    assert_eq!(ids(&captured), ["100-1"]);
    assert_eq!(captured.entries[0].event.state, b"x");
    assert_eq!(
        versions(&captured.entries[0].values)?,
        [Some(1), None, None]
    );
    std::fs::remove_dir_all(directory)?;
    Ok(())
}

#[test]
fn history_page_enforces_control_and_resource_limits_without_mutation() -> TestResult {
    let directory = crate::fixtures::directory("history-limits")?;
    let path = directory.join("state");
    let mut store = fresh(&path)?;
    let scope = crate::fixtures::scope()?;
    apply(
        &mut store,
        &batch(b"birth", vec![create(b"i", Some(b"payload"))?])?,
        0,
    )?;
    let old = read(&store, &scope, b"i", query(1))?;
    apply(
        &mut store,
        &batch(b"next", vec![transition(b"i", b"q", b"a", None)?])?,
        1,
    )?;
    let mut expected = query(1);
    expected.expected_control = Some(old.control);
    let partition = PartitionId::new(b"p".to_vec())?;
    let id = RecordId::new(b"i".to_vec())?;
    assert!(matches!(
        store.read_workflow_history_page(&scope, &partition, &id, expected, limits()),
        Err(StateTransactionError::KnownAborted)
    ));
    drop(store);
    let rows = crate::fixtures::raw_rows(&path)?;
    let meta = metadata(&path)?;
    let store = crate::fixtures::open(&path)?;
    for case in 0..6 {
        let mut small = limits();
        match case {
            0 => small.maximum_entries = 1,
            1 => small.maximum_row_reads = 1,
            2 => small.maximum_inspected_bytes = 1,
            3 => small.maximum_returned_bytes = 1,
            4 => small.maximum_work = 1,
            _ => small.maximum_temporary_bytes = 1,
        }
        assert!(
            matches!(
                store.read_workflow_history_page(&scope, &partition, &id, query(2), small),
                Err(StateTransactionError::Limit)
            ),
            "budget {case}"
        );
        assert_eq!(
            ids(&read(&store, &scope, b"i", query(10))?),
            ["100-1", "100-2"]
        );
    }
    assert!(matches!(
        store.read_workflow_history_page(&scope, &partition, &id, query(0), limits()),
        Err(StateTransactionError::InvalidChanges)
    ));
    let mut huge = limits();
    huge.maximum_entries = usize::MAX;
    assert!(matches!(
        store.read_workflow_history_page(&scope, &partition, &id, query(usize::MAX), huge),
        Err(StateTransactionError::Limit)
    ));
    let current = read(&store, &scope, b"i", query(10))?;
    expected.expected_control = Some(current.control);
    assert_eq!(ids(&read(&store, &scope, b"i", expected)?), ["100-1"]);
    drop(store);
    assert_eq!(crate::fixtures::raw_rows(&path)?, rows);
    assert_eq!(metadata(&path)?, meta);
    std::fs::remove_dir_all(directory)?;
    Ok(())
}
