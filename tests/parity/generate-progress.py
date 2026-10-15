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
     "source_reclaim_boundary_preserves_claim_and_explicit_reclaim_scenarios", 24,
     [(772, 0), (774, 1), (783, 5), (792, 6), (793, 8), (794, 9), (795, 10)]),
    ("284e6d5206fd4919", "crates/wf-domain/tests/lease_lifecycle.rs",
     "source_reclaim_boundary_preserves_claim_and_explicit_reclaim_scenarios", 24,
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
]

# These limits describe this counterpart's represented observations, rather
# than the complete implementation status of another owner or milestone.
LIMITS = {
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
        if rust_file == "crates/wf-domain/tests/lease_lifecycle.rs":
            limitations.append("Pure single-record proposed writes only. Original fixtures execute separately with actual Create and Claim outputs carried forward. Fixed positive identity suffixes and distinct committed tokens normalize only generated identities; explicit omitted-source clocks are disclosed. Direct typed Changed/NotEligible/Error links represent business components, never public replies, selection cardinality, aliases/default dispatch, physical reads/index maintenance/history queries or atomic durable writes. Helper setup assertions are not mapped to unrelated direct test macros. Original FlowTest async:false, flow/global_state tags, 120000 timeout, live shards, FLUSHDB and cleanup, invoked public helpers and every required source execution profile remain open; no FlowCase MemoryGuard is added to this host. Independent Linux/macOS execution remains required. First PASS covers existing behavior without retroactive tests-first RED or full case parity.")
        if rust_file == FENCING:
            limitations.append("History cardinality counts reducer-emitted events in a test harness; no durable storage, partition selection, or response-loss transport is implemented.")
        if rust_file in (POLICY, RETRY):
            limitations.append("Borrowed retry policy/reducer observations only; policy installation/readback, partition-wide claim cardinality, durable history/query and public wire projections are omitted. Typed Some/None and numeric metadata represent observations, not string serialization.")
        if rust_file in (CLAIM, FIFO, LANE):
            limitations.append("Pure borrowed authoritative-view observations only; snapshot completeness/revision identity, FIFO policy installation, native/durable index maintenance, automatic reclaim and public claim projections remain adapter obligations. Fixed fixture IDs/times preserve source order rather than original generated namespaces.")
        if source_id == "2302fd8c2cfa076d" and rust_file == "crates/wf-domain/tests/complete_policy/original_entry.rs":
            limitations.append("Executed original ETF observations retain the exact generation-boundary/public-v1 term, generation 9007199254740991, and the above-maximum sibling. Source554 links generation, atom:type and the complete returned policy-map bytes; source558 links strict None refusal for MAX+1. The fixed hand recipe preserves the source term and values, not the native encoder's default map order. This counterpart does not execute the original encoder or migration allocator/exhaustion; those remain separately mapped normalized observations. The direct above-maximum ledger anchor is a post-implementation supplement with no new tests-first RED. Borrowed mathematical atom/runtime catalogs replace live VM context. Original ExUnit setup/runtime/profiles, public installation/physical reads, full case parity and independent Linux acceptance remain required.")
        elif source_id in LIMITS:
            limitations.append(LIMITS[source_id])
        existing = result.setdefault(source_id, [])
        scenario_id = f"m1-{source_id}" if not existing else f"m1-{source_id}-{function}"
        existing.append(dict(scenario_id=scenario_id, rust_file=rust_file,
                             rust_file_sha256=digest(rust_bytes), rust_module="",
                             rust_function=function, assertions=assertions,
                             limitations=limitations))
    return result


def generate(collection_path):
    manifest_bytes = (ROOT / "tests/reference/workflow-test-manifest.json").read_bytes()
    if digest(manifest_bytes) != MANIFEST_SHA256:
        raise ValueError("frozen manifest digest changed")
    manifest = json.loads(manifest_bytes)
    if manifest["source_commit"] != SOURCE_COMMIT:
        raise ValueError("unexpected source commit")
    mappings = counterparts(manifest)
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
    ledger = dict(schema_version=1, source_commit=SOURCE_COMMIT,
                  reference_manifest_sha256=MANIFEST_SHA256,
                  runtime_collection=reference, declarations=entries)
    (ROOT / "tests/parity/workflow-progress.json").write_text(json.dumps(ledger, indent=2) + "\n")
    print(f"Generated {len(entries)} source entries, {len(mappings)} reviewed assertion mappings; run check-progress to validate. No execution proof generated.")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--collection", help="coordinator-owned real registration JSON path")
    generate(parser.parse_args().collection)
