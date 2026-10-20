//! Original retained identity precedes changed current formats and context.
mod authority;
use super::{TestResult, fixtures as f};
use std::path::Path;
use wf_domain::policy::complete::{PolicySchemaCharge, RawFlowPolicyValue as Raw};
use wf_domain::workflow::request::{RetainedOutcome, RetainedStatus};
use wf_ports::state::StateTransactionError;
use wf_storage::transaction::{OriginalPolicyTransactionError, RecordBatchTransactionOutcome};
use wf_types::identity::RequestId;

fn public_frame(
    kind: &[u8],
    request: &[u8],
    now: u64,
    attrs: Raw<'_>,
    opts: Raw<'_>,
) -> TestResult<Vec<u8>> {
    let profile = f::limits();
    f::public_frame(
        &f::scope()?,
        &RequestId::new(request.to_vec())?,
        now,
        kind,
        attrs,
        opts,
        &mut PolicySchemaCharge::new(profile.schema),
    )
}

fn complete_wire(kind: &[u8], generation: u64) -> TestResult<Vec<u8>> {
    let mut snapshot = b"FWPS\0\x02".to_vec();
    f::raw(&mut snapshot, kind)?;
    snapshot.extend_from_slice(&[0; 4]);
    snapshot.extend_from_slice(&0_u32.to_be_bytes());
    snapshot.extend_from_slice(&[0; 3]);
    let mut bytes = b"FWPV\0\x02".to_vec();
    bytes.extend_from_slice(&generation.to_be_bytes());
    f::raw(&mut bytes, &snapshot)?;
    Ok(bytes)
}

fn raw_policy(generation: u64, kind: &[u8]) -> TestResult<Vec<u8>> {
    f::etf_policy(generation, kind, &[(b"states", &f::etf_map(&[])?)])
}

fn install(
    path: &Path,
    kind: &[u8],
    request: &[u8],
    at: u64,
    now: u64,
    history: u32,
) -> TestResult<(Vec<u8>, Box<RetainedOutcome>)> {
    let mut expected = f::cohort(path)?;
    let frame = public_frame(kind, request, now, Raw::Map(&[]), Raw::List(&[]))?;
    let raw = raw_policy(1, kind)?;
    let s = f::scope()?;
    f::rows_mut(&mut expected)?.insert(f::key(6, &[b"g", b"z", b"t", kind]), raw.clone());
    let metadata = f::metadata(
        &s,
        request,
        at,
        now,
        f::frame_intent(&frame, at)?,
        RetainedStatus::Accepted,
    )?;
    f::finish(
        &mut expected,
        &s,
        &frame,
        f::log(at),
        (f::clock(now)?, f::clock(now)?),
        Some((&metadata, 1, &raw)),
    )?;
    f::upgrade6(
        &mut expected,
        &s,
        f::catalog(&s, [None, None], None, None, None)?,
    )?;
    let mut store = f::open(path, history)?;
    let profile = f::limits();
    let RecordBatchTransactionOutcome::Retained { outcome, .. } = f::apply(
        &mut store,
        &frame,
        at,
        profile,
        &mut PolicySchemaCharge::new(profile.schema),
    )?
    else {
        return Err("original install did not retain".into());
    };
    assert_eq!(
        *outcome,
        RetainedOutcome::new(metadata, 1, raw, profile.codec.admission())?
    );
    drop(store);
    assert_eq!(f::cohort(path)?, expected);
    drop(f::open(path, history)?);
    Ok((frame, outcome))
}

