use super::{TestResult, caller, policy_value};
use std::collections::BTreeMap;
use wf_domain::claim::caller::{ClaimDueInput, ClaimStatesInput};
use wf_domain::claim::coordination::{
    ClaimCoordinationError, ClaimCoordinationLimits, RunningCreditDemandKind, RunningLimitRule,
    admit_running_projection, resolve_running_credit, running_config_version,
};
use wf_domain::policy::definition::{PolicyVersion, StatePolicy};
use wf_domain::policy::installation::PolicyLimits;
use wf_domain::workflow::MAX_EXACT_MILLIS;
use wf_domain::workflow::active::LimitEnforcement;
use wf_types::version::TimestampMillis;

#[test]
fn running_rules_merge_and_take_strictest() -> TestResult {
    let states = BTreeMap::from([
        (b"a".to_vec(), StatePolicy::default()),
        (b"b".to_vec(), StatePolicy::default()),
    ]);
    let value = policy_value(b"job".to_vec(), None, states)?;
    let base = RunningLimitRule::new(4, Some(LimitEnforcement::ApproximateGlobal), Some(9))?;
    let rules = BTreeMap::from([
        (
            b"a".to_vec(),
            Some(RunningLimitRule::new(
                1,
                Some(LimitEnforcement::StrictGlobal),
                None,
            )?),
        ),
        (
            b"b".to_vec(),
            Some(RunningLimitRule::new(2, None, Some(3))?),
        ),
    ]);
    let projection = admit_running_projection(
        &value,
        Some(base),
        &rules,
        PolicyLimits::default(),
        ClaimCoordinationLimits::default(),
    )?;
    let mut input = ClaimDueInput::new(b"job", b"worker");
    input.limit = Some(2);
    input.lease_ms = Some(7);
    input.requested_now_ms = Some(u64::MAX);
    let call = caller(input)?;
    let demand = resolve_running_credit(
        &call,
        Some(&projection),
        TimestampMillis::new(100),
        4,
        PolicyLimits::default(),
        ClaimCoordinationLimits::default(),
    )?
    .ok_or("missing running demand")?;
    assert_eq!(
        (
            demand.amount(),
            demand.limit(),
            demand.lease_size(),
            demand.ttl_ms()
        ),
        (2, 1, 3, 7)
    );
    assert!(demand.enforcement() == LimitEnforcement::StrictGlobal);
    assert!(demand.auto_lease());
    assert_eq!(demand.trusted_now().get(), 100);
    assert_eq!(demand.config_version(), Some(1));
    assert!(
        matches!(demand.kind(), RunningCreditDemandKind::PolicyDerived { worker, .. } if std::ptr::eq(worker, call.worker()))
    );
    let names: [&[u8]; 1] = [b"b"];
    input.states = ClaimStatesInput::Many(&names);
    let only = caller(input)?;
    let inherited = resolve_running_credit(
        &only,
        Some(&projection),
        TimestampMillis::new(0),
        4,
        PolicyLimits::default(),
        ClaimCoordinationLimits::default(),
    )?
    .ok_or("missing inherited demand")?;
    assert!(inherited.enforcement() == LimitEnforcement::ApproximateGlobal);
    assert_eq!((inherited.limit(), inherited.lease_size()), (2, 3));
    assert!(RunningLimitRule::new(0, None, None).is_ok());
    assert!(RunningLimitRule::new(1, None, Some(0)).is_err());
    Ok(())
}

