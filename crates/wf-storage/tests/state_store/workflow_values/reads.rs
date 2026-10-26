//! Real ordered reads, independent value expiry and one server clock snapshot.

use wf_domain::workflow::{Cancel, Create, Operation};
use wf_ports::blob::{
    OriginalValuePartition, OriginalValueRefKind, WorkflowValueReadFailure,
    parse_original_value_ref, render_original_value_ref,
};
use wf_ports::consensus::PreparedOperation;
use wf_ports::state::StateTransactionError;
use wf_types::version::{Fence, HlcTimestamp, TimestampMillis};

use super::{TestResult, fixtures as f};

#[test]
fn value_binary_ordered_reads_preserve_duplicates_missing_and_expiry() -> TestResult {
    let directory = crate::fixtures::directory("value-ordered-reads")?;
    let fresh = f::fresh(&directory.join("state"))?;
    assert!(fresh.is_ok(), "{:?}", fresh.as_ref().err());
    let Ok(mut store) = fresh else {
        return Err("expected fresh value capability".into());
    };
    let scope = crate::fixtures::scope()?;
    let create = Operation::Create(Create {
        workflow_type: b"r".to_vec(),
        state: b"q".to_vec(),
        run_at: None,
        payload: Some(b"FSV2\x02opaque".to_vec()),
    });
    let cancel = Operation::Cancel(Cancel {
        fence: Fence::new(0),
        lease_token: None,
        expected_state: Some(b"q".to_vec()),
        expected_version: Some(1),
        error: Some(Vec::new()),
    });
    let mut terminal = f::input(1, 0);
    terminal.terminal_ttl_ms = Some(50);
    let operation = PreparedOperation::Batch(vec![
        f::item(b"read", 200, create)?,
        f::item(b"read", 200, cancel)?,
    ]);
    let inputs = vec![f::input(0, 0), terminal];
    let policies = vec![f::witness(b"r", None)?];
    let prepared = f::batch(5, b"read", 200, operation, inputs, policies)?;
    f::apply_value(&mut store, &scope, &prepared, 0)?;
    drop(prepared);
    let partition = OriginalValuePartition::Explicit(b"p");
    let kinds = [
        (OriginalValueRefKind::Payload, 1),
        (OriginalValueRefKind::Error, 2),
    ];
    let references = kinds
        .into_iter()
        .map(|(kind, version)| render_original_value_ref(b"read", kind, version, partition, 65_535))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("value reference fixture: {error:?}"))?;
    let refs = [
        references[0].as_slice(),
        b"f:{f}:v:p:missing:1",
        references[1].as_slice(),
        references[0].as_slice(),
        b"",
    ];
    let mut clock = f::ValueReadClock {
        hlc: HlcTimestamp::new(201, 0)?,
        wall: TimestampMillis::new(201),
        calls: 0,
        failure: None,
    };
    let expected = b"FSV2\x01FSV2\x02opaque".as_slice();
    let live = store.read_workflow_value_batch(&mut clock, &scope, &refs, f::read_limits());
    assert!(live.is_ok(), "{:?}", live.as_ref().err());
    let live = live?;
    f::assert_read(
        &live,
        &[expected, b""],
        &[Some(0), None, Some(1), Some(0), None],
    )?;
    assert_eq!(clock.calls, 1);
    clock.hlc = HlcTimestamp::new(250, 0)?;
    clock.wall = TimestampMillis::new(250);
    let expired = store.read_workflow_value_batch(&mut clock, &scope, &refs, f::read_limits());
    assert!(expired.is_ok(), "{:?}", expired.as_ref().err());
    let expired = expired?;
    f::assert_read(&expired, &[expected], &[Some(0), None, None, Some(0), None])?;
    assert_eq!(clock.calls, 2);
    clock.hlc = HlcTimestamp::new(1_500, 0)?;
    clock.wall = TimestampMillis::new(249);
    let unsafe_read = store.read_workflow_value_batch(&mut clock, &scope, &refs, f::read_limits());
    assert!(unsafe_read.is_ok(), "{:?}", unsafe_read.as_ref().err());
    let unsafe_read = unsafe_read?;
    assert_eq!(
        unsafe_read.slots[2],
        Err(WorkflowValueReadFailure::UnsafeDrift)
    );
    assert_eq!(unsafe_read.values.as_slice(), [expected.to_vec()]);
    assert_eq!(clock.calls, 3);
    drop(store);
    drop(references);
    assert_eq!(live.values.as_slice(), [expected.to_vec(), Vec::new()]);
    std::fs::remove_dir_all(directory)?;
    Ok(())
}

