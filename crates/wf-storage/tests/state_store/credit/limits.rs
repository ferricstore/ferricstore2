use super::{fixtures as f, *};
use std::sync::{
    Arc,
    atomic::{AtomicU8, Ordering},
};
use wf_ports::log::CodecError;
use wf_ports::state::StateTransactionError;
use wf_storage::state::{RedbStateStore, StateStoreGenesis, StateStoreLimits};

#[test]
fn credit_store_old_capability_and_wrong_actual_shard_count_refuse_unchanged() -> TestResult {
    let dir = common::directory("credit-closed")?;
    let path = dir.join("old");
    let scope = common::scope()?;
    drop(RedbStateStore::create_stored_value_database(
        redb::Builder::new().create(&path)?,
        scope.group().clone(),
        scope.recovery().clone(),
        StateStoreGenesis {
            initial_time: common::clock(0)?,
            placements: &common::placements()?,
            policies: &[],
        },
        f::defaults(),
        StateStoreLimits::default(),
    )?);
    let before = f::cohort(&path)?;
    let frame = f::frame(
        b"lease",
        1000,
        Some(f::config(1, 1)),
        Mutation::Lease {
            shard_id: 0,
            amount: 1,
            ttl_ms: 1000,
        },
    )?;
    let mut store = common::open(&path)?;
    assert!(matches!(
        apply_credit_transaction(
            &mut store,
            &scope,
            frame.as_bytes(),
            common::log(0),
            common::driver_limits(),
            CreditLimits::default()
        ),
        Err(CreditTransactionError::State(
            StateTransactionError::UnsupportedLayout
                | StateTransactionError::UnsupportedPolicyProfile
        ))
    ));
    drop(store);
    assert_eq!(f::cohort(&path)?, before);
    let path = dir.join("credit");
    drop(f::create(&path)?);
    let wrong = encode_credit_request(
        &scope,
        &wf_types::identity::RequestId::new(b"wrong".to_vec())?,
        common::clock(1000)?,
        IdentifiedCreditRequest {
            scope: b"global",
            shard_count: 8,
            configuration: Some(f::config(1, 1)),
            mutation: Mutation::Lease {
                shard_id: 7,
                amount: 1,
                ttl_ms: 1000,
            },
        },
        common::driver_limits().codec,
        16 * common::MIB,
    )?;
    let before = f::cohort(&path)?;
    let mut store = common::open(&path)?;
    assert!(
        apply_credit_transaction(
            &mut store,
            &scope,
            wrong.as_bytes(),
            common::log(0),
            common::driver_limits(),
            CreditLimits::default()
        )
        .is_err()
    );
    drop(store);
    assert_eq!(f::cohort(&path)?, before);
    std::fs::remove_dir_all(dir)?;
    Ok(())
}

#[test]
fn credit_store_narrow_workspaces_leave_every_table_and_control_unchanged() -> TestResult {
    let dir = common::directory("credit-limits")?;
    let path = dir.join("state");
    f::seed(&path)?;
    let ids = vec![b"fresh".to_vec()];
    let frame = f::frame(
        b"fresh",
        1002,
        None,
        Mutation::Spend {
            shard_id: 0,
            lease_epoch: 1,
            amount: 1,
            ttl_ms: None,
            reservation_ids: &ids,
        },
    )?;
    let before = f::cohort(&path)?;
    for field in 0..5 {
        let mut limits = common::driver_limits();
        match field {
            0 => limits.maximum_read_steps = 1,
            1 => limits.maximum_read_bytes = 1,
            2 => limits.maximum_staging_bytes = 1,
            3 => limits.maximum_adapter_bytes = 1,
            _ => limits.workspace_bytes = 1,
        }
        let mut store = common::open(&path)?;
        assert!(matches!(
            apply_credit_transaction(
                &mut store,
                &common::scope()?,
                frame.as_bytes(),
                common::log(2),
                limits,
                CreditLimits::default()
            ),
            Err(CreditTransactionError::State(StateTransactionError::Limit)
                | CreditTransactionError::Codec(
                    CodecError::ByteLimit | CodecError::OwnedLimit | CodecError::WorkLimit
                ))
        ));
        drop(store);
        assert_eq!(f::cohort(&path)?, before);
    }
    assert_eq!(
        f::retained(&path, &frame, 2)?.original_position(),
        common::log(2)
    );
    std::fs::remove_dir_all(dir)?;
    Ok(())
}

