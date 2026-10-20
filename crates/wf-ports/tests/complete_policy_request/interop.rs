//! Core ETF receipts retain their own version without widening the old grammar.

use crate::{TestResult, profile};
use wf_domain::policy::complete::{
    PolicySchemaCharge, PolicySchemaLimits, decode_original_complete_policy_entry_portable,
};
use wf_domain::policy::installation::{MAX_POLICY_GENERATION, PolicyError};
use wf_domain::workflow::admission::AdmissionLimits;
use wf_domain::workflow::request::{
    ClientIntent, PolicyRejection, RequestScope, RequestValueError, RetainedMetadata,
    RetainedOutcome, RetainedStatus,
};
use wf_ports::log::{
    CodecError, complete_core_policy_patch_response_matches,
    complete_core_policy_patch_retained_status, original_policy_set_response_status,
};
use wf_types::identity::{GroupId, RecoveryId, RequestId, TenantId};
use wf_types::version::{AppliedPosition, LogIdentity, TimestampMillis};

mod fresh;
mod retained;

fn stamp(now: u64) -> TestResult<wf_types::version::CommittedTime> {
    Ok(wf_types::version::CommittedTime::new(
        wf_types::version::HlcTimestamp::new(now, 0)?,
        now,
    )?)
}

fn log(index: u64) -> LogIdentity {
    LogIdentity::new(1, 7, AppliedPosition::new(index))
}

fn frame(
    now: u64,
    expected: Option<u64>,
    patch: wf_domain::policy::definition::PolicyPatch,
) -> TestResult<Vec<u8>> {
    use wf_domain::policy::complete::{
        CompleteCorePolicyPatchInput, CompleteCorePolicyPatchRequestInput,
    };
    let schema = profile(100_000_000)?;
    Ok(wf_ports::log::prepare_complete_core_policy_patch_frame(
        CompleteCorePolicyPatchRequestInput::new(
            metadata(RetainedStatus::Accepted)?.scope,
            RequestId::new(b"q".to_vec())?,
            stamp(now)?,
            CompleteCorePolicyPatchInput {
                workflow_type: b"r".to_vec(),
                expected_generation: expected,
                patch,
            },
        ),
        schema,
        AdmissionLimits::default(),
        &mut PolicySchemaCharge::new(schema),
    )?
    .as_bytes()
    .to_vec())
}

// Independent ETF envelope: gen1, type r, customer integer 2^128+1.
const BIG_VERSION: &[u8] = b"\x83\x68\x03\x77\x0eflow_policy_v1\x61\x01\
    \x74\0\0\0\x02\x77\x04type\x6d\0\0\0\x01r\
    \x77\x07version\x6e\x11\0\x01\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\x01";
const COMPLETE: &[u8] = b"FWPV\0\x02\0\0\0\0\0\0\0\x01\0\0\0\x16\
    FWPS\0\x02\0\0\0\x01r\0\0\0\0\0\0\0\0\0\0\0";

fn original(generation: u64) -> TestResult<Vec<u8>> {
    let mut raw = b"\x83\x68\x03\x77\x0eflow_policy_v1".to_vec();
    if let Ok(value) = u8::try_from(generation) {
        raw.extend_from_slice(&[97, value]);
    } else if let Ok(value) = i32::try_from(generation) {
        raw.push(98);
        raw.extend_from_slice(&value.to_be_bytes());
    } else {
        let bytes = generation.to_le_bytes();
        let length = bytes
            .iter()
            .rposition(|&byte| byte != 0)
            .ok_or("zero big")?
            + 1;
        raw.extend_from_slice(&[110, u8::try_from(length)?, 0]);
        raw.extend_from_slice(&bytes[..length]);
    }
    raw.extend_from_slice(b"\x74\0\0\0\x02\x77\x06states\x74\0\0\0\0\x77\x04type\x6d\0\0\0\x01r");
    Ok(raw)
}

fn metadata(status: RetainedStatus) -> TestResult<RetainedMetadata> {
    Ok(RetainedMetadata {
        scope: RequestScope::new(
            TenantId::new(b"t".to_vec())?,
            GroupId::new(b"g".to_vec())?,
            RecoveryId::new(b"r".to_vec())?,
        ),
        request: RequestId::new(b"q".to_vec())?,
        intent: ClientIntent::complete_core_policy_patch([7; 32]),
        original_position: LogIdentity::new(1, 7, AppliedPosition::new(42)),
        original_time: TimestampMillis::new(10),
        expires_at: TimestampMillis::new(20),
        status,
    })
}

fn status(
    version: u16,
    raw: &[u8],
    schema: PolicySchemaLimits,
    charge: &mut PolicySchemaCharge,
    owner: usize,
) -> Result<RetainedStatus, CodecError> {
    complete_core_policy_patch_retained_status(
        ClientIntent::complete_core_policy_patch([7; 32]),
        version,
        raw,
        (schema, AdmissionLimits::default()),
        charge,
        owner,
    )
}

#[test]
fn core_etf_response_v2_binds_full_raw_result_and_original_predecessor() -> TestResult {
    let schema = profile(100_000_000)?;
    assert_eq!((original(1)?.len(), BIG_VERSION.len()), (51, 67));
    for raw in [original(1)?, BIG_VERSION.to_vec()] {
        let owner = 65_536 + 256 * raw.len();
        // The existing intent4 path proves this independent fixture is healthy
        // and supplies the frozen one-decoder conservation control.
        let mut prior = PolicySchemaCharge::new(schema);
        assert_eq!(
            original_policy_set_response_status(&raw, schema, &mut prior, owner)?,
            RetainedStatus::Accepted
        );
        let mut charge = PolicySchemaCharge::new(schema);
        assert_eq!(
            status(2, &raw, schema, &mut charge, owner)?,
            RetainedStatus::Accepted
        );
        assert_eq!((charge.g(), charge.p()), (prior.g(), prior.p()));
        assert!(charge.g() > 0);
        assert_metadata(&raw, schema)?;
    }
    assert_versions_and_owners(schema)?;
    assert_invalid_etf(schema)?;
    Ok(())
}

