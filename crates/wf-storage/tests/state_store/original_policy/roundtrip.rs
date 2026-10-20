//! Real original allocator SET/GET, complete bytes and operational read origins.
use super::{TestResult, fixtures as f};
use std::path::Path;
use wf_domain::policy::complete::{
    CompletePolicyEncodingError as EncodingError, OriginalPolicyGetRejection, PolicySchemaCharge,
    RawFlowPolicyValue as Raw,
};
use wf_domain::workflow::request::{RetainedOutcome, RetainedStatus};
use wf_ports::state::StateTransactionError;
use wf_storage::state::RedbStateStore;
use wf_storage::transaction::{
    OriginalPolicyTransactionError, RecordBatchTransactionOutcome, with_original_policy_get_reply,
};
use wf_types::identity::RequestId;

fn frame(request: &[u8], now: u64, attrs: Raw<'_>, controls: Raw<'_>) -> TestResult<Vec<u8>> {
    let profile = f::limits();
    f::public_frame(
        &f::scope()?,
        &RequestId::new(request.to_vec())?,
        now,
        b"job",
        attrs,
        controls,
        &mut PolicySchemaCharge::new(profile.schema),
    )
}

fn publication(
    path: &Path,
    request: &[u8],
    now: u64,
    at: u64,
    bytes: &[u8],
    expected: &mut f::Cohort,
    response: Option<&[u8]>,
) -> TestResult<Box<RetainedOutcome>> {
    let s = f::scope()?;
    let status = if response.is_some() {
        RetainedStatus::Accepted
    } else {
        RetainedStatus::PolicyRejected(wf_domain::workflow::request::PolicyRejection::new(
            wf_domain::policy::installation::PolicyError::StaleGeneration,
        )?)
    };
    let body = response.unwrap_or(b"ERR stale flow policy generation");
    let metadata = f::metadata(&s, request, at, now, f::frame_intent(bytes, at)?, status)?;
    if let Some(raw) = response {
        f::rows_mut(expected)?.insert(f::key(6, &[b"g", b"z", b"t", b"job"]), raw.to_vec());
    }
    f::finish(
        expected,
        &s,
        bytes,
        f::log(at),
        (f::clock(now)?, f::clock(now)?),
        Some((&metadata, 1, body)),
    )?;
    if response.is_some() {
        let descriptor = f::catalog(&s, [None, None], None, None, None)?;
        f::upgrade6(expected, &s, descriptor)?;
    }
    let mut store = f::open(path, 64)?;
    let profile = f::limits();
    let mut charge = PolicySchemaCharge::new(profile.schema);
    let RecordBatchTransactionOutcome::Retained { control, outcome } =
        f::apply(&mut store, bytes, at, profile, &mut charge)?
    else {
        return Err("missing durable SET receipt".into());
    };
    assert_eq!(control.position().get(), at);
    let wanted = RetainedOutcome::new(metadata, 1, body.to_vec(), profile.codec.admission())?;
    assert_eq!(*outcome, wanted);
    assert_set_reply(request, &outcome, &mut charge)?;
    drop(store);
    assert_eq!(f::cohort(path)?, *expected);
    drop(f::open(path, 64)?);
    Ok(outcome)
}

fn assert_set_reply(
    request: &[u8],
    outcome: &RetainedOutcome,
    charge: &mut PolicySchemaCharge,
) -> TestResult {
    use wf_domain::policy::complete::{
        OriginalPolicyReplyContext, canonical_original_etf_safety,
        present_original_policy_set_reply,
    };
    let (generation, maximum, retry) = match request {
        b"set-1" => (1, Some(1000), 7),
        b"replace" => (3, None, 9),
        _ => return Ok(()),
    };
    let context = OriginalPolicyReplyContext::new(
        canonical_original_etf_safety(),
        f::store_limits(64)?.original_policy_reply_defaults(),
        Some(outcome.response()),
        None,
        0,
    );
    let before = charge.g();
    let reply = present_original_policy_set_reply(
        b"job",
        outcome.response(),
        None,
        context,
        charge.profile(),
        charge,
    )?
    .map_err(|reason| format!("durable SET projection rejected: {reason:?}"))?;
    assert_eq!(reply.generation(), generation);
    assert!(charge.g() > before);
    let root = reply.root();
    assert_eq!(root.map_len(), Some(10));
    assert_eq!(
        root.atom_field("type", charge)?
            .and_then(|v| v.binary_bytes()),
        Some(b"job".as_slice())
    );
    assert_eq!(
        root.atom_field("max_active_ms", charge)?
            .and_then(|v| v.integer()),
        maximum
    );
    let states = root
        .atom_field("states", charge)?
        .ok_or(EncodingError::Malformed)?;
    assert_eq!(states.map_len(), Some(1));
    let mut iter = states.map_iter().ok_or(EncodingError::Malformed)?;
    let (name, state) = iter.next(charge)?.ok_or(EncodingError::Malformed)?;
    assert_eq!(name.binary_bytes(), Some(b"queued".as_slice()));
    assert_eq!(
        state
            .atom_field("mode", charge)?
            .and_then(|v| v.atom_bytes()),
        Some(b"parallel".as_slice())
    );
    let policy = state
        .atom_field("retry", charge)?
        .ok_or(EncodingError::Malformed)?;
    assert_eq!(
        policy
            .atom_field("max_retries", charge)?
            .and_then(|v| v.integer()),
        Some(retry)
    );
    let retention = state
        .atom_field("retention", charge)?
        .ok_or(EncodingError::Malformed)?;
    assert_eq!(
        retention
            .atom_field("history_max_events", charge)?
            .and_then(|v| v.integer()),
        Some(64)
    );
    assert!(iter.next(charge)?.is_none());
    Ok(())
}

