//! Original retained responses use exact ETF and the original error grammar.

use super::{PolicySchemaCharge, PolicySchemaLimits, TestResult, hex, minimal};
use wf_domain::policy::complete::{
    FlowPolicyInputRejection, OriginalEtfSafety, PolicyRetentionInputRejection,
    decode_original_complete_policy_entry,
};
use wf_domain::policy::installation::PolicyError;
use wf_domain::workflow::request::{PolicyRejection, RetainedStatus};
use wf_ports::log::{CodecError, original_policy_set_response_status};

fn status(response: &[u8]) -> Result<RetainedStatus, CodecError> {
    let schema = PolicySchemaLimits::default();
    let mut charge = PolicySchemaCharge::new(schema);
    original_policy_set_response_status(
        response,
        schema,
        &mut charge,
        65_536 + 256 * response.len(),
    )
}

#[test]
fn accepted_original_etf_and_dynamic_rejections_restore_exactly() -> TestResult {
    let raw = hex(
        "836803770e666c6f775f706f6c6963795f7631610174000000027704747970656d0000000172770776657273696f6e6e11000100000000000000000000000000000001",
    )?;
    let schema = PolicySchemaLimits::default();
    let mut charge = PolicySchemaCharge::new(schema);
    let safety = OriginalEtfSafety::new(&["flow_policy_v1", "type", "version"], &[], None, &[]);
    let entry = decode_original_complete_policy_entry(&raw, &safety, schema, &mut charge)?
        .ok_or("independent original BigInteger receipt fixture")?;
    assert_eq!(entry.generation(), 1);
    assert_eq!(entry.workflow_type(), Some(b"r".as_slice()));
    assert_eq!(status(&raw)?, RetainedStatus::Accepted);
    assert_eq!(status(&minimal(1)?)?, RetainedStatus::Accepted);
    // Generation zero is a valid current entry, never an allocated success.
    assert!(status(&minimal(0)?).is_err());
    for error in [
        PolicyError::InvalidGeneration,
        PolicyError::StaleGeneration,
        PolicyError::GenerationExhausted,
    ] {
        let body = match error {
            PolicyError::InvalidGeneration => b"ERR invalid flow policy generation".as_slice(),
            PolicyError::StaleGeneration => b"ERR stale flow policy generation",
            _ => b"ERR flow policy generation exhausted",
        };
        assert_eq!(
            status(body)?,
            RetainedStatus::PolicyRejected(PolicyRejection::new(error)?)
        );
    }
    for maximum in [1, 64, 1_000_000] {
        let body = format!("ERR flow retention history_max_events must be between 1 and {maximum}");
        assert_eq!(
            status(body.as_bytes())?,
            RetainedStatus::OriginalPolicySetRejected(FlowPolicyInputRejection::Retention(
                PolicyRetentionInputRejection::History { maximum }
            ),)
        );
    }
    for suffix in ["", "0", "064", "+64", " 64", "1000001", "64\n"] {
        let body = format!("ERR flow retention history_max_events must be between 1 and {suffix}");
        assert!(status(body.as_bytes()).is_err());
    }
    assert!(status(b"ERR flow governance unit must be a positive integer").is_err());
    assert!(status(b"ERR unrelated error").is_err());
    let mut unsupported = minimal(1)?;
    // Independent ETF positions: root map starts21; its states atom starts28.
    assert_eq!(&unsupported[28..34], b"states");
    unsupported[28] = b'x';
    assert!(matches!(
        status(&unsupported),
        Err(CodecError::UnsupportedPolicyProfile)
    ));
    let mut unsupported_charge = PolicySchemaCharge::new(schema);
    assert!(matches!(
        original_policy_set_response_status(
            &unsupported,
            schema,
            &mut unsupported_charge,
            65_536 + 256 * unsupported.len(),
        ),
        Err(CodecError::UnsupportedPolicyProfile)
    ));
    // Full structural EOF/envelope is checked on the first lexical walk. An
    // unsupported atom must refuse before any Index/catalog allocation.
    assert_eq!(unsupported_charge.p(), 0);
    unsupported.push(0);
    assert!(matches!(status(&unsupported), Err(CodecError::Malformed)));
    for end in 0..raw.len() {
        assert!(status(&raw[..end]).is_err());
    }
    let mut trailing = raw;
    trailing.push(0);
    assert!(matches!(status(&trailing), Err(CodecError::Malformed)));
    Ok(())
}

