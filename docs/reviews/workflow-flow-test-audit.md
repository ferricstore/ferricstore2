# Flow business-test audit (ADR-014 steps 1 to 10)

After step 15 the user asked that steps 1 to 10 be checked against the
Elixir unit tests the same way (port their business cases). This map lists
every test in the 58 Elixir files picked as business tests for those steps
(Elixir `main` 6a5f2bc34, `apps/ferricstore/test/ferricstore`, 669 tests),
with its status. Files that test only storage, projection, hibernation,
Raft or LMDB internals are not listed.

Statuses:

- **ported**: a Rust test in `crates/wf-node/tests/queue_node/audit/`
  states the same scenario and assertions; it passed on the current code.
- **fixed**: ported, failed, and the node was changed to pass it (the
  failing run is recorded in the coordination review).
- **covered**: an earlier Rust test already states it (named).
- **internal**: not ported; it checks an Elixir mechanism the node does not
  have (reason given).
- **later step**: belongs to steps 11 to 15, ported there.
- **gap**: the node lacks the feature; the ported test waits outside the
  repository (`~/repos/ferricstore2-parity-rig/pending-tests/`) until the
  change that closes the gap, named below, lands.
- **open**: not yet audited.

Since 21 Nov the user wants Elixir's features, not its quirks ("i don't
care about legacy"): a port keeps what a client can rely on, not Elixir's
wording or internal steps, and an Elixir behaviour that looks unintended is
recorded instead of copied.

Gaps found so far:

- G2 (closed): a typed `FLOW.CREATE_MANY` took only the queue fast path
  (no all-or-nothing batches, item options, named values or `idempotent`).
- G3 (closed): `_MANY` item maps kept only the id, partition, lease token
  and fence.
- G4 (closed): `_MANY` list items were read in one order; Elixir reads
  `TRANSITION_MANY` as `[id, fence, token]` and `CANCEL_MANY` as
  `[id, fence]` or `[id, partition, fence]`.
- G5 (closed by the value codec): structured values (maps, lists) were
  taken only on create; transition, complete, fail, retry, cancel and value
  put took bytes only.
- G6 (closed by ADR-016 phase C): replacing a value dropped its old
  version, so a rewind or history could not bring it back.
- G7 (closed by ADR-016 phase C): `FLOW.HISTORY` ignored `values: true`.
- G8: retention cleanup reports `more?` until an extra empty pass.
- G9: `value_refs` (attach an existing value by reference) is not read.

## `flow_write_contract_test.exs`

| Line | Test | Status |
|---|---|---|
| 18 | flow create has a no-values fast path for named value refs | open |
| 25 | flow create rejects metadata that cannot fit its QueryRow before routing | open |
| 42 | flow transition rejects an unencodable QueryRow atomically | open |
| 83 | create admission reserves internal and policy projection bytes | open |
| 102 | flow named value refs do not scan whole flow records on the no-value hot path | open |
| 111 | flow no-value transition fast path avoids value_ref map lookup for normal records | open |
| 124 | router flow many batches use fixed shard buckets | open |
| 131 | router flow pipeline results use ordered tuples instead of index maps | open |
| 148 | flow create fast apply inserts due lifecycle indexes once | open |
| 155 | flow claim partition-key aggregation is not quadratic | open |
| 162 | flow pipeline write result assembly is tuple based | open |
| 172 | flow pipeline transitions avoid oversized transition batch apply | open |
| 201 | flow claim_due adjacent pipeline prepends run results without list concatenation | open |
| 213 | flow claim_due state normalization avoids generic Enum.uniq hot path | open |
| 228 | Flow LMDB async enqueue guard avoids Process.info on apply hot path | open |
| 249 | flow soak latency metrics are sampled without extra hot event counters | open |
| 262 | flow query aggregators accumulate chunks without repeated list concatenation | open |
| 271 | flow history projection avoids grouping when pending entries stay on the apply shard | open |
| 280 | flow LMDB mirror enqueue stays async on the state-machine hot path | open |
| 292 | flow history projection never sync-writes from state-machine apply | open |
| 305 | flow history projection entries stay lazy on the state-machine hot path | open |
| 332 | flow history projector pressure path stores overflow instead of sync-failing apply | open |
| 353 | flow claim_due native planner owns history-ready planning without LMDB sync | open |
| 369 | flow history hot path skips after-history pass when records need no trim or terminal mirror | open |
| 377 | flow transition due-index moves cache repeated batch keys | open |
| 394 | flow claim_due native multi-key path does not sort due keys before the NIF scan | open |
| 408 | flow apply key-size validation does not call Router on the hot path | open |
| 422 | flow small values skip blob externalization dispatch on apply hot path | open |
| 440 | flow many router stamps per-shard batches so apply does not rehash each item | open |
| 473 | flow many apply uses the command-level shard stamp before per-record rehash fallback | open |
| 500 | flow claim_due validates due-index key size without building every due key | open |
| 515 | flow claim_due AUTO skips empty shards before Raft writes | open |
| 540 | flow claim_due AUTO encodes invariant due-key components once per precheck | open |
| 562 | flow claim_due reclaim precheck does not scan cold due rows for running leases | open |
| 576 | flow claim_due AUTO cold precheck uses broad shard prefix instead of per bucket partition scans | open |
| 590 | flow claim_due AUTO cold precheck does not repeat the same LMDB probe per priority | open |
| 616 | flow claim_due cold precheck avoids repeated directory syscalls for initialized shards | open |
| 631 | flow claim_due fast index path avoids generic per-plan tuple dispatch | open |
| 647 | flow terminal transition skips empty due and metadata index passes | open |
| 656 | flow terminal transition caches repeated lifecycle index keys | open |
| 679 | flow non-idempotent create fast path does not synchronously query LMDB for existence | open |
| 706 | flow registry markers use segment-keydir storage instead of apply projection | open |
| 731 | flow retry many history builds entries and next records in one traversal | open |
| 738 | flow multi-row history queues async projection once per apply batch | open |
| 761 | flow fast create history builds entries and records in one traversal | open |
| 769 | flow secondary indexes are native-only on the replicated apply hot path | open |
| 783 | fifo lane maintenance bypasses unrelated due-catalog bookkeeping | open |
| 801 | flow secondary indexes are not backed by ETS tables at boot or rebuild | open |
| 834 | native Flow index apply_batch does not flatten grouped ops on the hot path | open |
| 847 | native Flow claim planning passes float candidates directly to the NIF | open |
| 861 | flow native claim key construction validates generated keys in one fast path | open |
| 873 | flow native claim hydration reuses precomputed state-key prefix | open |
| 890 | flow history projector computes initial LFU once per published batch | open |
| 902 | flow blob-ref writes compute initial LFU once per value | open |
| 914 | flow state-record blob staging reuses LFU for the ETS row and cold write | open |
| 925 | flow fast create state records use known-new pending originals without ETS lookups | open |
| 932 | singular write commands return success only while claim/get return flow data | open |
| 1113 | batch write commands return success only while claim/get return flow data | open |
| 1201 | independent complete_many returns per-item success and completes valid jobs | open |
| 1220 | independent complete_many can return single ok on full success | open |
| 1239 | independent complete_many ok-on-success preserves per-item failures | open |
| 1274 | flow step_continue atomically advances logical state and keeps a fresh running lease | open |
| 1336 | flow start_and_claim creates the first logical step with a running lease | open |
| 1384 | flow run_steps_many defaults persisted timestamps to the cluster clock | open |
| 1410 | flow run_steps_many runs deterministic step chains in one public command | open |
| 1442 | flow run_steps_many rejects duplicate ids without partial creation | open |
| 1471 | retention apply consumes explicit versioned candidates without lagged projection reads | open |
| 1526 | retention planning is bounded and executes shard-local commands | open |
| 1560 | retention uses durable shared ref counts without global reference scans | open |
| 1593 | replicated flow apply does not invoke node-local application callbacks | open |
| 1600 | cold Flow state reads use QueryRows and authoritative hydration only | open |
| 1616 | retention cleanup ownership planning uses the bounded durable member index | open |
| 1633 | retention apply requires the durable flow identity guard | open |
| 1663 | Flow mutations acquire shared refs in their owning shard apply | open |
| 1677 | startup refreshes active file size after retention metadata backfill writes | open |
| 1733 | state_meta policy reindex enqueues durable bounded migration work | open |
| 1757 | Flow policy updates allocate once and install through each Raft group | open |

