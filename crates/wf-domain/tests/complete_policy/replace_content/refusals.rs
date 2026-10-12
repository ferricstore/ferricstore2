//! Operational refusals and fresh payments on the caller's one continuous account.

use std::collections::BTreeMap;

use wf_domain::policy::complete::{
    CompletePolicyData, CompletePolicyEncodingError, CompletePolicyReplaceRejectionKind as Kind,
    EffectsData, GovernanceField, GovernanceRuleKind, LimitData, PolicySchemaCharge,
    PolicySchemaError, PolicySchemaLimits, check_complete_policy_replace_content,
    encode_complete_policy_replace_rejection_body, prepare_complete_policy_replace,
};
use wf_domain::policy::definition::PolicyVersion;
use wf_domain::policy::installation::{PolicyError, PolicyLimits};
use wf_domain::retention::RetentionError;

use super::super::{bind, candidate};
use super::{TestResult, assert_rejection, empty_governance, history_data, limits, minimal, state};

#[test]
fn replace_content_preserves_operational_preflight_and_schema_accounts() -> TestResult {
    assert_minimal_refusals()?;
    assert_held_account()?;
    assert_operational_preflight()?;
    let profile = limits(20_000_000)?;
    let mut charge = PolicySchemaCharge::new(profile);
    for maximum in [0, 1_000_001] {
        let before = (charge.g(), charge.p());
        assert_eq!(
            check_complete_policy_replace_content(&minimal(), maximum, profile, &mut charge),
            Err(PolicySchemaError::InvalidAuthority)
        );
        assert_eq!((charge.g(), charge.p()), (before.0 + 128, before.1));
    }
    let before = (charge.g(), charge.p());
    let other = limits(20_000_001)?;
    assert_eq!(
        check_complete_policy_replace_content(&minimal(), 0, other, &mut charge),
        Err(PolicySchemaError::ProfileMismatch)
    );
    assert_eq!((charge.g(), charge.p()), before);
    assert_correspondence_refusals(profile, &mut charge)?;
    assert_scratch_refusal()?;
    Ok(())
}

#[test]
fn replace_content_charges_response_construction_and_keeps_old_contracts() -> TestResult {
    for (maximum, retained, target) in [
        (1_025, 898, 1_026),
        (1_027, 1_026, 1_028),
        (1_217, 1_028, 1_218),
        (1_218, 1_218, 1_218),
        (1_537, 1_348, 1_538),
        (1_538, 1_538, 1_538),
    ] {
        let profile = limits(maximum)?;
        let mut charge = PolicySchemaCharge::new(profile);
        let supplied = history_data(18, false);
        let rejection = check_complete_policy_replace_content(&supplied, 17, profile, &mut charge)?
            .ok_or("history rejection")?;
        assert_eq!((charge.g(), charge.p()), (898, 0));
        let attempts = if maximum > 1_218 { 2 } else { 1 };
        let mut held = None;
        for attempt in 0..attempts {
            let result =
                encode_complete_policy_replace_rejection_body(rejection, profile, &mut charge);
            if attempt + 1 == attempts && maximum < target {
                assert_eq!(
                    result,
                    Err(CompletePolicyEncodingError::Schema(
                        PolicySchemaError::WorkBytes {
                            bytes: target,
                            maximum
                        }
                    ))
                );
            } else {
                let body = result?;
                assert_eq!(
                    body,
                    b"ERR flow retention history_max_events must be between 1 and 17"
                );
                held = Some(body);
            }
        }
        assert_eq!((charge.g(), charge.p()), (retained, 0));
        assert!(supplied == history_data(18, false));
        assert_eq!(rejection.history_max_events_limit(), Some(17));
        if maximum > 1_218 {
            assert_eq!(
                held.as_deref(),
                Some(b"ERR flow retention history_max_events must be between 1 and 17".as_slice())
            );
        }
        let before = (charge.g(), charge.p());
        let other = limits(maximum + 1)?;
        assert_eq!(
            encode_complete_policy_replace_rejection_body(rejection, other, &mut charge),
            Err(CompletePolicyEncodingError::Schema(
                PolicySchemaError::ProfileMismatch
            ))
        );
        assert_eq!((charge.g(), charge.p()), before);
    }
    assert_fixed_body_account()?;
    assert_old_guard_and_diagnostics()?;
    Ok(())
}

