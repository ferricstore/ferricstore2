#!/usr/bin/env python3
"""Offline bookkeeping for reviewed assertion links; never execution evidence."""

import argparse
import hashlib
import json
import re
from pathlib import Path

SOURCE_COMMIT = "66586c596e271408d9184af32eb31cd7218c126e"
MANIFEST_SHA256 = "7df8c7885a64b6276e84517dffa4f1ead0f592f6d0996b398cdb61551c1bae61"
ROOT = Path(__file__).resolve().parents[2]

# Each tuple is a reviewed source declaration, actual test function, expected
# number of direct assertion macros, and source-line/Rust-assertion-index pairs.
# An index identifies a macro in the named function, never a helper/comment.
# Updating peer tests requires renewed semantic review before regeneration.
PARTITION = "crates/wf-types/tests/partition_key.rs"
FENCING = "crates/wf-domain/tests/fencing_retry.rs"
POLICY = "crates/wf-domain/tests/retry_policy.rs"
RETRY = "crates/wf-domain/tests/retry_policy_transitions.rs"
CLAIM = "crates/wf-domain/tests/claim_selection.rs"
FIFO = "crates/wf-domain/tests/claim_selection_fifo.rs"
LANE = "crates/wf-domain/tests/claim_selection_lane.rs"
LINKS = [
    ("8cafee0df511d738", "crates/wf-node/tests/snapshot_recovery/credit_cleanup.rs",
     "credit_cleaner_actual_600_ids_obey_two_command_budget_then_delete_remaining_88", 7,
     [(461, 0), (463, 1), (466, 4)]),
    ("f251fb1f83025582", "crates/wf-node/tests/snapshot_recovery/credit_cleanup.rs",
     "credit_cleaner_actual_busy_300_then_later_owner_respects_fairness_and_cursor_budget", 7,
     [(30, 0), (33, 1), (34, 3), (37, 4), (39, 5), (42, 6)]),
    ("429a5809355b7a23", "crates/wf-node/tests/snapshot_recovery/credit_cleanup.rs",
     "credit_cleaner_actual_empty_catalog_issues_no_command_and_changes_neither_store", 6,
     [(49, 0), (52, 1), (55, 4), (56, 5)]),
    ("f60d6ed55eeda186", "crates/wf-node/tests/snapshot_recovery/credit_cleanup.rs",
     "credit_cleaner_actual_invalid_hint_reset_uses_whole_tick_budget_before_cleanup", 5,
     [(67, 0), (69, 1), (72, 2), (73, 3), (75, 4)]),
    ("b43dccc831566cd3", "crates/wf-node/tests/snapshot_recovery/catalog_publication/source.rs",
     "source_catalog_collection_keeps_global_top_results_across_real_committed_pages", 6,
     [(89, 0), (94, 2), (104, 3)]),
    ("ff09a262ca76bbd1", "crates/wf-node/tests/snapshot_recovery/catalog_publication/source.rs",
     "source_catalog_paging_anchors_on_the_real_last_member_during_committed_insertion", 5,
     [(186, 0), (193, 2), (198, 1), (205, 3)]),
    ("88c3d8e9ce458865", "crates/wf-domain/tests/original_compact_history/source.rs",
     "source_history_codec_keeps_terminal_metadata_and_refs", 7,
     [(362, 0), (364, 1), (373, 2), (374, 3), (375, 4), (376, 5), (377, 6)]),
    ("e8f30f518d2328fb", "crates/wf-domain/tests/original_compact_history/source.rs",
     "source_history_codec_omits_immutable_metadata_and_restores_context", 10,
     [(406, 0), (407, 1), (408, 2), (409, 3),
      (417, 5), (418, 6), (419, 7), (420, 8), (421, 9)]),
    ("f52f128c812f46e5", "crates/wf-domain/tests/original_compact_history/source.rs",
     "source_history_codec_roundtrips_multibyte_length_headers", 2,
     [(436, 0), (438, 1)]),
    ("5baf7621b3712082", "crates/wf-domain/tests/history_metadata/source.rs",
     "source_history_metadata_ignores_unsupported_compound_values", 1,
     [(443, 0)]),
    ("5b12595d8af45093", "crates/wf-domain/tests/original_flow_value/source.rs",
     "source_value_codec_preserves_raw_escape_and_typed_roundtrips", 7,
     [(452, 0), (453, 1), (456, 2), (457, 3), (462, 4), (463, 5), (466, 6)]),
    ("d7ba621e9eb64c5d", "crates/wf-domain/tests/original_flow_value/source.rs",
     "source_typed_value_codec_returns_noncanonical_terms_without_deserializing", 7,
     [(471, 0), (474, 2), (476, 3), (480, 4)]),
    ("7486b505b2ba580e", "crates/wf-domain/tests/history_values/overlay.rs",
     "history_overlay_marks_storage_failures_without_exposing_their_reason", 8,
     [(11, 4)]),
    ("bdafcf374348156d", "crates/wf-domain/tests/history_values/overlay.rs",
     "history_overlay_file_reference_size_is_exact_stored_count", 8,
     [(22, 4), (22, 5)]),
    ("381f48c9d21d1d52", "crates/wf-domain/tests/history_values/overlay.rs",
     "history_overlay_marks_corrupted_typed_values_without_inserting_payload", 5,
     [(44, 2), (45, 3)]),
    ("bdafcf374348156d", "crates/wf-storage/tests/state_store/workflow_values/history_hydration/source.rs",
     "history_hydration_preserves_original_file_reference_size", 5,
     [(22, 1), (22, 2)]),
    ("381f48c9d21d1d52", "crates/wf-storage/tests/state_store/workflow_values/history_hydration/source.rs",
     "history_hydration_preserves_original_corrupted_typed_value", 6,
     [(44, 1), (45, 2)]),
    ("deb8dc5136bd2e4e", POLICY,
     "original_flow_policy_rejects_both_malformed_state_policy_recipes", 2,
     [(9, 0), (9, 1), (12, 0), (12, 1)]),
    ("9e5df11d9c3d3842", POLICY,
     "original_flow_policy_omits_absent_fields_and_preserves_the_exact_empty_state", 1,
     [(19, 0), (22, 0), (23, 0), (24, 0)]),
    ("3cc1821bd694c95e", POLICY,
     "original_flow_policy_retains_independently_configured_execution_modes", 9,
     [(28, 0), (36, 1), (37, 1), (38, 1), (39, 2), (39, 3),
      (40, 4), (41, 5), (42, 6), (43, 7), (43, 8)]),
    ("4963f0d0a4e54c1e", "crates/wf-domain/tests/independent_terminals.rs",
     "independent_terminal_many_preserves_per_item_success", 14,
     [(329, 0), (329, 1), (329, 2), (348, 3), (351, 10),
      (370, 0), (370, 1), (370, 2), (390, 3), (393, 10),
      (412, 0), (412, 1), (412, 2), (431, 3), (434, 10),
      (450, 0), (450, 1), (450, 2), (461, 3), (464, 10)]),
    ("538a6ff12c1e8b33", "crates/wf-domain/tests/independent_terminals.rs",
     "independent_terminal_pipeline_preserves_success_error_success", 9,
     [(609, 0), (609, 1), (631, 5), (634, 2), (637, 5)]),
    ("e0a28881c94ae756", "crates/wf-domain/tests/independent_terminals.rs",
     "independent_terminal_duplicate_complete_is_one_write", 9,
     [(721, 0), (721, 1), (721, 5), (724, 4), (729, 8)]),
    ("67e5682f41dbbb5b", "crates/wf-domain/tests/independent_transitions.rs",
     "independent_transition_many_keeps_success_after_stale_guard", 11,
     [(560, 0), (560, 1), (560, 2), (573, 3), (574, 8)]),
    ("5c2bdf9abea86479", "crates/wf-domain/tests/independent_transitions.rs",
     "independent_transition_pipeline_preserves_duplicate_order", 9,
     [(583, 0), (591, 1), (597, 6), (597, 7)]),
    ("1222bd491908472b", "crates/wf-ports/tests/record_batch/source.rs",
     "pinned_fail_many_preserves_order_and_atomic_guard_rollback", 13,
     [(44, 0), (45, 1)]),
    ("dab579f2989c77fd", "crates/wf-ports/tests/record_batch/source.rs",
     "pinned_fail_many_preserves_order_and_atomic_guard_rollback", 13,
     [(90, 11), (91, 11), (92, 12), (93, 12)]),
    ("88c6507d3eccfe8c", "crates/wf-ports/tests/record_batch/source.rs",
     "pinned_complete_many_preserves_order_and_atomic_guard_rollback", 13,
     [(795, 0), (796, 1)]),
    ("6286a35397e402f7", "crates/wf-ports/tests/record_batch/source.rs",
     "pinned_complete_many_preserves_order_and_atomic_guard_rollback", 13,
     [(845, 11), (846, 11), (847, 12), (848, 12)]),
    ("a16aec6cceb6253a", "crates/wf-domain/tests/complete_policy/original_entry.rs",
     "original_entry_preserves_original_shape_generation_and_consumption_errors", 8,
     [(603, 2)]),
    ("bd8fd99c0d608b90", "crates/wf-domain/tests/complete_policy/indexed_input.rs",
     "indexed_input_normalizes_frozen_original_policy_names_and_presence", 12,
     [(750, 4), (753, 4)]),
    ("bd8fd99c0d608b90", "crates/wf-domain/tests/complete_policy/indexed_input.rs",
     "indexed_input_preserves_all_source_diagnostics_and_untouched_inputs", 5,
     [(755, 0), (755, 1)]),
    ("9d3afdde6564e562", "crates/wf-domain/tests/complete_policy/indexed_input.rs",
     "indexed_input_preserves_nested_meta_local_priority_and_nil_atom_semantics", 5,
     [(125, 0)]),
    ("c8b5602be989ea05", "crates/wf-domain/tests/complete_policy/indexed_input.rs",
     "indexed_input_preserves_nested_meta_local_priority_and_nil_atom_semantics", 5,
     [(254, 0)]),
    ("bfba0cb7df3bc2ee", "crates/wf-domain/tests/complete_policy/indexed_input.rs",
     "indexed_input_preserves_nested_meta_local_priority_and_nil_atom_semantics", 5,
     [(295, 0), (304, 0)]),
    ("af1eb34f22715541", "crates/wf-domain/tests/complete_policy/indexed_input.rs",
     "indexed_input_preserves_all_source_diagnostics_and_untouched_inputs", 5,
     [(451, 2), (451, 3)]),
    ("2302fd8c2cfa076d", "crates/wf-domain/tests/complete_policy/encoding_vectors.rs",
     "source_policy_generation_stops_at_exact_migration_score_boundary", 11,
     [(554, 2), (554, 3), (558, 4), (560, 5), (563, 7), (563, 8)]),
    ("2302fd8c2cfa076d", "crates/wf-domain/tests/complete_policy/original_entry.rs",
     "original_entry_preserves_original_shape_generation_and_consumption_errors", 8,
     [(554, 3), (554, 4), (554, 6), (558, 7)]),
    ("d9078b362e9f0d80", "crates/wf-domain/tests/lease_lifecycle.rs",
     "source_reclaim_boundary_preserves_claim_and_explicit_reclaim_scenarios", 20,
     [(772, 0), (774, 1), (783, 5), (792, 6), (793, 8), (794, 9), (795, 10)]),
    ("284e6d5206fd4919", "crates/wf-domain/tests/lease_lifecycle.rs",
     "source_reclaim_boundary_preserves_claim_and_explicit_reclaim_scenarios", 20,
     [(812, 1), (820, 5), (828, 6), (829, 8), (830, 10)]),
    ("16b7387e060b86f8", "crates/wf-domain/tests/lease_lifecycle.rs",
     "source_extension_preserves_ownership_deadline_and_reclaim_ineligibility", 20,
     [(847, 1), (854, 4), (855, 5), (856, 6), (857, 7), (858, 8), (860, 13), (868, 15)]),
    ("becdbf189214b351", "crates/wf-domain/tests/lease_lifecycle.rs",
     "source_extension_preserves_ownership_deadline_and_reclaim_ineligibility", 20,
     [(900, 5), (901, 8)]),
    ("9466123fbaacecff", "crates/wf-domain/tests/lease_lifecycle.rs",
     "source_completed_record_preserves_transition_cancel_and_extension_errors", 14,
     [(969, 0), (975, 7)]),
    ("2f84fe8c1b8f4eb7", "crates/wf-domain/tests/lease_lifecycle.rs",
     "source_completed_record_preserves_transition_cancel_and_extension_errors", 14,
     [(143, 0), (149, 3), (151, 7), (156, 7), (160, 10)]),
    ("a765ee1199887142", "crates/wf-domain/tests/lease_lifecycle.rs",
     "source_fail_and_unleased_cancel_remove_due_work_and_reject_stale_fail", 16,
     [(747, 0), (756, 1), (758, 2), (765, 6), (772, 7), (775, 9), (776, 9), (778, 6), (785, 7), (788, 9), (790, 14), (799, 14)]),
    ("4b2f9d1ceaa255e1", "crates/wf-domain/tests/lease_and_guards.rs",
     "source_reclaim_takeover_fences_old_worker_and_preserves_three_history_rows", 19,
     [(908, 0), (910, 1), (918, 3), (926, 4), (927, 9), (928, 10), (929, 11),
      (931, 12), (937, 12), (946, 16), (946, 17), (950, 18)]),
    ("4b70723c9149fbe8", "crates/wf-domain/tests/complete_policy/presentation.rs",
     "presentation_preserves_pinned_type_and_state_response_defaults", 7,
     [(34, 0), (35, 0), (36, 0), (38, 0), (39, 0), (40, 0), (41, 0), (42, 0), (44, 0),
      (47, 1), (48, 1), (49, 1), (50, 1), (51, 1), (53, 1)]),
    ("f05f226483bfbc13", PARTITION, "encodes_components_with_byte_lengths", 1, [(7, 0)]),
    ("3221412a3842b3c5", PARTITION, "delimiter_bytes_cannot_make_different_component_lists_collide", 1, [(12, 0)]),
    ("4ba84b7b957de543", PARTITION, "uses_byte_sizes_for_arbitrary_binary_components", 1, [(17, 0)]),
    ("8ad5a4d7b9e59aa1", FENCING, "complete_retry_after_committed_response_loss_is_noop_success", 6, [(45, 0), (47, 1), (50, 2)]),
    ("7dee0331572ef4c4", FENCING, "fail_retry_after_committed_response_loss_is_noop_success", 5, [(65, 0), (66, 1), (67, 2)]),
    ("57bb245681edfa80", FENCING, "transition_retry_after_committed_response_loss_is_noop_success", 6, [(83, 0), (85, 1), (93, 2)]),
    ("093dafce3f08d6c3", FENCING, "retry_retry_is_guarded_by_latest_history_event", 9, [(109, 0), (111, 1), (118, 2), (120, 5), (127, 6)]),
    ("21028e29c3421d3a", FENCING, "retry_retry_without_explicit_run_at_preserves_original_policy_schedule", 6, [(147, 0), (149, 1), (156, 2)]),
    ("eea318ca431453d0", FENCING, "same_fence_does_not_allow_different_terminal_effect", 5, [(164, 0), (172, 1)]),
    ("ab0be7e9b8c01099", RETRY, "stored_state_retry_policy_drives_retry_exhaustion_without_command_override", 5, [(425, 0), (426, 1)]),
    ("579ae5ad18ea20b4", RETRY, "command_retry_policy_overrides_stored_state_policy", 2, [(462, 0), (463, 1)]),
    ("999b558a2dc276d2", POLICY, "flow_retry_policy_accepts_thirty_day_backoff_cap", 2, [(479, 0), (480, 1)]),
    ("bd7d579285676091", POLICY, "flow_retry_policy_computes_standard_backoff_timing", 10,
     [(491, 0), (492, 1), (501, 2), (502, 3), (503, 4), (512, 5), (515, 6), (518, 7), (529, 8), (530, 9)]),
    ("ecaed3fd34f388ec", POLICY, "flow_retry_policy_keeps_generated_backoff_values_bounded_and_deterministic", 6,
     [(562, 0), (563, 1), (564, 2), (567, 3), (572, 4), (580, 5)]),
    ("999b558a2dc276d2", "crates/wf-domain/tests/complete_policy/retry_input.rs",
     "retry_input_reconstructs_original_normalize_cases_before_timing", 11,
     [(479, 0), (480, 1)]),
    ("bd7d579285676091", "crates/wf-domain/tests/complete_policy/retry_input.rs",
     "retry_input_reconstructs_original_normalize_cases_before_timing", 11,
     [(491, 2), (492, 2), (501, 2), (502, 2), (503, 2), (512, 2), (515, 2), (518, 2),
      (529, 3), (530, 3)]),
    ("ecaed3fd34f388ec", "crates/wf-domain/tests/complete_policy/retry_input.rs",
     "retry_input_reconstructs_original_normalize_cases_before_timing", 11,
     [(562, 4), (563, 5), (564, 6), (567, 8), (572, 9), (580, 10)]),
    ("146040b0f178fda6", "crates/wf-domain/tests/complete_policy/retry_input.rs",
     "retry_input_preserves_source_diagnostics_and_field_order", 4,
     [(584, 0), (584, 1)]),
    ("e1a8cf7f619f136a", RETRY, "flow_retry_returns_to_claimed_run_state_with_computed_backoff", 20,
     [(607, 0), (608, 1), (621, 2), (622, 3), (623, 4), (624, 5), (631, 7), (632, 8),
      (633, 9), (634, 10), (635, 11), (636, 12), (637, 13), (638, 14), (639, 15),
      (641, 16), (649, 17), (657, 18), (658, 19)]),
    ("78839ca2df54b42f", RETRY, "flow_retry_exhausts_to_configured_active_state", 9,
     [(690, 0), (691, 1), (692, 2), (693, 3), (695, 4), (703, 5)]),
    ("537f7338ecf1c648", RETRY, "flow_retry_terminal_exhaustion_keeps_stable_audit_metadata", 5,
     [(735, 0), (736, 1), (743, 2), (744, 3), (745, 4)]),
    ("7b6561ea0f73c257", LANE, "member_round_trips_exact_128_bit_sequence_and_id", 1, [(12, 0)]),
    ("4808d163b1ee3c1f", LANE, "member_byte_order_preserves_sequence_beyond_float_precision", 1, [(19, 0)]),
    ("83dcf64f5941ec87", LANE, "lane_rank_preserves_sequence_beyond_float_precision", 2, [(34, 0), (35, 1)]),
    ("6cb33b54203c08af", LANE, "decoder_rejects_truncation_and_empty_id", 2, [(39, 0), (40, 1)]),
    ("f72492d5892f240b", LANE, "running_record_remains_in_logical_state_lane", 4,
     [(54, 0), (54, 1), (54, 2), (55, 3)]),
    ("49ba97e84872f9d0", LANE, "running_members_rank_before_queued_even_with_later_sequence", 4,
     [(79, 3), (80, 2), (84, 0), (84, 1), (87, 2), (88, 3)]),
    ("835390d5953260a3", LANE, "terminal_record_has_no_active_lane_identity", 1, [(125, 0)]),
    ("9bdbecf61fe86d5f", CLAIM, "higher_priorities_drain_before_lower_priorities_by_default", 4, [(34, 0), (44, 2)]),
    ("7d7eaf003a23555b", CLAIM, "priority_option_targets_one_priority_band", 3, [(65, 0), (74, 1), (85, 2)]),
    ("ed190d79e3813b5b", CLAIM, "multi_state_claims_earliest_due_instead_of_first_listed_state", 3,
     [(32, 0), (32, 1), (32, 2)]),
    ("3c9f0622ef767c44", CLAIM, "fifo_selects_one_head_per_lane_in_entry_order", 2, [(68, 0), (77, 1)]),
    ("3c9f0622ef767c44", FIFO, "live_running_head_blocks_follower_and_completion_unblocks_lane", 3,
     [(79, 1), (96, 2), (105, 2)]),
    ("e339e69ff4c5a0fa", FIFO, "retry_preserves_lane_blocking_until_due_and_terminal_exit", 3,
     [(156, 0), (165, 1), (174, 1), (184, 2), (193, 2)]),
    ("d66bd07c7dc9d94e", FIFO, "fifo_state_and_partition_lanes_are_independent", 3, [(232, 0), (243, 1)]),
    ("be6beed30e05ded4", FIFO, "fifo_state_and_partition_lanes_are_independent", 3, [(276, 2)]),
    ("2c1a0a952f9bef71", CLAIM, "fifo_partition_list_selects_only_one_head_per_listed_lane", 1, [(345, 0)]),
    ("485560e32e405cf1", FIFO, "expired_running_head_is_reclaimed_before_due_follower_with_limit_one", 5,
     [(487, 0), (497, 1), (498, 3), (500, 4)]),
    ("ba618b494b159854", FIFO, "not_due_head_blocks_eighty_due_followers", 1, [(536, 0)]),
    ("74f5fcdcf4328003", CLAIM, "fifo_deep_head_is_selected_with_eighty_earlier_due_followers", 6,
     [(568, 0), (571, 1), (574, 2), (574, 3)]),
    ("9f5d395534240b02", CLAIM, "fifo_global_due_order_survives_deep_head_lookup", 2, [(616, 0)]),
    ("bea9ae0f6f1e970c", CLAIM, "mixed_fifo_policy_preserves_due_order_for_parallel_states", 1, [(1146, 0)]),
    ("4bda857b47effb76", "crates/wf-domain/tests/claim_apply/main.rs", "source_future_due_boundary_preserves_empty_and_singleton_claims", 4, [(1295, 0), (1304, 1), (1312, 2), (1320, 3)]),
    ("0bf563deac76e307", "crates/wf-domain/tests/claim_apply/main.rs", "source_leased_and_terminal_records_are_absent_from_later_claims", 7, [(1250, 0), (1259, 5), (1267, 6), (1275, 1), (1282, 6)]),
    ("62328c799831fd82", "crates/wf-domain/tests/claim_apply/replay_models.rs", "source_default_parallel_claim_returns_both_same_lane_records", 6, [(13, 0), (25, 1), (35, 2)]),
    ("587581adc9eb0ceb", "crates/wf-domain/tests/claim_apply/main.rs", "source_fifo_activation_preserves_populated_state_entry_order", 2, [(783, 0), (792, 1)]),
    ("be9fd6690bf99980", "crates/wf-domain/tests/claim_apply/policy_activation.rs", "source_fifo_activation_waits_for_already_leased_lane_to_drain", 7, [(812, 0), (821, 1), (824, 3), (833, 4), (840, 5)]),
    ("4b70723c9149fbe8", "crates/wf-domain/tests/complete_policy/retention_input.rs",
     "retention_input_preserves_optional_presence_and_original_type_state_values", 8,
     [(36, 2), (38, 3), (42, 2), (44, 3)]),
    ("bb1cff7dbab51d19", "crates/wf-domain/tests/complete_policy/retention_input.rs",
     "retention_input_preserves_ttl_internal_history_and_captured_error_order", 5,
     [(140, 0), (140, 1)]),
    ("c718031a8b58adbb", "crates/wf-domain/tests/complete_policy/flow_input/governance.rs",
     "original_flow_input_preserves_complete_governance_model", 3,
     [(12, 0)] + [(line, 0) for line in range(41, 55)]),
    ("34c1385be24604d9", "crates/wf-domain/tests/complete_policy/flow_input/governance.rs",
     "original_flow_input_preserves_governance_late_size_priority", 3,
     [(225, 1), (225, 2)]),
    ("6ad2cd82f0217fe3", "crates/wf-domain/tests/complete_policy/flow_input/states.rs",
     "original_flow_input_preserves_state_map_keyword_and_pair_semantics", 2,
     [(149, 0), (149, 1)]),
    ("663a4d706bc368f4", "crates/wf-domain/tests/complete_policy/flow_input/governance.rs",
     "original_flow_input_preserves_governance_lists_and_rule_bounds", 1,
     [(164, 0), (169, 0)]),
    ("d4199dfa1b750255", "crates/wf-ports/tests/complete_policy_request/flow/source/anchors.rs",
     "original_flow_source_monotonic_generation_values", 5,
     [(18, 0), (18, 1), (21, 2), (21, 3)]),
    ("dea49ead483519f8", "crates/wf-ports/tests/complete_policy_request/flow/source/anchors.rs",
     "original_flow_source_stale_generation_values", 8,
     [(31, 0), (31, 1), (34, 2), (34, 3), (40, 4), (40, 5)]),
    ("17d8471407b0c04a", "crates/wf-ports/tests/complete_policy_request/flow/source/anchors.rs",
     "original_flow_source_replacement_omission_values", 5,
     [(323, 0), (323, 1), (329, 2), (329, 3)]),
    ("470baaca150ba604", "crates/wf-ports/tests/complete_policy_request/flow/source/anchors.rs",
     "original_flow_source_cross_policy_generation_values", 8,
     [(572, 0), (572, 1), (609, 5), (609, 6)]),
    ("006b42c44fb9d2e5", "crates/wf-ports/tests/complete_policy_request/flow/source/anchors.rs",
     "original_flow_source_target_highwater_value", 3,
     [(624, 0), (624, 1)]),
    ("b0c4b373d72b2a6c", "crates/wf-ports/tests/complete_policy_request/flow/source/anchors.rs",
     "original_flow_source_captured_history_limit_value", 6,
     [(668, 0), (668, 1)]),
    ("49bda0ec2fab73ce", "crates/wf-domain/tests/complete_policy/public_set.rs",
     "original_public_set_preserves_root_and_first_control_priority", 2,
     [(242, 0), (242, 1)]),
    ("4b70723c9149fbe8", "crates/wf-domain/tests/complete_policy/original_reply/source.rs",
     "original_reply_preserves_pinned_type_and_state_retry_retention_maps", 21,
     [(37, 5), (43, 14), (52, 18)]),
    ("d4199dfa1b750255", "crates/wf-ports/tests/original_policy_get.rs",
     "original_policy_get_preserves_pinned_allocator_and_type_binding", 6,
     [(24, 0), (24, 1)]),
    ("dea49ead483519f8", "crates/wf-ports/tests/original_policy_get.rs",
     "original_policy_get_preserves_pinned_allocator_and_type_binding", 6,
     [(46, 0), (46, 1)]),
    ("b48dac33c99e2b07", "crates/wf-ports/tests/original_policy_get.rs",
     "original_policy_get_preserves_pinned_allocator_and_type_binding", 6,
     [(82, 0), (82, 1)]),
    ("3fb6f6aed5d5d455", "crates/wf-ports/tests/original_policy_get.rs",
     "original_policy_get_preserves_pinned_allocator_and_type_binding", 6,
     [(101, 3), (101, 4)]),
]

