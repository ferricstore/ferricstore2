//! Separate producer assertions; a rejecting encoder is never a setup error.

use wf_domain::policy::complete::StoredPolicyValue;
use wf_domain::retention::RetentionOverride;
use wf_domain::workflow::active::{LifecycleOverrides, MaxActiveOverride, capture_lifecycle};
use wf_domain::workflow::caller::PublicMutationEvidence;
use wf_domain::workflow::{Cancel, Create, Operation, StartAndClaim};
use wf_ports::consensus::{
    PreparedOperation, PreparedWorkflowValueBatch, WorkflowValueTargetGuard,
};
use wf_ports::log::{
    decode_workflow_value_batch, encode_workflow_value_batch, normal_recovery_header,
};
use wf_types::identity::RequestId;
use wf_types::version::{Fence, TimestampMillis};

use super::{TestResult, fixtures as f};

#[test]
fn value_binary_encoder_preserves_presence_and_configured_defaults() -> TestResult {
    let mut request = presence_request()?;
    let limits = f::limits().record_batch.codec;
    let encoded = encode_workflow_value_batch(&request, limits);
    assert!(encoded.is_ok(), "{:?}", encoded.as_ref().err());
    let Ok(encoded) = encoded else {
        return Err("expected producer success".into());
    };
    let header = normal_recovery_header(encoded.as_bytes(), limits)?;
    assert_eq!(header.0, 8);
    assert_eq!(
        header.1,
        [b"t".as_slice(), b"g".as_slice(), b"z".as_slice()]
    );
    assert_eq!(header.2, crate::fixtures::clock(100)?);
    let decoded = decode_workflow_value_batch(encoded.as_bytes(), limits);
    assert!(decoded.is_ok(), "{:?}", decoded.as_ref().err());
    let Ok(decoded) = decoded else {
        return Err("expected owned decoder success".into());
    };
    assert_presence_fields(&decoded)?;
    let donor = StoredPolicyValue::Complete(Box::new(f::complete_policy(2, 11_000, 22_000)?));
    request.policies = vec![f::witness(b"r", Some(&donor))?];
    request.request = RequestId::new(b"next".to_vec())?;
    request.stamp = crate::fixtures::clock(101)?;
    request.inputs[0].target = WorkflowValueTargetGuard::Present { incarnation: 55 };
    request.policy_references_captured = None;
    let changed = encode_workflow_value_batch(&request, limits);
    assert!(changed.is_ok(), "{:?}", changed.as_ref().err());
    let Ok(changed) = changed else {
        return Err("server-context encoding rejected".into());
    };
    assert_ne!(encoded.digest(), changed.digest());
    let original_intent = encoded
        .as_bytes()
        .get(32..64)
        .ok_or("original intent missing")?;
    let changed_intent = changed
        .as_bytes()
        .get(32..64)
        .ok_or("changed intent missing")?;
    assert_eq!(original_intent.len(), 32);
    assert_eq!(original_intent, changed_intent);
    request.inputs[0]
        .birth
        .as_mut()
        .ok_or("input birth")?
        .active = MaxActiveOverride::Inherit;
    let caller = encode_workflow_value_batch(&request, limits);
    assert!(caller.is_ok(), "{:?}", caller.as_ref().err());
    let Ok(caller) = caller else {
        return Err("caller encoding rejected".into());
    };
    let caller_intent = caller
        .as_bytes()
        .get(32..64)
        .ok_or("caller intent missing")?;
    assert_ne!(caller_intent, changed_intent);
    drop((request, encoded, changed, caller, donor));
    assert_presence_fields(&decoded)?;
    Ok(())
}

#[test]
fn value_binary_encoder_preserves_reducer_modes_and_policy_witnesses() -> TestResult {
    let original = f::original_policy(1)?;
    for profile in [1, 5, 6, 7] {
        let witness = f::witness(b"r", Some(&original))?;
        let expected_digest = witness.digest;
        let now = if profile == 1 { 90 } else { 100 };
        let command = if profile == 7 {
            Operation::StartAndClaim(StartAndClaim {
                workflow_type: b"r".to_vec(),
                state: b"q".to_vec(),
                worker: b"w".to_vec(),
                lease_ms: 100,
                payload: Some(Vec::new()),
            })
        } else {
            Operation::Create(Create {
                workflow_type: b"r".to_vec(),
                state: b"q".to_vec(),
                run_at: None,
                payload: Some(Vec::new()),
            })
        };
        let item = f::item(b"i", now, command)?;
        let operation = if profile == 1 {
            PreparedOperation::PublicMutation {
                item: Box::new(item),
                evidence: PublicMutationEvidence {
                    requested_now_ms: Some(90),
                    auto_partition: false,
                    retry_override_present: false,
                },
            }
        } else {
            PreparedOperation::Batch(vec![item])
        };
        let mut presence = f::input(0, 0);
        presence.birth = Some(LifecycleOverrides {
            active: MaxActiveOverride::Inherit,
            retention: RetentionOverride::default(),
            history_hot_max_events: None,
        });
        presence.requested_at = (profile == 1).then_some(TimestampMillis::new(90));
        let request = f::batch(
            profile,
            b"mode",
            100,
            operation,
            vec![presence],
            vec![witness],
        )?;
        let limits = f::limits().record_batch.codec;
        let encoded = encode_workflow_value_batch(&request, limits);
        assert!(encoded.is_ok(), "{:?}", encoded.as_ref().err());
        let Ok(encoded) = encoded else {
            return Err("expected mode encoder success".into());
        };
        let decoded = decode_workflow_value_batch(encoded.as_bytes(), limits);
        assert!(decoded.is_ok(), "{:?}", decoded.as_ref().err());
        let Ok(decoded) = decoded else {
            return Err("expected mode decoder success".into());
        };
        assert_eq!(decoded.profile, profile);
        assert_eq!(decoded.policies.len(), 1);
        assert_eq!(
            (decoded.policies[0].profile, decoded.policies[0].generation),
            (2, 1)
        );
        assert_eq!(decoded.policies[0].digest, expected_digest);
        assert_eq!(decoded.inputs[0].requested_at, presence.requested_at);
        let items = decoded.operation.mutation_items().ok_or("decoded items")?;
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].command.now.get(), now);
        assert_eq!(normal_recovery_header(encoded.as_bytes(), limits)?.0, 8);
        drop((encoded, request));
        let items = decoded.operation.mutation_items().ok_or("owned items")?;
        assert_eq!(items[0].partition.as_bytes(), b"p");
        assert_eq!(items[0].command.id.as_bytes(), b"i");
    }
    Ok(())
}