#[test]
fn any_filter_checks_base_and_all_configured_states() -> TestResult {
    let value = policy_value(
        b"job".to_vec(),
        None,
        BTreeMap::from([
            (b"a".to_vec(), StatePolicy::default()),
            (b"b".to_vec(), StatePolicy::default()),
        ]),
    )?;
    let rules = BTreeMap::from([
        (b"a".to_vec(), None),
        (b"b".to_vec(), Some(RunningLimitRule::new(0, None, None)?)),
    ]);
    let base = RunningLimitRule::new(5, None, None)?;
    let projection = admit_running_projection(
        &value,
        Some(base),
        &rules,
        PolicyLimits::default(),
        ClaimCoordinationLimits::default(),
    )?;
    assert!(std::ptr::eq(projection.installed(), &raw const value));
    assert!(std::ptr::eq(projection.states(), &raw const rules));
    assert!(projection.type_rule() == Some(base));
    let call = caller(ClaimDueInput::new(b"job", b"w"))?;
    assert_eq!(
        resolve_running_credit(
            &call,
            Some(&projection),
            TimestampMillis::new(0),
            4,
            PolicyLimits::default(),
            ClaimCoordinationLimits::default()
        )?
        .ok_or("missing any rule")?
        .limit(),
        0
    );
    let mut input = ClaimDueInput::new(b"job", b"w");
    input.states = ClaimStatesInput::One(b"missing");
    let missing = caller(input)?;
    assert_eq!(
        resolve_running_credit(
            &missing,
            Some(&projection),
            TimestampMillis::new(0),
            4,
            PolicyLimits::default(),
            ClaimCoordinationLimits::default()
        )?
        .ok_or("missing inherited type")?
        .limit(),
        5
    );
    assert!(matches!(
        resolve_running_credit(
            &call,
            Some(&projection),
            TimestampMillis::new(0),
            4,
            PolicyLimits::new(1)?,
            ClaimCoordinationLimits::default()
        ),
        Err(ClaimCoordinationError::InvalidPolicyAuthority)
    ));
    let no_rules = BTreeMap::from([(b"a".to_vec(), None), (b"b".to_vec(), None)]);
    let empty = admit_running_projection(
        &value,
        None,
        &no_rules,
        PolicyLimits::default(),
        ClaimCoordinationLimits::default(),
    )?;
    let foreign = caller(ClaimDueInput::new(b"other", b"w"))?;
    assert!(matches!(
        resolve_running_credit(
            &foreign,
            Some(&empty),
            TimestampMillis::new(0),
            4,
            PolicyLimits::default(),
            ClaimCoordinationLimits::default()
        ),
        Err(ClaimCoordinationError::InvalidPolicyAuthority)
    ));
    assert!(matches!(
        admit_running_projection(
            &value,
            Some(base),
            &BTreeMap::new(),
            PolicyLimits::default(),
            ClaimCoordinationLimits::default()
        ),
        Err(ClaimCoordinationError::InvalidPolicyAuthority)
    ));
    Ok(())
}

