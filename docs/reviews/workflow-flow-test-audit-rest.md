# Flow test audit: the rest of the Elixir suite

The audits of ADR-014 steps 1 to 10 (`workflow-flow-test-audit.md`) and 11
to 15 (`workflow-flow-test-audit-11-15.md`) covered the Elixir tests of the
Flow commands. This map covers every other Elixir test file that can hold
a guarantee the node owes its clients: files that test Flows (three or
more mentions), every native protocol file, and the replication and
recovery files. Source: Elixir `main` 31994beda (25 Nov, 39 commits past
the 6a5f2bc34 pin of the earlier audits). Files with fewer mentions and
outside the native protocol test KV, Redis or storage behaviour the node
does not have, and are not listed.

Statuses are those of the earlier audits, plus **surface**: the test
checks a surface the node does not serve (KV and Redis commands, ACL and
AUTH, TLS, MULTI, PubSub, chunked frames, the dashboard and HTTP
endpoints). Gaps continue the numbering at G63.

## Files

| Status | Files | Tests |
|---|---:|---:|
| rows | 75 | 998 |
| internal | 189 | 2119 |
| surface | 113 | 1412 |

`rows` files are audited test by test below; the others are settled
for the whole file:

| File | Tests | Status | Reason |
|---|---:|---|---|
| `ferricstore/test/ferricstore/application_test.exs` | 25 | internal | Elixir OTP application, embedded-instance, lock or telemetry internals |
| `ferricstore/test/ferricstore/bench/flow_state_lmdb_soak_guard_test.exs` | 1 | internal | LMDB projection: the node keeps Flows in its shard store (ADR-016) |
| `ferricstore/test/ferricstore/bench/query_performance_benchmark_guard_test.exs` | 19 | internal | repository guard, benchmark or release check of the Elixir tree |
| `ferricstore/test/ferricstore/cache_stats_scope_test.exs` | 4 | internal | Elixir OTP application, embedded-instance, lock or telemetry internals |
| `ferricstore/test/ferricstore/ci_test_partition_test.exs` | 3 | internal | repository guard, benchmark or release check of the Elixir tree |
| `ferricstore/test/ferricstore/cluster/manager_test/sections/role_membership_mapping_part_2.exs` | 12 | internal | Elixir cluster manager internals |
| `ferricstore/test/ferricstore/cluster/target_data_probe_test.exs` | 9 | internal | Elixir cluster manager internals |
| `ferricstore/test/ferricstore/commands/catalog_test.exs` | 54 | surface | COMMAND catalog, INFO, extensions, invocations or the KV keyspace: not served |
| `ferricstore/test/ferricstore/commands/command_test.exs` | 44 | surface | COMMAND catalog, INFO, extensions, invocations or the KV keyspace: not served |
| `ferricstore/test/ferricstore/commands/extension_test.exs` | 11 | surface | COMMAND catalog, INFO, extensions, invocations or the KV keyspace: not served |
| `ferricstore/test/ferricstore/commands/internal_key_access_test.exs` | 9 | surface | COMMAND catalog, INFO, extensions, invocations or the KV keyspace: not served |
| `ferricstore/test/ferricstore/commands/invocation_test.exs` | 4 | surface | COMMAND catalog, INFO, extensions, invocations or the KV keyspace: not served |
| `ferricstore/test/ferricstore/commands/server_info_test.exs` | 53 | surface | COMMAND catalog, INFO, extensions, invocations or the KV keyspace: not served |
| `ferricstore/test/ferricstore/embedded_internal_key_access_test.exs` | 14 | surface | COMMAND catalog, INFO, extensions, invocations or the KV keyspace: not served |
| `ferricstore/test/ferricstore/flow/claim_due_cursor_table_test.exs` | 1 | internal | Elixir index, locator, codec or hydration internals |
| `ferricstore/test/ferricstore/flow/codec_history_snapshot_test.exs` | 3 | internal | Elixir index, locator, codec or hydration internals |
| `ferricstore/test/ferricstore/flow/codec_nif_schedule_test.exs` | 5 | internal | Elixir index, locator, codec or hydration internals |
| `ferricstore/test/ferricstore/flow/codec_sidecar_format_test.exs` | 5 | internal | Elixir index, locator, codec or hydration internals |
| `ferricstore/test/ferricstore/flow/cold_due_precheck_test.exs` | 2 | internal | Elixir index, locator, codec or hydration internals |
| `ferricstore/test/ferricstore/flow/cold_lmdb_schema_test.exs` | 16 | internal | LMDB projection: the node keeps Flows in its shard store (ADR-016) |
| `ferricstore/test/ferricstore/flow/command_scope_test.exs` | 2 | internal | Elixir index, locator, codec or hydration internals |
| `ferricstore/test/ferricstore/flow/due_catalog_test.exs` | 8 | internal | Elixir index, locator, codec or hydration internals |
| `ferricstore/test/ferricstore/flow/durable_index_marker_security_test.exs` | 4 | internal | Elixir index, locator, codec or hydration internals |
| `ferricstore/test/ferricstore/flow/hibernation_claim_test.exs` | 3 | internal | hibernation of far-future Flows: the node keeps live Flows in RAM and spills only finished ones (ADR-016) |
| `ferricstore/test/ferricstore/flow/hibernation_determinism_test.exs` | 5 | internal | hibernation of far-future Flows: the node keeps live Flows in RAM and spills only finished ones (ADR-016) |
| `ferricstore/test/ferricstore/flow/hibernation_test.exs` | 55 | internal | hibernation of far-future Flows: the node keeps live Flows in RAM and spills only finished ones (ADR-016) |
| `ferricstore/test/ferricstore/flow/history_log_merge_test.exs` | 3 | internal | Elixir projection, mirror, backfill or shard-transfer repair internals |
| `ferricstore/test/ferricstore/flow/history_log_rotation_test.exs` | 4 | internal | Elixir projection, mirror, backfill or shard-transfer repair internals |
| `ferricstore/test/ferricstore/flow/history_projector/config_test.exs` | 2 | internal | asynchronous history projector: the node writes history inside the apply |
| `ferricstore/test/ferricstore/flow/history_projector/key_codec_test.exs` | 4 | internal | asynchronous history projector: the node writes history inside the apply |
| `ferricstore/test/ferricstore/flow/history_projector/log_test.exs` | 1 | internal | asynchronous history projector: the node writes history inside the apply |
| `ferricstore/test/ferricstore/flow/history_projector/pending_registry_test.exs` | 4 | internal | asynchronous history projector: the node writes history inside the apply |
| `ferricstore/test/ferricstore/flow/history_projector/recovery_test.exs` | 12 | internal | asynchronous history projector: the node writes history inside the apply |
| `ferricstore/test/ferricstore/flow/history_projector/retry_test.exs` | 2 | internal | asynchronous history projector: the node writes history inside the apply |
| `ferricstore/test/ferricstore/flow/history_projector/storage_test.exs` | 3 | internal | asynchronous history projector: the node writes history inside the apply |
| `ferricstore/test/ferricstore/flow/history_projector/telemetry_test.exs` | 2 | internal | asynchronous history projector: the node writes history inside the apply |
| `ferricstore/test/ferricstore/flow/history_projector/tombstone_recovery_test.exs` | 5 | internal | asynchronous history projector: the node writes history inside the apply |
| `ferricstore/test/ferricstore/flow/history_projector/trim_test.exs` | 6 | internal | asynchronous history projector: the node writes history inside the apply |
| `ferricstore/test/ferricstore/flow/history_projector/value_projection_test.exs` | 5 | internal | asynchronous history projector: the node writes history inside the apply |
| `ferricstore/test/ferricstore/flow/history_projector_kill_test.exs` | 1 | internal | asynchronous history projector: the node writes history inside the apply |
| `ferricstore/test/ferricstore/flow/history_projector_pending_test.exs` | 9 | internal | asynchronous history projector: the node writes history inside the apply |
| `ferricstore/test/ferricstore/flow/history_projector_request_test.exs` | 10 | internal | asynchronous history projector: the node writes history inside the apply |
| `ferricstore/test/ferricstore/flow/history_projector_streaming_scan_test.exs` | 5 | internal | asynchronous history projector: the node writes history inside the apply |
| `ferricstore/test/ferricstore/flow/history_projector_test/sections/async_enqueue_rejects_above_configured_pending_cap_so_apply_fall_back_sy.exs` | 12 | internal | asynchronous history projector: the node writes history inside the apply |
| `ferricstore/test/ferricstore/flow/history_projector_test/sections/sync_projection_hydrates_spilled_waraft_apply_projection_values_batch.exs` | 16 | internal | asynchronous history projector: the node writes history inside the apply |
| `ferricstore/test/ferricstore/flow/history_projector_test/sections/sync_projection_writes_dedicated_history_log_updates_index_advances_wate.exs` | 11 | internal | asynchronous history projector: the node writes history inside the apply |
| `ferricstore/test/ferricstore/flow/history_recovery_checkpoint_test.exs` | 11 | internal | Elixir projection, mirror, backfill or shard-transfer repair internals |
| `ferricstore/test/ferricstore/flow/history_value_release_worker_test.exs` | 2 | internal | Elixir projection, mirror, backfill or shard-transfer repair internals |
| `ferricstore/test/ferricstore/flow/index_merge_test.exs` | 15 | internal | Elixir index, locator, codec or hydration internals |
| `ferricstore/test/ferricstore/flow/internal_key_test.exs` | 12 | internal | Elixir index, locator, codec or hydration internals |
| `ferricstore/test/ferricstore/flow/keydir_spill_test.exs` | 14 | internal | Elixir index, locator, codec or hydration internals |
| `ferricstore/test/ferricstore/flow/keys_digest_test.exs` | 2 | internal | Elixir index, locator, codec or hydration internals |
| `ferricstore/test/ferricstore/flow/lmdb_bounded_range_test.exs` | 8 | internal | LMDB projection: the node keeps Flows in its shard store (ADR-016) |
| `ferricstore/test/ferricstore/flow/lmdb_compare_batch_test.exs` | 3 | internal | LMDB projection: the node keeps Flows in its shard store (ADR-016) |
| `ferricstore/test/ferricstore/flow/lmdb_flush_coordinator_test.exs` | 9 | internal | LMDB projection: the node keeps Flows in its shard store (ADR-016) |
| `ferricstore/test/ferricstore/flow/lmdb_history_scan_perf_guard_test.exs` | 1 | internal | LMDB projection: the node keeps Flows in its shard store (ADR-016) |
| `ferricstore/test/ferricstore/flow/lmdb_history_scan_test.exs` | 4 | internal | LMDB projection: the node keeps Flows in its shard store (ADR-016) |
| `ferricstore/test/ferricstore/flow/lmdb_index_codec_test.exs` | 15 | internal | LMDB projection: the node keeps Flows in its shard store (ADR-016) |
| `ferricstore/test/ferricstore/flow/lmdb_index_decode_test.exs` | 12 | internal | LMDB projection: the node keeps Flows in its shard store (ADR-016) |
| `ferricstore/test/ferricstore/flow/lmdb_index_read_test.exs` | 15 | internal | LMDB projection: the node keeps Flows in its shard store (ADR-016) |
| `ferricstore/test/ferricstore/flow/lmdb_mirror_enqueue_repair_test.exs` | 1 | internal | LMDB projection: the node keeps Flows in its shard store (ADR-016) |
| `ferricstore/test/ferricstore/flow/lmdb_mirror_test.exs` | 5 | internal | LMDB projection: the node keeps Flows in its shard store (ADR-016) |
| `ferricstore/test/ferricstore/flow/lmdb_moved_env_test.exs` | 1 | internal | LMDB projection: the node keeps Flows in its shard store (ADR-016) |
| `ferricstore/test/ferricstore/flow/lmdb_physical_key_test.exs` | 3 | internal | LMDB projection: the node keeps Flows in its shard store (ADR-016) |
| `ferricstore/test/ferricstore/flow/lmdb_query_window_test.exs` | 5 | internal | LMDB projection: the node keeps Flows in its shard store (ADR-016) |
| `ferricstore/test/ferricstore/flow/lmdb_rebuilder_active_indexes_test.exs` | 5 | internal | LMDB projection: the node keeps Flows in its shard store (ADR-016) |
| `ferricstore/test/ferricstore/flow/lmdb_rebuilder_cold_state_test.exs` | 13 | internal | LMDB projection: the node keeps Flows in its shard store (ADR-016) |
| `ferricstore/test/ferricstore/flow/lmdb_rebuilder_prune_race_test.exs` | 3 | internal | LMDB projection: the node keeps Flows in its shard store (ADR-016) |
| `ferricstore/test/ferricstore/flow/lmdb_rebuilder_terminal_counts_test.exs` | 3 | internal | LMDB projection: the node keeps Flows in its shard store (ADR-016) |
| `ferricstore/test/ferricstore/flow/lmdb_rebuilder_terminal_projection_test.exs` | 6 | internal | LMDB projection: the node keeps Flows in its shard store (ADR-016) |
| `ferricstore/test/ferricstore/flow/lmdb_rebuilder_terminal_state_test.exs` | 5 | internal | LMDB projection: the node keeps Flows in its shard store (ADR-016) |
| `ferricstore/test/ferricstore/flow/lmdb_rebuilder_test.exs` | 22 | internal | LMDB projection: the node keeps Flows in its shard store (ADR-016) |
| `ferricstore/test/ferricstore/flow/lmdb_retention_test.exs` | 20 | internal | LMDB projection: the node keeps Flows in its shard store (ADR-016) |
| `ferricstore/test/ferricstore/flow/lmdb_segment_pins_test.exs` | 9 | internal | LMDB projection: the node keeps Flows in its shard store (ADR-016) |
| `ferricstore/test/ferricstore/flow/lmdb_terminal_counts_test.exs` | 5 | internal | LMDB projection: the node keeps Flows in its shard store (ADR-016) |
| `ferricstore/test/ferricstore/flow/lmdb_test.exs` | 27 | internal | LMDB projection: the node keeps Flows in its shard store (ADR-016) |
| `ferricstore/test/ferricstore/flow/lmdb_value_codec_test.exs` | 6 | internal | LMDB projection: the node keeps Flows in its shard store (ADR-016) |
| `ferricstore/test/ferricstore/flow/lmdb_writer/after_flush_generation_test.exs` | 6 | internal | LMDB projection: the node keeps Flows in its shard store (ADR-016) |
| `ferricstore/test/ferricstore/flow/lmdb_writer/after_flush_prune_race_test.exs` | 9 | internal | LMDB projection: the node keeps Flows in its shard store (ADR-016) |
| `ferricstore/test/ferricstore/flow/lmdb_writer/after_flush_test.exs` | 16 | internal | LMDB projection: the node keeps Flows in its shard store (ADR-016) |
| `ferricstore/test/ferricstore/flow/lmdb_writer/config_test.exs` | 5 | internal | LMDB projection: the node keeps Flows in its shard store (ADR-016) |
| `ferricstore/test/ferricstore/flow/lmdb_writer/control_test.exs` | 2 | internal | LMDB projection: the node keeps Flows in its shard store (ADR-016) |
| `ferricstore/test/ferricstore/flow/lmdb_writer/enqueue_control_test.exs` | 3 | internal | LMDB projection: the node keeps Flows in its shard store (ADR-016) |
| `ferricstore/test/ferricstore/flow/lmdb_writer/flush_failure_recovery_test.exs` | 1 | internal | LMDB projection: the node keeps Flows in its shard store (ADR-016) |
| `ferricstore/test/ferricstore/flow/lmdb_writer/outbox_test.exs` | 14 | internal | LMDB projection: the node keeps Flows in its shard store (ADR-016) |
| `ferricstore/test/ferricstore/flow/lmdb_writer/projection_ops_test.exs` | 39 | internal | LMDB projection: the node keeps Flows in its shard store (ADR-016) |
| `ferricstore/test/ferricstore/flow/lmdb_writer/quarantine_test.exs` | 4 | internal | LMDB projection: the node keeps Flows in its shard store (ADR-016) |
| `ferricstore/test/ferricstore/flow/lmdb_writer/replay_safe_request_test.exs` | 1 | internal | LMDB projection: the node keeps Flows in its shard store (ADR-016) |
| `ferricstore/test/ferricstore/flow/lmdb_writer/restart_recovery_test.exs` | 2 | internal | LMDB projection: the node keeps Flows in its shard store (ADR-016) |
| `ferricstore/test/ferricstore/flow/lmdb_writer/shards_test.exs` | 3 | internal | LMDB projection: the node keeps Flows in its shard store (ADR-016) |
| `ferricstore/test/ferricstore/flow/lmdb_writer/source_wait_test.exs` | 4 | internal | LMDB projection: the node keeps Flows in its shard store (ADR-016) |
| `ferricstore/test/ferricstore/flow/lmdb_writer/timer_test.exs` | 1 | internal | LMDB projection: the node keeps Flows in its shard store (ADR-016) |
| `ferricstore/test/ferricstore/flow/locator_test.exs` | 10 | internal | Elixir index, locator, codec or hydration internals |
| `ferricstore/test/ferricstore/flow/long_state_key_reconcile_test.exs` | 1 | internal | Elixir projection, mirror, backfill or shard-transfer repair internals |
| `ferricstore/test/ferricstore/flow/ordered_index_earliest_due_test.exs` | 2 | internal | Elixir index, locator, codec or hydration internals |
| `ferricstore/test/ferricstore/flow/ordered_index_test.exs` | 38 | internal | Elixir index, locator, codec or hydration internals |
| `ferricstore/test/ferricstore/flow/orphan_purge_test.exs` | 7 | internal | Elixir projection, mirror, backfill or shard-transfer repair internals |
| `ferricstore/test/ferricstore/flow/policy_backfill_read_cache_test.exs` | 1 | internal | Elixir projection, mirror, backfill or shard-transfer repair internals |
| `ferricstore/test/ferricstore/flow/policy_mirror_recovery_test.exs` | 12 | internal | Elixir projection, mirror, backfill or shard-transfer repair internals |
| `ferricstore/test/ferricstore/flow/policy_mirror_writer_restart_test.exs` | 1 | internal | Elixir projection, mirror, backfill or shard-transfer repair internals |
| `ferricstore/test/ferricstore/flow/projected_value_apply_test.exs` | 3 | internal | Elixir projection, mirror, backfill or shard-transfer repair internals |
| `ferricstore/test/ferricstore/flow/projected_value_level_test.exs` | 2 | internal | Elixir projection, mirror, backfill or shard-transfer repair internals |
| `ferricstore/test/ferricstore/flow/projected_value_read_test.exs` | 3 | internal | Elixir projection, mirror, backfill or shard-transfer repair internals |
| `ferricstore/test/ferricstore/flow/projection_locator_test.exs` | 3 | internal | Elixir index, locator, codec or hydration internals |
| `ferricstore/test/ferricstore/flow/query_index_provider_test.exs` | 5 | internal | Elixir index, locator, codec or hydration internals |
| `ferricstore/test/ferricstore/flow/ram_index_read_test.exs` | 4 | internal | Elixir index, locator, codec or hydration internals |
| `ferricstore/test/ferricstore/flow/record_hydrator_test.exs` | 19 | internal | Elixir index, locator, codec or hydration internals |
| `ferricstore/test/ferricstore/flow/record_loader_test.exs` | 3 | internal | Elixir index, locator, codec or hydration internals |
| `ferricstore/test/ferricstore/flow/record_read_availability_test.exs` | 3 | internal | Elixir index, locator, codec or hydration internals |
| `ferricstore/test/ferricstore/flow/retention_cleanup_mirror_lag_test.exs` | 2 | internal | Elixir projection, mirror, backfill or shard-transfer repair internals |
| `ferricstore/test/ferricstore/flow/retention_missing_mirror_test.exs` | 1 | internal | Elixir projection, mirror, backfill or shard-transfer repair internals |
| `ferricstore/test/ferricstore/flow/score_bound_test.exs` | 3 | internal | Elixir index, locator, codec or hydration internals |
| `ferricstore/test/ferricstore/flow/shared_ref_backfill_test.exs` | 41 | internal | Elixir projection, mirror, backfill or shard-transfer repair internals |
| `ferricstore/test/ferricstore/flow/shared_scope_test.exs` | 13 | surface | shared multi-tenant scope bound by ACL tenants: the node has no tenants |
| `ferricstore/test/ferricstore/flow/signal_digest_level_test.exs` | 1 | internal | cluster feature-level rollout gates: the node has one format |
| `ferricstore/test/ferricstore/flow/signal_idempotency_level_test.exs` | 2 | internal | cluster feature-level rollout gates: the node has one format |
| `ferricstore/test/ferricstore/flow/transfer/retag_test.exs` | 6 | internal | Elixir projection, mirror, backfill or shard-transfer repair internals |
| `ferricstore/test/ferricstore/flow/transfer_format_upgrade_test.exs` | 1 | internal | Elixir projection, mirror, backfill or shard-transfer repair internals |
| `ferricstore/test/ferricstore/flow/transfer_keys_test.exs` | 3 | internal | Elixir projection, mirror, backfill or shard-transfer repair internals |
| `ferricstore/test/ferricstore/flow/transfer_test.exs` | 13 | internal | Elixir projection, mirror, backfill or shard-transfer repair internals |
| `ferricstore/test/ferricstore/flow/value_hydration_test.exs` | 8 | internal | Elixir index, locator, codec or hydration internals |
| `ferricstore/test/ferricstore/flow_history_retention_guard_test.exs` | 1 | internal | repository guard, benchmark or release check of the Elixir tree |
| `ferricstore/test/ferricstore/flow_lmdb_test/sections/async_terminal_history_cold_only_after_lmdb_projection.exs` | 9 | internal | LMDB projection: the node keeps Flows in its shard store (ADR-016) |
| `ferricstore/test/ferricstore/flow_lmdb_test/sections/default_startup_repairs_active_projection_clears_stale_lmdb_flush_marker.exs` | 22 | internal | LMDB projection: the node keeps Flows in its shard store (ADR-016) |
| `ferricstore/test/ferricstore/flow_lmdb_test/sections/history_projector_fsyncs_copied_generated_values_before_publishing_lmdb.exs` | 18 | internal | LMDB projection: the node keeps Flows in its shard store (ADR-016) |
| `ferricstore/test/ferricstore/flow_lmdb_test/sections/lineage_include_cold_reverse_reads_newest_lmdb_prefix_rows.exs` | 12 | internal | LMDB projection: the node keeps Flows in its shard store (ADR-016) |
| `ferricstore/test/ferricstore/flow_lmdb_test/sections/mirror_flow_reads_reject_stale_lmdb_record_fall_back_bitcask_truth.exs` | 13 | internal | LMDB projection: the node keeps Flows in its shard store (ADR-016) |
| `ferricstore/test/ferricstore/flow_lmdb_test/sections/mirror_writer_projects_terminal_history_hot_flow_index_during_flush.exs` | 12 | internal | LMDB projection: the node keeps Flows in its shard store (ADR-016) |
| `ferricstore/test/ferricstore/flow_lmdb_test/sections/partial_retention_cleanup_keeps_values_still_referenced_terminal_state.exs` | 9 | internal | LMDB projection: the node keeps Flows in its shard store (ADR-016) |
| `ferricstore/test/ferricstore/flow_lmdb_test/sections/startup_keeps_terminal_history_cold_while_durable_history_remains_querya.exs` | 4 | internal | LMDB projection: the node keeps Flows in its shard store (ADR-016) |
| `ferricstore/test/ferricstore/flow_lmdb_test/sections/startup_rebuilds_active_flow_indexes_dedicated_lmdb_active_index.exs` | 19 | internal | LMDB projection: the node keeps Flows in its shard store (ADR-016) |
| `ferricstore/test/ferricstore/flow_lmdb_test/sections/state_machine_still_requires_lmdb_mirror_enqueue_mode_configured_off.exs` | 14 | internal | LMDB projection: the node keeps Flows in its shard store (ADR-016) |
| `ferricstore/test/ferricstore/flow_lmdb_test/sections/warm_opens_empty_shard_env_before_first_user_read.exs` | 25 | internal | LMDB projection: the node keeps Flows in its shard store (ADR-016) |
| `ferricstore/test/ferricstore/instance_test.exs` | 37 | internal | Elixir OTP application, embedded-instance, lock or telemetry internals |
| `ferricstore/test/ferricstore/isolated_instance_test.exs` | 8 | internal | Elixir OTP application, embedded-instance, lock or telemetry internals |
| `ferricstore/test/ferricstore/local_lock_test.exs` | 7 | internal | Elixir OTP application, embedded-instance, lock or telemetry internals |
| `ferricstore/test/ferricstore/memory_budget_test.exs` | 10 | internal | Elixir OTP application, embedded-instance, lock or telemetry internals |
| `ferricstore/test/ferricstore/metrics_test.exs` | 21 | internal | Elixir OTP application, embedded-instance, lock or telemetry internals |
| `ferricstore/test/ferricstore/observability/logging_noise_guard_test.exs` | 9 | internal | repository guard, benchmark or release check of the Elixir tree |
| `ferricstore/test/ferricstore/operational_guard_test.exs` | 12 | internal | repository guard, benchmark or release check of the Elixir tree |
| `ferricstore/test/ferricstore/production_defaults_test.exs` | 25 | internal | repository guard, benchmark or release check of the Elixir tree |
| `ferricstore/test/ferricstore/raft/apply_projection_retention_integration_test.exs` | 4 | internal | Elixir state-machine, KV apply, blob or wa_raft internals |
| `ferricstore/test/ferricstore/raft/blob_command_test.exs` | 33 | internal | Elixir state-machine, KV apply, blob or wa_raft internals |
| `ferricstore/test/ferricstore/raft/command_atoms_cluster_test.exs` | 2 | internal | Elixir state-machine, KV apply, blob or wa_raft internals |
| `ferricstore/test/ferricstore/raft/command_batching_test.exs` | 2 | internal | Elixir state-machine, KV apply, blob or wa_raft internals |
| `ferricstore/test/ferricstore/raft/command_codec_test.exs` | 7 | internal | Elixir state-machine, KV apply, blob or wa_raft internals |
| `ferricstore/test/ferricstore/raft/flow_history_order_level_test.exs` | 2 | internal | cluster feature-level rollout gates: the node has one format |
| `ferricstore/test/ferricstore/raft/flush_shard_apply_test.exs` | 10 | internal | Elixir state-machine, KV apply, blob or wa_raft internals |
| `ferricstore/test/ferricstore/raft/lmdb_projection_command_contract_test.exs` | 3 | internal | LMDB projection: the node keeps Flows in its shard store (ADR-016) |
| `ferricstore/test/ferricstore/raft/server_catalog_apply_test.exs` | 10 | internal | Elixir state-machine, KV apply, blob or wa_raft internals |
| `ferricstore/test/ferricstore/raft/state_machine_process_dict_guard_test.exs` | 9 | internal | repository guard, benchmark or release check of the Elixir tree |
| `ferricstore/test/ferricstore/raft/state_machine_test.exs` | 31 | internal | Elixir state-machine, KV apply, blob or wa_raft internals |
| `ferricstore/test/ferricstore/raft/state_machine_test/sections/apply_3_delete_key.exs` | 102 | internal | Elixir state-machine, KV apply, blob or wa_raft internals |
| `ferricstore/test/ferricstore/raft/state_machine_test/sections/apply_3_put_key_value_expire_at_ms_part_1.exs` | 28 | internal | Elixir state-machine, KV apply, blob or wa_raft internals |
| `ferricstore/test/ferricstore/raft/state_machine_test/sections/apply_3_set_key_value_expire_at_ms_opts.exs` | 51 | internal | Elixir state-machine, KV apply, blob or wa_raft internals |
| `ferricstore/test/ferricstore/raft/state_machine_test/sections/coalesces_consecutive_flow_native_index_ops_crossing_ordering_barriers.exs` | 13 | internal | Elixir state-machine, KV apply, blob or wa_raft internals |
| `ferricstore/test/ferricstore/raft/state_machine_test/sections/flow_blob_side_channel_apply.exs` | 7 | internal | Elixir state-machine, KV apply, blob or wa_raft internals |
| `ferricstore/test/ferricstore/raft/state_machine_test/sections/flow_shared_value_ref_persistence.exs` | 2 | internal | Elixir state-machine, KV apply, blob or wa_raft internals |
| `ferricstore/test/ferricstore/raft/state_machine_test/sections/release_cursor_log_compaction.exs` | 27 | internal | Elixir state-machine, KV apply, blob or wa_raft internals |
| `ferricstore/test/ferricstore/raft/state_machine_test/sections/spop_removes_type_marker_final_set_member_popped.exs` | 14 | internal | Elixir state-machine, KV apply, blob or wa_raft internals |
| `ferricstore/test/ferricstore/raft/waraft_backend_test.exs` | 1 | internal | wa_raft backend/storage: the node replicates with OpenRaft over its own group log (ADR-018) |
| `ferricstore/test/ferricstore/raft/waraft_backend_test/sections/acked_writes_survive_waraft_server_kill_during_active_write_load.exs` | 28 | internal | wa_raft backend/storage: the node replicates with OpenRaft over its own group log (ADR-018) |
| `ferricstore/test/ferricstore/raft/waraft_backend_test/sections/flow_retention_cleanup_scans_all_waraft_shards.exs` | 14 | internal | wa_raft backend/storage: the node replicates with OpenRaft over its own group log (ADR-018) |
| `ferricstore/test/ferricstore/raft/waraft_backend_test/sections/rejects_volatile_waraft_ets_log_module.exs` | 32 | internal | wa_raft backend/storage: the node replicates with OpenRaft over its own group log (ADR-018) |
| `ferricstore/test/ferricstore/raft/waraft_backend_test/sections/restart_recovers_missing_current_storage_metadata_using_previous_durable.exs` | 23 | internal | wa_raft backend/storage: the node replicates with OpenRaft over its own group log (ADR-018) |
| `ferricstore/test/ferricstore/raft/waraft_backend_test/sections/segment_projection_trim_without_stall.exs` | 16 | internal | wa_raft backend/storage: the node replicates with OpenRaft over its own group log (ADR-018) |
| `ferricstore/test/ferricstore/raft/waraft_backend_test/sections/snapshot_copy_durability.exs` | 3 | internal | wa_raft backend/storage: the node replicates with OpenRaft over its own group log (ADR-018) |
| `ferricstore/test/ferricstore/raft/waraft_backend_test/sections/snapshot_transfer_wraps_waraft_transport_exits.exs` | 23 | internal | wa_raft backend/storage: the node replicates with OpenRaft over its own group log (ADR-018) |
| `ferricstore/test/ferricstore/raft/waraft_backend_test/sections/startup_finalizes_interrupted_snapshot_swap_after_metadata_persisted.exs` | 30 | internal | wa_raft backend/storage: the node replicates with OpenRaft over its own group log (ADR-018) |
| `ferricstore/test/ferricstore/raft/waraft_backend_test/sections/storage_metadata_hot_writes_fsync_journal_rewriting_current_metadata.exs` | 18 | internal | wa_raft backend/storage: the node replicates with OpenRaft over its own group log (ADR-018) |
| `ferricstore/test/ferricstore/raft/waraft_backend_test/sections/unified_segment_trim_prunes_flow_apply_projection_value_cache.exs` | 33 | internal | wa_raft backend/storage: the node replicates with OpenRaft over its own group log (ADR-018) |
| `ferricstore/test/ferricstore/raft/waraft_backend_test/sections/waraft_generic_batches_coalesce_behind_in_flight_flush_default.exs` | 20 | internal | wa_raft backend/storage: the node replicates with OpenRaft over its own group log (ADR-018) |
| `ferricstore/test/ferricstore/raft/waraft_backend_test/sections/waraft_storage_close_flushes_async_flow_history_before_persisting_replay.exs` | 23 | internal | wa_raft backend/storage: the node replicates with OpenRaft over its own group log (ADR-018) |
| `ferricstore/test/ferricstore/raft/waraft_backend_test/sections/waraft_storage_recovery_reuses_segment_locations_flow_replay.exs` | 33 | internal | wa_raft backend/storage: the node replicates with OpenRaft over its own group log (ADR-018) |
| `ferricstore/test/ferricstore/raft/waraft_segment_projection_metrics_test.exs` | 1 | internal | wa_raft backend/storage: the node replicates with OpenRaft over its own group log (ADR-018) |
| `ferricstore/test/ferricstore/raft/waraft_segment_reader_security_test.exs` | 19 | internal | wa_raft backend/storage: the node replicates with OpenRaft over its own group log (ADR-018) |
| `ferricstore/test/ferricstore/raft/waraft_storage_hot_path_guard_test.exs` | 12 | internal | wa_raft backend/storage: the node replicates with OpenRaft over its own group log (ADR-018) |
| `ferricstore/test/ferricstore/readme_capability_contract_test.exs` | 5 | internal | repository guard, benchmark or release check of the Elixir tree |
| `ferricstore/test/ferricstore/store/blob_side_channel_test/sections/ra_generic_batch_accepts_pre_externalized_blob_refs.exs` | 24 | internal | Bitcask/ETS store internals |
| `ferricstore/test/ferricstore/store/blob_store_table_owner_test.exs` | 7 | internal | Bitcask/ETS store internals |
| `ferricstore/test/ferricstore/store/compaction_journal_test.exs` | 13 | internal | Bitcask/ETS store internals |
| `ferricstore/test/ferricstore/store/compaction_plan_test.exs` | 12 | internal | Bitcask/ETS store internals |
| `ferricstore/test/ferricstore/store/flush_durable_enumeration_test.exs` | 11 | internal | Bitcask/ETS store internals |
| `ferricstore/test/ferricstore/store/internal_key_visibility_test.exs` | 2 | internal | Bitcask/ETS store internals |
| `ferricstore/test/ferricstore/store/keydir_concurrency_test.exs` | 15 | internal | Bitcask/ETS store internals |
| `ferricstore/test/ferricstore/store/lfu_decay_test.exs` | 32 | internal | Bitcask/ETS store internals |
| `ferricstore/test/ferricstore/store/namespace_usage_test.exs` | 2 | internal | Bitcask/ETS store internals |
| `ferricstore/test/ferricstore/store/read_result_test.exs` | 32 | internal | Bitcask/ETS store internals |
| `ferricstore/test/ferricstore/store/router_flow_batch_contract_test.exs` | 6 | internal | Bitcask/ETS store internals |
| `ferricstore/test/ferricstore/store/router_query_row_read_test.exs` | 1 | internal | Bitcask/ETS store internals |
| `ferricstore/test/ferricstore/store/scan_read_cache_test.exs` | 4 | internal | Bitcask/ETS store internals |
| `ferricstore/test/ferricstore/store/shard/namespace_usage_index_test.exs` | 22 | internal | Bitcask/ETS store internals |
| `ferricstore/test/ferricstore/store/shard_async_io_test/sections/shared_log_compaction.exs` | 30 | internal | Bitcask/ETS store internals |
| `ferricstore/test/ferricstore/store/slot_map_test.exs` | 25 | internal | Bitcask/ETS store internals |
| `ferricstore/test/ferricstore/test_support/shard_helpers_test.exs` | 14 | internal | repository guard, benchmark or release check of the Elixir tree |
| `ferricstore/test/ferricstore/transaction/watch_index_test.exs` | 9 | surface | MULTI/WATCH transactions: not served |
| `ferricstore_http/test/ferricstore_http/command_service_test.exs` | 8 | surface | ferricstore_http invocation/HTTP-SDK surface: not served by the node |
| `ferricstore_http/test/ferricstore_http/invocation_result_values_test.exs` | 6 | surface | ferricstore_http invocation/HTTP-SDK surface: not served by the node |
| `ferricstore_http/test/ferricstore_http/invocation_runner_test.exs` | 6 | surface | ferricstore_http invocation/HTTP-SDK surface: not served by the node |
| `ferricstore_http/test/ferricstore_http/invocations_backend_test.exs` | 5 | surface | ferricstore_http invocation/HTTP-SDK surface: not served by the node |
| `ferricstore_http/test/ferricstore_http/oss_integration_test.exs` | 13 | surface | ferricstore_http invocation/HTTP-SDK surface: not served by the node |
| `ferricstore_http/test/ferricstore_http/python_sdk_integration_test.exs` | 1 | surface | ferricstore_http invocation/HTTP-SDK surface: not served by the node |
| `ferricstore_http/test/ferricstore_http/sdk_integration_test.exs` | 3 | surface | ferricstore_http invocation/HTTP-SDK surface: not served by the node |
| `ferricstore_server/test/ferricstore_server/acl/doctor_acl_test.exs` | 4 | surface | ACL/AUTH: the node has no authentication (ADR-011, auth_required false) |
| `ferricstore_server/test/ferricstore_server/acl_test.exs` | 112 | surface | ACL/AUTH: the node has no authentication (ADR-011, auth_required false) |
| `ferricstore_server/test/ferricstore_server/command_gateway_test.exs` | 46 | surface | stateless HTTP command gateway, health or dashboard HTTP endpoints: not served |
| `ferricstore_server/test/ferricstore_server/health/dashboard/access_recovery_seventh_review_test.exs` | 8 | surface | operator dashboard UI: the node has none |
| `ferricstore_server/test/ferricstore_server/health/dashboard/capped_counts_test.exs` | 3 | surface | operator dashboard UI: the node has none |
| `ferricstore_server/test/ferricstore_server/health/dashboard/circuit_review_test.exs` | 11 | surface | operator dashboard UI: the node has none |
| `ferricstore_server/test/ferricstore_server/health/dashboard/detail_action_forms_test.exs` | 10 | surface | operator dashboard UI: the node has none |
| `ferricstore_server/test/ferricstore_server/health/dashboard/detail_evidence_review_test.exs` | 7 | surface | operator dashboard UI: the node has none |
| `ferricstore_server/test/ferricstore_server/health/dashboard/detail_security_regressions_test.exs` | 5 | surface | operator dashboard UI: the node has none |
| `ferricstore_server/test/ferricstore_server/health/dashboard/due_redesign_test.exs` | 6 | surface | operator dashboard UI: the node has none |
| `ferricstore_server/test/ferricstore_server/health/dashboard/extreme_timestamps_test.exs` | 3 | surface | operator dashboard UI: the node has none |
| `ferricstore_server/test/ferricstore_server/health/dashboard/failures_redesign_test.exs` | 6 | surface | operator dashboard UI: the node has none |
| `ferricstore_server/test/ferricstore_server/health/dashboard/fifo_widgets_test.exs` | 14 | surface | operator dashboard UI: the node has none |
| `ferricstore_server/test/ferricstore_server/health/dashboard/flow_acl_sampling_test.exs` | 3 | surface | operator dashboard UI: the node has none |
| `ferricstore_server/test/ferricstore_server/health/dashboard/flow_detail_sampling_test.exs` | 2 | surface | operator dashboard UI: the node has none |
| `ferricstore_server/test/ferricstore_server/health/dashboard/flow_filter_semantics_test.exs` | 45 | surface | operator dashboard UI: the node has none |
| `ferricstore_server/test/ferricstore_server/health/dashboard/flow_governance_actions_test.exs` | 4 | surface | operator dashboard UI: the node has none |
| `ferricstore_server/test/ferricstore_server/health/dashboard/flow_lifecycle_test.exs` | 8 | surface | operator dashboard UI: the node has none |
| `ferricstore_server/test/ferricstore_server/health/dashboard/flow_projection_test.exs` | 1 | surface | operator dashboard UI: the node has none |
| `ferricstore_server/test/ferricstore_server/health/dashboard/flow_query_routing_test.exs` | 3 | surface | operator dashboard UI: the node has none |
| `ferricstore_server/test/ferricstore_server/health/dashboard/flow_query_workbench_test.exs` | 28 | surface | operator dashboard UI: the node has none |
| `ferricstore_server/test/ferricstore_server/health/dashboard/flow_schedule_safety_test.exs` | 4 | surface | operator dashboard UI: the node has none |
| `ferricstore_server/test/ferricstore_server/health/dashboard/flow_signal_scan_test.exs` | 1 | surface | operator dashboard UI: the node has none |
| `ferricstore_server/test/ferricstore_server/health/dashboard/flow_summary_scope_test.exs` | 3 | surface | operator dashboard UI: the node has none |
| `ferricstore_server/test/ferricstore_server/health/dashboard/flow_ux_redesign_test.exs` | 44 | surface | operator dashboard UI: the node has none |
| `ferricstore_server/test/ferricstore_server/health/dashboard/flow_value_inspector_test.exs` | 8 | surface | operator dashboard UI: the node has none |
| `ferricstore_server/test/ferricstore_server/health/dashboard/governance_metadata_review_test.exs` | 11 | surface | operator dashboard UI: the node has none |
| `ferricstore_server/test/ferricstore_server/health/dashboard/governance_redesign_test.exs` | 4 | surface | operator dashboard UI: the node has none |
| `ferricstore_server/test/ferricstore_server/health/dashboard/inspection_fourth_review_test.exs` | 11 | surface | operator dashboard UI: the node has none |
| `ferricstore_server/test/ferricstore_server/health/dashboard/internal_key_visibility_test.exs` | 6 | surface | operator dashboard UI: the node has none |
| `ferricstore_server/test/ferricstore_server/health/dashboard/lineage_review_test.exs` | 18 | surface | operator dashboard UI: the node has none |
| `ferricstore_server/test/ferricstore_server/health/dashboard/live_parity_all_pages_test.exs` | 1 | surface | operator dashboard UI: the node has none |
| `ferricstore_server/test/ferricstore_server/health/dashboard/live_status_payload_test.exs` | 1 | surface | operator dashboard UI: the node has none |
| `ferricstore_server/test/ferricstore_server/health/dashboard/management_forms_test.exs` | 7 | surface | operator dashboard UI: the node has none |
| `ferricstore_server/test/ferricstore_server/health/dashboard/management_fourth_review_test.exs` | 9 | surface | operator dashboard UI: the node has none |
| `ferricstore_server/test/ferricstore_server/health/dashboard/management_seventh_review_test.exs` | 16 | surface | operator dashboard UI: the node has none |
| `ferricstore_server/test/ferricstore_server/health/dashboard/management_third_review_test.exs` | 9 | surface | operator dashboard UI: the node has none |
| `ferricstore_server/test/ferricstore_server/health/dashboard/operational_review_test.exs` | 10 | surface | operator dashboard UI: the node has none |
| `ferricstore_server/test/ferricstore_server/health/dashboard/operations_second_review_test.exs` | 24 | surface | operator dashboard UI: the node has none |
| `ferricstore_server/test/ferricstore_server/health/dashboard/overview_redesign_test.exs` | 12 | surface | operator dashboard UI: the node has none |
| `ferricstore_server/test/ferricstore_server/health/dashboard/policies_redesign_test.exs` | 7 | surface | operator dashboard UI: the node has none |
| `ferricstore_server/test/ferricstore_server/health/dashboard/policy_editor_review_test.exs` | 8 | surface | operator dashboard UI: the node has none |
| `ferricstore_server/test/ferricstore_server/health/dashboard/presentation_seventh_review_test.exs` | 7 | surface | operator dashboard UI: the node has none |
| `ferricstore_server/test/ferricstore_server/health/dashboard/query_fifth_review_test.exs` | 11 | surface | operator dashboard UI: the node has none |
| `ferricstore_server/test/ferricstore_server/health/dashboard/query_fourth_review_test.exs` | 8 | surface | operator dashboard UI: the node has none |
| `ferricstore_server/test/ferricstore_server/health/dashboard/query_redesign_test.exs` | 7 | surface | operator dashboard UI: the node has none |
| `ferricstore_server/test/ferricstore_server/health/dashboard/query_scope_test.exs` | 12 | surface | operator dashboard UI: the node has none |
| `ferricstore_server/test/ferricstore_server/health/dashboard/query_second_review_test.exs` | 19 | surface | operator dashboard UI: the node has none |
| `ferricstore_server/test/ferricstore_server/health/dashboard/query_sixth_review_test.exs` | 15 | surface | operator dashboard UI: the node has none |
| `ferricstore_server/test/ferricstore_server/health/dashboard/query_time_validation_test.exs` | 4 | surface | operator dashboard UI: the node has none |
| `ferricstore_server/test/ferricstore_server/health/dashboard/query_triage_third_review_test.exs` | 9 | surface | operator dashboard UI: the node has none |
| `ferricstore_server/test/ferricstore_server/health/dashboard/retention_redesign_test.exs` | 3 | surface | operator dashboard UI: the node has none |
| `ferricstore_server/test/ferricstore_server/health/dashboard/runs_redesign_test.exs` | 6 | surface | operator dashboard UI: the node has none |
| `ferricstore_server/test/ferricstore_server/health/dashboard/schedule_third_review_test.exs` | 8 | surface | operator dashboard UI: the node has none |
| `ferricstore_server/test/ferricstore_server/health/dashboard/schedules_redesign_test.exs` | 4 | surface | operator dashboard UI: the node has none |
| `ferricstore_server/test/ferricstore_server/health/dashboard/shell_navigation_test.exs` | 7 | surface | operator dashboard UI: the node has none |
| `ferricstore_server/test/ferricstore_server/health/dashboard/shell_review_test.exs` | 10 | surface | operator dashboard UI: the node has none |
| `ferricstore_server/test/ferricstore_server/health/dashboard/state_filter_validation_test.exs` | 5 | surface | operator dashboard UI: the node has none |
| `ferricstore_server/test/ferricstore_server/health/dashboard/status_honesty_test.exs` | 7 | surface | operator dashboard UI: the node has none |
| `ferricstore_server/test/ferricstore_server/health/dashboard/system_fifth_review_test.exs` | 15 | surface | operator dashboard UI: the node has none |
| `ferricstore_server/test/ferricstore_server/health/dashboard/system_sixth_review_test.exs` | 7 | surface | operator dashboard UI: the node has none |
| `ferricstore_server/test/ferricstore_server/health/dashboard/workers_redesign_test.exs` | 7 | surface | operator dashboard UI: the node has none |
| `ferricstore_server/test/ferricstore_server/health/dashboard/workflow_compact_rows_test.exs` | 4 | surface | operator dashboard UI: the node has none |
| `ferricstore_server/test/ferricstore_server/health/dashboard/workflow_fifth_review_test.exs` | 7 | surface | operator dashboard UI: the node has none |
| `ferricstore_server/test/ferricstore_server/health/dashboard/workflow_full_audit_test.exs` | 23 | surface | operator dashboard UI: the node has none |
| `ferricstore_server/test/ferricstore_server/health/dashboard/workflow_review_regressions_test.exs` | 18 | surface | operator dashboard UI: the node has none |
| `ferricstore_server/test/ferricstore_server/health/dashboard/workflow_scope_bar_test.exs` | 4 | surface | operator dashboard UI: the node has none |
| `ferricstore_server/test/ferricstore_server/health/dashboard/workflow_scope_disclosure_test.exs` | 3 | surface | operator dashboard UI: the node has none |
| `ferricstore_server/test/ferricstore_server/health/dashboard/workflow_second_review_test.exs` | 17 | surface | operator dashboard UI: the node has none |
| `ferricstore_server/test/ferricstore_server/health/dashboard/workflow_sixth_review_test.exs` | 11 | surface | operator dashboard UI: the node has none |
| `ferricstore_server/test/ferricstore_server/health/dashboard/workflow_widgets_test.exs` | 22 | surface | operator dashboard UI: the node has none |
| `ferricstore_server/test/ferricstore_server/health/dashboard_test.exs` | 9 | surface | operator dashboard UI: the node has none |
| `ferricstore_server/test/ferricstore_server/health/dashboard_test/sections/acl_filtering_and_config.exs` | 41 | surface | operator dashboard UI: the node has none |
| `ferricstore_server/test/ferricstore_server/health/dashboard_test/sections/acl_flow_actions.exs` | 25 | surface | operator dashboard UI: the node has none |
| `ferricstore_server/test/ferricstore_server/health/dashboard_test/sections/collection_and_overview.exs` | 30 | surface | operator dashboard UI: the node has none |
| `ferricstore_server/test/ferricstore_server/health/dashboard_test/sections/flow_browse_and_queries.exs` | 24 | surface | operator dashboard UI: the node has none |
| `ferricstore_server/test/ferricstore_server/health/dashboard_test/sections/flow_detail_actions.exs` | 16 | surface | operator dashboard UI: the node has none |
| `ferricstore_server/test/ferricstore_server/health/dashboard_test/sections/flow_detail_policies_retention.exs` | 19 | surface | operator dashboard UI: the node has none |
| `ferricstore_server/test/ferricstore_server/health/dashboard_test/sections/flow_schedules.exs` | 3 | surface | operator dashboard UI: the node has none |
| `ferricstore_server/test/ferricstore_server/health/dashboard_test/sections/http_flow_routes.exs` | 20 | surface | operator dashboard UI: the node has none |
| `ferricstore_server/test/ferricstore_server/health/endpoint/auth_test.exs` | 16 | surface | stateless HTTP command gateway, health or dashboard HTTP endpoints: not served |
| `ferricstore_server/test/ferricstore_server/health/endpoint/dashboard_handlers_test.exs` | 4 | surface | operator dashboard UI: the node has none |
| `ferricstore_server/test/ferricstore_server/health/endpoint/flow_paths_test.exs` | 8 | surface | stateless HTTP command gateway, health or dashboard HTTP endpoints: not served |
| `ferricstore_server/test/ferricstore_server/health/endpoint/forbidden_test.exs` | 3 | surface | stateless HTTP command gateway, health or dashboard HTTP endpoints: not served |
| `ferricstore_server/test/ferricstore_server/health/endpoint/login_test.exs` | 6 | surface | stateless HTTP command gateway, health or dashboard HTTP endpoints: not served |
| `ferricstore_server/test/ferricstore_server/health/endpoint/route_requirements_test.exs` | 7 | surface | stateless HTTP command gateway, health or dashboard HTTP endpoints: not served |
| `ferricstore_server/test/ferricstore_server/health/endpoint/security_test.exs` | 11 | surface | stateless HTTP command gateway, health or dashboard HTTP endpoints: not served |
| `ferricstore_server/test/ferricstore_server/native/chunks_test.exs` | 12 | surface | chunked frames, MULTI, PubSub or KV commands: not served (ADR-011) |
| `ferricstore_server/test/ferricstore_server/native/feature_level_5_multi_test.exs` | 16 | surface | chunked frames, MULTI, PubSub or KV commands: not served (ADR-011) |
| `ferricstore_server/test/ferricstore_server/native/fql_benchmark_preflight_test.exs` | 1 | internal | repository guard, benchmark or release check of the Elixir tree |
| `ferricstore_server/test/ferricstore_server/native/internal_key_access_test.exs` | 7 | surface | chunked frames, MULTI, PubSub or KV commands: not served (ADR-011) |
| `ferricstore_server/test/ferricstore_server/native/pubsub_benchmark_client_test.exs` | 19 | internal | repository guard, benchmark or release check of the Elixir tree |
| `ferricstore_server/test/ferricstore_server/native/pubsub_coalescer_test.exs` | 5 | surface | chunked frames, MULTI, PubSub or KV commands: not served (ADR-011) |
| `ferricstore_server/test/ferricstore_server/native/session_subscription_budget_test.exs` | 3 | surface | chunked frames, MULTI, PubSub or KV commands: not served (ADR-011) |
| `ferricstore_server/test/ferricstore_server/native/set_semantics_test.exs` | 3 | surface | chunked frames, MULTI, PubSub or KV commands: not served (ADR-011) |
| `ferricstore_server/test/ferricstore_server/native/sharded_pubsub_test.exs` | 5 | surface | chunked frames, MULTI, PubSub or KV commands: not served (ADR-011) |
| `ferricstore_server/test/ferricstore_server/native/tls_listener_test.exs` | 5 | surface | TLS listener: the node serves plain TCP (ADR-011) |
| `ferricstore_server/test/ferricstore_server/native/zset_blocking_test.exs` | 7 | surface | chunked frames, MULTI, PubSub or KV commands: not served (ADR-011) |
| `ferricstore_server/test/ferricstore_server/spec/http_endpoints_test.exs` | 31 | surface | stateless HTTP command gateway, health or dashboard HTTP endpoints: not served |