fn get(
    store: &mut RedbStateStore,
    generation: u64,
    maximum: Option<i128>,
    states: &[(&[u8], &[u8])],
) -> TestResult {
    let profile = f::limits();
    let mut charge = PolicySchemaCharge::new(profile.schema);
    let mut visited = 0;
    let account = std::ptr::from_ref(&charge);
    with_original_policy_get_reply(
        store,
        &f::scope()?,
        &Raw::Binary(b"job"),
        &Raw::List(&[]),
        profile,
        &mut charge,
        |reply, charge| {
            assert!(std::ptr::eq(&*charge, account));
            visited += 1;
            let reply = reply.expect("real GET reply");
            assert_eq!(reply.generation(), generation);
            let root = reply.root();
            assert_eq!(root.map_len(), Some(10));
            assert_eq!(
                root.atom_field("type", charge)?
                    .and_then(|v| v.binary_bytes()),
                Some(b"job".as_slice())
            );
            assert_eq!(
                root.atom_field("max_active_ms", charge)?
                    .and_then(|v| v.integer()),
                maximum
            );
            let returned = root
                .atom_field("states", charge)?
                .ok_or(EncodingError::Malformed)?;
            assert_eq!(returned.map_len(), Some(states.len()));
            let mut iter = returned.map_iter().ok_or(EncodingError::Malformed)?;
            for (name, mode) in states {
                let (key, value) = iter.next(charge)?.ok_or(EncodingError::Malformed)?;
                assert_eq!(key.binary_bytes(), Some(*name));
                assert_eq!(
                    value
                        .atom_field("mode", charge)?
                        .and_then(|v| v.atom_bytes()),
                    Some(*mode)
                );
            }
            assert!(iter.next(charge)?.is_none());
            Ok(())
        },
    )?;
    assert_eq!(visited, 1);
    Ok(())
}

fn queued_retry(max: u8, target: Option<&[u8]>) -> TestResult<Vec<u8>> {
    let mut retry = vec![(f::atom(b"max_retries")?, vec![97, max])];
    if let Some(target) = target {
        retry.push((f::atom(b"exhausted_to")?, f::binary(target)?));
    }
    retry.sort_by(|a, b| a.0[2..].cmp(&b.0[2..]));
    f::etf_map(&[(f::atom(b"retry")?, f::etf_map(&retry)?)])
}

#[test]
fn original_allocator_merge_replace_stale_and_real_get() -> TestResult {
    let dir = f::directory("original-roundtrip")?;
    let path = dir.join("state");
    drop(f::create(&path, &[], 64)?);
    let mut expected = f::cohort(&path)?;
    let max = [(Raw::Atom("max_retries"), Raw::Integer(7))];
    let retry = [(Raw::Atom("retry"), Raw::Map(&max))];
    let states = [(Raw::Binary(b"queued"), Raw::Map(&retry))];
    let attrs = [
        (Raw::Atom("max_active_ms"), Raw::Integer(1000)),
        (Raw::Atom("states"), Raw::Map(&states)),
    ];
    let first = frame(b"set-1", 10, Raw::Map(&attrs), Raw::List(&[]))?;
    let first_states = f::etf_map(&[(f::binary(b"queued")?, queued_retry(7, None)?)])?;
    let maximum = [98, 0, 0, 3, 232];
    let raw1 = f::etf_policy(
        1,
        b"job",
        &[(b"max_active_ms", &maximum), (b"states", &first_states)],
    )?;
    let receipt = publication(&path, b"set-1", 10, 0, &first, &mut expected, Some(&raw1))?;
    assert_eq!(receipt.response(), raw1);
    drop(receipt);
    let mut store = f::open(&path, 64)?;
    get(&mut store, 1, Some(1000), &[(b"queued", b"parallel")])?;
    drop(store);
    assert_second_and_stale(&path, &mut expected)?;
    assert_replace_and_alias(&path, &mut expected)?;
    Ok(())
}

