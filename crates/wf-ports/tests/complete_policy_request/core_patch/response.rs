//! Complete intent-two receipts require semantic policy validation or exact errors.

use super::prepare::cases::decode_core_patch_literal;
use super::prepare::literals::FROZEN_CORE_PATCH_FRAMES;
use crate::{TestResult, profile};
use wf_domain::policy::complete::{PolicySchemaCharge, PolicySchemaLimits};
use wf_domain::policy::installation::{MAX_POLICY_GENERATION, PolicyError, PolicyLimits};
use wf_domain::retention::RetentionError;
use wf_domain::value::{DomainError, InputField, RetryPolicyField};
use wf_domain::workflow::admission::AdmissionLimits;
use wf_domain::workflow::request::{ClientIntent, PolicyRejection, RetainedStatus};
use wf_ports::log::{
    CodecError, complete_core_policy_patch_retained_status, verify_complete_core_policy_patch_frame,
};
use wf_types::version::{AppliedPosition, LogIdentity};

pub(super) fn intent() -> TestResult<ClientIntent> {
    // Fixture preparation uses its own account; measured receipt work starts later.
    let bytes = decode_core_patch_literal(FROZEN_CORE_PATCH_FRAMES[0].0)?;
    let schema = PolicySchemaLimits::default();
    let verified = verify_complete_core_policy_patch_frame(
        &bytes,
        schema,
        AdmissionLimits::default(),
        &mut PolicySchemaCharge::new(schema),
    )?;
    Ok(verified
        .header(LogIdentity::new(1, 7, AppliedPosition::new(0)))
        .intent)
}

pub(super) fn minimal_value() -> Vec<u8> {
    b"FWPV\0\x02\0\0\0\0\0\0\0\x01\0\0\0\x16FWPS\0\x02\0\0\0\x01r\0\0\0\0\0\0\0\0\0\0\0".to_vec()
}

pub(super) fn full_value() -> TestResult<Vec<u8>> {
    // Pre-existing independent full VALUE oracle: every governance/indexed field.
    decode_core_patch_literal(
        "46575056000200000000000000010000016c4657505300020000000172010100000000000000000100000000000000010101000001030000000000000001000000000000000200010000000178010100000000000000010100000001000000010000000173010101000000010100000000000000010100000001010000000000000001000000020000000161000000017a01000000016d0101020101000000020000000161000000017a0100000000010001010001020100000001000000016c00000000000000000100010000000000000001010000000100000001620000000000000001000000000000000101010100000000000000010100000001750100000001000000016300000000000000010000000000000001010000000000000001010000000000000001013201000000000000000101000000000000000101000000000000000101000000020000000161000000017a01010000000100000001610100000001000000017301000000010000000175010000000000000001",
    )
}

pub(super) fn patched_value() -> TestResult<Vec<u8>> {
    decode_core_patch_literal(
        "465750560002000000000000000200000194465750530002000000017201010000000000000000010000000000000001010100000103000000000000000100000000000000020001000000017801010000000000000001010000000100000003000000067175657565640001000001000000046465616400000000000672657669657701010000000000000173010101000000010100000000000000010100000001010000000000000001000000020000000161000000017a01000000016d0101020101000000020000000161000000017a0100000000010001010001020100000001000000016c00000000000000000100010000000000000001010000000100000001620000000000000001000000000000000101010100000000000000010100000001750100000001000000016300000000000000010000000000000001010000000000000001010000000000000001013201000000000000000101000000000000000101000000000000000101000000020000000161000000017a01010000000100000001610100000001000000017301000000010000000175010000000000000001",
    )
}

