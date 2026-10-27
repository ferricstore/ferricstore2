//! Reader-specific corruption tests on genuinely published live rows.

use wf_domain::workflow::request::RequestScope;
use wf_ports::blob::WorkflowValueBindingKnowledge;
use wf_ports::state::StateTransactionError;
use wf_types::identity::{PartitionId, RecordId, TenantId};

use crate::state::{key, value};

mod fixtures;
use fixtures::fixture_data::{fixture, limits, query, rewrite, rows};

type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;

#[test]
fn history_page_detects_scope_identity_binding_and_corruption() -> TestResult {
    for fault in 0..5 {
        let (path, store, scope) = fixture(&format!("closure-{fault}"))?;
        let partition = PartitionId::new(b"p".to_vec())?;
        let id = RecordId::new(b"i".to_vec())?;
        let healthy =
            store.read_workflow_history_page(&scope, &partition, &id, query(10), limits());
        assert!(
            healthy.is_ok(),
            "healthy history capture: {:?}",
            healthy.as_ref().err()
        );
        assert_eq!(healthy?.entries.len(), 2);
        let before = rows(&store, false)?;
        let meta = rows(&store, true)?;
        let prefix = key::encode_history_prefix(&scope, &partition, &id, 65_535)?;
        let history_key = before
            .keys()
            .find(|key| key.starts_with(&prefix))
            .ok_or("history")?;
        let event = value::decode_history(
            before.get(history_key).ok_or("event")?,
            &scope,
            &partition,
            &id,
            65_535,
        )?;
        let binding_key =
            key::encode_binding_key(&scope, &partition, &id, Some(&event.event_id), 65_535)?;
        corrupt(&store, &scope, &before, history_key, &binding_key, fault)?;
        let changed = rows(&store, false)?;
        let result = store.read_workflow_history_page(&scope, &partition, &id, query(10), limits());
        assert!(
            matches!(result, Err(StateTransactionError::Corrupt)),
            "fault {fault}: {:?}",
            result.as_ref().err()
        );
        assert_eq!(rows(&store, false)?, changed);
        assert_eq!(rows(&store, true)?, meta);
        assert!(matches!(
            store.read_workflow_history_page(&scope, &partition, &id, query(10), limits()),
            Err(StateTransactionError::Unavailable)
        ));
        drop(store);
        std::fs::remove_dir_all(path)?;
    }
    Ok(())
}

fn corrupt(
    store: &crate::state::redb::RedbStateStore,
    scope: &RequestScope,
    before: &std::collections::BTreeMap<Vec<u8>, Vec<u8>>,
    history_key: &[u8],
    binding_key: &[u8],
    fault: usize,
) -> TestResult {
    let partition = PartitionId::new(b"p".to_vec())?;
    let id = RecordId::new(b"i".to_vec())?;
    match fault {
        0 | 4 => {
            let mut event = value::decode_history(
                before.get(history_key).ok_or("event")?,
                scope,
                &partition,
                &id,
                65_535,
            )?;
            if fault == 0 {
                event.event_id = "101-1".into();
            }
            let foreign = RequestScope::new(
                TenantId::new(b"other".to_vec())?,
                scope.group().clone(),
                scope.recovery().clone(),
            );
            let selected = if fault == 4 { &foreign } else { scope };
            let changed = value::encode_history(selected, &partition, &id, &event, 65_535)?;
            rewrite(store, history_key, Some(&changed))?;
        }
        1 => {
            let donor = key::encode_binding_key(
                scope,
                &partition,
                &RecordId::new(b"j".to_vec())?,
                None,
                65_535,
            )?;
            rewrite(
                store,
                binding_key,
                Some(before.get(&donor).ok_or("real donor")?),
            )?;
        }
        2 => rewrite(store, binding_key, None)?,
        3 => {
            let mut changed = before.get(binding_key).ok_or("binding")?.clone();
            let last = changed.last_mut().ok_or("empty binding")?;
            *last ^= 1;
            rewrite(store, binding_key, Some(&changed))?;
        }
        _ => return Err("unknown fault".into()),
    }
    Ok(())
}

#[test]
fn history_page_rejects_bad_lookahead_and_keeps_missing_records_distinct() -> TestResult {
    for fault in 0..3 {
        let (path, store, scope) = fixture(&format!("peek-{fault}"))?;
        let partition = PartitionId::new(b"p".to_vec())?;
        let id = RecordId::new(b"i".to_vec())?;
        let healthy = store.read_workflow_history_page(&scope, &partition, &id, query(1), limits());
        assert!(
            healthy.is_ok(),
            "healthy history capture: {:?}",
            healthy.as_ref().err()
        );
        let healthy = healthy?;
        assert_eq!(healthy.entries[0].event.event_id, "100-1");
        assert_eq!(
            healthy
                .lookahead
                .as_ref()
                .ok_or("lookahead")?
                .event
                .event_id,
            "100-2"
        );
        let missing = store.read_workflow_history_page(
            &scope,
            &partition,
            &RecordId::new(b"missing".to_vec())?,
            query(1),
            limits(),
        )?;
        assert!(missing.record_version.is_none());
        assert!(missing.entries.is_empty());
        assert!(missing.lookahead.is_none());
        assert!(missing.continuation.is_none());
        assert!(matches!(
            missing.current_values,
            WorkflowValueBindingKnowledge::Known(None)
        ));
        let before = rows(&store, false)?;
        let key = if fault == 0 {
            key::encode_history_key(
                &scope,
                &partition,
                &id,
                &healthy.lookahead.as_ref().ok_or("peek")?.event,
                65_535,
            )?
        } else {
            key::encode_binding_key(
                &scope,
                &partition,
                &id,
                (fault == 1).then_some("100-2"),
                65_535,
            )?
        };
        assert!(before.contains_key(&key));
        if fault == 0 {
            let mut bytes = before.get(&key).ok_or("peek row")?.clone();
            bytes[0] ^= 1;
            rewrite(&store, &key, Some(&bytes))?;
        } else {
            rewrite(&store, &key, None)?;
        }
        let result = store.read_workflow_history_page(&scope, &partition, &id, query(1), limits());
        assert!(
            matches!(result, Err(StateTransactionError::Corrupt)),
            "peek {fault}: {:?}",
            result.as_ref().err()
        );
        drop(store);
        std::fs::remove_dir_all(path)?;
    }
    Ok(())
}