fn assert_second_and_stale(path: &Path, expected: &mut f::Cohort) -> TestResult {
    let target = [(Raw::Atom("exhausted_to"), Raw::Binary(b"dead"))];
    let retry = [(Raw::Atom("retry"), Raw::Map(&target))];
    let fifo = [(Raw::Atom("mode"), Raw::Atom("fifo"))];
    let states = [
        (Raw::Binary(b"queued"), Raw::Map(&retry)),
        (Raw::Binary(b"review"), Raw::Map(&fifo)),
    ];
    let attrs = [(Raw::Atom("states"), Raw::Map(&states))];
    let second = frame(b"set-2", 20, Raw::Map(&attrs), Raw::List(&[]))?;
    let review = f::etf_map(&[(f::atom(b"mode")?, f::atom(b"fifo")?)])?;
    let states = f::etf_map(&[
        (f::binary(b"queued")?, queued_retry(7, Some(b"dead"))?),
        (f::binary(b"review")?, review),
    ])?;
    let maximum = [98, 0, 0, 3, 232];
    let raw2 = f::etf_policy(
        2,
        b"job",
        &[(b"max_active_ms", &maximum), (b"states", &states)],
    )?;
    publication(path, b"set-2", 20, 1, &second, expected, Some(&raw2))?;
    let mut store = f::open(path, 64)?;
    get(
        &mut store,
        2,
        Some(1000),
        &[(b"queued", b"parallel"), (b"review", b"fifo")],
    )?;
    drop(store);
    let expected_generation = [Raw::Atom("expected_generation"), Raw::Integer(1)];
    let opts = [Raw::Tuple(&expected_generation)];
    let stale = frame(b"stale", 30, Raw::Map(&[]), Raw::List(&opts))?;
    publication(path, b"stale", 30, 2, &stale, expected, None)?;
    assert_eq!(
        f::rows(expected)?.get(&f::key(6, &[b"g", b"z", b"t", b"job"])),
        Some(&raw2)
    );
    Ok(())
}

fn assert_replace_and_alias(path: &Path, expected: &mut f::Cohort) -> TestResult {
    let mode = [
        (Raw::Binary(b"mode"), Raw::Atom("parallel")),
        (
            Raw::Atom("retry"),
            Raw::Map(&[(Raw::Atom("max_retries"), Raw::Integer(9))]),
        ),
    ];
    let states = [(Raw::Binary(b"queued"), Raw::Map(&mode))];
    let attrs = [(Raw::Atom("states"), Raw::Map(&states))];
    let replace = [Raw::Atom("replace"), Raw::Atom("true")];
    let opts = [Raw::Tuple(&replace)];
    let bytes = frame(b"replace", 40, Raw::Map(&attrs), Raw::List(&opts))?;
    let config = f::etf_map(&[
        (f::atom(b"mode")?, f::atom(b"parallel")?),
        (
            f::atom(b"retry")?,
            f::etf_map(&[(f::atom(b"max_retries")?, vec![97, 9])])?,
        ),
    ])?;
    let states = f::etf_map(&[(f::binary(b"queued")?, config)])?;
    let raw3 = f::etf_policy(3, b"job", &[(b"states", &states)])?;
    publication(path, b"replace", 40, 3, &bytes, expected, Some(&raw3))?;
    let mut store = f::open(path, 64)?;
    get(&mut store, 3, None, &[(b"queued", b"parallel")])?;
    drop(store);
    Ok(())
}

