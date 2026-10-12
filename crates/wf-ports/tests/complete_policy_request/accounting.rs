//! Independently frozen caller work and prospective nonrollback refusals.

use std::collections::BTreeMap;

use wf_domain::policy::RetryOverride;
use wf_domain::policy::complete::{
    CompletePolicyEncodingError, CompletePolicyReplaceInput, CompletePolicyRequestError,
    CompleteStatePolicyData, PolicySchemaCharge, PolicySchemaError, PolicySchemaLimits,
    prepare_complete_policy_replace_caller,
};
use wf_domain::policy::definition::PolicyVersion;
use wf_domain::policy::installation::PolicyLimits;
use wf_domain::workflow::admission::AdmissionLimits;
use wf_domain::workflow::request::RequestScope;
use wf_ports::log::prepare_complete_policy_replace_intent;
use wf_types::identity::{GroupId, RecoveryId, RequestId, TenantId};
use wf_types::version::{CommittedTime, HlcTimestamp};

use super::{
    ATTRIBUTE_INTENT_DIGEST, GUARDED_INTENT_DIGEST, MINIMAL_INTENT_DIGEST, TestResult, input,
    minimal_data, profile,
};

#[test]
fn complete_replace_caller_freezes_independent_intent_fees() -> TestResult {
    // Separate independent inputs; no account is reset during an operation.
    for (guard, attributes, fee, digest) in [
        (None, false, 1_564, MINIMAL_INTENT_DIGEST),
        (Some(0), false, 1_844, GUARDED_INTENT_DIGEST),
        (None, true, 2_615, ATTRIBUTE_INTENT_DIGEST),
    ] {
        let limits = profile(fee)?;
        let mut charge = PolicySchemaCharge::new(limits);
        let mut value = minimal_data();
        if attributes {
            value.indexed_attributes = Some(vec![b"a".to_vec()]);
        }
        let pointer = value.workflow_type.as_ptr();
        let result = prepare_complete_policy_replace_intent(
            input(value, guard)?,
            limits,
            AdmissionLimits::default(),
            &mut charge,
        )?;
        let mut expected = minimal_data();
        if attributes {
            expected.indexed_attributes = Some(vec![b"a".to_vec()]);
        }
        assert!(result.caller().input().data() == &expected);
        assert_eq!(result.caller().input().expected_generation(), guard);
        assert_eq!(result.digest(), &digest);
        assert_eq!((charge.g(), charge.p(), charge.total()), (fee, 0, fee));
        assert_eq!(
            result.caller().input().data().workflow_type.as_ptr(),
            pointer
        );
    }

    let limits = profile(3_128)?;
    let mut charge = PolicySchemaCharge::new(limits);
    for total in [1_564, 3_128] {
        let caller = prepare_complete_policy_replace_caller(
            input(minimal_data(), None)?,
            limits,
            AdmissionLimits::default(),
            &mut charge,
        )?;
        assert!(caller.input().data() == &minimal_data());
        assert!(caller.schema_limits() == limits);
        assert_eq!(caller.admission_limits(), AdmissionLimits::default());
        assert_eq!((charge.g(), charge.p(), charge.total()), (total, 0, total));
    }

    for tenant_length in [8_172, 8_173] {
        let limits = profile(100_000)?;
        let mut charge = PolicySchemaCharge::new(limits);
        let request = CompletePolicyReplaceInput::new(
            RequestScope::new(
                TenantId::new(vec![b't'; tenant_length])?,
                GroupId::new(b"g".to_vec())?,
                RecoveryId::new(b"r".to_vec())?,
            ),
            RequestId::new(b"q".to_vec())?,
            CommittedTime::new(HlcTimestamp::new(0, 0)?, 0)?,
            None,
            minimal_data(),
        );
        let result = prepare_complete_policy_replace_caller(
            request,
            limits,
            AdmissionLimits::default(),
            &mut charge,
        );
        if tenant_length == 8_172 {
            let caller = result?;
            assert!(caller.input().data() == &minimal_data());
            assert_eq!(
                caller.input().scope().tenant().as_bytes(),
                vec![b't'; 8_172]
            );
            assert_eq!(caller.input().scope().group().as_bytes(), b"g");
            assert_eq!(caller.input().scope().recovery().as_bytes(), b"r");
            // B=52+8171; two 5-node walks plus the hash node: 3*B+1408.
            assert_eq!((charge.g(), charge.p()), (26_077, 0));
        } else {
            assert_eq!(result.err(), Some(CompletePolicyRequestError::ScopeLimit));
            assert_eq!((charge.g(), charge.p()), (128, 0));
        }
    }
    check_complete_replace_attribute_limit()?;
    Ok(())
}