pub(super) fn business_messages() -> &'static [(PolicyError, &'static [u8])] {
    &[
        (
            PolicyError::InvalidVersion,
            b"ERR flow policy version must be a non-empty string or non-negative integer",
        ),
        (
            PolicyError::InvalidMaxActive,
            b"ERR flow max_active_ms must be between 1 and 31536000000 or infinity",
        ),
        (
            PolicyError::InvalidStateMode,
            b"ERR flow state mode must be parallel or fifo",
        ),
        (
            PolicyError::InvalidStatePolicy,
            b"ERR flow state policy must be a map or keyword list",
        ),
        (
            PolicyError::InvalidGeneration,
            b"ERR invalid flow policy generation",
        ),
        (
            PolicyError::StaleGeneration,
            b"ERR stale flow policy generation",
        ),
        (
            PolicyError::GenerationExhausted,
            b"ERR flow policy generation exhausted",
        ),
        (
            PolicyError::Retention(RetentionError::Ttl),
            b"ERR flow retention ttl_ms must be between 1 and 31536000000",
        ),
        (
            PolicyError::Retention(RetentionError::History),
            b"ERR flow retention history_max_events is outside configured bounds",
        ),
        (
            PolicyError::Domain(DomainError::EmptyValue(InputField::Type)),
            b"ERR flow type must be a non-empty string",
        ),
        (
            PolicyError::Domain(DomainError::InvalidRetryPolicy(
                RetryPolicyField::MaxRetries,
            )),
            b"ERR flow retry max_retries must be between 0 and 1000",
        ),
        (
            PolicyError::Domain(DomainError::InvalidRetryPolicy(
                RetryPolicyField::BaseMillis,
            )),
            b"ERR flow retry base_ms must be between 0 and 2592000000",
        ),
        (
            PolicyError::Domain(DomainError::InvalidRetryPolicy(RetryPolicyField::MaxMillis)),
            b"ERR flow retry max_ms must be between 0 and 2592000000",
        ),
        (
            PolicyError::Domain(DomainError::InvalidRetryPolicy(
                RetryPolicyField::JitterPercent,
            )),
            b"ERR flow retry jitter_pct must be between 0 and 100",
        ),
        (
            PolicyError::Domain(DomainError::InvalidRetryPolicy(
                RetryPolicyField::ExhaustedTo,
            )),
            b"ERR flow retry exhausted_to must be a non-empty string",
        ),
        (
            PolicyError::Domain(DomainError::InvalidRetryPolicy(
                RetryPolicyField::RunningExhaustion,
            )),
            b"ERR flow retry exhausted_to cannot be running",
        ),
        (
            PolicyError::Domain(DomainError::InvalidRetryPolicy(RetryPolicyField::State)),
            b"ERR flow policy state must be a non-empty string",
        ),
        (
            PolicyError::Domain(DomainError::InvalidRetryPolicy(
                RetryPolicyField::RunningState,
            )),
            b"ERR flow policy state cannot be running",
        ),
    ]
}

fn status(
    response: &[u8],
    schema: PolicySchemaLimits,
    charge: &mut PolicySchemaCharge,
    owner: usize,
) -> Result<RetainedStatus, CodecError> {
    complete_core_policy_patch_retained_status(
        intent().map_err(|_| CodecError::Malformed)?,
        1,
        response,
        (schema, AdmissionLimits::default()),
        charge,
        owner,
    )
}

#[test]
fn intent2_response1_validates_complete_semantics_and_exact_business_registry() -> TestResult {
    let schema = profile(100_000_000)?;
    for response in [minimal_value(), full_value()?] {
        let mut charge = PolicySchemaCharge::new(schema);
        let owner = 2048 + 216 * response.len();
        assert_eq!(
            status(&response, schema, &mut charge, owner)?,
            RetainedStatus::Accepted
        );
        assert!(charge.g() > 0);
    }
    assert_exact_registry()?;
    assert_malformed_receipts()?;
    Ok(())
}

fn assert_exact_registry() -> TestResult {
    let messages = business_messages();
    assert_eq!(messages.len(), 18);
    for (index, (error, message)) in messages.iter().enumerate() {
        // Independent fixed tariff, including every failed comparison before match.
        let fee = 128
            + messages[..=index]
                .iter()
                .map(|(_, prior)| 128 + message.len() as u64 + prior.len() as u64)
                .sum::<u64>();
        let schema = profile(fee)?;
        let mut charge = PolicySchemaCharge::new(schema);
        assert_eq!(
            status(message, schema, &mut charge, 0)?,
            RetainedStatus::PolicyRejected(PolicyRejection::new(*error)?)
        );
        assert_eq!((charge.g(), charge.p()), (fee, 0));
        let short = profile(fee - 1)?;
        let mut charge = PolicySchemaCharge::new(short);
        assert_eq!(
            status(message, short, &mut charge, 0),
            Err(CodecError::ByteLimit)
        );
        assert_eq!(
            (charge.g(), charge.p()),
            (fee - 128 - 2 * message.len() as u64, 0)
        );
        for malformed in [
            message[..message.len() - 1].to_vec(),
            [*message, &[0]].concat(),
        ] {
            let schema = profile(100_000_000)?;
            let mut charge = PolicySchemaCharge::new(schema);
            assert_eq!(
                status(&malformed, schema, &mut charge, 0),
                Err(CodecError::Malformed)
            );
            let all = 128
                + messages
                    .iter()
                    .map(|(_, literal)| 128 + malformed.len() as u64 + literal.len() as u64)
                    .sum::<u64>();
            assert_eq!((charge.g(), charge.p()), (all, 0));
        }
    }
    Ok(())
}

