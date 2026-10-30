//! Real authoritative state: independent complete cohorts and durable boundaries.

#![forbid(unsafe_code)]

use std::collections::BTreeMap;
use std::path::Path;

use wf_domain::claim::Claim;
use wf_domain::lease::{ExtendLease, LeaseGuard, LeaseToken};
use wf_domain::policy::RetryOverride;
use wf_domain::retry::{JitterSample, PolicyRetry, Retry, RetryTime};
use wf_domain::workflow::admission::AdmissionLimits;
use wf_domain::workflow::batch::BatchRejection;
use wf_domain::workflow::caller::{MutationFieldValue, MutationInput, normalize_core_mutation};
use wf_domain::workflow::indexed::IndexRejection;
use wf_domain::workflow::request::{RetainedOutcome, RetainedStatus};
use wf_domain::workflow::{Cancel, Create, DomainError, Operation, Terminal, Transition};
use wf_ports::log::{PreparedBytes, encode_public_mutation};
use wf_storage::state::{RedbStateStore, StateStoreGenesis, StateStoreLimits, key, value};
use wf_storage::transaction::{
    RecordBatchTransactionOutcome, apply_independent_record_batch_transaction,
    apply_public_mutation_transaction, apply_record_batch_transaction,
};
use wf_types::identity::{RecordId, RequestId};
use wf_types::version::{Fence, TimestampMillis};

mod atomic_start;
mod bootstrap;
mod claim;
mod control;
mod core_policy;
mod credit;
mod cursor;
mod faults;
mod fixtures;
mod recovery;
mod snapshot;
mod workflow_values;

type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;

#[test]
fn accepted_rejected_replay_atomicity() -> TestResult {
    atomic_and_retained()?;
    independent_final_once()?;
    other_effects_and_transition()?;
    public_large_and_original_reply()?;
    Ok(())
}

fn apply_and_check(
    path: &Path,
    frame: &PreparedBytes,
    index: u64,
    profile: u8,
    expected: &BTreeMap<Vec<u8>, Vec<u8>>,
) -> TestResult<Box<RetainedOutcome>> {
    let manifest = fixtures::raw_manifest(path)?;
    let scope = fixtures::scope()?;
    let mut store = fixtures::open(path)?;
    let result = match profile {
        5 => apply_record_batch_transaction(
            &mut store,
            &scope,
            frame.as_bytes(),
            fixtures::log(index),
            fixtures::driver_limits(),
        )?,
        6 => apply_independent_record_batch_transaction(
            &mut store,
            &scope,
            frame.as_bytes(),
            fixtures::log(index),
            fixtures::driver_limits(),
        )?,
        1 => apply_public_mutation_transaction(
            &mut store,
            &scope,
            frame.as_bytes(),
            fixtures::log(index),
            fixtures::driver_limits(),
        )?,
        _ => return Err("unsupported test profile".into()),
    };
    let RecordBatchTransactionOutcome::Retained { control, outcome } = result else {
        return Err("expected retained reply".into());
    };
    let expected_control = value::decode_control(
        expected
            .get(&key::encode_control_key(
                scope.group(),
                scope.recovery(),
                fixtures::MIB,
            )?)
            .ok_or("expected control absent")?,
        scope.group(),
        scope.recovery(),
        fixtures::MIB,
    )?;
    assert_eq!(control, expected_control);
    drop(store);
    assert_eq!(fixtures::raw_rows(path)?, *expected);
    assert_eq!(fixtures::raw_manifest(path)?, manifest);
    drop(fixtures::open(path)?);
    Ok(outcome)
}

