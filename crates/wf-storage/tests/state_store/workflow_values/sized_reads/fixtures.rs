//! Real value publication and independent read-oracle fixtures.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use redb::{ReadableDatabase, ReadableTable};
use wf_domain::workflow::request::{RequestScope, RetainedStatus};
use wf_domain::workflow::{Cancel, Create, Operation, Transition};
use wf_ports::blob::{
    OriginalValuePartition, OriginalValueRefKind, WorkflowValueSizedReadBatch,
    WorkflowValueSizedReadValue, render_original_named_value_ref, render_original_value_ref,
};
use wf_ports::consensus::PreparedOperation;
use wf_ports::log::encode_workflow_value_batch;
use wf_ports::state::StateTransactionError;
use wf_storage::state::RedbStateStore;
use wf_storage::transaction::{
    RecordBatchTransactionOutcome, apply_workflow_value_batch_transaction,
};
use wf_types::identity::{GroupId, PartitionId, RecordId, RecoveryId};
use wf_types::version::{Fence, HlcTimestamp, TimestampMillis};

use super::{TestResult, f};

pub(super) fn setup(tag: &str) -> TestResult<(PathBuf, RedbStateStore, RequestScope)> {
    let directory = crate::fixtures::directory(tag)?;
    let fresh = f::fresh(&directory.join("state"))?;
    assert!(fresh.is_ok(), "{:?}", fresh.as_ref().err());
    Ok((directory, fresh?, crate::fixtures::scope()?))
}

pub(super) fn read_clock(now: u64) -> TestResult<f::ValueReadClock> {
    Ok(f::ValueReadClock {
        hlc: HlcTimestamp::new(now, 0)?,
        wall: TimestampMillis::new(now),
        calls: 0,
        failure: None,
    })
}

pub(super) fn reference(
    id: &[u8],
    kind: OriginalValueRefKind,
    version: u64,
) -> TestResult<Vec<u8>> {
    render_original_value_ref(
        id,
        kind,
        version,
        OriginalValuePartition::Explicit(b"p"),
        65_535,
    )
    .map_err(|error| format!("reference fixture: {error:?}").into())
}

pub(super) fn unsupported_refs() -> TestResult<[Vec<u8>; 2]> {
    let shared = reference(b"shared", OriginalValueRefKind::Shared, 1)?;
    let named = render_original_named_value_ref(
        b"owner",
        b"name",
        1,
        OriginalValuePartition::Explicit(b"p"),
        65_535,
    )
    .map_err(|error| format!("named fixture: {error:?}"))?;
    Ok([shared, named])
}

pub(super) fn assert_values(
    batch: &WorkflowValueSizedReadBatch,
    stored: &[(usize, &[u8])],
    omitted: &[(usize, usize)],
) -> TestResult {
    assert_eq!(batch.values.len(), stored.len() + omitted.len());
    for (index, expected) in stored {
        let Some(WorkflowValueSizedReadValue::Stored(actual)) = batch.values.get(*index) else {
            return Err(format!("expected stored bytes at {index}").into());
        };
        assert_eq!(actual.as_slice(), *expected);
    }
    for (index, expected) in omitted {
        assert!(
            matches!(batch.values.get(*index), Some(WorkflowValueSizedReadValue::Omitted { stored_bytes }) if stored_bytes == expected)
        );
    }
    Ok(())
}

pub(super) fn publish(
    store: &mut RedbStateStore,
    scope: &RequestScope,
    values: &[(&[u8], &[u8])],
) -> TestResult<Vec<Vec<u8>>> {
    let mut items = Vec::new();
    let mut refs = Vec::new();
    for (id, payload) in values {
        let create = Operation::Create(Create {
            workflow_type: b"r".to_vec(),
            state: b"q".to_vec(),
            run_at: None,
            payload: Some(payload.to_vec()),
        });
        items.push(f::item(id, 200, create)?);
        refs.push(reference(id, OriginalValueRefKind::Payload, 1)?);
    }
    let inputs = (0..values.len())
        .map(|index| Ok(f::input(u32::try_from(index)?, 0)))
        .collect::<TestResult<Vec<_>>>()?;
    let prepared = f::batch(
        5,
        b"sized-publish",
        200,
        PreparedOperation::Batch(items),
        inputs,
        vec![f::witness(b"r", None)?],
    )?;
    let mut limits = f::limits();
    // A full 1MiB source value also funds the existing 512*E producer cohort.
    limits.maximum_value_work = 1_024 * crate::fixtures::MIB;
    let encoded = encode_workflow_value_batch(&prepared, limits.record_batch.codec)?;
    let applied = apply_workflow_value_batch_transaction(
        store,
        scope,
        encoded.as_bytes(),
        crate::fixtures::log(0),
        limits,
    )?;
    let RecordBatchTransactionOutcome::Retained { outcome, .. } = applied else {
        return Err("expected consumed retained publication".into());
    };
    assert_eq!(outcome.status(), RetainedStatus::Accepted);
    assert_eq!(outcome.original_position(), crate::fixtures::log(0));
    assert_eq!(outcome.response(), [0]);
    let borrowed = refs.iter().map(Vec::as_slice).collect::<Vec<_>>();
    let old = store.read_workflow_value_batch(
        &mut read_clock(201)?,
        scope,
        &borrowed,
        f::read_limits(),
    )?;
    assert_eq!(old.values.len(), values.len());
    for (index, (_, payload)) in values.iter().enumerate() {
        let expected = if payload.starts_with(b"FSV2") {
            [b"FSV2\x01".as_slice(), payload].concat()
        } else {
            payload.to_vec()
        };
        assert_eq!(old.values[index], expected);
        assert_eq!(old.slots[index], Ok(Some(index)));
    }
    Ok(refs)
}

