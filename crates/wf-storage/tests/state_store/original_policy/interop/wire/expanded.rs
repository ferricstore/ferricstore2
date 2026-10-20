//! Actual wide V2 donor, transient ETF and source-lazy reduction controls.
use super::{TestResult, accepted, f};
use wf_domain::policy::complete::{
    CompletePolicyData, CompletePolicyEncodingError as EncodingError, CompleteStatePolicyData,
    OriginalPolicyReplyError, OriginalPolicyReplyValue, PolicySchemaCharge,
    RawFlowPolicyValue as Raw, prepare_complete_policy_value_v2, restore_complete_policy,
};
use wf_storage::{state::RedbStateStore, transaction::with_original_policy_get_reply};
use wf_types::identity::{PartitionId, RequestId};

fn expanded_caps() -> TestResult<wf_storage::transaction::RecordBatchTransactionLimits> {
    use wf_domain::policy::{complete::PolicySchemaLimits, installation::PolicyLimits};
    let mut caps = f::limits();
    caps.schema = PolicySchemaLimits::new(PolicyLimits::default(), 2_000_000_000)?;
    caps.maximum_read_bytes = 128 * 1_048_576;
    caps.maximum_staging_bytes = 256 * 1_048_576;
    caps.maximum_adapter_bytes = 512 * 1_048_576;
    caps.maximum_read_steps = 1_000_000;
    caps.workspace_bytes = 3_500_000_000;
    Ok(caps)
}
fn expanded_store_limits() -> TestResult<wf_storage::state::StateStoreLimits> {
    use wf_storage::state::StateStoreLimits;
    let caps = expanded_caps()?;
    Ok(StateStoreLimits::new(
        caps.codec.admission(),
        caps.schema,
        (1000, 256 * 1_048_576),
        (1_000_000, 1_073_741_824, 10_000_000),
        2_000_000_000,
        3_500_000_000,
    )?
    .with_history_max_events_limit(64)?
    .with_original_policy_reply_defaults(f::store_limits(64)?.original_policy_reply_defaults())?)
}
fn expanded_open(path: &std::path::Path) -> TestResult<RedbStateStore> {
    let scope = f::scope()?;
    Ok(RedbStateStore::open(
        path,
        scope.group().clone(),
        scope.recovery().clone(),
        expanded_store_limits()?,
    )?)
}
pub(super) fn run(dir: &std::path::Path) -> TestResult {
    let path = dir.join("expanded-v2");
    let names = seed(&path).map_err(|error| format!("expanded seed: {error}"))?;
    let before = f::cohort(&path)?;
    check_get(&path, &names).map_err(|error| format!("expanded GET: {error}"))?;
    assert_eq!(f::cohort(&path)?, before);
    let empty =
        lazy_updates(&path, &before).map_err(|error| format!("expanded lazy updates: {error}"))?;
    reducing_merge(dir, &before, &names, &empty)
        .map_err(|error| format!("expanded reducing merge: {error}").into())
}
fn seed(path: &std::path::Path) -> TestResult<Vec<Vec<u8>>> {
    use wf_storage::state::StateStoreGenesis;
    let scope = f::scope()?;
    let caps = expanded_caps()?;
    let names = (0..16_384)
        .map(|n| format!("s{n:06}").into_bytes())
        .collect::<Vec<_>>();
    let states = names
        .iter()
        .map(|name| {
            (
                name.clone(),
                CompleteStatePolicyData {
                    mode: None,
                    retry: None,
                    retention: None,
                    governance: None,
                },
            )
        })
        .collect();
    let data = CompletePolicyData {
        workflow_type: b"job".to_vec(),
        version: None,
        max_active_ms: None,
        retry: None,
        retention: None,
        states,
        indexed_attributes: None,
        indexed_state_meta: None,
        governance: None,
    };
    let mut setup = PolicySchemaCharge::new(caps.schema);
    let value = prepare_complete_policy_value_v2(
        restore_complete_policy(1, data, caps.schema, &mut setup)?,
        caps.schema,
        &mut setup,
    )?;
    assert_eq!(value.encoded().len(), 245_802);
    // Each independent ETF state is BINARY_EXT(7)+MAP_EXT(0)=17 bytes;
    // envelope/type/empty states framing53 is independent of the converter.
    let projected_bytes = 53 + names.iter().map(|name| 10 + name.len()).sum::<usize>();
    assert_eq!(projected_bytes, 278_581);
    assert!(projected_bytes > 263_168);
    let policies = [(scope.tenant().clone(), value)];
    let placements = [(PartitionId::new(b"p".to_vec())?, scope.group().clone())];
    drop(RedbStateStore::create(
        path,
        scope.group().clone(),
        scope.recovery().clone(),
        StateStoreGenesis {
            initial_time: f::clock(0)?,
            placements: &placements,
            policies: &policies,
        },
        expanded_store_limits()?,
    )?);
    Ok(names)
}
fn check_get(path: &std::path::Path, names: &[Vec<u8>]) -> TestResult {
    let scope = f::scope()?;
    let caps = expanded_caps()?;
    let mut store = expanded_open(path)?;
    let mut charge = PolicySchemaCharge::new(caps.schema);
    let mut visited = 0;
    let mut unseen = names
        .iter()
        .map(Vec::as_slice)
        .collect::<std::collections::BTreeSet<_>>();
    with_original_policy_get_reply(
        &mut store,
        &scope,
        &Raw::Binary(b"job"),
        &Raw::List(&[]),
        caps,
        &mut charge,
        |reply, charge| {
            visited += 1;
            let reply =
                reply.map_err(|_| OriginalPolicyReplyError::Encoding(EncodingError::Malformed))?;
            assert_eq!(reply.generation(), 1);
            let states = reply
                .root()
                .atom_field("states", charge)?
                .ok_or(EncodingError::Malformed)?;
            assert_eq!(states.map_len(), Some(names.len()));
            let mut rows = states.map_iter().ok_or(EncodingError::Malformed)?;
            while let Some((name, state)) = rows.next(charge)? {
                assert!(unseen.remove(name.binary_bytes().ok_or(EncodingError::Malformed)?));
                assert_eq!(state.map_len(), Some(5));
                assert_eq!(
                    state
                        .atom_field("mode", charge)?
                        .and_then(OriginalPolicyReplyValue::atom_bytes),
                    Some(b"parallel".as_slice())
                );
            }
            Ok(())
        },
    )?;
    assert_eq!(visited, 1);
    drop(store);
    assert!(unseen.is_empty());
    Ok(())
}
fn lazy_updates(path: &std::path::Path, before: &f::Cohort) -> TestResult<Vec<u8>> {
    use wf_storage::transaction::RecordBatchTransactionOutcome;
    let scope = f::scope()?;
    let caps = expanded_caps()?;
    let old = f::rows(before)?
        .get(&f::key(6, &[b"g", b"z", b"t", b"job"]))
        .ok_or("missing actual V2 donor")?;
    let stale = [Raw::Atom("expected_generation"), Raw::Integer(0)];
    let bytes = super::super::original_frame(
        b"large-stale",
        10,
        Raw::Map(&[]),
        Raw::List(&[Raw::Tuple(&stale)]),
    )?;
    let mut store = expanded_open(path)?;
    pure_lazy(
        &mut store,
        &bytes,
        old,
        0,
        b"ERR stale flow policy generation",
    )?;
    let RecordBatchTransactionOutcome::Retained { outcome, .. } = f::apply(
        &mut store,
        &bytes,
        0,
        caps,
        &mut PolicySchemaCharge::new(caps.schema),
    )?
    else {
        return Err("large stale did not retain".into());
    };
    assert_eq!(outcome.response(), b"ERR stale flow policy generation");
    drop(store);
    let mut expected = before.clone();
    let metadata = f::metadata(
        &scope,
        b"large-stale",
        0,
        10,
        f::frame_intent(&bytes, 0)?,
        outcome.status(),
    )?;
    f::finish(
        &mut expected,
        &scope,
        &bytes,
        f::log(0),
        (f::clock(10)?, f::clock(10)?),
        Some((&metadata, 1, b"ERR stale flow policy generation")),
    )?;
    assert_eq!(f::cohort(path)?, expected);
    let replace = [Raw::Atom("replace"), Raw::Atom("true")];
    let bytes = super::super::original_frame(
        b"large-replace",
        11,
        Raw::Map(&[]),
        Raw::List(&[Raw::Tuple(&replace)]),
    )?;
    store = expanded_open(path)?;
    let empty = f::etf_policy(2, b"job", &[(b"states", &f::etf_map(&[])?)])?;
    pure_lazy(&mut store, &bytes, old, 1, &empty)?;
    let RecordBatchTransactionOutcome::Retained { outcome, .. } = f::apply(
        &mut store,
        &bytes,
        1,
        caps,
        &mut PolicySchemaCharge::new(caps.schema),
    )?
    else {
        return Err("large replace did not retain".into());
    };
    assert_eq!(
        *outcome,
        accepted(
            &mut expected,
            &bytes,
            b"large-replace",
            (1, 11),
            f::frame_intent(&bytes, 1)?,
            (1, &empty)
        )?
    );
    drop(store);
    assert_eq!(f::cohort(path)?, expected);
    drop(expanded_open(path)?);
    Ok(empty)
}
// The actual store/current capture is outside this PURE account. Both the
// committed frame and reached Fresh decision share its fresh100M schema charge.
// A conservative lazy upper is97,795,920; selected eager full conversion alone
// requires g+J=132,079,388, even before owning/decoding the current value.
fn pure_lazy(
    store: &mut RedbStateStore,
    bytes: &[u8],
    old: &[u8],
    at: u64,
    response: &[u8],
) -> TestResult {
    use wf_domain::policy::complete::{
        OriginalPolicyReadValue as Read, OriginalPolicySetFreshView,
    };
    use wf_domain::policy::{complete::PolicySchemaLimits, installation::PolicyLimits};
    use wf_domain::workflow::request::gate::PointGateView;
    use wf_domain::workflow::request::{DispatchDecision, RetainedStatus};
    use wf_ports::log::{
        OriginalPolicySetPointInspection, decide_original_policy_set_point_fresh_bounded,
        inspect_original_policy_set_point_frame_bounded,
        prepare_original_policy_set_point_frame_bounded,
    };
    use wf_ports::state::{GroupApplyStore, GroupApplyTransaction};
    let scope = f::scope()?;
    let mut caps = expanded_caps()?;
    let control = {
        let tx = store.begin_apply(
            scope.group(),
            scope.recovery(),
            caps.codec.admission(),
            caps.maximum_read_steps,
            caps.maximum_read_bytes,
            caps.maximum_adapter_bytes,
        )?;
        tx.control()?
    };
    caps.schema = PolicySchemaLimits::new(PolicyLimits::default(), 100_000_000)?;
    let mut charge = PolicySchemaCharge::new(caps.schema);
    let verified = prepare_original_policy_set_point_frame_bounded(
        bytes,
        caps.schema,
        caps.codec.admission(),
        &mut charge,
        caps.workspace_bytes,
    )?;
    let OriginalPolicySetPointInspection::Fresh(fresh) =
        inspect_original_policy_set_point_frame_bounded(
            PointGateView {
                scope: &scope,
                control,
                retained_nonempty: at != 0,
                outcome: None,
            },
            verified,
            f::log(at),
            caps.schema,
            caps.codec.admission(),
            &mut charge,
            caps.workspace_bytes,
        )?
    else {
        return Err("same committed lazy frame was not Fresh".into());
    };
    let plan = decide_original_policy_set_point_fresh_bounded(
        *fresh,
        OriginalPolicySetFreshView {
            current: Read::Binary(old),
            active_migration: Ok(0),
            completed_migration: Ok(0),
            history_max_events_limit: Ok(64),
            outcome_retention_ms: Ok(1000),
        },
        caps.schema,
        caps.codec.admission(),
        &mut charge,
        caps.workspace_bytes,
    )?;
    let DispatchDecision::Fresh((install, outcome)) = plan.decision() else {
        return Err("lazy pure operation lost its Fresh result".into());
    };
    assert_eq!(outcome.response(), response);
    assert_eq!(outcome.response_version(), 1);
    assert_eq!(install.is_some(), at != 0);
    assert_eq!(outcome.status() == RetainedStatus::Accepted, at != 0);
    Ok(())
}
fn reducing_merge(
    dir: &std::path::Path,
    before: &f::Cohort,
    names: &[Vec<u8>],
    empty: &[u8],
) -> TestResult {
    let path = dir.join("expanded-reducing-merge");
    let scope = f::scope()?;
    let caps = expanded_caps()?;
    // Fork the exact actually validated complete predecessor, not an ETF
    // surrogate; the accepted stored output still obeys persisted ETF263168.
    drop(f::create(&path, &[], 64)?);
    f::replace(&path, before)?;
    let mut store = expanded_open(&path)?;
    let removes = names
        .iter()
        .map(|name| (Raw::Binary(name), Raw::Nil))
        .collect::<Vec<_>>();
    let attrs = [(Raw::Atom("states"), Raw::Map(&removes))];
    let bytes = f::public_frame(
        &scope,
        &RequestId::new(b"reduce".to_vec())?,
        10,
        b"job",
        Raw::Map(&attrs),
        Raw::List(&[]),
        &mut PolicySchemaCharge::new(caps.schema),
    )?;
    let wf_storage::transaction::RecordBatchTransactionOutcome::Retained { outcome, .. } =
        f::apply(
            &mut store,
            &bytes,
            0,
            caps,
            &mut PolicySchemaCharge::new(caps.schema),
        )?
    else {
        return Err("reducing merge did not retain".into());
    };
    let mut expected = before.clone();
    assert_eq!(
        *outcome,
        accepted(
            &mut expected,
            &bytes,
            b"reduce",
            (0, 10),
            f::frame_intent(&bytes, 0)?,
            (1, empty)
        )?
    );
    drop(store);
    assert_eq!(f::cohort(&path)?, expected);
    drop(expanded_open(&path)?);
    Ok(())
}