fn atomic_and_retained() -> TestResult {
    let root = fixtures::directory("atomic")?;
    let path = root.join("state.redb");
    drop(fixtures::create(&path)?);
    let mut expected = fixtures::raw_rows(&path)?;
    let frame = fixtures::frame(
        b"seed",
        100,
        vec![(b"i", fixtures::create_op()), (b"j", fixtures::create_op())],
    )?;
    fixtures::step(&mut expected, b"i", 100, fixtures::create_op(), 0, 1)?;
    fixtures::step(&mut expected, b"j", 100, fixtures::create_op(), 1, 1)?;
    expected = fixtures::finish(
        expected,
        frame.as_bytes(),
        fixtures::log(0),
        5,
        vec![0],
        RetainedStatus::Accepted,
    )?;
    let original = apply_and_check(&path, &frame, 0, 5, &expected)?;
    assert_eq!(original.response(), [0]);
    assert_eq!(original.original_position(), fixtures::log(0));
    assert_eq!(original.original_time(), TimestampMillis::new(100));
    let rejected = fixtures::frame(b"bad", 101, vec![(b"i", fixtures::create_op())])?;
    let status = RetainedStatus::IndexedRejected(IndexRejection::Batch(BatchRejection::Domain(
        DomainError::AlreadyExists,
    )));
    expected = fixtures::finish(
        expected,
        rejected.as_bytes(),
        fixtures::log(1),
        5,
        vec![1, 0, 1],
        status,
    )?;
    let outcome = apply_and_check(&path, &rejected, 1, 5, &expected)?;
    assert_eq!(outcome.response(), [1, 0, 1]);
    assert_eq!(outcome.status(), status);
    expected = fixtures::control_only(&expected, frame.as_bytes(), fixtures::log(2), 5)?;
    let replay = apply_and_check(&path, &frame, 2, 5, &expected)?;
    assert_eq!(replay.response(), original.response());
    assert_eq!(replay.original_position(), original.original_position());
    assert_eq!(replay.original_time(), original.original_time());
    assert_eq!(replay.expires_at(), original.expires_at());
    let mut store = fixtures::open(&path)?;
    assert!(matches!(
        apply_record_batch_transaction(
            &mut store,
            &fixtures::scope()?,
            frame.as_bytes(),
            fixtures::log(2),
            fixtures::driver_limits()
        )?,
        RecordBatchTransactionOutcome::LogReplay(_)
    ));
    drop(store);
    assert_eq!(fixtures::raw_rows(&path)?, expected);
    retained_refusals(&path, &frame, expected)?;
    std::fs::remove_dir_all(root)?;
    Ok(())
}

fn retained_refusals(
    path: &Path,
    original: &PreparedBytes,
    mut expected: BTreeMap<Vec<u8>, Vec<u8>>,
) -> TestResult {
    let different = fixtures::frame(b"seed", 102, vec![(b"other", fixtures::create_op())])?;
    expected = fixtures::control_only(&expected, different.as_bytes(), fixtures::log(3), 5)?;
    let mut store = fixtures::open(path)?;
    assert!(matches!(
        apply_record_batch_transaction(
            &mut store,
            &fixtures::scope()?,
            different.as_bytes(),
            fixtures::log(3),
            fixtures::driver_limits()
        )?,
        RecordBatchTransactionOutcome::IntentConflict(_)
    ));
    drop(store);
    assert_eq!(fixtures::raw_rows(path)?, expected);
    let advance = fixtures::frame(b"advance", 1_200, vec![(b"k", fixtures::create_op())])?;
    fixtures::step(
        &mut expected,
        b"k",
        1_200,
        fixtures::create_op(),
        4_u128 << 32,
        1,
    )?;
    expected = fixtures::finish(
        expected,
        advance.as_bytes(),
        fixtures::log(4),
        5,
        vec![0],
        RetainedStatus::Accepted,
    )?;
    apply_and_check(path, &advance, 4, 5, &expected)?;
    expected = fixtures::control_only(&expected, original.as_bytes(), fixtures::log(5), 5)?;
    let mut store = fixtures::open(path)?;
    assert!(matches!(
        apply_record_batch_transaction(
            &mut store,
            &fixtures::scope()?,
            original.as_bytes(),
            fixtures::log(5),
            fixtures::driver_limits()
        )?,
        RecordBatchTransactionOutcome::Expired(_)
    ));
    drop(store);
    assert_eq!(fixtures::raw_rows(path)?, expected);
    Ok(())
}

fn guard(token: &[u8]) -> TestResult<LeaseGuard> {
    Ok(LeaseGuard {
        token: LeaseToken::new(token.to_vec())?,
        fence: Fence::new(1),
    })
}

fn terminal(token: &[u8]) -> TestResult<Operation> {
    Ok(Operation::Complete(Terminal {
        guard: guard(token)?,
        payload: None,
        value: None,
    }))
}

fn retry() -> TestResult<Operation> {
    Ok(Operation::Retry(Retry {
        guard: guard(b"k")?,
        run_at: RetryTime::Explicit(TimestampMillis::new(3_000)),
        payload: None,
        error: None,
    }))
}

fn cancel_retried() -> Operation {
    Operation::Cancel(Cancel {
        fence: Fence::new(1),
        lease_token: None,
        expected_state: None,
        expected_version: Some(3),
        error: None,
    })
}

