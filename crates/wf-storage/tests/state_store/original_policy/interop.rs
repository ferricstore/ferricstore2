//! Same-type original/V2 interoperability through actual durable owners.
use super::{TestResult, fixtures as f};
use std::{collections::BTreeMap, path::Path};
use wf_domain::policy::complete::{PolicySchemaCharge, RawFlowPolicyValue as Raw};
use wf_domain::policy::definition::{
    FieldPatch, PolicyPatch, StateChange, StateMode, StatePolicyPatch,
};
use wf_domain::workflow::request::RetainedOutcome;
use wf_storage::transaction::{
    RecordBatchTransactionOutcome, apply_complete_core_policy_patch_transaction,
};
use wf_types::identity::RequestId;
mod authority;
mod faults;
mod snapshots;
mod wire;
use wire as w;

fn original_frame(
    request: &[u8],
    now: u64,
    attrs: Raw<'_>,
    controls: Raw<'_>,
) -> TestResult<Vec<u8>> {
    let caps = f::limits();
    f::public_frame(
        &f::scope()?,
        &RequestId::new(request.to_vec())?,
        now,
        b"job",
        attrs,
        controls,
        &mut PolicySchemaCharge::new(caps.schema),
    )
}
fn fifo_patch() -> PolicyPatch {
    PolicyPatch {
        states: BTreeMap::from([(
            b"queued".to_vec(),
            StateChange::Patch(StatePolicyPatch {
                mode: FieldPatch::Set(StateMode::Fifo),
                ..StatePolicyPatch::default()
            }),
        )]),
        ..PolicyPatch::default()
    }
}
fn retained(result: RecordBatchTransactionOutcome) -> TestResult<Box<RetainedOutcome>> {
    match result {
        RecordBatchTransactionOutcome::Retained { outcome, .. } => Ok(outcome),
        _ => Err("durable interoperability operation did not retain a response".into()),
    }
}
fn check_disk(path: &Path, expected: &f::Cohort) -> TestResult {
    assert_eq!(f::cohort(path)?, *expected);
    drop(f::open(path, 64)?);
    Ok(())
}

#[test]
fn same_type_v2_get_set_core_patch_round_trip_preserves_complete_authority() -> TestResult {
    let dir = f::directory("interop-v2-original-roundtrip")?;
    let path = dir.join("state");
    let value = w::rich_value()?;
    let v2 = value.encoded().to_vec();
    let mut store = f::create(&path, &[(b"job".to_vec(), v2.clone())], 64)?;
    drop(store);
    let mut expected = f::cohort(&path)?;
    assert_eq!(
        f::rows(&expected)?.get(&f::key(6, &[b"g", b"z", b"t", b"job"])),
        Some(&v2)
    );
    store = f::open(&path, 64)?;
    w::get(&mut store, 1, b"parallel", (Some(1000), 7, 7), false)?;
    drop(store);
    check_disk(&path, &expected)?;
    let retry = [(Raw::Atom("max_retries"), Raw::Integer(9))];
    let frame = original_frame(
        b"merge",
        10,
        Raw::Map(&[(Raw::Atom("retry"), Raw::Map(&retry))]),
        Raw::List(&[]),
    )?;
    store = f::open(&path, 64)?;
    let caps = f::limits();
    let mut charge = PolicySchemaCharge::new(caps.schema);
    let receipt = retained(f::apply(&mut store, &frame, 0, caps, &mut charge)?)?;
    let raw = w::rich_wire(2, 9, b"parallel")?;
    assert_eq!(
        *receipt,
        w::accepted(
            &mut expected,
            &frame,
            b"merge",
            (0, 10),
            f::frame_intent(&frame, 0)?,
            (1, &raw)
        )?
    );
    w::get(&mut store, 2, b"parallel", (Some(1000), 7, 9), false)?;
    drop(store);
    check_disk(&path, &expected)?;
    let frame = w::core_frame(b"core", 11, Some(2), fifo_patch())?;
    store = f::open(&path, 64)?;
    let receipt = retained(apply_complete_core_policy_patch_transaction(
        &mut store,
        &f::scope()?,
        &frame,
        f::log(1),
        caps,
    )?)?;
    let raw = w::rich_wire(3, 9, b"fifo")?;
    assert_eq!(
        *receipt,
        w::accepted(
            &mut expected,
            &frame,
            b"core",
            (1, 11),
            w::core_intent(&frame, 1)?,
            (2, &raw)
        )?
    );
    w::get(&mut store, 3, b"fifo", (Some(1000), 7, 9), false)?;
    drop(store);
    check_disk(&path, &expected)?;
    replace(&path, &mut expected)?;
    w::expanded(&dir)?;
    std::fs::remove_dir_all(dir)?;
    Ok(())
}
fn replace(path: &Path, expected: &mut f::Cohort) -> TestResult {
    let mode = [(Raw::Atom("mode"), Raw::Atom("parallel"))];
    let states = [(Raw::Binary(b"queued"), Raw::Map(&mode))];
    let attrs = [
        (Raw::Atom("version"), Raw::Integer(42)),
        (Raw::Atom("states"), Raw::Map(&states)),
    ];
    let replace = [Raw::Atom("replace"), Raw::Atom("true")];
    let expected_gen = [Raw::Atom("expected_generation"), Raw::Integer(3)];
    let frame = original_frame(
        b"replace",
        12,
        Raw::Map(&attrs),
        Raw::List(&[Raw::Tuple(&replace), Raw::Tuple(&expected_gen)]),
    )?;
    let caps = f::limits();
    let mut store = f::open(path, 64)?;
    let receipt = retained(f::apply(
        &mut store,
        &frame,
        2,
        caps,
        &mut PolicySchemaCharge::new(caps.schema),
    )?)?;
    let raw = w::simple_wire(4, b"parallel", &w::integer(42))?;
    assert_eq!(
        *receipt,
        w::accepted(
            expected,
            &frame,
            b"replace",
            (2, 12),
            f::frame_intent(&frame, 2)?,
            (1, &raw)
        )?
    );
    w::get(&mut store, 4, b"parallel", (None, 42, 3), false)?;
    drop(store);
    check_disk(path, expected)
}