## `flow_attributes_test.exs`

| Line | Test | Status |
|---|---|---|
| 8 | create stores attributes in durable Flow state and codec round trips them | open |
| 41 | transition merges and deletes attributes without touching payload refs | open |
| 83 | list and stats filter by projected attributes | open |
| 132 | claim_due compact job return can include attributes | open |
| 159 | claim_due records include attributes by default and compact mode omits them | open |
| 209 | attributes persist through transition complete fail retry and rewind | open |
| 322 | terminal records keep attributes queryable through async projection | open |
| 382 | restart rebuild preserves attribute search correctness | open |
| 465 | retention cleanup removes terminal attribute indexes | open |
| 587 | rejects oversized attributes at command boundary | open |
| 600 | rejects an attribute merge that would overflow the existing bounded record | open |
| 640 | explicit null attributes round trip without matching missing attributes | open |
| 681 | rejects oversized attribute keys values and invalid types | open |
| 710 | rejects attribute numbers that cannot be represented by query and native codecs | open |
| 742 | flow policy limits indexed attributes to three names | open |
| 761 | indexed attributes support broad cross-state and cross-type search | open |
| 847 | non-indexed attributes remain exact-queryable but broad search fails clearly | open |
| 894 | broad search retains indexed policy knowledge when no rows exist | open |
| 930 | broad search repairs missing and corrupt indexed-attribute refcounts | open |
| 1015 | indexed-attribute repair is fenced by the membership revision | open |
| 1057 | indexed attribute policy changes are future-only for broad indexes | open |
| 1109 | indexed attributes added during transition become broad-searchable | open |
| 1150 | indexed attributes support cross-type state search without partition-scope under-return | open |
| 1193 | indexed attribute search honors time reverse and count windows | open |
| 1259 | attribute discovery lists policy-indexed keys and top values | open |
| 1304 | attribute discovery preserves string values containing NUL bytes | open |
| 1330 | attribute discovery marks bounded counts as approximate when scan limit is reached | open |
| 1374 | multi-attribute search plans from selective candidate before applying count | open |
| 1416 | retention cleanup removes attribute discovery entries | open |
| 1470 | auto partition attribute discovery excludes expired terminal projections | open |

## `flow_test/sections/flow_history_hot_max_rejects_values_above_configured_maximum.exs`

| Line | Test | Status |
|---|---|---|
| 8 | flow history hot max rejects values above configured maximum | open |
| 27 | flow create_many and transition_many reject oversized batches | open |
| 55 | flow claim_due rejects oversized limit | open |
| 80 | flow_rewind rejects trimmed target event with stale stream index | open |
| 123 | flow_rewind restores a previous history state and reindexes atomically | open |
| 185 | flow_rewind persists the supplied reason and its history reference | open |
| 249 | flow_rewind reads back a large binary reason | open |
| 268 | flow_rewind validates target, expected state, and active leases | open |
| 303 | flow_rewind refuses an event recorded while the Flow was claimed | open |
| 333 | flow_rewind rejects parent and child flows | open |
| 386 | terminal retention from create expires flow state record | open |
| 418 | terminal retention uses wall-valid command time | open |
| 448 | retention cleanup revalidates stale LMDB terminal candidate after rewind | open |
| 504 | retention cleanup skips while async history projection is still pending | open |
| 579 | retention cleanup preserves shared value refs used by another flow | open |
| 650 | retention cleanup does not global-scan refs for private payload values | open |
| 708 | retention cleanup reclaims unreferenced owner named shared values without scanning | open |
| 775 | retention cleanup preserves owner named values referenced by a live flow | open |
| 899 | retention cleanup preserves an owner shared value used as a payload ref | open |
| 968 | retention tracks named payload result and error shared refs | open |
| 1095 | bounded partial and full cleanup preserve a shared value with a live consumer | open |
| 1177 | retention cleanup rejects a racing cross-shard shared value consumer | open |
| 1290 | same-shard child shared ref acquisition ordered before cleanup preserves the value | open |
| 1413 | retention cleanup preserves owner named values referenced by retained history | open |
| 1521 | retention cleanup does not delete generated values for colon-prefixed flow ids | open |

## `flow_test/sections/flow_policy_exposes_type_state_retry_retention_defaults.exs`

| Line | Test | Status |
|---|---|---|
| 8 | flow policy exposes type and state retry and retention defaults | open |
| 57 | flow policy writes use one monotonic internal generation on every shard | open |
| 137 | flow policy rejects hot history cap because it is internal | open |
| 146 | flow policy rejects state-level max_active_ms because it is type-level only | open |
| 157 | flow policy is mirrored to LMDB asynchronously | open |
| 193 | standalone restart rebuilds stored policy and retry uses it | open |
| 277 | native claim batch state write survives standalone restart | open |
| 333 | standalone restart preserves spawned child group state | open |
| 396 | stored state retry policy drives retry exhaustion without command override | open |
| 429 | command retry policy overrides stored state policy | open |
| 466 | flow retry policy accepts thirty day backoff cap | open |
| 483 | flow retry policy computes standard backoff timing | open |
| 533 | flow retry policy keeps generated backoff values bounded and deterministic | open |
| 583 | flow retry policy rejects old max_attempts name | open |
| 592 | flow retry policy decoder rejects old stored max_attempts key | open |
| 618 | flow_retry_many atomically reschedules one-partition batch | open |
| 671 | flow_retry_many rolls back when any item fails guard | open |
| 715 | flow_retry_many rejects invalid retry policy before mutating records | open |
| 757 | expired running lease can be reclaimed | open |
| 798 | flow_reclaim exposes expired running lease reclaim | open |
| 833 | flow_extend_lease extends running lease with fencing guards | open |
| 877 | flow_extend_lease can return OK without materializing the updated record | open |
| 904 | expired lease reclaim fences old worker and records takeover history | open |
| 955 | flow_extend_lease rejects terminal flow | open |

