//! Logical callers retain source values before explicit stored-value ingress.

#![forbid(unsafe_code)]

use wf_domain::value::{OriginalValueInput as I, OriginalValueNode as N};
use wf_domain::workflow::Operation;
use wf_domain::workflow::caller::{
    LogicalMutationError as E, LogicalMutationFieldValue as V, LogicalMutationInput as Input,
    LogicalMutationLimits as L, MutationCallerError as C, MutationFieldValue as S,
    PreparedLogicalMutation as P, prepare_logical_mutation as prepare,
};

mod fixtures;
mod limits;
mod normalization;
mod ownership;
mod values;

type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;