#[test]
fn present_zero_and_opaque_terms_survive_core_patch_and_strict_reopen() -> TestResult {
    let dir = f::directory("interop-present-zero")?;
    let path = dir.join("state");
    let scope = f::scope()?;
    let caps = f::limits();
    let mut store = f::create(&path, &[], 64)?;
    // A restored layout6 cohort requires a real applied control predecessor.
    let seed = f::public_frame(
        &scope,
        &RequestId::new(b"other-type".to_vec())?,
        10,
        b"other",
        Raw::Map(&[]),
        Raw::List(&[]),
        &mut PolicySchemaCharge::new(caps.schema),
    )?;
    let receipt = retained(f::apply(
        &mut store,
        &seed,
        0,
        caps,
        &mut PolicySchemaCharge::new(caps.schema),
    )?)?;
    assert_eq!(
        receipt.response(),
        f::etf_policy(1, b"other", &[(b"states", &f::etf_map(&[])?)])?
    );
    drop(store);
    let mut expected = f::cohort(&path)?;
    let old = w::opaque_wire(0, b"parallel")?;
    f::rows_mut(&mut expected)?.insert(f::key(6, &[b"g", b"z", b"t", b"job"]), old.clone());
    f::upgrade6(
        &mut expected,
        &scope,
        f::catalog(&scope, [None, None], None, None, None)?,
    )?;
    f::replace(&path, &expected)?;
    let mut store = f::open(&path, 64)?;
    w::get(&mut store, 0, b"parallel", (None, 0, 3), true)?;
    drop(store);
    check_disk(&path, &expected)?;
    let frame = w::core_frame(b"zero-core", 20, Some(0), fifo_patch())?;
    store = f::open(&path, 64)?;
    let receipt = retained(apply_complete_core_policy_patch_transaction(
        &mut store,
        &scope,
        &frame,
        f::log(1),
        caps,
    )?)?;
    let raw = w::opaque_wire(1, b"fifo")?;
    assert_eq!(
        *receipt,
        w::accepted(
            &mut expected,
            &frame,
            b"zero-core",
            (1, 20),
            w::core_intent(&frame, 1)?,
            (2, &raw)
        )?
    );
    w::get(&mut store, 1, b"fifo", (None, 0, 3), true)?;
    drop(store);
    check_disk(&path, &expected)?;
    let frame = original_frame(b"normalize", 21, Raw::Map(&[]), Raw::List(&[]))?;
    store = f::open(&path, 64)?;
    let receipt = retained(f::apply(
        &mut store,
        &frame,
        2,
        caps,
        &mut PolicySchemaCharge::new(caps.schema),
    )?)?;
    let mut big = vec![110, 17, 0, 1];
    big.extend_from_slice(&[0; 15]);
    big.push(1);
    let normalized = w::simple_wire(2, b"fifo", &big)?;
    assert_eq!(
        *receipt,
        w::accepted(
            &mut expected,
            &frame,
            b"normalize",
            (2, 21),
            f::frame_intent(&frame, 2)?,
            (1, &normalized)
        )?
    );
    assert_ne!(normalized, w::opaque_wire(2, b"fifo")?);
    w::get(&mut store, 2, b"fifo", (None, 0, 3), true)?;
    drop(store);
    check_disk(&path, &expected)?;
    std::fs::remove_dir_all(dir)?;
    Ok(())
}