## `flow_test/sections/flow_internal_keys_use_compact_partition_tags.exs`

| Line | Test | Status |
|---|---|---|
| 8 | flow internal keys use compact partition tags | open |
| 30 | flow state record encoding is compact | open |
| 100 | flow state record encoding preserves zeroes nils and empty binaries | open |
| 137 | flow history encoding is compact and includes metadata | open |
| 242 | flow_create stores state and prevents duplicate ids | open |
| 269 | flow_create and flow_transition accept payload_ref for shared payloads | open |
| 298 | managed shared value refs must share the Flow hash slot | open |
| 327 | flow_transition without value options preserves existing named value refs | open |
| 356 | flow_get hydrates payload refs only when full or payload is requested | open |
| 387 | flow_claim_due returns payload inline up to cap without rolling back missing payloads | open |
| 437 | pipeline_read_batch preserves order across get, missing, and history reads | open |
| 480 | pipeline_read_batch accepts Rust Flow AST reads directly | open |
| 504 | pipeline_read_batch rejects removed Flow terminal query ASTs | open |
| 518 | pipeline_write_batch_independent works in raft mode | open |
| 552 | pipeline_write_batch_independent accepts Rust Flow AST writes directly | open |
| 578 | pipeline_write_batch_independent terminal commands preserve per-command success | open |
| 643 | pipeline_write_batch_independent terminal commands skip cross-terminal pre-read | open |
| 690 | pipeline_write_batch_independent terminal commands preserve duplicate flow order | open |
| 736 | pipeline_write_batch_independent transitions preserve duplicate flow order | open |
| 772 | pipeline_claim_due_batch coalesces compatible claims and preserves worker boundaries | open |
| 810 | pipeline_claim_due_batch coalesces job-only claim responses | open |
| 868 | claim_due supports compact job responses | open |
| 894 | claim_due compact state response includes the claimed run state | open |
| 921 | claim_due multi-state precheck does not miss work after an empty route state | open |

## `flow_test/sections/flow_spawn_children_rejects_missing_wait_state_waiting_children.exs`

| Line | Test | Status |
|---|---|---|
| 8 | flow_spawn_children rejects missing wait_state when waiting for children | open |
| 41 | flow_spawn_children rejects empty wait_state for running parent | open |
| 71 | stale child terminal command does not double count parent child group | open |
| 128 | nested child completion resolves parent and propagates to grandparent | open |
| 198 | flow_spawn_children can fail parent on child failure or ignore terminal failures | open |
| 282 | flow_cancel parent cancels direct running children when configured | open |
| 329 | flow_spawn_children is idempotent by group id and rejects conflicts | open |
| 371 | flow_spawn_children idempotency includes child max_active_ms | open |
| 413 | flow_spawn_children remains idempotent after child progress | open |
| 465 | flow_spawn_children rejects new groups on terminal parents | open |
| 501 | child group resolution cancels other open child groups when parent closes | open |
| 566 | flow_spawn_children rejects child partition overrides across Raft groups | open |
| 599 | colocated child completion survives Ra shard restart without duplicate parent summary | open |
| 657 | terminal parent cancellation cancels colocated children | open |
| 700 | flow_complete_many resolves colocated child groups | open |
| 762 | Raft routing rejects child dependencies across independent groups | open |
| 804 | colocated nested child completion resolves parent and grandparent | open |
| 880 | resolved nonterminal child join schedules the parent for a due claim | open |
| 975 | wait any keeps a parent blocked until an unresolved sibling closes | open |
| 1085 | resolved nonterminal child join is included in a cross-shard due scan | open |
| 1178 | resolved failed child join schedules a nonterminal failure successor | open |
| 1188 | resolved successful child join leaves terminal success out of the due index | open |
| 1192 | resolved failed child join leaves terminal failure out of the due index | open |

## `flow_test/sections/flow_max_active_ms_times_out_flows.exs`

| Line | Test | Status |
|---|---|---|
| 6 | max_active_ms defaults to infinite and does not affect active flows when unset | open |
| 28 | max_active_ms moves overdue non-terminal flows to failed and starts retention | open |
| 87 | timeout cleanup also removes terminal flows already expired before the pass | open |
| 144 | timeout cleanup applies one active candidate limit across shards | open |
| 181 | timeout and terminal cleanup share one limit across shards | open |
| 262 | a stale active candidate does not consume the terminal cleanup limit | open |
| 351 | retention bounds shard-local cleanup commands globally | open |
| 441 | terminal cleanup requires trusted backfill verification and the exact watermark | open |
| 526 | create max_active_ms overrides the type policy for one flow | open |
| 551 | create max_active_ms infinity opts one flow out of the type policy timeout | open |
| 576 | claim_due fails a flow whose max active deadline has elapsed | open |
| 603 | claim_due keeps a batch of unexpired max active candidates eligible | open |
| 637 | claim_due skips an expired candidate and claims the next eligible flow | open |
| 676 | spawn rejects max-active child dependencies across independent groups | open |
| 724 | rejected cross-shard timeout dependencies do not starve an eligible claim | open |
| 833 | claim_due applies an expired child to a same-shard parent in another partition | open |
| 898 | complete fails a running flow whose max active deadline has elapsed | open |
| 933 | complete enforces max_active_ms for a colocated child and parent | open |
| 1004 | timeout cleanup reloads lineage records changed earlier in the same batch | open |
| 1062 | timeout cleanup enforces max_active_ms for hibernated flows | open |
| 1133 | timeout cleanup fails closed when a hibernated flow reverse index is missing | open |
| 1182 | timeout cleanup propagates a child failure to a colocated parent | open |
| 1244 | max active deadlines are projected into a bounded hot index | open |

## `flow_codec_test.exs`

| Line | Test | Status |
|---|---|---|
| 11 | record codec round trips small and nil fields | open |
| 25 | record codec round trips fields that require multibyte length headers | open |
| 40 | batch record decoding preserves order and matches single-record decoding | open |
| 56 | batch record decoding rejects malformed input atomically and enforces its page bound | open |
| 73 | record codec round trips private governance reservation identity | open |
| 91 | record codec round trips sealed system metadata without exposing it | open |
| 109 | record codec round trips private schedule metadata without exposing it | open |
| 130 | private schedule metadata preserves opaque target type bytes | open |
| 146 | empty system metadata keeps dedicated record bytes unchanged | open |
| 153 | record codec rejects governance metadata outside the current exact shape | open |
| 185 | record decoders reject semantically invalid required fields | open |
| 210 | record decoders reject malformed reserved sidecar fields | open |
| 245 | record encoders reject invalid query metadata instead of dropping it | open |
| 259 | record codec omits redundant immutable defaults from every state version | open |
| 291 | record meta codec matches full record projection | open |
| 336 | terminal after noop batch NIF detects records without terminal side effects | open |
| 357 | history codec keeps terminal metadata and refs | open |
| 380 | history codec stores per-event data without repeated immutable flow metadata | open |
| 424 | history codec round trips fields that require multibyte length headers | open |
| 442 | history metadata normalization ignores unsupported compound values | open |
| 451 | value codec stores normal binaries raw and escapes magic-prefixed binaries | open |
| 469 | typed values reject non-canonical external terms without deserializing them | open |
| 483 | child group decoding rejects the unsupported external-term format | open |