fn entire_raw() -> &'static [u8] {
    // Independent ETF: gen1, limits.budget.limit1, queued FIFO, type r, and
    // customer version2^128+1. No product encoder supplies expected bytes.
    b"\x83\x68\x03\x77\x0eflow_policy_v1\x61\x01\x74\0\0\0\x04\
      \x77\x0agovernance\x74\0\0\0\x01\x77\x06limits\x74\0\0\0\x01\
      \x6d\0\0\0\x06budget\x74\0\0\0\x01\x77\x05limit\x61\x01\
      \x77\x06states\x74\0\0\0\x01\x6d\0\0\0\x06queued\
      \x74\0\0\0\x01\x77\x04mode\x77\x04fifo\
      \x77\x04type\x6d\0\0\0\x01r\
      \x77\x07version\x6e\x11\0\x01\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\x01"
}

#[test]
fn response_correspondence_rejects_legal_wrong_install_and_context() -> TestResult {
    use super::super::profile;
    use super::{Raw, empty_orders, frame, scope};
    use wf_domain::policy::complete::OriginalPolicyReadValue as ReadValue;
    use wf_ports::log::original_policy_set_response_matches;

    let schema = profile(2_000_000_000)?;
    let namespace = scope()?;
    let original = entire_raw();
    let replace = [Raw::Atom("replace"), Raw::Atom("true")];
    let guard = [Raw::Atom("expected_generation"), Raw::Integer(0)];
    let opts = [Raw::Tuple(&replace), Raw::Tuple(&guard)];
    for rejected in [false, true] {
        let bytes = frame(
            Raw::List(if rejected { &opts } else { &opts[..1] }),
            &empty_orders(),
        )?;
        let mut charge = PolicySchemaCharge::new(schema);
        let plan = super::point::point_plan(
            &bytes,
            &namespace,
            super::point::authority(ReadValue::Binary(original)),
            schema,
            &mut charge,
        )?;
        assert!(original_policy_set_response_matches(
            &plan,
            schema,
            &mut charge,
            1_000_000
        )?);
        if rejected {
            assert_eq!(
                super::retained(&plan)?.response(),
                b"ERR stale flow policy generation"
            );
        } else {
            super::accepted(&plan, 1, 2, &minimal(2)?)?;
        }
        for index in 0..3 {
            let other = changed_field(original, index)?;
            // Each alternate is a complete legal same-generation/type value.
            assert_eq!(status(&other)?, RetainedStatus::Accepted);
            assert_changes_correspondence(&bytes, original, &other, schema)?;
        }
        assert_changes_arguments(&bytes, original, schema, !rejected)?;
        if !rejected {
            assert_late_aggregate(&bytes, original, schema)?;
        }
    }
    assert_query_correspondence(original, schema)?;
    assert_absence_correspondence(original, schema)?;
    Ok(())
}

fn assert_late_aggregate(bytes: &[u8], original: &[u8], schema: PolicySchemaLimits) -> TestResult {
    use super::{AdmissionLimits, ControlState, ReadValue, scope, stamp};
    use wf_ports::state::{GroupOriginalPolicyChanges, StateTransactionError};
    let namespace = scope()?;
    // Accepted replacement is independent minimal ETF51, key14, four scoped
    // identity bytes. The actual late producer owns TWO separate ETF copies.
    let exact = 2 * 51 + b"f:{f}:policy:r".len() + 4;
    assert_eq!(exact, 120);
    for maximum in [exact - 1, exact] {
        let mut charge = PolicySchemaCharge::new(schema);
        let plan = super::point::point_plan(
            bytes,
            &namespace,
            super::point::authority(ReadValue::Binary(original)),
            schema,
            &mut charge,
        )?;
        let result = GroupOriginalPolicyChanges::new(
            &namespace,
            ControlState::initial(stamp(0)?),
            Some(original),
            plan,
            (AdmissionLimits::new(1_048_576, maximum, 1000)?, schema),
            &mut charge,
            1_000_000,
        );
        if maximum == exact {
            let changes = result?;
            assert_eq!(
                changes
                    .outcome()
                    .ok_or("missing Fresh response")?
                    .response(),
                minimal(2)?
            );
        } else {
            assert_eq!(result.err(), Some(StateTransactionError::Limit));
        }
    }
    Ok(())
}

