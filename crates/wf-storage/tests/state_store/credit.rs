//! Identified owner protocol through actual disk transactions and recovery.

use super::{TestResult, fixtures as common};
use wf_domain::governance::credit::{CreditLimits, CreditOwner, CreditRejection};
use wf_domain::governance::identified::{
    CreditConfiguration, IdentifiedCreditDecision as Decision,
    IdentifiedCreditMutation as Mutation, IdentifiedCreditRejection as Rejection,
    IdentifiedCreditReply, IdentifiedCreditRequest,
};
use wf_ports::log::{decode_credit_reply, encode_credit_request};
use wf_storage::transaction::{CreditTransactionError, apply_credit_transaction};

mod bounds;
mod fixtures;
mod limits;
mod recovery;
mod sequence;