## `flow/policy_command_test.exs`

| Line | Test | Status |
|---|---|---|
| 6 | ordinary KV commands bypass policy stamping unchanged | open |
| 14 | signal transitions are policy-sensitive commands | open |
| 23 | ordinary KV batches return the original list without consulting context | open |
| 34 | ordered batches stamp later commands against an earlier Flow target | open |
| 77 | an explicit type overrides an earlier target for the same batch key | open |
| 110 | mixed policy batches preserve zero-arity opaque commands | open |
| 132 | malformed policy-sensitive commands fail before policy reads | open |
| 140 | malformed keyed command batches return errors instead of raising | open |
| 148 | existing-flow policy stamping fails closed while the state shard is unavailable | open |
| 170 | existing hot flows are stamped without a shard process round trip | open |
| 223 | single commands fence their compact reference with the exact coordinator policy | open |
| 260 | policy-sensitive commands carry a fixed-size reference and one exact fence | open |
| 301 | existing-flow stamps fence the observed type and incarnation | open |
| 356 | policy and migration codecs expose one canonical beta wire shape | open |
| 388 | outer Flow batches replicate compact references instead of repeated policies | open |
| 434 | obsolete cross-shard envelopes bypass policy traversal unchanged | open |
| 442 | nested batches store each policy reference once | open |
| 489 | policy snapshot size has a deterministic inclusive boundary | open |
| 516 | oversized, compressed, and trailing stored policy envelopes are rejected before decode | open |
| 539 | policy generations stop at the exact migration score boundary | open |
| 557 | multi-type commands enforce one total snapshot bundle limit | open |
| 626 | command tag dispatch does not allocate tag strings | open |
| 634 | hot policy stamping does not call the shard process | open |

## `flow_test/sections/flow_spawn_children_wait_any_resolves_on_first_successful_child_across_s.exs`

| Line | Test | Status |
|---|---|---|
| 8 | flow_spawn_children wait any resolves on first successful child in one Raft group | open |
| 60 | flow_spawn_children wait any resolves failure when every local child fails | open |
| 124 | flow_fail_many fail_parent policy closes parent and cancels local siblings | open |
| 190 | flow_create_many spans shards and rolls back failing shard group | fixed (G2) |
| 234 | flow_create emits telemetry without automatic worker pubsub wakeups | open |
| 267 | flow APIs reject malformed inputs before raft apply | open |
| 404 | flow_claim_due atomically leases due flows and removes them from due set | open |
| 441 | flow_claim_due skips Raft write when native due index proves selected partition is empty | open |
| 487 | flow_claim_due empty precheck does not drop work due near unstamped apply time | open |
| 522 | all-partition claims avoid Raft writes for an empty type without hiding newly due work | open |
| 567 | all-partition empty precheck respects future deadlines and encoded type boundaries | open |
| 602 | all-partition empty precheck falls back when its bounded catalog page is full | open |
| 648 | all-partition empty precheck does not hide hibernated work | open |
| 690 | flow_claim_due skips Raft writes when all selected partition keys are empty | open |
| 720 | flow_claim_due leases large batches without duplicates and drains due members | open |
| 760 | partition_key keeps related flow keys on one shard and can spread partitions | open |
| 780 | flow lifecycle maintains state, inflight, and worker indexes | open |
| 865 | flow_list, flow_info, and flow_stuck read lifecycle indexes | open |
| 916 | flow_info counts pending terminal records before LMDB writer flush | open |
| 967 | flow_info does not write zero LMDB terminal counters for active-only types | open |
| 1004 | flow_info does not build empty terminal score indexes for active-only types | open |

## `flow_test/sections/flow_history_supports_range_reverse_event_worker_filters.exs`

| Line | Test | Status |
|---|---|---|
| 8 | flow_history supports range reverse event and worker filters | open |
| 125 | flow_history filtered cold query finds recent tail events beyond oldest scan window | open |
| 173 | flow_history filtered cold query returns latest matching page | open |
| 224 | flow_history filtered cold query keeps old head window before filtering | open |
| 287 | flow_history filtered cold query finds retained middle events beyond sampled edges | open |
| 337 | flow_terminals and flow_failures list terminal records by state and time range | open |
| 431 | flow_terminals returns completed failed and cancelled records with the same timestamp | open |
| 478 | cold terminal query seeks to filtered time window instead of sampling prefix head | open |
| 538 | flow_history event ids stay monotonic when claim time is behind record time | open |
| 568 | flow_history falls back to bounded history key scan when Flow index misses | open |
| 617 | flow history hot max keeps only latest configured hot events | open |
| 672 | flow history hot-only count returns latest events when hot window is larger than total | open |
| 710 | flow history max events hard-caps stored history records | open |
| 771 | flow history max events defaults to 100k and rejects invalid caps | open |
| 791 | flow history LMDB filtered scans stay bounded to requested history count | open |
| 801 | flow history hard default is clamped by configured maximum | open |
| 818 | flow history configured maximum cannot exceed hard cap | open |
| 828 | flow history uses configured default hot max when omitted | open |
| 859 | flow history default hot max keeps no events hot | open |
| 875 | flow history hot max accepts zero | open |

## `flow_test/sections/flow_create_stores_debug_lineage_metadata_indexes_it.exs`

| Line | Test | Status |
|---|---|---|
| 8 | flow_create stores debug lineage metadata and indexes it | open |
| 64 | flow_create stores default root without indexing one unique root per flow | open |
| 82 | flow_by_parent root and correlation query lineage indexes | open |
| 200 | lineage before_id cursor requires an explicit reverse timestamp boundary | open |
| 226 | lineage before_id cursor paginates equal timestamps without duplicates | open |
| 259 | cold lineage query seeks to filtered time window instead of sampling prefix head | open |
| 334 | reverse cold lineage query samples from the newest side of a filtered window | open |
| 396 | flow_create_many creates one-partition batch atomically | fixed (G2) |
| 428 | flow write APIs return ok and reject return option | open |
| 495 | flow_create_many lets item attrs override common attrs | fixed (G2) |
| 524 | flow_create_many auto-partitions missing partition and rolls back duplicate batches | fixed (G2) |
| 566 | flow_create_many independent keeps successful items when one create fails | fixed (G2) |
| 592 | flow_create_many independent auto-partitions without client-side grouping | open |
| 624 | flow_create_many independent auto-partitioned flows are claimable by partition keys | open |
| 668 | flow_claim_due partition_keys tries later shards when an earlier stale candidate claims nothing | open |
| 755 | flow_create_many idempotent retry returns existing records without duplicate writes | fixed (G2) |
| 794 | flow_create_many idempotency conflict rolls back same shard group | fixed (G2) |
| 824 | flow_spawn_children wait none creates full child flows and advances parent | open |
| 864 | flow_spawn_children wait all advances parent only after direct children finish | open |

## `flow_test/sections/flow_claim_due_automatically_reclaims_expired_leases_ratio.exs`

