use super::*;

pub(super) fn owner(scope: &[u8], limit: u64) -> TestResult<CreditOwner> {
    configured(scope, limit, 0, None)
}

pub(super) fn configured(
    scope: &[u8],
    limit: u64,
    version: u64,
    fingerprint: Option<PreparedDigest>,
) -> TestResult<CreditOwner> {
    let result = new_credit_owner(
        scope.to_vec(),
        limit,
        version,
        fingerprint,
        CreditLimits::default(),
    );
    assert!(
        result.is_ok(),
        "healthy credit owner creation: {:?}",
        result.as_ref().err()
    );
    Ok(result?)
}

pub(super) fn apply(
    owner: CreditOwner,
    operation: Op<'_>,
) -> TestResult<(CreditOwner, CreditDecision)> {
    let result = decide_credit(owner, operation, CreditLimits::default());
    assert!(
        result.is_ok(),
        "healthy credit owner decision: {:?}",
        result.as_ref().err()
    );
    Ok(result?.into_parts())
}

pub(super) fn grant(
    owner: CreditOwner,
    shard: u64,
    amount: u64,
    now: u64,
    ttl: u64,
) -> TestResult<CreditOwner> {
    let (owner, result) = apply(
        owner,
        Op::Grant {
            shard_id: shard,
            amount,
            now: TimestampMillis::new(now),
            ttl_ms: ttl,
        },
    )?;
    assert_eq!(result, CreditDecision::Granted { shard_id: shard });
    Ok(owner)
}

pub(super) fn spend(
    owner: CreditOwner,
    shard: u64,
    amount: u64,
    now: u64,
    ids: &[&[u8]],
) -> TestResult<CreditOwner> {
    let (owner, result) = apply(
        owner,
        Op::Spend {
            shard_id: shard,
            amount,
            now: TimestampMillis::new(now),
            ttl_ms: None,
            reservation_ids: ids,
        },
    )?;
    assert_eq!(result, CreditDecision::Spent { shard_id: shard });
    Ok(owner)
}

pub(super) fn release(owner: CreditOwner, shard: u64, ids: &[&[u8]]) -> TestResult<CreditOwner> {
    let (owner, result) = apply(
        owner,
        Op::Release {
            shard_id: shard,
            amount: u64::try_from(ids.len())?,
            reservation_ids: Some(ids),
        },
    )?;
    assert_eq!(result, CreditDecision::Released);
    Ok(owner)
}

pub(super) fn lease(owner: &CreditOwner, shard: u64) -> TestResult<&CreditLeaseData> {
    Ok(owner.lease(shard).ok_or("credit lease")?)
}

pub(super) fn fingerprint(version: u8) -> TestResult<PreparedDigest> {
    // Independent SHA256 of ETF binary policy-v1/v2/v3/stale-policy/conflicting-policy.
    let hex = match version {
        1 => "9a136555d9ec4ba128da41b9debcb4ae9b90adac99437ee63d9f23b1ac33121d",
        2 => "2b57577e06597b823ca15b886c7b9a3d74a716ca09057bc0e1178378091612ef",
        3 => "afd37390a2a9e9d60d624d33abb597f25a67e9e410962d8b0d9001fc975fa248",
        4 => "53cca4d44da8098d9ad88077aa488dca4158f56ab67c1590075c93805946c217",
        5 => "185a91880295bd27f89604669de292bf634587d629119fd96c0dbecda2185b40",
        _ => return Err("unknown literal fingerprint".into()),
    };
    let mut bytes = [0; 32];
    for (out, pair) in bytes.iter_mut().zip(hex.as_bytes().as_chunks::<2>().0) {
        *out = u8::from_str_radix(std::str::from_utf8(pair)?, 16)?;
    }
    Ok(PreparedDigest::new(bytes))
}

pub(super) fn configure(
    owner: CreditOwner,
    limit: u64,
    version: u64,
    fingerprint: u8,
) -> TestResult<CreditOwner> {
    let (owner, result) = apply(
        owner,
        Op::Configure {
            limit,
            config_version: version,
            policy_fingerprint: Some(self::fingerprint(fingerprint)?),
        },
    )?;
    assert_eq!(result, CreditDecision::Configured);
    Ok(owner)
}
