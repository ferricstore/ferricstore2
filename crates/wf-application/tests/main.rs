//! Every integration test of `wf-application` in one binary, so the suite compiles
//! and links once (ADR-012). Each module is an earlier test target, loaded
//! unchanged from its own file.

#![forbid(unsafe_code)]

#[path = "catalog_collection/main.rs"]
mod catalog_collection;
#[path = "claim_due_intent/main.rs"]
mod claim_due_intent;
#[path = "credit_cleanup/main.rs"]
mod credit_cleanup;
#[path = "independent_mutation/main.rs"]
mod independent_mutation;
#[path = "intent_comparison.rs"]
mod intent_comparison;
#[path = "kept_on_disk.rs"]
mod kept_on_disk;
#[path = "policy_intent.rs"]
mod policy_intent;
#[path = "public_claim/main.rs"]
mod public_claim;
#[path = "queue_routing.rs"]
mod queue_routing;