#[test]
fn present_gen0_big_integer_and_get_error_order() -> TestResult {
    let dir = f::directory("original-gen0-get")?;
    let path = dir.join("state");
    drop(f::create(&path, &[], 64)?);
    let mut expected = f::cohort(&path)?;
    let bytes = frame(b"allocate", 1, Raw::Map(&[]), Raw::List(&[]))?;
    let states = f::etf_map(&[])?;
    let raw = f::etf_policy(1, b"job", &[(b"states", &states)])?;
    publication(&path, b"allocate", 1, 0, &bytes, &mut expected, Some(&raw))?;
    let mut big = vec![110, 17, 0, 1];
    big.extend_from_slice(&[0; 15]);
    big.push(1);
    let zero = f::etf_policy(0, b"job", &[(b"states", &states), (b"version", &big)])?;
    f::rows_mut(&mut expected)?.insert(f::key(6, &[b"g", b"z", b"t", b"job"]), zero);
    f::upgrade6(
        &mut expected,
        &f::scope()?,
        f::catalog(&f::scope()?, [None, None], None, None, None)?,
    )?;
    // Explicit restored component: policy wire+census together, strict reopen.
    f::replace(&path, &expected)?;
    let mut store = f::open(&path, 64)?;
    get(&mut store, 0, None, &[])?;
    let profile = f::limits();
    let mut charge = PolicySchemaCharge::new(profile.schema);
    with_original_policy_get_reply(
        &mut store,
        &f::scope()?,
        &Raw::Binary(b"job"),
        &Raw::List(&[]),
        profile,
        &mut charge,
        |reply, charge| {
            let reply = reply.expect("present zero");
            let value = reply
                .root()
                .atom_field("version", charge)?
                .ok_or(EncodingError::Malformed)?;
            let (negative, magnitude) = value.big_integer().ok_or(EncodingError::Malformed)?;
            assert!(!negative);
            assert_eq!(magnitude, &big[3..]);
            Ok(())
        },
    )?;
    for adapter in [false, true] {
        let mut tight = profile;
        if adapter {
            tight.maximum_adapter_bytes = 1;
        } else {
            tight.maximum_read_bytes = 1;
        }
        assert!(matches!(
            with_original_policy_get_reply(
                &mut store,
                &f::scope()?,
                &Raw::Binary(b"job"),
                &Raw::List(&[]),
                tight,
                &mut PolicySchemaCharge::new(profile.schema),
                |_, _| panic!("refused read must not visit")
            ),
            Err(OriginalPolicyTransactionError::Transaction(
                StateTransactionError::Limit
            ))
        ));
        get(&mut store, 0, None, &[])?;
    }
    drop(store);
    assert_zero_read_invalid_opts(&path)?;
    assert_eq!(f::cohort(&path)?, expected);
    super::limits::assert_original_hard_length_corrupt(&path, &expected)?;
    drop(f::open(&path, 64)?);
    Ok(())
}

fn assert_zero_read_invalid_opts(path: &Path) -> TestResult {
    use super::super::faults::fault_database_counted;
    use std::sync::{
        Arc,
        atomic::{AtomicU8, AtomicUsize, Ordering},
    };
    let count = Arc::new(AtomicUsize::new(0));
    let remaining = Arc::new(AtomicUsize::new(usize::MAX));
    let db = fault_database_counted(
        path,
        Arc::new(AtomicU8::new(0)),
        0,
        Some((Arc::clone(&count), Arc::clone(&remaining))),
        None,
    )?;
    let s = f::scope()?;
    let mut store = RedbStateStore::open_database(
        db,
        s.group().clone(),
        s.recovery().clone(),
        f::store_limits(64)?,
    )?;
    count.store(0, Ordering::SeqCst);
    remaining.store(0, Ordering::SeqCst);
    let profile = f::limits();
    let mut visited = false;
    with_original_policy_get_reply(
        &mut store,
        &s,
        &Raw::Binary(b"job"),
        &Raw::Integer(1),
        profile,
        &mut PolicySchemaCharge::new(profile.schema),
        |reply, _| {
            visited = true;
            assert!(matches!(reply, Err(OriginalPolicyGetRejection::Options)));
            Ok(())
        },
    )?;
    let observed = count.load(Ordering::SeqCst);
    remaining.store(usize::MAX, Ordering::SeqCst);
    assert!(visited);
    assert_eq!(observed, 0);
    drop(store);
    Ok(())
}

