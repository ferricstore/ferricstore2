//! Real complete current policy and credit configuration under the parent guard.
use super::super::fixtures as leaf;
use super::{fixtures as a, *};
use wf_domain::{
    claim::caller::{
        ClaimCallerLimits, ClaimDueInput, ClaimPartitionsInput, ClaimStatesInput,
        normalize_claim_due,
    },
    governance::{
        caller::CreditPolicyInput,
        identified::{CreditConfiguration, IdentifiedCreditMutation as M, IdentifiedCreditRequest},
        release::LimitEnforcement,
    },
    policy::complete::RawFlowPolicyValue as Raw,
};
use wf_ports::log::{
    AtomicClaimOptions, GovernedClaimPayment, encode_atomic_claim_request, encode_credit_request,
    fingerprint_credit_policy, inspect_atomic_claim_reply,
};
use wf_ports::state::GroupApplyStore;
use wf_storage::transaction::{apply_atomic_claim_transaction, apply_credit_transaction};
use wf_types::identity::{PartitionId, RecordId, RequestId};

const DERIVED: &[u8] = b"flow-running:RUNJ5CLwUpcZHq0T4h09tSDlq-9SBV5JZLgvshP1k6E";

fn install(
    store: &mut RedbStateStore,
    at: u64,
    limit: u64,
    version: u64,
    running: bool,
) -> TestResult {
    let rule = [(Raw::Atom("limit"), Raw::Integer(i128::from(limit)))];
    let rules = [(
        Raw::Binary(if running { b"running" } else { b"unrelated" }),
        Raw::Map(&rule),
    )];
    let governance = [(Raw::Atom("limits"), Raw::Map(&rules))];
    let attrs = [
        (Raw::Atom("version"), Raw::Integer(i128::from(version))),
        (Raw::Atom("governance"), Raw::Map(&governance)),
    ];
    leaf::policy(store, at, 2490 + at, Raw::Map(&attrs))
}

fn fund(store: &mut RedbStateStore, ids: &[Vec<u8>], derived: bool) -> TestResult {
    let (scope, shard) = if derived {
        (DERIVED, 2)
    } else {
        (b"global".as_slice(), 0)
    };
    for (at, mutation, configuration) in [
        (
            6,
            M::Lease {
                shard_id: shard,
                amount: 4,
                ttl_ms: 100_000,
            },
            Some(CreditConfiguration {
                limit: 8,
                config_version: Some(1),
                policy_fingerprint: Some(fingerprint_credit_policy(CreditPolicyInput::Number(17))?),
            }),
        ),
        (
            7,
            M::Spend {
                shard_id: shard,
                lease_epoch: 1,
                amount: 4,
                ttl_ms: None,
                reservation_ids: ids,
            },
            None,
        ),
    ] {
        let frame = encode_credit_request(
            &f::scope()?,
            &RequestId::new(format!("fund-{at}").into_bytes())?,
            f::clock(2490 + at)?,
            IdentifiedCreditRequest {
                scope,
                shard_count: 4,
                configuration,
                mutation,
            },
            v::limits().record_batch.codec,
            v::limits().record_batch.workspace_bytes,
        )?;
        assert_eq!(
            leaf::retained_record(apply_credit_transaction(
                store,
                &f::scope()?,
                frame.as_bytes(),
                f::log(at),
                v::limits().record_batch,
                wf_domain::governance::credit::CreditLimits::default()
            )?)?
            .status(),
            RetainedStatus::Accepted
        );
    }
    Ok(())
}

fn frame(
    payment: Option<GovernedClaimPayment<'_>>,
    explicit: bool,
    request: &[u8],
    now: u64,
) -> TestResult<PreparedBytes> {
    frame_mode(payment, explicit, request, now, true)
}

fn frame_mode(
    payment: Option<GovernedClaimPayment<'_>>,
    explicit: bool,
    request: &[u8],
    now: u64,
    reclaim: bool,
) -> TestResult<PreparedBytes> {
    let keys: [&[u8]; 1] = [b"p"];
    let mut input = ClaimDueInput::new(b"r", b"new");
    input.states = ClaimStatesInput::One(b"q");
    input.partitions = ClaimPartitionsInput::Many(&keys);
    input.limit = Some(4);
    input.lease_ms = Some(100);
    input.reclaim_expired = Some(reclaim);
    if explicit {
        let paid = payment.ok_or("explicit fixture payment")?;
        input.governance_limit_scope = Some(paid.scope);
        input.governance_shard_id = Some(paid.shard_id);
    }
    let call = normalize_claim_due(input, ClaimCallerLimits::default())?;
    Ok(encode_atomic_claim_request(
        &f::scope()?,
        &RequestId::new(request.to_vec())?,
        f::clock(now)?,
        &call,
        AtomicClaimOptions {
            token_version: 1,
            first_ordinal: 0,
            scan_budget: 100,
            head_budget: 100,
            payment,
        },
        v::limits().record_batch.codec,
        v::limits().record_batch.workspace_bytes,
    )?)
}