fn presence_request() -> TestResult<PreparedWorkflowValueBatch> {
    let absent = f::witness(b"r", None)?;
    assert_eq!((absent.profile, absent.generation), (0, 0));
    assert_eq!(
        absent.digest.as_bytes(),
        &[
            0x6e, 0x99, 0x78, 0x63, 0x6a, 0x60, 0x17, 0xb6, 0xe2, 0xab, 0xf8, 0x16, 0x47, 0xa5,
            0xaa, 0xb1, 0xb7, 0x1f, 0x00, 0x3b, 0x16, 0xe2, 0x3a, 0xd9, 0x31, 0xa1, 0xbe, 0xda,
            0x2d, 0x62, 0xb9, 0xa9,
        ]
    );
    let mut birth = f::input(0, 0);
    birth.target = WorkflowValueTargetGuard::Absent;
    birth.birth = Some(LifecycleOverrides {
        active: MaxActiveOverride::Infinite,
        retention: RetentionOverride {
            ttl_ms: Some(33_000),
            history_max_events: Some(50),
        },
        history_hot_max_events: Some(4),
    });
    assert!(
        capture_lifecycle(
            None,
            b"r",
            b"q",
            birth.birth.ok_or("wire birth fixture missing")?,
            f::defaults().lifecycle,
        )
        .is_ok()
    );
    let mut terminal = f::input(1, 0);
    terminal.terminal_ttl_ms = Some(0);
    let create = Operation::Create(Create {
        workflow_type: b"r".to_vec(),
        state: b"q".to_vec(),
        run_at: None,
        payload: Some(b"FSV2\x02opaque".to_vec()),
    });
    let cancel = Operation::Cancel(Cancel {
        fence: Fence::new(0),
        lease_token: None,
        expected_state: None,
        expected_version: None,
        error: Some(Vec::new()),
    });
    let operation = PreparedOperation::Batch(vec![
        f::item(b"i", 100, create)?,
        f::item(b"j", 100, cancel)?,
    ]);
    f::batch(
        5,
        b"wire",
        100,
        operation,
        vec![birth, terminal],
        vec![absent],
    )
}

fn assert_presence_fields(decoded: &PreparedWorkflowValueBatch) -> TestResult {
    assert_eq!(decoded.profile, 5);
    assert_eq!(decoded.policy_references_captured, Some(true));
    assert_eq!(decoded.inputs.len(), 2);
    assert_eq!(decoded.inputs[1].terminal_ttl_ms, Some(0));
    assert!(matches!(
        decoded.inputs[0].target,
        WorkflowValueTargetGuard::Absent
    ));
    let selected = decoded.inputs[0].birth.ok_or("birth presence lost")?;
    assert!(matches!(selected.active, MaxActiveOverride::Infinite));
    assert_eq!(selected.retention.ttl_ms, Some(33_000));
    assert_eq!(selected.retention.history_max_events, Some(50));
    assert_eq!(selected.history_hot_max_events, Some(4));
    let actual = decoded.defaults.lifecycle;
    assert_eq!(actual.ttl_ms, 7_000);
    assert_eq!(actual.history_hot_max_events, 2);
    assert_eq!(actual.history_max_events, 10);
    assert_eq!(actual.max_history_hot_max_events, 100);
    assert_eq!(actual.max_history_max_events, 1_000);
    assert_eq!(
        decoded.defaults.configuration_digest,
        f::defaults().configuration_digest
    );
    assert_eq!(decoded.policies.len(), 1);
    assert_eq!(decoded.policies[0].workflow_type.as_slice(), b"r");
    assert_eq!(
        (decoded.policies[0].profile, decoded.policies[0].generation),
        (0, 0)
    );
    assert_eq!(
        decoded.policies[0].digest.as_bytes(),
        &[
            0x6e, 0x99, 0x78, 0x63, 0x6a, 0x60, 0x17, 0xb6, 0xe2, 0xab, 0xf8, 0x16, 0x47, 0xa5,
            0xaa, 0xb1, 0xb7, 0x1f, 0x00, 0x3b, 0x16, 0xe2, 0x3a, 0xd9, 0x31, 0xa1, 0xbe, 0xda,
            0x2d, 0x62, 0xb9, 0xa9,
        ]
    );
    let items = decoded.operation.mutation_items().ok_or("decoded items")?;
    assert_eq!(items.len(), 2);
    assert_eq!(items[0].command.id.as_bytes(), b"i");
    let Operation::Create(create) = &items[0].command.operation else {
        return Err("create operation class changed".into());
    };
    assert_eq!(
        create.payload.as_deref(),
        Some(b"FSV2\x02opaque".as_slice())
    );
    let Operation::Cancel(cancel) = &items[1].command.operation else {
        return Err("cancel operation class changed".into());
    };
    assert_eq!(cancel.error.as_deref(), Some(b"".as_slice()));
    Ok(())
}