#[test]
fn unchanged_effects_preserve_full_policy_and_changed_effects_refuse_atomically() -> TestResult {
    let dir = f::directory("original-effects")?;
    let path = dir.join("state");
    drop(f::create(&path, &[], 64)?);
    let mut expected = f::cohort(&path)?;
    let bytes = frame(b"empty", 1, Raw::Map(&[]), Raw::List(&[]))?;
    let states = f::etf_map(&[])?;
    let raw = f::etf_policy(1, b"job", &[(b"states", &states)])?;
    publication(&path, b"empty", 1, 0, &bytes, &mut expected, Some(&raw))?;
    let attrs = [Raw::Binary(b"customer")];
    let governance = [(Raw::Atom("effects"), Raw::Map(&[]))];
    let patch = [
        (Raw::Atom("indexed_attributes"), Raw::List(&attrs)),
        (Raw::Atom("governance"), Raw::Map(&governance)),
    ];
    let changed = frame(b"index-change", 2, Raw::Map(&patch), Raw::List(&[]))?;
    let mut store = f::open(&path, 64)?;
    let profile = f::limits();
    assert!(
        matches!(f::apply(&mut store,&changed,1,profile,&mut PolicySchemaCharge::new(profile.schema)),
        Err(error) if matches!(error.downcast_ref::<OriginalPolicyTransactionError>(),
            Some(OriginalPolicyTransactionError::Transaction(StateTransactionError::UnsupportedPolicyEffects))))
    );
    get(&mut store, 1, None, &[])?;
    drop(store);
    assert_eq!(f::cohort(&path)?, expected);
    assert_unchanged_rich_and_meta(&path, &mut expected)?;
    Ok(())
}

fn assert_unchanged_rich_and_meta(path: &Path, expected: &mut f::Cohort) -> TestResult {
    let mut index = vec![108];
    index.extend_from_slice(&1_u32.to_be_bytes());
    index.extend_from_slice(&f::binary(b"customer")?);
    index.push(106);
    let governance = f::etf_map(&[(f::atom(b"effects")?, f::etf_map(&[])?)])?;
    let states = f::etf_map(&[])?;
    let raw = f::etf_policy(
        1,
        b"job",
        &[
            (b"governance", &governance),
            (b"indexed_attributes", &index),
            (b"states", &states),
        ],
    )?;
    f::rows_mut(expected)?.insert(f::key(6, &[b"g", b"z", b"t", b"job"]), raw);
    f::upgrade6(
        expected,
        &f::scope()?,
        f::catalog(&f::scope()?, [None, None], None, None, None)?,
    )?;
    f::replace(path, expected)?;
    drop(f::open(path, 64)?);
    // Explicit restored component with state-local governance, coherent census
    // and strict reopen; the original root-governance cohort is restored below.
    let state_config = f::etf_map(&[(f::atom(b"governance")?, f::etf_map(&[])?)])?;
    let state_rows = f::etf_map(&[(f::binary(b"queued")?, state_config)])?;
    let mut state_governance = expected.clone();
    f::rows_mut(&mut state_governance)?.insert(
        f::key(6, &[b"g", b"z", b"t", b"job"]),
        f::etf_policy(1, b"job", &[(b"states", &state_rows)])?,
    );
    f::upgrade6(
        &mut state_governance,
        &f::scope()?,
        f::catalog(&f::scope()?, [None, None], None, None, None)?,
    )?;
    f::replace(path, &state_governance)?;
    drop(f::open(path, 64)?);
    f::assert_original_governance_claim_refused(path, 1, &state_governance)?;
    f::replace(path, expected)?;
    drop(f::open(path, 64)?);
    f::assert_original_governance_claim_refused(path, 1, expected)?;
    let bytes = frame(b"unchanged", 3, Raw::Map(&[]), Raw::List(&[]))?;
    let raw = f::etf_policy(
        2,
        b"job",
        &[
            (b"governance", &governance),
            (b"indexed_attributes", &index),
            (b"states", &states),
        ],
    )?;
    publication(path, b"unchanged", 3, 1, &bytes, expected, Some(&raw))?;
    f::assert_original_governance_claim_refused(path, 2, expected)?;
    let patch = [(
        Raw::Atom("indexed_state_meta"),
        Raw::List(&[Raw::Binary(b"customer")]),
    )];
    let bytes = frame(b"meta-change", 4, Raw::Map(&patch), Raw::List(&[]))?;
    let mut store = f::open(path, 64)?;
    let profile = f::limits();
    let error = f::apply(
        &mut store,
        &bytes,
        2,
        profile,
        &mut PolicySchemaCharge::new(profile.schema),
    )
    .err()
    .ok_or("migration change accepted")?;
    assert!(matches!(
        error.downcast_ref::<OriginalPolicyTransactionError>(),
        Some(OriginalPolicyTransactionError::Transaction(
            StateTransactionError::UnsupportedPolicyEffects
        ))
    ));
    drop(store);
    assert_eq!(f::cohort(path)?, *expected);
    drop(f::open(path, 64)?);
    Ok(())
}
