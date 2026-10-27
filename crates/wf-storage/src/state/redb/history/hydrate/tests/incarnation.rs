//! A valid textual value closure cannot override the captured workflow birth.

use wf_ports::blob::WorkflowValueSizedReadValue;

use super::*;
use crate::state::value;

#[test]
fn history_hydration_rejects_live_alias_incarnation_mismatch() -> TestResult {
    let (path, store, scope) = fixture("alias-incarnation")?;
    let partition = PartitionId::new(b"p".to_vec())?;
    let id = RecordId::new(b"i".to_vec())?;
    let mut clock = clock()?;
    let healthy = store.read_workflow_history_page_with_values(
        &mut clock,
        &scope,
        &partition,
        &id,
        query(100),
        limits(),
    );
    assert!(
        healthy.is_ok(),
        "healthy history hydration: {:?}",
        healthy.as_ref().err()
    );
    healthy?;
    let before = rows(&store, false)?;
    let reference = reference(1)?;
    let alias_key = key::encode_alias_key(&scope, &reference, 65_535)?;
    let mut alias = value::decode_alias(before.get(&alias_key).ok_or("real alias")?)?;
    assert_eq!(alias.incarnation, 0);
    let old_key = key::encode_immutable_key(&scope, &reference, alias.incarnation, 65_535)?;
    let bytes = before.get(&old_key).ok_or("real immutable")?;
    assert_eq!(bytes, b"first");
    alias.incarnation = 1;
    let wrong_key = key::encode_immutable_key(&scope, &reference, alias.incarnation, 65_535)?;
    assert!(!before.contains_key(&wrong_key));
    let changed = value::encode_alias(alias, 65_535)?;
    let mut write = store.database.begin_write()?;
    write.set_durability(Durability::Immediate)?;
    let mut table = write.open_table(ROWS)?;
    assert!(table.remove(old_key.as_slice())?.is_some());
    assert!(
        table
            .insert(wrong_key.as_slice(), bytes.as_slice())?
            .is_none()
    );
    assert!(
        table
            .insert(alias_key.as_slice(), changed.as_slice())?
            .is_some()
    );
    drop(table);
    write.commit()?;
    let standalone = store.read_workflow_value_batch_with_sizes(
        &mut clock,
        &scope,
        &[reference.as_slice()],
        100,
        limits().values,
    )?;
    assert_eq!(standalone.slots.as_slice(), [Ok(Some(0))]);
    assert!(
        matches!(standalone.values.as_slice(), [WorkflowValueSizedReadValue::Stored(actual)] if actual.as_slice() == b"first")
    );
    let result = store.read_workflow_history_page_with_values(
        &mut clock,
        &scope,
        &partition,
        &id,
        query(-1),
        limits(),
    );
    assert!(
        matches!(result, Err(Error::State(StateTransactionError::Corrupt))),
        "{:?}",
        result.as_ref().err()
    );
    drop(store);
    std::fs::remove_dir_all(path)?;
    Ok(())
}