fn assert_minimal_refusals() -> TestResult {
    for (target, prior) in [
        (128, 0),
        (256, 128),
        (384, 256),
        (385, 384),
        (642, 385),
        (770, 642),
    ] {
        let maximum = target - 1;
        let profile = limits(maximum)?;
        let mut charge = PolicySchemaCharge::new(profile);
        let supplied = minimal();
        assert_eq!(
            check_complete_policy_replace_content(&supplied, 17, profile, &mut charge),
            Err(PolicySchemaError::WorkBytes {
                bytes: target,
                maximum
            })
        );
        assert_eq!((charge.g(), charge.p()), (prior, 0));
        assert!(supplied == minimal());
    }
    for maximum in [770, 1_539, 1_540] {
        let profile = limits(maximum)?;
        let mut charge = PolicySchemaCharge::new(profile);
        let supplied = minimal();
        assert!(
            check_complete_policy_replace_content(&supplied, 17, profile, &mut charge)?.is_none()
        );
        assert_eq!((charge.g(), charge.p()), (770, 0));
        if maximum > 770 {
            let result = check_complete_policy_replace_content(&supplied, 17, profile, &mut charge);
            if maximum == 1_539 {
                assert_eq!(
                    result,
                    Err(PolicySchemaError::WorkBytes {
                        bytes: 1_540,
                        maximum
                    })
                );
                assert_eq!((charge.g(), charge.p()), (1_412, 0));
            } else {
                assert!(result?.is_none());
                assert_eq!((charge.g(), charge.p()), (1_540, 0));
            }
        }
        assert!(supplied == minimal());
    }
    Ok(())
}

fn assert_held_account() -> TestResult {
    for maximum in [2_761, 2_762, 3_531, 3_532] {
        let profile = limits(maximum)?;
        let mut charge = PolicySchemaCharge::new(profile);
        let mut seeded = minimal();
        seeded.indexed_attributes = Some(vec![b"x".to_vec()]);
        let held = candidate(1, seeded, profile, &mut charge)?;
        assert_eq!((charge.g(), charge.p()), (1_928, 64));
        let supplied = minimal();
        let attempts = if maximum > 2_762 { 2 } else { 1 };
        for attempt in 0..attempts {
            let result = check_complete_policy_replace_content(&supplied, 17, profile, &mut charge);
            let target = if attempts == 1 { 2_762 } else { 3_532 };
            if attempt + 1 == attempts && maximum < target {
                assert_eq!(
                    result,
                    Err(PolicySchemaError::WorkBytes {
                        bytes: target,
                        maximum
                    })
                );
            } else {
                assert!(result?.is_none());
            }
        }
        let expected_g = match maximum {
            2_761 => 2_570,
            2_762 => 2_698,
            3_531 => 3_340,
            _ => 3_468,
        };
        assert_eq!(
            (charge.g(), charge.p(), charge.total()),
            (expected_g, 64, expected_g + 64)
        );
        let mut expected = minimal();
        expected.indexed_attributes = Some(vec![b"x".to_vec()]);
        assert_eq!(held.generation(), 1);
        assert!(held.data() == &expected);
        assert!(supplied == minimal());
        assert_eq!(
            held.data().indexed_attributes.as_deref(),
            Some([b"x".to_vec()].as_slice())
        );
    }
    Ok(())
}

