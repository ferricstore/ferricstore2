//! Public credit option recipes from pinned `LimitStore` callers.
use super::TestResult;
use wf_domain::governance::caller::{
    CreditCallAction as A, CreditCallKind as K, CreditCallerError as E, CreditIdList as Ids,
    CreditOptionValue as V, CreditPolicyInput as P, NormalizedCreditCall,
    normalize_credit_call as normalize,
};
const MAX: u64 = 9_007_199_254_740_991;
#[test]
fn public_credit_caller_preserves_all_six_operations_and_borrowed_release_ids() -> TestResult {
    let base = [
        ("shard_id", V::Integer(0)),
        ("amount", V::Integer(2)),
        ("ttl_ms", V::Integer(1000)),
        ("limit", V::Integer(2)),
        ("now_ms", V::Integer(1000)),
    ];
    let call = normalize(K::Lease, b"scope", 4, &base, 65535)?;
    assert_eq!(call.scope(), b"scope");
    assert_eq!(call.now_ms(), Some(1000));
    assert!(matches!(
        call.action(),
        A::Lease {
            amount: 2,
            ttl_ms: 1000,
            ..
        }
    ));
    let call = normalize(K::Spend, b"scope", 4, &base, 65535)?;
    assert!(matches!(
        call.action(),
        A::Spend {
            amount: 2,
            ttl_ms: Some(1000),
            ..
        }
    ));
    let ids = vec![b"second".to_vec(), b"first".to_vec()];
    let call = normalize(
        K::Release,
        b"scope",
        4,
        &[
            ("shard_id", V::Integer(0)),
            ("reservation_ids", V::Ids(Ids::Owned(&ids))),
        ],
        65535,
    )?;
    let A::Release {
        amount,
        reservation_ids,
        ..
    } = call.action()
    else {
        return Err("wrong release".into());
    };
    assert_eq!(amount, 2);
    assert_eq!(
        reservation_ids.iter().collect::<Vec<_>>(),
        vec![b"second".as_slice(), b"first".as_slice()]
    );
    assert_eq!(
        reservation_ids.iter().next().ok_or("no IDs")?.as_ptr(),
        ids[0].as_ptr()
    );
    assert_eq!(
        normalize(K::Renew, b"scope", 4, &base, 65535)?
            .action()
            .kind(),
        K::Renew
    );
    assert_eq!(normalize(K::Get, b"scope", 4, &[], 65535)?.action(), A::Get);
    assert_eq!(
        normalize(K::Cleanup, b"scope", 4, &[], 65535)?.action(),
        A::Cleanup
    );
    Ok(())
}
#[test]
fn public_credit_caller_numeric_policy_falls_back_and_binary_policy_requires_version() -> TestResult
{
    let mut opts = vec![
        ("shard_id", V::Integer(0)),
        ("amount", V::Integer(1)),
        ("ttl_ms", V::Integer(1)),
        ("limit", V::Integer(1)),
        ("policy_version", V::Integer(7)),
    ];
    let A::Lease {
        configuration: Some(config),
        ..
    } = normalize(K::Lease, b"scope", 4, &opts, 65535)?.action()
    else {
        return Err("missing configuration".into());
    };
    assert_eq!(config.config_version, Some(7));
    assert_eq!(config.policy_version, Some(P::Number(7)));
    opts[4].1 = V::Bytes(b"policy-a");
    assert_eq!(
        normalize(K::Lease, b"scope", 4, &opts, 65535).err(),
        Some(E::PolicyVersionRequiresConfig)
    );
    opts.push(("config_version", V::Integer(1)));
    let A::Lease {
        configuration: Some(config),
        ..
    } = normalize(K::Lease, b"scope", 4, &opts, 65535)?.action()
    else {
        return Err("missing binary configuration".into());
    };
    assert_eq!(config.config_version, Some(1));
    assert_eq!(config.policy_version, Some(P::Bytes(b"policy-a")));
    Ok(())
}
#[test]
fn public_credit_caller_enforces_source_shard_time_and_version_bounds() {
    for shard in 0..4 {
        let opts = [
            ("shard_id", V::Integer(shard)),
            ("amount", V::Integer(1)),
            ("ttl_ms", V::Integer(1)),
            ("now_ms", V::Integer(0)),
        ];
        assert!(normalize(K::Lease, b"scope", 4, &opts, 65535).is_ok());
    }
    let mut opts = vec![
        ("shard_id", V::Integer(4)),
        ("amount", V::Integer(1)),
        ("ttl_ms", V::Integer(1)),
        ("now_ms", V::Integer(0)),
    ];
    assert_eq!(
        normalize(K::Lease, b"scope", 4, &opts, 65535).err(),
        Some(E::Shard { maximum: 3 })
    );
    opts[0].1 = V::Integer(0);
    opts[3].1 = V::Integer(i128::from(MAX));
    assert_eq!(
        normalize(K::Lease, b"scope", 4, &opts, 65535).err(),
        Some(E::Deadline)
    );
    opts[3].1 = V::Integer(0);
    opts.push(("limit", V::Integer(i128::from(MAX) + 1)));
    assert_eq!(
        normalize(K::Lease, b"scope", 4, &opts, 65535).err(),
        Some(E::InvalidField("limit"))
    );
}
#[test]
fn public_credit_caller_forbids_supplied_spend_ids_and_requires_exact_release_cardinality() {
    let ids = vec![b"a".to_vec(), b"b".to_vec()];
    let opts = [
        ("shard_id", V::Integer(0)),
        ("amount", V::Integer(2)),
        ("reservation_ids", V::Ids(Ids::Owned(&ids))),
    ];
    assert_eq!(
        normalize(K::Spend, b"scope", 4, &opts, 65535).err(),
        Some(E::ReservationIdsNotAllowed)
    );
    assert!(normalize(K::Release, b"scope", 4, &opts, 65535).is_ok());
    let duplicate = vec![b"a".to_vec(), b"a".to_vec()];
    let opts = [
        ("shard_id", V::Integer(0)),
        ("reservation_ids", V::Ids(Ids::Owned(&duplicate))),
    ];
    assert_eq!(
        normalize(K::Release, b"scope", 4, &opts, 65535).err(),
        Some(E::ReservationIds)
    );
}
#[test]
fn public_credit_caller_first_keyword_wins_but_shadowed_values_still_pay_raw_bytes() {
    let bytes = vec![0; 100];
    let opts = [
        ("shard_id", V::Integer(0)),
        ("amount", V::Integer(1)),
        ("ttl_ms", V::Integer(1)),
        ("amount", V::Integer(-1)),
        ("ignored", V::Bytes(&bytes)),
    ];
    assert!(normalize(K::Lease, b"s", 4, &opts, 1000).is_ok());
    assert_eq!(
        normalize(K::Lease, b"s", 4, &opts, 100).err(),
        Some(E::Limit)
    );
    assert!(normalize(K::Get, b"s", 4, &[("ttl_ms", V::Nil)], 1000).is_ok());
    assert_eq!(
        normalize(K::Lease, b"s", 4, &[("shard_id", V::Nil)], 1000).err(),
        Some(E::Shard { maximum: 3 })
    );
}
#[test]
fn public_credit_caller_typed_restore_cannot_bypass_configuration_or_deadline_validation()
-> TestResult {
    let restored = NormalizedCreditCall::from_parts(
        b"scope",
        4,
        Some(1000),
        A::Renew {
            shard_id: 3,
            ttl_ms: 1,
        },
    )?;
    assert_eq!(restored.shard_count(), 4);
    assert_eq!(
        NormalizedCreditCall::from_parts(
            b"scope",
            4,
            Some(MAX),
            A::Renew {
                shard_id: 0,
                ttl_ms: 1
            }
        )
        .err(),
        Some(E::Deadline)
    );
    Ok(())
}

#[test]
fn public_credit_caller_preserves_source_scalar_and_release_error_order() {
    let spend = [
        ("shard_id", V::Integer(0)),
        ("amount", V::Integer(1)),
        ("now_ms", V::Integer(-1)),
        ("ttl_ms", V::Integer(-1)),
    ];
    assert_eq!(
        normalize(K::Spend, b"scope", 4, &spend, 65535).err(),
        Some(E::InvalidField("now_ms"))
    );
    let release = [
        ("shard_id", V::Integer(0)),
        ("reservation_ids", V::Nil),
        ("now_ms", V::Integer(-1)),
    ];
    assert_eq!(
        normalize(K::Release, b"scope", 4, &release, 65535).err(),
        Some(E::ReservationIds)
    );
}

#[test]
fn public_credit_caller_duplicate_release_ids_precede_invalid_clock() {
    let ids = vec![b"a".to_vec(), b"a".to_vec()];
    let release = [
        ("shard_id", V::Integer(0)),
        ("reservation_ids", V::Ids(Ids::Owned(&ids))),
        ("now_ms", V::Integer(-1)),
    ];
    assert_eq!(
        normalize(K::Release, b"scope", 4, &release, 65535).err(),
        Some(E::ReservationIds)
    );
}
