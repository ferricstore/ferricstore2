//! Closed retained version/status associations, independent of storage codecs.
use super::{ClientIntent, RequestValueError, RetainedStatus};
use crate::workflow::indexed::IndexRejection;

pub(super) fn validate_response_profile(
    intent: ClientIntent,
    response_version: u16,
    status: RetainedStatus,
) -> Result<(), RequestValueError> {
    if intent.version() == 21 {
        return if response_version == 18
            && matches!(
                status,
                RetainedStatus::Accepted
                    | RetainedStatus::ClaimRejected(_)
                    | RetainedStatus::GovernanceRejected(_)
            ) {
            Ok(())
        } else {
            Err(RequestValueError::UnsupportedVersion)
        };
    }
    if intent.version() == 20 {
        return if response_version == 17
            && matches!(
                status,
                RetainedStatus::Accepted | RetainedStatus::ClaimRejected(_)
            ) {
            Ok(())
        } else {
            Err(RequestValueError::UnsupportedVersion)
        };
    }
    if intent.version() == 18 {
        return if response_version == 15
            && matches!(
                status,
                RetainedStatus::Accepted | RetainedStatus::WorkflowReleaseRejected(_)
            ) {
            Ok(())
        } else {
            Err(RequestValueError::UnsupportedVersion)
        };
    }
    if matches!(status, RetainedStatus::WorkflowReleaseRejected(_)) {
        return Err(RequestValueError::UnsupportedVersion);
    }
    if intent.version() == 19 {
        return if response_version == 16
            && matches!(
                status,
                RetainedStatus::Accepted | RetainedStatus::ClaimRejected(_)
            ) {
            Ok(())
        } else {
            Err(RequestValueError::UnsupportedVersion)
        };
    }
    if status
        == RetainedStatus::ClaimRejected(
            crate::claim::apply::ClaimRejection::GoverningReservationUnavailable,
        )
    {
        return Err(RequestValueError::UnsupportedVersion);
    }
    if matches!(
        status,
        RetainedStatus::IndexedRejected(IndexRejection::ReleaseBacklogFull)
    ) && (intent.version() != 9 || !matches!(response_version, 5 | 6 | 8))
    {
        return Err(RequestValueError::UnsupportedVersion);
    }
    validate_legacy_response_profile(intent, response_version, status)
}

fn validate_legacy_response_profile(
    intent: ClientIntent,
    response_version: u16,
    status: RetainedStatus,
) -> Result<(), RequestValueError> {
    if intent.version() == 16 {
        return if response_version == 14 && status == RetainedStatus::Accepted {
            Ok(())
        } else {
            Err(RequestValueError::UnsupportedVersion)
        };
    }
    if intent.version() == 15 {
        return if response_version == 13 && status == RetainedStatus::Accepted {
            Ok(())
        } else {
            Err(RequestValueError::UnsupportedVersion)
        };
    }
    if intent.version() == 14 {
        return if response_version == 12 && status == RetainedStatus::Accepted {
            Ok(())
        } else {
            Err(RequestValueError::UnsupportedVersion)
        };
    }
    if intent.version() == 13 {
        return if response_version == 11
            && matches!(
                status,
                RetainedStatus::Accepted | RetainedStatus::CreditCatalogRejected(_)
            ) {
            Ok(())
        } else {
            Err(RequestValueError::UnsupportedVersion)
        };
    }
    if matches!(status, RetainedStatus::CreditCatalogRejected(_)) {
        return Err(RequestValueError::UnsupportedVersion);
    }
    if matches!(intent.version(), 12 | 17) {
        return if response_version == 10
            && matches!(
                status,
                RetainedStatus::Accepted | RetainedStatus::GovernanceRejected(_)
            ) {
            Ok(())
        } else {
            Err(RequestValueError::UnsupportedVersion)
        };
    }
    if matches!(status, RetainedStatus::GovernanceRejected(_)) {
        return Err(RequestValueError::UnsupportedVersion);
    }
    if matches!(intent.version(), 10 | 11) {
        return if response_version == 9
            && matches!(
                status,
                RetainedStatus::Accepted | RetainedStatus::IndexedRejected(_)
            ) {
            Ok(())
        } else {
            Err(RequestValueError::UnsupportedVersion)
        };
    }
    if matches!(intent.version(), 8 | 9)
        && (!matches!(response_version, 5..=8)
            || !matches!(
                status,
                RetainedStatus::Accepted | RetainedStatus::IndexedRejected(_)
            ))
    {
        return Err(RequestValueError::UnsupportedVersion);
    }
    if !matches!(intent.version(), 8 | 9)
        && response_version != 1
        && !(response_version == 2 && intent.version() == 2 && status == RetainedStatus::Accepted)
        && !(response_version == 2
            && intent.version() == 1
            && matches!(
                status,
                RetainedStatus::Accepted | RetainedStatus::IndexedRejected(_)
            ))
        && !(response_version == 3
            && intent.version() == 1
            && matches!(
                status,
                RetainedStatus::Accepted | RetainedStatus::ClaimRejected(_)
            ))
        && !(response_version == 4
            && intent.version() == 7
            && matches!(
                status,
                RetainedStatus::Accepted | RetainedStatus::IndexedRejected(_)
            ))
    {
        return Err(RequestValueError::UnsupportedVersion);
    }
    Ok(())
}
