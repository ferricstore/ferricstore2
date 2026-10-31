//! Governance maintenance orchestration through configured semantic capabilities.

mod publication;
pub use publication::{
    CreditCatalogPublicationError, CreditCatalogPublicationLimits, CreditCatalogPublicationRequest,
    CreditCatalogPublicationStats, publish_credit_catalog,
};