fn payment(ids: &[Vec<u8>], derived: bool) -> GovernedClaimPayment<'_> {
    GovernedClaimPayment {
        scope: if derived { DERIVED } else { b"global" },
        shard_id: if derived { 2 } else { 0 },
        epoch: 1,
        enforcement: if derived {
            LimitEnforcement::StrictGlobal
        } else {
            LimitEnforcement::ApproximateGlobal
        },
        reservation_ids: ids,
    }
}

#[test]
fn actual_current_policy_routes_the_original_paid_batch_across_all_phases() -> TestResult {
    let dir = f::directory("atomic-current-policy-paid")?;
    let path = dir.join("state");
    let mut store = a::filled(&path)?;
    let ids = [b"a\0".to_vec(), b"b".to_vec(), b"c".to_vec(), b"d".to_vec()];
    install(&mut store, 5, 8, 17, true)?;
    fund(&mut store, &ids, true)?;
    let encoded = frame(Some(payment(&ids, true)), false, b"derived", 3000)?;
    let outcome = leaf::retained(apply_atomic_claim_transaction(
        &mut store,
        &f::scope()?,
        encoded.as_bytes(),
        f::log(8),
        a::limits(),
    )?)?;
    let reply = inspect_atomic_claim_reply(
        outcome.response(),
        outcome.original_time(),
        v::limits().record_batch.codec.admission(),
    )?;
    assert_eq!(
        (reply.status(), reply.count(), reply.phase_count()),
        (RetainedStatus::Accepted, 4, 3)
    );
    let scope = f::scope()?;
    let tx = store.begin_apply(
        scope.group(),
        scope.recovery(),
        v::limits().record_batch.codec.admission(),
        10000,
        1024 * 1024,
        1024 * 1024,
    )?;
    for (id, original) in [b"r1", b"q1", b"r2", b"r3"].into_iter().zip(&ids) {
        let (_, binding) = tx
            .workflow_governance_binding(
                &scope,
                &PartitionId::new(b"p".to_vec())?,
                &RecordId::new(id.to_vec())?,
            )?
            .ok_or("binding")?;
        assert_eq!(
            (
                binding.scope.as_slice(),
                binding.shard_id,
                binding.reservation_id.as_slice()
            ),
            (DERIVED, 2, original.as_slice())
        );
    }
    drop(tx);
    install(&mut store, 9, 0, 18, true)?;
    drop(store);
    let mut reopened = f::open(&path)?;
    let retry = frame(
        Some(payment(&[b"unspent".to_vec()], true)),
        false,
        b"derived",
        3001,
    )?;
    assert_eq!(
        leaf::retained(apply_atomic_claim_transaction(
            &mut reopened,
            &scope,
            retry.as_bytes(),
            f::log(10),
            a::limits()
        )?)?,
        outcome
    );
    drop(reopened);
    std::fs::remove_dir_all(dir)?;
    Ok(())
}

#[test]
fn current_running_rule_requires_payment_before_any_unpaid_phase() -> TestResult {
    let dir = f::directory("atomic-current-policy-unpaid")?;
    let path = dir.join("state");
    let mut store = a::filled(&path)?;
    install(&mut store, 5, 8, 17, true)?;
    let encoded = frame(None, false, b"unpaid", 3000)?;
    let outcome = leaf::retained(apply_atomic_claim_transaction(
        &mut store,
        &f::scope()?,
        encoded.as_bytes(),
        f::log(6),
        a::limits(),
    )?)?;
    assert_eq!(
        outcome.status(),
        RetainedStatus::ClaimRejected(
            wf_domain::claim::apply::ClaimRejection::GoverningReservationUnavailable
        )
    );
    assert_eq!(leaf::record(&mut store, b"r1")?.record.version(), 2);
    assert_eq!(leaf::record(&mut store, b"q1")?.record.version(), 1);
    drop(store);
    drop(f::open(&path)?);
    std::fs::remove_dir_all(dir)?;
    Ok(())
}