fn rejected(path: &Path) -> TestResult<(Vec<u8>, Box<RetainedOutcome>)> {
    use wf_domain::policy::complete::{
        FlowPolicyInputRejection as R, PolicyRetentionInputRejection as H,
    };
    let history = [(Raw::Atom("history_max_events"), Raw::Integer(65))];
    let attrs = [(Raw::Atom("retention"), Raw::Map(&history))];
    let bytes = public_frame(b"job", b"original", 100, Raw::Map(&attrs), Raw::List(&[]))?;
    let mut expected = f::cohort(path)?;
    let s = f::scope()?;
    let status =
        RetainedStatus::OriginalPolicySetRejected(R::Retention(H::History { maximum: 64 }));
    let body = b"ERR flow retention history_max_events must be between 1 and 64";
    let metadata = f::metadata(&s, b"original", 0, 100, f::frame_intent(&bytes, 0)?, status)?;
    f::finish(
        &mut expected,
        &s,
        &bytes,
        f::log(0),
        (f::clock(100)?, f::clock(100)?),
        Some((&metadata, 1, body)),
    )?;
    let mut store = f::open(path, 64)?;
    let profile = f::limits();
    let RecordBatchTransactionOutcome::Retained { outcome, .. } = f::apply(
        &mut store,
        &bytes,
        0,
        profile,
        &mut PolicySchemaCharge::new(profile.schema),
    )?
    else {
        return Err("rejection did not retain".into());
    };
    assert_eq!(
        *outcome,
        RetainedOutcome::new(metadata, 1, body.to_vec(), profile.codec.admission())?
    );
    drop(store);
    assert_eq!(f::cohort(path)?, expected);
    Ok((bytes, outcome))
}

#[test]
fn original_receipt_precedes_changed_format_context_and_order() -> TestResult {
    let dir = f::directory("original-replay")?;
    for business in [false, true] {
        let path = dir.join(if business { "rejected" } else { "accepted" });
        drop(f::create(&path, &[], 64)?);
        let (frame, original) = if business {
            rejected(&path)?
        } else {
            install(&path, b"job", b"original", 0, 100, 64)?
        };
        let (next, now) = if business {
            install(&path, b"other", b"advance", 1, 101, 64)?;
            (2, 101)
        } else {
            (1, 100)
        };
        let mut expected = f::cohort(&path)?;
        // Explicit restored alternate CURRENT authority, with its full census;
        // no claim this executes deferred same-type cross-format publication.
        f::rows_mut(&mut expected)?.insert(
            f::key(6, &[b"g", b"z", b"t", b"job"]),
            complete_wire(b"job", 1)?,
        );
        f::upgrade6(
            &mut expected,
            &f::scope()?,
            f::catalog(&f::scope()?, [None, None], None, None, None)?,
        )?;
        f::replace(&path, &expected)?;
        drop(f::open(&path, 65)?);
        nonfresh(&path, &frame, next, now, &original, &mut expected)?;
        let replace = [Raw::Atom("replace"), Raw::Atom("true")];
        let conflict = public_frame(
            b"job",
            b"original",
            100,
            Raw::Map(&[]),
            Raw::List(&[Raw::Tuple(&replace)]),
        )?;
        let profile = f::limits();
        let mut store = f::open(&path, 65)?;
        assert!(matches!(
            f::apply(
                &mut store,
                &conflict,
                next + 1,
                profile,
                &mut PolicySchemaCharge::new(profile.schema)
            )?,
            RecordBatchTransactionOutcome::IntentConflict(_)
        ));
        f::finish(
            &mut expected,
            &f::scope()?,
            &conflict,
            f::log(next + 1),
            (f::clock(now)?, f::clock(100)?),
            None,
        )?;
        drop(store);
        assert_eq!(f::cohort(&path)?, expected);
        let (advance, _) = install(&path, b"later", b"expiry-clock", next + 2, 1100, 65)?;
        assert_ne!(advance, [] as [u8; 0]);
        expected = f::cohort(&path)?;
        let mut store = f::open(&path, 65)?;
        assert!(matches!(
            f::apply(
                &mut store,
                &frame,
                next + 3,
                profile,
                &mut PolicySchemaCharge::new(profile.schema)
            )?,
            RecordBatchTransactionOutcome::Expired(_)
        ));
        f::finish(
            &mut expected,
            &f::scope()?,
            &frame,
            f::log(next + 3),
            (f::clock(1100)?, f::clock(100)?),
            None,
        )?;
        drop(store);
        assert_eq!(f::cohort(&path)?, expected);
        drop(f::open(&path, 65)?);
    }
    std::fs::remove_dir_all(dir)?;
    Ok(())
}

