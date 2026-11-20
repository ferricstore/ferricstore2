//! Every integration test of `wf-ports` in one binary, so the suite compiles
//! and links once (ADR-012). Each module is an earlier test target, loaded
//! unchanged from its own file.

#![forbid(unsafe_code)]

#[path = "atomic_claim/main.rs"]
mod atomic_claim;
#[path = "claim_phase_codec/main.rs"]
mod claim_phase_codec;
#[path = "committed_codec.rs"]
mod committed_codec;
#[path = "credit_codec/main.rs"]
mod credit_codec;
#[path = "history_value_reads.rs"]
mod history_value_reads;
#[path = "independent_batch/main.rs"]
mod independent_batch;
#[path = "original_create/main.rs"]
mod original_create;
#[path = "original_policy_get.rs"]
mod original_policy_get;
#[path = "original_query_read/main.rs"]
mod original_query_read;
#[path = "original_query_row/main.rs"]
mod original_query_row;
#[path = "policy_codec/main.rs"]
mod policy_codec;
#[path = "public_claim/main.rs"]
mod public_claim;
#[path = "public_mutation/main.rs"]
mod public_mutation;
#[path = "raft_control/main.rs"]
mod raft_control;
#[path = "record_batch/main.rs"]
mod record_batch;
#[path = "record_identity/main.rs"]
mod record_identity;
#[path = "recovery_header.rs"]
mod recovery_header;
#[path = "state_transaction/main.rs"]
mod state_transaction;
#[path = "step_continue_apply/main.rs"]
mod step_continue_apply;
#[path = "step_continue_codec/main.rs"]
mod step_continue_codec;
#[path = "step_continue_response/main.rs"]
mod step_continue_response;
#[path = "workflow_value_bytes/main.rs"]
mod workflow_value_bytes;