# These limits describe this counterpart's represented observations, rather
# than the complete implementation status of another owner or milestone.
LIMITS = {
    '8cafee0df511d738': 'Executed exact600-ID/budget2 scenario preserves ID count600, first confirmed commands2/deleted512/errors0 and second commands1/deleted88/errors0. Actual remaining row identities equal the last88 IDs, then become empty; no progress write exceeds the two-command budget. The direct source Lease/Spend success assertions449/458 remain separate from these three links. Actual combined GC2/credit2 Redb stores receive complete original private frames through appended/committed logs and the real recovery drivers. Shard0/profile4 and fixed opaque a/b/ID suffixes replace native instance allocation and unique names. Source time inputs1000/1001/1006 and TTL5 remain; synthetic registration uses1001 so monotone apply does not incorrectly leave later owners live at the cleanup tick. Successful seed commits are checked in the fixture; direct public Lease/Spend reply assertions inside original cases remain unlinked where not represented by a named direct macro. Controlled lost/foreign replies, exact workspace/default16/max64/attempt bounds and purged/received snapshots are additional Rust safety tests. Rejecting interfaces were saved atafdfed8 before runner bodies; two additional empty-hint macros and later fault/default controls are post-body evidence without retroactive RED. Accepted Raft service, periodic GenServer/timer lifecycle, native FlowCase shard/FLUSHDB/MemoryGuard setup and all original execution profiles/independent Linux acceptance remain open. No full source-case, workflow, native or performance credit is claimed.',
    'f251fb1f83025582': 'Executed exact busy300/later1 two-owner scenario preserves lexical traversal rank, first commands2/deleted300/errors0, every busy reservation absent, the later original ID still present and missing persisted cursor after exhausted budget; the second tick preserves commands2/deleted1/errors0 and absent later reservation. Raw scoped reservation identities and actual progress reads supply the six original observations. Selected monotone execution may give the second lease deadline1006 rather than1005; both remain expired at the preserved1006 observation, with no earlier-deadline equivalence claimed. Actual combined GC2/credit2 Redb stores receive complete original private frames through appended/committed logs and the real recovery drivers. Shard0/profile4 and fixed opaque a/b/ID suffixes replace native instance allocation and unique names. Source time inputs1000/1001/1006 and TTL5 remain; synthetic registration uses1001 so monotone apply does not incorrectly leave later owners live at the cleanup tick. Successful seed commits are checked in the fixture; direct public Lease/Spend reply assertions inside original cases remain unlinked where not represented by a named direct macro. Controlled lost/foreign replies, exact workspace/default16/max64/attempt bounds and purged/received snapshots are additional Rust safety tests. Rejecting interfaces were saved atafdfed8 before runner bodies; two additional empty-hint macros and later fault/default controls are post-body evidence without retroactive RED. Accepted Raft service, periodic GenServer/timer lifecycle, native FlowCase shard/FLUSHDB/MemoryGuard setup and all original execution profiles/independent Linux acceptance remain open. No full source-case, workflow, native or performance credit is claimed.',
    '429a5809355b7a23': 'Executed empty-catalog/budget1 scenario directly observes missing progress before/after and commands0/deleted0/errors0. Complete equality of all six state and both log tables is stronger than unchanged applied index; no preparation/submission occurs. The original native applied_index/WARaft tuple encoding is not implemented by the whole-cohort equality. Actual combined GC2/credit2 Redb stores receive complete original private frames through appended/committed logs and the real recovery drivers. Shard0/profile4 and fixed opaque a/b/ID suffixes replace native instance allocation and unique names. Source time inputs1000/1001/1006 and TTL5 remain; synthetic registration uses1001 so monotone apply does not incorrectly leave later owners live at the cleanup tick. Successful seed commits are checked in the fixture; direct public Lease/Spend reply assertions inside original cases remain unlinked where not represented by a named direct macro. Controlled lost/foreign replies, exact workspace/default16/max64/attempt bounds and purged/received snapshots are additional Rust safety tests. Rejecting interfaces were saved atafdfed8 before runner bodies; two additional empty-hint macros and later fault/default controls are post-body evidence without retroactive RED. Accepted Raft service, periodic GenServer/timer lifecycle, native FlowCase shard/FLUSHDB/MemoryGuard setup and all original execution profiles/independent Linux acceptance remain open. No full source-case, workflow, native or performance credit is claimed.',
    'f60d6ed55eeda186': 'Executed malformed derived-progress setup is observed Invalid after test-only durable raw injection. The first budget1 tick confirms exactly one committed reset with no cleanup, missing progress and the original reservation still present; the next budget1 tick confirms one deletion with no errors. Invalid FWPC1 bytes replace the noncanonical native ETF progress payload as the explicitly selected derived-hint migration; no native TermCodec format or general KV API compatibility is claimed. Actual Invalid classification after successful injection represents original setup success67. Actual combined GC2/credit2 Redb stores receive complete original private frames through appended/committed logs and the real recovery drivers. Shard0/profile4 and fixed opaque a/b/ID suffixes replace native instance allocation and unique names. Source time inputs1000/1001/1006 and TTL5 remain; synthetic registration uses1001 so monotone apply does not incorrectly leave later owners live at the cleanup tick. Successful seed commits are checked in the fixture; direct public Lease/Spend reply assertions inside original cases remain unlinked where not represented by a named direct macro. Controlled lost/foreign replies, exact workspace/default16/max64/attempt bounds and purged/received snapshots are additional Rust safety tests. Rejecting interfaces were saved atafdfed8 before runner bodies; two additional empty-hint macros and later fault/default controls are post-body evidence without retroactive RED. Accepted Raft service, periodic GenServer/timer lifecycle, native FlowCase shard/FLUSHDB/MemoryGuard setup and all original execution profiles/independent Linux acceptance remain open. No full source-case, workflow, native or performance credit is claimed.',
    "5baf7621b3712082": (
        "Exact FlowCodecTest442 preserves float1.5, the nested:true map, nested binary list "
        "and nested/true tuple; the direct equality at443 returns only float=>1.5. "
        "Captured logical map order and leaf spans drive the actual pure normalizer. "
        "The source declares ExUnit.Case async:true/flow and no local setup; original "
        "umbrella/native/mandatory profiles and independent Linux acceptance remain required. "
        "Seven compiled healthy-API rejections were committed in78ada59 before the body. "
        "Separate genuine regressions saved in26ae829/85984b7 then correct decimal-even ties "
        "and the narrower lower round-trip interval at normal powers of two. "
        "All seven complete local tails pass, including separately supplemental alias/order/"
        "borrow, arbitrary integer/Latin1/short-float, invalid selected capture and resource "
        "controls. Float goldens/round-trip samples and explicit captures are not VM "
        "execution, canonical equality against every native float, public history service, "
        "storage authority or full source/profile/workflow parity."
    ),
    "b43dccc831566cd3": "Executed stored counterpart preserves all300 catalog-page-test prefix/padded000..299 members, budget catalog, prefix filter, global descending top5, and exact expected last-five equality. Two bounded actual committed membership commands replace the direct source300-member ZADD setup; no single300-member atomic-command guarantee is claimed. Typed accepted retained statuses/successful Result and count/equality checks represent the source successful registration/collection observations. Noise skipping, whole-table/log nonmutation and purged/received snapshot readback are supplemental controls. Fixed suffix rust replaces only an opaque positive unique suffix. Actual Node committed-prefix recovery and the real catalog writer/reader are exercised, without an accepted Raft submission service, public Elixir tuple/wire compatibility, FlowCase live-shard/FLUSHDB/MemoryGuard setup, original native profiles or independent Linux acceptance. This is post-implementation conformance, not original tests-first RED or full source-case/workflow parity.",
    "ff09a262ca76bbd1": "Executed stored counterpart preserves300 catalog-anchor-test padded001..300 members, approval catalog, inserted000-before, actual commit during traversal, exactly300 original observed members and full ordered equality excluding the new earlier member. Bounded two-command setup replaces direct source ZADD; one actual retained membership command inserts the new member. The existing anchored collector with all300 results/reverse presentation supplies the traversal observation in place of the source arbitrary reducer callback; this does not claim the whole reduce_pages API. Fixed rust suffix replaces only a generated opaque suffix. Insertion-success and typed Result/cardinality/equality link the source observations; the did-insert control is supplemental. Real stored namespace/cursor checks and complete committed recovery execute, while accepted Raft service, public/native response encoding, original FlowCase setup/cleanup and mandatory profiles/independent Linux acceptance remain open. This is after-body conformance without retroactive tests-first RED or full source-case/workflow parity.",
    "deb8dc5136bd2e4e": "Executed original-input normalization preserves both exact workflow type names, keyword states opts, queued state, integer42 and the one-element non-keyword atom list. The shared direct rejection-kind/body macros execute for BOTH original recipes; each source error tuple maps to both observations. This covers the complete pure rejection observations using existing Rust descriptors, without a runtime term decoder or transport claim. The source ExUnit.Case async:true file has no case setup or special tags; its original umbrella helper/profile still requires execution. The actual normal mix test attempt stopped before tests at Mix.PubSub TCP socket :eperm, retained as setup failure. First Rust PASS and strict checks are macOS acceptance of existing implemented behavior, not tests-first RED, original source execution, independent Linux validation or full workflow parity. The separate FIFO helper observations have their own public-view counterpart; complete original-case/profile acceptance remains open.",
    "9e5df11d9c3d3842": "Executed original-input normalization preserves compact-policy, original keyword states opts and queued empty state policy. One direct full Result/canonical ETF equality proves normalization success, exactly the states/type fields, absent version/max_active_ms and exactly queued=>empty map; all four original assertions are linked to that complete literal equality. Independent grammar packing proves all60 expected bytes; it is not an externally promised legacy wire format. The source ExUnit.Case async:true file has no case setup or special tags; original umbrella helper/profile execution still requires a usable Mix environment. Actual mix test stopped before tests at Mix.PubSub TCP socket :eperm. Existing-behavior Rust/macOS PASS adds neither retroactive TDD, independent Linux acceptance, source runtime/profile execution nor full workflow parity. The third execution-mode/FIFO observations now have a separate public-view counterpart; complete original-case/profile acceptance remains required.",
    "3cc1821bd694c95e": 'Executed original ordered-policy normalization preserves the exact keyword states recipe with queued FIFO and review parallel. The actual flow_policy_v1 reader and charged public reply resolver supply queued/review/undeclared modes and the complete two-row map before ordinary Rust set/any/queued/review predicates; actual Absent supplies an empty map and nil false. Nine original assertions map to direct success/mode/full-map/set/predicate macros in the named test, including both empty-map and false observations for nil. The two same-file helpers only construct the original raw recipe or mechanically inspect charged product map rows; they implement no mode fallback or FIFO business logic. This is the reviewed Rust public-view semantic counterpart, with no new named FIFO helper API. The explicit generation1 envelope and same-byte states span5 identity-order correspondence are represented fixtures, not VM/native map-order capture, authoritative storage or failure-order parity. A first attempt lacking required all-state correspondence failed InvalidAuthority; the fixture was corrected without changing production or claiming tests-first RED. All eleven retry-policy tests and strict lint pass on macOS; the mutable-output extractor refinement preserves the complete old13790-byte prefix and original observations. Original ExUnit.Case async:true umbrella setup/helper/profile execution remains required: the real source attempt stopped before assertions at Mix.PubSub TCP socket :eperm. No source runtime, transport, full source-case/workflow parity, independent Linux acceptance or measured performance is certified.',
    "4963f0d0a4e54c1e": "Executed pure independent terminal components preserve all four original Complete/explicit Retry/Fail/Cancel groups, bad-first stale guard and good-second success, original colon-UID prefixes and distinct repeated type calls versus Cancel's shared type. Real Create and named Claim reducers supply current versions/guards; only unspecified creation clocks and opaque tokens are fixed. Shared direct macros execute for each effect: source329/370/412/450 link cardinality, StaleLease and exact ERR stale flow lease body, while source348/390/431/461 and351/393/434/464 link unchanged borrowed bad state and successful proposed good state. Bad fences are actual+1, good fences exact; Cancel is unleased1/0. Terminal time2000 and Retry explicit3000 remain intact. No physical nonoverwrite/get, compound public replies, claim_due selection or default/installed policy resolution/exhaustion/audit is certified. Source441's creation prerequisite, public preprocessing, retention/values/governance/children, authoritative index/history/durable independent publication, FlowTest async:false/flow/global_state/120000/live-shard/FLUSHDB setup/cleanup and every required original profile remain open. Required hook/no-I/O successors5020f3d94d04459d/4367a8c983c1da95 are not excluded. Runtime identity stays unresolved. Genuine missing-API RED twice precedes production; first assertion execution passes all four new tests on macOS. A later empty-result assertion style repair preserves the complete frozen oracle by whole-file inverse. Logical ownership/admission proves no measured performance, independent Linux result or full source-case/workflow parity.",
    "538a6ff12c1e8b33": "Executed pure terminal pipeline components preserve the original shared type/partition and three ID prefixes, actual Create plus named Claim at1000 with worker pipeline-terminal/lease30000, Complete at1001 and exact wrong-token middle guard. Direct source609 links cardinality/error,631/637 share the completed-state macro reached for first and third success,634 links unchanged borrowed middle running state. Both successful write helpers and supplemental IDs/predecessor2/version3/history checks execute, but helper assertions receive no invented direct anchors. These are owned proposed plans, not public :ok replies, physical reads/nonoverwrite, claim_due/index selection, Instance/Router preprocessing, indexed/history/durable publication, retention/governance or original FlowTest setup/runtime profiles. Required hook/no-I/O successors remain unported. Runtime identity stays empty; generated positive suffixes and opaque current tokens alone are fixed. Twice-reproduced missing-API RED precedes production and all four new tests pass first execution on macOS; lint-only empty assertion repair has a whole-file inverse. Independent Linux, full source-case/workflow parity and measured native performance remain unproved.",
    "e0a28881c94ae756": "Executed pure duplicate terminal components retain separate type/ID uid calls, source prefixes, actual Create/Claim at1000 with pipeline-terminal-dup/lease30000, and the identical Complete pair at1001. Source721 links exact cardinality plus Changed/LegacyNoop outcomes,724 links the completed proposed state,729 links exactly one newly emitted Completed event. The second result owns neither write nor history; expected predecessor2/version3 and unchanged old Transition-only refusal are supplemental. Source727 physical history-query success remains REQUIRED UNLINKED: counting reducer-emitted events is not a persisted history query or public compound/get reply. No retained request ID simulates legacy duplicate semantics. Original claim_due selection, physical/index/history/durable publication, retention/values/governance/children, FlowTest async:false/flow/global_state/120000/live-shard/FLUSHDB setup/cleanup and every required profile remain open. Runtime identity is unresolved. Genuine missing-API RED twice precedes production; all four selected tests pass first execution on macOS, with a later lint-only empty assertion repair proven by entire-file inverse. Neither full source-case/workflow parity, measured native resource gains nor independent Linux acceptance follows.",
    "67e5682f41dbbb5b": "Executed pure independent-transition components preserve original positive-colon partition/type/ID prefixes, queued creation/run1000, bad-first fencing1 and good-second fencing0, ready run2000, ordered StaleLease/Ok outcomes and exact ERR stale flow lease body. Five direct macros link only source560 cardinality/error/body and source573/574 state components. No successful plan exists for bad; its borrowed original stays queued, while the good proposed write is ready. This is not a physical flow_get/nonoverwrite or public compound reply assertion. Typed successful Create fixtures replace original create_many/hydrate setup; source551 public creation, routing/placement provenance, indexed FIFO/priority effects, independent durable publication/history, wire/response encoding, FlowTest async:false/flow/global_state/120000/live-shard/FLUSHDB cleanup and every required original runtime profile remain open. Runtime case identity remains unresolved. Supplemental admission, no-op, partition, ownership and cumulative payload controls add no source links. Genuine precise missing-API RED twice preceded production; all four selected tests passed first execution on macOS. These component links do not establish full source-case/workflow parity, measured native performance or independent Linux acceptance.",
    "5c2bdf9abea86479": "Executed pure independent pipeline preserves original positive-colon partition/type/ID prefixes and actual queued creation, then ordered queued-to-ready and ready-to-processing with fencing0 and both successful results. Four direct macros link only source583 initial-state component, source591 exact result cardinality (both success helpers additionally execute), and source597 final processing/version3 components. Proposed predecessor versions1/2 and intermediate ready/version2 are supplemental. Fixed explicit2000/3000 replace only unasserted clocks; original run1000 is retained. Owned plans and an in-memory initial GroupView replace source Instance/Router/state-machine/public create/get calls; aliases/default input normalization, physical/index/history changes, distributed grouping, durable independent publication, FlowTest async:false/flow/global_state/120000/live-shard/FLUSHDB setup/cleanup and every required original runtime profile remain open. Runtime case identity remains unresolved and no IDs are invented. The atomic duplicate-ID refusal stays separate. Genuine precise missing-API RED twice preceded production; all four selected tests passed first execution on macOS. This partial component mapping does not certify the complete source case, full workflow parity, native resource gains or independent Linux acceptance.",
    "49bda0ec2fab73ce": "Executed pure public-input preparation preserves generation-state-validation-1 and state:1 from source239-244. Two direct test-body macros represent only source242's invalid-state diagnosis and exact ERR flow state must be a string body; encoding uses the same continuously charged account. Fixed positive suffix1 replaces the generated identity. Function-head, FIRST controls, LAST patch values, raw sizing and ownership receive additional controls, but this counterpart links none of their helper assertions to unrelated source assertions. Genuine missing-API RED was independently reproduced before production; the later predicate-order and improper-terminal supplements and ordinary compiler/lint repairs retain separate provenance. Public dispatch and returned error tuple, arbitrary-current default-false merge, committed identity/replies, physical atomic/all-shard publication, original async:false/global_state setup/flush and every mandatory runtime profile remain required. Borrowed represented maps assume mathematical key uniqueness and captured reduce order; opaque identity and struct Enumerable support remain temporary required gaps, not exclusions. Passing macOS component tests and logical bounded-work improvement establish neither full source-case/workflow parity, measured native performance nor independent Linux acceptance.",
    "d4199dfa1b750255": "Postimplementation lower-parent coverage preserves generation-always-on-1, max_active_ms1000 then2000, no expected-generation guard and the ACTUAL first original ETF output as next current. Direct generation/full-value macros represent only the returned generation/value components of source18/21. The positive generated type suffix is fixed to1; explicit private request IDs/stamps supply additional Rust metadata. Public flow_policy_set preprocessing and reply maps, source24 public get, all-shard helper calls19/22, original async:false/global_state setup/flush and required runtime profiles remain unexecuted. Each result is a proposed owned installation, not a physical write/read, transaction or durable outcome. These direct tests were added after e4859af and pass existing behavior without new or retroactive tests-first RED. Independent Linux acceptance and full source-case/workflow parity remain required; logical work/ownership implies no measured native gain.",
    "dea49ead483519f8": "Postimplementation lower-parent coverage preserves generation-cas-1, initial max_active_ms1000, CAS1 update2000 using ACTUAL first original ETF as current, then stale CAS1/update3000 using ACTUAL second output. Direct generation/full-value macros represent source31/34 components; exact retained StaleGeneration and caller body represent source40 error components. Fresh(None) and reobserved immutable second output are supplemental and do not prove source46 public get or physical nonoverwrite. Fixed positive suffix1 and explicit private request/stamp fixtures do not implement public preprocessing/replies, all-shard publication, original async:false/global_state flush/setup or required runtime profiles. Proposed installation/outcome owners are not a durable transaction. Tests were added after e4859af with first-PASS existing behavior, not tests-first RED. Independent Linux acceptance and complete source-case/workflow parity remain required; no measured allocation/speed claim follows.",
    "17d8471407b0c04a": "Postimplementation lower-parent coverage preserves generation-replace-1, initial max_active_ms1000 and queued FIFO, executes the first installation, then supplies its ACTUAL original ETF output as current for an empty replacement. Direct generation/full-value observations represent source323 generation1/content and source329 generation2/empty-states components only. Stored active-duration omission is supplemental; the public max_active_ms:nil projection remains UNLINKED and required. Raw already-converted maps substitute for public keyword/PolicyPatch preprocessing; public replies, physical atomic/all-shard installation, original async:false/global_state flush/setup and mandatory runtime profiles remain required. Generated suffix1 and private metadata are fixed fixtures. Tests supplement existing e4859af behavior without retroactive RED or complete source-case parity; independent Linux execution and full workflow acceptance remain open, with no native gain claim.",
    "470baaca150ba604": "Postimplementation lower-parent coverage retains current-cross-policy-wire normalized max_active_ms1000 at time1000, ACTUAL original ETF output as next current, and normalized3000 at time3000. Direct generation/full-byte observations represent only source572/609 decoded-value components. The intervening beta-cross-policy-wire normalized2000 at time2000 is the actual generationless TWO-TUPLE BINARY patch and reaches the exact PatchShape/body controls; source585 is a pattern assignment, while physical ETS assertions569/592/606 remain UNLINKED. Supplied coherent views and private log/request metadata do not execute StateMachine persistence, ETS publication, all-shard behavior or original setup/context/runtime profiles. Tests were added after e4859af as existing-behavior direct anchors without new RED. Independent Linux execution and complete source-case/workflow parity remain required; source-level ownership is not measured native performance.",
    "006b42c44fb9d2e5": "Postimplementation lower-parent coverage retains target-high-water-policy, independently encoded full original stored generation3/max_active_ms1000, normalized replacement2000 at time2000 and exact generation4/full original ETF output. The two direct macros represent only source624 decoded allocation components. The original initial physical put is replaced by an explicitly supplied coherent current view; later stale generation2 put and source636 ETS nonoverwrite assertion remain UNLINKED. Reobserving immutable stored fixture bytes proves no physical mutation. Original StateMachine/store setup, atomic/all-shard publication, required runtime profiles and independent Linux acceptance remain required. Tests were added after e4859af and pass existing behavior without retroactive tests-first RED, full source-case/workflow parity or measured native allocation/speed claims.",
    "b0c4b373d72b2a6c": "Postimplementation lower-parent coverage preserves replicated-policy-limit-accepted at time1000 and replicated-policy-limit-rejected at time2000, with the SAME retention.history_max_events50 under supplied immutable C100 and C10. Direct generation1/full original ETF equality represents only source668 accepted decoded-value components; derived type/empty states are additional normalized fields permitted by that source partial-map pattern. C10 reaches exact History{maximum:10} and the original62-byte body, but source684 is a pattern assignment and source692 physical ETS absence remains UNLINKED. This pure input does not execute conflicting global application environment changes, ApplyContext encoding, StateMachine/ETS publication, all-shard setup or original runtime profiles. Tests were added after e4859af as first-PASS existing-behavior anchors without new RED. Independent Linux execution and complete source-case/workflow parity remain required, with no measured native gain.",
    "663a4d706bc368f4": "Executed input retains original email/webhook/email/sms/webhook occurrences and compares the entire independently assembled normalized snapshot, including allowed email/webhook/sms in first-seen order. The sole direct test macro represents source normalization success164 and exact list order169. Extra binary type r and empty states are whole-flow context; the governance map itself preserves the original source recipe. Other list/rule-bound assertions in called helpers remain unlinked here. Original async:true ExUnit/runtime/setup and mandatory profiles plus independent Linux execution remain required. Borrowed input, deterministic ETF and logical work controls imply no public installation, origin/payment authority, physical publication or measured native allocation/speed. Initial missing-API RED and subsequent recorded test corrections remain separate; these component links do not certify full source-case or workflow parity.",
    "c718031a8b58adbb": "Executed whole-flow normalization contains the exact original governance input: full mode, running limit100/strict_global/lease20, llm:gpt budget10000/window60000/tokens, all nine effect:email.send circuit fields and stripe.refund/payment_review/finance approvals. One complete deterministic ETF equality, independently assembled from explicit expected fields, represents source normalization success12 and all fourteen scalar/list observations41-54. The extra outer type r and empty states map are whole-flow context, not part of the source governance map. Borrowed descriptors replace original terms; no public installation, current-policy/frame/origin authority or physical publication is inferred. Source async:true ExUnit execution, original runtime/setup and all mandatory profiles plus independent Linux acceptance remain required. Genuine missing-API RED preceded implementation; recorded source-based post-RED test corrections and postimplementation supplements are distinct from that original buffer. These links record represented components, not complete source-case or workflow parity, and imply no measured native speed or allocation gain.",
    "34c1385be24604d9": "Executed input preserves all600 distinct original e*2000+decimal1..600 leaves, totaling1201692 payload bytes. It reaches the governance1048576-byte diagnosis and exact response body; extra invalid states proves this governance gate precedes later state validation. Original source-derived normalized governance size1204727 was independently captured before production. Whole-flow borrowed normalization is an explicit wrapper around the source Policy.normalize recipe; source ExUnit/runtime/setup/profiles, physical publication and independent Linux execution remain required. The direct reason/body assertions link only source225. Separate exact-cap neighbors and arbitrary customer-version/snapshot ETF controls are supplemental and do not link original2357's allowed stored-map fixture that omits states. The real missing-API RED, post-RED corrections, first99/1 accounting-test failure and subsequent primitive-event repair remain distinct evidence; this component mapping is neither whole source-case parity nor a native performance measurement.",
    "6ad2cd82f0217fe3": "Executed raw whole-flow input preserves queued keyword max_active_ms1000 and returns StateMaxActive plus the exact ERR flow max_active_ms is type-level only body. Fixed binary type r replaces only the source generated type suffix. The direct macros represent the error component at149, not the source public FerricStore.flow_policy_set error tuple, caller/options/frame/Fresh/current ordering, authoritative reads or atomic installation. Original FlowTest async:false tags/timeout/shards/FLUSHDB/cleanup, required runtime profiles and independent Linux execution remain required. State-class/order/hash/nested-workspace controls are supplemental; macOS source execution and genuine initial missing-API RED do not establish whole public/source-case or workflow parity.",
    "a16aec6cceb6253a": "The frozen original generationless two-tuple retains checkout, the root max_attempts=5 and charge_card state max_attempts=2. The direct Rust None assertion represents source603 :error after complete safe ETF parsing and strict three-tuple shape refusal; it does not claim a deprecated-key predicate result. The original fixture is retained alongside its existing helper control. This direct ledger anchor was added after implementation and supplies supplemental executed coverage, not new tests-first RED. Pure supplied atom/runtime catalogs replace the live VM context; original FlowTest setup, public policy reads, physical storage, broader runtime profiles and independent Linux execution remain required. This single error component does not close the entire source case or full workflow parity.",
    "bd8fd99c0d608b90": "Executed normalization and All/State presentation preserve tenant/atom-region/campaign/tenant first-occurrence names; exact all-binary a/b/c/d input preserves the three-key rejection kind and body. Source750/753 link field components only; public set/get replies745/752, atomic installation, original FlowTest setup and required profiles remain unlinked. This normalizer follows genuine missing-API RED; the links do not establish whole original-case or Linux acceptance.",
    "9d3afdde6564e562": "The actually executed bare Binary version input preserves the normalized indexed_state_meta field at125. This is a post-implementation supplemental observation; public get replies, flow/migration-worker setup, record updates, query/count/index/LMDB behavior, cleanup and required profiles remain unlinked. A typed field observation does not certify complete runtime or independent Linux acceptance.",
    "c8b5602be989ea05": "The actually executed bare Binary version input preserves the normalized indexed_state_meta field at254. This post-implementation supplement does not execute public get replies, worker/migration configuration, claim/signal/query/index/LMDB operations, original setup/cleanup or required profiles. Complete source-case and independent Linux acceptance remain required.",
    "bfba0cb7df3bc2ee": "Separate actually executed bare Binary owner and Nil fixtures preserve the normalized field components at295/304. These post-implementation supplements do not execute public replies, policy reconfiguration, create/claim/signal/index/query operations, original setup/cleanup or required profiles. Complete source-case and independent Linux acceptance remain required.",
    "af1eb34f22715541": "The actually executed two-binary version/owner list preserves the one-key rejection kind and exact body at451. The source17-entry record metadata error441 is a separate REQUIRED UNLINKED observation, as are public policy/error tuples, original setup/cleanup and all required profiles. Genuine normalizer RED and macOS source tests do not establish full source-case or independent Linux acceptance.",
    "2302fd8c2cfa076d": "Executed pure normalized-policy observations preserve the pinned generation-boundary/public-v1 content, exact 9007199254740991 generation ceiling, above-ceiling decode refusal, MAX-1 migration allocation and exact exhaustion error. FWPV/FWPS V2 is an explicit semantic encoding replacement for the original ETF tuple; this does not prove legacy ETF decoding, arbitrary stored maps, generation0, public policy installation or physical reads. Supplemental full-byte and unchanged-owner assertions are separate. Source ExUnit async:true setup, original runtime and required profiles remain unexecuted by this Rust test; macOS passing evidence does not replace independent Linux execution. This is coverage of existing code after implementation, not tests-first RED or full workflow parity.",
    "d9078b362e9f0d80": "Original lease/flow-reclaim fixture preserves Claim50@1000 deadline1050, running Reclaim50@1049 NotEligible and @1050 Changed ID/worker-b/version3/new token. Source760/763 helper/public setup remains unlinked;774/783 links cover one-record eligibility/effects only, not public empty/singleton query selection.",
    "284e6d5206fd4919": "Separate lease-api/flow-reclaim-api fixture preserves early1049 and boundary1050 observations, exact ID/worker-b/new token. Source801/804 helper/public setup and source public flow_reclaim alias/default-state dispatch remain unlinked;812/820 component links do not certify public empty/singleton cardinality.",
    "16b7387e060b86f8": "Separate lease-extend fixture carries actual Claim50@1000 guard into Extend500@1020; state running, version3, identical token/fence and deadline1520. Reclaim1100 is NotEligible; wrong fence+1 Extend1030 returns exact stale lease. Source837/839 helper/public setup remains unlinked;847 typed effect and860 one-record eligibility do not certify public replies/selection.",
    "becdbf189214b351": "Separate lease-extend-ok fixture retains only source900 version3 and901 deadline1520 links. Source881/883 setup,891 public :ok return:ok_on_success/no-materialization and899 public get success remain REQUIRED UNLINKED; typed record effects do not cover the return-OK allocation contract or lease_return_ok profile.",
    "9466123fbaacecff": "Separate lease-extend-terminal fixture Claim50@1000, Complete original owner guard@1010 and Extend original guard500@1020 yields stale lease. Source959/961 helper/public setup unlinked;969 represents typed Changed completion only. Other terminal refusals in this fixture are supplemental.",
    "2f84fe8c1b8f4eb7": "Separate image/flow-terminal-guard fixture Claim30000@1000 and Complete result:ID uses explicit1010 since source clock was omitted. Completed Transition and Cancel use actual completed fence without token and preserve exact terminal/use-FLOW.REWIND errors and completed state. Source151/156 share reached error assertion for separate operations;131/134 helper/public setup and159 public get stay unlinked. Original guard Extend is supplemental.",
    "a765ee1199887142": "Both jobs-fail/jobs-cancel Creates precede sole fail Claim worker-a30000@1000. Wrong-fence+1 Fail error:ID@1500 is rejected before accepted Fail; Cancel affects the original UNLEASED queued record with fence0/no token/reason:ID@1500. Both terminal records have no lease/due work; Fail version3 and Cancel version2 plus inline bodies/history are supplemental. Shared direct macros execute for each distinct effect. Source744/745 setup and external error_ref773/774/786/787 stay REQUIRED UNLINKED; inline error bytes are not external references. Source790/799 NotEligible components do not certify public limit10 empty full-index selection.",
    "4b2f9d1ceaa255e1": "Pure single-record Create/Claim/Reclaim plans preserve leases 50 at 1000 and 500 at 1050, first-owner completion and extension rejections at 1060, exact ID/worker/fence/token observations, and all three proposed history kind/worker entries with created=1/claimed=2. Source 908/910/918 links represent typed successful plans only; public success replies and claim/reclaim selection/cardinality remain required. Source 931/937 share a reached assertion that runs once for each old-owner operation. Source 946 maps both event counts; supplemental exact length and full sequence prevent other events. Source 944 public history query success remains UNLINKED. Supplied record/history snapshot equality is a pure input/fixture control, not persisted atomic rollback. Fixed positive ID/type suffixes and distinct committed token bytes normalize nondeterministic identities only; explicit pure creation time 1000 does not certify the omitted source creation clock. Public projections, automatic partition placement, routing/selection, physical history query/count/order/hydration, durable record/index/history/outcome writes, original async:false flow/global_state shard/FLUSHDB setup and cleanup, required source profiles and independent Linux/macOS execution remain open. First PASS is existing-behavior regression coverage, not preimplementation TDD or complete original-case parity.",
    "4b70723c9149fbe8": "The executed pure normalized presentation preserves every fifteen scalar response assertion at source34-36/38-42/44/47-51/53. Whole independently specified All and State equality checks retain max-active120000, type retry5/fixed10000/30000/jitter0/failed, type TTL60000/history512, charge_card retry2/payment_failed with inherited fixed backoff and TTL30000/history256. Fixed flow-policy replaces only the generated unique suffix. Real normalized restoration and V2 preparation supply current content with explicit generation1; they do not execute public installation or generation allocation. Original public set/get success replies11/46 and hot-field map-key refutes37/43/52 remain UNLINKED acceptance obligations; the Rust public retention type has only TTL/history fields, which is compile/type-shape evidence rather than executed map serialization. Public options/raw-schema/state validation, configuration capture, authoritative physical reads, source ETF/hash/namespace authority, public maps/wire, atomic durable installation, FlowCase shard/WARaft/FLUSHDB/MemoryGuard setup/cleanup and every original execution profile remain omitted. Supplemental absence/unknown state/raw governance/version/indexed borrows, captured defaults and logical resource controls do not close those obligations. Actual tests-first missing-API RED precedes this new presentation implementation; independent Linux and macOS execution and full original workflow parity remain required.",
    "be9fd6690bf99980": "Typed successful Create fixtures preserve creation times 1000/1001, queued state, payload=id, explicit partition, due times 5000/2000, default priority 0 and state-entry sequences 1/2. The actual apply-selected parallel claim at2000 carries its second-record write and next control; only then is the queued FIFO patch prepared and its actual policy used to rebuild the next view. The FIFO claim at5000 is accepted empty while that observed lease remains live; completion at5100 uses its actual token/fence and carries the actual Changed record/control before the FIFO claim at5101 selects the exact first-record singleton. All three queries retain queued state, original partition, limit1, parallel-worker/fifo-worker and pinned default lease30000. Fixed original ID/type/partition prefixes replace only unique suffixes. Source812 links exact singleton cardinality,821 actual selected ID,824 accepted no-write complete selection,833 actual guarded Changed completion,840 final exact singleton/ID; helper return patterns enforce fresh acceptance but public tuple replies are not implemented. Stop/completed-state assertions are supplemental. Original helper assertions1384/1379 remain public setup obligations. Policy preparation and test-side reconstructed indexes do not execute committed installation, durable activation/backfill or atomic migration. Public replies/projections, routing/placement authority, public parent reclaim/governance/timeout orchestration, real committed encoding/hash verification, atomic durable record/index/history/outcome/control writes, persisted retained replay, FlowCase shard/WARaft/FLUSHDB/MemoryGuard setup/cleanup and every original execution profile remain omitted. First passing execution is existing-behavior coverage without retroactive TDD RED or full public/durable/profile parity; independent Linux and macOS execution remain required.",
    "62328c799831fd82": "Typed successful reduce(Create) fixtures and a fresh accepted apply-selected plan preserve the original first/second creation times 1000/1001, queued state, explicit partition, priority 1 in record metadata and query, payload=id, common due 2000, no policy, claim time 2000, limit 10, parallel-worker and exact order-insensitive two-ID set. Source line 13 links typed successful creation and retained type/state/payload only; line 25 links the actual fresh accepted claim decision only. Actual selected writes and next control are carried to the owning fixture; exact cardinality/complete stop and final running/lease/control checks are supplemental. Fixed original ID/type/partition prefixes replace only positive unique suffixes, and explicit lease_ms 30000 preserves the pinned default. Explicit monotone fixture sequences 1/2 retain creation order without certifying original Raft allocation. Public :ok/{:ok, ...} replies and record projections, routing/placement authority, public parent reclaim/governance/timeout orchestration, real committed encoding/hash verification, atomic durable record/index/history/outcome/control writes, persisted retained replay, FlowCase shard/WARaft/FLUSHDB/MemoryGuard setup/cleanup and every required original profile remain omitted. Rebuilt in-memory views and carried proposed records are not persistence evidence. This inert proposal supplies no assertion execution or passing status. A first passing run covers existing behavior without retroactive tests-first RED or complete original/public/durable/profile parity; independent Linux and macOS execution remain required.",
    "587581adc9eb0ceb": "Typed successful Create fixtures retain the original first/second creation times 1000/1001, queued state, explicit partition, payload=id and common due 2000; the original queued FIFO patch is prepared only after both creations and its actual resulting policy drives the claim fixture's index mode. Fresh apply-selected claims retain times 2000/2001, limit 10, fifo-worker, explicit original partition/state, exact first-ID singleton and later accepted empty selection. The actual first selected write and next control are carried into the second coherent view; complete-selection stop is supplemental. Fixed original ID/type/partition prefixes replace only positive unique suffixes, priority 0 represents omitted creation priority, explicit monotone fixture sequences 1/2 retain creation order without certifying source Raft allocation, and explicit lease_ms 30000 preserves the pinned default. Invoked original helper assertions at 1384 and 1379 remain public setup obligations and are not counted as direct case assertions. Successful policy preparation and test-side full-index reconstruction do not execute committed policy installation, durable activation/backfill, native rank maintenance or a migration transaction. Public :ok/{:ok, ...} replies and record projections, routing/placement authority, public parent reclaim/governance/timeout orchestration, real committed encoding/hash verification, atomic durable record/index/history/outcome/control writes, persisted retained replay, FlowCase shard/WARaft/FLUSHDB/MemoryGuard setup/cleanup and every required original profile remain omitted. This inert proposal supplies no assertion execution or passing status. A first passing run covers existing behavior without retroactive tests-first RED or complete original/public/durable/profile parity; independent Linux and macOS execution remain required.",
    "0bf563deac76e307": "Typed successful reduce(Create) fixture and fresh apply-selected plans retain the original type/state/payload, times, first limit 1 and later limits 10, worker and create→claim→leased-empty→guarded-complete→terminal-empty order. Source line 1250 links typed successful creation and retained state/payload only; line 1275 links Changed completion with the actual selected lease token/fence only. Public :ok/{:ok, ...} projections, automatic default partition placement/routing, public claim parent reclaim/governance/timeout orchestration, durable record/index/history/outcome/control writes, persisted retained replay, real encoded-entry/hash verification, FlowCase shard/WARaft/FLUSHDB/MemoryGuard setup and required original profiles remain omitted. Each view is rebuilt in memory from the current proposed record, rather than validating a persisted index mutation. Fixed ID/type prefixes replace unique suffixes; explicit lease_ms 30000 preserves the pinned default. Typed complete result/state and complete-selection stop checks are supplemental. The inert proposal supplies no assertion execution or passing status. A first passing run covers existing behavior and supplies no tests-first RED or full original/public/durable/profile parity. Independent Linux and macOS execution remain required.",
    "4bda857b47effb76": "Typed successful reduce(Create) fixture and fresh apply-selected plans preserve source input values, query ordering and empty/singleton/ID observations. Source line 1295 is linked only for typed successful creation and retained inputs, not its public :ok reply. Public :ok/{:ok, ...} projections, automatic default partition placement, durable state/index/outcome writes, FlowCase shard/WARaft/FLUSHDB/MemoryGuard setup and required original execution profiles are omitted. Fixed ID/type prefixes replace unique suffixes; explicit lease_ms 30000 preserves the pinned default. This test is existing-behavior coverage; first passing execution is neither preimplementation TDD nor full public/profile parity. Linux and macOS execution remain separately required.",
    "21028e29c3421d3a": "This retained fixture supplies PolicyResolved time; it does not exercise installed policy resolution/exhaustion.",
    "ecaed3fd34f388ec": "All 1,800 source input combinations are retained, but source phash2 jitter preparation is replaced by an explicit committed sample; source hash bytes are not certified.",
    "f72492d5892f240b": "Semantic type/state/partition lane identity is represented; encoded physical lane-key bytes and colocation are omitted.",
    "83dcf64f5941ec87": "Ordered Rust maps replace native index storage/rank operations; native backend behavior is omitted.",
    "49ba97e84872f9d0": "Exact member order and integer rank classes are represented; native inserts and floating score projection are omitted.",
    "9bdbecf61fe86d5f": "Same-snapshot continuation replaces two persisted claim calls; returned IDs and stop reasons are checked, but source singleton response patterns are not separately asserted.",
    "7d7eaf003a23555b": "Low-band singleton and both chosen IDs are checked; high-band singleton is not separately asserted.",
    "ed190d79e3813b5b": "The compound source assertion is linked only for ID, partition and state; compact response bytes, cardinality assertion, lease token and fence projection are omitted.",
    "e339e69ff4c5a0fa": "Retry run_at is an explicit 2100 fixture and initial claim prepares the reducer record; installed retry policy and initial selection assertions are omitted.",
    "d66bd07c7dc9d94e": "Separate state filters retain chosen IDs; source singleton response patterns are not separately asserted.",
    "be6beed30e05ded4": "Only the initial exact two-partition head set is represented; later live blockage and completion/reselection assertions are omitted.",
    "2c1a0a952f9bef71": "Only the initial listed-partition head set is represented; live blockage and later separate c claim are omitted.",
    "485560e32e405cf1": "Explicit expired-head selection/reducer reclaim represents the final observations; automatic reclaim scheduling and initial selection are omitted.",
    "ba618b494b159854": "Complete authoritative lane truth represents empty not-due selection; disposable state-index deletion and hot/cold storage are omitted. Eighty followers are supplemental.",
    "74f5fcdcf4328003": "Eighty followers and exact due ordering are retained with scan budget 1; create-many persistence and public get responses are omitted.",
    "9f5d395534240b02": "Two direct heads and eighty followers retain chosen global due order with scan budget 1; persisted creation and public response cardinality are omitted.",
    "bea9ae0f6f1e970c": "Only the earlier parallel ID under mixed state filters is represented; policy installation, payload return and public response cardinality are omitted.",
}