pub(super) fn publish_steps(
    store: &mut RedbStateStore,
    scope: &RequestScope,
    terminal: bool,
) -> TestResult<Vec<Vec<u8>>> {
    let create = Operation::Create(Create {
        workflow_type: b"r".to_vec(),
        state: b"q".to_vec(),
        run_at: None,
        payload: Some(b"FSV2\x02opaque".to_vec()),
    });
    let next = if terminal {
        Operation::Cancel(Cancel {
            fence: Fence::new(0),
            lease_token: None,
            expected_state: Some(b"q".to_vec()),
            expected_version: Some(1),
            error: Some(b"e".to_vec()),
        })
    } else {
        Operation::Transition(Transition {
            from: b"q".to_vec(),
            to: b"waiting".to_vec(),
            fence: Fence::new(0),
            lease_token: None,
            run_at: None,
            payload: Some(Vec::new()),
        })
    };
    let now = if terminal { 200 } else { 100 };
    let mut input = f::input(1, 0);
    input.terminal_ttl_ms = terminal.then_some(50);
    let prepared = f::batch(
        6,
        b"sized-steps",
        now,
        PreparedOperation::Batch(vec![
            f::item(b"snap", now, create)?,
            f::item(b"snap", now, next)?,
        ]),
        vec![f::input(0, 0), input],
        vec![f::witness(b"r", None)?],
    )?;
    let applied = f::apply_value(store, scope, &prepared, 0)?;
    let RecordBatchTransactionOutcome::Retained { outcome, .. } = applied else {
        return Err("expected retained steps".into());
    };
    assert_eq!(outcome.status(), RetainedStatus::Accepted);
    let refs = vec![
        reference(b"snap", OriginalValueRefKind::Payload, 1)?,
        reference(
            b"snap",
            if terminal {
                OriginalValueRefKind::Error
            } else {
                OriginalValueRefKind::Payload
            },
            2,
        )?,
    ];
    let old = store.read_workflow_value_batch(
        &mut read_clock(if terminal { 249 } else { 201 })?,
        scope,
        &[&refs[0], &refs[1]],
        f::read_limits(),
    )?;
    f::assert_read(
        &old,
        &[b"FSV2\x01FSV2\x02opaque", if terminal { b"e" } else { b"" }],
        &[Some(0), Some(1)],
    )?;
    Ok(refs)
}

pub(super) fn assert_history(store: &RedbStateStore, scope: &RequestScope) -> TestResult {
    let partition = PartitionId::new(b"p".to_vec())?;
    let id = RecordId::new(b"snap".to_vec())?;
    for (event, version) in [(None, 2), (Some("100-1"), 1), (Some("100-2"), 2)] {
        let binding =
            store.workflow_value_binding(scope, &partition, &id, event, f::read_limits())?;
        f::assert_binding(
            &binding,
            (b"p", b"snap"),
            0,
            [Some(version), None, None],
            (None, 7_000, 2, 10, Some(7_000), None),
            &f::witness(b"r", None)?,
        )?;
    }
    Ok(())
}

pub(super) fn metadata(path: &Path) -> TestResult<BTreeMap<Vec<u8>, Vec<u8>>> {
    let database = redb::Builder::new().open(path)?;
    let read = database.begin_read()?;
    let table = read.open_table(crate::fixtures::META)?;
    let rows = table
        .iter()?
        .map(|row| {
            let (key, value) = row?;
            Ok((key.value().to_vec(), value.value().to_vec()))
        })
        .collect::<TestResult<BTreeMap<_, _>>>()?;
    assert_eq!(
        rows.keys().map(Vec::as_slice).collect::<Vec<_>>(),
        [
            b"manifest".as_slice(),
            b"membership",
            b"origin",
            b"snapshot"
        ]
    );
    Ok(rows)
}

pub(super) fn assert_scope_refusals(
    store: &RedbStateStore,
    clock: &mut f::ValueReadClock,
    scope: &RequestScope,
    refs: &[&[u8]],
) -> TestResult {
    let wrong = RequestScope::new(
        scope.tenant().clone(),
        GroupId::new(b"other".to_vec())?,
        scope.recovery().clone(),
    );
    assert!(matches!(
        store.read_workflow_value_batch_with_sizes(clock, &wrong, refs, 3, f::read_limits()),
        Err(StateTransactionError::InvalidChanges)
    ));
    let wrong_recovery = RequestScope::new(
        scope.tenant().clone(),
        scope.group().clone(),
        RecoveryId::new(b"other".to_vec())?,
    );
    assert!(matches!(
        store.read_workflow_value_batch_with_sizes(
            clock,
            &wrong_recovery,
            refs,
            3,
            f::read_limits()
        ),
        Err(StateTransactionError::InvalidChanges)
    ));
    assert!(matches!(
        store.read_workflow_value_batch_with_sizes(clock, scope, refs, 0, f::read_limits()),
        Err(StateTransactionError::InvalidChanges)
    ));
    Ok(())
}

pub(super) fn assert_source_catalog(
    rows: &BTreeMap<Vec<u8>, Vec<u8>>,
    source_meta: &BTreeMap<Vec<u8>, Vec<u8>>,
    initial_meta: &BTreeMap<Vec<u8>, Vec<u8>>,
) {
    for name in [b"manifest".as_slice(), b"membership", b"origin"] {
        assert_eq!(source_meta.get(name), initial_meta.get(name));
    }
    for (family, count) in [(8, 2), (9, 2), (10, 1), (11, 2)] {
        assert_eq!(
            rows.keys()
                .filter(|key| key.starts_with(&[b'F', b'W', b'P', b'K', 0, 1, family]))
                .count(),
            count
        );
    }
}