fn assert_operational_preflight() -> TestResult {
    let profile = limits(128 * 1_024 * 1_024)?;
    let mut charge = PolicySchemaCharge::new(profile);
    let small = PolicySchemaLimits::new(PolicyLimits::new(1)?, 128 * 1_024 * 1_024)?;
    let mut small_charge = PolicySchemaCharge::new(small);
    let mut supplied = minimal();
    supplied.version = Some(PolicyVersion::Bytes(Vec::new()));
    supplied.states.insert(b"a".to_vec(), state(None, None));
    supplied.states.insert(b"b".to_vec(), state(None, None));
    assert_eq!(
        check_complete_policy_replace_content(&supplied, 17, small, &mut small_charge),
        Err(PolicySchemaError::Policy(PolicyError::StateBudget))
    );
    assert_eq!((small_charge.g(), small_charge.p()), (256, 0));
    for (attributes, name) in [(true, vec![b'x'; 65]), (false, vec![b'x'; 65])] {
        let mut supplied = minimal();
        supplied.version = Some(PolicyVersion::Bytes(Vec::new()));
        if attributes {
            supplied.indexed_attributes = Some(vec![name]);
        } else {
            supplied.indexed_state_meta = Some(name);
        }
        let expected = if attributes {
            PolicySchemaError::InvalidIndexedAttributes
        } else {
            PolicySchemaError::InvalidIndexedStateMeta
        };
        assert_eq!(
            check_complete_policy_replace_content(&supplied, 17, profile, &mut charge),
            Err(expected)
        );
    }
    let mut supplied = minimal();
    supplied.indexed_attributes = Some(vec![b"x".to_vec(); 4]);
    assert_eq!(
        check_complete_policy_replace_content(&supplied, 17, profile, &mut charge),
        Err(PolicySchemaError::InvalidIndexedAttributes)
    );
    supplied = minimal();
    supplied.workflow_type = vec![b'r'; 262_145];
    assert_eq!(
        check_complete_policy_replace_content(&supplied, 17, profile, &mut charge),
        Err(PolicySchemaError::SnapshotBytes)
    );
    assert_governance_caps(profile, &mut charge)
}

fn assert_governance_caps(
    profile: PolicySchemaLimits,
    charge: &mut PolicySchemaCharge,
) -> TestResult {
    for (names, expected) in [
        (
            vec![b"x".to_vec(); 1_001],
            PolicySchemaError::ListCount(GovernanceField::Allowed),
        ),
        (
            vec![vec![b'x'; 65_536]],
            PolicySchemaError::ListBytes(GovernanceField::Allowed),
        ),
        (
            vec![vec![b'x'; 65_535]; 17],
            PolicySchemaError::GovernanceBytes,
        ),
    ] {
        let mut supplied = minimal();
        supplied.version = Some(PolicyVersion::Bytes(Vec::new()));
        let mut governance = empty_governance();
        governance.effects = Some(EffectsData {
            allowed: Some(names),
            denied: None,
            require_idempotency_key: None,
        });
        supplied.governance = Some(governance);
        assert_eq!(
            check_complete_policy_replace_content(&supplied, 17, profile, charge),
            Err(expected)
        );
    }
    for (count, width, expected) in [
        (
            1_001,
            2,
            PolicySchemaError::RuleCount(GovernanceRuleKind::Limits),
        ),
        (
            1,
            65_536,
            PolicySchemaError::NameBytes(GovernanceRuleKind::Limits),
        ),
    ] {
        let mut supplied = minimal();
        supplied.version = Some(PolicyVersion::Bytes(Vec::new()));
        let mut governance = empty_governance();
        let mut rules = BTreeMap::new();
        for index in 0..count {
            let mut name = vec![0; width];
            name[..2].copy_from_slice(&u16::try_from(index)?.to_be_bytes());
            rules.insert(
                name,
                LimitData {
                    limit: 0,
                    enforcement: None,
                    lease_size: None,
                },
            );
        }
        governance.limits = Some(rules);
        supplied.governance = Some(governance);
        assert_eq!(
            check_complete_policy_replace_content(&supplied, 17, profile, charge),
            Err(expected)
        );
    }
    Ok(())
}

