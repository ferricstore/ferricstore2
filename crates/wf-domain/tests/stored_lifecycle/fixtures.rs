//! Independently admitted full donors; expected capture values remain test-owned.

use std::collections::BTreeMap;
use wf_domain::policy::complete::{
    CompletePolicyData, CompleteStatePolicyData, PolicySchemaCharge, PolicySchemaLimits,
    StoredLifecycleCaptureInput, StoredPolicyValue, decode_original_complete_policy_entry_portable,
    prepare_complete_policy_value_v2, restore_complete_policy, verify_complete_policy_value_v2,
};
use wf_domain::retention::RetentionOverride;
use wf_domain::workflow::active::{LifecycleDefaults, LifecycleOverrides, MaxActiveOverride};

pub(super) type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;

pub(super) fn input() -> StoredLifecycleCaptureInput<'static> {
    StoredLifecycleCaptureInput {
        workflow_type: b"r",
        initial_state: b"q",
        overrides: LifecycleOverrides {
            active: MaxActiveOverride::Inherit,
            retention: RetentionOverride::default(),
            history_hot_max_events: None,
        },
        defaults: LifecycleDefaults {
            ttl_ms: 7_000,
            history_hot_max_events: 2,
            history_max_events: 10,
            max_history_hot_max_events: 100,
            max_history_max_events: 1_000,
        },
    }
}

pub(super) fn policy_pair() -> TestResult<(StoredPolicyValue, StoredPolicyValue)> {
    let schema = PolicySchemaLimits::default();
    let mut charge = PolicySchemaCharge::new(schema);
    let data = CompletePolicyData {
        workflow_type: b"r".to_vec(),
        version: None,
        max_active_ms: Some(5_000),
        retry: None,
        retention: Some(RetentionOverride {
            ttl_ms: Some(11_000),
            history_max_events: Some(30),
        }),
        states: BTreeMap::from([(
            b"q".to_vec(),
            CompleteStatePolicyData {
                mode: None,
                retry: None,
                retention: Some(RetentionOverride {
                    ttl_ms: Some(22_000),
                    history_max_events: Some(40),
                }),
                governance: None,
            },
        )]),
        indexed_attributes: None,
        indexed_state_meta: None,
        governance: None,
    };
    let candidate = restore_complete_policy(7, data, schema, &mut charge)?;
    let complete = prepare_complete_policy_value_v2(candidate, schema, &mut charge)?;
    verify_complete_policy_value_v2(&complete, schema, &mut charge)?;
    assert_eq!(complete.generation(), 7);
    assert_eq!(complete.data().workflow_type, b"r");
    assert_eq!(complete.data().max_active_ms, Some(5_000));

    // Literal ETF: {flow_policy_v1,7,#{type => <<"r">>,max_active_ms => 5000,
    // retention => #{ttl_ms => 11000,history_max_events => 30},
    // states => #{<<"q">> => #{retention => #{ttl_ms => 22000,history_max_events => 40}}}}}.
    let raw = [
        b"\x83\x68\x03\x77\x0eflow_policy_v1\x61\x07\x74\x00\x00\x00\x04".as_slice(),
        b"\x77\x04type\x6d\x00\x00\x00\x01r",
        b"\x77\x0dmax_active_ms\x62\x00\x00\x13\x88",
        b"\x77\x09retention\x74\x00\x00\x00\x02",
        b"\x77\x06ttl_ms\x62\x00\x00\x2a\xf8",
        b"\x77\x12history_max_events\x61\x1e",
        b"\x77\x06states\x74\x00\x00\x00\x01\x6d\x00\x00\x00\x01q",
        b"\x74\x00\x00\x00\x01\x77\x09retention\x74\x00\x00\x00\x02",
        b"\x77\x06ttl_ms\x62\x00\x00\x55\xf0\x77\x12history_max_events\x61\x28",
    ]
    .concat();
    let original = decode_original_complete_policy_entry_portable(&raw, schema, &mut charge)?
        .ok_or("literal original lifecycle donor was not accepted")?;
    assert_eq!(original.generation(), 7);
    assert_eq!(original.workflow_type(), Some(b"r".as_slice()));
    assert_eq!(original.encoded(), raw);
    Ok((
        StoredPolicyValue::Complete(Box::new(complete)),
        StoredPolicyValue::Original(raw),
    ))
}