#[test]
fn cross_format_policy_modes_preserve_original_fifo_chain_and_foreign_rows() -> TestResult {
    let dir = f::directory("interop-real-fifo")?;
    for priority in [0, 1, 2] {
        fifo_chain(&dir.join(format!("state-{priority}")), priority)?;
    }
    std::fs::remove_dir_all(dir)?;
    Ok(())
}
fn running(
    id: &[u8],
    worker: &[u8],
    created: u64,
    now: u64,
    sequence: u128,
    priority: u8,
) -> TestResult<wf_ports::state::StoredWorkflowRecord> {
    use wf_domain::history::EventKind;
    use wf_domain::lease::{Lease, LeaseToken};
    use wf_types::version::{Fence, TimestampMillis};
    let mut data = f::data(id, b"job", b"queued", created, None)?;
    data.state = b"running".to_vec();
    data.version = 2;
    data.fence = Fence::new(1);
    data.updated_at = TimestampMillis::new(now);
    data.next_run_at = Some(TimestampMillis::new(now + 30_000));
    data.run_state = Some(b"queued".to_vec());
    data.payload = Some(id.to_vec());
    data.lease = Some(Lease {
        worker: worker.to_vec(),
        token: LeaseToken::new(format!("{}:{now}:1", std::str::from_utf8(worker)?).into_bytes())?,
        deadline: TimestampMillis::new(now + 30_000),
    });
    data.latest_kind = EventKind::Claimed;
    data.latest_version = 2;
    data.latest_fence = Fence::new(1);
    f::owner(&f::scope()?, b"p", data, priority, sequence)
}
fn fifo_chain(path: &Path, priority: u8) -> TestResult {
    use wf_domain::workflow::request::{ClientIntent, RetainedStatus};
    let scope = f::scope()?;
    let job = w::parallel_value(b"job")?;
    let other = w::parallel_value(b"other")?;
    let other_wire = other.encoded().to_vec();
    let mut store = f::create(
        path,
        &[
            (b"job".to_vec(), job.encoded().to_vec()),
            (b"other".to_vec(), other_wire.clone()),
        ],
        64,
    )?;
    let mut first_data = f::data(b"first", b"job", b"queued", 1000, Some(5000))?;
    first_data.payload = Some(b"first".to_vec());
    let first = f::owner(&scope, b"p", first_data, priority, 0)?;
    let foreign = f::owner(
        &scope,
        b"p",
        f::data(b"foreign", b"other", b"q", 1000, Some(9000))?,
        0,
        1,
    )?;
    f::seed(
        &mut store,
        &scope,
        &[first.clone(), foreign.clone()],
        0,
        1000,
    )?;
    let mut second_data = f::data(b"second", b"job", b"queued", 1001, Some(2000))?;
    second_data.payload = Some(b"second".to_vec());
    let second = f::owner(&scope, b"p", second_data, priority, 1_u128 << 32)?;
    f::seed(&mut store, &scope, std::slice::from_ref(&second), 1, 1001)?;
    drop(store);
    let mut expected = f::cohort(path)?;
    for row in [&first, &second, &foreign] {
        assert_eq!(
            f::rows(&expected)?.get(&f::point(&scope, row)),
            Some(&f::record_wire(&scope, row)?)
        );
        let (key, value) = f::history_wire(&scope, row, &f::history(row))?;
        assert_eq!(f::rows(&expected)?.get(&key), Some(&value));
    }
    let leased = running(
        b"second",
        b"parallel-worker",
        1001,
        2000,
        1_u128 << 32,
        priority,
    )?;
    let bytes = f::claim_frame(
        &scope,
        b"parallel",
        2000,
        b"job",
        b"parallel-worker",
        Some(&[b"queued"]),
        Some(&[b"p"]),
    )?;
    let response = f::receipt(std::slice::from_ref(&leased), 0, 1, 0)?;
    f::put_record(&mut expected, &scope, &leased)?;
    assert_eq!(
        f::rows_mut(&mut expected)?.remove(&f::due(&scope, &second)?),
        Some(vec![0])
    );
    f::rows_mut(&mut expected)?.insert(f::due(&scope, &leased)?, vec![0]);
    let metadata = f::metadata(
        &scope,
        b"parallel",
        2,
        2000,
        ClientIntent::new(1, [61; 32])?,
        RetainedStatus::Accepted,
    )?;
    f::finish(
        &mut expected,
        &scope,
        &bytes,
        f::log(2),
        (f::clock(2000)?, f::clock(2000)?),
        Some((&metadata, 3, &response)),
    )?;
    store = f::open(path, 64)?;
    f::claim(&mut store, &scope, &bytes, 2, &response)?;
    drop(store);
    check_disk(path, &expected)?;
    activate_original(path, &mut expected, &first, &leased)?;
    super::activation::blocked_claim(path, &scope, &mut expected, priority)?;
    super::activation::complete_running(path, &scope, &mut expected, priority)?;
    super::activation::claim_first(path, &scope, &mut expected, priority)?;
    restore_parallel(path, &scope, &mut expected, priority, &other_wire)
}
fn restore_parallel(
    path: &Path,
    scope: &wf_domain::workflow::request::RequestScope,
    expected: &mut f::Cohort,
    priority: u8,
    other_wire: &[u8],
) -> TestResult {
    let previous = expected.clone();
    let active = running(b"first", b"fifo-worker", 1000, 5101, 0, priority)?;
    let mut changes = fifo_patch();
    if let Some(StateChange::Patch(state)) = changes.states.get_mut(b"queued".as_slice()) {
        state.mode = FieldPatch::Set(StateMode::Parallel);
    }
    let bytes = w::core_frame(b"back-to-parallel", 5102, Some(2), changes)?;
    let mut store = f::open(path, 64)?;
    let outcome = retained(apply_complete_core_policy_patch_transaction(
        &mut store,
        scope,
        &bytes,
        f::log(7),
        f::limits(),
    )?)?;
    assert_eq!(
        f::rows_mut(expected)?.remove(&f::member(scope, &active)),
        Some(Vec::new())
    );
    assert_eq!(
        f::rows_mut(expected)?.remove(&f::due(scope, &active)?),
        Some(vec![1])
    );
    f::rows_mut(expected)?.insert(f::due(scope, &active)?, vec![0]);
    let state = w::fields(&[(b"mode", f::atom(b"parallel")?)])?;
    let response = f::etf_policy(
        3,
        b"job",
        &[(b"states", &f::etf_map(&[(f::binary(b"queued")?, state)])?)],
    )?;
    assert_eq!(
        *outcome,
        w::accepted(
            expected,
            &bytes,
            b"back-to-parallel",
            (7, 5102),
            w::core_intent(&bytes, 7)?,
            (2, &response)
        )?
    );
    drop(store);
    check_disk(path, expected)?;
    for (key, value) in f::rows(&previous)?
        .iter()
        .filter(|(key, _)| matches!(key.get(6), Some(1 | 5)))
    {
        assert_eq!(f::rows(expected)?.get(key), Some(value));
    }
    assert_eq!(
        f::rows(expected)?
            .get(&f::key(6, &[b"g", b"z", b"t", b"other"]))
            .map(Vec::as_slice),
        Some(other_wire)
    );
    Ok(())
}
fn activate_original(
    path: &Path,
    expected: &mut f::Cohort,
    first: &wf_ports::state::StoredWorkflowRecord,
    leased: &wf_ports::state::StoredWorkflowRecord,
) -> TestResult {
    let scope = f::scope()?;
    let mode = [(Raw::Atom("mode"), Raw::Atom("fifo"))];
    let states = [(Raw::Binary(b"queued"), Raw::Map(&mode))];
    let bytes = original_frame(
        b"activate",
        2001,
        Raw::Map(&[(Raw::Atom("states"), Raw::Map(&states))]),
        Raw::List(&[]),
    )?;
    let state = w::fields(&[(b"mode", f::atom(b"fifo")?)])?;
    let response = f::etf_policy(
        2,
        b"job",
        &[(b"states", &f::etf_map(&[(f::binary(b"queued")?, state)])?)],
    )?;
    for row in [first, leased] {
        assert_eq!(
            f::rows_mut(expected)?.remove(&f::due(&scope, row)?),
            Some(vec![0])
        );
        f::rows_mut(expected)?.insert(f::member(&scope, row), Vec::new());
    }
    f::rows_mut(expected)?.insert(f::due(&scope, leased)?, vec![1]);
    let caps = f::limits();
    let mut store = f::open(path, 64)?;
    let outcome = retained(f::apply(
        &mut store,
        &bytes,
        3,
        caps,
        &mut PolicySchemaCharge::new(caps.schema),
    )?)?;
    assert_eq!(
        *outcome,
        w::accepted(
            expected,
            &bytes,
            b"activate",
            (3, 2001),
            f::frame_intent(&bytes, 3)?,
            (1, &response)
        )?
    );
    drop(store);
    check_disk(path, expected)
}