| Line | Test | Status |
|---|---|---|
| 8 | flow_claim_due automatically reclaims expired leases by ratio | open |
| 49 | flow_claim_due can disable automatic expired lease reclaim | open |
| 79 | flow_claim_due limit one prefers ready queued work before expired lease reclaim | open |
| 119 | expired running lease reclaim is partition scoped | open |
| 199 | flow_transition atomically moves state, due index, and history | open |
| 250 | flow_transition rejects direct moves into running state | open |
| 271 | flow_transition_many atomically moves one-partition batch | open |
| 325 | flow_transition_many rejects direct moves into running state | open |
| 359 | flow_transition_many shared payload keeps per-flow refs and values | open |
| 414 | flow_transition_many shared payload is encoded once in the replicated command | open |
| 453 | independent flow_transition_many shared payload is encoded once per shard | open |
| 494 | flow_transition_many rolls back when any item fails guard | open |
| 545 | flow_transition_many independent keeps successful items when one item fails | open |
| 577 | flow transition pipeline batch preserves duplicate key ordering | open |
| 601 | flow_transition_many spans shards and rolls back failing shard group | open |
| 655 | flow many commands route auto-partition records by their real state key | open |
| 758 | flow_complete_many atomically completes one-partition batch | open |
| 807 | flow_complete_many rolls back when any item fails guard | open |
| 864 | flow_complete_many spans shards and rolls back failing shard group | open |

## `flow_test/sections/claim_due_multi_state_claims_earliest_due_job_instead_first_listed_state.exs`

| Line | Test | Status |
|---|---|---|
| 8 | claim_due multi-state claims earliest due job instead of first listed state | open |
| 46 | pipeline_claim_due_batch groups interleaved independent partitions | open |
| 99 | pipeline_claim_due_batch batches distinct partition claims without overclaiming | open |
| 167 | pipeline_claim_due_batch singleton partition list claims across shards | open |
| 205 | pipeline_claim_due_batch singleton auto partition scans across shards | open |
| 243 | pipeline_claim_due_batch singleton any partition scans across shards | open |
| 280 | pipeline_claim_due_batch accepts omitted NOW option | open |
| 303 | flow_claim_due BLOCK waits without polling and wakes after due create | open |
| 343 | flow_claim_due BLOCK re-registers after a spurious wake loses the claim | open |
| 389 | flow_claim_due BLOCK wakes when due create uses default state | open |
| 428 | flow_claim_due BLOCK uses claim limit as waiter credit capacity | open |
| 482 | flow_claim_due BLOCK times out with an empty claim | open |
| 498 | flow_claim_due BLOCK is not consumed by retry policy backoff wake | open |
| 563 | flow_claim_due BLOCK wakes for retry scheduled before waiter registers | open |
| 623 | pipeline_claim_due_batch preserves sequential reclaim preference | open |
| 680 | pipeline_read_batch hydrates Flow GET payloads with per-command caps | open |
| 718 | flow_create idempotent retry returns matching existing record and rejects conflicts | open |
| 758 | flow_create idempotent retry matches large blob-backed payloads | open |
| 806 | flow due index stays derived and does not persist per-flow zset members | open |

## `flow/workflow_sdk_test.exs`

| Line | Test | Status |
|---|---|---|
| 188 | create derives type, state and partition key from attrs | open |
| 208 | derived partition keys cannot collide through component delimiters | open |
| 226 | create_many groups by shard in core by passing per-item partition keys | open |
| 244 | workflow commands cannot override the declared flow type | open |
| 270 | partition builders reject non-scalar values without raising | open |
| 281 | child builds child specs with workflow defaults | open |
| 297 | fanout wraps flow_spawn_children with parent guard defaults and aliases | open |
| 329 | children and waiting_children query by parent with partition inherited from job | open |
| 350 | claim_due applies state defaults and wraps returned records as jobs | open |
| 367 | ok follows on_ok transition and carries lease/fencing guards | open |
| 386 | ok can complete terminal states | open |
| 404 | error applies state retry policy and exhausted target | open |
| 425 | install_policy writes DSL retry policy into embedded API | open |
| 445 | install_policy writes each declared state execution mode | open |
| 461 | install_policy can explicitly patch an existing policy | open |
| 470 | worker polls claim_due and applies ok result | open |
| 497 | run_once and handle convert handler return values to ok or error commands | open |
| 531 | handle does not convert storage mutation failures into a second command | open |
| 554 | reclaim_once wraps reclaimed records as jobs | open |

## `flow_value_payload_test.exs`

| Line | Test | Status |
|---|---|---|
| 19 | create stores a full payload value and claim/get hydrate it from internal storage | ported |
| 55 | transition can replace payload and retry error preserves current payload | fixed (G5, value codec) |
| 124 | complete and fail store result/error values without requiring public refs | fixed (G5, value codec) |
| 182 | terminal retention does not materialize existing generated payload value refs | internal: Elixir copies no generated ref at terminal time; its read after cleanup contradicts the cleanup tests |
| 223 | value_mget can omit values larger than max_bytes without decoding them | fixed (G5, value codec) |
| 240 | shared value TTL uses command time when now_ms is omitted | ported |
| 247 | shared value expiry stays in the exact integer domain | ported (wording; an expiry past 2^53 - 1 is no limit with u64 times) |
| 260 | value_mget returns hot values and leaves ordinary missing refs as nil | ported |
| 269 | cancel terminal retention does not materialize existing generated payload value refs | internal: as line 182 |
| 300 | cancel rejects public reason_ref input without touching external value | ported (refused as an unknown option) |
| 331 | cancel stores inline reason payload as an owned terminal value | fixed (G5, value codec) |
| 385 | retention cleanup removes expired terminal state, history, and owned values | fixed (G5, value codec) |
| 441 | retention cleanup returns an exact continuation without a final empty pass | gap G8 |
| 523 | retention sweeper runs cleanup through Flow command path | covered: `retention::a_completed_flow_is_removed_after_retention` |
| 610 | rewind from terminal back to active clears value ref expiration | ported |
| 675 | batch APIs also persist full value fields | fixed (G2, G3, G5) |
| 787 | history only hydrates stored values when values option is requested | fixed (G5, value codec) |

## `flow_test/sections/flow_native_due_index_mirrors_create_claim_due.exs`

| Line | Test | Status |
|---|---|---|
| 8 | Flow native due index mirrors create and claim_due | open |
| 117 | FLUSHDB clears native Flow secondary indexes after durable deletes | open |
| 166 | flow_claim_due validates record due time when native due score is stale | open |
| 209 | flow_claim_due keeps metadata-indexed flows claimable through generic fallback | open |
| 243 | flow_claim_due restores native overfetch beyond requested limit | open |
| 282 | partition_key scopes claim, complete, retry, get, and history | open |
| 344 | flow_claim_due only scans the selected partition | open |
| 398 | flow_claim_due can scan selected partition_keys in one call | open |
| 452 | flow_create without partition auto-spreads and default claim_due fetches across buckets | open |
| 510 | flow_claim_due can scan any partition and selected states | open |
| 570 | flow_claim_due omitted state claims any due state | open |
| 595 | flow_claim_due any partition drains higher priority across shards first | open |
| 647 | flow_claim_due any partition rotates start shard per type | open |
| 706 | flow_claim_due any partition spreads small limits across shards under skew | open |
| 756 | flow_claim_due skips stale due index members without starving live work | open |
| 779 | flow_claim_due single-key native scan continues after stale candidate batch | open |
| 820 | flow_claim_due multi-key native scan continues after stale candidate batch | open |