fn changed_field(original: &[u8], index: usize) -> TestResult<Vec<u8>> {
    let mut changed = original.to_vec();
    match index {
        0 => *changed.last_mut().ok_or("version magnitude")? = 2,
        1 => {
            let start = original
                .windows(6)
                .position(|w| w == b"queued")
                .ok_or("state name")?;
            changed[start + 5] = b'e';
        }
        _ => {
            let field = b"\x77\x05limit\x61\x01";
            let start = original
                .windows(field.len())
                .position(|w| w == field)
                .ok_or("limit field")?;
            changed[start + field.len() - 1] = 2;
        }
    }
    assert_ne!(changed, original);
    Ok(changed)
}

fn assert_changes_correspondence(
    bytes: &[u8],
    original: &[u8],
    other: &[u8],
    schema: PolicySchemaLimits,
) -> TestResult {
    use super::{AdmissionLimits, ControlState, ReadValue, scope, stamp};
    use wf_ports::state::{GroupOriginalPolicyChanges, StateTransactionError};

    let namespace = scope()?;
    let base = ControlState::initial(stamp(0)?);
    for supplied in [original, other] {
        let mut charge = PolicySchemaCharge::new(schema);
        let plan = super::point::point_plan(
            bytes,
            &namespace,
            super::point::authority(ReadValue::Binary(original)),
            schema,
            &mut charge,
        )?;
        let result = GroupOriginalPolicyChanges::new(
            &namespace,
            base,
            Some(supplied),
            plan,
            (AdmissionLimits::default(), schema),
            &mut charge,
            1_000_000,
        );
        if supplied == original {
            let changes = result?;
            assert_eq!(changes.scope(), &namespace);
            assert_eq!(changes.expected(), base);
            assert_eq!(changes.predecessor(), Some(original));
        } else {
            assert_eq!(result.err(), Some(StateTransactionError::InvalidChanges));
        }
    }
    // Reverse the donor as well: the alternate is valid preparation authority,
    // so rejection above cannot be explained by an invalid fixture.
    let mut charge = PolicySchemaCharge::new(schema);
    let plan = super::point::point_plan(
        bytes,
        &namespace,
        super::point::authority(ReadValue::Binary(other)),
        schema,
        &mut charge,
    )?;
    GroupOriginalPolicyChanges::new(
        &namespace,
        base,
        Some(other),
        plan,
        (AdmissionLimits::default(), schema),
        &mut charge,
        1_000_000,
    )?;
    Ok(())
}

fn assert_changes_arguments(
    bytes: &[u8],
    original: &[u8],
    schema: PolicySchemaLimits,
    accepted: bool,
) -> TestResult {
    use super::{AdmissionLimits, ControlState, ReadValue, scope, stamp};
    use wf_domain::workflow::request::RequestScope;
    use wf_ports::state::{GroupOriginalPolicyChanges, StateTransactionError};
    use wf_types::identity::TenantId;

    let namespace = scope()?;
    let foreign = RequestScope::new(
        TenantId::new(b"other".to_vec())?,
        namespace.group().clone(),
        namespace.recovery().clone(),
    );
    // Only accepted response decoding reaches the temporary ETF owner cap.
    for case in 0..if accepted { 5 } else { 4 } {
        let mut charge = PolicySchemaCharge::new(schema);
        let plan = super::point::point_plan(
            bytes,
            &namespace,
            super::point::authority(ReadValue::Binary(original)),
            schema,
            &mut charge,
        )?;
        let mut wrong = PolicySchemaCharge::new(super::super::profile(2_000_000_001)?);
        let result = GroupOriginalPolicyChanges::new(
            if case == 0 { &foreign } else { &namespace },
            ControlState::initial(stamp(u64::from(case == 1))?),
            if case == 2 { None } else { Some(original) },
            plan,
            (AdmissionLimits::default(), schema),
            if case == 3 { &mut wrong } else { &mut charge },
            if case == 4 { 1 } else { 1_000_000 },
        );
        assert_eq!(
            result.err(),
            Some(if case == 4 {
                StateTransactionError::Limit
            } else {
                StateTransactionError::InvalidChanges
            }),
        );
        assert_eq!((wrong.g(), wrong.p()), (0, 0));
    }
    Ok(())
}

