//! Full current backing is required before resolving running Claim rules.

use super::{TestResult, data, limits};
use std::collections::BTreeMap;
use wf_domain::policy::complete::{
    CompleteStatePolicyData, LimitData, PolicySchemaCharge, StoredPolicyValue,
    resolve_stored_running_claim_policy as resolve,
};
use wf_domain::policy::definition::PolicyVersion;
use wf_domain::workflow::active::LimitEnforcement as Enforcement;

mod fixtures;
mod original;
mod refusals;
use fixtures::{governance, typed};

#[test]
fn typed_current_merges_all_states_without_losing_zero_or_original_label() -> TestResult {
    let mut value = data(
        b"job".to_vec(),
        Some(PolicyVersion::Bytes(b"v\0x".to_vec())),
        BTreeMap::new(),
    );
    value.governance = Some(governance(LimitData {
        limit: 8,
        lease_size: Some(7),
        enforcement: Some(Enforcement::ApproximateGlobal),
    }));
    value.states.insert(
        b"a".to_vec(),
        CompleteStatePolicyData {
            mode: None,
            retry: None,
            retention: None,
            governance: Some(governance(LimitData {
                limit: 0,
                lease_size: None,
                enforcement: Some(Enforcement::StrictGlobal),
            })),
        },
    );
    value.states.insert(
        b"b".to_vec(),
        CompleteStatePolicyData {
            mode: None,
            retry: None,
            retention: None,
            governance: Some(governance(LimitData {
                limit: 3,
                lease_size: Some(2),
                enforcement: None,
            })),
        },
    );
    let stored = typed(value, 9)?;
    let schema = limits(128 * 1024 * 1024)?;
    let mut charge = PolicySchemaCharge::new(schema);
    let rule = resolve(&stored, b"job", None, 4, schema, &mut charge)?.ok_or("rule")?;
    assert_eq!(
        (rule.limit(), rule.lease_size(), rule.config_version()),
        (0, 2, Some(9))
    );
    assert!(rule.enforcement() == Enforcement::StrictGlobal);
    let StoredPolicyValue::Complete(value) = &stored else {
        return Err("typed owner".into());
    };
    let Some(PolicyVersion::Bytes(label)) = &value.data().version else {
        return Err("label".into());
    };
    let Some(wf_domain::policy::complete::RunningClaimPolicyVersion::Bytes(actual)) =
        rule.policy_version()
    else {
        return Err("borrowed label".into());
    };
    assert!(std::ptr::eq(actual.as_ptr(), label.as_ptr()));
    let only: [&[u8]; 1] = [b"b"];
    let inherited =
        resolve(&stored, b"job", Some(&only), 4, schema, &mut charge)?.ok_or("state rule")?;
    assert_eq!((inherited.limit(), inherited.lease_size()), (3, 2));
    assert!(inherited.enforcement() == Enforcement::ApproximateGlobal);
    Ok(())
}

#[test]
fn valid_complete_policy_without_running_rule_is_ungoverned() -> TestResult {
    let stored = typed(data(b"job".to_vec(), None, BTreeMap::new()), 1)?;
    let schema = limits(128 * 1024 * 1024)?;
    let mut charge = PolicySchemaCharge::new(schema);
    assert!(resolve(&stored, b"job", None, 2, schema, &mut charge)?.is_none());
    // The credit service's 1000-ID cap is only reached for governed work.
    assert!(resolve(&stored, b"job", None, 5000, schema, &mut charge)?.is_none());
    assert!(charge.g() > 0);
    Ok(())
}

#[test]
fn any_state_resolution_fits_one_bounded_account_for_a_large_current_catalog() -> TestResult {
    let mut value = data(b"job".to_vec(), None, BTreeMap::new());
    value.governance = Some(governance(LimitData {
        limit: 8,
        lease_size: Some(7),
        enforcement: Some(Enforcement::ApproximateGlobal),
    }));
    for ordinal in 0..512 {
        value.states.insert(
            format!("state-{ordinal}").into_bytes(),
            CompleteStatePolicyData {
                mode: None,
                retry: None,
                retention: None,
                governance: (ordinal == 255).then(|| {
                    governance(LimitData {
                        limit: 0,
                        lease_size: Some(2),
                        enforcement: Some(Enforcement::StrictGlobal),
                    })
                }),
            },
        );
    }
    let stored = typed(value, 9)?;
    let schema = limits(16 * 1024 * 1024)?;
    let mut charge = PolicySchemaCharge::new(schema);
    let rule = resolve(&stored, b"job", None, 4, schema, &mut charge)?.ok_or("large rule")?;
    assert_eq!((rule.limit(), rule.lease_size()), (0, 2));
    assert!(rule.enforcement() == Enforcement::StrictGlobal);
    assert!(charge.total() <= schema.max_work_bytes());
    Ok(())
}