#[test]
fn credit_store_uncertain_commit_fences_and_reopens_a_complete_old_or_new_cohort() -> TestResult {
    let dir = common::directory("credit-uncertain")?;
    let path = dir.join("fault");
    let expected_path = dir.join("expected");
    f::seed(&path)?;
    f::seed(&expected_path)?;
    let before = f::cohort(&path)?;
    assert_eq!(f::cohort(&expected_path)?, before);
    let ids = vec![b"a".to_vec(), b"b".to_vec()];
    let frame = f::frame(
        b"release",
        1002,
        None,
        Mutation::Release {
            shard_id: 0,
            amount: 2,
            reservation_ids: &ids,
        },
    )?;
    f::retained(&expected_path, &frame, 2)?;
    let after = f::cohort(&expected_path)?;
    assert_ne!(before, after);
    let fault = Arc::new(AtomicU8::new(0));
    let mut store = crate::faults::wrapped(&path, Arc::clone(&fault))?;
    // Skip the engine's growth sync and fail after the publication sync.
    fault.store(4, Ordering::SeqCst);
    let result = apply_credit_transaction(
        &mut store,
        &common::scope()?,
        frame.as_bytes(),
        common::log(2),
        common::driver_limits(),
        CreditLimits::default(),
    );
    assert!(
        matches!(
            result,
            Err(CreditTransactionError::State(
                StateTransactionError::Uncertain
            ))
        ),
        "{result:?}"
    );
    assert_eq!(
        fault.load(Ordering::SeqCst),
        0,
        "reached delegated publication sync"
    );
    assert!(matches!(
        store.read_credit_owner(
            &common::scope()?,
            b"global",
            CreditLimits::default(),
            10_000,
            64 * common::MIB
        ),
        Err(StateTransactionError::Unavailable)
    ));
    drop(store);
    drop(common::open(&path)?);
    let recovered = f::cohort(&path)?;
    assert!(recovered == before || recovered == after);
    f::retained(&path, &frame, if recovered == before { 2 } else { 3 })?;
    assert_eq!(f::owner(&path)?.view().epoch, 2);
    assert_eq!(f::owner(&path)?.lease(0).ok_or("missing lease")?.in_use, 0);
    std::fs::remove_dir_all(dir)?;
    Ok(())
}

#[test]
fn credit_store_actual_shard_count_mismatch_is_an_authority_error() -> TestResult {
    let dir = common::directory("credit-actual-count")?;
    let path = dir.join("state");
    drop(f::create(&path)?);
    let scope = common::scope()?;
    let frame = encode_credit_request(
        &scope,
        &wf_types::identity::RequestId::new(b"mismatch".to_vec())?,
        common::clock(1000)?,
        IdentifiedCreditRequest {
            scope: b"global",
            shard_count: 8,
            configuration: Some(f::config(1, 1)),
            mutation: Mutation::Lease {
                shard_id: 7,
                amount: 1,
                ttl_ms: 1000,
            },
        },
        common::driver_limits().codec,
        16 * common::MIB,
    )?;
    let before = f::cohort(&path)?;
    let mut store = common::open(&path)?;
    assert!(matches!(
        apply_credit_transaction(
            &mut store,
            &scope,
            frame.as_bytes(),
            common::log(0),
            common::driver_limits(),
            CreditLimits::default()
        ),
        Err(CreditTransactionError::State(
            StateTransactionError::InvalidChanges
        ))
    ));
    drop(store);
    assert_eq!(f::cohort(&path)?, before);
    std::fs::remove_dir_all(dir)?;
    Ok(())
}

#[test]
fn credit_store_every_step_cut_aborts_the_complete_staged_release_cohort() -> TestResult {
    let dir = common::directory("credit-every-step")?;
    let path = dir.join("actual");
    let expected = dir.join("expected");
    f::seed(&path)?;
    f::seed(&expected)?;
    let before = f::cohort(&path)?;
    assert_eq!(f::cohort(&expected)?, before);
    let ids = vec![b"a".to_vec(), b"b".to_vec()];
    let frame = f::frame(
        b"release-cut",
        1002,
        None,
        Mutation::Release {
            shard_id: 0,
            amount: 2,
            reservation_ids: &ids,
        },
    )?;
    f::retained(&expected, &frame, 2)?;
    let after = f::cohort(&expected)?;
    assert_ne!(before, after);
    let mut success = None;
    for steps in 1..=256 {
        let mut limits = common::driver_limits();
        limits.maximum_read_steps = steps;
        let mut store = common::open(&path)?;
        let result = apply_credit_transaction(
            &mut store,
            &common::scope()?,
            frame.as_bytes(),
            common::log(2),
            limits,
            CreditLimits::default(),
        );
        drop(store);
        match result {
            Err(CreditTransactionError::State(StateTransactionError::Limit)) => {
                assert_eq!(
                    f::cohort(&path)?,
                    before,
                    "step cut {steps} published a prefix"
                );
            }
            Ok(wf_storage::transaction::RecordBatchTransactionOutcome::Retained {
                control,
                outcome,
            }) => {
                assert_eq!(control.position().get(), 2);
                assert_eq!(f::reply(&outcome)?.decision, Decision::Released);
                assert_eq!(f::cohort(&path)?, after);
                success = Some(steps);
                break;
            }
            other => return Err(format!("unexpected step cut {steps}: {other:?}").into()),
        }
    }
    assert!(
        success.is_some_and(|steps| steps > 12),
        "did not exercise late step cuts: {success:?}"
    );
    std::fs::remove_dir_all(dir)?;
    Ok(())
}