#[test]
fn value_binary_read_clock_is_sampled_once_and_failure_stays_operational() -> TestResult {
    let mut oversized = b"f:{f}:v:p:".to_vec();
    oversized.resize(65_534, b'x');
    oversized.extend_from_slice(b":1");
    assert_eq!(oversized.len(), 65_536);
    assert!(parse_original_value_ref(&oversized).is_some());
    let directory = crate::fixtures::directory("value-clock-read")?;
    let fresh = f::fresh(&directory.join("state"))?;
    assert!(fresh.is_ok(), "{:?}", fresh.as_ref().err());
    let Ok(mut store) = fresh else {
        return Err("expected fresh value capability".into());
    };
    let scope = crate::fixtures::scope()?;
    let operation = Operation::Create(Create {
        workflow_type: b"r".to_vec(),
        state: b"q".to_vec(),
        run_at: None,
        payload: Some(b"clock".to_vec()),
    });
    let prepared = f::batch(
        5,
        b"read-clock-create",
        100,
        PreparedOperation::Batch(vec![f::item(b"clock", 100, operation)?]),
        vec![f::input(0, 0)],
        vec![f::witness(b"r", None)?],
    )?;
    f::apply_value(&mut store, &scope, &prepared, 0)?;
    drop(prepared);
    let reference = render_original_value_ref(
        b"clock",
        OriginalValueRefKind::Payload,
        1,
        OriginalValuePartition::Explicit(b"p"),
        65_535,
    )
    .map_err(|error| format!("value reference fixture: {error:?}"))?;
    let mut clock = f::ValueReadClock {
        hlc: HlcTimestamp::new(100, 0)?,
        wall: TimestampMillis::new(100),
        calls: 0,
        failure: Some(StateTransactionError::Unavailable),
    };
    let empty = store.read_workflow_value_batch(&mut clock, &scope, &[], f::read_limits());
    assert!(empty.is_ok(), "{:?}", empty.as_ref().err());
    let empty = empty?;
    f::assert_read(&empty, &[], &[])?;
    let invalid = [
        b"".as_slice(),
        b"not-a-reference",
        b"f:{f}:state:clock",
        &oversized,
    ];
    let skipped = store.read_workflow_value_batch(&mut clock, &scope, &invalid, f::read_limits());
    assert!(skipped.is_ok(), "{:?}", skipped.as_ref().err());
    let skipped = skipped?;
    f::assert_read(&skipped, &[], &[None, None, None, None])?;
    assert_eq!(clock.calls, 0);
    clock.failure = None;
    let refs = [
        reference.as_slice(),
        b"f:{f}:v:p:missing:1",
        reference.as_slice(),
    ];
    let live = store.read_workflow_value_batch(&mut clock, &scope, &refs, f::read_limits());
    assert!(live.is_ok(), "{:?}", live.as_ref().err());
    let live = live?;
    f::assert_read(&live, &[b"clock"], &[Some(0), None, Some(0)])?;
    assert_eq!(clock.calls, 1);
    clock.failure = Some(StateTransactionError::Unavailable);
    let failure = store.read_workflow_value_batch(&mut clock, &scope, &refs, f::read_limits());
    assert!(matches!(failure, Err(StateTransactionError::Unavailable)));
    assert_eq!(clock.calls, 2);
    let skipped = store.read_workflow_value_batch(&mut clock, &scope, &invalid, f::read_limits());
    assert!(skipped.is_ok(), "{:?}", skipped.as_ref().err());
    let skipped = skipped?;
    f::assert_read(&skipped, &[], &[None, None, None, None])?;
    assert_eq!(clock.calls, 2);
    let mut limits = f::read_limits();
    limits.maximum_references = 2;
    assert!(matches!(
        store.read_workflow_value_batch(&mut clock, &scope, &refs, limits),
        Err(StateTransactionError::Limit)
    ));
    assert_eq!(clock.calls, 2);
    drop(store);
    drop(reference);
    assert_eq!(live.values.as_slice(), [b"clock".to_vec()]);
    std::fs::remove_dir_all(directory)?;
    Ok(())
}
