use super::{fixtures as f, *};

#[test]
fn credit_limits_and_canonical_state_refuse_before_new_owner_growth() -> TestResult {
    let owner = f::grant(f::owner(b"scope", 2)?, 1, 1, 100, 1000)?;
    let short = CreditLimits::new(1, 100, 1_048_576, 1_048_576)?;
    assert_eq!(
        decide_credit(
            owner.clone(),
            Op::Grant {
                shard_id: 2,
                amount: 1,
                now: TimestampMillis::new(101),
                ttl_ms: 1000
            },
            short
        )
        .err(),
        Some(CreditError::Limit)
    );
    let short = CreditLimits::new(10, 1, 1_048_576, 1_048_576)?;
    let full = f::grant(owner.clone(), 1, 1, 101, 1000)?;
    assert_eq!(
        decide_credit(
            full,
            Op::Spend {
                shard_id: 1,
                amount: 2,
                now: TimestampMillis::new(102),
                ttl_ms: None,
                reservation_ids: &[b"a", b"b"]
            },
            short
        )
        .err(),
        Some(CreditError::Limit)
    );
    for short in [
        CreditLimits::new(10, 100, 1, 1_048_576)?,
        CreditLimits::new(10, 100, 1_048_576, 1)?,
    ] {
        assert_eq!(
            decide_credit(
                owner.clone(),
                Op::ReclaimExpired {
                    now: TimestampMillis::new(100)
                },
                short
            )
            .err(),
            Some(CreditError::Limit)
        );
    }
    for values in [[0, 1, 1, 1], [1, 0, 1, 1], [1, 1, 0, 1], [1, 1, 1, 0]] {
        assert_eq!(
            CreditLimits::new(values[0], values[1], values[2], u64::try_from(values[3])?).err(),
            Some(CreditError::InvalidLimits)
        );
    }
    let mut data = owner.clone().into_data();
    data.leases.push(data.leases[0].clone());
    assert_eq!(
        restore_credit_owner(data, CreditLimits::default()).err(),
        Some(CreditError::InvalidState)
    );
    let mut data = owner.clone().into_data();
    data.free += 1;
    assert_eq!(
        restore_credit_owner(data, CreditLimits::default()).err(),
        Some(CreditError::InvalidState)
    );
    let mut scope = Vec::with_capacity(4096);
    scope.extend_from_slice(b"scope");
    let mut data = owner.into_data();
    data.scope = scope;
    assert_eq!(
        restore_credit_owner(data, CreditLimits::new(10, 10, 1024, 1_048_576)?).err(),
        Some(CreditError::Limit)
    );
    Ok(())
}