## `flow_user_value_atoms_test.exs`

| Line | Test | Status |
|---|---|---|
| 11 | create rejects payloads with atom keys or atom values | open |
| 33 | create accepts payloads whose only atoms are true, false and nil | open |
| 59 | create_many rejects an item payload with atoms before writing any item | open |
| 77 | complete and fail reject atoms in result and error values | open |
| 115 | value_put rejects values containing atoms | open |
| 123 | signal rejects atoms in named values before recording the signal | open |
| 141 | complete_many rejects atoms in an atom-keyed map item result | open |
| 163 | complete_many rejects atoms in a string-keyed map item result | open |
| 185 | create_many rejects atoms in a tagged tuple item payload | open |
| 213 | transition_many rejects atoms in item values | open |
| 229 | retry_many rejects atoms in an item error | open |
| 252 | fail_many rejects atoms in the batch error | open |
| 274 | cancel rejects atoms in the reason | open |
| 289 | instance cancel rejects atoms in the reason | open |
| 304 | cancel_many rejects atoms in an item reason | open |
| 318 | batch, signal, value and cancel paths accept string keys with true, false and nil | open |

## `flow_test/sections/flow_fail_many_atomically_fails_one_partition_batch.exs`

| Line | Test | Status |
|---|---|---|
| 8 | flow_fail_many atomically fails one-partition batch | open |
| 52 | flow_fail_many rolls back when any item fails guard | open |
| 96 | flow_retry_many spans shards and rolls back failing shard group | open |
| 144 | flow_retry_many terminal exhaustion updates colocated parent child group | open |
| 198 | flow_fail_many spans shards and rolls back failing shard group | open |
| 245 | flow_cancel_many atomically cancels one-partition queued batch | open |
| 279 | flow_cancel_many rolls back when any item fails guard | open |
| 310 | terminal many independent keeps successful items when one item fails | open |
| 468 | terminal many independent does not pre-read records for cross-shard planning | open |
| 518 | terminal local-only mixed complete_many skips cross-shard pre-read | open |
| 573 | flow_cancel_many spans shards and rolls back failing shard group | open |
| 612 | flow_transition enforces expected state and running lease guard | open |
| 661 | flow_transition rejects terminal states so terminal hooks stay centralized | open |
| 708 | flow_transition rolls back index changes when derived keys are invalid | open |
| 746 | flow_fail and flow_cancel write terminal states and remove due work | open |
| 817 | flow_history returns transition events | open |

## `flow_test/sections/flow_claim_due_drains_higher_priorities_before_lower_priorities_default.exs`

| Line | Test | Status |
|---|---|---|
| 8 | flow_claim_due drains higher priorities before lower priorities by default | open |
| 47 | flow_claim_due priority option targets one priority band | open |
| 88 | flow_complete enforces lease token guard and writes terminal state | open |
| 128 | terminal flows reject normal transition and cancel | open |
| 163 | flow_retry clears lease and reschedules flow | open |
| 211 | flow_claim_due promotes hibernated cold due flow | open |
| 485 | blocking claim_due schedules wake from cold due rows | open |
| 555 | hibernation schedules due wake for existing claim waiter | open |
| 588 | flow_retry returns to claimed run_state with computed backoff | open |
| 661 | flow_retry exhausts to configured active state | open |
| 706 | flow_retry terminal exhaustion keeps stable audit metadata | open |
| 748 | flow_retry terminal exhaustion updates colocated parent child group | open |
| 795 | flow_retry rejects invalid retry policy | open |

## `flow_state_meta_test.exs`

| Line | Test | Status |
|---|---|---|
| 36 | state_meta stores durable per-state metadata and survives later states | open |
| 88 | claim preserves separator-bearing type and state index components | open |
| 119 | state_meta version is queryable only when policy indexes that metadata key | open |
| 180 | state metadata value discovery returns bounded typed counts from its index | open |
| 210 | indexed state_meta policy backfills existing flow records | open |
| 273 | changing or removing indexed state_meta policy deletes stale LMDB query rows | open |
| 315 | indexed state_meta can query terminal state metadata | open |
| 362 | retention cleanup deletes indexed state_meta query rows from LMDB | open |
| 438 | state_meta and indexed_state_meta stay bounded | open |
| 455 | rejects a state_meta merge that would overflow an existing state entry | open |
| 495 | explicit null state_meta values survive the durable codec | open |
| 512 | rejects state_meta numbers that cannot be represented by query and native codecs | open |
| 547 | state_meta update rejects growth beyond the entry cap atomically | open |

## `flow/read_api_test.exs`

| Line | Test | Status |
|---|---|---|
| 28 | terminal query timestamps reject values above the exact integer ceiling | open |
| 33 | stuck query timestamps reject values above the exact integer ceiling | open |
| 38 | candidate scan counts cover disjoint RAM and LMDB index entries | open |
| 42 | candidate selection never prefers a failed count over a successful count | open |
| 51 | query callers may lower but cannot raise the bounded candidate scan ceiling | open |
| 56 | attribute discovery fails closed on corrupt query-index values | open |
| 68 | attribute discovery rejects query-index key and value mismatches | open |
| 81 | attribute discovery decodes typed values inside query-index key components | open |
| 95 | attribute discovery retains maximum-size string values outside the LMDB key | open |
| 111 | attribute discovery propagates expired query-index deletion failures | open |
| 124 | an omitted count respects a configured maximum below the built-in default | open |
| 143 | relationship queries scan auto partitions instead of passing atoms into key builders | open |
| 165 | list applies terminal-only filtering before truncating active state indexes | open |

## `flow/flow_input_limits_test.exs`

| Line | Test | Status |
|---|---|---|
| 9 | public create takes a 256-byte id and rejects a longer one without writing it | open |
| 23 | a batch with one long id writes none of its items | open |
| 38 | start and claim rejects a long id | open |
| 45 | spawned children are new Flows and take the same limit | open |
| 58 | a Flow created with a longer id before the limit keeps working | open |
| 92 | a new Flow takes a 256-byte partition key and rejects a longer one | open |
| 119 | children inherit a parent's longer partition key but cannot open a new one | open |
| 149 | a Flow stored under a longer partition key keeps working | open |
| 179 | new Flows take a type up to 128 bytes, a state up to 64 and a correlation id up to 256 | open |
| 211 | a claim or start takes a worker name up to 256 bytes | open |
| 229 | a transition, step or reschedule moves only into a state up to 64 bytes | open |
| 243 | a child group takes a group id up to 128 bytes and states up to 64 | open |
| 268 | a Flow stored with a longer type and state can still be claimed and completed | open |

## `flow_signal_test.exs`

