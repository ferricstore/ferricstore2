//! Actual public caller identity through the existing sparse atomic credit driver.
use super::{CreditLimits, TestResult, common, fixtures as f};
use wf_domain::governance::{
    caller::{CreditCallKind as K, CreditOptionValue as V, normalize_credit_call},
    identified::IdentifiedCreditDecision as D,
};
use wf_ports::log::{CreditSpendPreparation as Spend, PreparedBytes, encode_public_credit_request};
use wf_ports::state::StateTransactionError as S;
use wf_storage::{
    state::{RedbStateStore, StateStoreGenesis, StateStoreLimits},
    transaction::{
        CreditTransactionError as E, RecordBatchTransactionOutcome as O,
        apply_public_credit_transaction,
    },
};
use wf_types::identity::RequestId;
fn create(path: &std::path::Path) -> TestResult<RedbStateStore> {
    let scope = common::scope()?;
    Ok(RedbStateStore::create_credit_catalog_database(
        redb::Builder::new().create(path)?,
        scope.group().clone(),
        scope.recovery().clone(),
        StateStoreGenesis {
            initial_time: common::clock(0)?,
            placements: &[],
            policies: &[],
        },
        f::defaults(),
        4,
        StateStoreLimits::default(),
    )?)
}
fn frame(
    id: &[u8],
    kind: K,
    now: u64,
    opts: &[(&str, V<'_>)],
    spend: Option<Spend>,
) -> TestResult<PreparedBytes> {
    let call = normalize_credit_call(kind, b"global", 4, opts, 1024 * 1024)?;
    Ok(encode_public_credit_request(
        &common::scope()?,
        &RequestId::new(id.to_vec())?,
        call,
        common::clock(now)?,
        spend,
        common::driver_limits().codec,
        16 * common::MIB,
    )?)
}
fn apply(store: &mut RedbStateStore, frame: &PreparedBytes, index: u64) -> TestResult<O> {
    Ok(apply_public_credit_transaction(
        store,
        &common::scope()?,
        frame.as_bytes(),
        common::log(index),
        common::driver_limits(),
        CreditLimits::default(),
    )?)
}
fn retained(outcome: O) -> TestResult<Box<wf_domain::workflow::request::RetainedOutcome>> {
    let O::Retained { outcome, .. } = outcome else {
        return Err("missing public receipt".into());
    };
    assert_eq!(
        (outcome.intent().version(), outcome.response_version()),
        (17, 10)
    );
    Ok(outcome)
}
#[test]
fn public_credit_transaction_retains_original_spend_before_new_epoch_or_changed_preparation()
-> TestResult {
    let dir = common::directory("public-credit-retained")?;
    let path = dir.join("state");
    let mut store = create(&path)?;
    let lease = [
        ("shard_id", V::Integer(0)),
        ("amount", V::Integer(2)),
        ("limit", V::Integer(2)),
        ("ttl_ms", V::Integer(1000)),
    ];
    retained(apply(
        &mut store,
        &frame(b"lease", K::Lease, 1000, &lease, None)?,
        0,
    )?)?;
    let opts = [("shard_id", V::Integer(0)), ("amount", V::Integer(2))];
    let first = frame(
        b"spend",
        K::Spend,
        1001,
        &opts,
        Some(Spend {
            lease_epoch: 1,
            batch: [0; 16],
        }),
    )?;
    let original = retained(apply(&mut store, &first, 1)?)?;
    let decoded = super::decode_credit_reply(
        original.response(),
        CreditLimits::default(),
        16 * common::MIB,
        32 * common::MIB,
    )?;
    assert_eq!(decoded.decision, D::Spent { shard_id: 0 });
    assert_eq!(decoded.reservation_ids.len(), 2);
    let changed = frame(
        b"spend",
        K::Spend,
        1002,
        &opts,
        Some(Spend {
            lease_epoch: 999,
            batch: [9; 16],
        }),
    )?;
    assert_eq!(retained(apply(&mut store, &changed, 2)?)?, original);
    let changed_call = frame(
        b"spend",
        K::Spend,
        1003,
        &[("shard_id", V::Integer(0)), ("amount", V::Integer(1))],
        Some(Spend {
            lease_epoch: 999,
            batch: [9; 16],
        }),
    )?;
    assert!(matches!(
        apply(&mut store, &changed_call, 3)?,
        O::IntentConflict(_)
    ));
    let owner = store
        .read_credit_owner(
            &common::scope()?,
            b"global",
            CreditLimits::default(),
            10000,
            64 * common::MIB,
        )?
        .ok_or("missing owner")?;
    assert_eq!(owner.lease(0).ok_or("missing lease")?.in_use, 2);
    drop(store);
    drop(common::open(&path)?);
    std::fs::remove_dir_all(dir)?;
    Ok(())
}
#[test]
fn public_credit_transaction_requires_actual_catalog_credit_profile_before_any_write() -> TestResult
{
    let dir = common::directory("public-credit-old")?;
    let path = dir.join("state");
    drop(f::create(&path)?);
    let before = f::cohort(&path)?;
    let mut store = common::open(&path)?;
    let get = frame(b"get", K::Get, 1000, &[], None)?;
    assert_eq!(
        apply_public_credit_transaction(
            &mut store,
            &common::scope()?,
            get.as_bytes(),
            common::log(0),
            common::driver_limits(),
            CreditLimits::default()
        )
        .err(),
        Some(E::State(S::UnsupportedPolicyProfile))
    );
    drop(store);
    assert_eq!(f::cohort(&path)?, before);
    std::fs::remove_dir_all(dir)?;
    Ok(())
}
