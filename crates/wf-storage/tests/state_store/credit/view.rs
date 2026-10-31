use super::{fixtures as f, *};
use std::sync::{
    Arc,
    atomic::{AtomicU8, Ordering},
};
use wf_domain::governance::view::CreditReplyView;
use wf_ports::log::{CodecError, prepare_credit_request, with_public_credit_reply};
use wf_ports::state::StateTransactionError;

#[test]
fn credit_public_reply_retains_original_spend_after_release_reopen_and_exact_replay() -> TestResult
{
    let dir = common::directory("credit-public-retained")?;
    let path = dir.join("state");
    f::seed(&path)?;
    let ids = vec![b"a".to_vec(), b"b".to_vec()];
    let original = f::frame(
        b"spend",
        1001,
        None,
        Mutation::Spend {
            shard_id: 0,
            lease_epoch: 1,
            amount: 2,
            ttl_ms: None,
            reservation_ids: &ids,
        },
    )?;
    let outcome = f::retained(&path, &original, 2)?;
    with_public_credit_reply(
        outcome.response(),
        CreditLimits::default(),
        16 * common::MIB,
        32 * common::MIB,
        |view| -> TestResult {
            let CreditReplyView::Spent {
                owner,
                lease,
                reservation_ids,
            } = view
            else {
                return Err("expected original public spend".into());
            };
            assert_eq!((owner.limit, owner.free, owner.epoch), (3, 0, 1));
            assert_eq!((lease.epoch, lease.available, lease.in_use), (1, 1, 2));
            assert_eq!(reservation_ids, ids);
            Ok(())
        },
    )??;
    f::execute(
        &path,
        b"release",
        (3, 1002),
        None,
        Mutation::Release {
            shard_id: 0,
            amount: 2,
            reservation_ids: &ids,
        },
    )?;
    assert_eq!(f::owner(&path)?.view().epoch, 2);
    let replay = f::retained(&path, &original, 4)?;
    assert_eq!(replay.response(), outcome.response());
    with_public_credit_reply(
        replay.response(),
        CreditLimits::default(),
        16 * common::MIB,
        32 * common::MIB,
        |view| -> TestResult {
            let CreditReplyView::Spent {
                owner,
                lease,
                reservation_ids,
            } = view
            else {
                return Err("expected retained original public spend".into());
            };
            assert_eq!((owner.epoch, lease.epoch, lease.in_use), (1, 1, 2));
            assert_eq!(reservation_ids, ids);
            Ok(())
        },
    )??;
    assert_eq!(f::owner(&path)?.view().epoch, 2);
    std::fs::remove_dir_all(dir)?;
    Ok(())
}

#[test]
fn credit_unknown_actual_spend_preserves_original_binary_ids_and_complete_recovery() -> TestResult {
    let dir = common::directory("credit-public-unknown")?;
    let path = dir.join("fault");
    let expected = dir.join("expected");
    f::seed(&path)?;
    f::seed(&expected)?;
    let before = f::cohort(&path)?;
    assert_eq!(f::cohort(&expected)?, before);
    let ids = vec![b"\0original\xff".to_vec()];
    let frame = f::frame(
        b"uncertain-spend",
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
    let prepared = prepare_credit_request(
        frame.as_bytes(),
        common::driver_limits().codec,
        16 * common::MIB,
    )?;
    f::retained(&expected, &frame, 2)?;
    let after = f::cohort(&expected)?;
    assert_ne!(before, after);
    let fault = Arc::new(AtomicU8::new(0));
    let mut store = crate::faults::wrapped(&path, Arc::clone(&fault))?;
    fault.store(4, Ordering::SeqCst);
    let result = apply_credit_transaction(
        &mut store,
        &common::scope()?,
        frame.as_bytes(),
        common::log(2),
        common::driver_limits(),
        CreditLimits::default(),
    );
    let error = result.err().ok_or("expected uncertainty")?;
    assert_eq!(
        error,
        CreditTransactionError::State(StateTransactionError::Uncertain)
    );
    assert_eq!(fault.load(Ordering::SeqCst), 0);
    let context = error
        .unknown_spend(&prepared)
        .ok_or("missing original compensation context")?;
    assert_eq!(context.code, "FLOW_LIMIT_SPEND_UNKNOWN_OUTCOME");
    assert_eq!((context.scope, context.shard_id), (b"global".as_slice(), 0));
    assert_eq!(context.reservation_ids, ids);
    let Mutation::Spend {
        reservation_ids, ..
    } = prepared.command().mutation
    else {
        return Err("expected prepared spend".into());
    };
    assert_eq!(context.reservation_ids.as_ptr(), reservation_ids.as_ptr());
    assert_eq!(
        context.message,
        "Flow limit spend outcome is unknown; release these reservation_ids before retrying"
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
    assert_eq!(
        f::owner(&path)?.lease(0).ok_or("missing lease")?.in_use,
        if recovered == before { 2 } else { 3 }
    );
    std::fs::remove_dir_all(dir)?;
    Ok(())
}

#[test]
fn credit_unknown_mapping_excludes_known_failures_and_non_spend_operations() -> TestResult {
    let ids = vec![b"original".to_vec()];
    let frame = f::frame(
        b"spend",
        1001,
        None,
        Mutation::Spend {
            shard_id: 0,
            lease_epoch: 1,
            amount: 1,
            ttl_ms: None,
            reservation_ids: &ids,
        },
    )?;
    let prepared = prepare_credit_request(
        frame.as_bytes(),
        common::driver_limits().codec,
        16 * common::MIB,
    )?;
    let unknown = CreditTransactionError::State(StateTransactionError::Uncertain);
    assert!(unknown.unknown_spend(&prepared).is_some());
    for error in [
        CreditTransactionError::State(StateTransactionError::Unavailable),
        CreditTransactionError::State(StateTransactionError::Limit),
        CreditTransactionError::State(StateTransactionError::Corrupt),
        CreditTransactionError::Codec(CodecError::Malformed),
        CreditTransactionError::Credit(wf_domain::governance::credit::CreditError::Limit),
        CreditTransactionError::InvalidLimits,
    ] {
        assert!(error.unknown_spend(&prepared).is_none());
    }
    for operation in [
        Mutation::Get,
        Mutation::Cleanup,
        Mutation::Release {
            shard_id: 0,
            amount: 1,
            reservation_ids: &ids,
        },
        Mutation::Lease {
            shard_id: 0,
            amount: 1,
            ttl_ms: 1000,
        },
        Mutation::Renew {
            shard_id: 0,
            ttl_ms: 1000,
        },
    ] {
        let frame = f::frame(b"not-spend", 1001, None, operation)?;
        let prepared = prepare_credit_request(
            frame.as_bytes(),
            common::driver_limits().codec,
            16 * common::MIB,
        )?;
        assert!(unknown.unknown_spend(&prepared).is_none());
    }
    Ok(())
}
