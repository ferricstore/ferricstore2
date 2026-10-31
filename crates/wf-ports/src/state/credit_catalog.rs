//! Neutral bounded publication observations; no durable/current authority flags.

use wf_domain::governance::catalog::CreditCatalogMeta;

/// One original pending publication identity in sequence order.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CreditCatalogEntry {
    /// Original positive monotone sequence.
    pub sequence: u64,
    /// Exact original credit scope within the queried tenant/group/recovery.
    pub credit_scope: Vec<u8>,
}

/// One coherent bounded queue observation; no publication or acknowledgement.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CreditCatalogPage {
    /// Actual observed queue header.
    pub meta: CreditCatalogMeta,
    /// At most256 consecutive pending entries, preserving original order.
    pub entries: Vec<CreditCatalogEntry>,
    /// Whether more entries followed this page in the same revision.
    pub more: bool,
}