| Line | Test | Status |
|---|---|---|
| 21 | signal attaches named values and history records the signal | ported |
| 62 | signal can atomically attach values and guarded-transition state | ported |
| 109 | signal rejects entry into a fifo state without an explicit partition | fixed |
| 137 | fifo claims follow signal state-entry order instead of creation order | ported |
| 180 | signal transition requires an if_state guard | ported |
| 206 | signal cannot transition a leased running flow without matching running guard | ported |
| 242 | signal cannot transition an already terminal flow | ported |
| 282 | signal if_state guard rejects stale state without value writes | ported |
| 315 | signal idempotency key replays same digest and rejects conflicting digest | ported |
| 357 | signal idempotency key rejects conflicting transition semantics | ported |
| 403 | passive signal preserves active lease and fencing token | ported |
| 452 | retention cleanup removes signal-owned named value blobs | ported |

## `flow_named_values_test.exs`

| Line | Test | Status |
|---|---|---|
| 21 | create stores multiple named values and get hydrates selected names only | ported |
| 45 | named value put is idempotent and override creates a new immutable ref | ported |
| 106 | WARaft blob side-channel named value put returns original large value | ported (a large value reads back whole; no side channel here) |
| 146 | claim_due hydrates selected named values with one logical request | ported |
| 173 | value_mget decodes refs and preserves request order | ported |
| 193 | transition can add and drop named values without hydrating bytes | ported |
| 235 | terminal commands can attach named values | ported |
| 270 | retention cleanup removes owned named value blobs | ported |
| 324 | retention cleanup removes overridden named value versions from history | fixed (G6, ADR-016 phase C) |
| 386 | transition_many stores named value blobs for every transitioned flow | ported |
| 438 | rewind restores named value refs from the target history snapshot | fixed (G6, ADR-016 phase C) |
| 486 | create_many and spawn_children support per-item named values | gap G2, G9 |

## `flow/pipeline_history_read_test.exs`

| Line | Test | Status |
|---|---|---|
| 10 | results returns empty map for no history ops | open |
| 14 | results sends cold or consistent ops through read callback | open |
| 35 | results prepares consistent cold ops once before reading without per-op consistency | open |
| 67 | results returns prepare error for consistent cold ops without reading | open |
| 91 | results batches hot rank requests and hydrates ranked events | open |
| 118 | results fail every hot request closed when the rank batch reply is short | open |
| 141 | results fail a malformed hot rank result closed without raising | open |
| 161 | results batch fetches decode contexts for hot history entries | open |
| 218 | results fails a hot read closed when its batched decode context is missing | open |
| 239 | results preserves an explicit decode context error | open |
| 262 | results skips hot index and fallback scan when record disables hot history | open |
| 290 | results falls back when hot rank index is unavailable or empty | open |

## `flow/fifo_lane_test.exs`

| Line | Test | Status |
|---|---|---|
| 8 | member round-trips an exact 128-bit state-entry sequence and id | open |
| 15 | member byte order preserves sequence order beyond float precision | open |
| 22 | native lane rank uses encoded sequence order beyond float precision | open |
| 38 | decoder rejects truncation and an empty id | open |
| 43 | running records remain in the lane of their logical run state | open |
| 58 | running members sort ahead of queued members while preserving entry order within each class | open |
| 91 | native fifo head lookup resolves all lanes and due scores in one request | open |
| 124 | terminal records do not have active lane entries | open |
| 135 | lane keys remain colocated with their partition | open |

## `flow/claim_due_api_test.exs`

| Line | Test | Status |
|---|---|---|
| 7 | wait_keys supports explicit state | open |
| 12 | wait_keys rejects a state filter whose expanded footprint is unbounded | open |
| 19 | wait_keys rejects a partition filter whose expanded footprint is unbounded | open |
| 26 | wait_keys bounds the combined state and partition footprint | open |
| 42 | wait_keys accounts for encoded type and state bytes before generating internal keys | open |
| 50 | return_records supports compact jobs | open |
| 61 | claim_due rejects inexact lease timestamps before routing | open |
| 93 | claim_due rejects block timeouts larger than the VM timer limit before routing | open |
| 101 | claim_due rejects payload hydration above the configured ceiling before routing | open |

## `flow/mutation_attrs_test.exs`

| Line | Test | Status |
|---|---|---|
| 7 | create_attrs preserves default state, auto partition, and named value options | open |
| 23 | create_attrs accepts infinity while validating compact query metadata | open |
| 33 | schedule discovery metadata is accepted only through the internal schedule path | open |
| 63 | transition_attrs rejects direct transition into running | open |
| 71 | create attrs rejects unleased running state before replicated mutation preparation | open |
| 81 | batch and spawned child creation share the unleased running rejection | open |
| 99 | create_attrs rejects timestamps that cannot be represented exactly by Flow indexes | open |
| 116 | lease mutations reject deadlines outside the exact Flow timestamp range | open |

## `flow_test/sections/flow_list_auto_partitions_merges_hot_ids_before_hydration.exs`

| Line | Test | Status |
|---|---|---|
| 6 | Flow auto partition key list exposes all hidden buckets in order | open |
| 15 | Flow list without partition merges hot auto partitions by update order | open |
| 44 | Flow list applies reverse ordering before count on an explicit partition | open |
| 76 | Flow list without partition preserves sparse auto partition ordering | open |
| 108 | Flow list return meta trims heavy fields through embedded API | open |
| 142 | Flow list without partition fetches more hot records from a skewed auto partition | open |

## `flow/many_item_opts_test.exs`

| Line | Test | Status |
|---|---|---|
| 7 | create parses binary tuple and map item shapes | open |
| 24 | terminal item parsers preserve map values and lease tuple shapes | open |
| 38 | cancel and transition preserve optional lease and partition keys | open |
| 59 | merge applies item opts over base opts and pins partition key | open |
| 67 | invalid item opts keep existing error messages | open |
| 77 | item parsers reject empty ids and required lease tokens at the boundary | open |

## `flow/history_read_test.exs`

| Line | Test | Status |
|---|---|---|
| 26 | filtered reverse hot reads scan the bounded history window | open |
| 44 | history availability is not normalized to an empty result | open |
| 55 | cold history batches require exact results and preserve storage failures | open |
| 84 | history sweep limits stay bounded when mutable configuration is invalid | open |
| 95 | history projector failures do not expose internal reasons | open |
| 105 | history decode context distinguishes missing state from corruption and read failures | open |

## `flow/history_query_test.exs`

| Line | Test | Status |
|---|---|---|
| 20 | filtering is false when query has no filters | open |
| 24 | fetch_count only asks scan count for filtered queries | open |
| 33 | apply filters by event worker timestamp version and event id ranges | open |
| 73 | apply reverses before taking count for reverse queries | open |
| 86 | range validators reject inverted ranges | open |
| 96 | event id ranges compare versions within a millisecond as numbers | open |

## `flow/fencing_retry_test.exs`

| Line | Test | Status |
|---|---|---|
| 33 | complete retry after committed response loss is a no-op success | open |
| 53 | fail retry after committed response loss is a no-op success | open |
| 70 | transition retry after committed response loss is a no-op success | open |
| 96 | retry retry after committed response loss is guarded by latest history event | open |
| 135 | retry retry without explicit run_at is a no-op while scheduled in original state | open |
| 159 | same fencing token does not turn a different terminal effect into success | open |

