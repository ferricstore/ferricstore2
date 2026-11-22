# Flow test audit: steps 11-15

The Elixir Flow tests behind ADR-014 steps 11 to 15 (queries, schedules,
effects and governance), from the pinned Elixir tree
(`ferricstore-slice-baseline`, `apps/ferricstore/test/ferricstore`). Each
row is one test; its status says where the node covers it, which gap
fixed it, or why it is Elixir-internal. Gaps continue the numbering of
`workflow-flow-test-audit.md` (G39 on).

Status (22 Nov): complete. 1,308 rows: 160 ported, 26 fixed by node
changes (gaps G39-G54), 98 covered by existing node tests, the rest
Elixir-internal (its query engine, storage, caches, catalogs, codecs,
telemetry and in-process API). No row is open.

## Gaps

- G39 (closed): `FLOW.QUERY` had no text form (`COMMAND_EXEC`); the
  node refused it as not yet supported. It now reads Elixir's text form,
  `version query [name value ...]` (pairs, at most 64, unique names, with
  Elixir's texts), decodes each value by its field's declared type (an
  integer from its digits, all else as text, as `Binder.bind_text`), and
  names missing parameters in the failure's detail, hint and context.
- G40 (closed): a Flow created in a terminal state took its retention
  from its record time, so with a client clock in the past
  (`now_ms: 5000`) its removal time was decades ago and the retention
  sweep removed it at once: reads, queries and counts never saw it. Its
  retention now starts at the command's time, never before the shard's
  clock, as G13 made it for Flows that finish later.
- G41 (closed): `FLOW.QUERY.INDEXES` looked an id up without checking
  it, so a malformed one answered "not found"; it is checked first (1 to
  64 bytes of letters, digits, `_-:.`), with Elixir's "invalid query
  index id" text.
- G42 (closed): the schedule commands silently ignored a Flow option they
  do not take (`worker` on CREATE, `payload` on PAUSE). Each now takes
  only its own options, as Elixir's `validate_option_fields/3`: a known
  Flow option is refused with "ERR unsupported flow schedule option X",
  a name no command knows keeps the wire's "unknown flow option" text,
  and GET keeps accepting (and ignoring) Elixir's `partition_key`,
  `payload` and `payload_max_bytes`.
- G43 (closed): operator changes to a schedule had no
  `expected_state`/`expected_version` guards, so a stale pause, fire or
  delete applied over a newer change. CREATE with `overwrite`, FIRE,
  PAUSE, RESUME and DELETE now take them, checked in the shard against the
  stored state and version ("ERR flow schedule changed concurrently");
  a guarded overwrite needs the schedule to exist. Elixir reaches these
  through its API and dashboard; the node's only API is the wire, so the
  wire takes them.
- G44 (closed): a schedule target was not bounded: a 9 KB inline payload
  or values map was stored in the definition, and a list target answered
  "target is required". Inline payload and values are now held to 8 KiB
  encoded ("too large; use payload_ref/value_refs"), and a non-empty list
  target is refused as not a keyword list or map, as Elixir's.
- G45 (closed): public Flow commands could reach a schedule's internal
  Flow: `FLOW.GET` on `__ferricstore_schedule__:<id>` in its internal
  partition returned the record, and REWIND, SPAWN_CHILDREN (as parent)
  and batch items were not refused by name. As Elixir's
  `Internal.reject_reserved_id/2`, they are now refused with "ERR flow id
  is reserved for internal use" (a batch whole); creates and single
  mutations were already refused, and the schedule's history stays
  readable.
- G46 (closed): an overwrite was refused while any lease was on the
  schedule, even one its fire had long lost, with its own text. As
  Elixir's, a lease blocks an overwrite only while the node's clock is
  before its deadline (a caller's `now_ms` cannot expire it), with "ERR
  flow schedule is currently leased".
- G47 (closed): a `queue_after_previous` schedule backing off behind a
  running target waited out its backoff (up to 30 s) after the target
  finished. As Elixir's `notify_target_terminal`/`wake_queued`, a
  scheduled target that turns terminal now wakes its schedule once the
  round is durable: the schedule's shard pulls the queued occurrence to
  its clock if the schedule is active and still waits on that target, and
  the next poll fires it.
- G48 (closed): schedules wrote no lifecycle events. As Elixir's, the
  schedule's own history now carries `schedule_created`,
  `schedule_fired`, `schedule_skipped_overlap`, `schedule_failed_overlap`,
  `schedule_failed_planning` and `schedule_deleted` signals.
- G49 (closed): a manual fire leased the schedule on the caller's clock,
  so near the exact-integer limit the lease deadline passed it and the
  fire failed with a misleading "definition too large". Manual fires
  lease on the node's clock, as Elixir's state machine does; automatic
  fires keep their command time.
- G50 (closed): cron planning converted through `jiff::Timestamp`, whose
  range ends at 9999-12-30T22:00Z, so the calendar's last day refused
  with "outside supported calendar range". Instants are now counted on
  the civil calendar up to 9999-12-31T23:59:59.999Z, Elixir's bound.
- G51 (closed): `FLOW.SCHEDULE.FIRE_DUE` ignored a shard whose claim
  failed and answered as if it had nothing due. As Elixir's dispatcher, a
  failed claim ends the claiming: before anything was claimed it fails
  the command, after it the summary reports it as `claim_error` beside
  the outcomes.
- G52 (closed): the summary's `coalesced` total could pass 2^53 (two
  schedules each far behind reported 2·(2^53−2)). It is held to the
  exact-integer limit, as Elixir's `Summary`.
- G53 (closed): `FLOW.APPROVAL.LIST` answered an empty list for a
  `flow_id` or `scope` filter longer than a key can be, while
  `FLOW.BUDGET.LIST` (and Elixir, for both) refuse it with "ERR key too
  large (max 65535 bytes)". The approval listing now checks its filters
  as the approval request checks its dimensions.
- G54 (closed): extending a governed Flow's lease left its credit lease
  to expire, so a long-running Flow lost the credit it held. As Elixir's
  `maybe_renew_governance_limit/2`, an extension now renews the credit
  lease to at least the command time plus the new lease length (an
  internal `Op::Renew` on the limit, sent once the extension is durable,
  as credit releases are). Recorded decision: Elixir renews first and
  refuses the extension when the credit lease has already lapsed; the
  node extends and the renewal finds no lease, since the shard holding
  the Flow cannot consult the limit's shard within one step.

## Recorded decisions

- Type-level governance (`FLOW.POLICY.SET` `governance` and `version` at
  the top level) is Elixir's in-process API only: its wire refuses both
  as unknown options, and an effect's governance resolves from the
  type's policy and the state `running`, which takes no state policy. So
  over the wire Elixir applies only the default effect rule (an
  idempotency key is required), as the node does; its policy-driven
  effect rules (allowed and denied effect types, required approvals,
  effect-driven circuits with latency, failure-rate and half-open
  tracking, telemetry) are not ported. Building them would add wire
  options Elixir lacks and cross-shard circuit state.

## `flow/governance_argument_errors_test.exs`

| Line | Test | Status |
|---|---|---|
| 8 | governance calls name the bad argument | internal: Elixir's API argument types; on the wire a non-binary id or scope is the native layer's refusal first (`governance::tests::the_id_is_checked_by_the_native_layer_then_the_store`) |

## `flow/index_query_test.exs`

| Line | Test | Status |
|---|---|---|
| 18 | filtering is false when index query has no filters | internal: Elixir's index query helpers; the node evaluates FLOW.QUERY in memory (ADR-014 step 12) |
| 22 | fetch_count only asks scan count for filtered queries | internal: Elixir's index query helpers; the node evaluates FLOW.QUERY in memory (ADR-014 step 12) |
| 29 | filter_records matches index value state terminal flag and time range | internal: Elixir's index query helpers; the node evaluates FLOW.QUERY in memory (ADR-014 step 12) |
| 58 | record_matches supports explicit state filter | internal: Elixir's index query helpers; the node evaluates FLOW.QUERY in memory (ADR-014 step 12) |
| 74 | cursor tie-breaking filters records and RAM index entries by timestamp and id | internal: Elixir's index query helpers; the node evaluates FLOW.QUERY in memory (ADR-014 step 12) |

## `flow/query/admission_controller_test.exs`

| Line | Test | Status |
|---|---|---|
| 7 | derives the default query pool from the node memory limit | internal: Elixir's index registry, planner and admission (admission_controller); the node evaluates FLOW.QUERY in memory from its shard catalogs, with no persisted indexes to build, plan over or admit |
| 26 | enforces independent scope and node memory ceilings | internal: Elixir's index registry, planner and admission (admission_controller); the node evaluates FLOW.QUERY in memory from its shard catalogs, with no persisted indexes to build, plan over or admit |
| 51 | resizes lease memory atomically without weakening ownership | internal: Elixir's index registry, planner and admission (admission_controller); the node evaluates FLOW.QUERY in memory from its shard catalogs, with no persisted indexes to build, plan over or admit |
| 88 | owner death releases the lease memory reservation | internal: Elixir's index registry, planner and admission (admission_controller); the node evaluates FLOW.QUERY in memory from its shard catalogs, with no persisted indexes to build, plan over or admit |
| 122 | cancels an acquisition committed after its caller times out | internal: Elixir's index registry, planner and admission (admission_controller); the node evaluates FLOW.QUERY in memory from its shard catalogs, with no persisted indexes to build, plan over or admit |
| 181 | rejects invalid memory reservations before admission | internal: Elixir's index registry, planner and admission (admission_controller); the node evaluates FLOW.QUERY in memory from its shard catalogs, with no persisted indexes to build, plan over or admit |
| 192 | enforces independent scope and node concurrency ceilings | internal: Elixir's index registry, planner and admission (admission_controller); the node evaluates FLOW.QUERY in memory from its shard catalogs, with no persisted indexes to build, plan over or admit |
| 213 | reports active leases per instance for lifecycle draining | internal: Elixir's index registry, planner and admission (admission_controller); the node evaluates FLOW.QUERY in memory from its shard catalogs, with no persisted indexes to build, plan over or admit |
| 225 | fails drain checks closed during the orphan-lease restart grace | internal: Elixir's index registry, planner and admission (admission_controller); the node evaluates FLOW.QUERY in memory from its shard catalogs, with no persisted indexes to build, plan over or admit |
| 246 | fences and drains only leases pinned to the retiring index | internal: Elixir's index registry, planner and admission (admission_controller); the node evaluates FLOW.QUERY in memory from its shard catalogs, with no persisted indexes to build, plan over or admit |
| 275 | automatically releases a lease when its owner exits | internal: Elixir's index registry, planner and admission (admission_controller); the node evaluates FLOW.QUERY in memory from its shard catalogs, with no persisted indexes to build, plan over or admit |
| 303 | only the owner can release a live lease | internal: Elixir's index registry, planner and admission (admission_controller); the node evaluates FLOW.QUERY in memory from its shard catalogs, with no persisted indexes to build, plan over or admit |
| 317 | with_permit releases on returned errors and exceptions | internal: Elixir's index registry, planner and admission (admission_controller); the node evaluates FLOW.QUERY in memory from its shard catalogs, with no persisted indexes to build, plan over or admit |
| 333 | controller shutdown during admitted work does not replace the work result | internal: Elixir's index registry, planner and admission (admission_controller); the node evaluates FLOW.QUERY in memory from its shard catalogs, with no persisted indexes to build, plan over or admit |
| 345 | an unavailable controller fails closed without running admitted work | internal: Elixir's index registry, planner and admission (admission_controller); the node evaluates FLOW.QUERY in memory from its shard catalogs, with no persisted indexes to build, plan over or admit |
| 358 | stores only an internal digest of the scope | internal: Elixir's index registry, planner and admission (admission_controller); the node evaluates FLOW.QUERY in memory from its shard catalogs, with no persisted indexes to build, plan over or admit |
| 372 | rejects an oversized scope before hashing or admission | internal: Elixir's index registry, planner and admission (admission_controller); the node evaluates FLOW.QUERY in memory from its shard catalogs, with no persisted indexes to build, plan over or admit |
| 382 | rejects index identities outside the hashed unsigned 64-bit contract | internal: Elixir's index registry, planner and admission (admission_controller); the node evaluates FLOW.QUERY in memory from its shard catalogs, with no persisted indexes to build, plan over or admit |
| 398 | rejects a new pin when the exact index build is no longer active | internal: Elixir's index registry, planner and admission (admission_controller); the node evaluates FLOW.QUERY in memory from its shard catalogs, with no persisted indexes to build, plan over or admit |

## `flow/query/backfill_source_test.exs`

| Line | Test | Status |
|---|---|---|
| 22 | snapshots long state keys behind fixed-size staging keys and pages current records | internal: Elixir's persisted query engine (backfill_source); the node evaluates FLOW.QUERY in memory from its shard catalogs (ADR-014 step 12) |
| 95 | pages small records past the worst-case value size in bounded primary reads | internal: Elixir's persisted query engine (backfill_source); the node evaluates FLOW.QUERY in memory from its shard catalogs (ADR-014 step 12) |
| 124 | ends a page at the byte budget and resumes after its last hydrated record | internal: Elixir's persisted query engine (backfill_source); the node evaluates FLOW.QUERY in memory from its shard catalogs (ADR-014 step 12) |
| 161 | keeps each primary read within the shard reader's key limits | internal: Elixir's persisted query engine (backfill_source); the node evaluates FLOW.QUERY in memory from its shard catalogs (ADR-014 step 12) |
| 195 | resumes a durable source-catalog cursor without a live keydir | internal: Elixir's persisted query engine (backfill_source); the node evaluates FLOW.QUERY in memory from its shard catalogs (ADR-014 step 12) |
| 216 | cleanup removes at most one bounded staging page per call | internal: Elixir's persisted query engine (backfill_source); the node evaluates FLOW.QUERY in memory from its shard catalogs (ADR-014 step 12) |
| 236 | bounds collision reads by the snapshot byte budget | internal: Elixir's persisted query engine (backfill_source); the node evaluates FLOW.QUERY in memory from its shard catalogs (ADR-014 step 12) |
| 250 | snapshot cannot overwrite a staging row replaced after collision validation | internal: Elixir's persisted query engine (backfill_source); the node evaluates FLOW.QUERY in memory from its shard catalogs (ADR-014 step 12) |
| 273 | rejects a corrupt durable snapshot completion marker | internal: Elixir's persisted query engine (backfill_source); the node evaluates FLOW.QUERY in memory from its shard catalogs (ADR-014 step 12) |
| 286 | rejects malformed decoded record ownership without raising | internal: Elixir's persisted query engine (backfill_source); the node evaluates FLOW.QUERY in memory from its shard catalogs (ADR-014 step 12) |
| 301 | binds shared source records to their sealed physical scope | internal: Elixir's persisted query engine (backfill_source); the node evaluates FLOW.QUERY in memory from its shard catalogs (ADR-014 step 12) |
| 332 | rejects staging cursors that exceed the LMDB key boundary | internal: Elixir's persisted query engine (backfill_source); the node evaluates FLOW.QUERY in memory from its shard catalogs (ADR-014 step 12) |
| 340 | returns an explicit tombstone when a staged state was concurrently deleted | internal: Elixir's persisted query engine (backfill_source); the node evaluates FLOW.QUERY in memory from its shard catalogs (ADR-014 step 12) |
| 364 | hydrates a retained terminal query row when the live state key is absent | internal: Elixir's persisted query engine (backfill_source); the node evaluates FLOW.QUERY in memory from its shard catalogs (ADR-014 step 12) |
| 397 | does not resurrect a retained nonterminal row after its live state disappears | internal: Elixir's persisted query engine (backfill_source); the node evaluates FLOW.QUERY in memory from its shard catalogs (ADR-014 step 12) |
| 429 | preserves the primary storage expiry for active records | internal: Elixir's persisted query engine (backfill_source); the node evaluates FLOW.QUERY in memory from its shard catalogs (ADR-014 step 12) |
| 447 | backfills a cold-parked scheduled row without reading its WAL payload | internal: Elixir's persisted query engine (backfill_source); the node evaluates FLOW.QUERY in memory from its shard catalogs (ADR-014 step 12) |
| 527 | mismatched or corrupt parked metadata is not treated as deletion | internal: Elixir's persisted query engine (backfill_source); the node evaluates FLOW.QUERY in memory from its shard catalogs (ADR-014 step 12) |

## `flow/query/binder_test.exs`

| Line | Test | Status |
|---|---|---|
| 7 | binds every value in a bounded collection request with exact types | ported (`query::tests::evaluates_equality_in_time_windows_and_metadata_predicates` binds every value of a collection request) |
| 46 | preserves the validated return projection while binding values | ported (`query::tests::projects_selected_fields_after_filtering_ordering_and_limiting`) |
| 62 | binds repeated parameters once and rejects missing, unused, or mistyped values | ported (`query::validation_tests::binds_repeated_parameters_once_and_rejects_missing_unused_or_mistyped`) |
| 85 | preserves parameter key validation precedence around map membership | internal: non-string parameter keys have no wire form |
| 102 | diagnostic binding names missing parameters without exposing values | fixed (`query::text_tests::a_missing_parameter_is_named_in_the_context`; G39) |
| 117 | ordinary dynamic comparisons cannot bind null or unsupported compound values | ported (`query::validation_tests::dynamic_comparisons_cannot_bind_null_or_compound_values`) |
| 135 | text binding decodes only the type declared by the query field | fixed (`query::text_tests::decodes_each_value_by_the_type_its_field_declares`; G39) |
| 174 | binds the cursor as an exact bounded keyword parameter | ported (`query::validation_tests::binds_the_cursor_as_a_bounded_keyword`) |

## `flow/query/budget_test.exs`

| Line | Test | Status |
|---|---|---|
| 8 | rejects budgets above the immutable execution ceilings | internal: Elixir's persisted query engine (budget); the node evaluates FLOW.QUERY in memory from its shard catalogs (ADR-014 step 12) |

## `flow/query/builder_test.exs`

| Line | Test | Status |
|---|---|---|
| 7 | any strings are literal predicates while atom any explicitly omits optional predicates | internal: a client-side FQL builder; the SDKs carry their own, whose text the ports use |
| 35 | builds every retired record-read shape as a bound FQL request | internal: a client-side FQL builder; the SDKs carry their own, whose text the ports use |
| 61 | uses parameters for values and validates dynamic metadata fields | internal: a client-side FQL builder; the SDKs carry their own, whose text the ports use |
| 103 | builds null predicates without inventing a scalar parameter | internal: a client-side FQL builder; the SDKs carry their own, whose text the ports use |
| 126 | binds lifecycle state and workflow step as independent predicates | internal: a client-side FQL builder; the SDKs carry their own, whose text the ports use |
| 142 | builds bounded return projections with safely quoted dynamic fields | internal: a client-side FQL builder; the SDKs carry their own, whose text the ports use |
| 158 | rejects invalid, duplicate, and oversized return projections | internal: a client-side FQL builder; the SDKs carry their own, whose text the ports use |
| 176 | preserves the list default state without widening the index scan | internal: a client-side FQL builder; the SDKs carry their own, whose text the ports use |
| 184 | applies stuck time filters to the expired lease deadline window | internal: a client-side FQL builder; the SDKs carry their own, whose text the ports use |
| 205 | defaults and validates the stuck lease deadline window | internal: a client-side FQL builder; the SDKs carry their own, whose text the ports use |
| 225 | preserves typed metadata values as bound parameters | internal: a client-side FQL builder; the SDKs carry their own, whose text the ports use |
| 240 | rejects unscoped, unbounded, and incomplete collection requests | internal: a client-side FQL builder; the SDKs carry their own, whose text the ports use |

## `flow/query/commands_test.exs`

| Line | Test | Status |
|---|---|---|
| 7 | declares one administrative, read-only, keyless management command | internal: Elixir's command table entry |
| 21 | returns actionable usage and availability errors | fixed (`query::text_tests::index_ids_are_checked_before_their_lookup`; G41; its registry-unavailable case has no counterpart) |
| 38 | the default OSS command prepares as an extension instead of a static native AST | internal: Elixir's command extension dispatch |
| 58 | does not claim unrelated extension commands | internal: as line 38 |

## `flow/query/composite_backfill_test.exs`

| Line | Test | Status |
|---|---|---|
| 36 | projects a bounded record page for every definition in one LMDB transaction | internal: Elixir's persisted query engine (composite_backfill); the node evaluates FLOW.QUERY in memory from its shard catalogs (ADR-014 step 12) |
| 102 | backfilling a new definition preserves existing index and counter projections | internal: Elixir's persisted query engine (composite_backfill); the node evaluates FLOW.QUERY in memory from its shard catalogs (ADR-014 step 12) |
| 209 | projects a 256-record page and reuses existing reverse ownership | internal: Elixir's persisted query engine (composite_backfill); the node evaluates FLOW.QUERY in memory from its shard catalogs (ADR-014 step 12) |
| 237 | verifies long state keys in reads within the shard reader's key limits | internal: Elixir's persisted query engine (composite_backfill); the node evaluates FLOW.QUERY in memory from its shard catalogs (ADR-014 step 12) |
| 271 | backfill writes byte-identical dynamic covering entries to live derivation | internal: Elixir's persisted query engine (composite_backfill); the node evaluates FLOW.QUERY in memory from its shard catalogs (ADR-014 step 12) |
| 342 | backfill projects a retained terminal record with the live writer's expiry | internal: Elixir's persisted query engine (composite_backfill); the node evaluates FLOW.QUERY in memory from its shard catalogs (ADR-014 step 12) |
| 413 | rejects an oversized page before allocating projection state | internal: Elixir's persisted query engine (composite_backfill); the node evaluates FLOW.QUERY in memory from its shard catalogs (ADR-014 step 12) |
| 420 | retries and then removes a record deleted during backfill | internal: Elixir's persisted query engine (composite_backfill); the node evaluates FLOW.QUERY in memory from its shard catalogs (ADR-014 step 12) |
| 456 | atomically projects a retained terminal row when the live primary is absent | internal: Elixir's persisted query engine (composite_backfill); the node evaluates FLOW.QUERY in memory from its shard catalogs (ADR-014 step 12) |
| 484 | retries when a retained row changes before its projection transaction | internal: Elixir's persisted query engine (composite_backfill); the node evaluates FLOW.QUERY in memory from its shard catalogs (ADR-014 step 12) |
| 503 | rejects an invalid retained-row projection guard | internal: Elixir's persisted query engine (composite_backfill); the node evaluates FLOW.QUERY in memory from its shard catalogs (ADR-014 step 12) |
| 520 | detects an expiry-only concurrent change after projection | internal: Elixir's persisted query engine (composite_backfill); the node evaluates FLOW.QUERY in memory from its shard catalogs (ADR-014 step 12) |
| 533 | shared backfill verifies the sealed owner after projection | internal: Elixir's persisted query engine (composite_backfill); the node evaluates FLOW.QUERY in memory from its shard catalogs (ADR-014 step 12) |
| 582 | rejects a forged definition before writing its namespace | internal: Elixir's persisted query engine (composite_backfill); the node evaluates FLOW.QUERY in memory from its shard catalogs (ADR-014 step 12) |
| 596 | rejects aggregate projection operations above the bounded page budget | internal: Elixir's persisted query engine (composite_backfill); the node evaluates FLOW.QUERY in memory from its shard catalogs (ADR-014 step 12) |

## `flow/query/composite_counter_test.exs`

| Line | Test | Status |
|---|---|---|
| 7 | counter values bind the full logical prefix and reject digest collisions | internal: Elixir's persisted query engine (composite_counter); the node evaluates FLOW.QUERY in memory from its shard catalogs (ADR-014 step 12) |
| 23 | counter values track the bounded subset that can expire | internal: Elixir's persisted query engine (composite_counter); the node evaluates FLOW.QUERY in memory from its shard catalogs (ADR-014 step 12) |
| 38 | counter values can bind logical membership to a larger physical fanout | internal: Elixir's persisted query engine (composite_counter); the node evaluates FLOW.QUERY in memory from its shard catalogs (ADR-014 step 12) |
| 51 | decodes a storage row only when its key, prefix, and definition agree | internal: Elixir's persisted query engine (composite_counter); the node evaluates FLOW.QUERY in memory from its shard catalogs (ADR-014 step 12) |
| 74 | derives declared prefixes for one physical key in stable prefix order | internal: Elixir's persisted query engine (composite_counter); the node evaluates FLOW.QUERY in memory from its shard catalogs (ADR-014 step 12) |
| 96 | derives one run counter prefix per declared dimension despite trailing fanout | internal: Elixir's persisted query engine (composite_counter); the node evaluates FLOW.QUERY in memory from its shard catalogs (ADR-014 step 12) |
| 130 | reads declared prefixes in one bounded batch with exact storage accounting | internal: Elixir's persisted query engine (composite_counter); the node evaluates FLOW.QUERY in memory from its shard catalogs (ADR-014 step 12) |
| 164 | bounded reads reject oversized, duplicate, undeclared, and mismatched counters | internal: Elixir's persisted query engine (composite_counter); the node evaluates FLOW.QUERY in memory from its shard catalogs (ADR-014 step 12) |

## `flow/query/composite_index_test.exs`

| Line | Test | Status |
|---|---|---|
| 34 | projects a tenant-isolated, ordered, bounded LMDB entry | internal: Elixir's persisted query engine (composite_index); the node evaluates FLOW.QUERY in memory from its shard catalogs (ADR-014 step 12) |
| 88 | entry values use the bounded compact codec and reject the pre-beta ETF shape | internal: Elixir's persisted query engine (composite_index); the node evaluates FLOW.QUERY in memory from its shard catalogs (ADR-014 step 12) |
| 124 | covering definitions persist a bounded authoritative record projection | internal: Elixir's persisted query engine (composite_index); the node evaluates FLOW.QUERY in memory from its shard catalogs (ADR-014 step 12) |
| 170 | plain and covering entry values reject single-bit corruption | internal: Elixir's persisted query engine (composite_index); the node evaluates FLOW.QUERY in memory from its shard catalogs (ADR-014 step 12) |
| 199 | covering entries preserve dynamic field identities and values | internal: Elixir's persisted query engine (composite_index); the node evaluates FLOW.QUERY in memory from its shard catalogs (ADR-014 step 12) |
| 239 | dynamic index values use the authoritative scalar and attribute-list domain | internal: Elixir's persisted query engine (composite_index); the node evaluates FLOW.QUERY in memory from its shard catalogs (ADR-014 step 12) |
| 329 | shared layout derives its hidden prefix from sealed metadata, not index values | internal: Elixir's persisted query engine (composite_index); the node evaluates FLOW.QUERY in memory from its shard catalogs (ADR-014 step 12) |
| 401 | supports valid long Flow IDs without expanding physical index keys | internal: Elixir's persisted query engine (composite_index); the node evaluates FLOW.QUERY in memory from its shard catalogs (ADR-014 step 12) |
| 418 | rejects a state locator that does not own the projected record | internal: Elixir's persisted query engine (composite_index); the node evaluates FLOW.QUERY in memory from its shard catalogs (ADR-014 step 12) |
| 432 | descending ordered fields sort newer records first | internal: Elixir's persisted query engine (composite_index); the node evaluates FLOW.QUERY in memory from its shard catalogs (ADR-014 step 12) |
| 452 | a canonical multivalue attribute emits one entry per member | internal: Elixir's persisted query engine (composite_index); the node evaluates FLOW.QUERY in memory from its shard catalogs (ADR-014 step 12) |
| 489 | bounds the cartesian fanout of multiple multivalue attributes before materialization | internal: Elixir's persisted query engine (composite_index); the node evaluates FLOW.QUERY in memory from its shard catalogs (ADR-014 step 12) |
| 536 | publishes the projection cardinality ceiling for bounded lifecycle planning | internal: Elixir's persisted query engine (composite_index); the node evaluates FLOW.QUERY in memory from its shard catalogs (ADR-014 step 12) |
| 540 | missing and explicit null components are distinct | internal: Elixir's persisted query engine (composite_index); the node evaluates FLOW.QUERY in memory from its shard catalogs (ADR-014 step 12) |
| 575 | rejects values that do not match a built-in field type | internal: Elixir's persisted query engine (composite_index); the node evaluates FLOW.QUERY in memory from its shard catalogs (ADR-014 step 12) |
| 593 | rejects forged definitions before deriving physical keys | internal: Elixir's persisted query engine (composite_index); the node evaluates FLOW.QUERY in memory from its shard catalogs (ADR-014 step 12) |
| 612 | entry identity binds the projected row to its key | internal: Elixir's persisted query engine (composite_index); the node evaluates FLOW.QUERY in memory from its shard catalogs (ADR-014 step 12) |
| 625 | rejects record versions outside the authoritative record codec domain | internal: Elixir's persisted query engine (composite_index); the node evaluates FLOW.QUERY in memory from its shard catalogs (ADR-014 step 12) |
| 653 | rejects oversized entry identities before encoding or persisted lookup | internal: Elixir's persisted query engine (composite_index); the node evaluates FLOW.QUERY in memory from its shard catalogs (ADR-014 step 12) |
| 697 | unscoped or malformed records fail without creating a shared tenant range | internal: Elixir's persisted query engine (composite_index); the node evaluates FLOW.QUERY in memory from its shard catalogs (ADR-014 step 12) |
| 712 | reverse metadata for the built-in indexes is compact and v1 rows still decode | internal: Elixir's persisted query engine (composite_index); the node evaluates FLOW.QUERY in memory from its shard catalogs (ADR-014 step 12) |
| 766 | reverse keys outside the composite key layout keep the v1 encoding | internal: Elixir's persisted query engine (composite_index); the node evaluates FLOW.QUERY in memory from its shard catalogs (ADR-014 step 12) |
| 781 | reverse metadata is bounded and validates ownership | internal: Elixir's persisted query engine (composite_index); the node evaluates FLOW.QUERY in memory from its shard catalogs (ADR-014 step 12) |
| 835 | rejects oversized persisted terms before external-term allocation | internal: Elixir's persisted query engine (composite_index); the node evaluates FLOW.QUERY in memory from its shard catalogs (ADR-014 step 12) |

## `flow/query/composite_projection_integration_test.exs`

| Line | Test | Status |
|---|---|---|
| 29 | LMDB writer snapshots definitions once and projects the final record version | internal: Elixir's persisted query engine (composite_projection_integration); the node evaluates FLOW.QUERY in memory from its shard catalogs (ADR-014 step 12) |
| 81 | disabled providers add no composite read or write operations | internal: Elixir's persisted query engine (composite_projection_integration); the node evaluates FLOW.QUERY in memory from its shard catalogs (ADR-014 step 12) |
| 107 | query-only projection writes hydration and composite rows without lifecycle indexes | internal: Elixir's persisted query engine (composite_projection_integration); the node evaluates FLOW.QUERY in memory from its shard catalogs (ADR-014 step 12) |
| 155 | writer coalescing keeps the highest record version regardless of arrival order | internal: Elixir's persisted query engine (composite_projection_integration); the node evaluates FLOW.QUERY in memory from its shard catalogs (ADR-014 step 12) |
| 204 | composite projection hydrates records whose logical state key exceeds LMDB limits | internal: Elixir's persisted query engine (composite_projection_integration); the node evaluates FLOW.QUERY in memory from its shard catalogs (ADR-014 step 12) |
| 253 | writer chunks never split one composite projection transaction | internal: Elixir's persisted query engine (composite_projection_integration); the node evaluates FLOW.QUERY in memory from its shard catalogs (ADR-014 step 12) |
| 338 | writer re-expands a projection after a concurrent reverse compare conflict | internal: Elixir's persisted query engine (composite_projection_integration); the node evaluates FLOW.QUERY in memory from its shard catalogs (ADR-014 step 12) |
| 442 | writer re-expands after a backfill commit makes its cached counter stale | internal: Elixir's persisted query engine (composite_projection_integration); the node evaluates FLOW.QUERY in memory from its shard catalogs (ADR-014 step 12) |
| 582 | writer does not retry an undecodable composite counter | internal: Elixir's persisted query engine (composite_projection_integration); the node evaluates FLOW.QUERY in memory from its shard catalogs (ADR-014 step 12) |
| 667 | deleted source atomically removes every projection derived from the old query row | internal: Elixir's persisted query engine (composite_projection_integration); the node evaluates FLOW.QUERY in memory from its shard catalogs (ADR-014 step 12) |

## `flow/query/composite_projection_test.exs`

| Line | Test | Status |
|---|---|---|
| 33 | atomically replaces stale entry keys and reverse ownership | internal: Elixir's persisted query engine (composite_projection); the node evaluates FLOW.QUERY in memory from its shard catalogs (ADR-014 step 12) |
| 75 | multiple updates in one LMDB transaction use the pending reverse value | internal: Elixir's persisted query engine (composite_projection); the node evaluates FLOW.QUERY in memory from its shard catalogs (ADR-014 step 12) |
| 107 | bounded reverse prefetch preserves found and missing ownership | internal: Elixir's persisted query engine (composite_projection); the node evaluates FLOW.QUERY in memory from its shard catalogs (ADR-014 step 12) |
| 140 | bounded reverse prefetch rejects corrupt rows and oversized requests | internal: Elixir's persisted query engine (composite_projection); the node evaluates FLOW.QUERY in memory from its shard catalogs (ADR-014 step 12) |
| 167 | compare guards reject a concurrent reverse replacement | internal: Elixir's persisted query engine (composite_projection); the node evaluates FLOW.QUERY in memory from its shard catalogs (ADR-014 step 12) |
| 193 | record deletion removes every owned entry | internal: Elixir's persisted query engine (composite_projection); the node evaluates FLOW.QUERY in memory from its shard catalogs (ADR-014 step 12) |
| 224 | multiple records in one write batch compose guarded counter deltas | internal: Elixir's persisted query engine (composite_projection); the node evaluates FLOW.QUERY in memory from its shard catalogs (ADR-014 step 12) |
| 255 | tracks physical fanout separately from logical counter membership | internal: Elixir's persisted query engine (composite_projection); the node evaluates FLOW.QUERY in memory from its shard catalogs (ADR-014 step 12) |
| 303 | tracks expiring membership across TTL changes without changing the total | internal: Elixir's persisted query engine (composite_projection); the node evaluates FLOW.QUERY in memory from its shard catalogs (ADR-014 step 12) |
| 337 | fails closed when reverse ownership would underflow a missing counter | internal: Elixir's persisted query engine (composite_projection); the node evaluates FLOW.QUERY in memory from its shard catalogs (ADR-014 step 12) |
| 369 | counter compare guards reject a concurrent projection without partial index writes | internal: Elixir's persisted query engine (composite_projection); the node evaluates FLOW.QUERY in memory from its shard catalogs (ADR-014 step 12) |

## `flow/query/composite_range_test.exs`

| Line | Test | Status |
|---|---|---|
| 36 | reads one tenant-isolated half-open descending time range | internal: Elixir's persisted query engine (composite_range); the node evaluates FLOW.QUERY in memory from its shard catalogs (ADR-014 step 12) |
| 76 | resumes at an exact entry boundary without duplicates | internal: Elixir's persisted query engine (composite_range); the node evaluates FLOW.QUERY in memory from its shard catalogs (ADR-014 step 12) |
| 100 | fused LMDB range returns validated source rows and exact byte accounting | internal: Elixir's persisted query engine (composite_range); the node evaluates FLOW.QUERY in memory from its shard catalogs (ADR-014 step 12) |
| 156 | single LMDB snapshot returns composite entries with bounded query rows | internal: Elixir's persisted query engine (composite_range); the node evaluates FLOW.QUERY in memory from its shard catalogs (ADR-014 step 12) |
| 236 | fused lookup rejects a composite row outside the expected state-key scope | internal: Elixir's persisted query engine (composite_range); the node evaluates FLOW.QUERY in memory from its shard catalogs (ADR-014 step 12) |
| 273 | fused reader keeps query rows aligned across cursor pages | internal: Elixir's persisted query engine (composite_range); the node evaluates FLOW.QUERY in memory from its shard catalogs (ADR-014 step 12) |
| 337 | fused QueryRow lookup uses the compact physical key for a long state key | internal: Elixir's persisted query engine (composite_range); the node evaluates FLOW.QUERY in memory from its shard catalogs (ADR-014 step 12) |
| 362 | fused LMDB range returns a validated covering record | internal: Elixir's persisted query engine (composite_range); the node evaluates FLOW.QUERY in memory from its shard catalogs (ADR-014 step 12) |
| 396 | maps inclusive bounds correctly for an ascending ordered field | internal: Elixir's persisted query engine (composite_range); the node evaluates FLOW.QUERY in memory from its shard catalogs (ADR-014 step 12) |
| 427 | rejects malformed range shapes and cursors | internal: Elixir's persisted query engine (composite_range); the node evaluates FLOW.QUERY in memory from its shard catalogs (ADR-014 step 12) |
| 466 | reader rejects a forged range outside the composite index keyspace | internal: Elixir's persisted query engine (composite_range); the node evaluates FLOW.QUERY in memory from its shard catalogs (ADR-014 step 12) |
| 493 | rejects forged definitions before constructing physical ranges | internal: Elixir's persisted query engine (composite_range); the node evaluates FLOW.QUERY in memory from its shard catalogs (ADR-014 step 12) |
| 512 | fails closed on a corrupt projected value | internal: Elixir's persisted query engine (composite_range); the node evaluates FLOW.QUERY in memory from its shard catalogs (ADR-014 step 12) |

## `flow/query/covering_codec_test.exs`

| Line | Test | Status |
|---|---|---|
| 10 | publishes one exact covering schema for every supported run builtin | internal: Elixir's persisted query engine (covering_codec); the node evaluates FLOW.QUERY in memory from its shard catalogs (ADR-014 step 12) |
| 28 | round trips dynamic attributes and state metadata without creating atoms | internal: Elixir's persisted query engine (covering_codec); the node evaluates FLOW.QUERY in memory from its shard catalogs (ADR-014 step 12) |
| 50 | preserves an unpartitioned run as an explicit null partition | internal: Elixir's persisted query engine (covering_codec); the node evaluates FLOW.QUERY in memory from its shard catalogs (ADR-014 step 12) |
| 62 | dynamic field encoding is deterministic across insertion order | internal: Elixir's persisted query engine (covering_codec); the node evaluates FLOW.QUERY in memory from its shard catalogs (ADR-014 step 12) |
| 78 | round trips every supported field and injects authoritative identity | internal: Elixir's persisted query engine (covering_codec); the node evaluates FLOW.QUERY in memory from its shard catalogs (ADR-014 step 12) |
| 106 | encoding is deterministic across map insertion order | internal: Elixir's persisted query engine (covering_codec); the node evaluates FLOW.QUERY in memory from its shard catalogs (ADR-014 step 12) |
| 122 | decoded large fields do not retain unrelated covering payload bytes | internal: Elixir's persisted query engine (covering_codec); the node evaluates FLOW.QUERY in memory from its shard catalogs (ADR-014 step 12) |
| 137 | compact payload is smaller than the previous bounded ETF envelope | internal: Elixir's persisted query engine (covering_codec); the node evaluates FLOW.QUERY in memory from its shard catalogs (ADR-014 step 12) |
| 151 | accepts exactly the authoritative dynamic metadata value domain | internal: Elixir's persisted query engine (covering_codec); the node evaluates FLOW.QUERY in memory from its shard catalogs (ADR-014 step 12) |
| 192 | rejects unsupported fields and values before persistence | internal: Elixir's persisted query engine (covering_codec); the node evaluates FLOW.QUERY in memory from its shard catalogs (ADR-014 step 12) |
| 201 | rejects truncation, trailing data, unknown bitmap bits, and invalid identity | internal: Elixir's persisted query engine (covering_codec); the node evaluates FLOW.QUERY in memory from its shard catalogs (ADR-014 step 12) |

## `flow/query/covering_projection_test.exs`

| Line | Test | Status |
|---|---|---|
| 22 | prepares field types for eligible count and sparse record projections | internal: Elixir's persisted query engine (covering_projection); the node evaluates FLOW.QUERY in memory from its shard catalogs (ADR-014 step 12) |
| 41 | rejects full records, uncovered fields, and uncovered residual predicates | internal: Elixir's persisted query engine (covering_projection); the node evaluates FLOW.QUERY in memory from its shard catalogs (ADR-014 step 12) |
| 70 | classifies only selected covering plans and flags impossible hydration | internal: Elixir's persisted query engine (covering_projection); the node evaluates FLOW.QUERY in memory from its shard catalogs (ADR-014 step 12) |

## `flow/query/cursor_key_store_test.exs`

| Line | Test | Status |
|---|---|---|
| 18 | atomically creates a private key and reloads it across restarts | internal: Elixir's persisted query engine (cursor_key_store); the node evaluates FLOW.QUERY in memory from its shard catalogs (ADR-014 step 12) |
| 34 | uses a configured shared key without writing local secret material | internal: Elixir's persisted query engine (cursor_key_store); the node evaluates FLOW.QUERY in memory from its shard catalogs (ADR-014 step 12) |
| 45 | concurrent first starts converge on the one durably published key | internal: Elixir's persisted query engine (cursor_key_store); the node evaluates FLOW.QUERY in memory from its shard catalogs (ADR-014 step 12) |
| 76 | fails closed instead of rotating corrupt or exposed persisted keys | internal: Elixir's persisted query engine (cursor_key_store); the node evaluates FLOW.QUERY in memory from its shard catalogs (ADR-014 step 12) |
| 92 | never follows a persisted key symlink | internal: Elixir's persisted query engine (cursor_key_store); the node evaluates FLOW.QUERY in memory from its shard catalogs (ADR-014 step 12) |
| 104 | rejects malformed context, key configuration, and unavailable servers | internal: Elixir's persisted query engine (cursor_key_store); the node evaluates FLOW.QUERY in memory from its shard catalogs (ADR-014 step 12) |

## `flow/query/cursor_test.exs`

| Line | Test | Status |
|---|---|---|
| 9 | issues an opaque authenticated cursor and verifies its exact binding | ported (`query::cursor::tests::a_cursor_opens_for_its_own_query_until_it_expires`) |
| 30 | opens an authenticated plan claim and binds it to the original token | internal: Elixir's index plan claims bound into cursors |
| 54 | rejects tampering and every security-sensitive binding mismatch | ported (`query::cursor::tests::a_cursor_opens_for_its_own_query_until_it_expires`: another query's digest, another key, an altered byte) |
| 83 | expires at the exact deadline and never trusts caller-controlled expiry | ported (`query::cursor::tests::a_cursor_opens_for_its_own_query_until_it_expires`: expired at the deadline) |
| 95 | fails closed for malformed and oversized inputs without raising | ported (`query::validation_tests::binds_the_cursor_as_a_bounded_keyword`) |
| 112 | rejects an oversized binding scope before encoding or hashing | internal: Elixir's cursor binding scope |
| 122 | rejects order bindings outside the canonical query contract | internal: Elixir's cursor binding order check |
| 136 | uses a fresh nonce for every cursor by default | decision: the node's cursor for one query position is deterministic (an HMAC over its query digest, expiry and position, no nonce); Elixir adds a fresh nonce to each |
| 144 | accepts the canonical event history order | internal: Elixir's cursor binding order check |

## `flow/query/documentation_contract_test.exs`

| Line | Test | Status |
|---|---|---|
| 6 | the command guide documents the complete OSS query surface | internal: checks Elixir's command guide text |

## `flow/query/engine_test.exs`

| Line | Test | Status |
|---|---|---|
| 246 | the embedded API exposes the canonical Flow query entry point | internal: Elixir's query engine installation, capability manifests, physical paths and storage reads; the node evaluates FLOW.QUERY in memory from its shard catalogs |
| 251 | the direct exact-path engine rejects an unsupported physical path | internal: Elixir's query engine installation, capability manifests, physical paths and storage reads; the node evaluates FLOW.QUERY in memory from its shard catalogs |
| 264 | direct exact-path EXPLAIN binds canonical result projections | internal: Elixir's query engine installation, capability manifests, physical paths and storage reads; the node evaluates FLOW.QUERY in memory from its shard catalogs |
| 296 | default query execution uses the bounded state/type index for collection reads | internal: Elixir's query engine installation, capability manifests, physical paths and storage reads; the node evaluates FLOW.QUERY in memory from its shard catalogs |
| 336 | bare state/time pages return public metadata from query rows without payload hydration | internal: Elixir's query engine installation, capability manifests, physical paths and storage reads; the node evaluates FLOW.QUERY in memory from its shard catalogs |
| 431 | default query execution preserves bounded metadata and terminal reads | internal: Elixir's query engine installation, capability manifests, physical paths and storage reads; the node evaluates FLOW.QUERY in memory from its shard catalogs |
| 505 | terminal query returns every state when records share an update timestamp | fixed (`audit::query_semantics::a_terminal_read_keeps_every_state_sharing_an_update_time`; G40: Flows created finished were removed at once) |
| 540 | default metadata queries do not invent a queued-state filter | fixed (`audit::query_semantics::a_metadata_search_adds_no_state_filter`; G40) |
| 645 | default query execution preserves bounded lineage and inflight reads | internal: Elixir's query engine installation, capability manifests, physical paths and storage reads; the node evaluates FLOW.QUERY in memory from its shard catalogs |
| 743 | default stuck queries honor descending lease-deadline order before limiting | ported (`audit::query_semantics::stuck_reads_order_by_lease_deadline_and_keep_both_bounds`) |
| 802 | stuck queries enforce both lease-deadline bounds without widening the index read | ported (same test) |
| 850 | malformed canonical envelopes never reach an installed query engine | internal: Elixir's query engine installation, capability manifests, physical paths and storage reads; the node evaluates FLOW.QUERY in memory from its shard catalogs |
| 873 | EXPLAIN rejects an unsupported canonical request version | covered: `flow_query::query_failures_carry_codes_and_indexes_describe_the_catalog` (an unsupported version) |
| 884 | the engine rejects oversized bound point values before storage | covered: `query::reference_bind_tests::bound_ids_stop_at_the_storage_key_bounds` |
| 904 | canonical validation rejects oversized values before any installed engine | covered: as line 884 |
| 926 | dispatches a validated canonical request through the installed query engine | internal: Elixir's query engine installation, capability manifests, physical paths and storage reads; the node evaluates FLOW.QUERY in memory from its shard catalogs |
| 943 | an instance freezes its query engine when it is built | internal: Elixir's query engine installation, capability manifests, physical paths and storage reads; the node evaluates FLOW.QUERY in memory from its shard catalogs |
| 969 | explicit per-instance query engines are isolated from application configuration | internal: Elixir's query engine installation, capability manifests, physical paths and storage reads; the node evaluates FLOW.QUERY in memory from its shard catalogs |
| 993 | an instance freezes and exposes its validated query capability manifest | internal: Elixir's query engine installation, capability manifests, physical paths and storage reads; the node evaluates FLOW.QUERY in memory from its shard catalogs |
| 1009 | cached capability manifests cannot bypass request-contract validation | internal: Elixir's query engine installation, capability manifests, physical paths and storage reads; the node evaluates FLOW.QUERY in memory from its shard catalogs |
| 1019 | index status capability and response contract are advertised coherently | internal: Elixir's query engine installation, capability manifests, physical paths and storage reads; the node evaluates FLOW.QUERY in memory from its shard catalogs |
| 1033 | default capabilities advertise the complete cost-aware planner surface | internal: Elixir's query engine installation, capability manifests, physical paths and storage reads; the node evaluates FLOW.QUERY in memory from its shard catalogs |
| 1049 | default capabilities expose the shared bounded response contract | internal: Elixir's query engine installation, capability manifests, physical paths and storage reads; the node evaluates FLOW.QUERY in memory from its shard catalogs |
| 1059 | every default EXPLAIN path matches the advertised OSS contract | internal: Elixir's query engine installation, capability manifests, physical paths and storage reads; the node evaluates FLOW.QUERY in memory from its shard catalogs |
| 1112 | capability support predicates fail closed for malformed provider values | internal: Elixir's query engine installation, capability manifests, physical paths and storage reads; the node evaluates FLOW.QUERY in memory from its shard catalogs |
| 1117 | default capabilities advertise every bounded lineage shape | internal: Elixir's query engine installation, capability manifests, physical paths and storage reads; the node evaluates FLOW.QUERY in memory from its shard catalogs |
| 1130 | instance construction rejects an invalid query engine | internal: Elixir's query engine installation, capability manifests, physical paths and storage reads; the node evaluates FLOW.QUERY in memory from its shard catalogs |
| 1143 | instance construction rejects an invalid query capability manifest | internal: Elixir's query engine installation, capability manifests, physical paths and storage reads; the node evaluates FLOW.QUERY in memory from its shard catalogs |
| 1156 | instance construction rejects capabilities the canonical parser cannot produce | internal: Elixir's query engine installation, capability manifests, physical paths and storage reads; the node evaluates FLOW.QUERY in memory from its shard catalogs |
| 1169 | capability negotiation rejects a surface without a complete request/result pair | internal: Elixir's query engine installation, capability manifests, physical paths and storage reads; the node evaluates FLOW.QUERY in memory from its shard catalogs |
| 1175 | capability negotiation rejects a request contract the parser cannot produce | internal: Elixir's query engine installation, capability manifests, physical paths and storage reads; the node evaluates FLOW.QUERY in memory from its shard catalogs |
| 1181 | keeps scope failures structured, permissioned, and value-free | internal: Elixir's query engine installation, capability manifests, physical paths and storage reads; the node evaluates FLOW.QUERY in memory from its shard catalogs |
| 1188 | query engine failures are fail-closed and never escape the boundary | internal: Elixir's query engine installation, capability manifests, physical paths and storage reads; the node evaluates FLOW.QUERY in memory from its shard catalogs |
| 1213 | query engine preserves canonical structured diagnostics | internal: Elixir's query engine installation, capability manifests, physical paths and storage reads; the node evaluates FLOW.QUERY in memory from its shard catalogs |
| 1246 | point-read storage outages remain structured and safe to retry | internal: Elixir's query engine installation, capability manifests, physical paths and storage reads; the node evaluates FLOW.QUERY in memory from its shard catalogs |
| 1276 | run-id-only point reads retain the direct auto-partition lookup | covered: `flow_query::queries_read_the_catalog_now_and_after_a_restart` (point reads in an auto bucket) |
| 1301 | event queries retain the direct ordered history path | covered: same node test (event history) |
| 1326 | history continuation performs bounded exclusive seeks without duplicates | covered: `query::tests::a_cursor_resumes_its_own_query_after_the_last_row` (a page resumes after its last row; Elixir's index seeks are its own) |
| 1385 | missing runs return a complete empty history page contract | ported (`audit::query_semantics::a_missing_run_has_an_empty_history_page`) |
| 1416 | history pages fail their raw scan budget without mutating expired LMDB rows | internal: Elixir's query engine installation, capability manifests, physical paths and storage reads; the node evaluates FLOW.QUERY in memory from its shard catalogs |
| 1480 | history pages fail closed when an indexed event payload is missing | internal: Elixir's query engine installation, capability manifests, physical paths and storage reads; the node evaluates FLOW.QUERY in memory from its shard catalogs |
| 1524 | history pages validate a dangling lookahead before issuing a cursor | internal: Elixir's query engine installation, capability manifests, physical paths and storage reads; the node evaluates FLOW.QUERY in memory from its shard catalogs |
| 1568 | parent lineage continuation performs bounded exclusive tuple seeks | covered: as line 1326 |
| 1647 | root lineage pages include the authoritative root record within their hard bound | covered: `audit::lineage::lineage_queries_take_windows_states_order_and_counts` (a root query includes the root) |
| 1723 | point reads fail closed when the physical record violates the bound predicates | internal: Elixir's query engine installation, capability manifests, physical paths and storage reads; the node evaluates FLOW.QUERY in memory from its shard catalogs |
| 1768 | point reads classify corrupt encoded records as storage inconsistency | internal: Elixir's query engine installation, capability manifests, physical paths and storage reads; the node evaluates FLOW.QUERY in memory from its shard catalogs |

## `flow/query/error_test.exs`

| Line | Test | Status |
|---|---|---|
| 6 | exposes stable bounded-execution errors with retry semantics | internal: Elixir's execution budget and concurrency reasons; the node's own reasons answer `retryable` false (`query::error_tests` payloads) |
| 43 | makes bounded return projection errors actionable | ported (`query::error_tests::projection_errors_say_what_to_change`) |
| 58 | maps only canonical wire messages back to their query reason | internal: Elixir's message-to-reason lookup |
| 67 | adds a parser-supplied source position and actionable guidance to syntax errors | ported (`query::error_tests::syntax_and_shape_errors_carry_their_position_and_guidance`) |
| 88 | lists bounded supported fields without echoing the rejected query | ported (`query::error_tests::cursor_field_and_source_errors_never_echo_the_query`) |
| 123 | renders diagnostics for the text command surface without changing canonical messages | covered: messages stay canonical (`query::error_tests` checks them); Elixir's text-surface formatter is its own |
| 131 | reports human columns and byte offsets after UTF-8 literals | ported (`query::validation_tests::positions_count_characters_lines_and_bytes`) |
| 137 | positions unsupported clause shapes without changing their stable code | ported (`query::error_tests::syntax_and_shape_errors_carry_their_position_and_guidance`) |
| 148 | positions EXPLAIN ANALYZE failures after the full mode prefix | ported (same test) |
| 157 | makes EXPLAIN ANALYZE cursor rejection actionable | ported (`query::error_tests::cursor_field_and_source_errors_never_echo_the_query`) |
| 171 | points at the first unexpected clause token instead of the query end | ported (`query::error_tests::syntax_and_shape_errors_carry_their_position_and_guidance` and `...::positions_count_bytes_lines_and_characters_at_grammar_boundaries`) |
| 194 | positions malformed grammar boundaries deterministically | ported (`query::error_tests::positions_count_bytes_lines_and_characters_at_grammar_boundaries`) |
| 221 | reports CRLF and multibyte positions in byte and character coordinates | ported (`query::validation_tests::positions_count_characters_lines_and_bytes`) |
| 232 | reports unsupported sources without echoing query literals | ported (`query::error_tests::cursor_field_and_source_errors_never_echo_the_query`) |
| 244 | bounds optional diagnostic sections independently | internal: bounds of hand-built Elixir diagnostics |
| 258 | drops source positions outside the supplied query boundary | ported (`query::error_tests::positions_count_bytes_lines_and_characters_at_grammar_boundaries`) |
| 263 | does not fabricate a position when a parser supplies no source span | covered: `query::validation_tests::only_diagnosed_reasons_report_a_position` |
| 270 | accepts only bounded wire-safe structured diagnostics | internal: validation of hand-built Elixir diagnostics |
| 301 | diagnostic context rejects integers outside the signed native wire range | internal: as line 270 |

## `flow/query/executor_test.exs`

| Line | Test | Status |
|---|---|---|
| 78 | executes a selective range and evaluates only residual predicates | internal: Elixir's index execution (query rows, covering indexes, fused pages, top-K, counters, budgets, deadlines, plan validation); the node evaluates FLOW.QUERY in memory from its shard catalogs |
| 108 | projects selected fields only after authoritative filtering and ordering | covered: `query::tests::projects_selected_fields_after_filtering_ordering_and_limiting` |
| 141 | evaluates sparse projections and residual predicates from query rows without log hydration | internal: Elixir's index execution (query rows, covering indexes, fused pages, top-K, counters, budgets, deadlines, plan validation); the node evaluates FLOW.QUERY in memory from its shard catalogs |
| 200 | returns the complete bare run allowlist from query rows without log hydration | internal: Elixir's index execution (query rows, covering indexes, fused pages, top-K, counters, budgets, deadlines, plan validation); the node evaluates FLOW.QUERY in memory from its shard catalogs |
| 257 | prepares the physical state-key prefix once for a query-row page | internal: Elixir's index execution (query rows, covering indexes, fused pages, top-K, counters, budgets, deadlines, plan validation); the node evaluates FLOW.QUERY in memory from its shard catalogs |
| 287 | prepares and applies a selective decoder for non-fused query rows | internal: Elixir's index execution (query rows, covering indexes, fused pages, top-K, counters, budgets, deadlines, plan validation); the node evaluates FLOW.QUERY in memory from its shard catalogs |
| 325 | uses exact dynamic fields for residual checks and response projection | internal: Elixir's index execution (query rows, covering indexes, fused pages, top-K, counters, budgets, deadlines, plan validation); the node evaluates FLOW.QUERY in memory from its shard catalogs |
| 390 | processes a fused composite and QueryRow page without legacy reads | internal: Elixir's index execution (query rows, covering indexes, fused pages, top-K, counters, budgets, deadlines, plan validation); the node evaluates FLOW.QUERY in memory from its shard catalogs |
| 438 | falls back to bounded two-stage reads when the first fused QueryRow exceeds its budget | internal: Elixir's index execution (query rows, covering indexes, fused pages, top-K, counters, budgets, deadlines, plan validation); the node evaluates FLOW.QUERY in memory from its shard catalogs |
| 475 | uses the bounded legacy path when worst-case fused page memory cannot be reserved | internal: Elixir's index execution (query rows, covering indexes, fused pages, top-K, counters, budgets, deadlines, plan validation); the node evaluates FLOW.QUERY in memory from its shard catalogs |
| 524 | rejects forged fused QueryRow byte accounting before decoding | internal: Elixir's index execution (query rows, covering indexes, fused pages, top-K, counters, budgets, deadlines, plan validation); the node evaluates FLOW.QUERY in memory from its shard catalogs |
| 556 | rejects malformed prefetched QueryRow bytes without a legacy retry | internal: Elixir's index execution (query rows, covering indexes, fused pages, top-K, counters, budgets, deadlines, plan validation); the node evaluates FLOW.QUERY in memory from its shard catalogs |
| 586 | rejects a query-row plan with a non-durable locator before log IO | internal: Elixir's index execution (query rows, covering indexes, fused pages, top-K, counters, budgets, deadlines, plan validation); the node evaluates FLOW.QUERY in memory from its shard catalogs |
| 613 | a custom authoritative reader remains an explicit query-row fallback | internal: Elixir's index execution (query rows, covering indexes, fused pages, top-K, counters, budgets, deadlines, plan validation); the node evaluates FLOW.QUERY in memory from its shard catalogs |
| 635 | serves a covered sparse projection without primary-record hydration | internal: Elixir's index execution (query rows, covering indexes, fused pages, top-K, counters, budgets, deadlines, plan validation); the node evaluates FLOW.QUERY in memory from its shard catalogs |
| 662 | serves covered dynamic attributes and state metadata without hydration | internal: Elixir's index execution (query rows, covering indexes, fused pages, top-K, counters, budgets, deadlines, plan validation); the node evaluates FLOW.QUERY in memory from its shard catalogs |
| 728 | rejects a covering index row that omits its required projection | internal: Elixir's index execution (query rows, covering indexes, fused pages, top-K, counters, budgets, deadlines, plan validation); the node evaluates FLOW.QUERY in memory from its shard catalogs |
| 749 | evaluates covered residual predicates without metadata or WAL hydration | internal: Elixir's index execution (query rows, covering indexes, fused pages, top-K, counters, budgets, deadlines, plan validation); the node evaluates FLOW.QUERY in memory from its shard catalogs |
| 776 | counts covering rows without primary-record hydration | internal: Elixir's index execution (query rows, covering indexes, fused pages, top-K, counters, budgets, deadlines, plan validation); the node evaluates FLOW.QUERY in memory from its shard catalogs |
| 794 | rejects a covering row whose payload does not match its index identity | internal: Elixir's index execution (query rows, covering indexes, fused pages, top-K, counters, budgets, deadlines, plan validation); the node evaluates FLOW.QUERY in memory from its shard catalogs |
| 815 | shared execution hydrates only the sealed physical scope and evaluates logical records | internal: Elixir's index execution (query rows, covering indexes, fused pages, top-K, counters, budgets, deadlines, plan validation); the node evaluates FLOW.QUERY in memory from its shard catalogs |
| 846 | shared execution validates and serves a scope-bound covering row | internal: Elixir's index execution (query rows, covering indexes, fused pages, top-K, counters, budgets, deadlines, plan validation); the node evaluates FLOW.QUERY in memory from its shard catalogs |
| 881 | shared execution serves a scope-bound query row without log hydration | internal: Elixir's index execution (query rows, covering indexes, fused pages, top-K, counters, budgets, deadlines, plan validation); the node evaluates FLOW.QUERY in memory from its shard catalogs |
| 946 | rejects a self-consistent plan outside the independently bound scope before storage | internal: Elixir's index execution (query rows, covering indexes, fused pages, top-K, counters, budgets, deadlines, plan validation); the node evaluates FLOW.QUERY in memory from its shard catalogs |
| 968 | stops an order-preserving range after one lookahead result | internal: Elixir's index execution (query rows, covering indexes, fused pages, top-K, counters, budgets, deadlines, plan validation); the node evaluates FLOW.QUERY in memory from its shard catalogs |
| 992 | bounds a covered native-order storage page to the result lookahead | internal: Elixir's index execution (query rows, covering indexes, fused pages, top-K, counters, budgets, deadlines, plan validation); the node evaluates FLOW.QUERY in memory from its shard catalogs |
| 1023 | continues bounded native reads after an expired projection is skipped | internal: Elixir's index execution (query rows, covering indexes, fused pages, top-K, counters, budgets, deadlines, plan validation); the node evaluates FLOW.QUERY in memory from its shard catalogs |
| 1069 | paginates an order-preserving range with an opaque bounded seek cursor | covered: `query::tests::a_cursor_resumes_its_own_query_after_the_last_row` |
| 1120 | rejects replaying a cursor with different bound predicate values | covered: same test (another query's cursor is refused) |
| 1147 | rejects an authenticated oversized native storage cursor before storage | internal: Elixir's index execution (query rows, covering indexes, fused pages, top-K, counters, budgets, deadlines, plan validation); the node evaluates FLOW.QUERY in memory from its shard catalogs |
| 1189 | uses bounded top-K for an explicit non-native numeric order | internal: Elixir's index execution (query rows, covering indexes, fused pages, top-K, counters, budgets, deadlines, plan validation); the node evaluates FLOW.QUERY in memory from its shard catalogs |
| 1219 | bounded top-K does not project candidates that cannot enter the retained heap | internal: Elixir's index execution (query rows, covering indexes, fused pages, top-K, counters, budgets, deadlines, plan validation); the node evaluates FLOW.QUERY in memory from its shard catalogs |
| 1249 | does not revalidate the sealed index definition per hydrated row | internal: Elixir's index execution (query rows, covering indexes, fused pages, top-K, counters, budgets, deadlines, plan validation); the node evaluates FLOW.QUERY in memory from its shard catalogs |
| 1282 | paginates bounded top-K order without duplicates | covered: as line 1069 |
| 1298 | paginates a multi-range union in global order after deduplication | covered: as line 1069 |
| 1322 | deduplicates a multi-value IN union before hydration | internal: Elixir's index execution (query rows, covering indexes, fused pages, top-K, counters, budgets, deadlines, plan validation); the node evaluates FLOW.QUERY in memory from its shard catalogs |
| 1366 | counts every exact match across bounded pages without retaining result rows | internal: Elixir's index execution (query rows, covering indexes, fused pages, top-K, counters, budgets, deadlines, plan validation); the node evaluates FLOW.QUERY in memory from its shard catalogs |
| 1403 | fully covered counts do not materialize public record maps | internal: Elixir's index execution (query rows, covering indexes, fused pages, top-K, counters, budgets, deadlines, plan validation); the node evaluates FLOW.QUERY in memory from its shard catalogs |
| 1424 | prepared record matching does not reconstruct logical scope per covered count row | internal: Elixir's index execution (query rows, covering indexes, fused pages, top-K, counters, budgets, deadlines, plan validation); the node evaluates FLOW.QUERY in memory from its shard catalogs |
| 1445 | contradictory exact predicates execute as an empty bounded count | internal: Elixir's index execution (query rows, covering indexes, fused pages, top-K, counters, budgets, deadlines, plan validation); the node evaluates FLOW.QUERY in memory from its shard catalogs |
| 1478 | count unions deduplicate multivalue index entries before hydration | internal: Elixir's index execution (query rows, covering indexes, fused pages, top-K, counters, budgets, deadlines, plan validation); the node evaluates FLOW.QUERY in memory from its shard catalogs |
| 1510 | single-range counts do not retain an unnecessary global ID set | internal: Elixir's index execution (query rows, covering indexes, fused pages, top-K, counters, budgets, deadlines, plan validation); the node evaluates FLOW.QUERY in memory from its shard catalogs |
| 1538 | single-range counts reject a duplicate current-version key not owned by the hydrated row | internal: Elixir's index execution (query rows, covering indexes, fused pages, top-K, counters, budgets, deadlines, plan validation); the node evaluates FLOW.QUERY in memory from its shard catalogs |
| 1560 | count budget exhaustion fails instead of returning a partial scalar | internal: Elixir's index execution (query rows, covering indexes, fused pages, top-K, counters, budgets, deadlines, plan validation); the node evaluates FLOW.QUERY in memory from its shard catalogs |
| 1578 | reads covered count prefixes without index scans or row hydration | internal: Elixir's index execution (query rows, covering indexes, fused pages, top-K, counters, budgets, deadlines, plan validation); the node evaluates FLOW.QUERY in memory from its shard catalogs |
| 1603 | falls back to a bounded scan when a covered count can include expired rows | internal: Elixir's index execution (query rows, covering indexes, fused pages, top-K, counters, budgets, deadlines, plan validation); the node evaluates FLOW.QUERY in memory from its shard catalogs |
| 1648 | counts live expiring rows from query metadata during counter fallback | internal: Elixir's index execution (query rows, covering indexes, fused pages, top-K, counters, budgets, deadlines, plan validation); the node evaluates FLOW.QUERY in memory from its shard catalogs |
| 1683 | sums disjoint scalar counter ranges and rejects overflow or prefix mismatch | internal: Elixir's index execution (query rows, covering indexes, fused pages, top-K, counters, budgets, deadlines, plan validation); the node evaluates FLOW.QUERY in memory from its shard catalogs |
| 1730 | returns no partial result when an index-entry budget is exhausted | internal: Elixir's index execution (query rows, covering indexes, fused pages, top-K, counters, budgets, deadlines, plan validation); the node evaluates FLOW.QUERY in memory from its shard catalogs |
| 1749 | rejects a cross-tenant state key before any record read | internal: Elixir's index execution (query rows, covering indexes, fused pages, top-K, counters, budgets, deadlines, plan validation); the node evaluates FLOW.QUERY in memory from its shard catalogs |
| 1776 | rejects underreported range bytes before any record read | internal: Elixir's index execution (query rows, covering indexes, fused pages, top-K, counters, budgets, deadlines, plan validation); the node evaluates FLOW.QUERY in memory from its shard catalogs |
| 1811 | rejects an entry whose decoded binaries exceed its claimed storage bytes | internal: Elixir's index execution (query rows, covering indexes, fused pages, top-K, counters, budgets, deadlines, plan validation); the node evaluates FLOW.QUERY in memory from its shard catalogs |
| 1834 | rejects a malformed final page entry before reading its cursor field | internal: Elixir's index execution (query rows, covering indexes, fused pages, top-K, counters, budgets, deadlines, plan validation); the node evaluates FLOW.QUERY in memory from its shard catalogs |
| 1866 | rejects an oversized projected identity before hashing or hydration | internal: Elixir's index execution (query rows, covering indexes, fused pages, top-K, counters, budgets, deadlines, plan validation); the node evaluates FLOW.QUERY in memory from its shard catalogs |
| 1891 | rejects an out-of-domain projected version before hydration | internal: Elixir's index execution (query rows, covering indexes, fused pages, top-K, counters, budgets, deadlines, plan validation); the node evaluates FLOW.QUERY in memory from its shard catalogs |
| 1914 | rejects an out-of-domain projected expiry before hydration | internal: Elixir's index execution (query rows, covering indexes, fused pages, top-K, counters, budgets, deadlines, plan validation); the node evaluates FLOW.QUERY in memory from its shard catalogs |
| 1937 | rejects an oversized composite storage key before hydration | internal: Elixir's index execution (query rows, covering indexes, fused pages, top-K, counters, budgets, deadlines, plan validation); the node evaluates FLOW.QUERY in memory from its shard catalogs |
| 1972 | detects a projection change between range and row reads | internal: Elixir's index execution (query rows, covering indexes, fused pages, top-K, counters, budgets, deadlines, plan validation); the node evaluates FLOW.QUERY in memory from its shard catalogs |
| 1990 | does not hide a live row behind a stale projected expiry | internal: Elixir's index execution (query rows, covering indexes, fused pages, top-K, counters, budgets, deadlines, plan validation); the node evaluates FLOW.QUERY in memory from its shard catalogs |
| 2009 | serves a live expiring covering row without authoritative hydration | internal: Elixir's index execution (query rows, covering indexes, fused pages, top-K, counters, budgets, deadlines, plan validation); the node evaluates FLOW.QUERY in memory from its shard catalogs |
| 2034 | skips an expired covering row without authoritative hydration | internal: Elixir's index execution (query rows, covering indexes, fused pages, top-K, counters, budgets, deadlines, plan validation); the node evaluates FLOW.QUERY in memory from its shard catalogs |
| 2058 | skips a missing row only when its projected entry is expired | internal: Elixir's index execution (query rows, covering indexes, fused pages, top-K, counters, budgets, deadlines, plan validation); the node evaluates FLOW.QUERY in memory from its shard catalogs |
| 2076 | does not hide a malformed hydrated value behind an expired projected entry | internal: Elixir's index execution (query rows, covering indexes, fused pages, top-K, counters, budgets, deadlines, plan validation); the node evaluates FLOW.QUERY in memory from its shard catalogs |
| 2091 | checks the wall deadline between bounded pages | internal: Elixir's index execution (query rows, covering indexes, fused pages, top-K, counters, budgets, deadlines, plan validation); the node evaluates FLOW.QUERY in memory from its shard catalogs |
| 2115 | checks the wall deadline after a query-row read before validating its rows | internal: Elixir's index execution (query rows, covering indexes, fused pages, top-K, counters, budgets, deadlines, plan validation); the node evaluates FLOW.QUERY in memory from its shard catalogs |
| 2148 | samples the wall deadline during a large covering page | internal: Elixir's index execution (query rows, covering indexes, fused pages, top-K, counters, budgets, deadlines, plan validation); the node evaluates FLOW.QUERY in memory from its shard catalogs |
| 2182 | bounds authoritative reads by the remaining query deadline | internal: Elixir's index execution (query rows, covering indexes, fused pages, top-K, counters, budgets, deadlines, plan validation); the node evaluates FLOW.QUERY in memory from its shard catalogs |
| 2206 | does not start authoritative IO after the query deadline | internal: Elixir's index execution (query rows, covering indexes, fused pages, top-K, counters, budgets, deadlines, plan validation); the node evaluates FLOW.QUERY in memory from its shard catalogs |
| 2234 | reports a deadline that expires during final response assembly | internal: Elixir's index execution (query rows, covering indexes, fused pages, top-K, counters, budgets, deadlines, plan validation); the node evaluates FLOW.QUERY in memory from its shard catalogs |
| 2267 | defers exact response-size enforcement to the response boundary | internal: Elixir's index execution (query rows, covering indexes, fused pages, top-K, counters, budgets, deadlines, plan validation); the node evaluates FLOW.QUERY in memory from its shard catalogs |
| 2314 | accounts for the range page while hydrated rows are live | internal: Elixir's index execution (query rows, covering indexes, fused pages, top-K, counters, budgets, deadlines, plan validation); the node evaluates FLOW.QUERY in memory from its shard catalogs |
| 2334 | rejects a decoded covering page that exceeds the executor memory budget | internal: Elixir's index execution (query rows, covering indexes, fused pages, top-K, counters, budgets, deadlines, plan validation); the node evaluates FLOW.QUERY in memory from its shard catalogs |
| 2358 | rejects decoded record heap that only fits under external-term accounting | internal: Elixir's index execution (query rows, covering indexes, fused pages, top-K, counters, budgets, deadlines, plan validation); the node evaluates FLOW.QUERY in memory from its shard catalogs |
| 2388 | splits an over-budget hydration batch before materializing its records | internal: Elixir's index execution (query rows, covering indexes, fused pages, top-K, counters, budgets, deadlines, plan validation); the node evaluates FLOW.QUERY in memory from its shard catalogs |
| 2419 | resumes a bounded hydration prefix without rereading returned state keys | internal: Elixir's index execution (query rows, covering indexes, fused pages, top-K, counters, budgets, deadlines, plan validation); the node evaluates FLOW.QUERY in memory from its shard catalogs |
| 2465 | rejects a plan whose defensive rechecks do not exactly match the request | internal: Elixir's index execution (query rows, covering indexes, fused pages, top-K, counters, budgets, deadlines, plan validation); the node evaluates FLOW.QUERY in memory from its shard catalogs |
| 2487 | rejects incompatible plan versions and request fingerprints before storage | internal: Elixir's index execution (query rows, covering indexes, fused pages, top-K, counters, budgets, deadlines, plan validation); the node evaluates FLOW.QUERY in memory from its shard catalogs |
| 2510 | rejects a forged index definition and matching forged range before storage | internal: Elixir's index execution (query rows, covering indexes, fused pages, top-K, counters, budgets, deadlines, plan validation); the node evaluates FLOW.QUERY in memory from its shard catalogs |
| 2536 | rejects an empty plan for a non-empty request | internal: Elixir's index execution (query rows, covering indexes, fused pages, top-K, counters, budgets, deadlines, plan validation); the node evaluates FLOW.QUERY in memory from its shard catalogs |
| 2552 | rejects malformed composite range fields before storage | internal: Elixir's index execution (query rows, covering indexes, fused pages, top-K, counters, budgets, deadlines, plan validation); the node evaluates FLOW.QUERY in memory from its shard catalogs |
| 2569 | rejects a tenant-contained range widened beyond the request constraints | internal: Elixir's index execution (query rows, covering indexes, fused pages, top-K, counters, budgets, deadlines, plan validation); the node evaluates FLOW.QUERY in memory from its shard catalogs |
| 2589 | rejects removed plan order states before storage | internal: Elixir's index execution (query rows, covering indexes, fused pages, top-K, counters, budgets, deadlines, plan validation); the node evaluates FLOW.QUERY in memory from its shard catalogs |
| 2610 | rejects an unbounded hydration callback before storage | internal: Elixir's index execution (query rows, covering indexes, fused pages, top-K, counters, budgets, deadlines, plan validation); the node evaluates FLOW.QUERY in memory from its shard catalogs |
| 2631 | generated planned executions agree with the independent reference evaluator | internal: Elixir's agreement check between its planned executor and its reference evaluator |
| 2683 | reads real composite entries from LMDB and authoritative Flow rows from WARaft | internal: Elixir's index execution (query rows, covering indexes, fused pages, top-K, counters, budgets, deadlines, plan validation); the node evaluates FLOW.QUERY in memory from its shard catalogs |
| 2761 | serves real covering composite entries from LMDB without primary rows | internal: Elixir's index execution (query rows, covering indexes, fused pages, top-K, counters, budgets, deadlines, plan validation); the node evaluates FLOW.QUERY in memory from its shard catalogs |

## `flow/query/explain_test.exs`

| Line | Test | Status |
|---|---|---|
| 7 | renders deterministic, value-redacted plan evidence | internal: Elixir's EXPLAIN plan evidence for its indexes; the node's EXPLAIN describes its in-memory evaluation |
| 86 | reports compact metadata-row projection without log hydration | internal: Elixir's EXPLAIN plan evidence for its indexes; the node's EXPLAIN describes its in-memory evaluation |
| 106 | reports when a sparse projection is eligible for covering execution | internal: Elixir's EXPLAIN plan evidence for its indexes; the node's EXPLAIN describes its in-memory evaluation |
| 139 | lists alternatives deterministically without physical bounds | internal: Elixir's EXPLAIN plan evidence for its indexes; the node's EXPLAIN describes its in-memory evaluation |
| 190 | renders a stable rejected plan with a bounded reason | internal: Elixir's EXPLAIN plan evidence for its indexes; the node's EXPLAIN describes its in-memory evaluation |
| 227 | renders scalar count semantics without row pagination | internal: Elixir's EXPLAIN plan evidence for its indexes; the node's EXPLAIN describes its in-memory evaluation |
| 243 | describes authoritative history pagination without projection semantics | internal: Elixir's EXPLAIN plan evidence for its indexes; the node's EXPLAIN describes its in-memory evaluation |
| 268 | describes authoritative lineage reads without projection semantics | internal: Elixir's EXPLAIN plan evidence for its indexes; the node's EXPLAIN describes its in-memory evaluation |
| 297 | turns budget plan rejection into bounded remediation instead of crashing | internal: Elixir's EXPLAIN plan evidence for its indexes; the node's EXPLAIN describes its in-memory evaluation |
| 327 | keeps the maximum registry alternative set inside the response budget | internal: Elixir's EXPLAIN plan evidence for its indexes; the node's EXPLAIN describes its in-memory evaluation |
| 355 | adds validated actual counters without changing logical plan evidence | internal: Elixir's EXPLAIN plan evidence for its indexes; the node's EXPLAIN describes its in-memory evaluation |
| 402 | distinguishes expected pressure from the hard execution ceiling | internal: Elixir's EXPLAIN plan evidence for its indexes; the node's EXPLAIN describes its in-memory evaluation |

## `flow/query/field_test.exs`

| Line | Test | Status |
|---|---|---|
| 6 | normalizes built-in and bounded metadata field names without creating atoms | internal: Elixir's field name normalization without atoms |
| 28 | round-trips metadata names that require bracket quoting | covered: `query::fql::reference_tests::accepts_every_shape_the_reference_parser_accepts` (bracket-quoted names) |
| 46 | rejects ambiguous, reserved, or oversized metadata paths | covered: `query::fql::reference_tests::refuses_what_the_reference_parser_refuses` (reserved names) |
| 58 | distinguishes explicit null from a missing record field | covered: `query::tests::null_and_missing_stay_distinct` |
| 74 | reads string-keyed decoded records without atomizing keys | internal: Elixir's string-keyed record decoding |

## `flow/query/fixed_index_executor_test.exs`

| Line | Test | Status |
|---|---|---|
| 9 | explicit fixed-index projections skip the full allowlisted intermediate | internal: Elixir's index registry, planner and admission (fixed_index_executor); the node evaluates FLOW.QUERY in memory from its shard catalogs, with no persisted indexes to build, plan over or admit |
| 56 | fixed fallback bounds each synchronous candidate window to one maximum page | internal: Elixir's index registry, planner and admission (fixed_index_executor); the node evaluates FLOW.QUERY in memory from its shard catalogs, with no persisted indexes to build, plan over or admit |
| 77 | fixed terminal reads synchronize the cold projection before answering | internal: Elixir's index registry, planner and admission (fixed_index_executor); the node evaluates FLOW.QUERY in memory from its shard catalogs, with no persisted indexes to build, plan over or admit |
| 116 | normalizes the indexed reader's candidate ceiling as a scan-budget error | internal: Elixir's index registry, planner and admission (fixed_index_executor); the node evaluates FLOW.QUERY in memory from its shard catalogs, with no persisted indexes to build, plan over or admit |
| 123 | normalizes a type-less state-metadata lookup as a missing bounded plan | internal: Elixir's index registry, planner and admission (fixed_index_executor); the node evaluates FLOW.QUERY in memory from its shard catalogs, with no persisted indexes to build, plan over or admit |
| 130 | EXPLAIN rejects a type-less state-metadata shape that execution cannot plan | internal: Elixir's index registry, planner and admission (fixed_index_executor); the node evaluates FLOW.QUERY in memory from its shard catalogs, with no persisted indexes to build, plan over or admit |
| 146 | EXPLAIN rejects nonterminal multi-state filters that lack a fixed bounded fanout | internal: Elixir's index registry, planner and admission (fixed_index_executor); the node evaluates FLOW.QUERY in memory from its shard catalogs, with no persisted indexes to build, plan over or admit |
| 185 | EXPLAIN accounts for terminal-subset fanout and rejects repeated states | internal: Elixir's index registry, planner and admission (fixed_index_executor); the node evaluates FLOW.QUERY in memory from its shard catalogs, with no persisted indexes to build, plan over or admit |

## `flow/query/history_executor_test.exs`

| Line | Test | Status |
|---|---|---|
| 20 | issues an authenticated cursor and resumes from its exclusive event boundary | internal: Elixir's index paging executors (a page resuming after its last row: `query::tests::a_cursor_resumes_its_own_query_after_the_last_row`) |
| 101 | projects history fields after validating the full page and continuation | internal: Elixir's index paging executors (a page resuming after its last row: `query::tests::a_cursor_resumes_its_own_query_after_the_last_row`) |
| 134 | bare history returns do not reserve a duplicate output page | internal: Elixir's index paging executors (a page resuming after its last row: `query::tests::a_cursor_resumes_its_own_query_after_the_last_row`) |
| 169 | rejects history pages containing fields outside the public result allowlist | internal: Elixir's index paging executors (a page resuming after its last row: `query::tests::a_cursor_resumes_its_own_query_after_the_last_row`) |
| 197 | rejects forged page accounting and malformed cursor continuations | internal: Elixir's index paging executors (a page resuming after its last row: `query::tests::a_cursor_resumes_its_own_query_after_the_last_row`) |
| 242 | rejects a page whose decoded-record peak exceeds the executor budget | internal: Elixir's index paging executors (a page resuming after its last row: `query::tests::a_cursor_resumes_its_own_query_after_the_last_row`) |
| 268 | rejects resumed pages that do not advance past the authenticated event boundary | internal: Elixir's index paging executors (a page resuming after its last row: `query::tests::a_cursor_resumes_its_own_query_after_the_last_row`) |
| 312 | accepts pages of one millisecond in version order | covered: `audit::history_requests::event_ids_within_a_millisecond_compare_by_version_number` |

## `flow/query/index_catalog_test.exs`

| Line | Test | Status |
|---|---|---|
| 6 | loads the bounded launch catalog deterministically | internal: Elixir's index registry, planner and admission (index_catalog); the node evaluates FLOW.QUERY in memory from its shard catalogs, with no persisted indexes to build, plan over or admit |
| 42 | loads canonical built-in and dynamic covering fields and rejects incomplete coverage | internal: Elixir's index registry, planner and admission (index_catalog); the node evaluates FLOW.QUERY in memory from its shard catalogs, with no persisted indexes to build, plan over or admit |
| 100 | binds the frozen hidden-scope width into every physical definition | internal: Elixir's index registry, planner and admission (index_catalog); the node evaluates FLOW.QUERY in memory from its shard catalogs, with no persisted indexes to build, plan over or admit |
| 113 | catalog identity is independent of JSON index and workload order | internal: Elixir's index registry, planner and admission (index_catalog); the node evaluates FLOW.QUERY in memory from its shard catalogs, with no persisted indexes to build, plan over or admit |
| 142 | rejects malformed fields instead of widening the catalog | internal: Elixir's index registry, planner and admission (index_catalog); the node evaluates FLOW.QUERY in memory from its shard catalogs, with no persisted indexes to build, plan over or admit |
| 170 | rejects two physical versions for one logical index | internal: Elixir's index registry, planner and admission (index_catalog); the node evaluates FLOW.QUERY in memory from its shard catalogs, with no persisted indexes to build, plan over or admit |
| 201 | rejects catalog versions outside the replicated unsigned 64-bit contract | internal: Elixir's index registry, planner and admission (index_catalog); the node evaluates FLOW.QUERY in memory from its shard catalogs, with no persisted indexes to build, plan over or admit |
| 229 | rejects counter prefixes that include an ordered range dimension | internal: Elixir's index registry, planner and admission (index_catalog); the node evaluates FLOW.QUERY in memory from its shard catalogs, with no persisted indexes to build, plan over or admit |

## `flow/query/index_definition_test.exs`

| Line | Test | Status |
|---|---|---|
| 6 | validates a tenant-prefixed composite definition and gives it a stable fingerprint | internal: Elixir's index registry, planner and admission (index_definition); the node evaluates FLOW.QUERY in memory from its shard catalogs, with no persisted indexes to build, plan over or admit |
| 50 | validates canonical exact-counter prefixes as part of the physical fingerprint | internal: Elixir's index registry, planner and admission (index_definition); the node evaluates FLOW.QUERY in memory from its shard catalogs, with no persisted indexes to build, plan over or admit |
| 77 | validates explicit covering fields as part of the physical fingerprint | internal: Elixir's index registry, planner and admission (index_definition); the node evaluates FLOW.QUERY in memory from its shard catalogs, with no persisted indexes to build, plan over or admit |
| 119 | allows dynamic index and projection fields in covering definitions | internal: Elixir's index registry, planner and admission (index_definition); the node evaluates FLOW.QUERY in memory from its shard catalogs, with no persisted indexes to build, plan over or admit |
| 150 | validates bounded workload identifiers without changing the physical key fingerprint | internal: Elixir's index registry, planner and admission (index_definition); the node evaluates FLOW.QUERY in memory from its shard catalogs, with no persisted indexes to build, plan over or admit |
| 171 | requires tenant scope as the leading ascending field | internal: Elixir's index registry, planner and admission (index_definition); the node evaluates FLOW.QUERY in memory from its shard catalogs, with no persisted indexes to build, plan over or admit |
| 187 | bounds definitions and rejects duplicate or unsafe fields | internal: Elixir's index registry, planner and admission (index_definition); the node evaluates FLOW.QUERY in memory from its shard catalogs, with no persisted indexes to build, plan over or admit |
| 227 | requires hash encoding for unbounded keyword fields and ascending hash components | internal: Elixir's index registry, planner and admission (index_definition); the node evaluates FLOW.QUERY in memory from its shard catalogs, with no persisted indexes to build, plan over or admit |
| 249 | shares exact identity validation with management projections | internal: Elixir's index registry, planner and admission (index_definition); the node evaluates FLOW.QUERY in memory from its shard catalogs, with no persisted indexes to build, plan over or admit |
| 265 | allowed definitions have a proven worst-case LMDB key below 512 bytes | internal: Elixir's index registry, planner and admission (index_definition); the node evaluates FLOW.QUERY in memory from its shard catalogs, with no persisted indexes to build, plan over or admit |
| 294 | rejects invalid hidden scope widths before computing a key layout | internal: Elixir's index registry, planner and admission (index_definition); the node evaluates FLOW.QUERY in memory from its shard catalogs, with no persisted indexes to build, plan over or admit |
| 307 | definition storage prefix is versioned, fixed-width, and isolated from identifier delimiters | internal: Elixir's index registry, planner and admission (index_definition); the node evaluates FLOW.QUERY in memory from its shard catalogs, with no persisted indexes to build, plan over or admit |
| 342 | keeps rejecting altered copies after the genuine definition validated | internal: Elixir's index registry, planner and admission (index_definition); the node evaluates FLOW.QUERY in memory from its shard catalogs, with no persisted indexes to build, plan over or admit |
| 365 | repeat validation of a known definition does not rebuild it | internal: Elixir's index registry, planner and admission (index_definition); the node evaluates FLOW.QUERY in memory from its shard catalogs, with no persisted indexes to build, plan over or admit |

## `flow/query/index_lifecycle_worker_test.exs`

| Line | Test | Status |
|---|---|---|
| 11 | resumes a fenced catalog build, validates every definition, and activates it atomically | internal: Elixir's index registry, planner and admission (index_lifecycle_worker); the node evaluates FLOW.QUERY in memory from its shard catalogs, with no persisted indexes to build, plan over or admit |
| 118 | pauses build work under operational pressure without moving durable checkpoints | internal: Elixir's index registry, planner and admission (index_lifecycle_worker); the node evaluates FLOW.QUERY in memory from its shard catalogs, with no persisted indexes to build, plan over or admit |
| 162 | fails pressure sampling closed before validation work | internal: Elixir's index registry, planner and admission (index_lifecycle_worker); the node evaluates FLOW.QUERY in memory from its shard catalogs, with no persisted indexes to build, plan over or admit |
| 191 | does not complete validation while staging cleanup is in progress or blocked | internal: Elixir's index registry, planner and admission (index_lifecycle_worker); the node evaluates FLOW.QUERY in memory from its shard catalogs, with no persisted indexes to build, plan over or admit |
| 253 | validation mismatch stops candidate projection and records bounded rollback evidence | internal: Elixir's index registry, planner and admission (index_lifecycle_worker); the node evaluates FLOW.QUERY in memory from its shard catalogs, with no persisted indexes to build, plan over or admit |
| 293 | confirms a validation mismatch after a writer barrier before failing the build | internal: Elixir's index registry, planner and admission (index_lifecycle_worker); the node evaluates FLOW.QUERY in memory from its shard catalogs, with no persisted indexes to build, plan over or admit |
| 346 | restarts validation from a durable fence after concurrent fanout growth | internal: Elixir's index registry, planner and admission (index_lifecycle_worker); the node evaluates FLOW.QUERY in memory from its shard catalogs, with no persisted indexes to build, plan over or admit |
| 391 | fences failed projection before resumable physical cleanup | internal: Elixir's index registry, planner and admission (index_lifecycle_worker); the node evaluates FLOW.QUERY in memory from its shard catalogs, with no persisted indexes to build, plan over or admit |
| 483 | continues retirement cleanup under operational pressure | internal: Elixir's index registry, planner and admission (index_lifecycle_worker); the node evaluates FLOW.QUERY in memory from its shard catalogs, with no persisted indexes to build, plan over or admit |
| 513 | idles after every failed catalog index has completed retirement | internal: Elixir's index registry, planner and admission (index_lifecycle_worker); the node evaluates FLOW.QUERY in memory from its shard catalogs, with no persisted indexes to build, plan over or admit |
| 561 | waits for admitted queries before fencing retirement | internal: Elixir's index registry, planner and admission (index_lifecycle_worker); the node evaluates FLOW.QUERY in memory from its shard catalogs, with no persisted indexes to build, plan over or admit |
| 601 | default retirement drain waits only for queries pinned to that index | internal: Elixir's index registry, planner and admission (index_lifecycle_worker); the node evaluates FLOW.QUERY in memory from its shard catalogs, with no persisted indexes to build, plan over or admit |
| 717 | contains callback exceptions so the worker can retry | internal: Elixir's index registry, planner and admission (index_lifecycle_worker); the node evaluates FLOW.QUERY in memory from its shard catalogs, with no persisted indexes to build, plan over or admit |
| 738 | rejects a non-advancing backfill cursor before repeating projection writes | internal: Elixir's index registry, planner and admission (index_lifecycle_worker); the node evaluates FLOW.QUERY in memory from its shard catalogs, with no persisted indexes to build, plan over or admit |
| 775 | reduces the source page until projection fits and advances the durable cursor | internal: Elixir's index registry, planner and admission (index_lifecycle_worker); the node evaluates FLOW.QUERY in memory from its shard catalogs, with no persisted indexes to build, plan over or admit |
| 865 | asks for pages of hundreds of records by default | internal: Elixir's index registry, planner and admission (index_lifecycle_worker); the node evaluates FLOW.QUERY in memory from its shard catalogs, with no persisted indexes to build, plan over or admit |
| 901 | halves a snapshot page until it fits the byte budget | internal: Elixir's index registry, planner and admission (index_lifecycle_worker); the node evaluates FLOW.QUERY in memory from its shard catalogs, with no persisted indexes to build, plan over or admit |
| 934 | reports a snapshot page that does not fit even one record | internal: Elixir's index registry, planner and admission (index_lifecycle_worker); the node evaluates FLOW.QUERY in memory from its shard catalogs, with no persisted indexes to build, plan over or admit |
| 960 | halves a validation page until its reads fit the byte budget | internal: Elixir's index registry, planner and admission (index_lifecycle_worker); the node evaluates FLOW.QUERY in memory from its shard catalogs, with no persisted indexes to build, plan over or admit |
| 997 | halves a backfill page whose records do not fit the hydration budget | internal: Elixir's index registry, planner and admission (index_lifecycle_worker); the node evaluates FLOW.QUERY in memory from its shard catalogs, with no persisted indexes to build, plan over or admit |
| 1044 | takes snapshot and backfill pages until the step budget is spent, then checkpoints once | internal: Elixir's index registry, planner and admission (index_lifecycle_worker); the node evaluates FLOW.QUERY in memory from its shard catalogs, with no persisted indexes to build, plan over or admit |
| 1109 | stops a multi-page step at its budget and resumes from the committed cursor | internal: Elixir's index registry, planner and admission (index_lifecycle_worker); the node evaluates FLOW.QUERY in memory from its shard catalogs, with no persisted indexes to build, plan over or admit |
| 1163 | takes validation pages within one definition, then commits at the next boundary | internal: Elixir's index registry, planner and admission (index_lifecycle_worker); the node evaluates FLOW.QUERY in memory from its shard catalogs, with no persisted indexes to build, plan over or admit |
| 1216 | commits the pages before a failed one and reports the failure on the next step | internal: Elixir's index registry, planner and admission (index_lifecycle_worker); the node evaluates FLOW.QUERY in memory from its shard catalogs, with no persisted indexes to build, plan over or admit |
| 1265 | commits the validation pages before a retried one | internal: Elixir's index registry, planner and admission (index_lifecycle_worker); the node evaluates FLOW.QUERY in memory from its shard catalogs, with no persisted indexes to build, plan over or admit |
| 1308 | rejects a snapshot callback that reports no progress | internal: Elixir's index registry, planner and admission (index_lifecycle_worker); the node evaluates FLOW.QUERY in memory from its shard catalogs, with no persisted indexes to build, plan over or admit |
| 1327 | rejects projection metrics that do not match the accepted page | internal: Elixir's index registry, planner and admission (index_lifecycle_worker); the node evaluates FLOW.QUERY in memory from its shard catalogs, with no persisted indexes to build, plan over or admit |
| 1366 | runs independent shard lifecycle steps with bounded concurrency | internal: Elixir's index registry, planner and admission (index_lifecycle_worker); the node evaluates FLOW.QUERY in memory from its shard catalogs, with no persisted indexes to build, plan over or admit |
| 1402 | skips unavailable shards without starving later ready shards | internal: Elixir's index registry, planner and admission (index_lifecycle_worker); the node evaluates FLOW.QUERY in memory from its shard catalogs, with no persisted indexes to build, plan over or admit |

## `flow/query/index_provider_test.exs`

| Line | Test | Status |
|---|---|---|
| 15 | serves validated ETS snapshots through the core provider contract | internal: Elixir's persisted query engine (index_provider); the node evaluates FLOW.QUERY in memory from its shard catalogs (ADR-014 step 12) |

## `flow/query/index_registry_journal_test.exs`

| Line | Test | Status |
|---|---|---|
| 6 | reading a missing journal initializes the durable empty stream | internal: Elixir's persisted query engine (index_registry_journal); the node evaluates FLOW.QUERY in memory from its shard catalogs (ADR-014 step 12) |
| 20 | reads the snapshot each event follows, and events older releases wrote | internal: Elixir's persisted query engine (index_registry_journal); the node evaluates FLOW.QUERY in memory from its shard catalogs (ADR-014 step 12) |

## `flow/query/index_registry_test.exs`

| Line | Test | Status |
|---|---|---|
| 45 | fails closed when the immutable metadata snapshot is absent | internal: Elixir's index registry, planner and admission (index_registry); the node evaluates FLOW.QUERY in memory from its shard catalogs, with no persisted indexes to build, plan over or admit |
| 60 | rejects restart under a different metadata schema with the same scope width | internal: Elixir's index registry, planner and admission (index_registry); the node evaluates FLOW.QUERY in memory from its shard catalogs, with no persisted indexes to build, plan over or admit |
| 89 | persists monotonic checkpoints and never exposes a partial build | internal: Elixir's index registry, planner and admission (index_registry); the node evaluates FLOW.QUERY in memory from its shard catalogs, with no persisted indexes to build, plan over or admit |
| 175 | journals progress without rewriting the registry snapshot and replays it on restart | internal: Elixir's index registry, planner and admission (index_registry); the node evaluates FLOW.QUERY in memory from its shard catalogs, with no persisted indexes to build, plan over or admit |
| 205 | restarts after validation completes but before activation | internal: Elixir's index registry, planner and admission (index_registry); the node evaluates FLOW.QUERY in memory from its shard catalogs, with no persisted indexes to build, plan over or admit |
| 230 | a validation restart survives stopping before the journal is emptied | internal: Elixir's index registry, planner and admission (index_registry); the node evaluates FLOW.QUERY in memory from its shard catalogs, with no persisted indexes to build, plan over or admit |
| 258 | repairs an incomplete final journal frame after replaying durable progress | internal: Elixir's index registry, planner and admission (index_registry); the node evaluates FLOW.QUERY in memory from its shard catalogs, with no persisted indexes to build, plan over or admit |
| 286 | fails startup closed for a complete journal frame with a corrupt checksum | internal: Elixir's index registry, planner and admission (index_registry); the node evaluates FLOW.QUERY in memory from its shard catalogs, with no persisted indexes to build, plan over or admit |
| 318 | overview exposes bounded lifecycle progress without resume cursors | internal: Elixir's index registry, planner and admission (index_registry); the node evaluates FLOW.QUERY in memory from its shard catalogs, with no persisted indexes to build, plan over or admit |
| 385 | overview reports validation failures and retirement work | internal: Elixir's index registry, planner and admission (index_registry); the node evaluates FLOW.QUERY in memory from its shard catalogs, with no persisted indexes to build, plan over or admit |
| 450 | checkpoints every index in one catalog build atomically | internal: Elixir's index registry, planner and admission (index_registry); the node evaluates FLOW.QUERY in memory from its shard catalogs, with no persisted indexes to build, plan over or admit |
| 479 | completion clears the resume cursor for every index in the build | internal: Elixir's index registry, planner and admission (index_registry); the node evaluates FLOW.QUERY in memory from its shard catalogs, with no persisted indexes to build, plan over or admit |
| 514 | only the completion API may write done and it requires backfill | internal: Elixir's index registry, planner and admission (index_registry); the node evaluates FLOW.QUERY in memory from its shard catalogs, with no persisted indexes to build, plan over or admit |
| 541 | rejects lifecycle counters outside the unsigned 64-bit contract | internal: Elixir's index registry, planner and admission (index_registry); the node evaluates FLOW.QUERY in memory from its shard catalogs, with no persisted indexes to build, plan over or admit |
| 557 | requires the initial build fence to preserve zero progress | internal: Elixir's index registry, planner and admission (index_registry); the node evaluates FLOW.QUERY in memory from its shard catalogs, with no persisted indexes to build, plan over or admit |
| 578 | validation completion cannot change counters after cleanup | internal: Elixir's index registry, planner and admission (index_registry); the node evaluates FLOW.QUERY in memory from its shard catalogs, with no persisted indexes to build, plan over or admit |
| 603 | rejects aggregate validation counter overflow without completing the shard | internal: Elixir's index registry, planner and admission (index_registry); the node evaluates FLOW.QUERY in memory from its shard catalogs, with no persisted indexes to build, plan over or admit |
| 631 | requires the durable fence checkpoint before build or validation can advance | internal: Elixir's index registry, planner and admission (index_registry); the node evaluates FLOW.QUERY in memory from its shard catalogs, with no persisted indexes to build, plan over or admit |
| 686 | rejects validation callbacks that skip phases or index definitions | internal: Elixir's index registry, planner and admission (index_registry); the node evaluates FLOW.QUERY in memory from its shard catalogs, with no persisted indexes to build, plan over or admit |
| 718 | requires empty cursors when validation advances to another definition | internal: Elixir's index registry, planner and admission (index_registry); the node evaluates FLOW.QUERY in memory from its shard catalogs, with no persisted indexes to build, plan over or admit |
| 751 | durably resets an index validation shard after a concurrent fanout change | internal: Elixir's index registry, planner and admission (index_registry); the node evaluates FLOW.QUERY in memory from its shard catalogs, with no persisted indexes to build, plan over or admit |
| 801 | fails startup closed for an impossible pending validation position | internal: Elixir's index registry, planner and admission (index_registry); the node evaluates FLOW.QUERY in memory from its shard catalogs, with no persisted indexes to build, plan over or admit |
| 862 | fails startup closed when an active index loses a shard completion proof | internal: Elixir's index registry, planner and admission (index_registry); the node evaluates FLOW.QUERY in memory from its shard catalogs, with no persisted indexes to build, plan over or admit |
| 907 | fails startup closed for an impossible retirement checkpoint | internal: Elixir's index registry, planner and admission (index_registry); the node evaluates FLOW.QUERY in memory from its shard catalogs, with no persisted indexes to build, plan over or admit |
| 962 | catalog removal of an unfinished build remains restartable for cleanup | internal: Elixir's index registry, planner and admission (index_registry); the node evaluates FLOW.QUERY in memory from its shard catalogs, with no persisted indexes to build, plan over or admit |
| 989 | built-in index selection drops disabled catalog indexes for cleanup | internal: Elixir's index registry, planner and admission (index_registry); the node evaluates FLOW.QUERY in memory from its shard catalogs, with no persisted indexes to build, plan over or admit |
| 1012 | built-in index selection can disable every index and rejects unknown ids | internal: Elixir's index registry, planner and admission (index_registry); the node evaluates FLOW.QUERY in memory from its shard catalogs, with no persisted indexes to build, plan over or admit |
| 1033 | catalog reconciliation rejects aggregate validation counter overflow | internal: Elixir's index registry, planner and admission (index_registry); the node evaluates FLOW.QUERY in memory from its shard catalogs, with no persisted indexes to build, plan over or admit |
| 1072 | publication failure terminates and reloads the already durable transition | internal: Elixir's index registry, planner and admission (index_registry); the node evaluates FLOW.QUERY in memory from its shard catalogs, with no persisted indexes to build, plan over or admit |
| 1100 | rejects a regressing backfill cursor even when counters increase | internal: Elixir's index registry, planner and admission (index_registry); the node evaluates FLOW.QUERY in memory from its shard catalogs, with no persisted indexes to build, plan over or admit |
| 1128 | rejects a checkpoint cursor that cannot be an LMDB key | internal: Elixir's index registry, planner and admission (index_registry); the node evaluates FLOW.QUERY in memory from its shard catalogs, with no persisted indexes to build, plan over or admit |
| 1146 | fails startup closed when the durable snapshot is corrupt | internal: Elixir's index registry, planner and admission (index_registry); the node evaluates FLOW.QUERY in memory from its shard catalogs, with no persisted indexes to build, plan over or admit |
| 1163 | rejects an older beta snapshot with an actionable rebuild requirement | internal: Elixir's index registry, planner and admission (index_registry); the node evaluates FLOW.QUERY in memory from its shard catalogs, with no persisted indexes to build, plan over or admit |
| 1196 | fails startup closed when the durable epoch exceeds unsigned 64-bit | internal: Elixir's index registry, planner and admission (index_registry); the node evaluates FLOW.QUERY in memory from its shard catalogs, with no persisted indexes to build, plan over or admit |
| 1225 | refuses a symlinked durable registry snapshot | internal: Elixir's index registry, planner and admission (index_registry); the node evaluates FLOW.QUERY in memory from its shard catalogs, with no persisted indexes to build, plan over or admit |
| 1241 | keeps the old version active until its replacement is validated and activated | internal: Elixir's index registry, planner and admission (index_registry); the node evaluates FLOW.QUERY in memory from its shard catalogs, with no persisted indexes to build, plan over or admit |
| 1272 | validates and activates every index in one catalog build atomically | internal: Elixir's index registry, planner and admission (index_registry); the node evaluates FLOW.QUERY in memory from its shard catalogs, with no persisted indexes to build, plan over or admit |
| 1299 | failed validation rolls the whole candidate build back without moving the active version | internal: Elixir's index registry, planner and admission (index_registry); the node evaluates FLOW.QUERY in memory from its shard catalogs, with no persisted indexes to build, plan over or admit |
| 1331 | retirement checkpoints survive restart and remove the obsolete registry entry only at the end | internal: Elixir's index registry, planner and admission (index_registry); the node evaluates FLOW.QUERY in memory from its shard catalogs, with no persisted indexes to build, plan over or admit |
| 1389 | rejects catalog rollback and same-version catalog mutation | internal: Elixir's index registry, planner and admission (index_registry); the node evaluates FLOW.QUERY in memory from its shard catalogs, with no persisted indexes to build, plan over or admit |

## `flow/query/index_retirement_test.exs`

| Line | Test | Status |
|---|---|---|
| 26 | deletes one bounded physical page then scrubs only retired reverse keys | internal: Elixir's index registry, planner and admission (index_retirement); the node evaluates FLOW.QUERY in memory from its shard catalogs, with no persisted indexes to build, plan over or admit |
| 82 | deletes a page of more than sixteen index rows in one step | internal: Elixir's index registry, planner and admission (index_retirement); the node evaluates FLOW.QUERY in memory from its shard catalogs, with no persisted indexes to build, plan over or admit |
| 120 | replaying a page after its write is idempotent | internal: Elixir's index registry, planner and admission (index_retirement); the node evaluates FLOW.QUERY in memory from its shard catalogs, with no persisted indexes to build, plan over or admit |
| 159 | retries a reverse rewrite lost to a concurrent projection | internal: Elixir's index registry, planner and admission (index_retirement); the node evaluates FLOW.QUERY in memory from its shard catalogs, with no persisted indexes to build, plan over or admit |
| 205 | rejects a forged index definition before retirement work | internal: Elixir's index registry, planner and admission (index_retirement); the node evaluates FLOW.QUERY in memory from its shard catalogs, with no persisted indexes to build, plan over or admit |
| 220 | rejects retirement checkpoint counters outside the durable u64 domain | internal: Elixir's index registry, planner and admission (index_retirement); the node evaluates FLOW.QUERY in memory from its shard catalogs, with no persisted indexes to build, plan over or admit |
| 232 | rejects a retirement cursor larger than an LMDB key | internal: Elixir's index registry, planner and admission (index_retirement); the node evaluates FLOW.QUERY in memory from its shard catalogs, with no persisted indexes to build, plan over or admit |
| 240 | rejects retirement counter overflow before deleting an index page | internal: Elixir's index registry, planner and admission (index_retirement); the node evaluates FLOW.QUERY in memory from its shard catalogs, with no persisted indexes to build, plan over or admit |
| 271 | rejects false page byte accounting before retirement writes | internal: Elixir's index registry, planner and admission (index_retirement); the node evaluates FLOW.QUERY in memory from its shard catalogs, with no persisted indexes to build, plan over or admit |
| 300 | rejects an oversized retirement page key before writes | internal: Elixir's index registry, planner and admission (index_retirement); the node evaluates FLOW.QUERY in memory from its shard catalogs, with no persisted indexes to build, plan over or admit |
| 332 | rejects a retirement page that regresses its persisted cursor | internal: Elixir's index registry, planner and admission (index_retirement); the node evaluates FLOW.QUERY in memory from its shard catalogs, with no persisted indexes to build, plan over or admit |

## `flow/query/index_status_test.exs`

| Line | Test | Status |
|---|---|---|
| 38 | returns one actionable catalog, progress, service, and freshness view | internal: Elixir's index build status view (catalog, progress, freshness); the node has no persisted indexes, and its FLOW.QUERY.INDEXES answers what it evaluates |
| 106 | filters by index id and reports an unknown generation | internal: Elixir's index build status view (catalog, progress, freshness); the node has no persisted indexes, and its FLOW.QUERY.INDEXES answers what it evaluates |
| 117 | accepts a registry pid as a server reference | internal: Elixir's index build status view (catalog, progress, freshness); the node has no persisted indexes, and its FLOW.QUERY.INDEXES answers what it evaluates |
| 124 | classifies exact freshness boundaries, stale samples, and future clocks | internal: Elixir's index build status view (catalog, progress, freshness); the node has no persisted indexes, and its FLOW.QUERY.INDEXES answers what it evaluates |
| 163 | distinguishes missing samples from an unavailable statistics service | internal: Elixir's index build status view (catalog, progress, freshness); the node has no persisted indexes, and its FLOW.QUERY.INDEXES answers what it evaluates |
| 182 | validates index filters at the wire-size boundary | internal: Elixir's index build status view (catalog, progress, freshness); the node has no persisted indexes, and its FLOW.QUERY.INDEXES answers what it evaluates |
| 196 | projects build completion and validation failure as one actionable lifecycle | internal: Elixir's index build status view (catalog, progress, freshness); the node has no persisted indexes, and its FLOW.QUERY.INDEXES answers what it evaluates |

## `flow/query/index_validation_test.exs`

| Line | Test | Status |
|---|---|---|
| 38 | proves source coverage and physical ownership in bounded resumable passes | internal: Elixir's index registry, planner and admission (index_validation); the node evaluates FLOW.QUERY in memory from its shard catalogs, with no persisted indexes to build, plan over or admit |
| 90 | checks a page of small records in one source step | internal: Elixir's index registry, planner and admission (index_validation); the node evaluates FLOW.QUERY in memory from its shard catalogs, with no persisted indexes to build, plan over or admit |
| 128 | validates shared composite rows from opaque QueryRow routing scope | internal: Elixir's index registry, planner and admission (index_validation); the node evaluates FLOW.QUERY in memory from its shard catalogs, with no persisted indexes to build, plan over or admit |
| 191 | validates each definition once per index page instead of once per row | internal: Elixir's index registry, planner and admission (index_validation); the node evaluates FLOW.QUERY in memory from its shard catalogs, with no persisted indexes to build, plan over or admit |
| 244 | rejects a corrupt exact counter before activating its index | internal: Elixir's index registry, planner and admission (index_validation); the node evaluates FLOW.QUERY in memory from its shard catalogs, with no persisted indexes to build, plan over or admit |
| 286 | resumes exact counter aggregation without rescanning prior index pages | internal: Elixir's index registry, planner and admission (index_validation); the node evaluates FLOW.QUERY in memory from its shard catalogs, with no persisted indexes to build, plan over or admit |
| 325 | counts one record across trailing multivalue index fanout when validation resumes | internal: Elixir's index registry, planner and admission (index_validation); the node evaluates FLOW.QUERY in memory from its shard catalogs, with no persisted indexes to build, plan over or admit |
| 376 | accepts trailing multivalue counter growth behind a resumable validation cursor | internal: Elixir's index registry, planner and admission (index_validation); the node evaluates FLOW.QUERY in memory from its shard catalogs, with no persisted indexes to build, plan over or admit |
| 448 | rejects a corrupt physical counter even when logical membership agrees | internal: Elixir's index registry, planner and admission (index_validation); the node evaluates FLOW.QUERY in memory from its shard catalogs, with no persisted indexes to build, plan over or admit |
| 480 | rejects a counter that omits expiring index membership | internal: Elixir's index registry, planner and admission (index_validation); the node evaluates FLOW.QUERY in memory from its shard catalogs, with no persisted indexes to build, plan over or admit |
| 526 | accepts consistent counter growth behind a resumable validation cursor | internal: Elixir's index registry, planner and admission (index_validation); the node evaluates FLOW.QUERY in memory from its shard catalogs, with no persisted indexes to build, plan over or admit |
| 575 | rejects an orphan exact counter during the bounded counter inventory pass | internal: Elixir's index registry, planner and admission (index_validation); the node evaluates FLOW.QUERY in memory from its shard catalogs, with no persisted indexes to build, plan over or admit |
| 603 | retries when a counter and its final index row disappear between inventory reads | internal: Elixir's index registry, planner and admission (index_validation); the node evaluates FLOW.QUERY in memory from its shard catalogs, with no persisted indexes to build, plan over or admit |
| 640 | reports a missing expected row without exposing record or tenant values | internal: Elixir's index registry, planner and admission (index_validation); the node evaluates FLOW.QUERY in memory from its shard catalogs, with no persisted indexes to build, plan over or admit |
| 665 | rejects self-consistent projections owned by the wrong state key | internal: Elixir's index registry, planner and admission (index_validation); the node evaluates FLOW.QUERY in memory from its shard catalogs, with no persisted indexes to build, plan over or admit |
| 708 | reports an orphan physical row during the index-to-source pass | internal: Elixir's index registry, planner and admission (index_validation); the node evaluates FLOW.QUERY in memory from its shard catalogs, with no persisted indexes to build, plan over or admit |
| 732 | retries rather than misclassifying a state changed during validation | internal: Elixir's index registry, planner and admission (index_validation); the node evaluates FLOW.QUERY in memory from its shard catalogs, with no persisted indexes to build, plan over or admit |
| 762 | retries when a physical row changes after its range snapshot | internal: Elixir's index registry, planner and admission (index_validation); the node evaluates FLOW.QUERY in memory from its shard catalogs, with no persisted indexes to build, plan over or admit |
| 793 | rejects an oversized index-page key before validation reads | internal: Elixir's index registry, planner and admission (index_validation); the node evaluates FLOW.QUERY in memory from its shard catalogs, with no persisted indexes to build, plan over or admit |
| 828 | passes the byte ceiling into bounded primary validation reads | internal: Elixir's index registry, planner and admission (index_validation); the node evaluates FLOW.QUERY in memory from its shard catalogs, with no persisted indexes to build, plan over or admit |
| 856 | rejects a forged index definition before validation work | internal: Elixir's index registry, planner and admission (index_validation); the node evaluates FLOW.QUERY in memory from its shard catalogs, with no persisted indexes to build, plan over or admit |
| 865 | validation instrumentation cannot authorize a forged definition | internal: Elixir's index registry, planner and admission (index_validation); the node evaluates FLOW.QUERY in memory from its shard catalogs, with no persisted indexes to build, plan over or admit |
| 882 | rejects a cleanup checkpoint that skipped index validation | internal: Elixir's index registry, planner and admission (index_validation); the node evaluates FLOW.QUERY in memory from its shard catalogs, with no persisted indexes to build, plan over or admit |
| 898 | rejects an unbounded primary-read callback | internal: Elixir's index registry, planner and admission (index_validation); the node evaluates FLOW.QUERY in memory from its shard catalogs, with no persisted indexes to build, plan over or admit |
| 915 | rejects malformed or unknown validation options as an invalid request | internal: Elixir's index registry, planner and admission (index_validation); the node evaluates FLOW.QUERY in memory from its shard catalogs, with no persisted indexes to build, plan over or admit |
| 943 | rejects duplicate definition identities before reading validation pages | internal: Elixir's index registry, planner and admission (index_validation); the node evaluates FLOW.QUERY in memory from its shard catalogs, with no persisted indexes to build, plan over or admit |
| 975 | caps source pages so one entry per definition per record fits the read-key budget | internal: Elixir's index registry, planner and admission (index_validation); the node evaluates FLOW.QUERY in memory from its shard catalogs, with no persisted indexes to build, plan over or admit |
| 1018 | reports a source page whose reads exceed the byte budget so it can be retried smaller | internal: Elixir's index registry, planner and admission (index_validation); the node evaluates FLOW.QUERY in memory from its shard catalogs, with no persisted indexes to build, plan over or admit |
| 1032 | rejects validation checkpoint counters outside the durable u64 domain | internal: Elixir's index registry, planner and admission (index_validation); the node evaluates FLOW.QUERY in memory from its shard catalogs, with no persisted indexes to build, plan over or admit |
| 1045 | rejects a validation cursor larger than an LMDB key | internal: Elixir's index registry, planner and admission (index_validation); the node evaluates FLOW.QUERY in memory from its shard catalogs, with no persisted indexes to build, plan over or admit |
| 1058 | rejects a source counter overflow before primary validation reads | internal: Elixir's index registry, planner and admission (index_validation); the node evaluates FLOW.QUERY in memory from its shard catalogs, with no persisted indexes to build, plan over or admit |
| 1101 | rejects an index counter overflow before primary validation reads | internal: Elixir's index registry, planner and admission (index_validation); the node evaluates FLOW.QUERY in memory from its shard catalogs, with no persisted indexes to build, plan over or admit |
| 1133 | rejects false staging byte accounting before primary reads | internal: Elixir's index registry, planner and admission (index_validation); the node evaluates FLOW.QUERY in memory from its shard catalogs, with no persisted indexes to build, plan over or admit |
| 1172 | rejects false index byte accounting before primary reads | internal: Elixir's index registry, planner and admission (index_validation); the node evaluates FLOW.QUERY in memory from its shard catalogs, with no persisted indexes to build, plan over or admit |
| 1204 | rejects an index page that regresses its persisted cursor | internal: Elixir's index registry, planner and admission (index_validation); the node evaluates FLOW.QUERY in memory from its shard catalogs, with no persisted indexes to build, plan over or admit |

## `flow/query/limits_test.exs`

| Line | Test | Status |
|---|---|---|
| 7 | derives point-value limits from the generated storage key | covered: `query::reference_bind_tests::bound_ids_stop_at_the_storage_key_bounds` |
| 23 | publishes the bounded query contract from one module | internal: Elixir's limits module |

## `flow/query/lineage_executor_test.exs`

| Line | Test | Status |
|---|---|---|
| 19 | issues an authenticated tuple cursor and resumes from its exclusive boundary | internal: Elixir's index paging executors (a page resuming after its last row: `query::tests::a_cursor_resumes_its_own_query_after_the_last_row`) |
| 107 | projects lineage output after validating identity and order fields | internal: Elixir's index paging executors (a page resuming after its last row: `query::tests::a_cursor_resumes_its_own_query_after_the_last_row`) |
| 141 | bare lineage returns do not reserve a duplicate output page | internal: Elixir's index paging executors (a page resuming after its last row: `query::tests::a_cursor_resumes_its_own_query_after_the_last_row`) |
| 177 | rejects lineage pages containing fields outside the public result allowlist | internal: Elixir's index paging executors (a page resuming after its last row: `query::tests::a_cursor_resumes_its_own_query_after_the_last_row`) |
| 208 | rejects forged page accounting and malformed continuation tuples | internal: Elixir's index paging executors (a page resuming after its last row: `query::tests::a_cursor_resumes_its_own_query_after_the_last_row`) |
| 253 | rejects a page whose decoded-record peak exceeds the executor budget | internal: Elixir's index paging executors (a page resuming after its last row: `query::tests::a_cursor_resumes_its_own_query_after_the_last_row`) |
| 279 | rejects resumed pages that do not advance past the authenticated lineage boundary | internal: Elixir's index paging executors (a page resuming after its last row: `query::tests::a_cursor_resumes_its_own_query_after_the_last_row`) |

## `flow/query/lineage_read_test.exs`

| Line | Test | Status |
|---|---|---|
| 9 | correlation pages seek backward by the exact update tuple | internal: Elixir's index paging executors (a page resuming after its last row: `query::tests::a_cursor_resumes_its_own_query_after_the_last_row`) |
| 69 | expired cold rows consume the scan window without query-side deletion | internal: Elixir's index paging executors (a page resuming after its last row: `query::tests::a_cursor_resumes_its_own_query_after_the_last_row`) |
| 125 | dangling lineage index references fail closed | internal: Elixir's index paging executors (a page resuming after its last row: `query::tests::a_cursor_resumes_its_own_query_after_the_last_row`) |
| 162 | a dangling lookahead cannot produce a false has-more cursor | internal: Elixir's index paging executors (a page resuming after its last row: `query::tests::a_cursor_resumes_its_own_query_after_the_last_row`) |
| 208 | lineage boundaries reject timestamps outside the exact ordered-index domain | internal: Elixir's index paging executors (a page resuming after its last row: `query::tests::a_cursor_resumes_its_own_query_after_the_last_row`) |
| 226 | an existing root with an inexact ordering timestamp fails closed | internal: Elixir's index paging executors (a page resuming after its last row: `query::tests::a_cursor_resumes_its_own_query_after_the_last_row`) |

## `flow/query/mandatory_scope_test.exs`

| Line | Test | Status |
|---|---|---|
| 44 | dedicated scope adds no storage bytes | internal: Elixir's embedding storage scope (metadata extension) |
| 64 | shared scope is hidden, canonical, and tenant-partitions physical storage | internal: Elixir's embedding storage scope (metadata extension) |
| 141 | missing shared authority fails closed | internal: Elixir's embedding storage scope (metadata extension) |
| 148 | forged scope values fail closed | internal: Elixir's embedding storage scope (metadata extension) |
| 164 | a structurally valid scope is rejected under a different frozen schema | internal: Elixir's embedding storage scope (metadata extension) |

## `flow/query/memory_budget_test.exs`

| Line | Test | Status |
|---|---|---|
| 7 | conservatively accounts for decoded Flow record heap | internal: Elixir's persisted query engine (memory_budget); the node evaluates FLOW.QUERY in memory from its shard catalogs (ADR-014 step 12) |
| 18 | reserves storage and worst-case decode expansion before hydration | internal: Elixir's persisted query engine (memory_budget); the node evaluates FLOW.QUERY in memory from its shard catalogs (ADR-014 step 12) |

## `flow/query/page_memory_test.exs`

| Line | Test | Status |
|---|---|---|
| 15 | retained page accounting covers decoded maps instead of trusting storage bytes | internal: Elixir's persisted query engine (page_memory); the node evaluates FLOW.QUERY in memory from its shard catalogs (ADR-014 step 12) |
| 35 | planner estimate includes covering schema expansion | internal: Elixir's persisted query engine (page_memory); the node evaluates FLOW.QUERY in memory from its shard catalogs (ADR-014 step 12) |
| 43 | fused prefetch estimate reserves a QueryRow reference for every possible entry | internal: Elixir's persisted query engine (page_memory); the node evaluates FLOW.QUERY in memory from its shard catalogs (ADR-014 step 12) |
| 50 | constant-time accounting bounds a full cover with the longest valid identity | internal: Elixir's persisted query engine (page_memory); the node evaluates FLOW.QUERY in memory from its shard catalogs (ADR-014 step 12) |

## `flow/query/planner_boundary_test.exs`

| Line | Test | Status |
|---|---|---|
| 6 | the OSS planner selects a bounded composite index for a general collection query | internal: Elixir's index registry, planner and admission (planner_boundary); the node evaluates FLOW.QUERY in memory from its shard catalogs, with no persisted indexes to build, plan over or admit |
| 44 | the OSS planner retains a bounded fixed-index plan while composite indexes build | internal: Elixir's index registry, planner and admission (planner_boundary); the node evaluates FLOW.QUERY in memory from its shard catalogs, with no persisted indexes to build, plan over or admit |
| 67 | the fixed-index fallback obeys hydration and result budgets | internal: Elixir's index registry, planner and admission (planner_boundary); the node evaluates FLOW.QUERY in memory from its shard catalogs, with no persisted indexes to build, plan over or admit |

## `flow/query/planner_engine_test.exs`

| Line | Test | Status |
|---|---|---|
| 71 | OSS publishes the complete parser-supported query surface | internal: Elixir's index registry, planner and admission (planner_engine); the node evaluates FLOW.QUERY in memory from its shard catalogs, with no persisted indexes to build, plan over or admit |
| 109 | plans and executes through the active registry with one bounded response contract | internal: Elixir's index registry, planner and admission (planner_engine); the node evaluates FLOW.QUERY in memory from its shard catalogs, with no persisted indexes to build, plan over or admit |
| 129 | settles engine usage with the trusted negotiated response codec | internal: Elixir's index registry, planner and admission (planner_engine); the node evaluates FLOW.QUERY in memory from its shard catalogs, with no persisted indexes to build, plan over or admit |
| 147 | emits one bounded telemetry event after an admitted query completes | internal: Elixir's index registry, planner and admission (planner_engine); the node evaluates FLOW.QUERY in memory from its shard catalogs, with no persisted indexes to build, plan over or admit |
| 178 | releases admission before invoking synchronous telemetry handlers | internal: Elixir's index registry, planner and admission (planner_engine); the node evaluates FLOW.QUERY in memory from its shard catalogs, with no persisted indexes to build, plan over or admit |
| 206 | emits bounded telemetry when admission rejects the query | internal: Elixir's index registry, planner and admission (planner_engine); the node evaluates FLOW.QUERY in memory from its shard catalogs, with no persisted indexes to build, plan over or admit |
| 242 | static EXPLAIN does not enqueue deferred statistics I/O | internal: Elixir's index registry, planner and admission (planner_engine); the node evaluates FLOW.QUERY in memory from its shard catalogs, with no persisted indexes to build, plan over or admit |
| 253 | explains the missing predicate and index when no bounded plan exists | internal: Elixir's index registry, planner and admission (planner_engine); the node evaluates FLOW.QUERY in memory from its shard catalogs, with no persisted indexes to build, plan over or admit |
| 306 | keeps missing-index recommendations invariant under predicate reordering | internal: Elixir's index registry, planner and admission (planner_engine); the node evaluates FLOW.QUERY in memory from its shard catalogs, with no persisted indexes to build, plan over or admit |
| 344 | missing-index diagnostics depend on query shape rather than literal values | internal: Elixir's index registry, planner and admission (planner_engine); the node evaluates FLOW.QUERY in memory from its shard catalogs, with no persisted indexes to build, plan over or admit |
| 368 | suggests equality fields before ranges and preserves requested range ordering | internal: Elixir's index registry, planner and admission (planner_engine); the node evaluates FLOW.QUERY in memory from its shard catalogs, with no persisted indexes to build, plan over or admit |
| 403 | suggests an exact counter prefix for an unplanned count | internal: Elixir's index registry, planner and admission (planner_engine); the node evaluates FLOW.QUERY in memory from its shard catalogs, with no persisted indexes to build, plan over or admit |
| 454 | does not suggest an inexact counter when predicates exceed the index field bound | internal: Elixir's index registry, planner and admission (planner_engine); the node evaluates FLOW.QUERY in memory from its shard catalogs, with no persisted indexes to build, plan over or admit |
| 478 | keeps suggested definitions within the single multivalue field limit | internal: Elixir's index registry, planner and admission (planner_engine); the node evaluates FLOW.QUERY in memory from its shard catalogs, with no persisted indexes to build, plan over or admit |
| 511 | does not suggest exact counters for overlapping multivalue IN predicates | internal: Elixir's index registry, planner and admission (planner_engine); the node evaluates FLOW.QUERY in memory from its shard catalogs, with no persisted indexes to build, plan over or admit |
| 528 | suggests exact counters only for disjoint exact predicate shapes | internal: Elixir's index registry, planner and admission (planner_engine); the node evaluates FLOW.QUERY in memory from its shard catalogs, with no persisted indexes to build, plan over or admit |
| 578 | keeps maximum-size recommendations valid, bounded, and value-redacted | internal: Elixir's index registry, planner and admission (planner_engine); the node evaluates FLOW.QUERY in memory from its shard catalogs, with no persisted indexes to build, plan over or admit |
| 630 | does not execute an index fenced after the active snapshot was published | internal: Elixir's index registry, planner and admission (planner_engine); the node evaluates FLOW.QUERY in memory from its shard catalogs, with no persisted indexes to build, plan over or admit |
| 648 | executes a scalar count through the active bounded index without row pagination | internal: Elixir's index registry, planner and admission (planner_engine); the node evaluates FLOW.QUERY in memory from its shard catalogs, with no persisted indexes to build, plan over or admit |
| 669 | returns an exact zero count for a logically empty window without registry access | internal: Elixir's index registry, planner and admission (planner_engine); the node evaluates FLOW.QUERY in memory from its shard catalogs, with no persisted indexes to build, plan over or admit |
| 684 | shared authorities query the same logical partition through disjoint physical plans | internal: Elixir's index registry, planner and admission (planner_engine); the node evaluates FLOW.QUERY in memory from its shard catalogs, with no persisted indexes to build, plan over or admit |
| 701 | EXPLAIN is deterministic and does not expose bound values or physical keys | internal: Elixir's index registry, planner and admission (planner_engine); the node evaluates FLOW.QUERY in memory from its shard catalogs, with no persisted indexes to build, plan over or admit |
| 721 | EXPLAIN ANALYZE executes the bounded plan and returns usage without records | internal: Elixir's index registry, planner and admission (planner_engine); the node evaluates FLOW.QUERY in memory from its shard catalogs, with no persisted indexes to build, plan over or admit |
| 749 | EXPLAIN ANALYZE reads usage from compact prepared execution responses | internal: Elixir's index registry, planner and admission (planner_engine); the node evaluates FLOW.QUERY in memory from its shard catalogs, with no persisted indexes to build, plan over or admit |
| 768 | EXPLAIN ANALYZE executes scalar counts without returning the count value | internal: Elixir's index registry, planner and admission (planner_engine); the node evaluates FLOW.QUERY in memory from its shard catalogs, with no persisted indexes to build, plan over or admit |
| 785 | EXPLAIN ANALYZE keeps rejected plans non-executing and actionable | internal: Elixir's index registry, planner and admission (planner_engine); the node evaluates FLOW.QUERY in memory from its shard catalogs, with no persisted indexes to build, plan over or admit |
| 799 | the public FQL1 boundary exposes EXPLAIN ANALYZE end to end | internal: Elixir's index registry, planner and admission (planner_engine); the node evaluates FLOW.QUERY in memory from its shard catalogs, with no persisted indexes to build, plan over or admit |
| 819 | the public FQL1 boundary preserves actionable planner diagnostics | internal: Elixir's index registry, planner and admission (planner_engine); the node evaluates FLOW.QUERY in memory from its shard catalogs, with no persisted indexes to build, plan over or admit |
| 850 | EXPLAIN ANALYZE covers point, history, and lineage operators | internal: Elixir's index registry, planner and admission (planner_engine); the node evaluates FLOW.QUERY in memory from its shard catalogs, with no persisted indexes to build, plan over or admit |
| 935 | dedicated mode does not confuse request authority with a user partition | internal: Elixir's index registry, planner and admission (planner_engine); the node evaluates FLOW.QUERY in memory from its shard catalogs, with no persisted indexes to build, plan over or admit |
| 954 | shared mode fails closed when server authority is missing | internal: Elixir's index registry, planner and admission (planner_engine); the node evaluates FLOW.QUERY in memory from its shard catalogs, with no persisted indexes to build, plan over or admit |
| 966 | shared admission is tenant-wide and isolated from the logical partition | internal: Elixir's index registry, planner and admission (planner_engine); the node evaluates FLOW.QUERY in memory from its shard catalogs, with no persisted indexes to build, plan over or admit |
| 990 | rejects an expired propagated deadline before registry access | internal: Elixir's index registry, planner and admission (planner_engine); the node evaluates FLOW.QUERY in memory from its shard catalogs, with no persisted indexes to build, plan over or admit |
| 1003 | enforces the instance scope and node permit before planning | internal: Elixir's index registry, planner and admission (planner_engine); the node evaluates FLOW.QUERY in memory from its shard catalogs, with no persisted indexes to build, plan over or admit |
| 1013 | resizes admission to the planned working set before storage access | internal: Elixir's index registry, planner and admission (planner_engine); the node evaluates FLOW.QUERY in memory from its shard catalogs, with no persisted indexes to build, plan over or admit |
| 1032 | composite execution reserves its hard memory ceiling rather than an average estimate | internal: Elixir's index registry, planner and admission (planner_engine); the node evaluates FLOW.QUERY in memory from its shard catalogs, with no persisted indexes to build, plan over or admit |
| 1045 | keeps the primary-key EXPLAIN path independent of the composite registry | internal: Elixir's index registry, planner and admission (planner_engine); the node evaluates FLOW.QUERY in memory from its shard catalogs, with no persisted indexes to build, plan over or admit |
| 1066 | shared run-id-only point EXPLAIN derives a tenant-contained auto partition | internal: Elixir's index registry, planner and admission (planner_engine); the node evaluates FLOW.QUERY in memory from its shard catalogs, with no persisted indexes to build, plan over or admit |
| 1092 | shared event history EXPLAIN stays on the tenant-contained direct index | internal: Elixir's index registry, planner and admission (planner_engine); the node evaluates FLOW.QUERY in memory from its shard catalogs, with no persisted indexes to build, plan over or admit |
| 1110 | shared parent lineage EXPLAIN bypasses the composite registry | internal: Elixir's index registry, planner and admission (planner_engine); the node evaluates FLOW.QUERY in memory from its shard catalogs, with no persisted indexes to build, plan over or admit |
| 1131 | event history executes as bounded authenticated pages in global order | internal: Elixir's index registry, planner and admission (planner_engine); the node evaluates FLOW.QUERY in memory from its shard catalogs, with no persisted indexes to build, plan over or admit |
| 1165 | event history cursors bind to the canonical projection set | internal: Elixir's index registry, planner and admission (planner_engine); the node evaluates FLOW.QUERY in memory from its shard catalogs, with no persisted indexes to build, plan over or admit |
| 1202 | parent lineage executes as bounded authenticated tuple pages | internal: Elixir's index registry, planner and admission (planner_engine); the node evaluates FLOW.QUERY in memory from its shard catalogs, with no persisted indexes to build, plan over or admit |
| 1256 | root lineage includes its root and reports the additional point probe | internal: Elixir's index registry, planner and admission (planner_engine); the node evaluates FLOW.QUERY in memory from its shard catalogs, with no persisted indexes to build, plan over or admit |
| 1314 | shared lineage execution isolates authorities and returns logical partitions | internal: Elixir's index registry, planner and admission (planner_engine); the node evaluates FLOW.QUERY in memory from its shard catalogs, with no persisted indexes to build, plan over or admit |
| 1368 | fixed-index fallback paginates without truncating records | internal: Elixir's index registry, planner and admission (planner_engine); the node evaluates FLOW.QUERY in memory from its shard catalogs, with no persisted indexes to build, plan over or admit |
| 1427 | fixed-index fallback cursor preserves timestamp tie ordering | internal: Elixir's index registry, planner and admission (planner_engine); the node evaluates FLOW.QUERY in memory from its shard catalogs, with no persisted indexes to build, plan over or admit |
| 1475 | fixed inflight-index fallback paginates by lease deadline | internal: Elixir's index registry, planner and admission (planner_engine); the node evaluates FLOW.QUERY in memory from its shard catalogs, with no persisted indexes to build, plan over or admit |
| 1541 | executes a logically empty half-open window without registry availability | internal: Elixir's index registry, planner and admission (planner_engine); the node evaluates FLOW.QUERY in memory from its shard catalogs, with no persisted indexes to build, plan over or admit |
| 1562 | keeps a continuation on its authenticated index when statistics change | internal: Elixir's index registry, planner and admission (planner_engine); the node evaluates FLOW.QUERY in memory from its shard catalogs, with no persisted indexes to build, plan over or admit |
| 1622 | rejects a continuation whose authenticated index generation is no longer active | internal: Elixir's index registry, planner and admission (planner_engine); the node evaluates FLOW.QUERY in memory from its shard catalogs, with no persisted indexes to build, plan over or admit |
| 1645 | rejects a continuation from a replaced build with the same index id and version | internal: Elixir's index registry, planner and admission (planner_engine); the node evaluates FLOW.QUERY in memory from its shard catalogs, with no persisted indexes to build, plan over or admit |
| 1670 | reports the current budget rejection for a valid pinned continuation | internal: Elixir's index registry, planner and admission (planner_engine); the node evaluates FLOW.QUERY in memory from its shard catalogs, with no persisted indexes to build, plan over or admit |

## `flow/query/planner_test.exs`

| Line | Test | Status |
|---|---|---|
| 62 | run-id-only point reads use the primary key without a composite scan | internal: Elixir's index registry, planner and admission (planner); the node evaluates FLOW.QUERY in memory from its shard catalogs, with no persisted indexes to build, plan over or admit |
| 81 | event history uses the direct history index without registry candidates | internal: Elixir's index registry, planner and admission (planner); the node evaluates FLOW.QUERY in memory from its shard catalogs, with no persisted indexes to build, plan over or admit |
| 130 | partition-contained parent lineage uses its built-in index | internal: Elixir's index registry, planner and admission (planner); the node evaluates FLOW.QUERY in memory from its shard catalogs, with no persisted indexes to build, plan over or admit |
| 165 | plans the bounded lease-deadline workload through the launch index | internal: Elixir's index registry, planner and admission (planner); the node evaluates FLOW.QUERY in memory from its shard catalogs, with no persisted indexes to build, plan over or admit |
| 193 | root lineage accounts for its authoritative root probe | internal: Elixir's index registry, planner and admission (planner); the node evaluates FLOW.QUERY in memory from its shard catalogs, with no persisted indexes to build, plan over or admit |
| 230 | plans a selective equality plus half-open time window as one bounded range | internal: Elixir's index registry, planner and admission (planner); the node evaluates FLOW.QUERY in memory from its shard catalogs, with no persisted indexes to build, plan over or admit |
| 274 | classifies projected and bare collection records as compact-row execution | internal: Elixir's index registry, planner and admission (planner); the node evaluates FLOW.QUERY in memory from its shard catalogs, with no persisted indexes to build, plan over or admit |
| 330 | prefers a covering index when scan cardinality is otherwise equal | internal: Elixir's index registry, planner and admission (planner); the node evaluates FLOW.QUERY in memory from its shard catalogs, with no persisted indexes to build, plan over or admit |
| 384 | plans scalar counts as bounded unordered scans with one scalar result | internal: Elixir's index registry, planner and admission (planner); the node evaluates FLOW.QUERY in memory from its shard catalogs, with no persisted indexes to build, plan over or admit |
| 445 | routes a fully covered scalar count to transactional prefix counters | internal: Elixir's index registry, planner and admission (planner); the node evaluates FLOW.QUERY in memory from its shard catalogs, with no persisted indexes to build, plan over or admit |
| 515 | does not sum multivalue IN counters that could double-count one run | internal: Elixir's index registry, planner and admission (planner); the node evaluates FLOW.QUERY in memory from its shard catalogs, with no persisted indexes to build, plan over or admit |
| 538 | expands bounded IN to an ordered range union without intersecting scans | internal: Elixir's index registry, planner and admission (planner); the node evaluates FLOW.QUERY in memory from its shard catalogs, with no persisted indexes to build, plan over or admit |
| 570 | predicate and IN value ordering do not change the physical plan | internal: Elixir's index registry, planner and admission (planner); the node evaluates FLOW.QUERY in memory from its shard catalogs, with no persisted indexes to build, plan over or admit |
| 613 | contradictory exact predicates remain residual and cannot use counters | internal: Elixir's index registry, planner and admission (planner); the node evaluates FLOW.QUERY in memory from its shard catalogs, with no persisted indexes to build, plan over or admit |
| 644 | chooses the lowest-cost selective index and uses a stable logical-id tie break | internal: Elixir's index registry, planner and admission (planner); the node evaluates FLOW.QUERY in memory from its shard catalogs, with no persisted indexes to build, plan over or admit |
| 683 | prefers the fully constrained native-order index when estimates are unavailable | internal: Elixir's index registry, planner and admission (planner); the node evaluates FLOW.QUERY in memory from its shard catalogs, with no persisted indexes to build, plan over or admit |
| 706 | does not prefer native order over a much cheaper bounded top-K index | internal: Elixir's index registry, planner and admission (planner); the node evaluates FLOW.QUERY in memory from its shard catalogs, with no persisted indexes to build, plan over or admit |
| 744 | missing and stale statistics stay pessimistic | internal: Elixir's index registry, planner and admission (planner); the node evaluates FLOW.QUERY in memory from its shard catalogs, with no persisted indexes to build, plan over or admit |
| 764 | forged statistics cannot bypass planner byte budgets | internal: Elixir's index registry, planner and admission (planner); the node evaluates FLOW.QUERY in memory from its shard catalogs, with no persisted indexes to build, plan over or admit |
| 783 | a failing statistics provider falls back to pessimistic planning | internal: Elixir's index registry, planner and admission (planner); the node evaluates FLOW.QUERY in memory from its shard catalogs, with no persisted indexes to build, plan over or admit |
| 799 | returns deterministic bounded probes only for the selected candidate | internal: Elixir's index registry, planner and admission (planner); the node evaluates FLOW.QUERY in memory from its shard catalogs, with no persisted indexes to build, plan over or admit |
| 834 | does not probe prefixes already covered by fresh exact statistics | internal: Elixir's index registry, planner and admission (planner); the node evaluates FLOW.QUERY in memory from its shard catalogs, with no persisted indexes to build, plan over or admit |
| 848 | histogram selectivity never lowers the hard prefix bound | internal: Elixir's index registry, planner and admission (planner); the node evaluates FLOW.QUERY in memory from its shard catalogs, with no persisted indexes to build, plan over or admit |
| 878 | a fresh mutable prefix count never lowers the execution hard bound | internal: Elixir's index registry, planner and admission (planner); the node evaluates FLOW.QUERY in memory from its shard catalogs, with no persisted indexes to build, plan over or admit |
| 896 | keeps an unsupported hash-field range as a bounded residual | internal: Elixir's index registry, planner and admission (planner); the node evaluates FLOW.QUERY in memory from its shard catalogs, with no persisted indexes to build, plan over or admit |
| 920 | never selects partial, non-active, or validation-pending generations | internal: Elixir's index registry, planner and admission (planner); the node evaluates FLOW.QUERY in memory from its shard catalogs, with no persisted indexes to build, plan over or admit |
| 943 | rejects range explosion before constructing physical keys | internal: Elixir's index registry, planner and admission (planner); the node evaluates FLOW.QUERY in memory from its shard catalogs, with no persisted indexes to build, plan over or admit |
| 969 | represents a non-native numeric order as bounded sorting | internal: Elixir's index registry, planner and admission (planner); the node evaluates FLOW.QUERY in memory from its shard catalogs, with no persisted indexes to build, plan over or admit |
| 989 | never treats hashed fields or unrequested trailing fields as native order | internal: Elixir's index registry, planner and admission (planner); the node evaluates FLOW.QUERY in memory from its shard catalogs, with no persisted indexes to build, plan over or admit |
| 1028 | rejects a plan that exceeds the planner memory ceiling | internal: Elixir's index registry, planner and admission (planner); the node evaluates FLOW.QUERY in memory from its shard catalogs, with no persisted indexes to build, plan over or admit |
| 1042 | includes bounded page and hydration peaks in executor memory estimates | internal: Elixir's index registry, planner and admission (planner); the node evaluates FLOW.QUERY in memory from its shard catalogs, with no persisted indexes to build, plan over or admit |
| 1056 | accounts for prepared residual membership in executor memory | internal: Elixir's index registry, planner and admission (planner); the node evaluates FLOW.QUERY in memory from its shard catalogs, with no persisted indexes to build, plan over or admit |
| 1080 | an empty half-open time window returns an empty plan without a read | internal: Elixir's index registry, planner and admission (planner); the node evaluates FLOW.QUERY in memory from its shard catalogs, with no persisted indexes to build, plan over or admit |
| 1094 | all generated ranges remain inside the bound tenant prefix | internal: Elixir's index registry, planner and admission (planner); the node evaluates FLOW.QUERY in memory from its shard catalogs, with no persisted indexes to build, plan over or admit |
| 1122 | point requests preserve the primary-key fast path | internal: Elixir's index registry, planner and admission (planner); the node evaluates FLOW.QUERY in memory from its shard catalogs, with no persisted indexes to build, plan over or admit |
| 1133 | rejects forged budget structs at the planner boundary | internal: Elixir's index registry, planner and admission (planner); the node evaluates FLOW.QUERY in memory from its shard catalogs, with no persisted indexes to build, plan over or admit |
| 1141 | does not plan against an active entry with a forged definition | internal: Elixir's index registry, planner and admission (planner); the node evaluates FLOW.QUERY in memory from its shard catalogs, with no persisted indexes to build, plan over or admit |
| 1157 | EXPLAIN, ANALYZE, and execution share one value-redacted query fingerprint | internal: Elixir's index registry, planner and admission (planner); the node evaluates FLOW.QUERY in memory from its shard catalogs, with no persisted indexes to build, plan over or admit |
| 1174 | result projection is bound into query fingerprints and cursor digests | internal: Elixir's index registry, planner and admission (planner); the node evaluates FLOW.QUERY in memory from its shard catalogs, with no persisted indexes to build, plan over or admit |
| 1185 | retains alternative evidence only for EXPLAIN modes | internal: Elixir's index registry, planner and admission (planner); the node evaluates FLOW.QUERY in memory from its shard catalogs, with no persisted indexes to build, plan over or admit |
| 1205 | mandatory shared scope creates disjoint physical plans outside the user AST | internal: Elixir's index registry, planner and admission (planner); the node evaluates FLOW.QUERY in memory from its shard catalogs, with no persisted indexes to build, plan over or admit |

## `flow/query/query_record_store_test.exs`

| Line | Test | Status |
|---|---|---|
| 7 | derives a bounded hydration input budget from the replicated value limit | internal: Elixir's persisted query engine (query_record_store); the node evaluates FLOW.QUERY in memory from its shard catalogs (ADR-014 step 12) |
| 15 | hydrates present query rows in order while preserving missing positions | internal: Elixir's persisted query engine (query_record_store); the node evaluates FLOW.QUERY in memory from its shard catalogs (ADR-014 step 12) |
| 41 | recovery reads only hydrate query rows written before the replayed command | internal: Elixir's persisted query engine (query_record_store); the node evaluates FLOW.QUERY in memory from its shard catalogs (ADR-014 step 12) |
| 65 | replay ordering also filters compact query-row references | internal: Elixir's persisted query engine (query_record_store); the node evaluates FLOW.QUERY in memory from its shard catalogs (ADR-014 step 12) |
| 84 | recovery hydration retries cannot observe rows after the replay index | internal: Elixir's persisted query engine (query_record_store); the node evaluates FLOW.QUERY in memory from its shard catalogs (ADR-014 step 12) |
| 125 | recovery locator repair rereads cannot observe rows after the replay index | internal: Elixir's persisted query engine (query_record_store); the node evaluates FLOW.QUERY in memory from its shard catalogs (ADR-014 step 12) |
| 171 | rejects an invalid replay index boundary before storage IO | internal: Elixir's persisted query engine (query_record_store); the node evaluates FLOW.QUERY in memory from its shard catalogs (ADR-014 step 12) |
| 183 | replay ordering does not hide non-WARaft storage locators | internal: Elixir's persisted query engine (query_record_store); the node evaluates FLOW.QUERY in memory from its shard catalogs (ADR-014 step 12) |
| 202 | replay file visibility is strict only for WARaft storage identities | internal: Elixir's persisted query engine (query_record_store); the node evaluates FLOW.QUERY in memory from its shard catalogs (ADR-014 step 12) |
| 218 | hydrates strict query-row references without requiring metadata maps | internal: Elixir's persisted query engine (query_record_store); the node evaluates FLOW.QUERY in memory from its shard catalogs (ADR-014 step 12) |
| 251 | forwards the explicit include-expired mode to authoritative hydration | internal: Elixir's persisted query engine (query_record_store); the node evaluates FLOW.QUERY in memory from its shard catalogs (ADR-014 step 12) |
| 273 | retention can fall back to validated metadata for an expired row with a retired source | internal: Elixir's persisted query engine (query_record_store); the node evaluates FLOW.QUERY in memory from its shard catalogs (ADR-014 step 12) |
| 303 | recovers expired QueryRow metadata after an authoritative identity mismatch | internal: Elixir's persisted query engine (query_record_store); the node evaluates FLOW.QUERY in memory from its shard catalogs (ADR-014 step 12) |
| 334 | expired fallback preserves successfully hydrated references in the same batch | internal: Elixir's persisted query engine (query_record_store); the node evaluates FLOW.QUERY in memory from its shard catalogs (ADR-014 step 12) |
| 365 | expired fallback uses validated metadata when the retired source location holds another frame | internal: Elixir's persisted query engine (query_record_store); the node evaluates FLOW.QUERY in memory from its shard catalogs (ADR-014 step 12) |
| 390 | expired fallback still hydrates a repaired live row next to an overwritten expired row | internal: Elixir's persisted query engine (query_record_store); the node evaluates FLOW.QUERY in memory from its shard catalogs (ADR-014 step 12) |
| 433 | expired fallback does not hide an unavailable live row | internal: Elixir's persisted query engine (query_record_store); the node evaluates FLOW.QUERY in memory from its shard catalogs (ADR-014 step 12) |
| 458 | rejects a non-boolean include-expired mode before reading storage | internal: Elixir's persisted query engine (query_record_store); the node evaluates FLOW.QUERY in memory from its shard catalogs (ADR-014 step 12) |
| 473 | fails closed without relocation when hydrated bytes are invalid | internal: Elixir's persisted query engine (query_record_store); the node evaluates FLOW.QUERY in memory from its shard catalogs (ADR-014 step 12) |
| 491 | rejects a query row with a non-durable locator before authoritative IO | internal: Elixir's persisted query engine (query_record_store); the node evaluates FLOW.QUERY in memory from its shard catalogs (ADR-014 step 12) |
| 510 | re-resolves query rows once when compaction races hydration | internal: Elixir's persisted query engine (query_record_store); the node evaluates FLOW.QUERY in memory from its shard catalogs (ADR-014 step 12) |
| 546 | retries an unavailable old segment only after its locator is relocated | internal: Elixir's persisted query engine (query_record_store); the node evaluates FLOW.QUERY in memory from its shard catalogs (ADR-014 step 12) |
| 583 | repairs an unchanged stale physical locator after a completed compaction rewrite | internal: Elixir's persisted query engine (query_record_store); the node evaluates FLOW.QUERY in memory from its shard catalogs (ADR-014 step 12) |
| 628 | repairs an unchanged locator after hydration reads the wrong record | internal: Elixir's persisted query engine (query_record_store); the node evaluates FLOW.QUERY in memory from its shard catalogs (ADR-014 step 12) |
| 675 | repairs an unchanged locator after hydration cannot find its record | internal: Elixir's persisted query engine (query_record_store); the node evaluates FLOW.QUERY in memory from its shard catalogs (ADR-014 step 12) |
| 722 | repairs only missing rows from a partially hydrated batch | internal: Elixir's persisted query engine (query_record_store); the node evaluates FLOW.QUERY in memory from its shard catalogs (ADR-014 step 12) |
| 772 | a location holding another record is inconsistent, not retryable, when repair cannot move it | internal: Elixir's persisted query engine (query_record_store); the node evaluates FLOW.QUERY in memory from its shard catalogs (ADR-014 step 12) |
| 800 | fails closed when identity-mismatch locator repair makes no progress | internal: Elixir's persisted query engine (query_record_store); the node evaluates FLOW.QUERY in memory from its shard catalogs (ADR-014 step 12) |
| 833 | reports storage unavailable when identity-mismatch repair cannot read storage | internal: Elixir's persisted query engine (query_record_store); the node evaluates FLOW.QUERY in memory from its shard catalogs (ADR-014 step 12) |
| 852 | fails closed when identity mismatch persists after a successful repair | internal: Elixir's persisted query engine (query_record_store); the node evaluates FLOW.QUERY in memory from its shard catalogs (ADR-014 step 12) |
| 877 | honors the shared deadline before identity-mismatch repair | internal: Elixir's persisted query engine (query_record_store); the node evaluates FLOW.QUERY in memory from its shard catalogs (ADR-014 step 12) |
| 904 | shares one timeout budget across a relocated hydration retry | internal: Elixir's persisted query engine (query_record_store); the node evaluates FLOW.QUERY in memory from its shard catalogs (ADR-014 step 12) |
| 949 | does not repeat unavailable IO when re-resolution returns the same locator | internal: Elixir's persisted query engine (query_record_store); the node evaluates FLOW.QUERY in memory from its shard catalogs (ADR-014 step 12) |
| 975 | forwards the authoritative read timeout | internal: Elixir's persisted query engine (query_record_store); the node evaluates FLOW.QUERY in memory from its shard catalogs (ADR-014 step 12) |
| 997 | preserves hydration timeouts without re-resolving the locator | internal: Elixir's persisted query engine (query_record_store); the node evaluates FLOW.QUERY in memory from its shard catalogs (ADR-014 step 12) |
| 1020 | fails closed when a locator is still inconsistent after one retry | internal: Elixir's persisted query engine (query_record_store); the node evaluates FLOW.QUERY in memory from its shard catalogs (ADR-014 step 12) |
| 1039 | fails closed when a missing authoritative record cannot be relocated | internal: Elixir's persisted query engine (query_record_store); the node evaluates FLOW.QUERY in memory from its shard catalogs (ADR-014 step 12) |
| 1065 | recovery fallback fails closed when a live missing record cannot be relocated | internal: Elixir's persisted query engine (query_record_store); the node evaluates FLOW.QUERY in memory from its shard catalogs (ADR-014 step 12) |
| 1101 | reserves decoded query-row memory before admitting authoritative bytes | internal: Elixir's persisted query engine (query_record_store); the node evaluates FLOW.QUERY in memory from its shard catalogs (ADR-014 step 12) |
| 1136 | supports validated encoded reads without re-encoding authoritative records | internal: Elixir's persisted query engine (query_record_store); the node evaluates FLOW.QUERY in memory from its shard catalogs (ADR-014 step 12) |

## `flow/query/query_row_codec_format_test.exs`

| Line | Test | Status |
|---|---|---|
| 14 | keeps the persisted QueryRow bytes stable while optimizing construction | internal: Elixir's persisted query engine (query_row_codec_format); the node evaluates FLOW.QUERY in memory from its shard catalogs (ADR-014 step 12) |

## `flow/query/query_row_codec_test.exs`

| Line | Test | Status |
|---|---|---|
| 9 | round trips all query metadata and a physical locator | internal: Elixir's persisted query engine (query_row_codec); the node evaluates FLOW.QUERY in memory from its shard catalogs (ADR-014 step 12) |
| 27 | round trips the worker identity required to rebuild running active indexes | internal: Elixir's persisted query engine (query_row_codec); the node evaluates FLOW.QUERY in memory from its shard catalogs (ADR-014 step 12) |
| 44 | preserves missing, null, scalar, and multivalue metadata distinctly | internal: Elixir's persisted query engine (query_row_codec); the node evaluates FLOW.QUERY in memory from its shard catalogs (ADR-014 step 12) |
| 66 | round trips an unpartitioned run through its auto-partition state key | internal: Elixir's persisted query engine (query_row_codec); the node evaluates FLOW.QUERY in memory from its shard catalogs (ADR-014 step 12) |
| 76 | does not persist payload references or internal system metadata | internal: Elixir's persisted query engine (query_row_codec); the node evaluates FLOW.QUERY in memory from its shard catalogs (ADR-014 step 12) |
| 96 | binds opaque shared routing scope while exposing only the logical partition | internal: Elixir's persisted query engine (query_row_codec); the node evaluates FLOW.QUERY in memory from its shard catalogs (ADR-014 step 12) |
| 134 | restores terminal retention time from an expired covering row | internal: Elixir's persisted query engine (query_row_codec); the node evaluates FLOW.QUERY in memory from its shard catalogs (ADR-014 step 12) |
| 155 | encoding is deterministic across nested map insertion order | internal: Elixir's persisted query engine (query_row_codec); the node evaluates FLOW.QUERY in memory from its shard catalogs (ADR-014 step 12) |
| 174 | supports every authoritative state locator source | internal: Elixir's persisted query engine (query_row_codec); the node evaluates FLOW.QUERY in memory from its shard catalogs (ADR-014 step 12) |
| 198 | rejects logical WARaft locations and round trips the complete physical frame address | internal: Elixir's persisted query engine (query_row_codec); the node evaluates FLOW.QUERY in memory from its shard catalogs (ADR-014 step 12) |
| 213 | derives large run identities from the state key instead of duplicating them | internal: Elixir's persisted query engine (query_row_codec); the node evaluates FLOW.QUERY in memory from its shard catalogs (ADR-014 step 12) |
| 226 | binds the row to its state key and logical generation | internal: Elixir's persisted query engine (query_row_codec); the node evaluates FLOW.QUERY in memory from its shard catalogs (ADR-014 step 12) |
| 242 | rejects inconsistent row and locator expiry | internal: Elixir's persisted query engine (query_row_codec); the node evaluates FLOW.QUERY in memory from its shard catalogs (ADR-014 step 12) |
| 261 | query-rich rows are smaller than the removed full-record LMDB envelope | internal: Elixir's persisted query engine (query_row_codec); the node evaluates FLOW.QUERY in memory from its shard catalogs (ADR-014 step 12) |
| 271 | relocation compare-and-swaps only the physical locator | internal: Elixir's persisted query engine (query_row_codec); the node evaluates FLOW.QUERY in memory from its shard catalogs (ADR-014 step 12) |
| 305 | moving a projection locator changes only its frame and keeps the rest of the row | internal: Elixir's persisted query engine (query_row_codec); the node evaluates FLOW.QUERY in memory from its shard catalogs (ADR-014 step 12) |
| 355 | applies expiry without decoding expired metadata | internal: Elixir's persisted query engine (query_row_codec); the node evaluates FLOW.QUERY in memory from its shard catalogs (ADR-014 step 12) |
| 363 | decodes a strict hydration reference without materializing query metadata | internal: Elixir's persisted query engine (query_row_codec); the node evaluates FLOW.QUERY in memory from its shard catalogs (ADR-014 step 12) |
| 387 | query decoding materializes only required public metadata sections | internal: Elixir's persisted query engine (query_row_codec); the node evaluates FLOW.QUERY in memory from its shard catalogs (ADR-014 step 12) |
| 427 | query decoding materializes only the selected dynamic fields | internal: Elixir's persisted query engine (query_row_codec); the node evaluates FLOW.QUERY in memory from its shard catalogs (ADR-014 step 12) |
| 451 | query decoding validates skipped metadata instead of trusting its checksum | internal: Elixir's persisted query engine (query_row_codec); the node evaluates FLOW.QUERY in memory from its shard catalogs (ADR-014 step 12) |
| 464 | query decoding rejects duplicate list values in selected and skipped attributes | internal: Elixir's persisted query engine (query_row_codec); the node evaluates FLOW.QUERY in memory from its shard catalogs (ADR-014 step 12) |
| 495 | rejects oversized metadata and invalid dynamic values | internal: Elixir's persisted query engine (query_row_codec); the node evaluates FLOW.QUERY in memory from its shard catalogs (ADR-014 step 12) |
| 554 | validates the bounded record before a physical locator exists | internal: Elixir's persisted query engine (query_row_codec); the node evaluates FLOW.QUERY in memory from its shard catalogs (ADR-014 step 12) |
| 568 | every admitted record fits with the largest valid locator | internal: Elixir's persisted query engine (query_row_codec); the node evaluates FLOW.QUERY in memory from its shard catalogs (ADR-014 step 12) |
| 576 | requires one fixed SHA-256 checksum in every query-row locator | internal: Elixir's persisted query engine (query_row_codec); the node evaluates FLOW.QUERY in memory from its shard catalogs (ADR-014 step 12) |
| 585 | rejects truncation, trailing bytes, malformed lengths, and unknown versions | internal: Elixir's persisted query engine (query_row_codec); the node evaluates FLOW.QUERY in memory from its shard catalogs (ADR-014 step 12) |
| 596 | detects a single-bit corruption at every byte position | internal: Elixir's persisted query engine (query_row_codec); the node evaluates FLOW.QUERY in memory from its shard catalogs (ADR-014 step 12) |

## `flow/query/query_row_compaction_test.exs`

| Line | Test | Status |
|---|---|---|
| 16 | retention validates a materialized record but copies only its stored blob reference | internal: Elixir's persisted query engine (query_row_compaction); the node evaluates FLOW.QUERY in memory from its shard catalogs (ADR-014 step 12) |
| 65 | retention preserves the authoritative frame expiry instead of the shorter row expiry | internal: Elixir's persisted query engine (query_row_compaction); the node evaluates FLOW.QUERY in memory from its shard catalogs (ADR-014 step 12) |
| 97 | a query repairs a stale QueryRow after a crash between rewrite and locator CAS | internal: Elixir's persisted query engine (query_row_compaction); the node evaluates FLOW.QUERY in memory from its shard catalogs (ADR-014 step 12) |
| 169 | a query repairs a stale locator that resolves to a different valid record | internal: Elixir's persisted query engine (query_row_compaction); the node evaluates FLOW.QUERY in memory from its shard catalogs (ADR-014 step 12) |
| 233 | the next retention pass repairs a stale QueryRow left after the rewrite swap | internal: Elixir's persisted query engine (query_row_compaction); the node evaluates FLOW.QUERY in memory from its shard catalogs (ADR-014 step 12) |
| 286 | an interrupted rewrite rolls back without invalidating the existing QueryRow | internal: Elixir's persisted query engine (query_row_compaction); the node evaluates FLOW.QUERY in memory from its shard catalogs (ADR-014 step 12) |
| 351 | relocation covers QueryRows on both sides of the trim boundary | internal: Elixir's persisted query engine (query_row_compaction); the node evaluates FLOW.QUERY in memory from its shard catalogs (ADR-014 step 12) |
| 430 | streaming compaction consumes bounded retention pages and keeps later records | internal: Elixir's persisted query engine (query_row_compaction); the node evaluates FLOW.QUERY in memory from its shard catalogs (ADR-014 step 12) |
| 511 | a failed retention stream leaves the source projection log unchanged | internal: Elixir's persisted query engine (query_row_compaction); the node evaluates FLOW.QUERY in memory from its shard catalogs (ADR-014 step 12) |

## `flow/query/query_row_decode_plan_test.exs`

| Line | Test | Status |
|---|---|---|
| 6 | omits dynamic sections for builtin-only projected queries | internal: Elixir's persisted query engine (query_row_decode_plan); the node evaluates FLOW.QUERY in memory from its shard catalogs (ADR-014 step 12) |
| 21 | includes dynamic sections used anywhere in execution | internal: Elixir's persisted query engine (query_row_decode_plan); the node evaluates FLOW.QUERY in memory from its shard catalogs (ADR-014 step 12) |
| 58 | whole-section projections take precedence over exact dynamic fields | internal: Elixir's persisted query engine (query_row_decode_plan); the node evaluates FLOW.QUERY in memory from its shard catalogs (ADR-014 step 12) |
| 73 | rejects malformed exact selectors without raising | internal: Elixir's persisted query engine (query_row_decode_plan); the node evaluates FLOW.QUERY in memory from its shard catalogs (ADR-014 step 12) |
| 85 | full record return includes both public metadata sections but count does not | internal: Elixir's persisted query engine (query_row_decode_plan); the node evaluates FLOW.QUERY in memory from its shard catalogs (ADR-014 step 12) |

## `flow/query/query_row_primitives_test.exs`

| Line | Test | Status |
|---|---|---|
| 6 | round trips canonical unsigned integers across every width | internal: Elixir's persisted query engine (query_row_primitives); the node evaluates FLOW.QUERY in memory from its shard catalogs (ADR-014 step 12) |
| 23 | rejects truncated, overlong, and overflowing unsigned integers | internal: Elixir's persisted query engine (query_row_primitives); the node evaluates FLOW.QUERY in memory from its shard catalogs (ADR-014 step 12) |
| 35 | round trips every supported scalar category and binary lists | internal: Elixir's persisted query engine (query_row_primitives); the node evaluates FLOW.QUERY in memory from its shard catalogs (ADR-014 step 12) |
| 51 | rejects non-finite floats and bounded-value violations | internal: Elixir's persisted query engine (query_row_primitives); the node evaluates FLOW.QUERY in memory from its shard catalogs (ADR-014 step 12) |
| 63 | skips canonical values without materializing them | internal: Elixir's persisted query engine (query_row_primitives); the node evaluates FLOW.QUERY in memory from its shard catalogs (ADR-014 step 12) |
| 81 | skip rejects malformed and noncanonical values even when their result is unused | internal: Elixir's persisted query engine (query_row_primitives); the node evaluates FLOW.QUERY in memory from its shard catalogs (ADR-014 step 12) |

## `flow/query/query_row_relocator_test.exs`

| Line | Test | Status |
|---|---|---|
| 7 | groups physical lookup and CAS-relocates only the central QueryRows | internal: Elixir's persisted query engine (query_row_relocator); the node evaluates FLOW.QUERY in memory from its shard catalogs (ADR-014 step 12) |
| 53 | isolates a compare race, preserves the newer row, and relocates unaffected rows | internal: Elixir's persisted query engine (query_row_relocator); the node evaluates FLOW.QUERY in memory from its shard catalogs (ADR-014 step 12) |
| 105 | repairs only QueryRows that still match the failed physical locator | internal: Elixir's persisted query engine (query_row_relocator); the node evaluates FLOW.QUERY in memory from its shard catalogs (ADR-014 step 12) |
| 157 | repair leaves a projection QueryRow alone when the frame now holds another key | internal: Elixir's persisted query engine (query_row_relocator); the node evaluates FLOW.QUERY in memory from its shard catalogs (ADR-014 step 12) |
| 184 | repair moves a projection QueryRow when the frame holds its record | internal: Elixir's persisted query engine (query_row_relocator); the node evaluates FLOW.QUERY in memory from its shard catalogs (ADR-014 step 12) |
| 215 | repair follows the keydir to a projection QueryRow's renumbered frame | internal: Elixir's persisted query engine (query_row_relocator); the node evaluates FLOW.QUERY in memory from its shard catalogs (ADR-014 step 12) |

## `flow/query/query_row_store_test.exs`

| Line | Test | Status |
|---|---|---|
| 13 | reads a bounded prefix of query rows and applies expiry | internal: Elixir's persisted query engine (query_row_store); the node evaluates FLOW.QUERY in memory from its shard catalogs (ADR-014 step 12) |
| 33 | fails closed on a corrupt row | internal: Elixir's persisted query engine (query_row_store); the node evaluates FLOW.QUERY in memory from its shard catalogs (ADR-014 step 12) |
| 42 | decodes fused raw values with the same missing, expiry, and corruption semantics | internal: Elixir's persisted query engine (query_row_store); the node evaluates FLOW.QUERY in memory from its shard catalogs (ADR-014 step 12) |
| 66 | applies one prepared query decode plan to bounded and fused rows | internal: Elixir's persisted query engine (query_row_store); the node evaluates FLOW.QUERY in memory from its shard catalogs (ADR-014 step 12) |
| 92 | reads bounded hydration references without materializing metadata | internal: Elixir's persisted query engine (query_row_store); the node evaluates FLOW.QUERY in memory from its shard catalogs (ADR-014 step 12) |

## `flow/query/record_order_test.exs`

| Line | Test | Status |
|---|---|---|
| 7 | uses the same opaque tie breaker as composite entry keys | internal: Elixir's composite-key ordering and tie breaker |
| 41 | explicit run-id ordering remains lexical and precedes the opaque final tie breaker | internal: Elixir's composite-key ordering and tie breaker |
| 51 | fails closed for an unsupported multi-valued order field | internal: Elixir's composite-key ordering and tie breaker |
| 58 | fails closed before encoding an oversized lexical order component | internal: Elixir's composite-key ordering and tie breaker |
| 65 | checks bounded key lists in native order without changing tie semantics | internal: Elixir's composite-key ordering and tie breaker |

## `flow/query/record_projection_test.exs`

| Line | Test | Status |
|---|---|---|
| 6 | projects structural and bounded user metadata fields | covered: `query::tests::projects_selected_fields_after_filtering_ordering_and_limiting` and the reference projections |
| 59 | preserves misses and storage errors | internal: Elixir's storage-error markers |
| 64 | projects only requested run fields while preserving nested metadata shape | covered: `query::tests::projects_selected_fields_after_filtering_ordering_and_limiting` and the reference projections |
| 90 | projects complete run metadata maps when explicitly requested | covered: `query::tests::projects_selected_fields_after_filtering_ordering_and_limiting` and the reference projections |
| 110 | projects bounded history fields without flattening the response contract | covered: `query::tests::projects_selected_fields_after_filtering_ordering_and_limiting` and the reference projections |

## `flow/query/reference_evaluator_test.exs`

| Line | Test | Status |
|---|---|---|
| 45 | evaluates equality, IN, bounded time windows, and metadata predicates | ported (`query::tests::evaluates_equality_in_time_windows_and_metadata_predicates`) |
| 65 | projects selected fields only after filtering, ordering, and limiting | ported (`query::tests::projects_selected_fields_after_filtering_ordering_and_limiting`) |
| 94 | time windows include the lower bound and exclude the upper bound | ported (`query::tests::time_windows_include_the_lower_bound_and_exclude_the_upper`) |
| 117 | attribute list equality uses membership semantics | ported (`query::tests::attribute_list_equality_uses_membership`) |
| 133 | keeps null and missing semantics distinct | ported (`query::tests::null_and_missing_stay_distinct`) |
| 158 | applies deterministic null/missing ordering and an opaque stable tie breaker | ported (`query::tests::nulls_and_missing_values_sort_last_with_a_stable_tie_breaker`) |
| 184 | generated predicate combinations agree with direct scalar evaluation | internal: Elixir's agreement check between its prepared and direct evaluators |
| 196 | prepared predicates preserve direct evaluation for every predicate shape | internal: as line 184 |
| 216 | prepared IN membership keeps strict runtime types and multivalue semantics | internal: Elixir's prepared IN form (membership semantics: `query::tests::attribute_list_equality_uses_membership`) |
| 230 | rejects malformed predicates during preparation | internal: hand-built malformed predicates |
| 247 | ordinary comparisons reject null and missing sentinels | covered: `query::validation_tests::dynamic_comparisons_cannot_bind_null_or_compound_values` |
| 266 | rejects unbounded execution and invalid mixed-type ranges | ported (`query::reference_bind_tests::ranges_keep_one_type_and_run_forward`) |
| 283 | accepts mixed literal and parameter bounds before binding | ported (same test) |
| 299 | rejects reversed literal ranges before execution | ported (same test; Elixir's `invalid_predicate_range` has no wire code and answers as invalid syntax) |

## `flow/query/reference_parser_test.exs`

| Line | Test | Status |
|---|---|---|
| 8 | parses bounded run and history return projections | ported (`query::fql::reference_tests::accepts_every_shape_the_reference_parser_accepts`) |
| 38 | rejects unsafe, duplicate, empty, and oversized return projections | ported (`query::fql::reference_tests::refuses_what_the_reference_parser_refuses`) |
| 68 | parses the bounded run point-read shape | ported (`query::fql::reference_tests::accepts_every_shape_the_reference_parser_accepts`) |
| 89 | parses a run-id point read that uses the canonical auto partition | ported (`query::fql::reference_tests::accepts_every_shape_the_reference_parser_accepts`) |
| 100 | parses bounded direct event history | ported (`query::fql::reference_tests::accepts_every_shape_the_reference_parser_accepts`) |
| 115 | parses a partition-contained parent lineage query | ported (`query::fql::reference_tests::accepts_every_shape_the_reference_parser_accepts`) |
| 135 | normalizes keywords and supports a trailing semicolon | ported (`query::fql::reference_tests::accepts_every_shape_the_reference_parser_accepts`) |
| 150 | parses EXPLAIN and named parameters without binding values | ported (`query::fql::reference_tests::accepts_every_shape_the_reference_parser_accepts`) |
| 166 | parses EXPLAIN ANALYZE as a distinct fresh-execution mode | ported (`query::fql::reference_tests::accepts_every_shape_the_reference_parser_accepts`; its refusals in `...::refuses_what_the_reference_parser_refuses`) |
| 189 | parses bounded collection equality, IN, time window, order, and limit | ported (`query::fql::reference_tests::accepts_every_shape_the_reference_parser_accepts`) |
| 217 | parses an optional parameterized collection cursor | ported (`query::fql::reference_tests::accepts_every_shape_the_reference_parser_accepts`) |
| 243 | parses inclusive ranges and explicit null or missing predicates | ported (`query::fql::reference_tests::accepts_every_shape_the_reference_parser_accepts`) |
| 258 | parses safely quoted metadata field segments | ported (`query::fql::reference_tests::accepts_every_shape_the_reference_parser_accepts`) |
| 283 | parses a bounded scalar count without row ordering or pagination | ported (`query::fql::reference_tests::accepts_every_shape_the_reference_parser_accepts`) |
| 315 | rejects collection queries without explicit bounds or malformed IN lists | ported (`query::fql::reference_tests::refuses_what_the_reference_parser_refuses`) |
| 326 | rejects unbounded and unsupported query shapes | ported (`query::fql::reference_tests::refuses_what_the_reference_parser_refuses`) |
| 341 | rejects oversized input before tokenization | ported (`query::fql::reference_tests::refuses_what_the_reference_parser_refuses`) |
| 349 | rejects huge integer literals without constructing an unbounded integer | ported (`query::fql::reference_tests::refuses_what_the_reference_parser_refuses`) |
| 360 | reports the unsupported source and field tokens after optional mode prefixes | ported (`query::fql::reference_tests::diagnostics_point_at_the_rejected_token`) |
| 376 | reports a later unsupported field after a valid metadata predicate | ported (`query::fql::reference_tests::diagnostics_point_at_the_rejected_token`) |
| 393 | reports unsupported and duplicate return projection fields at the rejected selector | ported (`query::fql::reference_tests::diagnostics_point_at_the_rejected_token`) |
| 421 | reports the clause that appears where WHERE is required | ported (`query::fql::reference_tests::diagnostics_point_at_the_rejected_token`) |
| 429 | rejects oversized input without a second diagnostic scan | internal: Elixir's reduction budget for oversized diagnostics (the node refuses oversized input on its length: `...::refuses_what_the_reference_parser_refuses`) |
| 445 | binds a named keyword parameter | ported (`query::reference_bind_tests::parameters_bind_by_name_repeated_or_beside_literals`) |
| 463 | binds repeated, mixed, and literal-only values exactly | ported (same test) |
| 495 | fails closed on missing, mistyped, and unused parameters | ported (`query::reference_bind_tests::missing_mistyped_and_unused_parameters_are_refused`; its atom-keyed map has no wire form) |
| 517 | rejects unsupported canonical predicates without raising | internal: hand-built malformed `Request` structs, which have no wire form |
| 531 | rejects malformed canonical envelopes before binding | internal: hand-built malformed `Request` structs, which have no wire form |
| 552 | rejects malformed canonical values without raising | internal: hand-built malformed `Request` structs, which have no wire form |
| 563 | rejects oversized bound values before planning | ported (`query::reference_bind_tests::bound_ids_stop_at_the_storage_key_bounds`) |
| 578 | accepts exact storage-key boundaries and rejects the next byte | ported (same test) |

## `flow/query/registry_snapshot_test.exs`

| Line | Test | Status |
|---|---|---|
| 6 | holds a full catalog generation alongside its retiring generation | internal: Elixir's persisted query engine (registry_snapshot); the node evaluates FLOW.QUERY in memory from its shard catalogs (ADR-014 step 12) |
| 19 | rejects forged registered definitions and unbounded generation counters | internal: Elixir's persisted query engine (registry_snapshot); the node evaluates FLOW.QUERY in memory from its shard catalogs (ADR-014 step 12) |
| 40 | registered-index construction rejects a forged definition | internal: Elixir's persisted query engine (registry_snapshot); the node evaluates FLOW.QUERY in memory from its shard catalogs (ADR-014 step 12) |

## `flow/query/request_test.exs`

| Line | Test | Status |
|---|---|---|
| 6 | point queries may derive the auto partition from run_id | ported (`query::fql::reference_tests::accepts_every_shape_the_reference_parser_accepts`: a run-id point read) |
| 19 | event history is bounded by one run and ordered by event id | ported (`query::fql::reference_tests::refuses_what_the_reference_parser_refuses`: events ordered by anything but event_id; its non-run predicate is a hand-built struct) |
| 38 | describes one exact partition-contained lineage shape | ported (`query::fql::reference_tests::accepts_every_shape_the_reference_parser_accepts`: the lineage shape; its lineage descriptor is Elixir's planner input) |
| 66 | collection queries require one unambiguous partition equality | ported (`query::validation_tests::collections_need_one_partition_equality_and_integer_order`) |
| 97 | EXPLAIN rejects execution cursors | ported (`query::fql::reference_tests::refuses_what_the_reference_parser_refuses`) |
| 112 | EXPLAIN ANALYZE is a bounded fresh execution and rejects cursors | ported (`query::fql::reference_tests::accepts_every_shape_the_reference_parser_accepts` and `query::fql::reference_tests::refuses_what_the_reference_parser_refuses`) |
| 135 | scalar counts are partition-contained and have no row pagination envelope | ported (`query::fql::reference_tests::accepts_every_shape_the_reference_parser_accepts` and `query::fql::reference_tests::refuses_what_the_reference_parser_refuses`) |
| 158 | public collection ordering uses bounded numeric fields and an implicit run reference | ported (`query::validation_tests::collections_need_one_partition_equality_and_integer_order`) |
| 176 | cursor order validation shares the canonical query ordering contract | internal: Elixir's cursor-order helper (a cursor resuming its own order: `query::tests::a_cursor_resumes_its_own_query_after_the_last_row`) |
| 191 | dynamic metadata allows empty strings without allowing empty identity fields | ported (`query::reference_bind_tests::identity_fields_take_no_empty_value` and `query::validation_tests::metadata_allows_empty_text_but_bounds_its_size`) |
| 215 | dynamic metadata literals cannot exceed the authoritative value limit | ported (`query::validation_tests::metadata_allows_empty_text_but_bounds_its_size`) |
| 232 | record projections are source-specific, unique, and bounded | ported (`query::fql::reference_tests::refuses_what_the_reference_parser_refuses`: projection refusals) |

## `flow/query/response_test.exs`

| Line | Test | Status |
|---|---|---|
| 7 | builds one versioned page contract and accounts its complete encoded size | internal: Elixir's response page contract and budgets |
| 30 | builds record responses directly from covering index rows | internal: Elixir's response page contract and budgets |
| 47 | response byte accounting is stable after writing any signed response size | internal: Elixir's response page contract and budgets |
| 65 | rejects the full response instead of returning a partial page | internal: Elixir's response page contract and budgets |
| 73 | enforces the response budget against the native wire representation | internal: Elixir's response page contract and budgets |
| 88 | settles a negotiated compact response against its exact representation | internal: Elixir's response page contract and budgets |
| 141 | rejects inconsistent pagination and usage metadata | internal: Elixir's response page contract and budgets |
| 152 | rejects pagination metadata above the public cursor limit | internal: Elixir's response page contract and budgets |
| 159 | rejects malformed outgoing pagination cursors | internal: Elixir's response page contract and budgets |
| 166 | rejects unsupported quality values in every dimension | internal: Elixir's response page contract and budgets |
| 175 | fails closed for usage counters outside the native signed integer contract | internal: Elixir's response page contract and budgets |
| 186 | encodes nested unsigned 64-bit record values | internal: Elixir's response page contract and budgets |
| 194 | fails closed instead of raising for a nested integer above unsigned 64-bit | internal: Elixir's response page contract and budgets |
| 201 | builds a non-paginated scalar count without inventing row hydration | covered: `flow_query::queries_read_the_catalog_now_and_after_a_restart` (a scalar count answers no page) |
| 222 | settles a compact scalar count with the compact codec size | internal: Elixir's response page contract and budgets |

## `flow/query/result_codec_test.exs`

| Line | Test | Status |
|---|---|---|
| 11 | the frozen wire schema covers the complete public result contract | internal: Elixir's negotiated compact result codec; the node answers the typed result map, which the SDKs read |
| 18 | encodes the shared cross-SDK golden corpus byte for byte | internal: Elixir's negotiated compact result codec; the node answers the typed result map, which the SDKs read |
| 65 | encodes a record page with one fixed schema and exact response bytes | internal: Elixir's negotiated compact result codec; the node answers the typed result map, which the SDKs read |
| 130 | encodes scalar counts without a record or page envelope | internal: Elixir's negotiated compact result codec; the node answers the typed result map, which the SDKs read |
| 150 | measures the exact compact payload without changing response usage | internal: Elixir's negotiated compact result codec; the node answers the typed result map, which the SDKs read |
| 178 | encodes and measures a validated response in one pass | internal: Elixir's negotiated compact result codec; the node answers the typed result map, which the SDKs read |
| 197 | encodes history records and preserves present nil separately from missing fields | internal: Elixir's negotiated compact result codec; the node answers the typed result map, which the SDKs read |
| 218 | encodes sparse projected rows without placeholders for omitted fields | internal: Elixir's negotiated compact result codec; the node answers the typed result map, which the SDKs read |
| 243 | explicit projection materially reduces encoded result work for metadata-heavy rows | internal: Elixir's negotiated compact result codec; the node answers the typed result map, which the SDKs read |
| 271 | encodes every currently emitted quality combination | internal: Elixir's negotiated compact result codec; the node answers the typed result map, which the SDKs read |
| 296 | keeps every public run field at its frozen bitmap position | internal: Elixir's negotiated compact result codec; the node answers the typed result map, which the SDKs read |
| 319 | accepts string-keyed native response maps without changing their wire schema | internal: Elixir's negotiated compact result codec; the node answers the typed result map, which the SDKs read |
| 333 | falls back for unknown contracts, fields, quality values, and unencodable values | internal: Elixir's negotiated compact result codec; the node answers the typed result map, which the SDKs read |
| 355 | fails closed outside the public count, cursor, row, and usage bounds | internal: Elixir's negotiated compact result codec; the node answers the typed result map, which the SDKs read |
| 376 | fails closed for structurally inconsistent usage | internal: Elixir's negotiated compact result codec; the node answers the typed result map, which the SDKs read |
| 400 | keeps representative record pages materially smaller than typed maps | internal: Elixir's negotiated compact result codec; the node answers the typed result map, which the SDKs read |

## `flow/query/shape_test.exs`

| Line | Test | Status |
|---|---|---|
| 6 | classifies point and history requests with their execution descriptors | internal: Elixir's execution shape classification |
| 34 | classifies generic, count, lineage, and fixed-index collection shapes | internal: Elixir's execution shape classification |
| 96 | the default OSS engine owns every parser-supported execution shape | internal: Elixir's execution shape classification |
| 108 | rejects ranges that cannot be represented exactly by fixed score indexes | internal: Elixir's execution shape classification |

## `flow/query/source_catalog_test.exs`

| Line | Test | Status |
|---|---|---|
| 18 | uses a bounded collision-free key while preserving the full Flow state key | internal: Elixir's persisted query engine (source_catalog); the node evaluates FLOW.QUERY in memory from its shard catalogs (ADR-014 step 12) |
| 29 | rejects a catalog member that does not own its state key | internal: Elixir's persisted query engine (source_catalog); the node evaluates FLOW.QUERY in memory from its shard catalogs (ADR-014 step 12) |
| 38 | decodes the global exact-type projection without trusting its components | internal: Elixir's persisted query engine (source_catalog); the node evaluates FLOW.QUERY in memory from its shard catalogs (ADR-014 step 12) |
| 55 | bootstrap cannot resurrect a catalog member deleted after hydration | internal: Elixir's persisted query engine (source_catalog); the node evaluates FLOW.QUERY in memory from its shard catalogs (ADR-014 step 12) |
| 87 | bootstrap cannot overwrite a source row replaced after collision validation | internal: Elixir's persisted query engine (source_catalog); the node evaluates FLOW.QUERY in memory from its shard catalogs (ADR-014 step 12) |
| 114 | bootstrap rejects a corrupt durable completion marker | internal: Elixir's persisted query engine (source_catalog); the node evaluates FLOW.QUERY in memory from its shard catalogs (ADR-014 step 12) |
| 122 | bootstrap counts the durable completion marker against the page budget | internal: Elixir's persisted query engine (source_catalog); the node evaluates FLOW.QUERY in memory from its shard catalogs (ADR-014 step 12) |

## `flow/query/statistics_store_test.exs`

| Line | Test | Status |
|---|---|---|
| 7 | publishes tenant-scoped statistics and rejects monotonic regressions | internal: Elixir's persisted query engine (statistics_store); the node evaluates FLOW.QUERY in memory from its shard catalogs (ADR-014 step 12) |
| 38 | evicts deterministically and never exceeds its entry bound | internal: Elixir's persisted query engine (statistics_store); the node evaluates FLOW.QUERY in memory from its shard catalogs (ADR-014 step 12) |
| 52 | replacements keep eviction metadata bounded to live cache entries | internal: Elixir's persisted query engine (statistics_store); the node evaluates FLOW.QUERY in memory from its shard catalogs (ADR-014 step 12) |
| 70 | concurrent readers observe only complete immutable structs | internal: Elixir's persisted query engine (statistics_store); the node evaluates FLOW.QUERY in memory from its shard catalogs (ADR-014 step 12) |
| 87 | statistics validation is bounded and inspection does not reveal digests | internal: Elixir's persisted query engine (statistics_store); the node evaluates FLOW.QUERY in memory from its shard catalogs (ADR-014 step 12) |
| 107 | summaries report freshness by index without exposing tenant digests | internal: Elixir's persisted query engine (statistics_store); the node evaluates FLOW.QUERY in memory from its shard catalogs (ADR-014 step 12) |
| 163 | summary projection enforces identity bounds without mutating the cache | internal: Elixir's persisted query engine (statistics_store); the node evaluates FLOW.QUERY in memory from its shard catalogs (ADR-014 step 12) |
| 195 | rejects histogram counters outside the unsigned 64-bit domain | internal: Elixir's persisted query engine (statistics_store); the node evaluates FLOW.QUERY in memory from its shard catalogs (ADR-014 step 12) |
| 206 | rejects advisory counts that exceed their population | internal: Elixir's persisted query engine (statistics_store); the node evaluates FLOW.QUERY in memory from its shard catalogs (ADR-014 step 12) |
| 230 | histograms are homogeneous and ignored for differently typed query bounds | internal: Elixir's persisted query engine (statistics_store); the node evaluates FLOW.QUERY in memory from its shard catalogs (ADR-014 step 12) |
| 261 | exact prefix counts expire by their own observation time | internal: Elixir's persisted query engine (statistics_store); the node evaluates FLOW.QUERY in memory from its shard catalogs (ADR-014 step 12) |
| 274 | lookup rejects an oversized scope before using its digest | internal: Elixir's persisted query engine (statistics_store); the node evaluates FLOW.QUERY in memory from its shard catalogs (ADR-014 step 12) |

## `flow/query/statistics_worker_test.exs`

| Line | Test | Status |
|---|---|---|
| 14 | coalesces requests and records an exact prefix only when a capped probe exhausts | internal: Elixir's persisted query engine (statistics_worker); the node evaluates FLOW.QUERY in memory from its shard catalogs (ADR-014 step 12) |
| 70 | an exact prefix probe does not refresh stale histogram evidence | internal: Elixir's persisted query engine (statistics_worker); the node evaluates FLOW.QUERY in memory from its shard catalogs (ADR-014 step 12) |
| 145 | a non-exhausted probe never turns a lower bound into an exact count | internal: Elixir's persisted query engine (statistics_worker); the node evaluates FLOW.QUERY in memory from its shard catalogs (ADR-014 step 12) |
| 177 | an exhausted callback cannot exceed the bounded probe request | internal: Elixir's persisted query engine (statistics_worker); the node evaluates FLOW.QUERY in memory from its shard catalogs (ADR-014 step 12) |
| 208 | asynchronous probes never wait for LMDB sampling | internal: Elixir's persisted query engine (statistics_worker); the node evaluates FLOW.QUERY in memory from its shard catalogs (ADR-014 step 12) |
| 252 | shared prepared probes validate hidden scope and physical shard routing | internal: Elixir's persisted query engine (statistics_worker); the node evaluates FLOW.QUERY in memory from its shard catalogs (ADR-014 step 12) |
| 307 | batched probe admission is bounded and validates prepared probes before I/O | internal: Elixir's persisted query engine (statistics_worker); the node evaluates FLOW.QUERY in memory from its shard catalogs (ADR-014 step 12) |
| 351 | asynchronous probe admission bounds the worker mailbox while I/O is blocked | internal: Elixir's persisted query engine (statistics_worker); the node evaluates FLOW.QUERY in memory from its shard catalogs (ADR-014 step 12) |
| 421 | duplicate asynchronous probes do not consume admission capacity | internal: Elixir's persisted query engine (statistics_worker); the node evaluates FLOW.QUERY in memory from its shard catalogs (ADR-014 step 12) |
| 471 | asynchronous admission does not expose raw scope or predicate values | internal: Elixir's persisted query engine (statistics_worker); the node evaluates FLOW.QUERY in memory from its shard catalogs (ADR-014 step 12) |
| 532 | malformed asynchronous admission entries cannot crash the worker | internal: Elixir's persisted query engine (statistics_worker); the node evaluates FLOW.QUERY in memory from its shard catalogs (ADR-014 step 12) |
| 555 | rejects malformed or cross-shard probe requests before I/O | internal: Elixir's persisted query engine (statistics_worker); the node evaluates FLOW.QUERY in memory from its shard catalogs (ADR-014 step 12) |
| 598 | rejects a valid scope submitted to the wrong shard | internal: Elixir's persisted query engine (statistics_worker); the node evaluates FLOW.QUERY in memory from its shard catalogs (ADR-014 step 12) |
| 649 | rejects malformed asynchronous contexts without raising | internal: Elixir's persisted query engine (statistics_worker); the node evaluates FLOW.QUERY in memory from its shard catalogs (ADR-014 step 12) |
| 659 | rejects prepared probes whose digests or range do not match their values | internal: Elixir's persisted query engine (statistics_worker); the node evaluates FLOW.QUERY in memory from its shard catalogs (ADR-014 step 12) |
| 705 | fails startup cleanly when the context admission table is already owned | internal: Elixir's persisted query engine (statistics_worker); the node evaluates FLOW.QUERY in memory from its shard catalogs (ADR-014 step 12) |
| 725 | bounds the recently-seen probe cache under high-cardinality traffic | internal: Elixir's persisted query engine (statistics_worker); the node evaluates FLOW.QUERY in memory from its shard catalogs (ADR-014 step 12) |
| 765 | evicts an old exact-prefix observation when a statistics row reaches capacity | internal: Elixir's persisted query engine (statistics_worker); the node evaluates FLOW.QUERY in memory from its shard catalogs (ADR-014 step 12) |

## `flow/query/telemetry_test.exs`

| Line | Test | Status |
|---|---|---|
| 25 | emits bounded resource measurements and non-sensitive plan metadata | internal: Elixir's persisted query engine (telemetry); the node evaluates FLOW.QUERY in memory from its shard catalogs (ADR-014 step 12) |
| 48 | reports failures without exposing the request | internal: Elixir's persisted query engine (telemetry); the node evaluates FLOW.QUERY in memory from its shard catalogs (ADR-014 step 12) |

## `flow/query/tuple_codec_test.exs`

| Line | Test | Status |
|---|---|---|
| 8 | uses stable golden encodings for scalar values | internal: Elixir's persisted query engine (tuple_codec); the node evaluates FLOW.QUERY in memory from its shard catalogs (ADR-014 step 12) |
| 26 | round trips supported values in both directions | internal: Elixir's persisted query engine (tuple_codec); the node evaluates FLOW.QUERY in memory from its shard catalogs (ADR-014 step 12) |
| 36 | canonicalizes signed floating zero to the equality semantics | internal: Elixir's persisted query engine (tuple_codec); the node evaluates FLOW.QUERY in memory from its shard catalogs (ADR-014 step 12) |
| 47 | ascending bytes preserve the declared total order | internal: Elixir's persisted query engine (tuple_codec); the node evaluates FLOW.QUERY in memory from its shard catalogs (ADR-014 step 12) |
| 80 | descending bytes reverse value ordering without changing component boundaries | internal: Elixir's persisted query engine (tuple_codec); the node evaluates FLOW.QUERY in memory from its shard catalogs (ADR-014 step 12) |
| 96 | comparison stays identical to encoded ordering at scalar boundaries | internal: Elixir's persisted query engine (tuple_codec); the node evaluates FLOW.QUERY in memory from its shard catalogs (ADR-014 step 12) |
| 128 | direct comparison matches encoded ordering across deterministic scalar samples | internal: Elixir's persisted query engine (tuple_codec); the node evaluates FLOW.QUERY in memory from its shard catalogs (ADR-014 step 12) |
| 155 | direct comparison preserves invalid-value rejection | internal: Elixir's persisted query engine (tuple_codec); the node evaluates FLOW.QUERY in memory from its shard catalogs (ADR-014 step 12) |
| 160 | both directions place concrete values before null and missing | internal: Elixir's persisted query engine (tuple_codec); the node evaluates FLOW.QUERY in memory from its shard catalogs (ADR-014 step 12) |
| 172 | composite prefixes are collision-free across embedded zero bytes | internal: Elixir's persisted query engine (tuple_codec); the node evaluates FLOW.QUERY in memory from its shard catalogs (ADR-014 step 12) |
| 183 | rejects unsupported or out-of-range values instead of producing unstable keys | internal: Elixir's persisted query engine (tuple_codec); the node evaluates FLOW.QUERY in memory from its shard catalogs (ADR-014 step 12) |

## `flow/query/usage_test.exs`

| Line | Test | Status |
|---|---|---|
| 6 | validates one canonical usage contract for records and counts | internal: Elixir's persisted query engine (usage); the node evaluates FLOW.QUERY in memory from its shard catalogs (ADR-014 step 12) |
| 35 | rejects impossible page and residual-check accounting | internal: Elixir's persisted query engine (usage); the node evaluates FLOW.QUERY in memory from its shard catalogs (ADR-014 step 12) |
| 42 | accepts covered records without inventing hydration work | internal: Elixir's persisted query engine (usage); the node evaluates FLOW.QUERY in memory from its shard catalogs (ADR-014 step 12) |
| 53 | allows residual checks against metadata rows without inventing log hydration | internal: Elixir's persisted query engine (usage); the node evaluates FLOW.QUERY in memory from its shard catalogs (ADR-014 step 12) |

## `flow/schedule_catchup_test.exs`

| Line | Test | Status |
|---|---|---|
| 6 | interval schedules expose the bounded catch-up policy | ported (`audit::schedule_catchups::only_intervals_take_the_fire_once_catch_up_policy`) |
| 27 | an overdue interval coalesces every missed occurrence into one fire | ported (`audit::schedule_catchups::an_overdue_interval_fires_once_for_every_missed_period`) |
| 84 | catch-up planning remains constant-work near the exact-integer limit | ported (`schedule::tests::catch_up_is_constant_work_and_an_elapsed_end_wins`) |
| 100 | an interval less than one period late keeps its original cadence | ported (`audit::schedule_catchups::an_overdue_interval_fires_once_for_every_missed_period`) |
| 128 | catch-up counts only occurrences within an interval schedule end bound | ported (`audit::schedule_catchups::catch_up_counts_only_periods_within_the_end_bound`) |
| 160 | catch-up remains independent from overlap skipping | ported (`audit::schedule_catchups::catch_up_runs_alongside_overlap_skips`) |
| 197 | a queued overlap fires once and coalesces intervals missed while waiting | ported (`audit::schedule_catchups::a_queued_or_paused_interval_catches_up_in_one_fire`) |
| 262 | resuming an overdue interval coalesces the paused periods into one fire | ported (`audit::schedule_catchups::a_queued_or_paused_interval_catches_up_in_one_fire`) |
| 308 | an overlap skip completes when catch-up passes the interval end bound | ported (`audit::schedule_catchups::catch_up_counts_only_periods_within_the_end_bound`) |
| 348 | an overlap skip completes safely at the exact timestamp limit | ported (`audit::schedule_catchups::catch_up_runs_alongside_overlap_skips`; its internal retention-deadline read is Elixir's) |
| 401 | an elapsed end bound takes precedence over recovery timestamp overflow | ported (`schedule::tests::catch_up_is_constant_work_and_an_elapsed_end_wins`) |
| 416 | max_fires keeps precedence when the end bound is reached by the same fire | ported (`audit::schedule_catchups::catch_up_counts_only_periods_within_the_end_bound`) |
| 445 | a batch catch-up total stays within the public exact-integer contract | fixed (`audit::schedule_catchups::a_batch_catch_up_total_stays_an_exact_integer`, `schedule::tests::a_later_claim_failure_is_reported_beside_the_outcomes`; G52) |
| 479 | catchup_policy accepts only fire_once on interval schedules | ported (`audit::schedule_catchups::only_intervals_take_the_fire_once_catch_up_policy`) |

## `flow/schedule_dispatcher_test.exs`

| Line | Test | Status |
|---|---|---|
| 6 | claims bounded waves only after the previous wave finishes | internal: Elixir's dispatcher waves and task concurrency; the node claims per shard and fires in due order |
| 57 | single-record waves avoid task creation | internal: Elixir's task creation |
| 66 | returns completed work when a later claim fails | covered (G51: the summary keeps completed outcomes beside a later claim failure, `schedule::tests::a_later_claim_failure_is_reported_beside_the_outcomes`) |

## `flow/schedule_recovery_test.exs`

| Line | Test | Status |
|---|---|---|
| 18 | schedule record and due index survive application restart | ported (`audit::schedule_recovery::schedules_their_catch_up_and_backoff_survive_a_restart`) |
| 70 | overdue interval catch-up remains bounded after application restart | ported (`audit::schedule_recovery::schedules_their_catch_up_and_backoff_survive_a_restart`) |
| 143 | queued overlap keeps its retry deadline and backoff across restart | ported (`audit::schedule_recovery::schedules_their_catch_up_and_backoff_survive_a_restart`) |
| 199 | claimed schedule is reclaimable after shard leader restart | ported (`audit::schedule_recovery::a_lease_a_lost_fire_left_is_reclaimed_after_a_restart`) |

## `flow/schedule_retention_members_test.exs`

| Line | Test | Status |
|---|---|---|
| 62 | a recurring schedule keeps a bounded number of retention cleanup members | ported (`audit::schedule_recovery::a_schedule_s_history_stays_bounded`; its keydir members are Elixir's) |
| 82 | a new schedule's Flow keeps a short history | ported (`audit::schedule_recovery::a_schedule_s_history_stays_bounded`) |
| 90 | a schedule created with a long history takes the short one on its next fire | internal: changing Elixir's history setting at run time; the node's schedules always keep 100 events |
| 105 | a long-history schedule held back by overlap keeps a bounded number of retention cleanup members | ported (`audit::schedule_recovery::a_schedule_s_history_stays_bounded`) |
| 138 | a long-history schedule held back by overlap (#{policy}) takes the short history | internal: changing Elixir's history setting at run time; the node's schedules always keep 100 events |

## `flow/schedule_summary_test.exs`

| Line | Test | Status |
|---|---|---|
| 6 | reports a later claim failure separately from claimed schedule outcomes | fixed (`schedule::tests::a_later_claim_failure_is_reported_beside_the_outcomes`; G51) |
| 23 | omits claim_error from the normal response | ported (`schedule::tests::a_later_claim_failure_is_reported_beside_the_outcomes`) |

## `flow/schedule_test.exs`

| Line | Test | Status |
|---|---|---|
| 59 | delay schedule defaults are based on the cluster-adjusted clock | ported (`audit::schedule_fires::a_delay_starts_from_the_command_time`; the node's command clock is its wall clock, Elixir's hybrid-clock hook is its own) |
| 85 | one-shot schedule fires target flow once and completes schedule | ported (`audit::schedule_fires::a_one_shot_creates_its_target_once`) |
| 132 | schedule views preserve the complete recurrence configuration | ported (`audit::schedule_fires::views_keep_the_whole_recurrence`; its internal-record field checks are Elixir's) |
| 198 | scheduled targets preserve Flow create resource governance and activity | internal: Elixir's pluggable `FerricStore.ResourceLimits` probe (acquire, activity, release callbacks); the node has no such hook, and a scheduled target is a FLOW.CREATE on its own shard (`composition::schedule::fire`) |
| 244 | fire_due drains multiple bounded claim waves without losing schedules | ported (`audit::schedule_fires::fire_due_drains_every_claim_wave_up_to_its_limit`) |
| 277 | fire_due returns after an exact full dispatch wave below the requested limit | ported (`audit::schedule_fires::fire_due_drains_every_claim_wave_up_to_its_limit`) |
| 307 | interval schedule reschedules without consuming retry attempts | ported (`audit::schedule_fires::an_interval_reschedules_without_spending_attempts`) |
| 368 | concurrent schedule firing leases a due schedule once | ported (`audit::schedule_fires::concurrent_fire_due_leases_a_schedule_once`) |
| 412 | concurrent manual fires fence before creating distinct targets | ported (`audit::schedule_fires::concurrent_manual_fires_number_their_targets_apart`: the node's shard serializes the two fires, so one is refused while the other holds the lease, or both fire under numbers 1 and 2; never two targets for one number) |
| 467 | recurring schedule rejects fixed target id | covered (`schedule::tests::targets_and_end_conditions_follow_the_kind`) |
| 476 | schedule target rejects ambiguous fixed and generated ids | covered (`schedule::tests::targets_and_end_conditions_follow_the_kind`) |
| 486 | schedule creation rejects timing fields from another kind | covered (`schedule::tests::timing_fields_belong_to_their_kind`) |
| 506 | schedule creation rejects ambiguous absolute start timestamps | covered (`schedule::tests::timing_fields_belong_to_their_kind`) |
| 518 | overlap retry is accepted only by queue_after_previous | covered (`schedule::tests::targets_and_end_conditions_follow_the_kind`) |
| 532 | public flow APIs reject internal schedule namespace | fixed (`audit::schedule_namespace::public_flow_commands_keep_out_of_the_schedule_namespace`; G45: FLOW.GET returned a schedule's internal record) |
| 550 | public callers cannot forge the internal schedule capability | internal: Elixir's in-process capability token; on the wire `__ferricstore_internal__` is an unknown option (`audit::schedule_namespace::public_flow_commands_keep_out_of_the_schedule_namespace`) |
| 574 | internal schedule creation rejects non-active states | internal: `Flow.create_internal/3`, the scheduler's own create; the node's schedule ops write only active schedules (`supervision::shard::state::schedule`) |
| 611 | schedule replacement cannot mutate ordinary flow records | internal: `Router.flow_schedule_replace/2`; the node's schedule ops read only records of the schedule type (`ShardState::stored`) |
| 649 | non-claim schedule replacement rejects scheduler-internal states | internal: `Router.flow_schedule_replace/2` and its metadata checks; the node rewrites schedules only through its own ops |
| 709 | schedule APIs reject non-keyword option lists without raising | internal: Elixir's keyword-list option shape; a wire payload is a map (`ERR native ... payload must be a map`) |
| 724 | schedule APIs report invalid identifiers before storage access | ported (`audit::schedule_api::schedule_ids_are_non_empty_texts`) |
| 736 | schedule creation rejects unknown and duplicate options | fixed (`audit::schedule_api::each_command_takes_only_its_own_options`; G42; duplicate options have no wire form) |
| 756 | schedule creation rejects malformed, unknown, and duplicate target fields | fixed (`audit::schedule_api::a_target_is_a_map_that_can_create_a_flow`; G44: a list target answered "target is required"; duplicate fields have no wire form) |
| 778 | schedule listing rejects unknown and duplicate options | fixed (`audit::schedule_api::each_command_takes_only_its_own_options`; G42) |
| 786 | schedule operations reject unknown and duplicate options before storage access | fixed (`audit::schedule_api::each_command_takes_only_its_own_options`; G42) |
| 812 | schedule listing rejects invalid states and inverted time ranges | ported (`audit::schedule_api::listing_names_its_states_and_keeps_ranges_ordered`) |
| 820 | manual fire without a timestamp uses the current command time | ported (`audit::schedule_fires::a_manual_fire_without_a_time_uses_the_command_time`) |
| 841 | schedule get keeps internal routing and payload options authoritative | ported (`audit::schedule_api::get_keeps_its_internal_routing_and_payload_options`; G42 keeps them accepted) |
| 867 | stored schedules remain hydratable when the public payload return limit changes | internal: Elixir's application env `flow_payload_return_max_bytes`; the node reads a definition from its own record, with no runtime payload-return setting |
| 901 | schedule rejects large inline target payloads | fixed (`audit::schedule_api::inline_target_values_stay_small`; G44) |
| 913 | schedule rejects large inline target named values | fixed (`audit::schedule_api::inline_target_values_stay_small`; G44) |
| 925 | schedule rejects reserved target ids and prefixes | ported (`audit::schedule_api::a_target_is_a_map_that_can_create_a_flow`; `schedule::tests::targets_and_end_conditions_follow_the_kind`) |
| 946 | schedule rejects target definitions that cannot create a flow | ported (`audit::schedule_api::a_target_is_a_map_that_can_create_a_flow`) |
| 968 | schedule time inputs are bounded and cron conversion never raises | ported (`audit::schedule_api::time_inputs_stay_within_exact_and_calendar_bounds`) |
| 1013 | cron fire at the calendar ceiling completes after emitting exactly one target | fixed (`audit::schedule_lifecycle::the_last_cron_fire_the_calendar_allows_completes_the_schedule`, `schedule::cron_tests::the_calendar_s_last_minute_still_plans`; G50) |
| 1062 | duplicate target id must belong to the schedule before it is treated as idempotent | covered (`flow_schedule::schedules_pause_resume_delete_and_overwrite`, its owned-target case) |
| 1094 | predictable scheduler correlation cannot forge target ownership | ported (`audit::schedule_lifecycle::the_scheduler_correlation_does_not_forge_ownership`) |
| 1140 | scheduler-owned target is accepted after a crash before schedule completion | internal: building the owner attributes needs the schedule's ownership secret, which only the scheduler reads; a retried fire accepts its own target through `target::owned` (`composition::schedule::fire::create_target`) |
| 1196 | duplicate schedule create returns already exists unless overwrite is explicit | covered (`flow_schedule::schedules_pause_resume_delete_and_overwrite`) |
| 1223 | schedule overwrite atomically replaces active definition | ported (`audit::schedule_lifecycle::an_overwrite_replaces_the_whole_definition_at_once`) |
| 1346 | schedule overwrite rejects a currently leased schedule | fixed (`audit::schedule_lifecycle::an_overwrite_waits_for_a_live_lease_on_the_node_clock`; G46) |
| 1389 | schedule overwrite cannot expire a live lease with caller time | fixed (`audit::schedule_lifecycle::an_overwrite_waits_for_a_live_lease_on_the_node_clock`; G46) |
| 1432 | schedule overwrite can reactivate a completed one-shot schedule | covered (`flow_schedule::schedules_pause_resume_delete_and_overwrite`) |
| 1478 | schedule target can use payload_ref and value_refs | ported (`audit::schedule_lifecycle::a_target_can_take_its_payload_and_values_by_reference`) |
| 1524 | schedule list filters schedules by kind timezone target type and due range | covered (`flow_schedule::listing_filters_schedules_and_orders_them_by_due_time`) |
| 1569 | schedule list applies reverse ordering before count | covered (`flow_schedule::listing_filters_schedules_and_orders_them_by_due_time`) |
| 1609 | schedule list keeps terminal schedules last when reverse ordering | ported (`audit::schedule_lifecycle::listings_keep_finished_schedules_out_of_due_ranges_and_last_in_reverse`) |
| 1661 | schedule list pages past more candidates than one catalog page | internal: Elixir's catalog page size; the node lists every shard's schedules in one pass |
| 1701 | schedule list preserves global ordering across catalog pages and shards | covered: order across shards (`flow_schedule::listing_filters_schedules_and_orders_them_by_due_time` on 4 shards); catalog pages are Elixir's |
| 1737 | schedule list finds a filtered match after earlier catalog pages | internal: Elixir's catalog pages; the filter is covered (`flow_schedule::listing_filters_schedules_and_orders_them_by_due_time`) |
| 1763 | exact catalog and async projections handle long schedule state keys | internal: `Flow.create_internal/3` and the async history projector |
| 1800 | schedule due ranges exclude terminal schedules without a next run | ported (`audit::schedule_lifecycle::listings_keep_finished_schedules_out_of_due_ranges_and_last_in_reverse`) |
| 1829 | schedule list fails closed when its exact catalog is unavailable | internal: an instance context with a missing shard |
| 1841 | schedule exact catalog tracks retention without a second membership index | ported (`audit::schedule_lifecycle::retention_removes_a_deleted_schedule_from_listings`) |
| 1896 | recurring schedule completes after max_fires | covered (`flow_schedule::schedules_fire_their_targets_until_they_end`) |
| 1933 | recurring schedule completes when next fire would pass end_at_ms | covered (`flow_schedule::schedules_fire_their_targets_until_they_end`) |
| 1968 | recurring schedule completes when its next timestamp exceeds the exact limit | ported (`audit::schedule_overlaps::exact_integer_bounds_end_a_schedule_cleanly`) |
| 1999 | one-shot and delay schedules reject recurring end conditions | covered (`schedule::tests::targets_and_end_conditions_follow_the_kind`) |
| 2018 | manual schedule fire targets one schedule immediately | ported (`audit::schedule_overlaps::a_manual_skip_past_the_end_bound_completes`) |
| 2043 | pause and resume controls schedule firing | covered (`flow_schedule::schedules_pause_resume_delete_and_overwrite`) |
| 2075 | operator schedule mutations reject stale expected state and version | fixed (`audit::schedule_api::operator_changes_keep_to_their_expected_state_and_version`, `audit::schedule_api::a_guarded_overwrite_needs_the_schedule_as_expected`; G43) |
| 2123 | schedule lifecycle writes explicit signal history events | fixed (`audit::schedule_overlaps::a_schedule_records_its_lifecycle_as_signals`; G48) |
| 2154 | overlap policy skip advances recurring schedule without creating a new target | covered (`flow_schedule::overlap_policies_hold_back_a_schedule_while_its_target_runs`) |
| 2185 | strict overlap state reads use the replicated state-only path | internal: `Router.flow_consistent_state/3`; the node reads the previous target with FLOW.GET on its shard |
| 2209 | strict overlap state reads include hibernated targets | internal: Elixir's hibernation; the node's FLOW.GET reads a cold Flow from disk |
| 2240 | overlap policy allow does not depend on reading the previous target | internal: a corrupted raw row; the node reads no previous target for `allow` (`composition::schedule::fire::fire_one`) |
| 2297 | overlap policy queue_after_previous preserves queued due occurrence | covered (`flow_schedule::overlap_policies_hold_back_a_schedule_while_its_target_runs`) |
| 2345 | repeated unchanged overlap backs off while preserving the queued occurrence | covered (`schedule::tests::a_repeated_overlap_backs_off_on_the_same_occurrence`) |
| 2394 | target completion wakes a backed-off queued schedule without changing its occurrence | fixed (`audit::schedule_overlaps::a_finished_target_wakes_its_backed_off_queued_schedule`; G47) |
| 2478 | queued overlap terminates cleanly when its retry timestamp would overflow | ported (`audit::schedule_overlaps::exact_integer_bounds_end_a_schedule_cleanly`) |
| 2517 | manual queued overlap terminates cleanly when its retry timestamp would overflow | fixed (`audit::schedule_overlaps::exact_integer_bounds_end_a_schedule_cleanly`; G49: the manual lease passed the exact-integer bound) |
| 2554 | manual overlap skip completes when the next occurrence passes the end bound | ported (`audit::schedule_overlaps::a_manual_skip_past_the_end_bound_completes`) |
| 2587 | manual overlap skip terminates cleanly at the exact timestamp limit | fixed (`audit::schedule_overlaps::exact_integer_bounds_end_a_schedule_cleanly`; G49) |
| 2624 | overlap policy fail_schedule marks recurring schedule failed | ported (`audit::schedule_overlaps::fail_schedule_keeps_the_overlap_that_failed_it`) |
| 2656 | manual fail_schedule overlap persists the same terminal details | ported (`audit::schedule_overlaps::fail_schedule_keeps_the_overlap_that_failed_it`) |
| 2692 | schedule delete cancels future schedule | fixed (`audit::schedule_overlaps::a_schedule_records_its_lifecycle_as_signals`; G48) |
| 2725 | invalid cron is rejected during schedule creation | ported (`schedule::cron_tests::malformed_expressions_are_refused_with_their_reason`) |
| 2734 | recurrence planning failures do not create orphan targets | internal: a stored definition corrupted in place; its reason text is ported (`schedule::cron_tests::malformed_expressions_are_refused_with_their_reason`) |
| 2799 | recurrence planning failures terminate an overlap skip | internal: a stored definition corrupted in place during an overlap skip |
| 2836 | manual overlap skip persists recurrence planning failures | internal: a stored definition corrupted in place during a manual overlap skip |
| 2876 | invalid cron range is rejected during schedule creation | ported (`schedule::cron_tests::malformed_expressions_are_refused_with_their_reason`) |
| 2888 | cron fields reject empty comma elements before searching the time window | ported (`schedule::cron_tests::malformed_expressions_are_refused_with_their_reason`) |
| 2902 | cron rejects impossible calendar combinations before searching minutes | ported (`schedule::cron_tests::malformed_expressions_are_refused_with_their_reason`) |
| 2915 | cron accepts leap-day schedules independently of mutable application limits | ported (`schedule::cron_tests::aliases_ranges_steps_and_leap_days_plan_their_runs`) |
| 2939 | cron schedule supports aliases, ranges, and steps | ported (`schedule::cron_tests::aliases_ranges_steps_and_leap_days_plan_their_runs`) |
| 2988 | cron schedule supports weekday aliases | ported (`schedule::cron_tests::aliases_ranges_steps_and_leap_days_plan_their_runs`) |
| 3004 | cron schedule matches wall-clock time in configured timezone | ported (`schedule::cron_tests::zones_and_daylight_saving_folds_plan_wall_clock_times`) |
| 3027 | a schedule fire that fails transiently is released for the next poll | internal: Elixir's fault-injection hook; on a failure that is not a refusal the node hands the schedule back at once (`composition::schedule::fire::fire_one`) |
| 3073 | cron schedule handles repeated DST wall-clock minute as distinct due times | ported (`schedule::cron_tests::zones_and_daylight_saving_folds_plan_wall_clock_times`) |
| 3100 | cron chooses the earliest instant across multiple DST fallback wall minutes | ported (`schedule::cron_tests::zones_and_daylight_saving_folds_plan_wall_clock_times`) |
| 3113 | due schedule is not missed after hibernation removes its hot due key | internal: Elixir's LMDB writer and Raft acceptor race |
| 3194 | cron schedule rejects invalid timezone | ported (`schedule::cron_tests::malformed_expressions_are_refused_with_their_reason`) |
| 3207 | cron schedule fires on matching UTC minutes and reschedules to next match | ported (`audit::schedule_lifecycle::a_cron_fires_on_matching_minutes_and_moves_to_the_next`) |
| 3263 | runtime bookkeeping cannot make an accepted schedule unreadable | covered by bounds: a schedule's record holds up to 512 MiB and its target's inline values 8 KiB (G44), with ids at most 256 bytes, so runtime bookkeeping cannot outgrow it |
| 3317 | schedule creation reserves hydration capacity for bounded runtime state | internal: Elixir's configurable definition and hydration limits (`Schedule.Limits`) |

## `flow/scheduler_test.exs`

| Line | Test | Status |
|---|---|---|
| 9 | a target's terminal wakes its queued schedule from the shard leader only | internal: Elixir's GenServer cast and leader check; the node's shards deliver wakes only while they serve (`supervision::shard::run`, G47) |
| 32 | configuration validates the batch limit and is a stable snapshot | internal: Elixir's scheduler settings (application env) |
| 60 | configuration defaults the runner on but fails closed for invalid boolean values | internal: Elixir's scheduler settings (application env) |
| 75 | configuration caps the scheduler batch at the claim command maximum | internal: Elixir's scheduler settings (application env) |
| 94 | a claimed batch drains immediately even when every schedule was skipped | internal: the scheduler's own loop; the node's poll counts a skip as activity and polls again (`composition::schedule::fire_due`, `supervision::scheduler`) |
| 111 | a scheduler waiting on an empty store wakes for a newly-created future schedule | ported (`audit::schedule_recovery::the_poll_fires_a_future_schedule_and_a_woken_queue`; an idle poll claims nothing and logs nothing) |
| 160 | orphaned cold schedules and unrelated cold work do not append empty claims | internal: Elixir's cold LMDB due keys |
| 236 | a committed target completion wakes a queued schedule before its fallback retry | fixed (`audit::schedule_recovery::the_poll_fires_a_future_schedule_and_a_woken_queue`; G47) |

## `flow_governance_approval_catalog_repair_test.exs`

| Line | Test | Status |
|---|---|---|
| 8 | catalog bulk unregister removes one bounded page in one write | internal: Elixir's approval catalog repair |
| 29 | source repair restores membership created during a batched stale removal | internal: Elixir's approval catalog repair |
| 59 | target repair restores membership that becomes valid during batched removal | internal: Elixir's approval catalog repair |
| 81 | source repair restores uncertain membership when post-delete validation fails | internal: Elixir's approval catalog repair |
| 109 | target repair restores uncertain membership when post-delete validation fails | internal: Elixir's approval catalog repair |

## `flow_governance_cache_session_store_test.exs`

| Line | Test | Status |
|---|---|---|
| 26 | session heads reject trailing external-term bytes | internal: Elixir's limit cache sessions and their durable pages |
| 42 | session manifests reject trailing external-term bytes | internal: Elixir's limit cache sessions and their durable pages |
| 60 | session pages reject trailing external-term bytes | internal: Elixir's limit cache sessions and their durable pages |
| 94 | power loss before activation releases every durably proven-unused page | internal: Elixir's limit cache sessions and their durable pages |
| 148 | restart retains the uncertain consumption window and releases later unused pages | internal: Elixir's limit cache sessions and their durable pages |
| 194 | a newer cache session fences stale persistence and activation | internal: Elixir's limit cache sessions and their durable pages |
| 233 | acknowledged activated pages advance a bounded recovery floor and are deleted | internal: Elixir's limit cache sessions and their durable pages |
| 264 | acknowledge deletes its page before advancing the recovery floor | internal: Elixir's limit cache sessions and their durable pages |
| 313 | discarded denied plans advance the recovery floor without durable holes | internal: Elixir's limit cache sessions and their durable pages |
| 341 | acknowledging an early gap compacts a bounded run of later deleted pages | internal: Elixir's limit cache sessions and their durable pages |
| 385 | floor compaction does not treat an unavailable page read as deletion | internal: Elixir's limit cache sessions and their durable pages |
| 428 | discard cannot delete a page that won an activation race | internal: Elixir's limit cache sessions and their durable pages |
| 463 | partial manifest persistence failure deletes every unspent reserved page | internal: Elixir's limit cache sessions and their durable pages |
| 499 | discard deletes its pages before advancing the recovery floor | internal: Elixir's limit cache sessions and their durable pages |
| 546 | discard rejects pages owned by another cache session | internal: Elixir's limit cache sessions and their durable pages |
| 574 | completed recovery severs the historical session chain | internal: Elixir's limit cache sessions and their durable pages |
| 609 | recovery resumes a durable metadata cleanup checkpoint after process loss | internal: Elixir's limit cache sessions and their durable pages |
| 649 | recovery repairs a stale cursor after metadata deletion but before checkpoint clearing | internal: Elixir's limit cache sessions and their durable pages |
| 690 | corrupt oversized manifest pages fail closed without invoking release | internal: Elixir's limit cache sessions and their durable pages |
| 745 | recovery rejects cursors outside the durable exact-integer boundary | internal: Elixir's limit cache sessions and their durable pages |
| 760 | opening a session fails closed when the durable generation is exhausted | internal: Elixir's limit cache sessions and their durable pages |
| 781 | session open reuses staged metadata after head publication is interrupted | internal: Elixir's limit cache sessions and their durable pages |
| 818 | session pages use the same reservation id byte boundary as LimitStore | internal: Elixir's limit cache sessions and their durable pages |
| 849 | transient recovery release failure retains its cursor and retries before compaction | internal: Elixir's limit cache sessions and their durable pages |
| 907 | transient recovery page deletion failure retains its cursor before metadata compaction | internal: Elixir's limit cache sessions and their durable pages |
| 969 | unavailable recovery page read retains its cursor without releasing or compacting | internal: Elixir's limit cache sessions and their durable pages |
| 1021 | session key families co-locate internally and distribute by node and instance | internal: Elixir's limit cache sessions and their durable pages |
| 1032 | graceful flush releases activated and unopened manifest pages | internal: Elixir's limit cache sessions and their durable pages |
| 1072 | clear without context refuses pending pages left by a partial flush failure | internal: Elixir's limit cache sessions and their durable pages |
| 1111 | expired pending pages are exact-released after the durable lease is renewed | internal: Elixir's limit cache sessions and their durable pages |
| 1165 | newer policy config fences and releases prefetched credits before a cache hit | internal: Elixir's limit cache sessions and their durable pages |
| 1223 | failed policy-fence release persists retry pages instead of losing detached ids | internal: Elixir's limit cache sessions and their durable pages |
| 1283 | large valid cache spend clamps prefetch to the store mutation maximum | internal: Elixir's limit cache sessions and their durable pages |
| 1313 | crash after exact spend but before cache activation leaves prefetched ids recoverable | internal: Elixir's limit cache sessions and their durable pages |
| 1356 | LimitCache process restart automatically recovers untouched manifest pages | internal: Elixir's limit cache sessions and their durable pages |

## `flow_governance_catalog_test.exs`

| Line | Test | Status |
|---|---|---|
| 6 | approval catalog repair rejects non-canonical and oversized progress | internal: Elixir's approval catalog repair progress |
| 43 | governance list paths never enumerate the full application keyspace | internal: Elixir's catalog traversal |
| 55 | governance catalog traversal is unbounded in count but bounded in memory | internal: Elixir's catalog traversal |
| 78 | catalog collection keeps global top results across pages | internal: Elixir's catalog paging |
| 107 | exact-scope lists derive record keys without scanning catalog membership | covered for exact-scope listings (`flow_approval::listings_filter_and_order_across_shards`, `flow_overview::partition_scopes_filter_the_overview`); deleting catalog membership in place is Elixir's |
| 174 | catalog paging anchors on the last member during concurrent insertion | internal: Elixir's catalog paging |
| 208 | governance catalogs preserve limit, budget, circuit, and approval listing | covered (`flow_overview::the_overview_summarizes_every_kind_across_shards`, `flow_approval::listings_filter_and_order_across_shards`); circuits have no list command on the wire |
| 259 | approval NX conflicts cannot poison attacker-supplied exact catalogs | internal: Elixir's exact approval catalogs |
| 312 | filtered approval lists repair missing and stale exact membership | internal: Elixir's exact approval catalog repair |
| 353 | approval exact repair is bounded and resumes from durable progress | internal: Elixir's exact approval catalog repair |

## `flow_governance_exhaustive_review_test.exs`

| Line | Test | Status |
|---|---|---|
| 30 | budget reservation retries are idempotent and cannot reuse a reservation id | ported (`audit::governance_bounds::a_reservation_id_retries_but_is_never_reused`) |
| 56 | budget active reservations are bounded without breaking idempotent retries | ported (`governance::tests::a_budget_holds_at_most_4096_active_reservations`) |
| 107 | budget validation rejects undercounted current-window reservations | internal: Elixir's model checks on hand-built structs |
| 116 | budget validation rejects impossible released reservation payloads | internal: Elixir's model checks on hand-built structs |
| 133 | budget validation rejects oversized scopes and aggregate records | internal: Elixir's model checks on hand-built structs |
| 163 | policy string-list deduplication preserves first-seen order | internal: Elixir's model checks on hand-built structs; the wire's state governance is covered by `flow_policy_governance::state_governance_is_normalized_merged_and_kept` |
| 172 | policy resolution ignores unsupported effect-specific policy maps | internal: type-level governance is Elixir's in-process API (see Recorded decisions) |
| 186 | policy string lists stop at the governance collection bound | internal: Elixir's model checks on hand-built structs |
| 193 | policy circuit error classes have a bounded persisted width | internal: Elixir's model checks on hand-built structs |
| 208 | policy rule maps stop at the governance collection bound | internal: Elixir's model checks on hand-built structs |
| 215 | policy rule names stop at the durable dimension bound | internal: Elixir's model checks on hand-built structs |
| 222 | normalized governance policies have a fixed aggregate byte budget | internal: Elixir's model checks on hand-built structs |
| 229 | policy rules reject integers outside the exact durable range | internal: Elixir's model checks on hand-built structs |
| 245 | ignored half-open failures release their probe without closing the circuit | internal: type-level governance is Elixir's in-process API (see Recorded decisions) |
| 269 | circuit records reject oversized error-class collections | internal: Elixir's model checks on hand-built structs (`governance::tests::stored_circuits_round_trip_and_refuse_damage`; error-class bound covered by `flow_circuit::impossible_rules_are_refused_before_anything_is_stored`) |
| 275 | circuit records reject oversized scope dimensions | internal: Elixir's model checks on hand-built structs (`governance::tests::stored_circuits_round_trip_and_refuse_damage`) |
| 279 | circuit records bound configured and observed error-class widths | internal: Elixir's model checks on hand-built structs (`governance::tests::stored_circuits_round_trip_and_refuse_damage`) |
| 297 | circuit records reject impossible half-open probe counters | internal: Elixir's model checks on hand-built structs (`governance::tests::stored_circuits_round_trip_and_refuse_damage`) |
| 314 | release outbox can persist its terminal acknowledged metadata | internal: Elixir's release outbox |
| 328 | release outbox rejects structurally invalid metadata before acknowledgement | internal: Elixir's release outbox |
| 340 | release outbox bounds unreconciled durable intents | internal: Elixir's release outbox |
| 350 | limit catalog outbox rejects structurally invalid metadata before acknowledgement | internal: Elixir's limit catalog outbox |
| 355 | limit catalog outbox terminal acknowledgement retries remain idempotent | internal: Elixir's limit catalog outbox |
| 366 | release outbox cursors reject sequences outside the durable integer range | internal: Elixir's release outbox |
| 373 | approval records reject terminal decisions at or after their expiry | ported (`audit::governance_bounds::approval_deadlines_expire_once_and_times_never_run_backward`) |
| 392 | governance models reject integers outside the exact durable range | internal: Elixir's model checks on hand-built structs; through the wire covered by `audit::governance_bounds::approval_text_and_stored_integers_stay_bounded` |
| 410 | limit owner decoding rejects malformed detached reservations before normalization | internal: Elixir's model checks on hand-built structs (`governance::tests::stored_limits_round_trip_and_refuse_damage`) |
| 418 | limit owner decoding enforces capacity, epoch, and reclaim invariants | internal: Elixir's model checks on hand-built structs (`governance::tests::stored_limits_round_trip_and_refuse_damage`) |
| 433 | limit owner records reject oversized scope dimensions | internal: Elixir's model checks on hand-built structs (`governance::tests::stored_limits_round_trip_and_refuse_damage`) |
| 438 | credit reclaim selection is deterministic across large shard maps | internal: Elixir's credit reclaim selection over shard maps |
| 470 | limit cleanup checkpoints reject page numbers outside the durable page bound | internal: Elixir's limit cleanup checkpoints |
| 493 | cache-session public helpers reject malformed sessions and options without raising | internal: Elixir's cache sessions |
| 511 | governance entrypoints reject non-keyword option lists without touching storage | internal: Elixir's keyword-list option shape; a wire payload is a map |
| 573 | scope parsing rejects malformed exported arguments without raising | internal: Elixir's exported scope parser |
| 583 | maintenance and ledger boundaries reject invalid finite inputs | internal: Elixir's maintenance entry points |
| 593 | telemetry uses a bounded stable code for arbitrary error terms | internal: Elixir's telemetry |
| 615 | credit leases reject generation overflow before creating an invalid owner | internal: a lease generation at 2^53, which no client sequence reaches |
| 627 | credit lease top-ups never shorten an existing deadline | ported (`audit::governance_bounds::circuit_and_lease_times_keep_their_order`) |
| 649 | credit lease initial grants set an exact deadline | ported (`audit::governance_bounds::circuit_and_lease_times_keep_their_order`) |
| 658 | credit lease spends reject timestamps older than the previous spend | ported (`audit::governance_bounds::circuit_and_lease_times_keep_their_order`) |
| 679 | budget transitions reject timestamps that move durable state backward | ported (`audit::governance_bounds::budget_times_and_totals_never_run_backward_or_past_2_pow_53`) |
| 695 | budget validation rejects impossible reservation timestamps | internal: Elixir's model checks on hand-built structs |
| 720 | circuit validation rejects backward event history | covered through the wire (`audit::governance_bounds::circuit_and_lease_times_keep_their_order`) |

## `flow_governance_exhaustive_store_review_test.exs`

| Line | Test | Status |
|---|---|---|
| 19 | limit-cache flush releases each bounded batch before detaching the next one | internal: Elixir's limit cache |
| 53 | atomic governance records reject oversized encoded values before registration | internal: Elixir's atomic record registration |
| 70 | approval requests reject oversized aggregate records before catalog registration | covered by bounds (`audit::governance_bounds::approval_text_and_stored_integers_stay_bounded`) |
| 90 | expired pending cache pages use the configured exact-release contract | internal: Elixir's cache pages |
| 144 | one cache spend bounds expired pending-page cleanup work | internal: Elixir's cache pages |
| 195 | cache recovery derives progress from durable metadata instead of trusting a forged cursor | internal: Elixir's cache recovery |
| 240 | circuit mutations preserve configured values when options omit them | covered (`flow_circuit::manual_opens_and_closes_keep_the_rule`) |
| 264 | circuit mutations reject oversized error-class collections before storage | covered (`flow_circuit::impossible_rules_are_refused_before_anything_is_stored`) |
| 287 | circuit mutations fingerprint oversized observed error classes | internal: type-level governance is Elixir's in-process API (see Recorded decisions) |
| 308 | effect reservation retries consult the existing decision before changed policy | covered (`flow_effect::reservations_replay_for_their_digest_and_conflict_for_another`) |
| 346 | effect decoding rejects aggregate records above the durable byte budget | internal: Elixir's effect codec (`effect::tests::stored_records_round_trip_and_refuse_damage`) |
| 381 | an effect reservation that loses the NX race releases its half-open probe | internal: type-level governance is Elixir's in-process API (see Recorded decisions); its NX race is Elixir's storage |
| 451 | invalid terminal effect metadata is rejected without changing status | covered (`flow_effect::refused_options_and_older_times_leave_the_effect_unchanged`) |
| 482 | corrupt circuit state returns an error instead of crashing effect reservation | internal: a corrupted circuit record |
| 507 | repeated terminal effect updates do not duplicate ledger events | covered (`flow_effect::terminal_moves_are_recorded_once_and_listed_by_window`) |
| 542 | ledger append uses the CAS index as its only authoritative record | internal: Elixir's CAS ledger index |
| 567 | effect reservation is not acknowledged until its ledger event is durable | internal: Elixir's fault injection; the node replies once the round, ledger included, is durable |
| 610 | terminal effect retry completes a ledger side effect that previously failed | internal: Elixir's fault injection |
| 668 | terminal effect retry completes a circuit side effect that previously failed | internal: Elixir's fault injection |
| 725 | ledger codec rejects a status that does not match its event kind | internal: Elixir's ledger codec (`effect::tests::stored_records_round_trip_and_refuse_damage`) |
| 751 | ledger indexes retain newest events within a fixed byte budget | covered (`effect::tests::the_ledger_keeps_the_newest_events_within_its_byte_budget`) |
| 785 | governance lists surface corrupt cataloged records | internal: corrupted catalog records |
| 802 | exact approval lists preserve and surface corrupt cataloged records | internal: corrupted catalog records |
| 824 | approval decisions atomically expire requests at their deadline | ported (`audit::governance_bounds::approval_deadlines_expire_once_and_times_never_run_backward`) |
| 851 | approval requests reject an absolute deadline that is not in the future | ported (`audit::governance_bounds::approval_deadlines_expire_once_and_times_never_run_backward`) |
| 865 | approval decisions reject time regression without corrupting the record | ported (`audit::governance_bounds::approval_deadlines_expire_once_and_times_never_run_backward`) |
| 884 | budget commits reject unsafe usage terms without corrupting the record | covered (`flow_budget::refused_commits_leave_the_record_as_it_was`, `budget::request::tests::usage_is_bounded_as_elixir_walks_it`) |
| 916 | oversized governance storage keys are rejected before catalog publication | ported (`audit::governance_bounds::oversized_governance_keys_are_refused_before_anything_is_stored`) |
| 963 | governance stores reject integers outside the exact durable range | ported (`audit::governance_bounds::approval_text_and_stored_integers_stay_bounded`) |
| 994 | budget commits reject aggregate usage outside the exact durable range | ported (`audit::governance_bounds::budget_times_and_totals_never_run_backward_or_past_2_pow_53`) |
| 1023 | limit reconciliation rejects time outside the exact durable range | internal: Elixir's limit reconciler |
| 1033 | circuit mutations reject timestamps older than durable state | ported (`audit::governance_bounds::circuit_and_lease_times_keep_their_order`) |
| 1053 | circuit list preserves option validation errors | internal: `flow_circuit_list/1` has no wire command |
| 1061 | approval rejects oversized secondary catalog dimensions before storage | ported (`audit::governance_bounds::oversized_governance_keys_are_refused_before_anything_is_stored`) |
| 1088 | exact governance lists reject oversized derived keys | fixed (`audit::governance_bounds::oversized_governance_keys_are_refused_before_anything_is_stored`; G53) |
| 1101 | budget rejects oversized reservation identifiers before catalog publication | ported (`audit::governance_bounds::a_reservation_id_retries_but_is_never_reused`) |
| 1118 | approval metadata is bounded before record publication | ported (`audit::governance_bounds::approval_text_and_stored_integers_stay_bounded`) |
| 1155 | cache session heads reject oversized durable identities | internal: Elixir's cache session heads |
| 1178 | effect transitions reject timestamps older than the durable effect | covered (`flow_effect::refused_options_and_older_times_leave_the_effect_unchanged`) |

## `flow_governance_limit_cleaner_correctness_test.exs`

| Line | Test | Status |
|---|---|---|
| 22 | a pending scope consumes a bounded page burst before traversal advances | internal: Elixir's limit cleaner |
| 45 | an empty catalog does not issue an absent cursor delete | internal: Elixir's limit cleaner |
| 61 | a non-canonical progress cursor is reset within the tick write budget | internal: Elixir's limit cleaner |

## `flow_governance_limit_correctness_test.exs`

| Line | Test | Status |
|---|---|---|
| 20 | terminal completion releases the reservation captured by claim | ported (`audit::governance_limits::every_terminal_move_releases_its_credit_once`; `flow_governed_claim::credits_govern_claims_and_terminal_moves_free_them`) |
| 41 | terminal caller cannot redirect release to another governance scope | ported (`audit::governance_limits::a_terminal_caller_cannot_redirect_its_release`) |
| 77 | duplicate terminal completion releases a reservation exactly once | ported (`audit::governance_limits::every_terminal_move_releases_its_credit_once`) |
| 101 | retry, fail, and cancel release their captured reservation | ported (`audit::governance_limits::every_terminal_move_releases_its_credit_once`) |
| 117 | Flow lease extension renews the bound governance credit lease | fixed (`audit::governance_limits::extending_a_flow_s_lease_renews_its_credit_lease`; G54) |
| 157 | limit cache table is owned by its supervised process | internal: Elixir's limit cache |
| 186 | limit cache rejects every untagged entry shape instead of granting credits | internal: Elixir's limit cache |
| 212 | strict global running policy enforces claims without caller limit options | ported with the rule on the claimed state (`audit::governance_limits::running_policies_govern_claims_and_the_strictest_one_wins`) |
| 252 | approximate global running policy bounds prefetched claims and recycles completion | ported with the rule on the claimed state (`audit::governance_limits::running_policies_govern_claims_and_the_strictest_one_wins`); Elixir's prefetch cache is its own |
| 309 | multi-state claims enforce the strictest resolved running policy | ported for its state rules (`audit::governance_limits::running_policies_govern_claims_and_the_strictest_one_wins`); its type-level rule is Elixir's in-process API |
| 361 | batched and independent completion release only committed reservations | ported (`audit::governance_limits::every_terminal_move_releases_its_credit_once`) |
| 398 | bound governance reservation is not exposed in public Flow records | ported (`audit::governance_limits::a_terminal_caller_cannot_redirect_its_release`) |
| 409 | durable reservation ids make replayed release unable to free another flow | covered (`audit::governance_limits::a_public_release_names_its_exact_reservations`); its internal release call is Elixir's |
| 433 | public limit release requires exact reservation ids | ported (`audit::governance_limits::a_public_release_names_its_exact_reservations`) |
| 468 | record codec rejects governance metadata without a reservation id | internal: Elixir's record codec |
| 487 | existing limit owner applies only monotonic capacity configurations | covered (`flow_limit::configuration_versions_guard_limit_changes`) |
| 596 | running policy generations reconfigure an existing limit owner | internal: type-level governance is Elixir's in-process API (see Recorded decisions) |
| 680 | large valid policy versions are fingerprinted across governed claims | internal: type-level governance is Elixir's in-process API (see Recorded decisions) |
| 720 | reconciler releases a terminal reservation after post-commit release is skipped | internal: Elixir's release reconciler; the node releases a Flow's credit once the round that ends its run is durable |
| 750 | reconciler releases governance credit after automatic max-active timeout | internal: Elixir's release reconciler; a max-active timeout ends the run like any terminal move, so its credit is released in the same round (`supervision::shard::state::credit`) |
| 788 | duplicate cache release cannot mint credits beyond the durable chunk | internal: Elixir's limit cache |
| 811 | cache shutdown flush returns only unused prefetched reservations | internal: Elixir's limit cache |
| 835 | flushdb drains cached reservations before deleting governance owners | internal: Elixir's limit cache |
| 877 | destructive cache drain keeps concurrent spends uncached until deletion finishes | internal: Elixir's limit cache |
| 928 | cache drain owner death releases the flush fence | internal: Elixir's limit cache |
| 961 | cache drain owner death releases entries detached before durable release | internal: Elixir's limit cache |
| 1010 | pending cache-page release runs outside the cache server and survives owner death | internal: Elixir's limit cache |
| 1101 | cache drain quarantines a page when durable release succeeds before discard fails | internal: Elixir's limit cache |
| 1169 | concurrent cache drains serialize by instance | internal: Elixir's limit cache |
| 1210 | nested cache drain rejects reentrant ownership without blocking | internal: Elixir's limit cache |
| 1227 | cache refills after a durable clear resets its session | internal: Elixir's limit cache |
| 1280 | flush never releases a cached reservation taken before atomic detach | internal: Elixir's limit cache |
| 1334 | failed flush release merges detached reservations with a concurrent refill | internal: Elixir's limit cache |
| 1379 | unknown flush release outcome fences detached reservations before reuse | internal: Elixir's limit cache |
| 1418 | clear refuses to erase live cached reservations without a durable flush | internal: Elixir's limit cache |
| 1453 | governed retry still wakes claim waiters after releasing its reservation | ported (`audit::governance_limits::a_governed_retry_still_wakes_a_waiting_claim`) |

## `flow_governance_limit_storage_test.exs`

| Line | Test | Status |
|---|---|---|
| 16 | catalog publication codecs reject non-canonical and out-of-range records | internal: Elixir's limit owner storage, catalog publication and cleaner (the node's limit record codec: `governance::tests::stored_limits_round_trip_and_refuse_damage`) |
| 47 | limit record decoders reject non-canonical external terms | internal: Elixir's limit owner storage, catalog publication and cleaner (the node's limit record codec: `governance::tests::stored_limits_round_trip_and_refuse_damage`) |
| 79 | individual reservation records enforce the reservation id bound | internal: Elixir's limit owner storage, catalog publication and cleaner (the node's limit record codec: `governance::tests::stored_limits_round_trip_and_refuse_damage`) |
| 85 | reservation records require the current explicit status field | internal: Elixir's limit owner storage, catalog publication and cleaner (the node's limit record codec: `governance::tests::stored_limits_round_trip_and_refuse_damage`) |
| 100 | limit owner bytes stay bounded as detached reservations grow | internal: Elixir's limit owner storage, catalog publication and cleaner (the node's limit record codec: `governance::tests::stored_limits_round_trip_and_refuse_damage`) |
| 149 | identical replicated spend and release commands replay idempotently | internal: Elixir's limit owner storage, catalog publication and cleaner (the node's limit record codec: `governance::tests::stored_limits_round_trip_and_refuse_damage`) |
| 214 | released reservation history rotates despite lease renewal | internal: Elixir's limit owner storage, catalog publication and cleaner (the node's limit record codec: `governance::tests::stored_limits_round_trip_and_refuse_damage`) |
| 274 | public spend rejects caller ids and delayed old releases cannot affect a new epoch | internal: Elixir's limit owner storage, catalog publication and cleaner (the node's limit record codec: `governance::tests::stored_limits_round_trip_and_refuse_damage`) |
| 333 | expired reservation cleanup is resumable and bounded to 256 entries | internal: Elixir's limit owner storage, catalog publication and cleaner (the node's limit record codec: `governance::tests::stored_limits_round_trip_and_refuse_damage`) |
| 365 | production cleaner drains expired pages without a hot-path scan | internal: Elixir's limit owner storage, catalog publication and cleaner (the node's limit record codec: `governance::tests::stored_limits_round_trip_and_refuse_damage`) |
| 404 | a corrupt catalog entry does not starve a later expired limit | internal: Elixir's limit owner storage, catalog publication and cleaner (the node's limit record codec: `governance::tests::stored_limits_round_trip_and_refuse_damage`) |
| 445 | cleaner tick enforces its Raft page-command budget | internal: Elixir's limit owner storage, catalog publication and cleaner (the node's limit record codec: `governance::tests::stored_limits_round_trip_and_refuse_damage`) |
| 470 | shard, version, and time bounds keep owner and command terms finite | internal: Elixir's limit owner storage, catalog publication and cleaner (the node's limit record codec: `governance::tests::stored_limits_round_trip_and_refuse_damage`) |
| 525 | all valid shards are bounded and large policy versions are fingerprinted | internal: Elixir's limit owner storage, catalog publication and cleaner (the node's limit record codec: `governance::tests::stored_limits_round_trip_and_refuse_damage`) |
| 558 | internal preallocated spends validate ids and replay exactly | internal: Elixir's limit owner storage, catalog publication and cleaner (the node's limit record codec: `governance::tests::stored_limits_round_trip_and_refuse_damage`) |
| 603 | reservation history cap rejects before detached writes | internal: Elixir's limit owner storage, catalog publication and cleaner (the node's limit record codec: `governance::tests::stored_limits_round_trip_and_refuse_damage`) |
| 636 | unknown public spend outcomes preserve exact ids for compensation | internal: Elixir's limit owner storage, catalog publication and cleaner (the node's limit record codec: `governance::tests::stored_limits_round_trip_and_refuse_damage`) |
| 662 | binary policy changes require an explicit monotonic config version | internal: Elixir's limit owner storage, catalog publication and cleaner (the node's limit record codec: `governance::tests::stored_limits_round_trip_and_refuse_damage`) |
| 688 | current owner records reject embedded reservations as corruption | internal: Elixir's limit owner storage, catalog publication and cleaner (the node's limit record codec: `governance::tests::stored_limits_round_trip_and_refuse_damage`) |
| 717 | an uncataloged current owner is repaired before it creates detached storage | internal: Elixir's limit owner storage, catalog publication and cleaner (the node's limit record codec: `governance::tests::stored_limits_round_trip_and_refuse_damage`) |
| 761 | existing owner reads renewals and releases repair catalog membership | internal: Elixir's limit owner storage, catalog publication and cleaner (the node's limit record codec: `governance::tests::stored_limits_round_trip_and_refuse_damage`) |
| 811 | missing spends do not register ghosts and the cleaner prunes stale members | internal: Elixir's limit owner storage, catalog publication and cleaner (the node's limit record codec: `governance::tests::stored_limits_round_trip_and_refuse_damage`) |
| 833 | new owners commit before catalog publication so pruning cannot race lease creation | internal: Elixir's limit owner storage, catalog publication and cleaner (the node's limit record codec: `governance::tests::stored_limits_round_trip_and_refuse_damage`) |
| 870 | durable owner publication repairs a catalog member after caller-side loss | internal: Elixir's limit owner storage, catalog publication and cleaner (the node's limit record codec: `governance::tests::stored_limits_round_trip_and_refuse_damage`) |
| 925 | catalog publication batches owner reads and the idempotent catalog write | internal: Elixir's limit owner storage, catalog publication and cleaner (the node's limit record codec: `governance::tests::stored_limits_round_trip_and_refuse_damage`) |
| 996 | catalog publication discards a torn intent whose owner never committed | internal: Elixir's limit owner storage, catalog publication and cleaner (the node's limit record codec: `governance::tests::stored_limits_round_trip_and_refuse_damage`) |
| 1021 | catalog publication acknowledges only the valid prefix before a corrupt owner | internal: Elixir's limit owner storage, catalog publication and cleaner (the node's limit record codec: `governance::tests::stored_limits_round_trip_and_refuse_damage`) |
| 1086 | catalog publication backlog is strictly bounded | internal: Elixir's limit owner storage, catalog publication and cleaner (the node's limit record codec: `governance::tests::stored_limits_round_trip_and_refuse_damage`) |
| 1096 | removed metadata binding and reservation scan APIs stay unavailable | internal: Elixir's limit owner storage, catalog publication and cleaner (the node's limit record codec: `governance::tests::stored_limits_round_trip_and_refuse_damage`) |