fn assert_correspondence_refusals(
    profile: PolicySchemaLimits,
    charge: &mut PolicySchemaCharge,
) -> TestResult {
    for (attributes, name) in [(true, b" x ".as_slice()), (false, b" x "), (false, b"")] {
        let mut supplied = minimal();
        if attributes {
            supplied.indexed_attributes = Some(vec![name.to_vec()]);
        } else {
            supplied.indexed_state_meta = Some(name.to_vec());
        }
        let expected = if attributes {
            PolicySchemaError::InvalidIndexedAttributes
        } else {
            PolicySchemaError::InvalidIndexedStateMeta
        };
        assert_eq!(
            check_complete_policy_replace_content(&supplied, 17, profile, charge),
            Err(expected)
        );
        if attributes {
            assert_eq!(supplied.indexed_attributes, Some(vec![name.to_vec()]));
        } else {
            assert_eq!(supplied.indexed_state_meta, Some(name.to_vec()));
        }
    }
    let mut supplied = minimal();
    supplied.indexed_attributes = Some(vec![b"x".to_vec(), b"x".to_vec()]);
    assert_eq!(
        check_complete_policy_replace_content(&supplied, 17, profile, charge),
        Err(PolicySchemaError::InvalidIndexedAttributes)
    );
    assert_eq!(
        supplied.indexed_attributes,
        Some(vec![b"x".to_vec(), b"x".to_vec()])
    );
    let mut governance = empty_governance();
    governance.effects = Some(EffectsData {
        allowed: Some(vec![b"x".to_vec(), b"x".to_vec()]),
        denied: None,
        require_idempotency_key: None,
    });
    supplied = minimal();
    supplied.governance = Some(governance);
    assert_eq!(
        check_complete_policy_replace_content(&supplied, 17, profile, charge),
        Err(PolicySchemaError::NonCanonicalList(
            GovernanceField::Allowed
        ))
    );
    assert_eq!(
        supplied
            .governance
            .as_ref()
            .ok_or("governance")?
            .effects
            .as_ref()
            .ok_or("effects")?
            .allowed,
        Some(vec![b"x".to_vec(), b"x".to_vec()])
    );
    Ok(())
}

fn assert_scratch_refusal() -> TestResult {
    // Existing metrics: six nodes and two leaf bytes => 770; new root+two passes=1668.
    let profile = limits(1_731)?;
    let mut charge = PolicySchemaCharge::new(profile);
    let mut supplied = minimal();
    let mut governance = empty_governance();
    governance.effects = Some(EffectsData {
        allowed: Some(vec![b"x".to_vec()]),
        denied: None,
        require_idempotency_key: None,
    });
    supplied.governance = Some(governance);
    assert_eq!(
        check_complete_policy_replace_content(&supplied, 17, profile, &mut charge),
        Err(PolicySchemaError::WorkBytes {
            bytes: 1_732,
            maximum: 1_731
        })
    );
    assert_eq!((charge.g(), charge.p()), (1_668, 0));
    Ok(())
}

fn assert_fixed_body_account() -> TestResult {
    for maximum in [935, 936] {
        let profile = limits(maximum)?;
        let mut charge = PolicySchemaCharge::new(profile);
        let mut supplied = minimal();
        supplied.workflow_type.clear();
        supplied.version = Some(PolicyVersion::Bytes(Vec::new()));
        let rejection = check_complete_policy_replace_content(&supplied, 17, profile, &mut charge)?
            .ok_or("empty type rejection")?;
        assert_eq!((charge.g(), charge.p()), (640, 0));
        let result = encode_complete_policy_replace_rejection_body(rejection, profile, &mut charge);
        if maximum == 935 {
            assert_eq!(
                result,
                Err(CompletePolicyEncodingError::Schema(
                    PolicySchemaError::WorkBytes {
                        bytes: 936,
                        maximum
                    }
                ))
            );
            assert_eq!((charge.g(), charge.p()), (768, 0));
        } else {
            assert_eq!(result?, b"ERR flow type must be a non-empty string");
            assert_eq!((charge.g(), charge.p()), (936, 0));
        }
        assert_eq!(supplied.workflow_type, [] as [u8; 0]);
    }
    Ok(())
}

fn assert_old_guard_and_diagnostics() -> TestResult {
    let profile = limits(20_000_000)?;
    let mut setup = PolicySchemaCharge::new(profile);
    let current = bind(
        candidate(1, minimal(), profile, &mut setup)?,
        vec![0],
        0..1,
        profile,
        &mut setup,
    )?;
    let mut input = minimal();
    input.max_active_ms = Some(0);
    let mut charge = PolicySchemaCharge::new(profile);
    let result =
        prepare_complete_policy_replace(Some(&current), 0, Some(0), input, profile, &mut charge);
    assert!(matches!(
        result,
        Err(CompletePolicyEncodingError::Schema(
            PolicySchemaError::Policy(PolicyError::StaleGeneration)
        ))
    ));
    assert_eq!((charge.g(), charge.p()), (128, 0));
    assert_eq!(current.generation(), 1);
    assert!(current.data() == &minimal());
    assert_eq!(current.encoded(), [0]);
    assert_eq!(
        RetentionError::History.to_string(),
        "ERR flow retention history_max_events is outside configured bounds"
    );
    Ok(())
}

