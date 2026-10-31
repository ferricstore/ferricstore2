//! Actual new-owner publication, bounded observations and canonical reopen.

use super::{fixtures as credit, *};
use wf_domain::governance::catalog::CreditCatalogMeta;
use wf_ports::state::{CreditCatalogPage, StateTransactionError};

mod ack;
mod fixtures;
mod recovery;
mod sequence;