#[test]
fn a_valid_explicit_paid_pair_skips_unused_governance_but_uses_actual_policy_modes() -> TestResult {
    let dir = f::directory("atomic-current-policy-explicit")?;
    let path = dir.join("state");
    let mut store = a::filled(&path)?;
    let ids = [b"a".to_vec(), b"b".to_vec(), b"c".to_vec(), b"d".to_vec()];
    install(&mut store, 5, 0, 17, true)?;
    fund(&mut store, &ids, false)?;
    let encoded = frame(Some(payment(&ids, false)), true, b"explicit", 3000)?;
    let outcome = leaf::retained(apply_atomic_claim_transaction(
        &mut store,
        &f::scope()?,
        encoded.as_bytes(),
        f::log(8),
        a::limits(),
    )?)?;
    assert_eq!(outcome.status(), RetainedStatus::Accepted);
    assert_eq!(
        inspect_atomic_claim_reply(
            outcome.response(),
            outcome.original_time(),
            v::limits().record_batch.codec.admission()
        )?
        .count(),
        4
    );
    drop(store);
    drop(f::open(&path)?);
    std::fs::remove_dir_all(dir)?;
    Ok(())
}

#[test]
fn governance_without_a_running_rule_does_not_require_running_credit() -> TestResult {
    let dir = f::directory("atomic-current-policy-unrelated")?;
    let path = dir.join("state");
    let mut store = a::filled(&path)?;
    install(&mut store, 5, 0, 17, false)?;
    let encoded = frame(None, false, b"unrelated", 3000)?;
    let outcome = leaf::retained(apply_atomic_claim_transaction(
        &mut store,
        &f::scope()?,
        encoded.as_bytes(),
        f::log(6),
        a::limits(),
    )?)?;
    assert_eq!(outcome.status(), RetainedStatus::Accepted);
    drop(store);
    drop(f::open(&path)?);
    std::fs::remove_dir_all(dir)?;
    Ok(())
}

fn assert_rejection_only(
    path: &std::path::Path,
    before: &[std::collections::BTreeMap<Vec<u8>, Vec<u8>>],
) -> TestResult {
    let after = leaf::cohort(path)?;
    let mut controls = 0;
    for (key, value) in &before[1] {
        if value.starts_with(b"FWSC\0\x01") {
            controls += 1;
        } else {
            assert_eq!(
                after[1].get(key),
                Some(value),
                "unchanged workflow/credit row {key:?}"
            );
        }
    }
    assert_eq!(controls, 1);
    assert_eq!(after[1].len(), before[1].len() + 1);
    assert!(
        after[1]
            .iter()
            .find(|(key, _)| !before[1].contains_key(*key))
            .ok_or("one receipt")?
            .1
            .starts_with(b"FWSO\0\x01")
    );
    assert_eq!(after[0], before[0]);
    assert_eq!(after[2..], before[2..]);
    Ok(())
}

#[test]
fn actual_generation_limit_and_fingerprint_must_all_match_current_policy() -> TestResult {
    for (limit, version, reinstall) in [(8, 18, false), (7, 17, false), (8, 17, true)] {
        let dir = f::directory("atomic-current-policy-stale-owner")?;
        let path = dir.join("state");
        let mut store = a::filled(&path)?;
        let ids = [b"a".to_vec(), b"b".to_vec(), b"c".to_vec(), b"d".to_vec()];
        install(&mut store, 5, limit, version, true)?;
        fund(&mut store, &ids, true)?;
        let at = if reinstall {
            install(&mut store, 8, 8, 17, true)?;
            9
        } else {
            8
        };
        drop(store);
        let before = leaf::cohort(&path)?;
        let mut store = f::open(&path)?;
        let encoded = frame(Some(payment(&ids, true)), false, b"stale-config", 3000)?;
        let outcome = leaf::retained(apply_atomic_claim_transaction(
            &mut store,
            &f::scope()?,
            encoded.as_bytes(),
            f::log(at),
            a::limits(),
        )?)?;
        assert_eq!(
            outcome.status(),
            RetainedStatus::ClaimRejected(
                wf_domain::claim::apply::ClaimRejection::GoverningReservationUnavailable
            )
        );
        let reply = inspect_atomic_claim_reply(
            outcome.response(),
            outcome.original_time(),
            v::limits().record_batch.codec.admission(),
        )?;
        assert_eq!((reply.count(), reply.phase_count()), (0, 0));
        assert_eq!(reply.payment().ok_or("original payment")?.count(), 4);
        assert!(reply.used_indices().next().is_none());
        drop(store);
        assert_rejection_only(&path, &before)?;
        drop(f::open(&path)?);
        std::fs::remove_dir_all(dir)?;
    }
    Ok(())
}