fn assert_query_correspondence(original: &[u8], schema: PolicySchemaLimits) -> TestResult {
    use super::{
        AdmissionLimits, ControlState, Raw, ReadValue, empty_orders, frame_for_type, scope, stamp,
    };
    use wf_ports::state::{GroupOriginalPolicyChanges, StateTransactionError};

    let field = b"\x77\x04type\x6d\0\0\0\x01r";
    let start = original
        .windows(field.len())
        .position(|w| w == field)
        .ok_or("type field")?;
    let mut foreign = original.to_vec();
    foreign[start + field.len() - 1] = b's';
    assert_eq!(status(&foreign)?, RetainedStatus::Accepted);
    let namespace = scope()?;
    let replace = [Raw::Atom("replace"), Raw::Atom("true")];
    let opts = [Raw::Tuple(&replace)];
    let bytes = frame_for_type(b"s", Raw::List(&opts), &empty_orders(), b"q", 0)?;
    for supplied in [foreign.as_slice(), original] {
        let mut charge = PolicySchemaCharge::new(schema);
        let plan = super::point::point_plan(
            &bytes,
            &namespace,
            super::point::authority(ReadValue::Binary(&foreign)),
            schema,
            &mut charge,
        )?;
        assert_eq!(plan.workflow_type(), Some(b"s".as_slice()));
        let result = GroupOriginalPolicyChanges::new(
            &namespace,
            ControlState::initial(stamp(0)?),
            Some(supplied),
            plan,
            (AdmissionLimits::default(), schema),
            &mut charge,
            1_000_000,
        );
        if supplied == foreign {
            result?;
        } else {
            assert_eq!(result.err(), Some(StateTransactionError::InvalidChanges));
        }
    }
    Ok(())
}

fn assert_absence_correspondence(original: &[u8], schema: PolicySchemaLimits) -> TestResult {
    use super::{AdmissionLimits, ControlState, Raw, ReadValue, empty_orders, frame, scope, stamp};
    use wf_ports::state::{GroupOriginalPolicyChanges, StateTransactionError};

    let namespace = scope()?;
    let guard = [Raw::Atom("expected_generation"), Raw::Integer(1)];
    let opts = [Raw::Tuple(&guard)];
    for rejected in [false, true] {
        let bytes = frame(
            Raw::List(if rejected { &opts } else { &[] }),
            &empty_orders(),
        )?;
        for supplied in [None, Some(original)] {
            let mut charge = PolicySchemaCharge::new(schema);
            let plan = super::point::point_plan(
                &bytes,
                &namespace,
                super::point::authority(ReadValue::Absent),
                schema,
                &mut charge,
            )?;
            assert!(plan.matches_predecessor(None));
            if rejected {
                assert_eq!(
                    super::retained(&plan)?.response(),
                    b"ERR stale flow policy generation"
                );
            } else {
                super::accepted(&plan, 0, 1, &minimal(1)?)?;
            }
            let result = GroupOriginalPolicyChanges::new(
                &namespace,
                ControlState::initial(stamp(0)?),
                supplied,
                plan,
                (AdmissionLimits::default(), schema),
                &mut charge,
                1_000_000,
            );
            if supplied.is_none() {
                result?;
            } else {
                assert_eq!(result.err(), Some(StateTransactionError::InvalidChanges));
            }
        }
    }
    Ok(())
}