## `flow_governance_models_test.exs`

| Line | Test | Status |
|---|---|---|
| 11 | policy normalization keeps limits, budgets, circuits, and approvals | covered for state governance (`flow_policy_governance::state_governance_is_normalized_merged_and_kept`); Elixir's policy model on hand-built maps |
| 57 | policy normalization rejects unenforced limit modes without crashing | covered (`flow_policy_governance::state_governance_is_normalized_merged_and_kept`, its refusals) |
| 68 | policy normalization returns errors for every malformed enum value | covered (`flow_policy_governance::state_governance_is_normalized_merged_and_kept`, its refusals) |
| 79 | policy normalization rejects circuit windows larger than tracked samples | covered (`flow_circuit::impossible_rules_are_refused_before_anything_is_stored`) |
| 107 | resolved governance policy carries stable version metadata | internal: type-level governance is Elixir's in-process API (see Recorded decisions) |
| 123 | scope resolution prefers explicit governance scope over partition key | covered (`flow_effect::refusals_are_structured_and_each_recorded`: an explicit governance_scope names the scope) |
| 138 | scope resolution falls back to partition key and then auto bucket | covered (`flow_effect::lease::reads_default_to_the_flow_ids_auto_partition`) |
| 150 | credit owner grants leases, spends credits, and refuses over-limit requests | covered (`flow_limit::leases_spends_and_releases_move_credits`) |
| 170 | credit owner reclaims expired lease credits | covered (`flow_limit::expired_leases_in_use_are_reclaimed`) |
| 182 | credit release by reservation id is idempotent and cannot free a sibling | covered (`audit::governance_limits::a_public_release_names_its_exact_reservations`) |
| 201 | credit release rejects invalid reservation value shapes | covered (`audit::governance_limits::a_public_release_names_its_exact_reservations`) |
| 225 | credit owner applies monotonic config versions and drains capacity on decreases | covered (`flow_limit::configuration_versions_guard_limit_changes`, `flow_limit::spends_and_lower_limits_respect_what_is_leased`) |
| 288 | credit owner marks reclaim when global credits exist on another shard | covered (`flow_limit::leases_spends_and_releases_move_credits`: a refused shard marks a reclaim) |
| 298 | budget fixed window resets and returns structured exhaustion | covered (`flow_budget::a_fixed_window_refuses_past_its_limit_and_resets`) |
| 320 | budget commit settles actual usage and records overage | covered (`flow_budget::reservations_settle_with_what_was_used`) |
| 346 | budget release refunds unused reservation | covered (`flow_budget::reservations_settle_with_what_was_used`) |
| 361 | circuit opens after threshold and half-opens after cooldown | internal: type-level governance is Elixir's in-process API (see Recorded decisions): effect-driven circuits |
| 376 | default circuit success is no-op until a prior failure needs reset | internal: type-level governance is Elixir's in-process API (see Recorded decisions): effect-driven circuits |
| 394 | failure-rate circuit still records closed successes for exact rate math | internal: type-level governance is Elixir's in-process API (see Recorded decisions): effect-driven circuits |
| 409 | circuit supports sliding failure rate latency filters and slow-start probes | internal: type-level governance is Elixir's in-process API (see Recorded decisions): effect-driven circuits |
| 453 | circuit error filters ignore missing classes and sequential half-open probes can close | internal: type-level governance is Elixir's in-process API (see Recorded decisions): effect-driven circuits |
| 481 | approval request accepts exactly one terminal decision | covered (`flow_approval::a_request_takes_one_terminal_decision_and_survives_restarts`) |