fn seed_claims(path: &Path, fifo: bool) -> TestResult<BTreeMap<Vec<u8>, Vec<u8>>> {
    if fifo {
        drop(fixtures::create(path)?);
    } else {
        let scope = fixtures::scope()?;
        let placement = fixtures::placements()?;
        drop(RedbStateStore::create(
            path,
            scope.group().clone(),
            scope.recovery().clone(),
            StateStoreGenesis {
                initial_time: fixtures::clock(0)?,
                placements: &placement,
                policies: &[],
            },
            StateStoreLimits::default(),
        )?);
    }
    let mut expected = fixtures::raw_rows(path)?;
    let frame = fixtures::frame(
        b"seed",
        100,
        vec![(b"i", fixtures::create_op()), (b"j", fixtures::create_op())],
    )?;
    fixtures::step(&mut expected, b"i", 100, fixtures::create_op(), 0, 1)?;
    fixtures::step(&mut expected, b"j", 100, fixtures::create_op(), 1, 1)?;
    expected = fixtures::finish(
        expected,
        frame.as_bytes(),
        fixtures::log(0),
        5,
        vec![0],
        RetainedStatus::Accepted,
    )?;
    apply_and_check(path, &frame, 0, 5, &expected)?;
    for (id, sequence) in [(b"i".as_slice(), 0), (b"j".as_slice(), 1)] {
        fixtures::step(
            &mut expected,
            id,
            1_000,
            Operation::Claim(Claim {
                expected_state: b"q".to_vec(),
                worker: b"worker".to_vec(),
                token: LeaseToken::new(b"k".to_vec())?,
                lease_ms: 30_000,
            }),
            sequence,
            2,
        )?;
    }
    fixtures::reindex(&mut expected)?;
    let manifest = fixtures::raw_manifest(path)?;
    fixtures::replace(path, &expected, &manifest)?;
    drop(fixtures::open(path)?);
    Ok(expected)
}

fn independent_final_once() -> TestResult {
    let root = fixtures::directory("independent")?;
    let path = root.join("state.redb");
    let mut expected = seed_claims(&path, true)?;
    let frame = fixtures::independent_frame(
        b"independent",
        2_000,
        vec![
            (b"i", retry()?),
            (b"j", terminal(b"wrong")?),
            (b"j", terminal(b"k")?),
            (b"i", cancel_retried()),
            (b"j", terminal(b"k")?),
        ],
    )?;
    fixtures::step(&mut expected, b"i", 2_000, retry()?, 0, 3)?;
    fixtures::step(&mut expected, b"j", 2_000, terminal(b"k")?, 1, 3)?;
    fixtures::step(&mut expected, b"i", 2_000, cancel_retried(), 0, 4)?;
    let response = vec![2, 0, 0, 0, 5, 0, 1, 0, 4, 0, 0, 0];
    expected = fixtures::finish(
        expected,
        frame.as_bytes(),
        fixtures::log(1),
        6,
        response.clone(),
        RetainedStatus::Accepted,
    )?;
    let original = apply_and_check(&path, &frame, 1, 6, &expected)?;
    assert_eq!(original.response(), response);
    assert_eq!(
        expected.keys().filter(|key| key.get(6) == Some(&1)).count(),
        2
    );
    assert_eq!(
        expected.keys().filter(|key| key.get(6) == Some(&5)).count(),
        7
    );
    assert_eq!(
        expected
            .keys()
            .filter(|key| matches!(key.get(6), Some(3 | 4)))
            .count(),
        0
    );
    expected = fixtures::control_only(&expected, frame.as_bytes(), fixtures::log(2), 6)?;
    let replay = apply_and_check(&path, &frame, 2, 6, &expected)?;
    assert_eq!(replay.response(), original.response());
    assert_eq!(replay.original_position(), fixtures::log(1));
    std::fs::remove_dir_all(root)?;
    Ok(())
}

