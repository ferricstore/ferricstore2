//! Real workflow writer and queue composition; paid Claim remains a successor.
use super::*;
use wf_domain::workflow::{Cancel, request::RetainedStatus};
use wf_ports::{
    log::{PreparedBytes, encode_workflow_stored_value_batch},
    state::{GroupApplyStore, GroupApplyTransaction},
};
use wf_storage::transaction::{
    RecordBatchTransactionOutcome as O, apply_workflow_release_transaction,
};
use wf_types::{identity::RecordId, version::Fence};

fn frame(request: &[u8], profile: u8, ids: &[&[u8]]) -> TestResult<PreparedBytes> {
    let mut items = Vec::new();
    for id in ids {
        items.push(f::item(
            id,
            200,
            Operation::Cancel(Cancel {
                fence: Fence::new(0),
                lease_token: None,
                expected_state: Some(b"q".to_vec()),
                expected_version: Some(1),
                error: None,
            }),
        )?);
    }
    let inputs = (0..ids.len())
        .map(|n| Ok(f::input(u32::try_from(n)?, 0)))
        .collect::<TestResult<Vec<_>>>()?;
    Ok(encode_workflow_stored_value_batch(
        &f::batch(
            profile,
            request,
            200,
            PreparedOperation::Batch(items),
            inputs,
            vec![f::witness(b"r", None)?],
        )?,
        f::limits().record_batch.codec,
    )?)
}

#[test]
fn workflow_release_terminal_write_commits_intent_and_replays_without_duplicate_append()
-> TestResult {
    for profile in [5, 6] {
        let dir = crate::fixtures::directory("release-atomic-terminal")?;
        let path = dir.join("state");
        fixture(&path)?;
        edit(&path, &[(key(20, None, b"i")?, binding(b"i")?)], &[])?;
        let mut store = open(&path)?;
        let scope = crate::fixtures::scope()?;
        let prepared = frame(b"cancel", profile, &[b"i"])?;
        let applied = apply_workflow_release_transaction(
            &mut store,
            &scope,
            prepared.as_bytes(),
            crate::fixtures::log(1),
            f::limits(),
        )?;
        let O::Retained { outcome, .. } = applied else {
            return Err("missing original terminal reply".into());
        };
        assert_eq!(outcome.status(), RetainedStatus::Accepted);
        let page = store.read_workflow_release_outbox(&scope, None, 256, 1000, 65536)?;
        assert_eq!(
            (page.meta.head, page.meta.tail, page.entries.len()),
            (1, 1, 1)
        );
        assert_eq!(page.entries[0].intent()?.reservation_id, b"paid\0id");
        assert_eq!(page.entries[0].intent()?.created_at.get(), 200);
        let tx = store.begin_apply(
            scope.group(),
            scope.recovery(),
            f::limits().record_batch.codec.admission(),
            1000,
            65536,
            65536,
        )?;
        let record = tx
            .record(
                scope.tenant(),
                &PartitionId::new(b"p".to_vec())?,
                &RecordId::new(b"i".to_vec())?,
            )?
            .ok_or("missing terminal record")?;
        assert_eq!(record.record.state(), b"cancelled");
        assert_eq!(record.record.version(), 2);
        drop(tx);
        let replay = apply_workflow_release_transaction(
            &mut store,
            &scope,
            prepared.as_bytes(),
            crate::fixtures::log(2),
            f::limits(),
        )?;
        let O::Retained {
            outcome: replay, ..
        } = replay
        else {
            return Err("missing original replay".into());
        };
        assert_eq!(*replay, *outcome);
        assert_eq!(
            store
                .read_workflow_release_outbox(&scope, None, 256, 1000, 65536)?
                .meta
                .tail,
            1
        );
        drop(store);
        drop(open(&path)?);
        std::fs::remove_dir_all(dir)?;
    }
    Ok(())
}

#[test]
fn workflow_release_independent_final_writes_deduplicate_in_original_source_order() -> TestResult {
    let dir = crate::fixtures::directory("release-atomic-dedup")?;
    let path = dir.join("state");
    fixture(&path)?;
    let mut store = open(&path)?;
    super::super::claim::fixtures::create(&mut store, b"j", b"q", (1, 100, 0), None)?;
    drop(store);
    edit(
        &path,
        &[
            (key(20, None, b"i")?, binding(b"i")?),
            (key(20, None, b"j")?, binding(b"j")?),
        ],
        &[],
    )?;
    let mut store = open(&path)?;
    let scope = crate::fixtures::scope()?;
    let prepared = frame(b"cancel-both", 6, &[b"j", b"i"])?;
    let applied = apply_workflow_release_transaction(
        &mut store,
        &scope,
        prepared.as_bytes(),
        crate::fixtures::log(2),
        f::limits(),
    )?;
    assert!(
        matches!(applied, O::Retained { outcome, .. } if outcome.status() == RetainedStatus::Accepted)
    );
    let page = store.read_workflow_release_outbox(&scope, None, 256, 1000, 65536)?;
    assert_eq!((page.meta.tail, page.entries.len()), (1, 1));
    assert_eq!(page.entries[0].intent()?.flow_id, b"j");
    drop(store);
    drop(open(&path)?);
    std::fs::remove_dir_all(dir)?;
    Ok(())
}

#[test]
fn workflow_release_full_queue_retains_business_rejection_without_workflow_prefix() -> TestResult {
    let dir = crate::fixtures::directory("release-atomic-full")?;
    let path = dir.join("state");
    fixture(&path)?;
    edit(&path, &[(key(20, None, b"i")?, binding(b"i")?)], &[])?;
    let database = redb::Database::open(&path)?;
    let tx = database.begin_write()?;
    {
        let mut rows = tx.open_table(redb::TableDefinition::<&[u8], &[u8]>::new(
            "wf_state_rows_v1",
        ))?;
        let meta = encode_release_meta(
            ReleaseOutboxState {
                head: 1,
                tail: 65536,
            },
            22,
        )?;
        rows.insert(key(21, None, b"")?.as_slice(), meta.as_slice())?;
        let original = intent(b"i", b"previous-release")?;
        for sequence in 1..=65536 {
            rows.insert(
                key(22, Some(sequence), b"")?.as_slice(),
                original.as_slice(),
            )?;
        }
    }
    tx.commit()?;
    drop(database);
    let before = crate::fixtures::raw_rows(&path)?;
    let mut store = open(&path)?;
    let scope = crate::fixtures::scope()?;
    let prepared = frame(b"full", 5, &[b"i"])?;
    let applied = apply_workflow_release_transaction(
        &mut store,
        &scope,
        prepared.as_bytes(),
        crate::fixtures::log(1),
        f::limits(),
    )?;
    let O::Retained { outcome, .. } = applied else {
        return Err("queue full must retain a business reply".into());
    };
    assert_eq!(outcome.response(), [1, 7, 0]);
    assert!(matches!(
        outcome.status(),
        RetainedStatus::IndexedRejected(_)
    ));
    assert_eq!(outcome.original_position(), crate::fixtures::log(1));
    drop(store);
    let after = crate::fixtures::raw_rows(&path)?;
    for (key, bytes) in &before {
        if key[6] != 0 {
            assert_eq!(after.get(key), Some(bytes));
        }
    }
    let added = after
        .keys()
        .filter(|key| !before.contains_key(*key))
        .collect::<Vec<_>>();
    assert_eq!(added.len(), 1);
    assert_eq!(added[0][6], 2);
    assert_eq!(after.len(), before.len() + 1);
    drop(open(&path)?);
    std::fs::remove_dir_all(dir)?;
    Ok(())
}
