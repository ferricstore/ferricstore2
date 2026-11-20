//! Every integration test of `wf-domain` in one binary, so the suite compiles
//! and links once (ADR-012). Each module is an earlier test target, loaded
//! unchanged from its own file.

#![forbid(unsafe_code)]

#[path = "active_timeout/main.rs"]
mod active_timeout;
#[path = "admission_work.rs"]
mod admission_work;
#[path = "child_policy.rs"]
mod child_policy;
#[path = "claim_apply/main.rs"]
mod claim_apply;
#[path = "claim_caller/main.rs"]
mod claim_caller;
#[path = "claim_coordination/main.rs"]
mod claim_coordination;
#[path = "claim_selection.rs"]
mod claim_selection;
#[path = "claim_selection_fifo.rs"]
mod claim_selection_fifo;
#[path = "claim_selection_lane.rs"]
mod claim_selection_lane;
#[path = "claim_selection_limits.rs"]
mod claim_selection_limits;
#[path = "committed_control.rs"]
mod committed_control;
#[path = "complete_policy/main.rs"]
mod complete_policy;
#[path = "control_restoration.rs"]
mod control_restoration;
#[path = "core.rs"]
mod core;
#[path = "fifo_order_models.rs"]
mod fifo_order_models;
#[path = "governance_credit/main.rs"]
mod governance_credit;
#[path = "governance_release/main.rs"]
mod governance_release;
#[path = "group_batches.rs"]
mod group_batches;
#[path = "history_metadata/main.rs"]
mod history_metadata;
#[path = "history_original.rs"]
mod history_original;
#[path = "history_values/main.rs"]
mod history_values;
#[path = "independent_indexed/main.rs"]
mod independent_indexed;
#[path = "independent_terminals.rs"]
mod independent_terminals;
#[path = "independent_transitions.rs"]
mod independent_transitions;
#[path = "index_authority.rs"]
mod index_authority;
#[path = "index_sequence_models.rs"]
mod index_sequence_models;
#[path = "indexed_batches.rs"]
mod indexed_batches;
#[path = "lease_fence_models.rs"]
mod lease_fence_models;
#[path = "logical_mutation/main.rs"]
mod logical_mutation;
#[path = "many_item_opts/main.rs"]
mod many_item_opts;
#[path = "metadata_validation/main.rs"]
mod metadata_validation;
#[path = "options.rs"]
mod options;
#[path = "original_compact_history/main.rs"]
mod original_compact_history;
#[path = "original_external_term/main.rs"]
mod original_external_term;
#[path = "original_flow_value/main.rs"]
mod original_flow_value;
#[path = "original_record_metadata/main.rs"]
mod original_record_metadata;
#[path = "original_value_prepare/main.rs"]
mod original_value_prepare;
#[path = "payload_return/main.rs"]
mod payload_return;
#[path = "policy_byte_budget.rs"]
mod policy_byte_budget;
#[path = "policy_installation/main.rs"]
mod policy_installation;
#[path = "policy_restoration.rs"]
mod policy_restoration;
#[path = "policy_source_observations.rs"]
mod policy_source_observations;
#[path = "public_mutation/main.rs"]
mod public_mutation;
#[path = "record_policy_projection.rs"]
mod record_policy_projection;
#[path = "rejected_requests.rs"]
mod rejected_requests;
#[path = "request_dispatch.rs"]
mod request_dispatch;
#[path = "request_gate/main.rs"]
mod request_gate;
#[path = "request_replay.rs"]
mod request_replay;
#[path = "retry_policy.rs"]
mod retry_policy;
#[path = "score_bound.rs"]
mod score_bound;
#[path = "source_binary_hash.rs"]
mod source_binary_hash;
#[path = "storage_scope/main.rs"]
mod storage_scope;
#[path = "stored_lifecycle/main.rs"]
mod stored_lifecycle;
#[path = "touched_indexed/main.rs"]
mod touched_indexed;
