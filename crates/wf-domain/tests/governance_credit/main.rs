//! Pinned governing-credit model traces and identified-owner safety controls.
#![forbid(unsafe_code)]

use wf_domain::governance::credit::{
    CreditDecision, CreditError, CreditLeaseData, CreditLimits, CreditOperation as Op, CreditOwner,
    CreditOwnerData, CreditRejection, CreditReservationValue as Value, decide_credit,
    new_credit_owner, restore_credit_owner,
};
use wf_domain::workflow::control::PreparedDigest;
use wf_types::version::TimestampMillis;

type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;
mod catalog;
mod fixtures;
mod identified;
mod limits;
mod membership;
mod protocol;
mod source;
mod view;