def digest(data):
    return hashlib.sha256(data).hexdigest()


def counterparts(manifest):
    declarations = {item["id"]: item for item in manifest["declarations"]}
    result = {}
    for source_id, rust_file, function, count, pairs in LINKS:
        source = declarations[source_id]
        source_lines = (ROOT / "tests/reference/elixir-baseline" / source["source_path"]).read_text().splitlines()
        rust_bytes = (ROOT / rust_file).read_bytes()
        rust_lines = rust_bytes.decode().splitlines()
        start = next(index for index, line in enumerate(rust_lines) if re.match(rf"^fn {re.escape(function)}\(", line))
        end = next((index for index in range(start + 1, len(rust_lines)) if rust_lines[index].startswith("fn ")), len(rust_lines))
        macros = [(index + 1, rust_lines[index].strip()) for index in range(start + 1, end)
                  if re.match(r"\s*assert(?:_eq|_ne)?!", rust_lines[index])]
        if len(macros) != count:
            raise ValueError(f"{function}: assertion layout changed; review before regenerating")
        assertions = []
        for source_line, rust_index in pairs:
            source_text = source_lines[source_line - 1].strip()
            if not source_text.startswith(("assert ", "refute ")):
                raise ValueError(f"{source_id}: missing source assertion at {source_line}")
            rust_line, rust_text = macros[rust_index]
            assertions.append(dict(source_line=source_line, source_text=source_text,
                                   rust_line=rust_line, rust_text=rust_text))
        limitations = ["Source-derived assertion mapping; registration and command execution are separate evidence."]
        if rust_file == "crates/wf-domain/tests/original_compact_history/source.rs":
            limitations.append(
                "Executed pure FSH2 scenarios retain the pinned terminal result reference, reason=ok, "
                "compact transition/version12/attempts2/fence9/created1000/at2000/shared payload, "
                "and the 160/190/180-byte context with 170-byte reason. Actual existing metadata "
                "normalization feeds the new encoder. All source field/size/omission observations "
                "are direct links. The original NIF-versus-Elixir byte and decode comparisons "
                "use independent source-derived 95/51/249-byte golden recipes and complete "
                "expected field lists as the Rust semantic counterpart; no native NIF or Elixir "
                "execution is claimed by those equality links. Every snapshot/context value "
                "is explicit mathematical input, without live record capture, atom/binary alias "
                "acquisition, JSON sidecar injection, public history-read authority or durable "
                "integration. Eight compiled rejecting API cases were saved in f217eee before "
                "the codec body; their complete tails pass unchanged. Wide integer carry "
                "vectors are additional after-body conformance without earlier RED credit. "
                "A separate pure-flag numeric equality regression was saved in bab403e "
                "before correction; exact integral float magnitudes retain nonstrict equality "
                "without rounding neighboring integers. A later complete signed integer "
                "exponent-range property is supplemental, with no native time-validation credit. "
                "Logical resource bounds and borrowed fields do not establish native RSS, "
                "allocation or performance gains. Original ExUnit async:true/flow context, "
                "native comparison, required profiles and independent Linux/macOS acceptance "
                "remain required. These mappings do not certify complete source-case or workflow parity."
            )
        if rust_file == "crates/wf-domain/tests/original_flow_value/source.rs":
            limitations.append(
                "Exact FlowCodecTest451/469 fixtures preserve payload-bytes, FSV1 plus ETF %{old: true}, "
                "FSV2+2+not-a-term escaping, %{kind: typed}, and typed repeated1024. The real logical "
                "encoder and additive compatibility decoder run all source assertions: raw/escaped "
                "roundtrips, exact typed map, typed prefix, trailing-zero raw suffix/user size, and "
                "valid level9 compressed inequality. A stdlib-zlib9 golden is independently expanded "
                "against the ETF recipe and checked against actual producer bytes; it is not a VM "
                "capture. Exact binary fallback and valid strict decoding are supplemental controls. "
                "Three compiled rejecting API cases were saved in e54c2dc before the compatibility "
                "body; all complete source/resource tails now pass. Safe tables are explicit immutable "
                "mathematical fixtures. A separate after-body completed-invalid-parse budget test "
                "rejects a temporary budget-reset fault; it carries no preimplementation RED claim. "
                "Live VM capture and public typed ingress are not established. "
                "The original suite uses ExUnit async:true and flow tagging without storage setup. "
                "Native original profiles and independent Linux/macOS acceptance remain required. "
                "These links do not establish full source-case/profile or workflow parity."
            )
        if rust_file == "crates/wf-domain/tests/history_values/overlay.rs":
            limitations.append("Exact pure HistoryValuesTest helper fixtures preserve arbitrary payload_ref=ref, missing_file with /private/secret/path, file-reference /segment/offset10/stored100 with cap10, or FSV2+2+not-an-external-term with cap1024. Source11 links the exact sanitized error; source22 links both omitted=true and stored-size100; source44/45 link the exact invalid-value error and absent payload. Typed ordered overlay writes represent the source map updates while retaining the original borrowed fields. These pure tests do not acquire physical values or execute BEAM/Bitcask; real coherent acquisition and backend fault controls are separate evidence. Links are added after the existing implementation and carry no retroactive tests-first RED. Original ExUnit flow-tag/setup, all required profiles and independent Linux/macOS acceptance remain open; this is not full case/workflow parity.")
        if rust_file == "crates/wf-storage/tests/state_store/workflow_values/history_hydration/source.rs":
            limitations.append("After-body storage conformance checks publish real consumed9 workflow data, capture coherent history/bindings, authenticate aliases and immutable bytes, convert actual sized reads and run the pure projector. They preserve exact source stored100/cap10 or FSV2+2+not-an-external-term/cap1024 and repeat after reopen. Genuine rendered aliases replace the source helper's arbitrary ref; /segment/offset10 is not a Rust physical layout promise. The exact arbitrary-ref/diagnostic/file-tuple pure fixtures are separately linked. Full-read and omission-before-decoding controls are supplemental. First execution is existing-behavior conformance, never preimplementation RED. Native ExUnit setup, all required profiles, public history authority/lifecycle and independent Linux acceptance remain required; local macOS execution does not establish full case/workflow parity.")
        if rust_file == "crates/wf-domain/tests/lease_lifecycle.rs":
            limitations.append("Pure single-record proposed writes only. Original fixtures execute separately with actual Create and Claim outputs carried forward. Fixed positive identity suffixes and distinct committed tokens normalize only generated identities; explicit omitted-source clocks are disclosed. Direct typed Changed/NotEligible/Error links represent business components, never public replies, selection cardinality, aliases/default dispatch, physical reads/index maintenance/history queries or atomic durable writes. Helper setup assertions are not mapped to unrelated direct test macros. Original FlowTest async:false, flow/global_state tags, 120000 timeout, live shards, FLUSHDB and cleanup, invoked public helpers and every required source execution profile remain open; no FlowCase MemoryGuard is added to this host. Independent Linux/macOS execution remains required. First PASS covers existing behavior without retroactive tests-first RED or full case parity.")
        if rust_file == FENCING:
            limitations.append("History cardinality counts reducer-emitted events in a test harness; no durable storage, partition selection, or response-loss transport is implemented.")
        if rust_file in (POLICY, RETRY) and source_id not in ("deb8dc5136bd2e4e", "9e5df11d9c3d3842", "3cc1821bd694c95e"):
            limitations.append("Borrowed retry policy/reducer observations only; policy installation/readback, partition-wide claim cardinality, durable history/query and public wire projections are omitted. Typed Some/None and numeric metadata represent observations, not string serialization.")
        if rust_file in (CLAIM, FIFO, LANE):
            limitations.append("Pure borrowed authoritative-view observations only; snapshot completeness/revision identity, FIFO policy installation, native/durable index maintenance, automatic reclaim and public claim projections remain adapter obligations. Fixed fixture IDs/times preserve source order rather than original generated namespaces.")
        if rust_file == "crates/wf-domain/tests/complete_policy/retry_input.rs":
            limitations.append("Executed borrowed original retry descriptors preserve the original thirty-day keyword recipe, fixed/linear/exponential timing rows, jitter bounds, all 1800 per-attempt raw normalizations and default/zero-retry observations. Source584 additionally preserves its original max_attempts=3, fixed100/1000, jitter0, failed recipe and exact caller rejection kind/body; it is distinct from stored source603 decoding. Repeated timing rows map to the one direct assertion executed for every corresponding table row, and the inclusive range assertion preserves both jitter bounds. Normalization success patterns enforced by the helper are not separately linked as direct macros. Explicit committed JitterSample replaces source phash2 preparation and supplies no hash/runtime identity proof; typed owners replace Elixir maps/error tuples. Visible descriptor limits do not prove raw parser correspondence, hidden term bytes or arbitrary map uniqueness. The source FlowTest async:false setup/tags/timeout, live shards/FLUSHDB/cleanup, public policy/lease/store operations, every mandatory runtime profile and independent Linux execution remain required. Original normalized counterpart IDs and limitations are preserved. The new tests preceded production with genuine missing-API RED and actually passed on macOS, while ledger anchors were added afterward; these component links do not certify whole source cases or full workflow parity.")
        elif rust_file == "crates/wf-domain/tests/complete_policy/retention_input.rs":
            limitations.append("Executed borrowed optional retention normalization preserves original type TTL60000/history512 and charge_card TTL30000/history256 inputs; the two direct scalar macros execute for both rows and link only source36/38/42/44. A separate original TTL5000/history_hot_max_events3 recipe links source140 to its real caller reason and exact body. Existing Copy RetentionOverride replaces scalar maps, without proving public map serialization or hot-field absence. Public set/get/default projection, authoritative installation, source error tuple, whole raw flow/state ingress, ETF sizing/emission, physical atomic publication and original FlowTest async:false, flow/global_state tags, 120000 timeout, shards/FLUSHDB/cleanup and all runtime profiles remain required. Captured C100/C10 with history50 and exact body10, changed later capture and dropped donors execute as additional pure controls, but source b0c4 line684 is a pattern assignment and its generation1/decoded-value assertion668 and ETS assertion692 remain UNLINKED. Neighbor TTL5000/history9 does not link source3e46 creation/inheritance/store assertions. Visible descriptor bounds do not prove hidden term bytes, arbitrary map identity or parser correspondence. Real tests-only missing-API RED preceded production; all86 policy controls and strict target lint actually pass on macOS. These post-GREEN ledger anchors preserve every older record and limitation, certify only the represented field/error components and establish neither whole source-case/full workflow parity nor independent Linux acceptance.")
        elif rust_file == "crates/wf-domain/tests/complete_policy/original_reply/source.rs":
            limitations.append("Executed original ETF reply views preserve the pinned flow-policy type/default retry and retention recipe, charge_card overrides and inherited defaults. Three direct test-body atom-field absence assertions link only source37/43/52: the type, all-states charge_card and selected charge_card retention maps omit history_hot_max_events. The manually encoded stored-policy recipe preserves source values but does not execute public keyword preprocessing, physical set/get calls or the original encoder. Source11/46 public success replies, authoritative/atomic/all-shard publication, coherent VM capture, opaque identity/Enumerable support, original setup/cleanup and every required runtime profile remain acceptance obligations. Other scalar observations and helper assertions add no new ledger links. Tests-only missing-API RED preceded production; the separate empty/atom-state lookup supplements were added after production and retain their actual provenance. Seven new domain tests and the real Fresh/Replay caller test pass on macOS; logical resource controls do not establish measured native allocation/speed, full source-case/workflow parity or independent Linux acceptance. Every older counterpart, limitation and runtime identity is preserved.")
        elif source_id == "2302fd8c2cfa076d" and rust_file == "crates/wf-domain/tests/complete_policy/original_entry.rs":
            limitations.append("Executed original ETF observations retain the exact generation-boundary/public-v1 term, generation 9007199254740991, and the above-maximum sibling. Source554 links generation, atom:type and the complete returned policy-map bytes; source558 links strict None refusal for MAX+1. The fixed hand recipe preserves the source term and values, not the native encoder's default map order. This counterpart does not execute the original encoder or migration allocator/exhaustion; those remain separately mapped normalized observations. The direct above-maximum ledger anchor is a post-implementation supplement with no new tests-first RED. Borrowed mathematical atom/runtime catalogs replace live VM context. Original ExUnit setup/runtime/profiles, public installation/physical reads, full case parity and independent Linux acceptance remain required.")
        elif rust_file == "crates/wf-ports/tests/record_batch/source.rs":
            limitations.append("Executed private mutation-byte composition preserves the pinned fail/complete bulk type/partition/worker/ID prefixes, sorted input order, run/claim1000 and terminal2000 recipes. Fixed suffixes and explicit create900 normalize generated identities and unspecified source clocks; complete success retains its explicit source900. Links cover only ordered IDs and terminal states, or BOTH original running/version2 observations after bad-first guard refusal. An additional reversed input exercises late refusal without relabeling the source order. Coherent borrowed views replace physical reads; source compound public error/success replies, error/result references, info counts, fetched histories, public normalization/routing and original setup/runtime profiles remain UNLINKED. Inline payload/result/error, proposed index/history changes, retained scalar acknowledgements and priority/FIFO controls supply additional component checks only. Genuine independently reproduced missing-API RED preceded production; finite test/layout repairs before SUT retain their actual timing. All six new tests pass on macOS, with no complete source-case/workflow parity, physical atomic durability, measured native resource gain or independent Linux result implied. Every older counterpart and runtime binding is preserved.")
        elif rust_file == "crates/wf-ports/tests/original_policy_get.rs":
            limitations.append("Executed pure original GET composition validates raw opts=[], invokes the recording policy reader once with allocator zero and the exact derived key, then uses the existing borrowed projector. Hand-encoded stored-value recipes retain generation-always-on-1 and generation-cas-1 with generation2/max_active2000, generation-canonical-get-1-0 with generation1/max_active1000, and foreign-policy-type with generation1/max_active9000 under generation-mismatched-read-1. Each record links only its declaration's direct generation/max-active components at source24/46/82 or exact Corrupt/body components at source101. The canonical type is a possible source template spelling; native routed-away hashing is unproved. The original SET/CAS sequences, normalization/encoder, stale replica deletion, all-shard setup, physical writes/reads, cache suppression, consistency barriers, authorization and original async:false/global_state/flush/runtime profiles remain required. Recording-call macros and selected charge_card helper assertions add no ledger credit. Seven new behavior tests pass on macOS after genuine independently reproduced missing-API RED; pre-SUT fixture refinements and later source/lint repairs retain their actual timing. Stage-specific work/key reservations do not prove summed external owners, measured native resources, full source-case/workflow parity or independent Linux acceptance. Every older counterpart and runtime binding is preserved.")
        elif source_id in LIMITS:
            limitations.append(LIMITS[source_id])
        existing = result.setdefault(source_id, [])
        scenario_id = f"m1-{source_id}" if not existing else f"m1-{source_id}-{function}"
        existing.append(dict(scenario_id=scenario_id, rust_file=rust_file,
                             rust_file_sha256=digest(rust_bytes), rust_module="",
                             rust_function=function, assertions=assertions,
                             limitations=limitations))
    return result