fn nonfresh(
    path: &Path,
    frame: &[u8],
    at: u64,
    now: u64,
    original: &RetainedOutcome,
    expected: &mut f::Cohort,
) -> TestResult {
    let profile = f::limits();
    let mut store = f::open(path, 65)?;
    let RecordBatchTransactionOutcome::Retained { outcome, .. } = f::apply(
        &mut store,
        frame,
        at,
        profile,
        &mut PolicySchemaCharge::new(profile.schema),
    )?
    else {
        return Err("original replay missing".into());
    };
    assert_eq!(*outcome, *original);
    assert_eq!(outcome.original_time().get(), 100);
    assert_eq!(outcome.expires_at().get(), 1100);
    f::finish(
        expected,
        &f::scope()?,
        frame,
        f::log(at),
        (f::clock(now)?, f::clock(100)?),
        None,
    )?;
    assert!(matches!(
        f::apply(
            &mut store,
            frame,
            at,
            profile,
            &mut PolicySchemaCharge::new(profile.schema)
        )?,
        RecordBatchTransactionOutcome::LogReplay(_)
    ));
    drop(store);
    assert_eq!(f::cohort(path)?, *expected);
    drop(f::open(path, 65)?);
    Ok(())
}

#[test]
fn mixed_complete_original_types_keep_layout6_and_exact_identity() -> TestResult {
    let dir = f::directory("original-mixed")?;
    let path = dir.join("state");
    let complete = complete_wire(b"complete", 1)?;
    drop(f::create(
        &path,
        &[(b"complete".to_vec(), complete.clone())],
        64,
    )?);
    install(&path, b"job", b"original", 0, 10, 64)?;
    let before = f::cohort(&path)?;
    let profile = f::limits();
    let mut store = f::open(&path, 64)?;
    let cross = public_frame(
        b"complete",
        b"cross-format",
        11,
        Raw::Map(&[]),
        Raw::List(&[]),
    )?;
    let error = f::apply(
        &mut store,
        &cross,
        1,
        profile,
        &mut PolicySchemaCharge::new(profile.schema),
    )
    .err()
    .ok_or("cross format accepted")?;
    assert!(matches!(
        error.downcast_ref::<OriginalPolicyTransactionError>(),
        Some(OriginalPolicyTransactionError::Transaction(
            StateTransactionError::UnsupportedPolicyFormat
        ))
    ));
    drop(store);
    assert_eq!(f::cohort(&path)?, before);
    mixed_mutation_and_core(&path, &complete)?;
    drop(f::open(&path, 64)?);
    std::fs::remove_dir_all(dir)?;
    Ok(())
}

fn mixed_mutation_and_core(path: &Path, complete: &[u8]) -> TestResult {
    use wf_ports::log::verify_complete_core_policy_patch_frame;
    use wf_storage::transaction::{
        CorePolicyTransactionError, apply_complete_core_policy_patch_transaction,
    };
    let s = f::scope()?;
    let profile = f::limits();
    let before = f::cohort(path)?;
    let mut store = f::open(path, 64)?;
    let wrong = complete_core_frame(b"job", b"core-cross", 11)?;
    assert_eq!(
        apply_complete_core_policy_patch_transaction(
            &mut store,
            &s,
            wrong.as_bytes(),
            f::log(1),
            profile
        ),
        Err(CorePolicyTransactionError::Transaction(
            StateTransactionError::UnsupportedPolicyFormat
        ))
    );
    drop(store);
    assert_eq!(f::cohort(path)?, before);
    let mut expected = complete_record_mutations(path, before)?;
    let bytes = complete_core_frame(b"complete", b"core-v2", 40)?;
    let mut new = complete.to_vec();
    new[6..14].copy_from_slice(&2_u64.to_be_bytes());
    f::rows_mut(&mut expected)?.insert(f::key(6, &[b"g", b"z", b"t", b"complete"]), new.clone());
    let intent = verify_complete_core_policy_patch_frame(
        bytes.as_bytes(),
        profile.schema,
        profile.codec.admission(),
        &mut PolicySchemaCharge::new(profile.schema),
    )?
    .header(f::log(3))
    .intent;
    let metadata = f::metadata(&s, b"core-v2", 3, 40, intent, RetainedStatus::Accepted)?;
    f::finish(
        &mut expected,
        &s,
        bytes.as_bytes(),
        f::log(3),
        (f::clock(40)?, f::clock(40)?),
        Some((&metadata, 1, &new)),
    )?;
    f::upgrade6(
        &mut expected,
        &s,
        f::catalog(&s, [None, None], None, None, None)?,
    )?;
    store = f::open(path, 64)?;
    assert!(
        matches!(apply_complete_core_policy_patch_transaction(&mut store,&s,bytes.as_bytes(),f::log(3),profile)?,
        RecordBatchTransactionOutcome::Retained{outcome,..}if outcome.response()==new)
    );
    drop(store);
    assert_eq!(f::cohort(path)?, expected);
    Ok(())
}