fn assert_malformed_receipts() -> TestResult {
    let schema = profile(100_000_000)?;
    let mut invalid = Vec::new();
    for generation in [0, MAX_POLICY_GENERATION + 1] {
        let mut bytes = minimal_value();
        bytes[6..14].copy_from_slice(&generation.to_be_bytes());
        invalid.push((bytes, CodecError::Malformed));
    }
    let mut bytes = minimal_value();
    bytes[5] = 3;
    invalid.push((bytes, CodecError::UnsupportedVersion));
    let mut bytes = minimal_value();
    bytes.push(0);
    invalid.push((bytes, CodecError::TrailingBytes));
    let mut bytes = minimal_value();
    bytes[39] = 2;
    invalid.push((bytes, CodecError::InvalidOptionality));
    // Complete structural EOF with an empty workflow type is semantically invalid.
    let mut bytes = minimal_value();
    bytes[24..28].copy_from_slice(&0_u32.to_be_bytes());
    bytes.remove(28);
    bytes[14..18].copy_from_slice(&21_u32.to_be_bytes());
    invalid.push((bytes, CodecError::Malformed));
    let mut bytes = full_value()?;
    assert_eq!(bytes[39], 1); // Present max_active_ms follows present numeric version.
    assert_eq!(&bytes[40..48], &1_u64.to_be_bytes());
    bytes[40..48].copy_from_slice(&0_u64.to_be_bytes());
    invalid.push((bytes, CodecError::Malformed));
    for (bytes, expected) in invalid {
        let mut charge = PolicySchemaCharge::new(schema);
        assert_eq!(
            status(&bytes, schema, &mut charge, 2048 + 216 * bytes.len()),
            Err(expected)
        );
    }
    for (intent, response_version) in [
        (ClientIntent::new(1, [7; 32])?, 1),
        (intent()?, 2),
        (ClientIntent::record_batch([7; 32]), 1),
        (ClientIntent::independent_record_batch([7; 32]), 1),
    ] {
        let mut charge = PolicySchemaCharge::new(schema);
        assert_eq!(
            complete_core_policy_patch_retained_status(
                intent,
                response_version,
                &minimal_value(),
                (schema, AdmissionLimits::default()),
                &mut charge,
                100_000,
            ),
            Err(CodecError::UnsupportedVersion)
        );
        assert_eq!((charge.g(), charge.p()), (0, 0));
    }
    Ok(())
}