#[test]
fn explicit_pairs_and_policy_versions_keep_boundaries() -> TestResult {
    let value = policy_value(b"other".to_vec(), None, BTreeMap::new())?;
    let rules = BTreeMap::new();
    let projection = admit_running_projection(
        &value,
        None,
        &rules,
        PolicyLimits::default(),
        ClaimCoordinationLimits::default(),
    )?;
    let mut input = ClaimDueInput::new(b"job", b"w");
    input.limit = Some(2);
    input.lease_ms = Some(1);
    input.governance_limit_scope = Some(b"scope");
    input.governance_shard_id = Some(0);
    let call = caller(input)?;
    let demand = resolve_running_credit(
        &call,
        Some(&projection),
        TimestampMillis::new(MAX_EXACT_MILLIS - 1),
        4,
        PolicyLimits::default(),
        ClaimCoordinationLimits::default(),
    )?
    .ok_or("missing explicit demand")?;
    assert!(!demand.auto_lease());
    assert!(demand.enforcement() == LimitEnforcement::ApproximateGlobal);
    assert_eq!(
        (demand.amount(), demand.lease_size(), demand.ttl_ms()),
        (2, 2, 1)
    );
    assert!(
        matches!(demand.kind(), RunningCreditDemandKind::Explicit { scope, shard_id: 0 } if std::ptr::eq(scope, call.governance_limit_scope().ok_or("missing scope")?))
    );
    assert!(matches!(
        resolve_running_credit(
            &call,
            None,
            TimestampMillis::new(MAX_EXACT_MILLIS),
            4,
            PolicyLimits::default(),
            ClaimCoordinationLimits::default()
        ),
        Err(ClaimCoordinationError::DeadlineOverflow)
    ));
    assert!(matches!(
        resolve_running_credit(
            &call,
            None,
            TimestampMillis::new(u64::MAX),
            4,
            PolicyLimits::default(),
            ClaimCoordinationLimits::default()
        ),
        Err(ClaimCoordinationError::InvalidClock)
    ));
    input.governance_shard_id = None;
    assert!(matches!(
        resolve_running_credit(
            &caller(input)?,
            None,
            TimestampMillis::new(0),
            4,
            PolicyLimits::default(),
            ClaimCoordinationLimits::default()
        ),
        Err(ClaimCoordinationError::InvalidGovernancePair)
    ));
    input.governance_shard_id = Some(4);
    assert!(matches!(
        resolve_running_credit(
            &caller(input)?,
            None,
            TimestampMillis::new(0),
            4,
            PolicyLimits::default(),
            ClaimCoordinationLimits::default()
        ),
        Err(ClaimCoordinationError::InvalidGovernanceShard)
    ));
    assert_eq!(
        running_config_version(0, Some(&PolicyVersion::Number(7))),
        Some(7)
    );
    assert_eq!(
        running_config_version(1, Some(&PolicyVersion::Number(7))),
        Some(1)
    );
    assert_eq!(
        running_config_version(0, Some(&PolicyVersion::Bytes(b"v".to_vec()))),
        None
    );
    Ok(())
}

#[test]
fn projection_admits_before_large_names_or_state_work() -> TestResult {
    let value = policy_value(
        b"job".to_vec(),
        None,
        BTreeMap::from([(b"a".to_vec(), StatePolicy::default())]),
    )?;
    let rules = BTreeMap::from([(b"a".to_vec(), None)]);
    assert!(
        admit_running_projection(
            &value,
            None,
            &rules,
            PolicyLimits::new(1)?,
            ClaimCoordinationLimits::new(1, 1, 267)?
        )
        .is_ok()
    );
    assert!(matches!(
        admit_running_projection(
            &value,
            None,
            &rules,
            PolicyLimits::new(1)?,
            ClaimCoordinationLimits::new(1, 1, 266)?
        ),
        Err(ClaimCoordinationError::WorkBudget)
    ));
    let projection = admit_running_projection(
        &value,
        None,
        &rules,
        PolicyLimits::default(),
        ClaimCoordinationLimits::default(),
    )?;
    let call = caller(ClaimDueInput::new(b"job", b"w"))?;
    assert!(
        resolve_running_credit(
            &call,
            Some(&projection),
            TimestampMillis::new(0),
            4,
            PolicyLimits::default(),
            ClaimCoordinationLimits::new(1, 1, 1430)?
        )?
        .is_none()
    );
    assert!(matches!(
        resolve_running_credit(
            &call,
            Some(&projection),
            TimestampMillis::new(0),
            4,
            PolicyLimits::default(),
            ClaimCoordinationLimits::new(1, 1, 1429)?
        ),
        Err(ClaimCoordinationError::WorkBudget)
    ));
    let large = vec![b'x'; 262_145];
    let mismatched = BTreeMap::from([(large, None)]);
    assert!(matches!(
        admit_running_projection(
            &value,
            None,
            &mismatched,
            PolicyLimits::default(),
            ClaimCoordinationLimits::new(1, 1, 1024)?
        ),
        Err(ClaimCoordinationError::WorkBudget)
    ));
    let tiny = ClaimCoordinationLimits::new(1, 1, 3)?;
    assert!(matches!(
        resolve_running_credit(
            &call,
            None,
            TimestampMillis::new(0),
            4,
            PolicyLimits::default(),
            tiny
        ),
        Err(ClaimCoordinationError::WorkBudget)
    ));
    Ok(())
}