def previous_ledger(manifest, collection_path):
    path = ROOT / "tests/parity/workflow-progress.json"
    if not path.exists():
        return None
    previous = json.loads(path.read_bytes())
    if (previous.get("schema_version") != 1
            or previous.get("source_commit") != SOURCE_COMMIT
            or previous.get("reference_manifest_sha256") != MANIFEST_SHA256):
        raise ValueError("existing progress has unexpected provenance")
    entries = previous["declarations"]
    expected = {item["id"] for item in manifest["declarations"]}
    identities = [entry["source_declaration_id"] for entry in entries]
    if len(identities) != len(set(identities)) or set(identities) != expected:
        raise ValueError("existing progress has missing, duplicate or unknown declarations")
    scenarios = [item["scenario_id"] for entry in entries for item in entry["rust_assertions"]]
    if any(not item.strip() for item in scenarios) or len(scenarios) != len(set(scenarios)):
        raise ValueError("existing progress has empty or duplicate scenarios")
    if not collection_path and (previous["runtime_collection"] is not None
                                or any(entry["runtime_case_ids"] for entry in entries)):
        raise ValueError("existing runtime bindings require an explicit --collection")
    return previous


def preserve_reviewed_links(mappings, previous):
    if previous is None:
        return
    owners = {item["scenario_id"]: entry["source_declaration_id"]
              for entry in previous["declarations"] for item in entry["rust_assertions"]}
    generated = set()
    for source_id, records in mappings.items():
        for record in records:
            scenario = record["scenario_id"]
            if scenario in generated or owners.get(scenario, source_id) != source_id:
                raise ValueError("generated scenario is duplicated or changes source lineage")
            generated.add(scenario)
    for entry in previous["declarations"]:
        source_id = entry["source_declaration_id"]
        refreshed = {item["scenario_id"]: item for item in mappings.get(source_id, [])}
        records = [refreshed.pop(item["scenario_id"], item) for item in entry["rust_assertions"]]
        records.extend(refreshed.values())
        if records:
            mappings[source_id] = records