## Gaps

- G63 (closed): HELLO negotiated nothing and STARTUP took any codec
  list, compression and client name; both now negotiate alike and refuse
  what Elixir refuses, as bad requests that keep the earlier choice.
- G64 (closed): a typed `PIPELINE` ignored `atomicity`; it now takes
  only Elixir's modes and refuses `same_shard` around a Flow command.
- G65 (closed): `COMMAND_EXEC` and pipeline items lost a refusal's
  status and structure (a query failure's diagnostic among them).

## `ferricstore/test/ferricstore/cluster/cross_node_decide_write_test.exs`

| Line | Test | Status |
|---:|---|---|
| 37 | EXPIRE NX from every node has one winner | open |
| 57 | DEL and UNLINK from every node count a key once | open |
| 77 | TDIGEST.ADD from every node keeps every value | open |
| 101 | TDIGEST.CREATE from every node has one winner | open |
| 112 | a list push wakes list waiters on every node | open |
| 140 | a sorted-set write wakes BZPOPMIN waiters on every node | open |
| 162 | a blocked claim takes a flow created through another node | open |

## `ferricstore/test/ferricstore/cluster/flow_hibernation_claim_cluster_test.exs`

| Line | Test | Status |
|---:|---|---|
| 37 | every node claims the same Flow while one node's writer is behind | open |
| 109 | a node that crashes after level 8 is raised keeps the Flows it restored | open |

## `ferricstore/test/ferricstore/cluster/flow_replay_after_restart_cluster_test.exs`

| Line | Test | Status |
|---:|---|---|
| 27 | a restarted node replays Flow writes its LMDB mirror already holds | open |

## `ferricstore/test/ferricstore/cluster/flow_retention_mirror_lag_cluster_test.exs`

| Line | Test | Status |
|---:|---|---|
| 30 | a re-create after cleanup applies on every node, with one writer behind | open |

## `ferricstore/test/ferricstore/cluster/flow_schedule_cluster_test.exs`

| Line | Test | Status |
|---:|---|---|
| 20 | due schedule can fire once after Raft leader failover | open |
| 86 | overdue interval catch-up fires once after Raft leader failover | open |

## `ferricstore/test/ferricstore/cluster/flow_state_meta_retention_cluster_test.exs`

| Line | Test | Status |
|---:|---|---|
| 20 | retention cleanup removes indexed state_meta projections after leader failover | open |

## `ferricstore/test/ferricstore/commands/flow_command_boundary_test.exs`

| Line | Test | Status |
|---:|---|---|
| 6 | Flow dispatch preserves the supplied instance context | open |
| 36 | structured Flow opcodes are rejected explicitly by scalar COMMAND_EXEC preparation | open |
| 59 | Flow response normalization converts keys recursively inside lists | open |

## `ferricstore/test/ferricstore/commands/flow_signal_command_test.exs`

| Line | Test | Status |
|---:|---|---|
| 21 | dispatches Flow signal through Rust AST | open |
| 79 | FLOW.GET returns every value its VALUE options name | open |

## `ferricstore/test/ferricstore/commands/flow_test/sections/dispatches_flow_create_many_through_rust_ast.exs`

| Line | Test | Status |
|---:|---|---|
| 9 | dispatches Flow create_many through Rust AST | open |
| 56 | dispatches cold auto-bucket create_many claim_due through Rust AST | open |
| 118 | dispatches idempotent Flow create_many retry through Rust AST | open |
| 146 | dispatches mixed-partition Flow create_many through Rust AST | open |
| 176 | dispatches Flow claim and complete through Rust AST | open |
| 231 | dispatches Flow complete_many through Rust AST | open |
| 309 | rejects terminal Flow TTL zero through Rust AST | open |
| 360 | dispatches Flow cancel inline reason through Rust AST | open |
| 400 | dispatches Flow fail_many through Rust AST | open |
| 478 | dispatches Flow retry_many through Rust AST | open |
| 562 | dispatches Flow retry policy override through Rust AST | open |
| 612 | dispatches Flow claim_due with job-only return through Rust AST | open |
| 656 | dispatches Flow claim_due with compact job-only return through Rust AST | open |
| 705 | dispatches Flow claim_due with compact job-state return through Rust AST | open |
| 752 | dispatches Flow claim_due with any partition and repeated states through Rust AST | open |
| 828 | dispatches Flow cancel_many through Rust AST | open |

## `ferricstore/test/ferricstore/commands/flow_test/sections/dispatches_flow_rewind_through_rust_ast.exs`

| Line | Test | Status |
|---:|---|---|
| 9 | dispatches Flow rewind through Rust AST | open |
| 67 | dispatches Flow AST parse errors without calling embedded API | open |
| 89 | dispatches Flow policy AST through embedded API | open |
| 147 | dispatches Flow policy commands through Rust AST | open |
| 250 | dispatches FIFO policy and claims through Rust AST command path | open |
| 326 | Flow commands are visible in COMMAND catalog | open |

## `ferricstore/test/ferricstore/commands/flow_test/sections/dispatches_flow_state_meta_through_rust_ast.exs`

| Line | Test | Status |
|---:|---|---|
| 9 | dispatches Flow state_meta and indexed_state_meta through Rust AST | open |

## `ferricstore/test/ferricstore/commands/flow_test/sections/dispatches_flow_value_put_payload_refs_through_rust_ast.exs`

| Line | Test | Status |
|---:|---|---|
| 67 | dispatches Flow value put and payload refs through Rust AST | open |
| 143 | rejects unsupported Flow value ref inputs through Rust AST | open |
| 166 | dispatches Flow create/get/history through Rust AST | open |
| 244 | dispatches Flow failures through Rust AST | open |
| 339 | dispatches Flow retention cleanup through Rust AST | open |
| 355 | dispatches Flow full values through Rust AST | open |
| 435 | dispatches Flow spawn children through Rust AST | open |
| 545 | dispatches Flow mutation values through Rust AST | open |
| 712 | dispatches Flow lineage queries through Rust AST | open |
| 820 | dispatches Flow reclaim through Rust AST | open |
| 852 | dispatches Flow claim_due reclaim controls through Rust AST | open |
| 898 | dispatches Flow extend_lease through Rust AST | open |
| 933 | dispatches Flow extend_lease with an OK-only response | open |

## `ferricstore/test/ferricstore/commands/native_ast_parser_contract_test.exs`

| Line | Test | Status |
|---:|---|---|
| 6 | prepared SCAN families produce typed, bounded options | open |
| 27 | prepared COPY rejects every unsupported or duplicate option | open |
| 40 | prepared command keywords are case-insensitive | open |
| 67 | lowercase XREADGROUP grammar still exposes every stream ACL key | open |
| 81 | prepared integer and blocking-timeout values are bounded | open |
| 104 | Flow special grammar is case-insensitive and counted operands are exact | open |
| 161 | FLOW.REWIND retains binary reasons and rejects unsupported reason forms | open |
| 184 | repeated Flow list options preserve order without append-based quadratic accumulation | open |

## `ferricstore/test/ferricstore/commands/native_ast_parser_test.exs`

| Line | Test | Status |
|---:|---|---|
| 19 | rejects nonconvertible command arguments without raising | open |
| 25 | rejects float arguments outside the VM float range without raising | open |
| 42 | generic or structured native parsing covers every ACL-visible command family | open |
| 57 | parses every built-in ACL-visible command family without unknown AST fallback | open |
| 78 | collection scan cursors decode to seek boundaries | open |
| 92 | every built-in Flow data command has a non-empty ACL scope | open |
| 120 | Flow ACL extraction uses parsed options rather than matching option values | open |
| 133 | XREADGROUP ACL extraction ignores STREAMS in group and consumer operands | open |
| 147 | Flow batch ACL extraction preserves mixed item partitions | open |
| 161 | Flow item commands read an option value spelled ITEMS as a value | open |
| 198 | malformed Flow item commands return the parser's error | open |
| 220 | generic Flow command families expose their effective ACL scopes | open |
| 246 | FLOW.QUERY keeps FQL version, query text, and named parameters distinct | open |
| 275 | removed Flow collection commands have no parser route | open |
| 282 | parses hot string command ASTs directly | open |
| 292 | parses collection and probabilistic command families | open |
| 339 | TDigest quantile domains are validated while preparing commands | open |
| 354 | merge commands expose destination and every source key | open |
| 380 | counted and variadic commands expose every data key | open |
| 391 | SINTERCARD validates its counted key list and LIMIT in the prepared parser | open |
| 407 | subcommand operations expose the data key instead of the subcommand | open |
| 419 | extra native operations expose their first key | open |
| 437 | parses Flow create claim terminal and query ASTs | open |
| 523 | Flow reads keep every VALUE option | open |
| 552 | extracts namespace ACL keys from direct Flow metadata commands | open |
| 589 | TOPK.RESERVE omits the unsupported decay field | open |
| 599 | parses Flow policy indexed attributes through native AST | open |
| 630 | parses Flow policy max active timeout through native AST | open |
| 650 | parses Flow policy compare-and-swap and replacement controls through native AST | open |
| 664 | rejects state-level Flow policy indexed attributes through native AST | open |
| 688 | parses Flow many and newer workflow/governance command names | open |

## `ferricstore/test/ferricstore/commands/prepared_command_test.exs`

| Line | Test | Status |
|---:|---|---|
| 10 | unknown commands are rejected before ACL and routing metadata are constructed | open |
| 19 | KeyDiscovery prepares parsing, ACL, routing, and mutation metadata together | open |
| 34 | Flow preparation separates COMMAND GETKEYS data keys from conservative ACL scope | open |
| 46 | structured schedule creation shares ACL, routing, and mutation key discovery | open |
| 68 | structured schedule creation fails closed for an unbounded generated target | open |
| 82 | structured schedule discovery does not let nested options shadow native fields | open |
| 100 | FLOW.QUERY preparation binds once and shares tenant ACL, routing, and read scope | open |
| 133 | FLOW.QUERY text preparation decodes declared integer parameters and rejects bad input | open |
| 180 | FLOW.QUERY raw preparation formats positioned parser guidance | open |
| 189 | KeyDiscovery describe keeps the command key contract | open |
| 197 | PreparedCommand has one canonical preparation boundary | open |
| 209 | dynamic merge keys share one ACL, routing, and mutation footprint | open |
| 235 | counted and variadic dynamic commands keep read-only footprints | open |
| 256 | MEMORY USAGE keeps its dynamically discovered key read-only | open |
| 265 | source and destination commands describe asymmetric access | open |
| 293 | conditional writes retain the keys whose prior state controls the result | open |
| 307 | read-modify-write commands and SET GET retain read access | open |
| 329 | key access normalization accepts lowercase names and classifies pipelines | open |
| 336 | routing derives cross-shard status from prepared routing keys | open |
| 348 | no-key commands do not invent ACL, routing, or mutation keys | open |
| 362 | Pub/Sub preparation separates channel ACLs from data-key ACLs | open |
| 374 | prepared commands identify deterministic transaction-local execution | open |
| 388 | transaction-local multi-key ASTs namespace every prepared routing key | open |
| 448 | prepared commands fail closed when replicated apply would escape the local store | open |
| 468 | native mutations remain request-scoped until their apply contract is admitted | open |
| 482 | transaction execution entries retain only the compact validated apply plan | open |
| 527 | compound work preflight stops at the replicated boundary | open |
| 541 | a literal wildcard data key remains routable | open |
| 548 | Flow commands expose coordinated routing instead of logical ACL keys | open |
| 576 | global data mutations expose coordinated routing | open |
| 585 | global keyspace reads expose coordinated routing | open |
| 600 | tuple projection APIs are not part of the prepared-command contract | open |
| 605 | raw transaction queue tuples are not part of the execution contract | open |

## `ferricstore/test/ferricstore/flow/claim_due_empty_precheck_test.exs`

| Line | Test | Status |
|---:|---|---|
| 10 | an automatic-partition claim on an empty queue checks each shard's index once | open |
| 32 | an automatic-partition claim still finds work at any priority | open |

## `ferricstore/test/ferricstore/flow/claim_waiter_scheduler_test.exs`

| Line | Test | Status |
|---:|---|---|
| 16 | schedule_next_due fails closed for unsupported inputs | open |
| 20 | schedule_next_due tolerates missing router/index context | open |
| 24 | broad hot scheduling aggregates the earliest score without copying all due keys | open |
| 32 | scoped auto scheduling uses one aggregate probe instead of per-bucket fanout | open |
| 59 | unbounded due-count-key router and shard endpoints are not compiled | open |
| 71 | broad cold scheduling stays correct when a bucket exceeds the scan limit | open |
| 189 | cold scheduling stops probing after the earliest matching bucket | open |
| 276 | exact cold scheduling stays conservative when stale rows fill the scan window | open |
| 397 | cold auto-partition matching accepts only canonical generated buckets | open |

## `ferricstore/test/ferricstore/flow/claim_waiters_test.exs`

| Line | Test | Status |
|---:|---|---|
| 36 | register capacity check does not scan all waiters on the hot path | open |
| 52 | capacity-pressure pruning is bounded and does not scan every waiter row | open |
| 68 | scheduled ready cleanup uses a bounded timer-table page | open |
| 84 | stale waiter pruning does not copy the whole waiter table | open |
| 97 | flow retry wake gate does not scan all blocked waiters | open |
| 106 | flow delayed wake gate checks only matching waiter keys | open |
| 115 | register fails closed when blocked claim_due waiter row cap is reached | open |
| 129 | register rejects broad waiter atomically when row cap cannot fit all keys | open |
| 140 | concurrent registrations cannot oversubscribe the waiter row cap | open |
| 185 | register rejects a direct key set larger than the waiter footprint limit | open |
| 194 | multi-state multi-partition wait keys stay compact | open |
| 206 | registering a broad claim_due waiter uses bounded ETS rows | open |
| 216 | compact partition waiter still wakes for matching ready work | open |
| 226 | small partition-list waiters do not wake for unrelated partitions | open |
| 240 | moderate partition-list waiters stay bounded without waking for unrelated partitions | open |
| 256 | broad shared-scope waiters stay compact without cross-scope wakeups | open |
| 273 | notify skips expired waiters | open |
| 283 | total_count prunes waiters whose process died before unregistering | open |
| 309 | ready credits coalesce repeated hints and wake available workers for ready work | open |
| 325 | ready credits keep distinct ready buckets independent | open |
| 342 | ready credits use waiter claim limits to avoid herd wakeups | open |
| 358 | large ready batches are not stranded behind the legacy fixed wake cap | open |
| 368 | future ready timers coalesce by ready bucket | open |
| 391 | future ready timers use flow wall-clock due times | open |
| 401 | future ready timers are pruned when the matching waiter unregisters | open |
| 413 | unregister cancels the underlying delayed wake timer | open |

## `ferricstore/test/ferricstore/flow/history_entry_test.exs`

| Line | Test | Status |
|---:|---|---|
| 7 | converts tuple history entries to event id and fields map | open |
| 12 | converts list history entries to event id and fields map | open |
| 17 | last duplicate field wins | open |
| 23 | malformed stored fields do not crash history reads | open |

## `ferricstore/test/ferricstore/flow/history_event_test.exs`

| Line | Test | Status |
|---:|---|---|
| 7 | parses millisecond prefix from generated event ids | open |
| 11 | parses bare millisecond event ids | open |
| 15 | returns zero for malformed and non-binary event ids | open |
| 21 | orders event ids by millisecond, then version | open |
| 28 | reads the version of generated event ids only | open |

## `ferricstore/test/ferricstore/flow/history_order_test.exs`

| Line | Test | Status |
|---:|---|---|
| 27 | lists and filters events in version order | open |
| 39 | pages through events in version order | open |
| 46 | trimming keeps the newest events | open |

## `ferricstore/test/ferricstore/flow/history_retention_restart_test.exs`

| Line | Test | Status |
|---:|---|---|
| 24 | a Flow ID reused after retention and a restart starts with an empty history | open |
| 72 | history retention removed stays removed when the node dies before its tombstones | open |
| 134 | retention waits for a Flow's history that waits in the overflow queue | open |

## `ferricstore/test/ferricstore/flow/history_value_release_test.exs`

| Line | Test | Status |
|---:|---|---|
| 84 | a release deletes the values only trimmed history used, with their cleanup members | open |
| 108 | a release pages through the cleanup members | open |
| 117 | trimming alone keeps a running Flow's cleanup members bounded | open |
| 133 | pruning a finished Flow deletes its release floor | open |
| 171 | nothing is released until feature level 2 is active | open |
| 179 | a rewound Flow keeps the old value it points at again | open |
| 204 | a rewind cannot restore a value a release deleted | open |

## `ferricstore/test/ferricstore/flow/info_api_test.exs`

| Line | Test | Status |
|---:|---|---|
| 7 | info rejects non-keyword opts | open |
| 11 | info rejects invalid type | open |

## `ferricstore/test/ferricstore/flow/info_count_read_test.exs`

| Line | Test | Status |
|---:|---|---|
| 7 | zset_count_many returns empty counts without router access | open |
| 11 | terminal_lmdb_counts skips cold counts when disabled | open |
| 22 | terminal_lmdb_counts skips LMDB when no terminal states match | open |
| 35 | terminal sweep limit rejects invalid configuration instead of disabling cleanup | open |

## `ferricstore/test/ferricstore/flow/info_counts_test.exs`

| Line | Test | Status |
|---:|---|---|
| 7 | zero_counts creates atom keys for active and terminal states | open |
| 16 | state_keys and inflight_key use Flow key layout | open |
| 29 | merge_auto combines counts and inflight totals | open |
| 34 | terminal_keys returns only terminal state index keys | open |
| 45 | merge_terminal_counts adds counts into existing accumulator | open |
| 50 | merge_terminal_counts rejects a partial count vector | open |
| 55 | merge_terminal_counts rejects malformed counts without raising | open |
| 67 | validate_counts rejects malformed vectors before callers inspect them | open |

## `ferricstore/test/ferricstore/flow/invariant_test.exs`

| Line | Test | Status |
|---:|---|---|
| 5 | states are parallel by default until policy opts a state into fifo | open |
| 38 | fifo state claims one flow per partition lane in state-entry order | open |
| 108 | fifo retry keeps the same partition lane blocked until the retried flow leaves | open |
| 196 | fifo lanes are scoped to type state and partition | open |
| 246 | fifo any-partition claims at most one flow per partition lane | open |
| 312 | fifo partition list claims at most one flow per listed partition lane | open |
| 368 | fifo pipeline claim batch does not overclaim a single partition lane | open |
| 413 | fifo reclaim picks the expired lane head instead of the queued follower | open |
| 466 | fifo limit one reclaims an expired lane head before its queued follower | open |
| 510 | fifo keeps a cold lane head ahead of a hot due follower | open |
| 546 | fifo claims a due lane head beyond the bounded follower scan window | open |
| 584 | fifo preserves global due order when a deep lane head is injected | open |
| 626 | fifo times out an expired lane head injected beyond the bounded scan window | open |
| 683 | fifo times out an expired follower while a live lane head blocks claims | open |
| 723 | fifo unlinks a cancelled middle entry without disturbing lane order | open |
| 772 | enabling fifo on a populated state uses its existing state-entry order | open |
| 802 | enabling fifo waits for already leased lane members to drain | open |
| 850 | step_continue records a fresh logical state entry order before fifo activation | open |
| 917 | fifo lane unblocks when the running flow transitions out of the fifo state | open |
| 958 | fifo state-entry order survives restart when ids and timestamps would sort differently | open |
| 989 | fifo state rejects priority and implicit partition entry | open |
| 1053 | fifo state rejects direct running entry | open |
| 1102 | fifo planning preserves due order for parallel states in mixed claims | open |
| 1149 | public record returns hide fifo state entry sequence | open |
| 1205 | pipeline get hides fifo state entry sequence | open |
| 1231 | spawn_children rejects reserved child group ids | open |
| 1246 | leased and terminal flows are not returned by later claim_due calls | open |
| 1291 | future due work is invisible until its due timestamp | open |
| 1325 | complete writes terminal state and keeps history queryable | open |
| 1370 | stored value refs are readable through mget without duplicating command flow state | open |

## `ferricstore/test/ferricstore/flow/pipeline_claim_due_command_test.exs`

| Line | Test | Status |
|---:|---|---|
| 17 | normalizes grouped claim_due command options | open |
| 48 | rejects mixed ANY and explicit states | open |
| 56 | rejects claim filters whose state and partition product exceeds the bound | open |
| 68 | rejects internal keys that exceed the limit only after component encoding | open |
| 79 | rejects lease durations that cannot produce exact Flow index deadlines | open |
| 87 | rejects claim deadlines that leave the exact Flow timestamp range | open |
| 96 | rejects blocking claims because pipelines cannot wait | open |

## `ferricstore/test/ferricstore/flow/pipeline_claim_due_test.exs`

| Line | Test | Status |
|---:|---|---|
| 28 | results preserves accumulated results for an empty command batch | open |
| 35 | results preserves errors and coalesces globally compatible claims | open |
| 66 | results falls back to adjacent grouping when queue keys conflict | open |
| 97 | routed singleton batches fail every claim on a partial router result | open |

## `ferricstore/test/ferricstore/flow/pipeline_read_command_test.exs`

| Line | Test | Status |
|---:|---|---|
| 15 | command parses get with default payload return and sealed scope | open |
| 22 | command preserves metadata-only return shape | open |
| 28 | scope-sensitive commands fail closed for a malformed context | open |
| 33 | command rejects unsupported operation | open |
| 38 | command rejects payload hydration above the configured ceiling | open |
| 46 | decode_get fails closed for corrupt stored records | open |
| 57 | history command uses the direct history preparation contract | open |
| 66 | history decode contexts distinguish missing state from corrupt state | open |

## `ferricstore/test/ferricstore/flow/pipeline_read_test.exs`

| Line | Test | Status |
|---:|---|---|
| 9 | batch fast path preserves same-partition get order directly | open |
| 35 | batch fast path projects internal record fields before returning | open |
| 60 | batch metadata return uses the lightweight decoder and public metadata projection | open |
| 88 | batch fast path fails every read closed when a batch reply is short | open |
| 108 | batch fast path preserves get order without payload hydration | open |
| 142 | batch fast path isolates a short reply to its partition group | open |
| 172 | batch generic path isolates a short get reply to its partition group | open |
| 207 | batch preserves order across history, other, and errors | open |
| 239 | batch prepares each command once when the get fast path falls back | open |
| 259 | batch coalesces identical keyed reads while preserving output order | open |
| 288 | batch keeps unkeyed other reads independent | open |
| 311 | hydrate_get_results passes through non-record results and records without payload | open |
| 325 | hydrate_get_results projects internal record fields | open |

## `ferricstore/test/ferricstore/flow/pipeline_write_command_test.exs`

| Line | Test | Status |
|---:|---|---|
| 7 | command parses create through callbacks | open |
| 14 | command rejects unsupported op | open |

## `ferricstore/test/ferricstore/flow/pipeline_write_test.exs`

| Line | Test | Status |
|---:|---|---|
| 7 | create_attrs_from_commands accepts unique flow_create commands | open |
| 17 | create_attrs_from_commands rejects duplicate keys and mixed commands | open |
| 30 | start_and_claim_attrs_from_commands accepts unique flow_start_and_claim commands | open |
| 40 | start_and_claim_attrs_from_commands rejects duplicate keys and mixed commands | open |
| 53 | named_value_put_attrs_from_commands accepts unique owned value put commands | open |
| 63 | named_value_put_attrs_from_commands preserves success-only return mode | open |
| 71 | named_value_put_attrs_from_commands rejects duplicate keys and mixed commands | open |
| 84 | transition_attrs_from_commands accepts only transition commands | open |

## `ferricstore/test/ferricstore/flow/record_query_test.exs`

| Line | Test | Status |
|---:|---|---|
| 18 | fetch_count keeps count when no time filter is present | open |
| 24 | fetch_count delegates when a time filter is present | open |
| 29 | filter_by_ms honors optional bounds | open |
| 47 | sort_by_update uses timestamp then id order | open |
| 57 | maybe_reverse and chunk helpers preserve query semantics | open |
| 68 | bounded ordered chunk merge preserves ascending rank and first-source duplicates | open |
| 91 | bounded ordered chunk merge preserves descending rank and first-source duplicates | open |
| 116 | auto-partition merge caps each fetch and fails instead of returning an inexact prefix | open |
| 140 | auto-partition merge keeps only the globally ranked result window | open |
| 158 | auto-partition merge applies the same global order in reverse | open |
| 175 | auto-partition merge rejects malformed fetch results without raising | open |
| 185 | filtered auto-partition merge enforces the global scanned candidate budget | open |
| 201 | filtered source merge uses the caller's global candidate budget | open |

## `ferricstore/test/ferricstore/flow/retention_sweeper_test.exs`

| Line | Test | Status |
|---:|---|---|
| 7 | test runtime keeps the application sweeper out of explicit cleanup tests | open |
| 15 | catch-up scheduling treats history-only cleanup at limit as backlog | open |
| 43 | catch-up scheduling continues after an underfilled cleanup pass does work | open |
| 74 | completed pass does not schedule a final empty catch-up | open |
| 103 | catch-up scheduling treats active timeouts and terminal cleanup as one budget | open |
| 131 | sustained backlog pauses between bounded catch-up bursts | open |
| 162 | pressure mode uses pressure limit and triggers compaction | open |
| 197 | pressure compaction is throttled | open |
| 225 | compaction task start failure does not stop retention cleanup | open |

## `ferricstore/test/ferricstore/flow/terminal_query_test.exs`

| Line | Test | Status |
|---:|---|---|
| 9 | state accepts any and known terminal states | open |
| 15 | state rejects non-terminal values | open |
| 20 | ids_from_chunks flattens deduplicates and caps by terminal state count | open |
| 29 | record filters keep only terminal or exact state records | open |

## `ferricstore/test/ferricstore/flow/value_store_test.exs`

| Line | Test | Status |
|---:|---|---|
| 9 | mget result decoding preserves order and enforces exact cardinality | open |
| 23 | mget result decoding sanitizes storage read failures | open |
| 30 | mget rejects corrupted typed values instead of returning encoded bytes | open |
| 37 | shared value batch mapping rejects extra backend results | open |
| 44 | named value puts reject independently expiring values | open |
| 53 | named value identity components must be non-empty | open |

## `ferricstore/test/ferricstore/flow_backpressure_test.exs`

| Line | Test | Status |
|---:|---|---|
| 49 | flow_create rejects new flows under memory overload | open |
| 65 | flow_create_pipeline_batch returns per-item overload errors | open |
| 89 | flow_create_pipeline_batch rejects malformed attrs without crashing shard | open |
| 108 | flow_create_many_independent returns top-level overload for SDK backpressure | open |
| 131 | flow admission gate rejects new creates before memory guard hard reject | open |
| 147 | flow_create telemetry separates attempts successes and rejections | open |
| 187 | flow_create_many rejected telemetry counts blocked records | open |
| 206 | concurrent flow creates under pressure return BUSY or succeed and recover cleanly | open |

## `ferricstore/test/ferricstore/flow_crash_matrix_test.exs`

| Line | Test | Status |
|---:|---|---|
| 35 | unknown outcome after WARaft commit recovers committed Flow state and hot due index | open |
| 69 | crash after WARaft apply projection write rebuilds visible Flow state | open |
| 102 | crash during blob side-channel write leaves command retryable and blobs collectible | open |
| 144 | LMDB writer crash during flush replays terminal projection from durable truth | open |
| 184 | history projector crash after fsync replays history and preserves values | open |
| 245 | blob GC crash after live-ref scan does not delete live Flow blobs | open |

## `ferricstore/test/ferricstore/flow_policy_generation_test.exs`

| Line | Test | Status |
|---:|---|---|
| 14 | policy generations are always allocated and advance monotonically | open |
| 28 | policy set rejects a stale expected generation without changing the policy | open |
| 50 | policy set uses the allocator shard generation when a routed replica is stale | open |
| 70 | policy reads use the allocator shard when a routed replica is stale | open |
| 86 | policy reads reject a value stored under a different type key | open |
| 108 | a corrupt policy row does not stop a restart and stays fail-closed for its type | open |
| 145 | missing existing-flow targets are stamped with an absence guard | open |
| 161 | existing-flow targets retain both their policy reference and incarnation guard | open |
| 199 | policy set merges independent type and state patches | open |
| 232 | string-key state patches override the equivalent normalized setting | open |
| 258 | native state-pair patches merge with existing states | open |
| 280 | policy set validates the optional response state | open |
| 287 | concurrent policy patches preserve both writers | open |
| 321 | a burst of concurrent policy patches preserves every writer | open |
| 361 | replace resets fields omitted from an explicit full policy snapshot | open |

## `ferricstore/test/ferricstore/flow_policy_migration_test.exs`

| Line | Test | Status |
|---:|---|---|
| 22 | catalog members store the exact type once per shard instead of once per flow | open |
| 37 | catalog projection keys sort by generation and retain exact catalog ownership | open |
| 53 | work cursors reject an after-key outside their staged run | open |
| 58 | batch state writes keep catalog generation current and lifetime aligned with state | open |
| 142 | terminal catalog survives retention TTL until explicit cleanup and later migration | open |
| 200 | policy changes are independent of unrelated shard keydir cardinality | open |
| 230 | bounded policy migration resumes from durable progress after index restart | open |
| 274 | exact-type catalog remains bounded above ten thousand members | open |
| 356 | flush clears catalog rows and pending migration jobs | open |
| 400 | worker uses configured migration and backfill limits independently | open |
| 430 | worker polls shards in bounded round-robin sweeps | open |
| 470 | explicit worker enabled option takes precedence over global configuration | open |
| 488 | worker reports a corrupt migration job projection instead of treating it as idle | open |
| 517 | job discovery fails closed when a committed job loses its mirror enqueue | open |
| 538 | worker bounds keydir snapshots and stops its owned snapshot task | open |
| 573 | worker ignores stale scheduled runs after timer replacement | open |
| 607 | worker rebuilds missing local staging before resuming a replicated work cursor | open |
| 667 | keydir snapshot pages persist their cursor and release the fixed table | open |
| 712 | a fresh keydir snapshot removes the staging of earlier runs | open |
| 746 | keydir snapshot rejects a non-canonical durable continuation | open |
| 781 | keydir snapshot restarts safely when its ETS table is rebuilt | open |
| 821 | keydir snapshot discards an invalidated continuation instead of retrying it forever | open |
| 860 | worker does not repeat snapshot cleanup after the manifest is removed | open |
| 904 | worker retains the manifest and retries a failed snapshot cleanup | open |
| 955 | catalog pagination enforces raw and hydrated byte bounds before final proof | open |
| 989 | catalog hydration binds shared records to their sealed physical scope | open |
| 1056 | backfill discovers a primary state that is absent from the lagged LMDB mirror | open |
| 1124 | backfill skips and counts a registered flow whose state record is gone | open |
| 1139 | backfill skips a registered flow whose LMDB row names a lost projection frame | open |
| 1152 | worker reports registered flows without a state record once a backfill finishes | open |
| 1193 | backfill keeps waiting for a registered flow while the LMDB mirror is degraded | open |
| 1209 | backfill derives a cold LMDB state candidate from its primary registry | open |
| 1295 | replicated cold backfill candidate does not read local LMDB during apply | open |
| 1392 | planned backfill record cannot recreate membership after retention cleanup | open |
| 1502 | replicated cold migration plan does not read local LMDB during apply | open |
| 1586 | backfill resolves a stale LMDB record from the current primary record | open |
| 1639 | duplicate old projection rows for one catalog member are repaired | open |
| 1676 | planner rejects a primary descriptor that is ahead of its durable mirror | open |
| 1738 | membership inserted after planning prevents the stale plan from finishing | open |
| 1769 | newer completed migration dominates stale policy entries applied out of order | open |
| 1818 | delayed older-policy mutation is rejected and a current mutation converges attributes | open |
| 1911 | malformed projection generation fails closed without crashing apply | open |
| 1958 | matching durable backfill completion survives worker process restart | open |
| 1988 | ordinary embedded restart preserves source token while destructive reconcile rotates it | open |

## `ferricstore/test/ferricstore/flow_policy_repair_test.exs`

| Line | Test | Status |
|---:|---|---|
| 26 | replace cannot repair a corrupt policy until feature level 1 is active | open |
| 36 | replace repairs a policy stored under another type above its generation | open |
| 57 | replace repairs an unreadable policy above every earlier generation | open |
| 71 | a corrupt policy still needs replace to be overwritten | open |
| 87 | repair also replaces corrupt copies installed on other shards | open |

## `ferricstore/test/ferricstore/flow_production_recovery_test.exs`

| Line | Test | Status |
|---:|---|---|
| 36 | Flow truth, hot indexes, blob values, history, and cold projection survive supervised shard crash | open |
| 173 | an FQL-visible Flow can expire before a restart without blocking recovery | open |
| 278 | retention deletes a cold expired QueryRow whose apply-projection source was compacted away before a process restart | open |
| 373 | a process restart keeps a completed shared-ref backfill instead of migrating again | open |

## `ferricstore/test/ferricstore/management_contract_test.exs`

| Line | Test | Status |
|---:|---|---|
| 153 | default management capabilities expose OSS-safe feature flags | open |
| 165 | FERRICSTORE.CAPABILITIES returns string-keyed SDK contract | open |
| 179 | FLOW_QUERY transports opaque cursor IDs without scalar coercion | open |
| 196 | capabilities can be extended by a configured implementation | open |
| 210 | capability adapters fail clearly when they violate the public contract | open |
| 224 | management mutations fail closed in OSS | open |
| 241 | ACL LOAD delegates to the configured durable management adapter | open |
| 248 | resource limit enforcement hooks are neutral no-ops by default | open |
| 269 | resource limit command hooks delegate to configured implementation | open |
| 305 | FERRICSTORE.QUOTA delegates to configured resource limit implementation | open |
| 322 | FERRICSTORE.NAMESPACE delegates mutations with caller store opts | open |
| 332 | ACL command delegates to configured implementation | open |
| 355 | native AST parser recognizes management command names | open |

## `ferricstore/test/ferricstore/raft/apply_context_test.exs`

| Line | Test | Status |
|---:|---|---|
| 8 | history hot limits cannot exceed the total history maximum | open |
| 61 | the replicated schema tag reflects store-wide apply ownership | open |
| 68 | captured retention limits do not change while a command is applying | open |
| 88 | standalone shards carry their startup apply context into Flow commands | open |
| 114 | isolated instances accept an explicit promotion threshold | open |
| 129 | captured cleanup and hibernation limits are flat immutable data | open |
| 148 | promotion threshold is captured once and strictly serialized | open |
| 166 | promotion threshold has no process-global runtime API | open |
| 181 | WARaft segment projection reads promotion threshold only from replicated state | open |
| 194 | invalid runtime values normalize once before replication | open |
| 222 | value-size limits are bounded and preserved by the replicated context | open |
| 239 | compound member apply budget is bounded and preserved by the replicated context | open |
| 258 | batch command apply budget is bounded and preserved by the replicated context | open |
| 277 | transaction result byte budget is bounded and preserved by the replicated context | open |
| 296 | transaction command budget is bounded and preserved by the replicated context | open |
| 315 | transaction key apply budget is bounded and preserved by the replicated context | open |
| 334 | Flow batch work is bounded and preserved by the replicated context | open |
| 351 | command construction and replicated apply clamp Flow batch work identically | open |
| 358 | replicated history limits retain the existing hot-history safety cap | open |
| 371 | replicated work limits clamp runtime values and reject oversized encodings | open |
| 406 | replicated metadata field versions stay inside the unsigned 64-bit contract | open |
| 426 | only policy-sensitive commands carry the compact replicated context | open |
| 450 | all policy-sensitive apply tags and shared-ref wrappers carry context | open |
| 521 | two-field async envelopes are not current context carriers | open |
| 528 | reserved wrappers cannot inject context or pessimize ordinary KV commands | open |
| 568 | ordinary command dispatch does not eagerly encode the apply context | open |
| 585 | large ordinary batches use a constant-stack sanitizer and preserve identity | open |
| 601 | pre-stamped commands cannot bypass or inject replicated context | open |
| 633 | replicated command context overrides a replica's node-local context | open |
| 656 | rollout requires an exact acknowledgement from every raft group | open |
| 676 | rollout fails closed when any raft group does not acknowledge the context | open |
| 690 | targeted rollout only writes the elected raft group | open |
| 706 | barrier adopts and acknowledges the exact replicated context | open |
| 732 | malformed replicated contexts fail closed without running the command | open |
| 754 | malformed replicated wrapper commands fail closed and advance apply accounting | open |
| 775 | retention cleanup budgets cannot exceed the replicated context | open |
| 812 | retention planning derives budgets from the instance apply context | open |
| 838 | Raft apply limit readers do not consult application configuration | open |

## `ferricstore/test/ferricstore/raft/apply_limits_flow_time_test.exs`

| Line | Test | Status |
|---:|---|---|
| 8 | rejects inexact Flow timestamps in top-level and batched command attributes | open |
| 25 | rejects derived deadlines that leave the exact Flow timestamp range | open |
| 39 | validates structural shared attributes but leaves user data opaque | open |
| 57 | both Flow apply wrappers enforce the replicated time invariant | open |
| 61 | replicated apply context bounds the total structural batch footprint | open |
| 76 | both Flow apply wrappers enforce the replicated batch invariant | open |

## `ferricstore/test/ferricstore/raft/flow_retention_active_reverse_test.exs`

| Line | Test | Status |
|---:|---|---|
| 8 | hibernated timeout cleanup accepts only the exact active-timeout reverse plan | open |
| 54 | hibernated timeout cleanup fails closed for missing, corrupt, and unreadable reverses | open |

## `ferricstore/test/ferricstore/raft/state_machine_test/sections/flow_command_time.exs`

| Line | Test | Status |
|---:|---|---|
| 14 | Flow writes materialize a preceding compound fast delete | open |
| 52 | MSET falls back to staged puts after a compound fast delete | open |
| 81 | RMW commands observe a preceding compound fast delete | open |
| 107 | list reads observe a preceding compound fast delete | open |
| 136 | fast-delete materialization ignores unrelated pending writes | open |
| 167 | Flow create rejects a policy reference older than the local generation | open |
| 256 | Flow create rejects a policy generation not yet applied locally | open |
| 287 | a policy fence installs a future generation before its Flow command | open |
| 323 | Flow create rejects a conflicting digest at the same generation | open |
| 366 | an existing-flow command rejects a recreated flow incarnation | open |
| 422 | unstamped Flow commands are rejected instead of reading replica-local policy | open |
| 464 | generation-zero direct puts remain valid before a high-water exists | open |
| 488 | reserved policy puts reject invalid keys and values without storing them | open |
| 550 | policy patch allocation stamps generations and rejects non-map payloads | open |
| 614 | policy patch allocation rejects an out-of-range expected generation | open |
| 639 | policy patch validation uses replicated limits instead of runtime config | open |
| 696 | successive generation-zero direct puts replay before a stamped policy | open |
| 744 | generationless policy payloads cannot overwrite a generation-stamped policy | open |
| 781 | generation-zero direct puts replay across an active migration high-water | open |
| 820 | generation-zero direct puts replay across a completed migration high-water | open |
| 859 | terminal batches are rejected without canonical policy metadata | open |
| 905 | policy patch allocation fails closed on corrupt migration high-water state | open |
| 950 | policy patch allocation and install fail closed on a corrupt stored high-water | open |
| 985 | an absent target guard rejects a flow created before apply | open |
| 1048 | Flow apply rejects an invalid policy reference before mutation | open |
| 1136 | Flow apply rejects conflicting batch policy references before mutation | open |
| 1184 | Flow batches resolve one deduplicated command-level policy reference | open |
| 1233 | internal policy generations do not replace public policy versions | open |
| 1260 | Flow create does not return projection failure after committing state | open |
| 1293 | uses stamped apply time when Flow attrs omit now_ms | open |
| 1399 | create_many stages Flow state writes into one append batch and projects history | open |
| 1461 | Ra-batched Flow commands share state append batch but keep semantic results | open |
| 1557 | claim_due pages past 256 stale due indexes without hiding an eligible job | open |
| 1703 | claim_due stages claimed state records into one append batch | open |
| 1782 | a terminal batch answers a refused state write instead of raising | open |
| 1846 | claim_due apply uses a claim-specific bulk index plan | open |
| 1859 | claim_due bulk index plan keeps metadata and reclaimed running indexes correct | open |
| 2031 | claim_due mirror does not enqueue full active state blobs | open |
| 2158 | active Flow writes do not enqueue cold LMDB projection | open |
| 2297 | Flow hot path does not depend on LMDB writer availability | open |

## `ferricstore/test/ferricstore/raft/state_machine_test/sections/flow_governance_limit.exs`

| Line | Test | Status |
|---:|---|---|
| 7 | malformed limit mutation is rejected as an applied command | open |
| 17 | new limit replay resumes a durable catalog intent without cursor metadata | open |
| 26 | new limit replay resumes durable catalog intent and cursor metadata | open |
| 35 | new limit replaces an intent-only prefix whose owner never committed | open |

## `ferricstore/test/ferricstore/raft/state_machine_test/sections/flow_governance_release_outbox.exs`

| Line | Test | Status |
|---:|---|---|
| 7 | malformed release outbox acknowledgement is rejected as an applied command | open |
| 24 | oversized release completion is rejected as an applied command | open |

## `ferricstore/test/ferricstore/raft/state_machine_test/sections/flow_index_rollback.exs`

| Line | Test | Status |
|---:|---|---|
| 14 | native Flow index apply errors are returned before durable publication | open |
| 40 | native Flow index preparation failures happen before durable append | open |
| 89 | native Flow index apply failures rebuild from committed keydir | open |
| 149 | rolls back native Flow index mutations when apply append fails | open |
| 197 | does not roll back published storage when native Flow index repair fails | open |
| 259 | waits for promoted compaction latch before cleanup | open |
| 312 | applied writes rotate the active file when they exceed max_active_file_size | open |
| 335 | persists the pending origin value when ETS still matches the command | open |
| 349 | replays origin PUT when recovery has no local pending row | open |
| 367 | replays origin large PUT over an older cold value | open |
| 389 | does not duplicate an already-applied origin large PUT | open |
| 409 | persists stale origin PUT for replay without publishing over newer pending ETS | open |
| 455 | replays origin RMW when recovery has no local value | open |
| 470 | replays origin RMW when recovery has the old pre-command value | open |
| 492 | replays origin APPEND when cold location changes during apply read | open |
| 533 | replays origin GETSET when recovery has the pre-command value | open |
| 552 | replays origin GETSET over an unaccepted pending local value | open |
| 571 | does not replay origin GETSET over a durable newer local value | open |
| 590 | does not replay stale origin PUT over a durable newer local value | open |
| 609 | does not replay stale origin DELETE over a durable newer local value | open |
| 628 | does not replay stale origin GETDEL over a durable newer local value | open |
| 647 | does not replay stale origin PUT over a pending newer local value | open |
| 666 | does not replay stale origin DELETE over a pending newer local value | open |
| 685 | does not replay stale origin GETDEL over a pending newer local value | open |
| 704 | replays origin GETDEL tombstone when local delete already removed the key | open |
| 724 | materializes pending origin GETSET even when value equals expected | open |
| 747 | does not replay origin INCR over a provably newer pending local value | open |
| 766 | does not replay origin DECR over a provably newer pending local value | open |
| 785 | replays origin GETEX when recovery has the old expiry | open |
| 805 | replays origin SETRANGE when recovery has the pre-command value | open |
| 824 | replays origin SETRANGE expected value over pending local value | open |
| 847 | replays origin checked large APPEND over an older cold value | open |
| 877 | replays origin async DELETE when recovery still has an older value | open |
| 898 | DELETE aborts when pending PUT cannot be flushed before tombstone | open |
| 931 | origin async DELETE persists tombstone even when Router already removed ETS | open |
| 954 | replays origin async GETDEL when recovery still has an older value | open |
| 975 | origin async GETDEL persists tombstone even when Router already removed ETS | open |
| 998 | GETDEL returns flush error when pending PUT cannot be ordered before tombstone | open |

## `ferricstore/test/ferricstore/raft/state_machine_test/sections/flow_restore.exs`

| Line | Test | Status |
|---:|---|---|
| 16 | a restored run takes its sequence numbers from this shard's Raft position | open |
| 76 | restore refuses a run that already exists | open |

## `ferricstore_server/test/ferricstore_server/native/accounting_gate_test.exs`

| Line | Test | Status |
|---:|---|---|
| 7 | reconciliation waits for an in-progress publication and excludes new entrants | open |
| 50 | a stalled live mutation defers repair instead of resetting active reservations | open |
| 70 | an interrupted unindexed grant to a surviving owner restores its index and monitor | open |
| 99 | death notifications reclaim interrupted reservations without a saturation request | open |

## `ferricstore_server/test/ferricstore_server/native/admission_test.exs`

| Line | Test | Status |
|---:|---|---|
| 7 | capacity is global and reclaimed when an owner is killed | open |
| 36 | a restart preserves live leases and accepts their original release tokens | open |

## `ferricstore_server/test/ferricstore_server/native/apply_budget_error_test.exs`

| Line | Test | Status |
|---:|---|---|
| 30 | RPUSH past the member budget explains the limit | open |
| 40 | HDEL past the member budget explains the limit | open |

## `ferricstore_server/test/ferricstore_server/native/blocking_lane_order_test.exs`

| Line | Test | Status |
|---:|---|---|
| 19 | later independent lane can unblock a lane with deferred work | open |
| 37 | replayed work defers again when its next command also blocks | open |

## `ferricstore_server/test/ferricstore_server/native/blocking_test.exs`

| Line | Test | Status |
|---:|---|---|
| 12 | blocking workers are monitored by their connection owner | open |
| 34 | blocking workers terminate when their connection owner dies | open |
| 64 | blocking workers share a server-wide waiter budget | open |
| 98 | finite blocking timeouts are not starved by queued notifications | open |
| 123 | list workers rearm after a consumed notification finds no remaining value | open |
| 173 | #{command} drains a multi-value push through FIFO waiter handoffs | open |
| 228 | blocking results retain outbound capacity after the worker exits | open |
| 267 | completed list-worker cleanup does not scale with unrelated waiter keys | open |
| 316 | a completed multi-key pop unregisters every watched key before worker exit | open |
| 363 | blocking result overflow emits a bounded close signal without the result | open |
| 394 | XREAD remains wakeable when a stream is deleted and immediately recreated | open |
| 432 | XREAD wakes with WRONGTYPE when SET replaces its stream | open |
| 471 | XREAD remains wakeable across FLUSHDB and immediate recreation | open |
| 509 | BRPOPLPUSH is not exposed as a blocking compatibility command | open |
| 564 | a served blocking pop notifies the key it popped | open |

## `ferricstore_server/test/ferricstore_server/native/codec_test.exs`

| Line | Test | Status |
|---:|---|---|
| 20 | request frames round-trip with lane id and request id | covered: fsnp::frame::parses_a_request_header_and_writes_the_matching_response |
| 28 | native NIF decodes the same frame tuple shape as the Elixir codec | internal: Elixir NIF and scheduler details |
| 36 | frame decoding bounds one pass and preserves buffered continuation frames | covered: session::queue::replies_on_one_lane_keep_request_order (frames sent in one write); the per-pass bound is BEAM scheduling (internal) |
| 51 | native NIF encodes the same frame header as the Elixir codec | internal: Elixir NIF and scheduler details |
| 58 | partial request frame leaves bytes in remainder | covered: fsnp::robustness::every_strict_prefix_of_a_request_is_refused |
| 67 | command probe skips nested values and uses the last duplicate command | internal: Elixir's routing probe before a full decode |
| 90 | command probe rejects custom, malformed, and over-budget typed payloads | internal: Elixir's routing probe before a full decode |
| 110 | response frames use response direction bit and are rejected as client input | covered: fsnp::frame::refuses_bad_magic_version_and_oversized_bodies (version 0x81) |
| 117 | response value sizing matches the wire encoding at exact boundaries | internal: Elixir's precomputed reply sizes |
| 128 | successful responses over their byte budget become bounded errors | ported: fsnp::frame::a_reply_longer_than_a_frame_is_refused_whole |
| 141 | busy response is structured and retryable | covered: session::backpressure::a_busy_refusal_is_the_retryable_busy_reply |
| 157 | chunked compressed responses mark compression only on the final logical chunk | surface: KV, Stream, PubSub, chunking or compression codecs the node does not serve |
| 179 | typed values round-trip nested protocol payloads | covered: codec::tests::every_tag_round_trips |
| 192 | typed values round-trip the full unsigned 64-bit domain | covered: codec::tests::every_tag_round_trips, integer_and_flag_readers_accept_only_their_types |
| 199 | typed value decoder rejects containers over configured item limit | covered: codec::tests::malformed_values_are_refused (TooManyItems) |
| 206 | typed value decoder applies the item limit across nested containers | covered: codec::tests::malformed_values_are_refused, fsnp::robustness::huge_declared_lengths_are_refused_without_allocating |
| 217 | typed value decoder rejects nesting over configured depth limit | covered: codec::tests::nesting_is_bounded |
| 225 | invalid typed value limits fail closed instead of disabling validation | internal: Elixir's configurable decode limits; the node's are constants |
| 238 | compact Flow claim jobs response uses custom payload tag | covered: fsnp::responses::compact_claim_jobs_match_elixir |
| 249 | compact Flow claim jobs response encodes state-only tuples | covered: fsnp::responses::claim_shapes_add_run_state_and_attributes |
| 260 | native NIF directly frames state-only compact Flow claims | internal: Elixir NIF and scheduler details |
| 278 | compact Flow claim jobs response can include attributes | covered: fsnp::responses::claim_shapes_add_run_state_and_attributes |
| 294 | compact Flow claim jobs response rejects mixed tuple modes | internal: an encoder check on Elixir's own tuples |
| 304 | compact Flow record response encodes known atom fields | surface: the compact Flow record codec (flow_record_v1) is not offered |
| 316 | compact Flow record response preserves unknown extension fields | surface: the compact Flow record codec (flow_record_v1) is not offered |
| 332 | compact OK-list response stores only count | surface: KV, Stream, PubSub, chunking or compression codecs the node does not serve |
| 340 | compact integer-list response stores pipeline count results directly | surface: KV, Stream, PubSub, chunking or compression codecs the node does not serve |
| 349 | native NIF encodes compact MGET payload directly | internal: Elixir NIF and scheduler details |
| 359 | decodes compact MSET and MGET request bodies | surface: KV, Stream, PubSub, chunking or compression codecs the node does not serve |
| 373 | compact request decoder rejects collections over the native item budget before allocating | covered: fsnp::robustness::huge_declared_lengths_are_refused_without_allocating |
| 386 | compact HMGET decoder applies one budget across nested field lists | surface: KV, Stream, PubSub, chunking or compression codecs the node does not serve |
| 398 | decodes compact append-only Stream pipeline request bodies | surface: KV, Stream, PubSub, chunking or compression codecs the node does not serve |
| 420 | decodes compact PubSub publish pipeline request bodies | surface: KV, Stream, PubSub, chunking or compression codecs the node does not serve |
| 435 | compact Stream pipeline decoder bounds nested field/value items | surface: KV, Stream, PubSub, chunking or compression codecs the node does not serve |
| 447 | compact Stream pipeline decoder rejects empty field lists | surface: KV, Stream, PubSub, chunking or compression codecs the node does not serve |
| 454 | decodes compact SMEMBERS pipeline request body | surface: KV, Stream, PubSub, chunking or compression codecs the node does not serve |
| 467 | decodes compact HMGET and ZSCORE pipeline request bodies | surface: KV, Stream, PubSub, chunking or compression codecs the node does not serve |
| 494 | decodes compact HGETALL pipeline request body and encodes binary map lists | surface: KV, Stream, PubSub, chunking or compression codecs the node does not serve |
| 512 | decodes compact pipeline FLOW.STEP_CONTINUE request bodies | surface: the compact pipeline codec (pipeline_v1) is not offered; clients send typed pipelines |
| 538 | decodes compact pipeline FLOW.VALUE.PUT request bodies | surface: the compact pipeline codec (pipeline_v1) is not offered; clients send typed pipelines |
| 601 | decodes compact pipeline FLOW.GET request bodies | surface: the compact pipeline codec (pipeline_v1) is not offered; clients send typed pipelines |
| 612 | decodes compact pipeline partitioned FLOW.GET request bodies | surface: the compact pipeline codec (pipeline_v1) is not offered; clients send typed pipelines |
| 630 | decodes compact pipeline FLOW.GET meta request bodies | surface: the compact pipeline codec (pipeline_v1) is not offered; clients send typed pipelines |
| 648 | decodes compact pipeline FLOW.HISTORY request bodies | surface: the compact pipeline codec (pipeline_v1) is not offered; clients send typed pipelines |
| 670 | decodes compact pipeline FLOW.SIGNAL request bodies | surface: the compact pipeline codec (pipeline_v1) is not offered; clients send typed pipelines |
| 694 | decodes compact pipeline FLOW.START_AND_CLAIM request bodies | surface: the compact pipeline codec (pipeline_v1) is not offered; clients send typed pipelines |
| 718 | decodes compact pipeline FLOW.START_AND_CLAIM job-only request bodies | surface: the compact pipeline codec (pipeline_v1) is not offered; clients send typed pipelines |
| 739 | native NIF encodes compact OK-list response frame directly | internal: Elixir NIF and scheduler details |
| 753 | native NIF encodes compact KV response frames directly | internal: Elixir NIF and scheduler details |
| 789 | native NIF encodes Stream range rows as the generic wire format | internal: Elixir NIF and scheduler details |
| 801 | native Stream row encoder preserves byte limits and rejects other shapes | surface: KV, Stream, PubSub, chunking or compression codecs the node does not serve |
| 818 | native NIF encodes persisted Stream field blobs without materializing rows | internal: Elixir NIF and scheduler details |
| 867 | raw Stream range responses use direct frames and safely materialize fallbacks | surface: KV, Stream, PubSub, chunking or compression codecs the node does not serve |
| 908 | large command Stream ranges use a wire-equivalent direct frame | surface: KV, Stream, PubSub, chunking or compression codecs the node does not serve |
| 918 | large non-Stream nested lists fall back to generic response encoding | surface: KV, Stream, PubSub, chunking or compression codecs the node does not serve |
| 925 | large Stream direct frames preserve compression and response limit behavior | surface: KV, Stream, PubSub, chunking or compression codecs the node does not serve |
| 941 | command response frames use native compact KV fast path | surface: KV, Stream, PubSub, chunking or compression codecs the node does not serve |
| 982 | chunked MGET encoding keeps values as bounded iodata frames | surface: KV, Stream, PubSub, chunking or compression codecs the node does not serve |
| 1011 | command response frames compact hot Flow many responses when negotiated | covered: session::queue::the_sdk_queue_frames_get_elixir_shaped_compact_replies |
| 1028 | command response frames use native compact claim jobs fast path for list jobs | covered: session::queue::the_sdk_queue_frames_get_elixir_shaped_compact_replies |
| 1053 | command response frames preserve full claim job maps when compact is negotiated | covered: session::flow::a_typed_map_claim_replies_in_the_requested_shape |
| 1129 | command response frames compact direct Flow records when negotiated | surface: the compact Flow record codec (flow_record_v1) is not offered |
| 1151 | compact pipeline response supports Flow records and Flow record lists | surface: the compact pipeline codec (pipeline_v1) is not offered; clients send typed pipelines |
| 1164 | compact pipeline response supports Flow value refs | surface: the compact pipeline codec (pipeline_v1) is not offered; clients send typed pipelines |
| 1177 | compact pipeline response supports binary lists | surface: the compact pipeline codec (pipeline_v1) is not offered; clients send typed pipelines |
| 1184 | compact binary list list encodes values-only range responses | surface: KV, Stream, PubSub, chunking or compression codecs the node does not serve |
| 1191 | compact binary map entry list encodes values-only hash responses without maps | surface: KV, Stream, PubSub, chunking or compression codecs the node does not serve |
| 1198 | command response frames keep normal encoding when compact is disabled | covered: session::queue::without_negotiation_replies_are_typed |
| 1210 | decodes compact FLOW.CREATE_MANY request body | covered: fsnp::requests::parses_the_sdk_create_many_request, session::compact_create |
| 1253 | decodes compact FLOW.CREATE_MANY request body with partition key | covered: session::compact_create::a_partition_batch_is_the_generic_command_with_its_items |
| 1293 | decodes compact FLOW.CREATE_MANY request body with mixed item partitions | covered: session::compact_create::an_independent_mixed_batch_takes_the_queue_path |
| 1338 | decodes compact FLOW.CLAIM_DUE request body | covered: fsnp::requests::parses_the_sdk_compact_claim_due_request |
| 1372 | decodes compact FLOW.VALUE.MGET request body | ported: request::compact_many::tests::a_value_read_takes_its_references_and_byte_limit |
| 1389 | rejects the retired compact FLOW.LIST request marker | surface: KV, Stream, PubSub, chunking or compression codecs the node does not serve |
| 1403 | decodes compact FLOW.COMPLETE_MANY request body | covered: fsnp::requests::parses_the_sdk_compact_complete_many_request; ported request::compact_many::tests::ok_on_success_batches_read_like_their_typed_form |
| 1431 | decodes compact FLOW.COMPLETE_MANY ok-on-success request body | ported: request::compact_many::tests::ok_on_success_batches_read_like_their_typed_form |
| 1460 | decodes compact FLOW.COMPLETE_MANY local-only request marker | ported: request::compact_many::tests::the_independence_byte_and_local_only_markers_are_read (local routing is internal) |
| 1486 | decodes compact FLOW.RETRY_MANY ok-on-success request body | ported: request::compact_many::tests::ok_on_success_batches_read_like_their_typed_form |
| 1522 | decodes compact FLOW.FAIL_MANY ok-on-success request body | ported: request::compact_many::tests::ok_on_success_batches_read_like_their_typed_form |
| 1551 | decodes compact FLOW.CANCEL_MANY ok-on-success request body | ported: request::compact_many::tests::ok_on_success_batches_read_like_their_typed_form |
| 1579 | decodes compact FLOW.TRANSITION_MANY ok-on-success request body | ported: request::compact_many::tests::ok_on_success_batches_read_like_their_typed_form |

## `ferricstore_server/test/ferricstore_server/native/commands_test.exs`

| Line | Test | Status |
|---:|---|---|
| 138 | compact collection pipelines reject over-limit counts before scanning | surface: KV, Stream, PubSub, MULTI, ACL or tenant surface the node does not serve |
| 176 | compact compound reads preserve shard failures | surface: KV, Stream, PubSub, MULTI, ACL or tenant surface the node does not serve |
| 210 | FLOW.QUERY exposes retry-safe storage outages without provider details | internal: the node answers a query from the shards holding the Flows; there is no separate query storage to lose |
| 240 | FLOW.QUERY never exposes a primary record that violates its predicates | internal: a record copied under another partition's key is a KV-store fault; the node holds each Flow once |
| 588 | STARTUP negotiates compact query result framing explicitly | surface: the query result codec is not offered; STARTUP takes its name and does not select it (negotiation::a_malformed_codec_request_is_a_bad_request_that_keeps_the_earlier_choice) |
| 612 | HELLO can negotiate the same explicit compact result codecs without a second round trip | fixed G63 (negotiation::hello_negotiates_compact_replies_as_startup_does) |
| 629 | STARTUP negotiates batched PubSub events explicitly | surface: KV, Stream, PubSub, MULTI, ACL or tenant surface the node does not serve |
| 645 | STARTUP rejects malformed, duplicate, and unsupported response codec requests | fixed G63 (negotiation::a_malformed_codec_request_is_a_bad_request_that_keeps_the_earlier_choice, a_refused_handshake_keeps_the_earlier_choice) |
| 664 | OPTIONS advertises native protocol capabilities and command coverage | covered: handshake::hello_declares_the_fields_the_sdk_requires, hello_declares_every_policy_field_the_node_takes; the rest lists KV and PubSub surfaces and schemas the SDKs no longer need declared (Elixir SDK #31) |
| 795 | retired Flow collection opcodes are rejected | surface: KV, Stream, PubSub, MULTI, ACL or tenant surface the node does not serve |
| 802 | OPTIONS advertises the capability manifest frozen into its instance | internal: Elixir's pluggable query engine |
| 810 | HELLO returns native route metadata only | covered: handshake::hello_declares_the_fields_the_sdk_requires |
| 830 | FLOW_WAKE fails closed without shared tenant authority | surface: KV, Stream, PubSub, MULTI, ACL or tenant surface the node does not serve |
| 847 | FLOW_WAKE broad subscriptions do not observe another shared tenant | surface: KV, Stream, PubSub, MULTI, ACL or tenant surface the node does not serve |
| 872 | compact FLOW.GET pipelines preserve shared tenant scope | surface: KV, Stream, PubSub, MULTI, ACL or tenant surface the node does not serve |
| 948 | HELLO bounds retained client names and requires UTF-8 | fixed G63 (negotiation::compression_and_client_names_are_checked_as_elixir_checks_them) |
| 979 | HELLO, STARTUP, and CLIENT.SETNAME detach codec-backed client names | internal: BEAM sub-binary retention |
| 1003 | HELLO redacts native endpoints before authentication is complete | surface: KV, Stream, PubSub, MULTI, ACL or tenant surface the node does not serve |
| 1021 | native AUTH does not reveal whether an ACL username exists | surface: KV, Stream, PubSub, MULTI, ACL or tenant surface the node does not serve |
| 1042 | native AUTH rate limits repeated password verification by peer IP | surface: KV, Stream, PubSub, MULTI, ACL or tenant surface the node does not serve |
| 1078 | successful native AUTH does not consume the failure budget | surface: KV, Stream, PubSub, MULTI, ACL or tenant surface the node does not serve |
| 1104 | a native connection pool authenticating at once is not rate limited | surface: AUTH |
| 1135 | native AUTH rejects oversized credentials before authentication | surface: KV, Stream, PubSub, MULTI, ACL or tenant surface the node does not serve |
| 1157 | ROUTE returns leader-aware native endpoint metadata | surface: KV, Stream, PubSub, MULTI, ACL or tenant surface the node does not serve |
| 1175 | SHARDS returns leader-aware endpoint metadata per slot range | covered: topology::shards_lists_one_leader_range_per_shard |
| 1191 | least-privilege native clients can bootstrap topology and event subscriptions | surface: KV, Stream, PubSub, MULTI, ACL or tenant surface the node does not serve |
| 1225 | COMMAND_EXEC delegates through native AST parser | covered: handshake::a_ping_sent_as_a_text_command_answers_as_the_ping_opcode |
| 1233 | COMMAND_EXEC XADD uses the atomic Stream append path | surface: KV, Stream, PubSub, MULTI, ACL or tenant surface the node does not serve |
| 1268 | COMMAND_EXEC can defer large Stream range materialization to the native encoder | surface: KV, Stream, PubSub, MULTI, ACL or tenant surface the node does not serve |
| 1300 | PIPELINE batches same-shard COMMAND_EXEC XADDs into one Raft entry | surface: KV, Stream, PubSub, MULTI, ACL or tenant surface the node does not serve |
| 1345 | compact Stream producer pipeline commits one Raft entry and returns compact IDs | surface: KV, Stream, PubSub, MULTI, ACL or tenant surface the node does not serve |
| 1380 | compact PubSub pipeline publishes in order and returns compact subscriber counts | surface: KV, Stream, PubSub, MULTI, ACL or tenant surface the node does not serve |
| 1412 | compact PubSub pipeline prepares bounded batch envelopes once for fanout | surface: KV, Stream, PubSub, MULTI, ACL or tenant surface the node does not serve |
| 1473 | typed PubSub pipeline reuses one guarded batch delivery | surface: KV, Stream, PubSub, MULTI, ACL or tenant surface the node does not serve |
| 1509 | compact PubSub pipeline fallback preserves command ACL checks | surface: KV, Stream, PubSub, MULTI, ACL or tenant surface the node does not serve |
| 1530 | compact Stream producer keeps reserved-key and same-shard validation | surface: KV, Stream, PubSub, MULTI, ACL or tenant surface the node does not serve |
| 1582 | batched COMMAND_EXEC XADD preserves same-stream ordering and per-command errors | surface: KV, Stream, PubSub, MULTI, ACL or tenant surface the node does not serve |
| 1629 | PIPELINE keeps exact-trim COMMAND_EXEC XADDs on the sequential correctness path | surface: KV, Stream, PubSub, MULTI, ACL or tenant surface the node does not serve |
| 1681 | native command exceptions are logged without exposing details to clients | surface: extension commands |
| 1698 | metrics exceptions return an error instead of empty success | surface: KV, Stream, PubSub, MULTI, ACL or tenant surface the node does not serve |
| 1716 | metrics exits return an error instead of terminating the request | surface: KV, Stream, PubSub, MULTI, ACL or tenant surface the node does not serve |
| 1732 | cluster feature levels are readable and raisable through native opcodes | surface: KV, Stream, PubSub, MULTI, ACL or tenant surface the node does not serve |
| 1751 | native admin args reject structured values without raising | surface: KV, Stream, PubSub, MULTI, ACL or tenant surface the node does not serve |
| 1756 | FLOW.POLICY.SET accepts indexed attributes through native opcode | covered: flow_policy::policies_merge_apply_on_every_shard_and_survive_a_restart, audit::state_meta::the_indexed_key_follows_the_policy |
| 1777 | FLOW.POLICY.SET accepts max_active_ms through native opcode | covered: audit::active_timeouts::a_create_limit_overrides_or_opts_out_of_the_type_s |
| 1788 | FLOW.POLICY.SET supports patch, replacement, and generation CAS through native opcode | covered: flow_policy::policies_merge_apply_on_every_shard_and_survive_a_restart |
| 1847 | native Flow creation opcodes accept max_active_ms | ported: audit::native_contract::every_create_path_keeps_its_own_active_time_limit |
| 1905 | native interval schedules accept the bounded catch-up policy | covered: audit::schedule_catchups::only_intervals_take_the_fire_once_catch_up_policy, a_queued_or_paused_interval_catches_up_in_one_fire |
| 1976 | native FLOW.CREATE preserves canonical lineage and creation semantics | covered: audit::lineage::a_flow_keeps_its_lineage_and_lineage_queries_find_it, audit::history_limits |
| 2007 | FLOW.SPAWN_CHILDREN accepts max_active_ms in child payloads | covered: audit::children_rules::a_group_spawn_is_idempotent_and_refuses_a_different_group |
| 2051 | native Flow opcodes enforce FIFO lane claims | covered: flow_fifo::fifo_lanes_take_one_head_per_partition_and_keep_order_across_a_restart |
| 2126 | FLOW.QUERY executes a parameterized run lookup through the point-read path | covered: query/fql/reference_tests.rs (steps 11-15 audit); parity probe scenario_query_native.py equal to Elixir |
| 2173 | FLOW.QUERY preserves partition isolation for identical run IDs | covered: parity probe scenario_query_native.py (same run id in two partitions) equal to Elixir; query/fql/reference_bind_tests.rs |
| 2238 | FLOW.QUERY executes reversed predicates and escaped literals exactly | covered: query/fql/reference_tests.rs; parity probe equal to Elixir |
| 2270 | FLOW.QUERY record projection includes query metadata but excludes operational secrets | covered: parity probe scenario_query_native.py (record has attributes and state_meta, no lease or payload) equal to Elixir |
| 2325 | FLOW.QUERY returns only explicitly projected run fields | covered: parity probe scenario_query_native.py equal to Elixir; query/fql/reference_tests.rs |
| 2365 | FLOW.QUERY returns source-specific projected history fields | covered: query/fql/reference_tests.rs; parity probe equal to Elixir |
| 2395 | FLOW.QUERY rejects operational fields in an explicit projection | covered: query/error_tests.rs; parity probe equal to Elixir |
| 2409 | FLOW.QUERY EXPLAIN is deterministic and redacts predicate values | covered: parity probe (two EXPLAINs identical, values redacted); the plan text describes the node's engine (internal) |
| 2461 | FLOW.QUERY rejects versions and unsupported shapes before storage access | covered: flow::flow_query_reaches_the_service_and_bad_requests_keep_their_status, query/error_tests.rs |
| 2484 | FLOW.QUERY returns positioned, actionable parser diagnostics | covered: query/error_tests.rs |
| 2527 | FLOW.QUERY and COMMAND_EXEC return identical structured diagnostics | fixed G65 (pipeline_rules::command_exec_keeps_a_query_failure_s_status_and_diagnostic) |
| 2566 | FLOW.QUERY reports admission saturation as retryable busy | internal: Elixir's bounded query worker pool; node queries run on shard threads under ADR-017 admission |
| 2593 | FLOW.QUERY rejects malformed envelopes before provider dispatch | covered: flow::flow_query_reaches_the_service_and_bad_requests_keep_their_status; parity probe scenario_transport.py equal to Elixir |
| 2619 | COMMAND_EXEC FLOW.QUERY preserves named-parameter envelope errors | covered: query/text_tests.rs (G39) |
| 2647 | FLOW.QUERY rejects malformed deadlines before invoking its engine | covered: flow::a_request_runs_before_its_deadline_and_is_refused_after_it (G62) |
| 2668 | FLOW.QUERY carries absolute deadlines to both query execution paths | covered: flow::a_request_runs_before_its_deadline_and_is_refused_after_it (G62) |
| 2716 | FLOW.QUERY preserves EXPLAIN ANALYZE mode on both native command paths | covered: query/fql/reference_tests.rs (EXPLAIN ANALYZE); COMMAND_EXEC runs the same parse (G39) |
| 2738 | FLOW.QUERY carries only the connection-negotiated result codec on both native paths | surface: the query result codec is not offered |
| 2760 | FLOW.QUERY passes only trusted request context to the installed query engine | surface: KV, Stream, PubSub, MULTI, ACL or tenant surface the node does not serve |
| 2822 | FLOW.QUERY passes trusted authority without copying the instance context | surface: KV, Stream, PubSub, MULTI, ACL or tenant surface the node does not serve |
| 2860 | FLOW.QUERY uses the connection-frozen request-context trust policy | surface: KV, Stream, PubSub, MULTI, ACL or tenant surface the node does not serve |
| 2899 | FLOW.QUERY bounds trusted request context before provider dispatch | surface: KV, Stream, PubSub, MULTI, ACL or tenant surface the node does not serve |
| 2928 | FLOW.QUERY preserves installed engine authorization failures | surface: KV, Stream, PubSub, MULTI, ACL or tenant surface the node does not serve |
| 2945 | FLOW.QUERY contains installed engine failures at the native boundary | surface: KV, Stream, PubSub, MULTI, ACL or tenant surface the node does not serve |
| 2962 | FLOW.QUERY keeps structured failures consistent in pipelines | fixed G65 (pipeline_rules::pipeline_items_keep_their_refusal_s_status_and_value) |
| 3001 | COMMAND_EXEC FLOW.QUERY preserves resource governance and activity accounting | surface: KV, Stream, PubSub, MULTI, ACL or tenant surface the node does not serve |
| 3049 | COMMAND_EXEC FLOW.QUERY rejects resource-limit failures before provider dispatch | surface: KV, Stream, PubSub, MULTI, ACL or tenant surface the node does not serve |
| 3079 | native write commands expose canonical mutation arguments to resource governance | surface: KV, Stream, PubSub, MULTI, ACL or tenant surface the node does not serve |
| 3126 | COMMAND_EXEC delegates configured extension commands | surface: KV, Stream, PubSub, MULTI, ACL or tenant surface the node does not serve |
| 3140 | COMMAND_EXEC exposes the intrinsic OSS query-index status contract | covered: query/text_tests.rs::index_ids_are_checked_before_their_lookup, query/indexes.rs |
| 3159 | COMMAND_EXEC ignores request context unless the native user is trusted | surface: KV, Stream, PubSub, MULTI, ACL or tenant surface the node does not serve |
| 3177 | COMMAND_EXEC attaches trusted request context to extension store | surface: KV, Stream, PubSub, MULTI, ACL or tenant surface the node does not serve |
| 3205 | COMMAND_EXEC rejects malformed trusted request-context scopes | surface: KV, Stream, PubSub, MULTI, ACL or tenant surface the node does not serve |
| 3221 | PIPELINE attaches top-level trusted request context to extension commands | surface: KV, Stream, PubSub, MULTI, ACL or tenant surface the node does not serve |
| 3261 | PIPELINE rejects a structured atomicity value without crashing | fixed G64 (pipeline_rules::a_pipeline_takes_only_elixir_s_atomicity_modes) |
| 3270 | COMMAND_EXEC authorizes extension command and key metadata | surface: KV, Stream, PubSub, MULTI, ACL or tenant surface the node does not serve |
| 3313 | COMMAND_EXEC prepares extension key metadata once | surface: KV, Stream, PubSub, MULTI, ACL or tenant surface the node does not serve |
| 3330 | same_shard pipeline validates COMMAND_EXEC routing keys | surface: KV, Stream, PubSub, MULTI, ACL or tenant surface the node does not serve |
| 3365 | GET rejects an oversized cold value before materializing it | surface: KV, Stream, PubSub, MULTI, ACL or tenant surface the node does not serve |
| 3396 | MGET rejects a response whose encoded bytes exceed the connection budget | surface: KV, Stream, PubSub, MULTI, ACL or tenant surface the node does not serve |
| 3416 | pipeline GET fast paths reject oversized cold values before materializing them | surface: KV, Stream, PubSub, MULTI, ACL or tenant surface the node does not serve |
| 3456 | collection reads reject oversized cold payloads before materializing members | surface: KV, Stream, PubSub, MULTI, ACL or tenant surface the node does not serve |
| 3522 | compact ZRANGE checks a cold string's metadata without reading its value | surface: KV, Stream, PubSub, MULTI, ACL or tenant surface the node does not serve |
| 3551 | compact ZRANGE ignores a ready index after the type marker expires | surface: KV, Stream, PubSub, MULTI, ACL or tenant surface the node does not serve |
| 3584 | same_shard pipeline rejects coordinated COMMAND_EXEC Flow routing | fixed G64 (pipeline_rules::a_pipeline_takes_only_elixir_s_atomicity_modes) |
| 3631 | same_shard pipeline rejects global data mutations and keyspace reads | surface: KV, Stream, PubSub, MULTI, ACL or tenant surface the node does not serve |
| 3659 | COMMAND_EXEC authorizes ACL subcommands before dispatch | surface: KV, Stream, PubSub, MULTI, ACL or tenant surface the node does not serve |
| 3673 | COMMAND_EXEC ACL WHOAMI returns the authenticated session username | surface: KV, Stream, PubSub, MULTI, ACL or tenant surface the node does not serve |
| 3686 | native full-access fast path fails closed while ACL projection is stale | surface: KV, Stream, PubSub, MULTI, ACL or tenant surface the node does not serve |
| 3701 | COMMAND_EXEC dispatches replicated ACL mutations and invalidates cached sessions | surface: KV, Stream, PubSub, MULTI, ACL or tenant surface the node does not serve |
| 3746 | COMMAND_EXEC enforces scoped keys for management commands | surface: KV, Stream, PubSub, MULTI, ACL or tenant surface the node does not serve |
| 3776 | COMMAND_EXEC enforces admin and dangerous categories for scoped credentials | surface: KV, Stream, PubSub, MULTI, ACL or tenant surface the node does not serve |
| 3803 | COMMAND_EXEC enforces key scope for scoped credentials | surface: KV, Stream, PubSub, MULTI, ACL or tenant surface the node does not serve |
| 3825 | COMMAND_EXEC authorizes every variadic UNLINK key | surface: KV, Stream, PubSub, MULTI, ACL or tenant surface the node does not serve |
| 3842 | typed command ACLs preserve read-modify-write and routing access | surface: KV, Stream, PubSub, MULTI, ACL or tenant surface the node does not serve |
| 3894 | FLOW.QUERY authorizes the bound partition for dedicated and command-exec paths | surface: KV, Stream, PubSub, MULTI, ACL or tenant surface the node does not serve |
| 3932 | FLOW.QUERY requires a separate ACL permission for static EXPLAIN | surface: KV, Stream, PubSub, MULTI, ACL or tenant surface the node does not serve |
| 3968 | FLOW.QUERY EXPLAIN ANALYZE requires execute and explain ACL permissions | surface: KV, Stream, PubSub, MULTI, ACL or tenant surface the node does not serve |
| 3996 | FLOW.QUERY key denials use one audit command across native envelopes | surface: ACL audit |
| 4042 | native sessions use the same Flow query ACL mode contract | surface: KV, Stream, PubSub, MULTI, ACL or tenant surface the node does not serve |
| 4084 | FLOW.QUERY denies users with neither mode permission before parsing | surface: KV, Stream, PubSub, MULTI, ACL or tenant surface the node does not serve |
| 4095 | native MULTI denies FLOW.QUERY before parsing for users without query permissions | surface: KV, Stream, PubSub, MULTI, ACL or tenant surface the node does not serve |
| 4115 | PIPELINE does not preparse FLOW.QUERY without a query permission | surface: KV, Stream, PubSub, MULTI, ACL or tenant surface the node does not serve |
| 4178 | typed FLOW commands authorize the effective partition instead of the flow id | surface: KV, Stream, PubSub, MULTI, ACL or tenant surface the node does not serve |
| 4192 | FLOW.GET and FLOW.HISTORY without a partition require unrestricted read scope | surface: KV, Stream, PubSub, MULTI, ACL or tenant surface the node does not serve |
| 4220 | FLOW.GET and FLOW.HISTORY authorize an explicit effective partition | surface: KV, Stream, PubSub, MULTI, ACL or tenant surface the node does not serve |
| 4243 | typed FLOW commands authorize nested options that override direct partitions | surface: KV, Stream, PubSub, MULTI, ACL or tenant surface the node does not serve |
| 4267 | typed FLOW.CLAIM_DUE authorizes every requested partition | surface: KV, Stream, PubSub, MULTI, ACL or tenant surface the node does not serve |
| 4284 | typed FLOW partition-wide commands require unrestricted scope without a partition | surface: KV, Stream, PubSub, MULTI, ACL or tenant surface the node does not serve |
| 4296 | claim selectors AUTO ANY and GLOBAL require unrestricted key scope | surface: KV, Stream, PubSub, MULTI, ACL or tenant surface the node does not serve |
| 4341 | raw FLOW ACL extraction cannot confuse option values with partition options | surface: KV, Stream, PubSub, MULTI, ACL or tenant surface the node does not serve |
| 4370 | typed FLOW batch commands authorize shared and item partitions | surface: KV, Stream, PubSub, MULTI, ACL or tenant surface the node does not serve |
| 4414 | compact FLOW batches authorize auto-partitioned normalized item ids | surface: KV, Stream, PubSub, MULTI, ACL or tenant surface the node does not serve |
| 4432 | FLOW.RUN_STEPS_MANY authorizes each effective item partition | surface: KV, Stream, PubSub, MULTI, ACL or tenant surface the node does not serve |
| 4468 | typed FLOW global queries require unrestricted key scope | surface: KV, Stream, PubSub, MULTI, ACL or tenant surface the node does not serve |
| 4487 | anonymous FLOW.VALUE.PUT requires unrestricted scope in typed and raw paths | surface: KV, Stream, PubSub, MULTI, ACL or tenant surface the node does not serve |
| 4506 | schedule ACLs ignore unsupported decoy partitions and protect target partitions | surface: KV, Stream, PubSub, MULTI, ACL or tenant surface the node does not serve |
| 4558 | scoped schedule ACLs require generated targets to use an authorized partition | surface: KV, Stream, PubSub, MULTI, ACL or tenant surface the node does not serve |
| 4594 | native Flow options cannot shadow authorized top-level schedule fields | surface: KV, Stream, PubSub, MULTI, ACL or tenant surface the node does not serve |
| 4633 | native schedule ACLs consume the shared structured command contract | surface: KV, Stream, PubSub, MULTI, ACL or tenant surface the node does not serve |
| 4643 | approval ACLs ignore decoy partitions and protect requested flow scope | surface: KV, Stream, PubSub, MULTI, ACL or tenant surface the node does not serve |
| 4679 | PIPELINE preserves COMMAND_EXEC and typed command ACL checks | surface: KV, Stream, PubSub, MULTI, ACL or tenant surface the node does not serve |
| 4714 | compact PIPELINE fallback preserves key ACL checks | surface: KV, Stream, PubSub, MULTI, ACL or tenant surface the node does not serve |
| 4733 | native transactions deny scoped credential boundary escapes before queueing | surface: KV, Stream, PubSub, MULTI, ACL or tenant surface the node does not serve |
| 4773 | native MULTI rejects retained command bytes before growing the queue | surface: KV, Stream, PubSub, MULTI, ACL or tenant surface the node does not serve |
| 4795 | native MULTI detaches queued arguments from the decoded request frame | surface: KV, Stream, PubSub, MULTI, ACL or tenant surface the node does not serve |
| 4822 | native WATCH enforces connection limits before submitting Raft commands | surface: KV, Stream, PubSub, MULTI, ACL or tenant surface the node does not serve |
| 4844 | native WATCH detaches retained keys from the decoded request frame | surface: KV, Stream, PubSub, MULTI, ACL or tenant surface the node does not serve |
| 4863 | native WATCH batches same-shard tokens into one Raft entry | surface: KV, Stream, PubSub, MULTI, ACL or tenant surface the node does not serve |
| 4889 | native MULTI rejects prepared routing across Raft groups at EXEC | surface: KV, Stream, PubSub, MULTI, ACL or tenant surface the node does not serve |
| 4929 | native MULTI rejects coordinated prepared commands before queueing | surface: KV, Stream, PubSub, MULTI, ACL or tenant surface the node does not serve |
| 4951 | typed HSET rejects a nested field name or value as a bad request | surface: KV, Stream, PubSub, MULTI, ACL or tenant surface the node does not serve |
| 4975 | native MULTI discards the transaction after a command is rejected while queueing | surface: KV, Stream, PubSub, MULTI, ACL or tenant surface the node does not serve |
| 5016 | hash field TTL commands notify only the changes they made | surface: KV, Stream, PubSub, MULTI, ACL or tenant surface the node does not serve |
| 5068 | sorted set STORE commands notify their destination | surface: KV, Stream, PubSub, MULTI, ACL or tenant surface the node does not serve |
| 5092 | ZMPOP notifies the key it popped with the side's event | surface: KV, Stream, PubSub, MULTI, ACL or tenant surface the node does not serve |
| 5108 | ZREMRANGEBY* commands notify the key they removed from | surface: KV, Stream, PubSub, MULTI, ACL or tenant surface the node does not serve |
| 5130 | RPOPLPUSH notifies like LMOVE, and neither notifies an empty source | surface: KV, Stream, PubSub, MULTI, ACL or tenant surface the node does not serve |
| 5150 | a compact SET pipeline notifies every key | surface: KV, Stream, PubSub, MULTI, ACL or tenant surface the node does not serve |
| 5172 | commands and typed operations notify the key and the event | surface: KV, Stream, PubSub, MULTI, ACL or tenant surface the node does not serve |
| 5208 | commands in MULTI and pipelines notify | surface: KV, Stream, PubSub, MULTI, ACL or tenant surface the node does not serve |
| 5247 | native MULTI discards the transaction after a command with the wrong arity | surface: KV, Stream, PubSub, MULTI, ACL or tenant surface the node does not serve |
| 5288 | native MULTI refuses ZRANGE's REV and BYSCORE forms whole and runs the plain one | surface: KV, Stream, PubSub, MULTI, ACL or tenant surface the node does not serve |
| 5321 | native MULTI BITOP keeps an empty destination below feature level 5 | surface: KV, Stream, PubSub, MULTI, ACL or tenant surface the node does not serve |
| 5338 | native MULTI INCR past the integer range keeps the old write below level 5 | surface: KV, Stream, PubSub, MULTI, ACL or tenant surface the node does not serve |
| 5356 | native MULTI DECRBY of the smallest integer keeps the old write below level 5 | surface: KV, Stream, PubSub, MULTI, ACL or tenant surface the node does not serve |
| 5377 | native MULTI GEOSEARCHSTORE keeps the old answer for a missing source below level 5 | surface: KV, Stream, PubSub, MULTI, ACL or tenant surface the node does not serve |
| 5403 | native MULTI GEOSEARCHSTORE keeps the old Earth radius below level 5 | surface: KV, Stream, PubSub, MULTI, ACL or tenant surface the node does not serve |
| 5425 | PFADD drops the sketch's TTL below level 5 | surface: KV, Stream, PubSub, MULTI, ACL or tenant surface the node does not serve |
| 5434 | the embedded expire leaves a Bloom filter intact | surface: KV, Stream, PubSub, MULTI, ACL or tenant surface the node does not serve |
| 5446 | native MULTI refuses level-5 forms (ZRANK WITHSCORE, PFMERGE, HEXPIRE) below level 5 | surface: KV, Stream, PubSub, MULTI, ACL or tenant surface the node does not serve |
| 5491 | native MULTI refuses forms older releases apply differently below level 4 | surface: KV, Stream, PubSub, MULTI, ACL or tenant surface the node does not serve |
| 5524 | native MULTI queues commands with optional arguments | surface: KV, Stream, PubSub, MULTI, ACL or tenant surface the node does not serve |
| 5554 | native MULTI rejects request-scoped commands before queueing | surface: KV, Stream, PubSub, MULTI, ACL or tenant surface the node does not serve |
| 5578 | native MULTI rejects global data mutations before queueing | surface: KV, Stream, PubSub, MULTI, ACL or tenant surface the node does not serve |
| 5590 | replicated ACL disable denies rotated-out native service credential sessions only | surface: KV, Stream, PubSub, MULTI, ACL or tenant surface the node does not serve |
| 5627 | direct ACL mutations invalidate cached native permissions | surface: KV, Stream, PubSub, MULTI, ACL or tenant surface the node does not serve |
| 5651 | replicated ACL delete denies active native service credential sessions | surface: KV, Stream, PubSub, MULTI, ACL or tenant surface the node does not serve |
| 5672 | replicated ACL delete of missing credential does not invalidate other sessions | surface: KV, Stream, PubSub, MULTI, ACL or tenant surface the node does not serve |
| 5698 | native scope-based governance commands enforce key ACLs | surface: KV, Stream, PubSub, MULTI, ACL or tenant surface the node does not serve |
| 5734 | CLIENT TRACKING is explicitly rejected after text protocol removal | surface: KV, Stream, PubSub, MULTI, ACL or tenant surface the node does not serve |
| 5746 | typed RATELIMIT.ADD answers a rejected request as an error, not a successful reply | surface: KV, Stream, PubSub, MULTI, ACL or tenant surface the node does not serve |

## `ferricstore_server/test/ferricstore_server/native/connection_decode_budget_test.exs`

| Line | Test | Status |
|---:|---|---|
| 33 | session classification does not fully decode command payloads | open |
| 48 | drains budgeted frame continuations without waiting for more socket data | open |
| 61 | preserves FIFO when a data frame precedes a session frame on one lane | open |
| 90 | flushes a synchronous session response before following data on one lane | open |
| 114 | orders multiple session and control frames around one lane batch | open |
| 162 | waits for a blocking session command before later same-lane work | open |
| 213 | deferred frames honor queued request byte caps | open |
| 268 | replayed deferred frames transfer queued byte reservations once | open |
| 345 | same-lane blocking work at the inflight cap cannot overtake | open |
| 378 | parked frames over queued request byte caps get ordered busy replies | open |
| 435 | orders a synchronous inflight error after earlier lane work | open |
| 471 | waits only for the session lane while draining another lane | open |
| 528 | buffers a later pipelined write while waiting on a lane barrier | open |
| 592 | closes when a lane terminates before its barrier | open |
| 644 | bounds a stalled lane barrier and cleans up its resources | open |
| 710 | stops a pending lane barrier when the client disconnects | open |
| 767 | preserves an incomplete frame until the remaining socket bytes arrive | open |
| 782 | partial frame assembly has an absolute deadline | open |
| 801 | chunked request assembly has an absolute deadline across complete wire frames | open |
| 826 | blocking commands inside MULTI are rejected by the session path | open |
| 847 | blocking session commands consume the connection inflight window | open |
| 875 | EXEC preserves its transaction when server-wide execution capacity is exhausted | open |
| 914 | NO_REPLY suppresses a data command rejected by the inflight gate | open |
| 950 | NO_REPLY suppresses native session command responses | open |
| 968 | NO_REPLY control commands skip response encoding | open |
| 1013 | control commands consume global execution capacity | open |
| 1041 | handshake and heartbeat frames answer while server execution capacity is exhausted | open |
| 1071 | NO_REPLY suppresses delayed native blocking responses | open |
| 1093 | blocking results release outbound capacity after socket send | open |
| 1117 | blocking result overflow closes the connection | open |
| 1139 | idle timeout does not terminate an active blocking request | open |
| 1158 | all native command forms enforce the connection response byte budget | open |
| 1194 | lane responses close a connection before crossing its outbound byte ceiling | open |
| 1213 | lane responses close when global outbound capacity is exhausted | open |
| 1237 | pubsub events release guarded outbound capacity after socket send | open |
| 1264 | pubsub batches use one guarded reservation and still emit one frame per message | open |
| 1290 | negotiated pubsub batches emit one ordered batch event frame | open |
| 1334 | HELLO refreshes prepared batch delivery metadata for existing subscriptions | open |
| 1371 | negotiated pubsub batches split before the response limit | open |
| 1425 | pubsub coalescing processes an ACL barrier before later events | open |
| 1449 | pubsub closes a slow subscriber before queueing an over-limit event | open |
| 1473 | pubsub batches cannot bypass the slow-subscriber outbound limit | open |
| 1497 | keeps fragmented multi-megabyte frames chunked until the frame is complete | open |
| 1534 | buffer accounting work stays constant as the fragment list grows | open |
| 1570 | frame buffer preserves nonuniform fragment order across many fragments | open |
| 1596 | frame buffer keeps already-large fragments independent | open |
| 1622 | accepts a complete maximum-size frame coalesced with continuation bytes | open |
| 1642 | rejects an oversized coalesced read even when its first frame is complete | open |
| 1657 | rejects a declared frame larger than the current buffer budget at its header | open |
| 1666 | unauthenticated connections apply the smaller frame budget | open |
| 1689 | unauthenticated chunk streams cannot exceed the logical frame budget | open |
| 1729 | unauthenticated compressed frames cannot inflate past the logical frame budget | open |
| 1779 | accepts a multi-megabyte frame sent as 64 KiB socket chunks | open |
| 1792 | closes a one-byte fragment batch when metadata exhausts inbound budget | open |
| 1841 | tiny received fragments exhaust metadata admission before the raw payload budget | open |
| 1890 | decoded requests remain charged to the inbound byte budget while queued in a lane | open |
| 1927 | validates configured frame bodies against the wire buffer limit | open |
| 1951 | outbound responses never exceed the connection frame limit | open |
| 1980 | compact MGET responses use the connection frame limit when chunking is disabled | open |
| 2013 | large response encoders run on dirty CPU schedulers | open |

## `ferricstore_server/test/ferricstore_server/native/connection_idle_lane_test.exs`

| Line | Test | Status |
|---:|---|---|
| 30 | a connection frees its idle lanes when the server's lane budget is full | open |
| 44 | lanes idle past the idle period stop and start again on their next frame | open |
| 56 | a lane with a request in flight is not stopped as idle | open |

## `ferricstore_server/test/ferricstore_server/native/flow_query_documentation_test.exs`

| Line | Test | Status |
|---:|---|---|
| 29 | every runnable FQL example parses identically and accepts typed bindings | open |
| 57 | the field reference tracks every supported built-in field | open |
| 70 | the index reference tracks the bundled catalog | open |
| 79 | the execution-limit reference tracks the immutable default budget | open |
| 88 | the guide contains the complete user and operator reference | open |
| 120 | the guide covers every public entry point and versioned response contract | open |
| 146 | the native protocol documents the complete compact query result schema | open |
| 168 | the troubleshooting table covers every structured query error | open |

## `ferricstore_server/test/ferricstore_server/native/flow_query_result_codec_test.exs`

| Line | Test | Status |
|---:|---|---|
| 12 | explicitly selected FLOW.QUERY responses use the fixed-schema result codec | open |
| 27 | COMMAND_EXEC gets the same compact query result contract | open |
| 41 | forwards a prepared bounded query payload without re-encoding its value | open |
| 55 | fails closed instead of framing a corrupted prepared payload | open |
| 74 | unwraps a prepared response when compact negotiation is unavailable | open |
| 89 | the broad compact flag cannot opt a client into an undeclared query codec | open |
| 134 | response byte limits use the selected compact representation | open |
| 167 | compact query results retain custom framing across bounded chunks | open |
| 190 | compact query results compose with negotiated response compression | open |

## `ferricstore_server/test/ferricstore_server/native/fql_parser_test.exs`

| Line | Test | Status |
|---:|---|---|
| 44 | Rust parser matches the Elixir reference parser | open |
| 51 | Rust parser matches bounded reference failures | open |
| 83 | Rust parser matches the oracle across deterministic byte mutations | open |
| 95 | Rust parser matches generated valid grammar combinations | open |
| 134 | Rust parser matches the oracle at exact byte and token limits | open |
| 157 | Rust parser enforces the canonical predicate and IN value limits | open |
| 181 | Rust parser matches the oracle for every truncation of a valid query | open |
| 192 | Rust parser matches the oracle for seeded arbitrary binary input | open |
| 207 | diagnostic parsing positions malformed sources, fields, lists, and terminators | open |
| 250 | diagnostic positions do not leak across failures or successful parses | open |
| 264 | diagnostics preserve bracket fields and report a missing tail at end of input | open |
| 278 | every truncated query diagnostic stays inside its source boundary | open |

## `ferricstore_server/test/ferricstore_server/native/lane_barrier_review_test.exs`

| Line | Test | Status |
|---:|---|---|
| 31 | a control frame does not wait for a blocking command on a lane with an actor | open |
| 49 | a blocking response cannot overtake an earlier session response on its lane | open |
| 68 | a barrier does not read the socket while decode backpressure paused input | open |
| 104 | a control barrier across lanes is bounded by one barrier timeout | open |
| 122 | a control frame skips barrier round trips for idle lanes | open |
| 142 | an ACL invalidation during a barrier still emits AUTH_INVALIDATED | open |
| 170 | a blocking command behind a busy lane does not stall another lane | open |
| 191 | a deferred blocking command keeps lane order around it | open |
| 209 | MULTI/EXEC behind a slow command still executes in lane order | open |
| 227 | a lane busy rejection stays ordered without stalling other lanes | open |
| 264 | pipelined frames on a busy lane classify each command once | open |
| 282 | a barrier stops reading once a complete frame is buffered | open |
| 311 | heartbeat and flow-control frames answer while a data lane is busy | open |
| 334 | deferred lanes are not rescanned for unrelated traffic | open |
| 361 | a MULTI behind a busy lane still captures later frames from other lanes | open |
| 379 | EXEC answers counter commands with their integers | open |
| 394 | a WATCH behind a busy lane still sees later writes from other lanes | open |
| 420 | a command rejected while MULTI is queuing makes EXEC discard the transaction | open |
| 440 | MULTI is refused while a blocking command is pending on the connection | open |
| 467 | a transaction command behind a blocked lane gets an ordered error | open |
| 488 | a queued command that would be parked aborts the transaction | open |
| 517 | the barrier timeout is fixed when the connection starts | open |
| 531 | per-frame dispatch cost does not grow with blocked requests on other lanes | open |
| 569 | pubsub messages are delivered while a barrier waits | open |
| 592 | pubsub during a barrier stops at an ACL invalidation in arrival order | open |

## `ferricstore_server/test/ferricstore_server/native/lane_test.exs`

| Line | Test | Status |
|---:|---|---|
| 83 | prepared fast batches never retry an applied mutation after encoding fails | open |
| 101 | lane workers share server-wide lane capacity | open |
| 122 | lane reports resource governor failure instead of polling forever | open |
| 143 | fetch waiters do not starve their completion at the global execution limit | open |
| 199 | already batched SET frames execute immediately without coalescing delay | open |
| 223 | coalesces ordinary COMMAND_EXEC XADD frames into one Raft entry | open |
| 265 | coalesced COMMAND_EXEC XADD preserves ordering and per-command errors | open |
| 294 | large COMMAND_EXEC Stream ranges encode correctly through the lane | open |
| 330 | trimmed COMMAND_EXEC XADD frames stay on the sequential correctness path | open |
| 361 | COMMAND_EXEC XADD burst coalescing cannot bypass resource governance | open |
| 387 | NO_REPLY lane commands execute without encoding a response | open |
| 418 | lane responses retain exact outbound capacity until the owner releases them | open |
| 447 | lane response overflow emits a bounded close signal instead of the response | open |
| 471 | batched GET and SET frames cannot bypass reserved-key authorization | open |
| 506 | batched GET and MGET reject oversized cold values before materializing them | open |
| 540 | batched SET frames honor resource checks and activity accounting | open |
| 563 | one-frame SET messages retain mailbox order | open |
| 578 | coalesces compact pipeline SET frames into one lane batch | open |
| 593 | coalesces compact Stream producer frames and preserves response boundaries | open |
| 646 | coalesces compact Stream producer frames once per destination shard | open |
| 699 | compact pipeline fast paths cannot bypass resource governance | open |
| 729 | compact pipeline fast paths cannot write reserved keys | open |
| 758 | one-frame compact pipeline SET messages retain mailbox order | open |
| 773 | coalesces compact pipeline data write frames into one lane batch | open |
| 798 | one-frame compact data writes retain mailbox order | open |
| 822 | command state updates are ordering barriers for batchable frames | open |
| 846 | preserves order between compact data write and following read frames | open |
| 861 | preserves order between SET and following GET frames | open |

## `ferricstore_server/test/ferricstore_server/native/listener_backpressure_test.exs`

| Line | Test | Status |
|---:|---|---|
| 6 | TCP listener bounds stalled sends | open |
| 15 | native connection always uses one-shot socket activation | open |

## `ferricstore_server/test/ferricstore_server/native/outbound_budget_test.exs`

| Line | Test | Status |
|---:|---|---|
| 6 | reserves local and global bytes atomically and releases both | open |
| 31 | rejects a per-connection overflow without consuming global capacity | open |
| 48 | growing a lease reserves only the encoded delta and rolls back rejection | open |
| 74 | bulk release clears every local and global lease | open |

## `ferricstore_server/test/ferricstore_server/native/protocol_audit_test.exs`

| Line | Test | Status |
|---:|---|---|
| 24 | deleted text protocol runtime modules are not loadable | open |
| 30 | application config enables the native protocol listener by default | open |
| 34 | Ranch listener capacity matches the configured global maxclients gate | open |
| 39 | application and test source do not reference deleted text protocol modules | open |

## `ferricstore_server/test/ferricstore_server/native/read_error_test.exs`

| Line | Test | Status |
|---:|---|---|
| 30 | GET sanitizes storage read failures | open |
| 35 | MGET sanitizes storage read failures | open |

## `ferricstore_server/test/ferricstore_server/native/request_context_test.exs`

| Line | Test | Status |
|---:|---|---|
| 19 | normalizes configured trusted native users once at connection setup | open |
| 31 | normalizes only bounded authority for a trusted connection | open |
| 63 | accepts exact authority limits and rejects the next byte or scope | open |
| 90 | rejects malformed or conflicting trusted authority instead of dropping it | open |

## `ferricstore_server/test/ferricstore_server/native/resource_budget_test.exs`

| Line | Test | Status |
|---:|---|---|
| 6 | enforces limits across owners and reclaims every lease when an owner exits | open |
| 37 | indexes leases and waiters by owner for bounded process-down cleanup | open |
| 109 | resizing a byte lease is atomic and cannot cross the global ceiling | open |
| 127 | scoped leases enforce limits and release exactly once | open |
| 164 | scoped leases cannot be released by another process | open |
| 176 | scoped leases are reclaimed when their owner exits | open |
| 202 | a saturated scoped acquire reclaims a dead owner before returning busy | open |
| 228 | monitored owner death reclaims all its scoped resources and preserves other owners | open |
| 261 | scoped lease accounting does not use the coordinator mailbox after caller registration | open |
| 284 | stale registry handles are rejected after the coordinator is killed | open |
| 303 | non-waiting accounting does not block on the coordinator mailbox | open |
| 344 | inbound buffers share one byte ceiling across connections | open |
| 373 | waiters are admitted when release creates capacity without polling | open |
| 406 | dead waiters cannot leave an unbounded saturated FIFO | open |
| 431 | parallel fast-path acquisitions never oversubscribe the configured limit | open |
| 477 | killed scoped accounting cannot strand capacity or erase a live owner's charge | open |
| 501 | reclaim repairs a killed accounting worker whose DOWN is still queued | open |
| 529 | killed transferable lease accounting cannot strand byte reservations | open |
| 564 | idle accounting sweeps do not scan registered inactive callers | open |

## `ferricstore_server/test/ferricstore_server/native/route_metadata_test.exs`

| Line | Test | Status |
|---:|---|---|
| 6 | local endpoint uses explicit advertised host | open |
| 17 | remote fallback endpoint does not reuse local advertised host | open |
| 31 | unknown leader metadata is explicit and does not advertise local endpoint as leader | open |

## `ferricstore_server/test/ferricstore_server/native/session_current_contract_test.exs`

| Line | Test | Status |
|---:|---|---|
| 7 | native sessions expose only the prepared-command parser | open |
| 14 | prepared-command parsing rejects structured argument values without raising | open |

## `ferricstore_server/test/ferricstore_server/native/session_retained_budget_test.exs`

| Line | Test | Status |
|---:|---|---|
| 8 | MULTI queues enforce one global retained-byte ceiling across sessions | open |
| 43 | WATCH and MULTI share a lease that shrinks and releases with session state | open |
| 79 | WATCH rejects global exhaustion before submitting a Raft command | open |