## `flow_governance_release_outbox_test.exs`

| Line | Test | Status |
|---|---|---|
| 29 | release outbox codecs reject non-canonical and invalid records | internal: Elixir's release outbox and reconciler; the node releases credits once the round that frees them is durable |
| 61 | governed claims do not rewrite the limit owner to bind Flow metadata | internal: Elixir's release outbox and reconciler; the node releases credits once the round that frees them is durable |
| 81 | terminal Raft apply leaves a durable release intent when API release is skipped | internal: Elixir's release outbox and reconciler; the node releases credits once the round that frees them is durable |
| 150 | release reconciliation bounds reservations and batches Flow reads by partition | internal: Elixir's release outbox and reconciler; the node releases credits once the round that frees them is durable |
| 212 | release reconciliation fails closed when the Flow record is corrupt | internal: Elixir's release outbox and reconciler; the node releases credits once the round that frees them is durable |
| 235 | release reconciliation fails closed when the Flow shard is unavailable | internal: Elixir's release outbox and reconciler; the node releases credits once the round that frees them is durable |
| 311 | replayed duplicate intents release one reservation and acknowledge every replay | internal: Elixir's release outbox and reconciler; the node releases credits once the round that frees them is durable |
| 327 | a retained head rotates reconciliation to a nonempty later shard | internal: Elixir's release outbox and reconciler; the node releases credits once the round that frees them is durable |
| 369 | a corrupt outbox shard is reported without starving a healthy later shard | internal: Elixir's release outbox and reconciler; the node releases credits once the round that frees them is durable |
| 410 | release outbox acknowledgements are bounded to one page | internal: Elixir's release outbox and reconciler; the node releases credits once the round that frees them is durable |
| 423 | release reconciliation uses one bounded cursor format | internal: Elixir's release outbox and reconciler; the node releases credits once the round that frees them is durable |
| 442 | release classification distinguishes missing from malformed read status | internal: Elixir's release outbox and reconciler; the node releases credits once the round that frees them is durable |
| 460 | release outbox page materializes 256 cold entries with one shard batch call | internal: Elixir's release outbox and reconciler; the node releases credits once the round that frees them is durable |
| 509 | shard batch reads reject unbounded and oversized requests before dispatch | internal: Elixir's release outbox and reconciler; the node releases credits once the round that frees them is durable |
| 531 | 512 cold shard reads are deferred as one batch without blocking the shard caller | internal: Elixir's release outbox and reconciler; the node releases credits once the round that frees them is durable |
| 572 | a fail-closed head does not starve entries beyond the first page | internal: Elixir's release outbox and reconciler; the node releases credits once the round that frees them is durable |