def generate(collection_path):
    manifest_bytes = (ROOT / "tests/reference/workflow-test-manifest.json").read_bytes()
    if digest(manifest_bytes) != MANIFEST_SHA256:
        raise ValueError("frozen manifest digest changed")
    manifest = json.loads(manifest_bytes)
    if manifest["source_commit"] != SOURCE_COMMIT:
        raise ValueError("unexpected source commit")
    previous = previous_ledger(manifest, collection_path)
    mappings = counterparts(manifest)
    preserve_reviewed_links(mappings, previous)
    registration = {}
    reference = None
    if collection_path:
        path = Path(collection_path)
        if path.is_absolute() or any(part in (".", "..") for part in path.parts):
            raise ValueError("collection must use a portable checkout-relative path")
        raw = (ROOT / path).read_bytes()
        collection = json.loads(raw)
        if collection["source_commit"] != SOURCE_COMMIT or collection["reference_manifest_sha256"] != MANIFEST_SHA256:
            raise ValueError("unexpected registration provenance")
        for case in collection["cases"]:
            registration.setdefault(case["source_declaration_id"], []).append(case["case_id"])
        reference = dict(path=path.as_posix(), sha256=digest(raw))
    entries = [dict(source_declaration_id=item["id"], runtime_case_ids=sorted(registration.get(item["id"], [])),
                    rust_assertions=mappings.get(item["id"], [])) for item in manifest["declarations"]]
    if set(registration) - {item["id"] for item in manifest["declarations"]}:
        raise ValueError("registration contains unknown declaration IDs")
    if previous is not None:
        for entry in previous["declarations"]:
            retained = registration.get(entry["source_declaration_id"], [])
            if not set(entry["runtime_case_ids"]).issubset(retained):
                raise ValueError("collection would remove existing runtime bindings")
    ledger = dict(schema_version=1, source_commit=SOURCE_COMMIT,
                  reference_manifest_sha256=MANIFEST_SHA256,
                  runtime_collection=reference, declarations=entries)
    (ROOT / "tests/parity/workflow-progress.json").write_text(json.dumps(ledger, indent=2) + "\n")
    print(f"Generated {len(entries)} source entries, {len(mappings)} reviewed assertion mappings; run check-progress to validate. No execution proof generated.")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--collection", help="coordinator-owned real registration JSON path")
    generate(parser.parse_args().collection)