## `flow_test/sections/flushdb_clears_flow_lmdb_terminal_projections.exs`

| Line | Test | Status |
|---|---|---|
| 8 | FLUSHDB clears Flow LMDB terminal projections | open |
| 134 | retention cleanup deletes owned payload refs from trimmed history | open |
| 214 | terminal retention expires queryable flow history | open |
| 247 | terminal ttl override must be positive | open |
| 272 | flow create inherits retention defaults from policy | open |

## `flow_stats_test.exs`

| Line | Test | Status |
|---|---|---|
| 10 | stats counts active workflows beyond list count cap | open |
| 34 | stats counts indexed attribute candidates beyond list count cap | open |
| 54 | attribute stats ignore expired terminal projection rows | open |
| 77 | attribute stats fail closed when exact scan would exceed limit | open |
| 96 | info and stats stop counting a Flow that retention cleaned up | open |

## `flow/metadata_extension_test.exs`

| Line | Test | Status |
|---|---|---|
| 61 | dedicated no-op configuration carries no schema or metadata | open |
| 79 | shared binding validates and seals required typed scope | open |
| 115 | dedicated storage scope leaves partition bytes untouched | open |
| 122 | provider output cannot introduce fields or malformed values | open |
| 132 | schema configuration rejects ambiguous and redefined field identities | open |

## `flow/history_api_test.exs`

| Line | Test | Status |
|---|---|---|
| 7 | validates history time range before reading storage | open |
| 12 | rejects non-keyword options | open |
| 17 | rejects value hydration above the configured ceiling before reading storage | open |
| 22 | prepare rejects malformed history event cursors | open |
| 36 | an omitted count respects a configured maximum below the built-in default | open |

## `flow/child_policy_test.exs`

| Line | Test | Status |
|---|---|---|
| 7 | validate_children requires a non-empty list | open |
| 14 | validate_no_parent_child_id rejects parent id as child id | open |
| 21 | optional_policy normalizes binary values and rejects unsupported values | open |
| 38 | exhaust_to_opts accepts explicit map or success/failure options | open |
| 49 | exhaust_to_opts rejects missing or empty states | open |

## `flow_run_steps_many_validation_test.exs`

| Line | Test | Status |
|---|---|---|
| 8 | run_steps_many rejects batches above the configured Flow item limit | open |
| 19 | run_steps_many bounds the item-by-step work admitted to one Raft apply | open |
| 30 | spawn_children rejects child batches above the configured Flow item limit | open |
| 44 | pipeline claim_due rejects inexact timestamps before routing | open |

## `flow/rewind_target_level_test.exs`

| Line | Test | Status |
|---|---|---|
| 24 | a rewind applies to an event this node's projector has not reached yet | open |
| 44 | apply refuses an event past the history cap that this node still holds | open |
| 75 | apply refuses a target entry that does not decode | open |
| 97 | a rewind restores a cold event of a partitioned Flow | open |

## `flow/options_test.exs`

| Line | Test | Status |
|---|---|---|
| 7 | binary options validate required and optional strings | open |
| 26 | integer and boolean options preserve Flow error messages | open |
| 45 | integer options reject values above Flow's exact integer ceiling | open |
| 64 | ref size and put helpers preserve existing semantics | open |

## `flow_facade_correctness_test.exs`

| Line | Test | Status |
|---|---|---|
| 8 | get fails closed when the persisted Flow record is corrupt | open |
| 26 | lease governance renewal preserves storage unavailability | open |
| 34 | get rejects payload hydration above the configured ceiling | open |

## `flow/retry_policy_test.exs`

| Line | Test | Status |
|---|---|---|
| 6 | flow policies reject malformed per-state policy values | open |
| 18 | flow policies omit absent fields from their canonical snapshot | open |
| 27 | each state retains its independently configured execution mode | open |

## `flow/record_read_test.exs`

| Line | Test | Status |
|---|---|---|
| 7 | filter_index_records applies field filter sort reverse and count | open |
| 20 | filtered candidate scans resume after an underfilled heuristic window | open |
| 52 | filtered candidate scans fail explicitly when the bounded window is not exhaustive | open |

## `flow/partition_key_test.exs`

| Line | Test | Status |
|---|---|---|
| 6 | encodes components with byte lengths | open |
| 11 | delimiter bytes cannot make different component lists collide | open |
| 16 | uses byte sizes for arbitrary binary components | open |

## `flow/metadata_validation_test.exs`

| Line | Test | Status |
|---|---|---|
| 6 | attributes identify canonical persisted metadata without rebuilding it | open |
| 36 | state metadata identifies canonical scalar-only persisted metadata | open |
| 56 | normalization rejects names that cannot be addressed by FQL | open |

## `flow/idempotent_create_after_progress_test.exs`

| Line | Test | Status |
|---|---|---|
| 31 | a retry after a worker claimed the Flow answers it | open |
| 38 | a retry after the Flow completed answers it | open |
| 52 | a different request for a claimed Flow still conflicts | open |

## `flow/history_values_test.exs`

| Line | Test | Status |
|---|---|---|
| 8 | history hydration marks storage failures without exposing their reason | open |
| 21 | history file-reference size is the exact stored byte count | open |
| 32 | history hydration marks corrupted typed values | open |

## `flow/create_running_validation_test.exs`

| Line | Test | Status |
|---|---|---|
| 6 | public create rejects running without reserving the identity or disrupting writes | open |
| 18 | public atomic batch rejects a running item without writing earlier valid items | open |
| 33 | the supported atomic start and claim still creates a leased running execution | open |

## `flow_batch_state_write_error_test.exs`

| Line | Test | Status |
|---|---|---|
| 23 | below level 5 COMPLETE_MANY answers OK past a refused state write | open |
| 29 | at level 5 COMPLETE_MANY fails on a refused state write, as FLOW.COMPLETE does | open |

## `flow_batch_indexed_attributes_test.exs`

| Line | Test | Status |
|---|---|---|
| 23 | below level 5 a Flow closed by COMPLETE_MANY keeps the indexed names it had | open |
| 34 | at level 5 Flows closed in batches are indexed as single commands index them | open |

## `flow_workflow_demo_test.exs`

| Line | Test | Status |
|---|---|---|
| 18 | demo logs full workflow lifecycle and query results | open |

## `flow_batch_waiter_correctness_test.exs`

| Line | Test | Status |
|---|---|---|
| 7 | a failed independent create does not suppress a committed item's waiter wake | open |

## `flow/retention_cleanup_error_test.exs`

| Line | Test | Status |
|---|---|---|
| 26 | a failed Flow storage flush answers a client error | open |

## `flow/payload_return_test.exs`

| Line | Test | Status |
|---|---|---|
| 7 | uses the configured payload return size as a hard ceiling | open |

## `flow/lmdb_writer/quarantine_flow_test.exs`

| Line | Test | Status |
|---|---|---|
| 21 | an undecodable Flow record is quarantined while the rest of its shard keeps projecting | open |