#[test]
fn complete_replace_caller_refuses_without_resetting_prior_work() -> TestResult {
    for (maximum, preceding, retained, prospective) in
        [(1_563, false, 1_384, 1_564), (3_127, true, 2_948, 3_128)]
    {
        let limits = profile(maximum)?;
        let mut charge = PolicySchemaCharge::new(limits);
        if preceding {
            let caller = prepare_complete_policy_replace_caller(
                input(minimal_data(), None)?,
                limits,
                AdmissionLimits::default(),
                &mut charge,
            )?;
            assert!(caller.input().data() == &minimal_data());
            assert_eq!((charge.g(), charge.p()), (1_564, 0));
        }
        let error = prepare_complete_policy_replace_intent(
            input(minimal_data(), None)?,
            limits,
            AdmissionLimits::default(),
            &mut charge,
        )
        .err()
        .ok_or("missing caller work refusal")?;
        assert_eq!(
            error,
            CompletePolicyRequestError::Encoding(CompletePolicyEncodingError::Schema(
                PolicySchemaError::WorkBytes {
                    bytes: prospective,
                    maximum,
                }
            ))
        );
        assert_eq!(
            (charge.g(), charge.p(), charge.total()),
            (retained, 0, retained)
        );
    }

    for scenario in 0..3 {
        let limits = PolicySchemaLimits::new(PolicyLimits::new(1)?, 20_000_000)?;
        let mut charge = PolicySchemaCharge::new(limits);
        let mut value = minimal_data();
        match scenario {
            0 => {
                for key in *b"az" {
                    value.states.insert(
                        vec![key],
                        CompleteStatePolicyData {
                            mode: None,
                            retry: None,
                            retention: None,
                            governance: None,
                        },
                    );
                }
            }
            1 => value.workflow_type = vec![b'r'; 262_145],
            _ => {
                value.workflow_type = vec![b'r'; 262_144];
                value.version = Some(PolicyVersion::Bytes(vec![b'v'; 262_144]));
                value.retry = Some(RetryOverride {
                    max_retries: None,
                    backoff: None,
                    exhausted_to: Some(vec![b'x'; 262_144]),
                });
                value.states = BTreeMap::from([(
                    vec![b's'; 262_144],
                    CompleteStatePolicyData {
                        mode: None,
                        retry: None,
                        retention: None,
                        governance: None,
                    },
                )]);
            }
        }
        let error = prepare_complete_policy_replace_caller(
            input(value, None)?,
            limits,
            AdmissionLimits::default(),
            &mut charge,
        )
        .err()
        .ok_or("missing structural caller refusal")?;
        let expected = if scenario == 0 {
            CompletePolicyEncodingError::ItemLimit
        } else {
            CompletePolicyEncodingError::ByteLimit
        };
        assert_eq!(error, CompletePolicyRequestError::Encoding(expected));
        assert_eq!(charge.p(), 0);
        match scenario {
            0 => assert_eq!(charge.g(), 557),
            1 => assert_eq!(charge.g(), 548),
            _ => assert!(charge.g() > 818),
        }
    }
    Ok(())
}

fn check_complete_replace_attribute_limit() -> TestResult {
    let count_limits = profile(1_000_000)?;
    let mut count_charge = PolicySchemaCharge::new(count_limits);
    let mut value = minimal_data();
    let names = vec![b"a".to_vec(), b"b".to_vec(), b"c".to_vec(), b"d".to_vec()];
    value.indexed_attributes = Some(names);
    let error = prepare_complete_policy_replace_caller(
        input(value, None)?,
        count_limits,
        AdmissionLimits::default(),
        &mut count_charge,
    )
    .err()
    .ok_or("missing attribute count refusal")?;
    assert_eq!(
        error,
        CompletePolicyRequestError::Encoding(CompletePolicyEncodingError::ItemLimit)
    );
    assert_eq!((count_charge.g(), count_charge.p()), (818, 0));
    Ok(())
}