## `flow_governance_test.exs`

| Line | Test | Status |
|---|---|---|
| 6 | approval records reject trailing external-term bytes | internal: Elixir's external term format; the node stores its own encoding (`governance::tests::stored_approvals_round_trip_and_refuse_damage`) |
| 25 | budget records reject compressed external terms | internal: Elixir's external term format; the node stores its own encoding |
| 51 | circuit records reject trailing external-term bytes | internal: Elixir's external term format; the node stores its own encoding (`governance::tests::stored_circuits_round_trip_and_refuse_damage`) |
| 70 | approval records reject structurally invalid structs | internal: Elixir's external term format; the node stores its own encoding (`governance::tests::stored_approvals_round_trip_and_refuse_damage`) |
| 87 | budget records reject structurally invalid structs without raising | internal: Elixir's external term format; the node stores its own encoding |
| 104 | circuit records reject structurally invalid structs | internal: Elixir's external term format; the node stores its own encoding (`governance::tests::stored_circuits_round_trip_and_refuse_damage`) |
| 121 | effect records reject trailing external-term bytes | internal: Elixir's external term format; the node stores its own encoding (`effect::tests::stored_records_round_trip_and_refuse_damage`) |
| 145 | effect records reject structurally invalid maps | internal: Elixir's external term format; the node stores its own encoding (`effect::tests::stored_records_round_trip_and_refuse_damage`) |
| 157 | ledger indexes reject non-canonical external terms | internal: Elixir's external term format; the node stores its own encoding (`effect::tests::the_ledger_orders_by_time_and_keeps_an_id_once`) |
| 181 | ledger indexes reject structurally invalid events without raising | internal: Elixir's external term format; the node stores its own encoding (`effect::tests::the_ledger_keeps_the_newest_events_within_its_byte_budget`) |
| 195 | flow policy stores governance rules and exposes them | covered for state governance (`flow_policy_governance::state_governance_is_normalized_merged_and_kept`); its type-level rules are type-level governance is Elixir's in-process API (see Recorded decisions) |
| 220 | flow policy version is returned and carried by governance decisions | internal: type-level governance is Elixir's in-process API (see Recorded decisions); the policy hash a decision carries is covered (`flow_effect::policy_hash::the_hash_names_the_types_policy_and_follows_its_changes`) |
| 252 | governance ledger defaults to the flow id auto partition | covered (`flow_effect::lease::reads_default_to_the_flow_ids_auto_partition`) |
| 261 | effect get defaults to the flow id auto partition | covered (`flow_effect::lease::reads_default_to_the_flow_ids_auto_partition`) |
| 297 | effect reserve is idempotent for same digest and conflicts for different digest | covered (`flow_effect::reservations_replay_for_their_digest_and_conflict_for_another`) |
| 344 | effect confirm updates the durable effect record | covered (`flow_effect::terminal_moves_are_recorded_once_and_listed_by_window`) |
| 393 | policy denial returns structured governance error | internal: type-level governance is Elixir's in-process API (see Recorded decisions); the structured denial and its ledger event are covered (`flow_effect::refusals_are_structured_and_each_recorded`) |
| 427 | approval policy returns structured approval-required denial | internal: type-level governance is Elixir's in-process API (see Recorded decisions) |
| 457 | approval requests are durable and accept one terminal decision | covered (`flow_approval::a_request_takes_one_terminal_decision_and_survives_restarts`) |
| 499 | approval request validates optional metadata | covered (`flow_approval::options_are_checked_before_anything_changes`) |
| 551 | approval decisions validate optional persisted metadata before mutation | covered (`flow_approval::options_are_checked_before_anything_changes`) |
| 571 | approval list filters pending requests and governance overview summarizes records | covered (`flow_approval::listings_filter_and_order_across_shards`, `flow_overview::the_overview_summarizes_every_kind_across_shards`) |
| 622 | governance overview and list APIs honor partition scope filters | covered (`flow_overview::partition_scopes_filter_the_overview`) |
| 688 | open circuit blocks governed effect reserve until closed | internal: type-level governance is Elixir's in-process API (see Recorded decisions): an effect consults a circuit only when its policy names one |
| 733 | circuit list filters by scope and status for dashboard use | internal: `flow_circuit_list/1` has no wire command; the overview lists circuits (`flow_overview::the_overview_summarizes_every_kind_across_shards`) |
| 755 | effect failures open governed circuit and later success resets it | internal: type-level governance is Elixir's in-process API (see Recorded decisions) |
| 900 | default circuit policy does not write circuit state on clean successful effect | internal: type-level governance is Elixir's in-process API (see Recorded decisions) |
| 940 | latency circuit does not write circuit state for fast clean success | internal: type-level governance is Elixir's in-process API (see Recorded decisions) |
| 982 | latency circuit opens when the first recorded effect is a slow success | internal: type-level governance is Elixir's in-process API (see Recorded decisions) |
| 1027 | failure-rate circuit records first clean successes for exact rate math | internal: type-level governance is Elixir's in-process API (see Recorded decisions) |
| 1074 | manual circuit open rejects impossible tracking windows | covered (`flow_circuit::impossible_rules_are_refused_before_anything_is_stored`) |
| 1088 | advanced circuit policy opens on latency and filtered failure rate | internal: type-level governance is Elixir's in-process API (see Recorded decisions) |
| 1210 | half-open circuit allows configured probe budget before closing | internal: type-level governance is Elixir's in-process API (see Recorded decisions) |
| 1296 | budget reserve enforces fixed window and resets after window | covered (`flow_budget::a_fixed_window_refuses_past_its_limit_and_resets`) |
| 1322 | budget commit and release settle reservations with actual usage | covered (`flow_budget::reservations_settle_with_what_was_used`) |
| 1364 | governance telemetry emits stable budget and approval metadata | internal: Elixir's telemetry events |
| 1496 | budget supports repeated same-scope reserve and commit operations | covered (`flow_budget::many_settle_in_one_scope_keeping_the_newest`) |
| 1535 | limit leases grant, spend, release, and reclaim durable credits | covered (`flow_limit::leases_spends_and_releases_move_credits`) |
| 1593 | expired in-use limit leases are reclaimed to avoid pinned capacity | covered (`flow_limit::expired_leases_in_use_are_reclaimed`) |
| 1613 | claim_due can spend leased running-limit credits and terminal release frees them | covered (`flow_governed_claim::credits_govern_claims_and_terminal_moves_free_them`) |
| 1674 | governed claim uses pre-spent local limit credits across batches | covered (`flow_governed_claim::claims_take_credits_spent_ahead_and_releases_return_each`) |
| 1721 | terminal release returns its named pre-spent credit durably | covered (`flow_governed_claim::claims_take_credits_spent_ahead_and_releases_return_each`) |
| 1783 | empty governed claim skips limit spend when no due work exists | covered (`flow_governed_claim::an_empty_claim_spends_nothing`) |
| 1841 | effect reserve requires the current flow lease and fencing token | covered (`flow_effect::lease::the_current_lease_and_fence_are_required`) |
| 1856 | ledger list treats its exact durable index as the source of truth | internal: deleting the ledger index key in place |
| 1882 | terminal retention removes effect and ledger governance records | covered (`flow_effect::lease::retention_removes_effects_and_ledger_with_the_flow`) |
| 1942 | terminal retention serializes with racing effect and ledger owner writes | internal: Elixir's retention plan hook; the node's shard applies retention and owner writes one at a time |
