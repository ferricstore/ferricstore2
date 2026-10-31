use super::TestResult;
use wf_domain::governance::catalog::{
    CreditCatalogAckRejection as R, CreditCatalogAckReply as Reply,
    CreditCatalogAckRequest as Request, CreditCatalogMeta as Meta, prepare_catalog_ack,
};
use wf_domain::workflow::MAX_EXACT_MILLIS as MAX;

#[test]
fn catalog_ack_plan_keeps_original_controls_and_exact_bounded_deletion_effects() -> TestResult {
    let meta = Meta::restore(1, 5)?;
    let request = Request {
        expected_head: 1,
        up_to: 2,
    };
    request.validate()?;
    let plan = prepare_catalog_ack(meta, request);
    assert_eq!(plan.original(), meta);
    assert_eq!(plan.request(), request);
    assert_eq!(
        plan.reply(),
        Reply::Accepted {
            meta: Meta::restore(3, 5)?,
            deleted: 2
        }
    );
    assert_eq!(
        plan.acknowledgement()
            .ok_or("missing effects")?
            .sequences()
            .collect::<Vec<_>>(),
        [1, 2]
    );
    let full = prepare_catalog_ack(
        Meta::restore(1, 257)?,
        Request {
            expected_head: 1,
            up_to: 256,
        },
    );
    assert_eq!(
        full.acknowledgement().ok_or("missing256 effects")?.len(),
        256
    );
    Ok(())
}

#[test]
fn catalog_ack_plan_retries_precede_stale_but_command_shape_precedes_retry() -> TestResult {
    let meta = Meta::restore(3, 5)?;
    assert_eq!(
        prepare_catalog_ack(
            meta,
            Request {
                expected_head: 1,
                up_to: 2
            }
        )
        .reply(),
        Reply::Accepted { meta, deleted: 0 }
    );
    assert_eq!(
        prepare_catalog_ack(
            meta,
            Request {
                expected_head: 1,
                up_to: 3
            }
        )
        .reply(),
        Reply::Rejected {
            meta,
            reason: R::StaleCursor
        }
    );
    assert_eq!(
        prepare_catalog_ack(
            meta,
            Request {
                expected_head: 3,
                up_to: 6
            }
        )
        .reply(),
        Reply::Rejected {
            meta,
            reason: R::InvalidRange
        }
    );
    let advanced = Meta::restore(300, 400)?;
    assert_eq!(
        prepare_catalog_ack(
            advanced,
            Request {
                expected_head: 1,
                up_to: 258
            }
        )
        .reply(),
        Reply::Rejected {
            meta: advanced,
            reason: R::InvalidRange
        }
    );
    Ok(())
}

#[test]
fn catalog_ack_plan_preserves_last_exact_head_and_rejects_invalid_original_controls() -> TestResult
{
    let last = prepare_catalog_ack(
        Meta::restore(MAX, MAX)?,
        Request {
            expected_head: MAX,
            up_to: MAX,
        },
    );
    assert_eq!(
        last.reply(),
        Reply::Accepted {
            meta: Meta::restore(MAX + 1, MAX)?,
            deleted: 1
        }
    );
    let retry = prepare_catalog_ack(
        Meta::restore(MAX + 1, MAX)?,
        Request {
            expected_head: MAX,
            up_to: MAX,
        },
    );
    assert_eq!(
        retry.acknowledgement().ok_or("missing noop effects")?.len(),
        0
    );
    for request in [
        Request {
            expected_head: 0,
            up_to: 1,
        },
        Request {
            expected_head: 1,
            up_to: 0,
        },
        Request {
            expected_head: 2,
            up_to: 1,
        },
        Request {
            expected_head: 1,
            up_to: 257,
        },
        Request {
            expected_head: MAX,
            up_to: MAX + 1,
        },
    ] {
        assert_eq!(request.validate(), Err(R::InvalidRange));
        let plan = prepare_catalog_ack(Meta::empty(), request);
        assert!(plan.acknowledgement().is_none());
        assert_eq!(
            plan.reply(),
            Reply::Rejected {
                meta: Meta::empty(),
                reason: R::InvalidRange
            }
        );
    }
    Ok(())
}

#[test]
fn catalog_ack_retained_business_status_cannot_escape_the_exact_intent_response_profile()
-> TestResult {
    use wf_domain::workflow::{
        admission::AdmissionLimits,
        request::{ClientIntent, RequestScope, RetainedMetadata, RetainedOutcome, RetainedStatus},
    };
    use wf_types::{
        identity::{GroupId, RecoveryId, RequestId, TenantId},
        version::{AppliedPosition, LogIdentity, TimestampMillis},
    };
    let scope = RequestScope::new(
        TenantId::new(b"t".to_vec())?,
        GroupId::new(b"g".to_vec())?,
        RecoveryId::new(b"r".to_vec())?,
    );
    let mut metadata = RetainedMetadata {
        scope,
        request: RequestId::new(b"ack".to_vec())?,
        intent: ClientIntent::credit_catalog_ack([0; 32]),
        original_position: LogIdentity::new(1, 2, AppliedPosition::new(0)),
        original_time: TimestampMillis::new(1000),
        expires_at: TimestampMillis::new(2000),
        status: RetainedStatus::CreditCatalogRejected(R::InvalidRange),
    };
    let mut bytes = b"FWCA\0\x01\x02".to_vec();
    bytes.extend_from_slice(&1_u64.to_be_bytes());
    bytes.extend_from_slice(&0_u64.to_be_bytes());
    bytes.extend_from_slice(&0_u16.to_be_bytes());
    assert!(
        RetainedOutcome::new(
            metadata.clone(),
            11,
            bytes.clone(),
            AdmissionLimits::default()
        )
        .is_ok()
    );
    assert!(
        RetainedOutcome::new(
            metadata.clone(),
            10,
            bytes.clone(),
            AdmissionLimits::default()
        )
        .is_err()
    );
    metadata.intent = ClientIntent::governance_credit([0; 32]);
    assert!(
        RetainedOutcome::new(
            metadata.clone(),
            10,
            bytes.clone(),
            AdmissionLimits::default()
        )
        .is_err()
    );
    metadata.intent = ClientIntent::credit_catalog_ack([0; 32]);
    metadata.status = RetainedStatus::GovernanceRejected(
        wf_domain::governance::identified::IdentifiedCreditRejection::CatalogBacklogFull,
    );
    assert!(RetainedOutcome::new(metadata, 11, bytes, AdmissionLimits::default()).is_err());
    Ok(())
}