fn other_effects_and_transition() -> TestResult {
    let root = fixtures::directory("effects")?;
    let path = root.join("state.redb");
    let mut expected = seed_claims(&path, false)?;
    let extend = Operation::ExtendLease(ExtendLease {
        guard: guard(b"k")?,
        lease_ms: 40_000,
    });
    let fail = Operation::Fail(Terminal {
        guard: guard(b"k")?,
        payload: None,
        value: Some(b"failed".to_vec()),
    });
    let retry = Operation::RetryWithPolicy(PolicyRetry {
        guard: guard(b"k")?,
        run_at: None,
        policy_override: RetryOverride::default(),
        jitter: JitterSample::new(0),
        payload: None,
        error: Some(b"retry".to_vec()),
    });
    let wrong = Operation::Fail(Terminal {
        guard: guard(b"wrong")?,
        payload: None,
        value: None,
    });
    let frame = fixtures::independent_frame(
        b"other",
        2_000,
        vec![
            (b"i", extend.clone()),
            (b"i", wrong),
            (b"i", fail.clone()),
            (b"j", retry.clone()),
        ],
    )?;
    fixtures::step(&mut expected, b"i", 2_000, extend, 0, 3)?;
    fixtures::step(&mut expected, b"i", 2_000, fail, 0, 4)?;
    fixtures::step(&mut expected, b"j", 2_000, retry, 1, 3)?;
    let response = vec![2, 0, 0, 0, 4, 0, 1, 0, 4, 0, 0];
    expected = fixtures::finish(
        expected,
        frame.as_bytes(),
        fixtures::log(1),
        6,
        response.clone(),
        RetainedStatus::Accepted,
    )?;
    assert_eq!(
        apply_and_check(&path, &frame, 1, 6, &expected)?.response(),
        response
    );
    let transition = Operation::Transition(Transition {
        from: b"q".to_vec(),
        to: b"next".to_vec(),
        fence: Fence::new(1),
        lease_token: None,
        run_at: Some(TimestampMillis::new(4_000)),
        payload: Some(vec![0, 255, 1]),
    });
    let frame = fixtures::frame(b"transition", 2_100, vec![(b"j", transition.clone())])?;
    fixtures::step(&mut expected, b"j", 2_100, transition, 2_u128 << 32, 4)?;
    expected = fixtures::finish(
        expected,
        frame.as_bytes(),
        fixtures::log(2),
        5,
        vec![0],
        RetainedStatus::Accepted,
    )?;
    assert_eq!(
        apply_and_check(&path, &frame, 2, 5, &expected)?.response(),
        [0]
    );
    std::fs::remove_dir_all(root)?;
    Ok(())
}

fn public_large_and_original_reply() -> TestResult {
    let root = fixtures::directory("public")?;
    let path = root.join("state.redb");
    drop(fixtures::create(&path)?);
    let scope = fixtures::scope()?;
    let request = RequestId::new(b"public".to_vec())?;
    let payload = vec![0x7f; fixtures::MIB];
    let fields = [
        ("type", MutationFieldValue::Bytes(b"r")),
        ("state", MutationFieldValue::Bytes(b"q")),
        ("payload", MutationFieldValue::Bytes(&payload)),
    ];
    let caller = normalize_core_mutation(
        MutationInput {
            operation: "create",
            id: b"k",
            fields: &fields,
        },
        &[],
        Some(b"p"),
        AdmissionLimits::default(),
        16 * fixtures::MIB,
    )?;
    let (frame, _) = encode_public_mutation(
        &scope,
        &request,
        &caller,
        fixtures::clock(2_000)?,
        fixtures::driver_limits().codec,
        fixtures::driver_limits().workspace_bytes,
    )?;
    let mut expected = fixtures::raw_rows(&path)?;
    fixtures::step(
        &mut expected,
        b"k",
        2_000,
        Operation::Create(Create {
            workflow_type: b"r".to_vec(),
            state: b"q".to_vec(),
            run_at: None,
            payload: Some(payload),
        }),
        0,
        1,
    )?;
    expected = fixtures::finish(
        expected,
        frame.as_bytes(),
        fixtures::log(0),
        1,
        vec![0],
        RetainedStatus::Accepted,
    )?;
    let original = apply_and_check(&path, &frame, 0, 1, &expected)?;
    assert_eq!(original.response(), [0]);
    assert_eq!(original.response_version(), 2);
    let cancel = fixtures::frame(b"cancel", 2_001, vec![(b"k", fixtures::cancel_op(1))])?;
    fixtures::step(&mut expected, b"k", 2_001, fixtures::cancel_op(1), 0, 2)?;
    expected = fixtures::finish(
        expected,
        cancel.as_bytes(),
        fixtures::log(1),
        5,
        vec![0],
        RetainedStatus::Accepted,
    )?;
    apply_and_check(&path, &cancel, 1, 5, &expected)?;
    expected = fixtures::control_only(&expected, frame.as_bytes(), fixtures::log(2), 1)?;
    let replay = apply_and_check(&path, &frame, 2, 1, &expected)?;
    assert_eq!(replay.response(), original.response());
    assert_eq!(replay.response_version(), 2);
    assert_eq!(replay.original_position(), fixtures::log(0));
    assert_eq!(replay.original_time(), TimestampMillis::new(2_000));
    let bytes = expected
        .get(&key::encode_record_key(
            &scope,
            &fixtures::placements()?[0].0,
            &RecordId::new(b"k".to_vec())?,
            fixtures::MIB,
        )?)
        .ok_or("public record absent")?;
    assert_eq!(
        value::decode_record(
            bytes,
            scope.group(),
            scope.recovery(),
            AdmissionLimits::default(),
            16 * fixtures::MIB
        )?
        .record
        .version(),
        2
    );
    std::fs::remove_dir_all(root)?;
    Ok(())
}

mod original_policy;