pub(super) fn assert_indexed_bodies(
    profile: PolicySchemaLimits,
    charge: &mut PolicySchemaCharge,
) -> TestResult {
    let rows: [(bool, &[u8], Kind, &[u8]); 8] = [
        (
            true,
            b"",
            Kind::IndexedAttributeEmpty,
            b"ERR flow attribute key must not be empty",
        ),
        (
            true,
            b"__x",
            Kind::IndexedAttributeReserved,
            b"ERR flow attribute key is reserved",
        ),
        (
            true,
            b" __x ",
            Kind::IndexedAttributeReserved,
            b"ERR flow attribute key is reserved",
        ),
        (
            true,
            &[b'_', b'_', 255],
            Kind::IndexedAttributeUtf8,
            b"ERR flow attribute key must be valid UTF-8",
        ),
        (
            false,
            b" \t",
            Kind::IndexedStateMetaEmpty,
            b"ERR flow state_meta key must not be empty",
        ),
        (
            false,
            b"__x",
            Kind::IndexedStateMetaReserved,
            b"ERR flow state_meta key is reserved",
        ),
        (
            false,
            b" __x ",
            Kind::IndexedStateMetaReserved,
            b"ERR flow state_meta key is reserved",
        ),
        (
            false,
            &[b'_', b'_', 255],
            Kind::IndexedStateMetaUtf8,
            b"ERR flow state_meta key must be valid UTF-8",
        ),
    ];
    for (attributes, name, kind, body) in rows {
        let supplied = indexed_input(attributes, name);
        let expected = if attributes {
            PolicySchemaError::InvalidIndexedAttributes
        } else {
            PolicySchemaError::InvalidIndexedStateMeta
        };
        assert_rejection(&supplied, 17, (kind, expected, body), profile, charge)?;
        assert!(supplied == indexed_input(attributes, name));
    }
    for ch in [
        '\u{0009}', '\u{000a}', '\u{000b}', '\u{000c}', '\u{000d}', '\u{0020}', '\u{0085}',
        '\u{00a0}', '\u{1680}', '\u{2000}', '\u{2001}', '\u{2002}', '\u{2003}', '\u{2004}',
        '\u{2005}', '\u{2006}', '\u{2007}', '\u{2008}', '\u{2009}', '\u{200a}', '\u{2028}',
        '\u{2029}', '\u{202f}', '\u{205f}', '\u{3000}',
    ] {
        let mut encoded = [0; 4];
        let text = ch.encode_utf8(&mut encoded).as_bytes();
        for attributes in [false, true] {
            let (kind, error, body) = if attributes {
                (
                    Kind::IndexedAttributeEmpty,
                    PolicySchemaError::InvalidIndexedAttributes,
                    b"ERR flow attribute key must not be empty".as_slice(),
                )
            } else {
                (
                    Kind::IndexedStateMetaEmpty,
                    PolicySchemaError::InvalidIndexedStateMeta,
                    b"ERR flow state_meta key must not be empty".as_slice(),
                )
            };
            assert_rejection(
                &indexed_input(attributes, text),
                17,
                (kind, error, body),
                profile,
                charge,
            )?;
        }
    }
    assert_indexed_neighbors(profile, charge)
}

fn assert_indexed_neighbors(
    profile: PolicySchemaLimits,
    charge: &mut PolicySchemaCharge,
) -> TestResult {
    for attributes in [false, true] {
        for name in [b"x".as_slice(), "\u{feff}".as_bytes(), b"x y", b"x__y"] {
            let supplied = indexed_input(attributes, name);
            assert!(
                check_complete_policy_replace_content(&supplied, 17, profile, charge)?.is_none()
            );
            assert!(supplied == indexed_input(attributes, name));
        }
    }
    Ok(())
}

fn indexed_input(attributes: bool, name: &[u8]) -> CompletePolicyData {
    let mut supplied = minimal();
    if attributes {
        supplied.indexed_attributes = Some(vec![name.to_vec()]);
    } else {
        supplied.indexed_state_meta = Some(name.to_vec());
    }
    supplied
}
