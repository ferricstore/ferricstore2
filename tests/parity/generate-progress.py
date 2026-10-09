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
]

# These limits describe this counterpart's represented observations, rather
# than the complete implementation status of another owner or milestone.
LIMITS = {
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
        if rust_file == FENCING:
            limitations.append("History cardinality counts reducer-emitted events in a test harness; no durable storage, partition selection, or response-loss transport is implemented.")
        if rust_file in (POLICY, RETRY):
            limitations.append("Borrowed retry policy/reducer observations only; policy installation/readback, partition-wide claim cardinality, durable history/query and public wire projections are omitted. Typed Some/None and numeric metadata represent observations, not string serialization.")
        if rust_file in (CLAIM, FIFO, LANE):
            limitations.append("Pure borrowed authoritative-view observations only; snapshot completeness/revision identity, FIFO policy installation, native/durable index maintenance, automatic reclaim and public claim projections remain adapter obligations. Fixed fixture IDs/times preserve source order rather than original generated namespaces.")
        if source_id in LIMITS:
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