fn complete_core_frame(
    kind: &[u8],
    request: &[u8],
    now: u64,
) -> TestResult<wf_ports::log::PreparedBytes> {
    use wf_domain::policy::complete::{
        CompleteCorePolicyPatchInput, CompleteCorePolicyPatchRequestInput,
    };
    use wf_domain::policy::definition::PolicyPatch;
    use wf_ports::log::prepare_complete_core_policy_patch_frame;
    let s = f::scope()?;
    let profile = f::limits();
    Ok(prepare_complete_core_policy_patch_frame(
        CompleteCorePolicyPatchRequestInput::new(
            s.clone(),
            RequestId::new(request.to_vec())?,
            f::clock(now)?,
            CompleteCorePolicyPatchInput {
                workflow_type: kind.to_vec(),
                expected_generation: Some(1),
                patch: PolicyPatch::default(),
            },
        ),
        profile.schema,
        profile.codec.admission(),
        &mut PolicySchemaCharge::new(profile.schema),
    )?)
}

fn complete_record_mutations(path: &Path, expected: f::Cohort) -> TestResult<f::Cohort> {
    let s = f::scope()?;
    let mut store = f::open(path, 64)?;
    let empty = f::claim_frame(&s, b"v2-query", 20, b"complete", b"worker", None, None)?;
    let response = f::receipt(&[], 0, 0, 0)?;
    f::claim(&mut store, &s, &empty, 1, &response)?;
    drop(store);
    let mut expected = expected;
    let metadata = f::metadata(
        &s,
        b"v2-query",
        1,
        20,
        wf_domain::workflow::request::ClientIntent::new(1, [61; 32])?,
        RetainedStatus::Accepted,
    )?;
    f::finish(
        &mut expected,
        &s,
        &empty,
        f::log(1),
        (f::clock(20)?, f::clock(20)?),
        Some((&metadata, 3, &response)),
    )?;
    assert_eq!(f::cohort(path)?, expected);
    let item = f::owner(
        &s,
        b"p",
        f::data(b"v2-record", b"complete", b"q", 30, Some(30))?,
        0,
        2_u128 << 32,
    )?;
    store = f::open(path, 64)?;
    f::seed(&mut store, &s, std::slice::from_ref(&item), 2, 30)?;
    drop(store);
    f::put_record(&mut expected, &s, &item)?;
    f::rows_mut(&mut expected)?.insert(f::due(&s, &item)?, vec![0]);
    let seed = f::batch(
        &s,
        b"seed-2",
        30,
        vec![wf_ports::consensus::PreparedBatchItem {
            partition: item.partition.clone(),
            create_priority: 0,
            command: wf_domain::workflow::CommittedCommand {
                id: item.record.id().clone(),
                now: wf_types::version::TimestampMillis::new(30),
                operation: wf_domain::workflow::Operation::Create(wf_domain::workflow::Create {
                    workflow_type: b"complete".to_vec(),
                    state: b"q".to_vec(),
                    run_at: Some(wf_types::version::TimestampMillis::new(30)),
                    payload: None,
                }),
            },
        }],
    )?;
    let metadata = f::metadata(
        &s,
        b"seed-2",
        2,
        30,
        f::batch_intent(seed.as_bytes(), 2)?,
        RetainedStatus::Accepted,
    )?;
    f::finish(
        &mut expected,
        &s,
        seed.as_bytes(),
        f::log(2),
        (f::clock(30)?, f::clock(30)?),
        Some((&metadata, 1, &[0])),
    )?;
    assert_eq!(f::cohort(path)?, expected);
    Ok(expected)
}