#[test]
fn intent2_response_profiles_separate_refusal_from_malformed_grammar() -> TestResult {
    let bytes = minimal_value();
    let schema = profile(100_000_000)?;
    let owner = 2048 + 216 * bytes.len();
    let mut charge = PolicySchemaCharge::new(schema);
    assert_eq!(
        status(&bytes, schema, &mut charge, owner - 1),
        Err(CodecError::ByteLimit)
    );
    assert_eq!((charge.g(), charge.p()), (0, 0));
    assert_eq!(
        status(&bytes, schema, &mut charge, owner)?,
        RetainedStatus::Accepted
    );
    // Existing SAME decoder: 552 structural +791 material +642 restore +168 copy.
    assert_eq!((charge.g(), charge.p()), (2153, 0));
    for maximum in [2153, 2152] {
        let schema = profile(maximum)?;
        let mut charge = PolicySchemaCharge::new(schema);
        if maximum == 2153 {
            assert_eq!(
                status(&bytes, schema, &mut charge, owner)?,
                RetainedStatus::Accepted
            );
            assert_eq!((charge.g(), charge.p()), (2153, 0));
        } else {
            assert_eq!(
                status(&bytes, schema, &mut charge, owner),
                Err(CodecError::ByteLimit)
            );
            assert_eq!((charge.g(), charge.p()), (1985, 0));
        }
    }
    let two_states = b"FWPV\0\x02\0\0\0\0\0\0\0\x01\0\0\0\x28FWPS\0\x02\0\0\0\x01r\0\0\0\0\0\0\0\x02\0\0\0\x01a\0\0\0\0\0\0\0\x01z\0\0\0\0\0\0\0";
    let narrow = PolicySchemaLimits::new(PolicyLimits::new(1)?, 100_000_000)?;
    let mut charge = PolicySchemaCharge::new(narrow);
    assert_eq!(
        status(
            two_states,
            narrow,
            &mut charge,
            2048 + 216 * two_states.len()
        ),
        Err(CodecError::ItemLimit)
    );
    let mut charge = PolicySchemaCharge::new(schema);
    assert_eq!(
        status(
            two_states,
            schema,
            &mut charge,
            2048 + 216 * two_states.len()
        )?,
        RetainedStatus::Accepted
    );
    let admission = AdmissionLimits::new(1_048_576, bytes.len() - 1, 1_000)?;
    let mut charge = PolicySchemaCharge::new(schema);
    assert_eq!(
        complete_core_policy_patch_retained_status(
            intent()?,
            1,
            &bytes,
            (schema, admission),
            &mut charge,
            owner,
        ),
        Err(CodecError::ByteLimit)
    );
    assert_eq!((charge.g(), charge.p()), (0, 0));
    Ok(())
}

#[test]
fn full_core_patch_plan_binds_the_predecessor_used_for_preparation() -> TestResult {
    use std::collections::BTreeMap;
    use wf_domain::policy::complete::{
        CompleteCorePolicyPatchApplyView, decode_complete_policy_value_v2,
    };
    use wf_domain::workflow::control::ControlState;
    use wf_domain::workflow::request::{RequestScope, gate::GateView};
    use wf_ports::log::decide_complete_core_policy_patch_frame;
    use wf_ports::state::{GroupCorePolicyChanges, StateTransactionError};
    use wf_types::identity::{GroupId, RecoveryId, TenantId};
    use wf_types::version::{CommittedTime, HlcTimestamp};

    let schema = profile(100_000_000)?;
    let mut charge = PolicySchemaCharge::new(schema);
    let scope = RequestScope::new(
        TenantId::new(b"t".to_vec())?,
        GroupId::new(b"g".to_vec())?,
        RecoveryId::new(b"r".to_vec())?,
    );
    let control = ControlState::initial(CommittedTime::new(HlcTimestamp::new(0, 0)?, 0)?);
    let actual = decode_complete_policy_value_v2(&full_value()?, schema, &mut charge)?;
    let policies = BTreeMap::from([(
        b"r".to_vec(),
        Ok(decode_complete_policy_value_v2(
            &minimal_value(),
            schema,
            &mut charge,
        )?),
    )]);
    let outcomes = BTreeMap::new();
    let migrations = BTreeMap::new();
    let mut mismatches = Vec::new();
    for case in [0, 14] {
        let frame = decode_core_patch_literal(FROZEN_CORE_PATCH_FRAMES[case].0)?;
        let view = CompleteCorePolicyPatchApplyView {
            gate: GateView {
                scope: &scope,
                control,
                outcomes: &outcomes,
            },
            revision: control.position().get(),
            policies: &policies,
            active_migrations: &migrations,
            completed_migrations: &migrations,
            history_max_events_limit: Ok(1000),
            outcome_retention_ms: Ok(100),
        };
        let plan = decide_complete_core_policy_patch_frame(
            view,
            &frame,
            LogIdentity::new(1, 7, AppliedPosition::new(0)),
            schema,
            AdmissionLimits::default(),
            &mut charge,
        )?;
        mismatches.push(
            GroupCorePolicyChanges::new(
                &scope,
                control,
                Some(&actual),
                plan,
                (schema, AdmissionLimits::default()),
                &mut charge,
                1_000_000,
            )
            .err(),
        );
    }
    assert_eq!(
        mismatches,
        vec![Some(StateTransactionError::InvalidChanges); 2]
    );
    Ok(())
}