#[test]
fn current_policy_refuses_a_foreign_derived_scope_shard_or_enforcement_without_writes() -> TestResult
{
    let dir = f::directory("atomic-current-policy-wrong-route")?;
    let path = dir.join("state");
    let mut store = a::filled(&path)?;
    let ids = [b"a".to_vec(), b"b".to_vec(), b"c".to_vec(), b"d".to_vec()];
    install(&mut store, 5, 8, 17, true)?;
    fund(&mut store, &ids, true)?;
    drop(store);
    let before = leaf::cohort(&path)?;
    for field in 0..3 {
        let mut store = f::open(&path)?;
        let mut paid = payment(&ids, true);
        match field {
            0 => paid.scope = b"foreign",
            1 => paid.shard_id = 1,
            _ => paid.enforcement = LimitEnforcement::ApproximateGlobal,
        }
        let encoded = frame(Some(paid), false, b"wrong-route", 3000)?;
        assert_eq!(
            apply_atomic_claim_transaction(
                &mut store,
                &f::scope()?,
                encoded.as_bytes(),
                f::log(8),
                a::limits()
            )
            .err(),
            Some(wf_storage::transaction::ClaimTransactionError::Transaction(
                wf_ports::state::StateTransactionError::InvalidChanges
            ))
        );
        drop(store);
        assert_eq!(leaf::cohort(&path)?, before);
    }
    std::fs::remove_dir_all(dir)?;
    Ok(())
}

fn budget_attempt(cap: u64, reclaim: bool) -> TestResult<bool> {
    use wf_domain::policy::complete::PolicySchemaLimits;
    let dir = f::directory("atomic-current-policy-account")?;
    let path = dir.join("state");
    let mut store = a::filled(&path)?;
    let ids = [b"a".to_vec(), b"b".to_vec(), b"c".to_vec(), b"d".to_vec()];
    install(&mut store, 5, 8, 17, true)?;
    fund(&mut store, &ids, true)?;
    drop(store);
    let before = leaf::cohort(&path)?;
    let mut store = f::open(&path)?;
    let mut limits = a::limits();
    limits.values.record_batch.schema =
        PolicySchemaLimits::new(limits.values.record_batch.schema.policy_limits(), cap)?;
    let encoded = frame_mode(Some(payment(&ids, true)), false, b"budget", 3000, reclaim)?;
    let result = apply_atomic_claim_transaction(
        &mut store,
        &f::scope()?,
        encoded.as_bytes(),
        f::log(8),
        limits,
    );
    let accepted = match result {
        Ok(outcome) => {
            let outcome = leaf::retained(outcome)?;
            let reply = inspect_atomic_claim_reply(
                outcome.response(),
                outcome.original_time(),
                v::limits().record_batch.codec.admission(),
            )?;
            assert_eq!(reply.status(), RetainedStatus::Accepted);
            assert_eq!(
                (reply.count(), reply.phase_count()),
                if reclaim { (4, 3) } else { (1, 1) }
            );
            true
        }
        Err(error) => {
            assert!(
                matches!(
                    error,
                    wf_storage::transaction::ClaimTransactionError::Transaction(
                        wf_ports::state::StateTransactionError::Limit
                    ) | wf_storage::transaction::ClaimTransactionError::Policy(
                        wf_domain::policy::complete::CompletePolicyEncodingError::Schema(
                            wf_domain::policy::complete::PolicySchemaError::WorkBytes { .. }
                        )
                    )
                ),
                "budget {cap}: {error:?}"
            );
            drop(store);
            assert_eq!(leaf::cohort(&path)?, before, "no prefix at budget {cap}");
            store = f::open(&path)?;
            false
        }
    };
    drop(store);
    std::fs::remove_dir_all(dir)?;
    Ok(accepted)
}

#[test]
fn one_schema_budget_that_accepts_one_phase_does_not_reset_for_three_phases() -> TestResult {
    let mut observed = None;
    for cap in [
        65_536, 131_072, 262_144, 524_288, 1_048_576, 2_097_152, 4_194_304, 8_388_608,
    ] {
        if budget_attempt(cap, false)? && !budget_attempt(cap, true)? {
            observed = Some(cap);
            break;
        }
    }
    assert!(
        observed.is_some(),
        "the three real phases must consume the same account"
    );
    assert!(budget_attempt(128 * 1024 * 1024, true)?);
    Ok(())
}
