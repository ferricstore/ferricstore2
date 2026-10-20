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