fn assert_metadata(raw: &[u8], schema: PolicySchemaLimits) -> TestResult {
    let wanted = metadata(RetainedStatus::Accepted)?;
    let outcome =
        RetainedOutcome::new(wanted.clone(), 2, raw.to_vec(), AdmissionLimits::default())?;
    assert_eq!(outcome.scope(), &wanted.scope);
    assert_eq!(outcome.request(), &wanted.request);
    assert_eq!(outcome.intent(), wanted.intent);
    assert_eq!(outcome.original_position(), wanted.original_position);
    assert_eq!(outcome.original_time(), wanted.original_time);
    assert_eq!(outcome.expires_at(), wanted.expires_at);
    assert_eq!(outcome.status(), RetainedStatus::Accepted);
    assert_eq!(outcome.response_version(), 2);
    assert_eq!(outcome.response(), raw);
    // A genuine accepted response without any proposed install cannot match.
    assert!(!complete_core_policy_patch_response_matches(
        &(None, outcome),
        (schema, AdmissionLimits::default()),
        &mut PolicySchemaCharge::new(schema),
        65_536 + 256 * raw.len(),
    )?);
    let rejected =
        RetainedStatus::PolicyRejected(PolicyRejection::new(PolicyError::StaleGeneration)?);
    assert_eq!(
        RetainedOutcome::new(
            metadata(rejected)?,
            2,
            b"ERR stale flow policy generation".to_vec(),
            AdmissionLimits::default(),
        )
        .err(),
        Some(RequestValueError::UnsupportedVersion)
    );
    let cap = AdmissionLimits::new(1_048_576, raw.len() - 1, 1000)?;
    assert_eq!(
        RetainedOutcome::new(wanted, 2, raw.to_vec(), cap).err(),
        Some(RequestValueError::ResponseTooLarge)
    );
    Ok(())
}

fn assert_versions_and_owners(schema: PolicySchemaLimits) -> TestResult {
    let raw = original(1)?;
    let owner = 65_536 + 256 * raw.len();
    let mut charge = PolicySchemaCharge::new(schema);
    assert_eq!(
        status(2, &raw, schema, &mut charge, owner - 1),
        Err(CodecError::ByteLimit)
    );
    assert_eq!((charge.g(), charge.p()), (0, 0));
    for (version, body) in [(0, raw.as_slice()), (3, &raw), (2, COMPLETE)] {
        let mut charge = PolicySchemaCharge::new(schema);
        assert_eq!(
            status(version, body, schema, &mut charge, owner),
            Err(CodecError::UnsupportedVersion)
        );
        assert_eq!((charge.g(), charge.p()), (0, 0));
    }
    assert_eq!(
        status(
            1,
            COMPLETE,
            schema,
            &mut PolicySchemaCharge::new(schema),
            2048 + 216 * COMPLETE.len(),
        )?,
        RetainedStatus::Accepted
    );
    let error = b"ERR stale flow policy generation";
    assert_eq!(
        status(1, error, schema, &mut PolicySchemaCharge::new(schema), 0)?,
        RetainedStatus::PolicyRejected(PolicyRejection::new(PolicyError::StaleGeneration,)?)
    );
    assert_eq!(
        status(2, error, schema, &mut PolicySchemaCharge::new(schema), 0),
        Err(CodecError::UnsupportedVersion)
    );
    Ok(())
}

fn assert_invalid_etf(schema: PolicySchemaLimits) -> TestResult {
    let zero = original(0)?;
    let entry = decode_original_complete_policy_entry_portable(
        &zero,
        schema,
        &mut PolicySchemaCharge::new(schema),
    )?
    .ok_or("valid present generation-zero fixture")?;
    assert_eq!(entry.generation(), 0);
    for raw in [zero, original(MAX_POLICY_GENERATION + 1)?] {
        assert_eq!(
            status(
                2,
                &raw,
                schema,
                &mut PolicySchemaCharge::new(schema),
                65_536 + 256 * raw.len(),
            ),
            Err(CodecError::Malformed)
        );
    }
    let mut unsupported = original(1)?;
    assert_eq!(&unsupported[28..34], b"states");
    unsupported[28] = b'x';
    let mut charge = PolicySchemaCharge::new(schema);
    assert_eq!(
        status(2, &unsupported, schema, &mut charge, 100_000),
        Err(CodecError::UnsupportedPolicyProfile)
    );
    assert_eq!(charge.p(), 0);
    unsupported.push(0);
    assert_eq!(
        status(
            2,
            &unsupported,
            schema,
            &mut PolicySchemaCharge::new(schema),
            100_000
        ),
        Err(CodecError::Malformed)
    );
    let raw = original(1)?;
    for end in 1..raw.len() {
        assert!(
            status(
                2,
                &raw[..end],
                schema,
                &mut PolicySchemaCharge::new(schema),
                100_000
            )
            .is_err()
        );
    }
    let mut oversized = vec![0; 263_169];
    oversized[0] = 131;
    let mut charge = PolicySchemaCharge::new(schema);
    assert_eq!(
        status(2, &oversized, schema, &mut charge, 0),
        Err(CodecError::Malformed)
    );
    assert_eq!((charge.g(), charge.p()), (0, 0));
    Ok(())
}
