# ADR-014: The whole FLOW command surface on the node

Status: accepted (2026-11-20). Solo decision on the user's request ("go 1 by
one for all features to exist"); no independent review is claimed.

## Context

After ADR-011 and ADR-013 the node serves the queue fast path: `FLOW.CREATE`,
`FLOW.CREATE_MANY`, `FLOW.CLAIM_DUE`, `FLOW.COMPLETE` and
`FLOW.COMPLETE_MANY`. The Elixir server (`main` 6a5f2bc34,
`apps/ferricstore_server/lib/ferricstore_server/native/commands.ex`) serves 66
FLOW opcodes, 0x0201 through 0x0258, and its records carry far more than the
node's: attributes, per-state metadata, named values, lineage, retention and
history settings, child groups.

Measured on 2026-11-20 with the Python SDK's integration suite (SDK `main`
53a6dd2, `tests/integration` and `tests/compatibility`): Elixir passes 60 of 60
(7 skipped for cluster, TLS and restart setups); the node passed 7, all of
them client-side checks. The failures are missing commands (`FLOW.RETRY_MANY`,
`FLOW.POLICY.SET`, `FLOW.TRANSITION_MANY`, `FLOW.GET`, `FLOW.VALUE.PUT`),
create options (`idempotent`, `values`), the typed-map claim, and key-value
commands, which this workflow-only server does not serve.

The Elixir server decides with the client's clock when a request carries
`now_ms`, and the SDK sends it on every create. The SDK's tests depend on it,
for example by claiming with `now_ms` 30 s ahead to reclaim an expired lease.

## Decision

1. **One generic path for every command beyond the fast path.** The TCP
   adapter decodes a typed-map body into an application `Term` (nil, booleans,
   integers, floats, binaries, lists, maps with binary keys).
   `wf_application::commands::flow::parse` reads the options as
   `Ferricstore.Flow.MutationAttrs` does, with the same defaults and `ERR ...`
   texts, into a typed `FlowCommand`. The node runs it on the shard that owns
   the Flow and replies with the `Term` the Elixir server would send; the
   adapter encodes it as a typed value. The compact fast paths stay as they
   are.
2. **The node logs after-images for these commands.** A change is logged as
   `Put`: the Flow's whole image (the snapshot image record, now with the
   extra fields). Replay stores it without deciding anything, so replay does
   not depend on command semantics that later releases change. The fast path
   keeps its logical records. Snapshot images move to version 2 (the extra
   fields after each record); version 1 snapshots are refused, which affects
   only benchmark data.
3. **Command time follows Elixir.** A command that carries `now_ms` is decided
   at that time; otherwise at the shard's committed time. This departs from
   ADR-004's proposal (client times as requested deadlines only) for drop-in
   compatibility; every logged change still carries the time it used.
4. **Record fields beyond the lifecycle record** live in
   `wf_application::commands::flow::record::FlowMeta`, boxed beside the
   domain record in each shard entry and absent for fast-path Flows (which
   have the defaults). Replies are built by `record::View`, matching
   `RecordProjection.public/1` field for field, including value reference
   names (`f:{tag}:v:...`, data format 2 digests).
5. **Acceptance.** Each step is checked three ways: Rust tests, a
   differential probe that sends the same commands to Elixir and to the node
   and compares the replies, and the SDK integration suite. Tests that need
   key-value commands are out of scope.

## Order

Each step is one commit with its tests.

1. Record fields, `FLOW.CREATE` with every option, `FLOW.GET`.
2. Typed-map `FLOW.CLAIM_DUE` with record replies, `FLOW.RECLAIM`.
3. `FLOW.TRANSITION`, `STEP_CONTINUE`, `RETRY`, `FAIL`, `CANCEL`,
   `EXTEND_LEASE` and their `_MANY` forms.
4. `FLOW.HISTORY`.
5. `FLOW.VALUE.PUT` and `FLOW.VALUE.MGET`.
6. `FLOW.START_AND_CLAIM`, `FLOW.RUN_STEPS_MANY`.
7. `FLOW.SIGNAL`.
8. `FLOW.SPAWN_CHILDREN` and child groups.
9. `FLOW.POLICY.SET` and `GET` (retry, state metadata, FIFO, retention),
   `FLOW.RETENTION_CLEANUP`.
10. `FLOW.REWIND`.
11. `FLOW.INFO`, `STATS`, `ATTRIBUTES`, `ATTRIBUTE_VALUES`.
12. `FLOW.QUERY`.
13. Schedules (`FLOW.SCHEDULE.*`).
14. Effects (`FLOW.EFFECT.*`).
15. Governance: approvals, circuits, budgets, limits, ledger, overview.

## Progress

- Step 1: `FLOW.CREATE` reads all 20 Elixir options and `FLOW.GET` replies
  with Elixir's record map. The differential probe (21 commands: creates with
  every option, idempotent retries and conflicts, reads with values, missing
  Flows, 8 invalid options) gave identical replies for 20; the 21st differs
  only in the wall-clock time of a create sent without `now_ms`.
- Step 2: typed-map `FLOW.CLAIM_DUE` (a body without the custom-payload
  flag) and `FLOW.RECLAIM` read Elixir's options (states, worker, lease,
  limit, priority, `now_ms`, return shape, payload and named values,
  reclaim, partitions, `block_ms`) and reply in all six Elixir shapes:
  records (with payload, result and error up to `payload_max_bytes` and
  their sizes, named values, attributes, state metadata), jobs, and the four
  compact job lists. They use the multi-shard claim of ADR-013; claims are
  decided at the client's `now_ms` when sent. Found while comparing: a claim
  without a partition takes only auto-partition Flows in Elixir (`ANY` takes
  every partition, `GLOBAL` the unpartitioned Flows the node never has). The
  node had claimed from every partition on both paths; both now follow
  Elixir. The differential probe matched Elixir on 16 of 16 commands; the
  SDK integration suite went from 7 to 11 passing, including both
  claim-record contract tests.
- Step 3: `FLOW.TRANSITION`, `STEP_CONTINUE`, `RETRY`, `COMPLETE` (typed),
  `FAIL`, `CANCEL` and `EXTEND_LEASE`, and `FLOW.COMPLETE_MANY` (typed),
  `TRANSITION_MANY`, `RETRY_MANY`, `FAIL_MANY` and `CANCEL_MANY` in typed and
  compact forms. The domain reducer decides each change (`reduce_public`, so
  a retry without `run_at_ms` gets Elixir's policy timing and jitter); the
  node applies attribute and state-metadata updates, named values, value
  reference versions (the record version that wrote each value) and the
  removal time of a Flow that became terminal (the shard's clock plus its
  retention, as Elixir uses its own clock there). A batch in one named
  partition that is not independent is all or nothing: the shard puts back
  the Flows it changed when an item fails and logs nothing. Independent and
  `MIXED` batches decide each item. The single-mutation probe matched Elixir
  on 33 of 38 commands (the rest: removal times taken at different moments,
  and two internal counters Elixir's `EXTEND_LEASE` reply leaks); the batch
  scenario, run through the SDK's own `_many` methods, matched exactly. The
  SDK suite passes 19 (was 11). Mutations that carry `state_meta` are sent
  by the SDK through the command-exec opcode, which comes later.
- Step 4: `FLOW.HISTORY`. Every Flow, fast-path ones included, gets one
  event per record version (Elixir's `created`, `claimed`, `transitioned`,
  `retry`, ... with its record snapshot and, for a retry, the retry audit:
  decision, backoff, limit, next run). Events are kept per Flow as one
  LEB128-packed byte stream and decoded only when read. A logged after-image
  carries the event its change appended (so replay keeps the retry audit);
  the fast path's logical records re-derive theirs on replay; snapshot images
  (version 3) carry the whole history. Reads follow Elixir: event-id, time,
  version, event-name and worker filters, `rev`, and the newest `count` in
  order. Retries now always take the domain's policy path, as Elixir does:
  an explicit `run_at_ms` replaces the computed time and the retry limit
  still applies. Probe: 19 of 20 identical (the other is `EXTEND_LEASE`'s
  internal counters). Cost on the queue fast path, SDK benchmark, 100k Flows,
  1 client, three alternating runs against b6c292b: throughput unchanged
  (client-bound), server CPU 9.4 to 9.8 ms per 1k Flows (+4%), RSS 186 to
  223 MB (about 370 bytes per Flow for its three events). A first version
  with one struct per event cost +17% CPU and +170 MB; packing fixed that.
- Step 5: `FLOW.VALUE.PUT` and `FLOW.VALUE.MGET` (typed, and the SDK's
  compact read). A named value has its own version: the same bytes change
  nothing, different bytes need `OVERRIDE` (or `OVERRIDE_VALUE` on a
  mutation), raise the value's version, the record's version and update
  time, and add a `value_put` history event. A shared value gets a new
  24-character reference from node-local entropy, so its auto bucket is
  known before routing, and an optional deadline; it is logged as its own
  record and carried in snapshot images (version 4). A read routes each
  reference by its hash tag. Routing now uses data format 2, a fresh Elixir
  node's: a named partition's tag (and so its slot) uses the first 16 bytes
  of its SHA-256 instead of all 32; logs and snapshots name partitions, so
  existing data replays onto the new placement. The value scenario run
  through the SDK matched Elixir exactly; the SDK suite passes 22 of 60.
- Step 6: `FLOW.START_AND_CLAIM` (the domain's start-and-claim: token
  `worker:now:1`, attempts 1; the reply always names `max_active_ms` and
  `value_refs`) and `FLOW.RUN_STEPS_MANY` (each item started claimed,
  continued through its states one millisecond apart on the first lease
  deadline, then completed; all or nothing per shard, so one existing id
  fails every item of its shard, and a repeated id fails the whole batch).
  A change that goes through several versions logs all its events in its
  after-image. History events now also carry the Flow's value references,
  attributes and state metadata as JSON when present, as Elixir's do. The
  start/run-steps scenario, the value scenario and the batch scenario match
  Elixir exactly (the Elixir reference server was restarted first: after
  about an hour its history reads timed out with projector flush timeouts).
- Step 7: `FLOW.SIGNAL`. The version rises; `transition_to` (allowed only
  from an `if_state`, never from a terminal state or into one) moves the
  Flow, releases its lease and makes it due at `run_at_ms` or now, keeping
  its run state; named values are written; a `signaled` event carries the
  signal's name. An idempotency key keeps the digest of what it applied: the
  same content again changes nothing, as Elixir does from feature level 5
  (the reference node runs below level 5 and applies it again), different
  content is refused. The signal scenario matches Elixir exactly.
- Step 8: `FLOW.SPAWN_CHILDREN` and child groups, plus the typed `PIPELINE`
  frame (0x000E) the SDK reads child results through. A spawn checks the
  parent's guards (group idempotency by request hash, `from_state`, fence,
  lease, not terminal, `wait_state` when waiting), creates every child
  (parent, root and partition linked; spawn-wide values merged under each
  child's own) and moves the parent to its wait state without a lease or due
  time; the parent and children are logged as one `Group` record, so the
  spawn is all or nothing and replay splits it by shard. A live change that
  makes a child terminal queues an event; the shard settles the queue before
  it appends: the open group records the child's status and references and,
  once resolved, moves the parent to its success or failure state and makes
  it due (a terminal parent cascades to its own parent). Replay re-applies
  the logged parent changes, so events are queued only after replay.
  Children must share their parent's slot (refused `CROSSSLOT`, as Elixir
  refuses children on another shard; the SDK places children in the
  parent's partition unless told otherwise), so a parent and its children
  stay together under any shard count. A parent that closes, or whose
  group resolves, cancels the running children of its groups that ask for
  it (`on_parent_closed: cancel_children`, the default), in one logged group
  with its `children_cancelled` event. The
  pipeline runs each Flow command as it would alone and replies with Elixir's
  typed list of `{lane_id, opcode, request_id, status, value}`; compact
  pipeline bodies (key-value modes) are refused. The child scenario matches
  Elixir except the opaque `request_hash` (Elixir hashes its own term
  encoding); the SDK suite passes 30 of 60, including all six child-group
  tests and all thirteen durable-step recovery tests.
- Step 9a: `FLOW.POLICY.SET` and `FLOW.POLICY.GET` (opcodes 0x021E/0x021F),
  as `Ferricstore.Flow.Policy`, `PolicyPatch` and `RetryPolicy`. Shard 0
  owns every type's policy, as in Elixir: a set checks the expected
  generation, deep-merges its patch into the stored normalized form (a nil
  state deletes it) or replaces it, normalizes the result with Elixir's
  checks and texts, raises the generation and logs the policy as one
  `Policy` record (tag 10). Once that record is durable the node installs a
  copy on every other shard, then replies; a shard keeps the newest
  generation it has seen. Replay sends `Policy` records to shard 0 whatever
  log holds them, snapshot images (version 5) carry each shard's table and
  load it into shard 0, and the node copies shard 0's table to every shard
  at open. The policy applies where Elixir applies it: a create resolves its
  retention (create options over the state's retention over node defaults)
  and its `max_active_ms` (a create's `infinity` opts out, which also fixes
  a step 1 departure that stored and showed `"infinity"`), and a retry hands
  the type's retry view to the domain reducer. The typed protocol has no
  top-level `retention`, `version`, `mode` or `governance` option, so those
  are refused as unknown; state-level governance is refused until the
  governance step. The policy scenario matches Elixir except those refusals
  and the unknown-option text. FIFO claim lanes (step 9b) and FLOW commands
  sent as text through COMMAND_EXEC (step 9c, which the SDK uses whenever a
  call carries `indexed_state_meta` or a non-create `state_meta`) follow.
- Step 9b: FIFO states, as Elixir's `FifoLane`. A lane is a type, a logical
  state (a running Flow's is the state it was claimed from) and a partition;
  its Flows are ordered by `Entry::sequence`, which is now stamped again
  whenever a Flow's logical state changes (Elixir's `state_enter_seq`), so a
  Flow entering a state joins the tail and images keep lane order. Lanes are
  kept only for types whose policy has a FIFO state (an index beside the
  due index, rebuilt when such a policy is installed, so other types pay
  nothing). A claim of a FIFO state takes only lane heads that wait and are
  due: a running head or one not due yet (a retry's backoff) blocks its
  partition. A head leaving its lane reports the lane's queue as an arrival,
  so parked claims wake. Entering a FIFO state follows
  `flow_require_fifo_entry/4`: creates (fast path, generic and children),
  transitions and step-continues need an explicit partition, priority must
  be 0, a Flow cannot enter running, and a retry checks priority only. The
  FIFO scenario (guards, heads per partition, retry and running blocking,
  completion and transition freeing a lane, entry at the tail, an
  auto-partition Flow entering FIFO, a policy set after its Flows exist)
  matches Elixir exactly. Not covered: `FLOW.START_AND_CLAIM` and
  `FLOW.RUN_STEPS_MANY` into a FIFO state are not refused.
- Step 9c: FLOW commands sent as text through `COMMAND_EXEC` (0x0100), as
  `NativeAstParser` reads them, also inside a typed `PIPELINE`. The SDK
  sends this form whenever a call carries `indexed_state_meta` or a
  non-create `state_meta`. A table of forms (positional keys, then
  options) covers CREATE, VALUE.PUT, SIGNAL, GET, POLICY.SET, POLICY.GET,
  CLAIM_DUE, RECLAIM, EXTEND_LEASE, COMPLETE, TRANSITION, RETRY, FAIL,
  CANCEL, REWIND, HISTORY, ATTRIBUTES, ATTRIBUTE_VALUES, STATS, INFO and
  RETENTION_CLEANUP; options follow `@flow_option_specs` (names in any
  case, values coerced per kind, `ATTRIBUTE`/`STATE_META` pairs, read
  options such as `FULL`, `PAYLOAD MAXBYTES`, `VALUE`, `PARTITIONS`, signal
  value pairs, POLICY.SET's scoped retry/backoff/retention options and
  `STATE` sections). Numbers and booleans among the arguments count as their
  text, as Elixir stringifies them. Each text command becomes the typed map
  its opcode would carry and goes through the same parser, so both forms
  decide and answer alike; commands whose typed form is not built yet are
  refused by that parser. Two departures it exposed are fixed: FLOW.RETRY's
  own `retry` override (`max_retries`, `exhausted_to`, backoff) was accepted
  but ignored and now layers over the type and state policy, and FLOW.GET
  now checks and ignores `full`, `payload_max_bytes`, `include_cold` and
  `consistent_projection` as Elixir does. A policy may now hold a type-level
  retention (only the text form can set one), layered under state
  retention. Item-list text forms (`*_MANY`, `SPAWN_CHILDREN`) and
  `FLOW.QUERY` are refused until step 9d and the query step; other
  commands are refused, as this server keeps no key-value store. The text
  scenario matches Elixir except the unknown-command status (`error`, not
  `bad_request`) and key-value commands.
- Step 9 (end): `FLOW.RETENTION_CLEANUP` (0x0221) and active-time limits.
  A live Flow with `max_active_ms` gets a deadline (creation time plus the
  limit) in a per-shard index that holds only such Flows. Past it, a
  timeout fails the Flow as Elixir's `flow_active_timeout_record/4` does:
  state `failed`, lease and due time cleared, run state kept, the error
  the term `{reason: "max_active_ms", max_active_ms}` (a new
  `FlowMeta::error_term` flag makes value reads return it as a term), a
  `failed` event carrying `reason` and `max_active_ms` (history extras grew
  from four to six), and terminal retention started from the later of
  command and wall time. Shards time Flows out at Elixir's sweeper cadence
  (ten minutes after start, then every ten minutes), so a client clock far
  behind the wall clock is not failed sooner than Elixir would fail it;
  terminal removal keeps running every round (ADR-013).
  `FLOW.RETENTION_CLEANUP` times out, then removes expired terminal Flows,
  shard by shard against one `limit`, sums Elixir's counts (`flows`,
  `history`, `values`, `active_timeouts`) and, when the limit runs out,
  answers `more?` with an opaque continuation naming the shard to resume.
  The counts of history and values follow this node's storage, not
  Elixir's key counts. Snapshot images are version 6 (meta flag and the
  wider extras); after-images logged by earlier development builds of this
  branch no longer decode. Not built: cancelling children when a parent
  times out (as for any parent closing).
- Step 10: `FLOW.REWIND` (0x0216, and its text form), as Elixir's
  `do_flow_rewind/2`. A rewind restores what one history event recorded:
  state, attempts, due time (or `run_at_ms`), priority, payload, result
  and error, named values, attributes and state metadata (read back from
  the event's JSON with a small reader, `json::decode`). Version and fence
  rise, the lease is cleared, the run state stays, a `reason` becomes the
  error, terminal retention restarts for a terminal target, and the Flow
  keeps `rewound_to_event_id` (on the record and on every later event; the
  event itself is `rewound`). Refusals follow Elixir: a leased Flow, a
  parent or child, a wrong `expect_state`, an event not held, a
  claim event (`running state is only entered by FLOW.CLAIM_DUE`, as
  Elixir's `flow_test` rewind tests expect; audit, 22 Nov), `reason_ref`. This
  node keeps only the current bytes of each value, so an event whose
  payload, result, error or named value version was replaced since is
  refused as not found where Elixir may still hold the older version.
  Snapshot images are version 7. The rewind scenario matches Elixir; the
  SDK suite passes 37 of 60, both rewind round trips included.
- Step 11: `FLOW.INFO` (0x021C), `FLOW.STATS` (0x022D), `FLOW.ATTRIBUTES`
  (0x022E) and `FLOW.ATTRIBUTE_VALUES` (0x022F), and their text forms, as
  `InfoAPI.info/3` and `ReadAPI.stats/3`, `attributes/3` and
  `attribute_values/4`. Each shard keeps a catalog of its Flows by type,
  state and partition, ordered by update time, beside the claim index;
  QUERY will read it too. A read of one partition goes to its shard; a
  read without a partition covers the 256 auto buckets (Elixir's
  `partition_key: :auto`), on every shard, and the parts are joined.
  INFO counts queued, running, completed, failed and cancelled Flows and
  the leased ones; STATS counts one state, or with `attributes` the Flows
  matching them (a list filter needs all its values); ATTRIBUTES lists the
  policy's `indexed_attributes` by values held (a list attribute counts
  each value), ATTRIBUTE_VALUES one name's values by count, ties in
  `inspect/1` order. State `any` reads Elixir's broad index: a Flow shows
  an attribute there only if the type's policy indexed it when the Flow
  was last written, and STATS counts through the candidate fewest Flows
  hold, as `select_scored_candidate/1` picks. To give that, a Flow records
  the policy's `indexed_attributes` and `indexed_state_meta` on every live
  whole-Flow write (create, mutations, signals, rewinds); GET shows them as
  Elixir does, and they are logged and in snapshot images (version 8).
  Claims and fast-path completions keep the names the Flow had, since their
  replay could not see the policy of the moment; Elixir also refreshes them
  there. Attribute reads leave out terminal Flows past their retention;
  state counts keep them until removal, which here happens within a round,
  so a Flow with a tiny retention leaves INFO sooner than in Elixir, where
  it stays until cleanup. Counts are exact: the catalog is in memory, so
  there is no scan limit, no `approximate` flag and no `exceeds scan limit`
  refusal. The scenario matches Elixir apart from that and the
  unknown-option wording; the SDK suite passes 38 of 60.
- Step 12: `FLOW.QUERY` (0x0231) and `FLOW.QUERY.INDEXES` (text). The FQL1
  parser is a port of the Elixir server's production parser (the native
  `fql.rs`): the same grammar, budgets (16 KiB, 256 tokens, 12 predicates,
  20 `IN` values, 32 projected fields, `LIMIT` 1 to 100) and failure bytes,
  rewritten without indexing for the panic ratchet. Binding and checks
  follow `Binder.bind/2` and `Request.validate/2`; reasons Elixir has no
  code for (a duplicate `IN` value, a reversed range, a repeated order
  field) answer `invalid_syntax` as there. A query runs on the shard owning
  its partition (a `run_id`-only read uses the run id's auto bucket):
  candidates come from the step-11 catalog, narrowed by fixed types and
  states, as rows of the GET map's 18 allowlisted fields; predicates,
  ordering (nulls, then absent values, last; ties by the SHA-256 of the
  id), paging and projection follow `ReferenceEvaluator`, `RecordOrder` and
  `RecordProjection`. Event queries page the Flow's history by event id.
  Cursors are `fqc1_` tokens bound to the query and expiring after five
  minutes, as Elixir's, but authenticated with HMAC-SHA256 under a
  per-shard key instead of encrypted, so they do not survive a restart.
  Failures answer with status 6 and Elixir's map: code, message, and for
  the seven parser reasons a position, detail, hint and context. Results
  are authoritative and current (Elixir's come from an asynchronous
  projection, so a new Flow appears there only after a lag); there are no
  scan, memory or response budgets, and usage reports rows and checks but
  zero bytes and time. EXPLAIN describes the catalog path, not Elixir's
  cost model; `FLOW.QUERY.INDEXES` reports the catalog as one active index.
  Not built: the compact `flow_query_result_v1` codec (the server declines
  it, so clients get typed values), the text form of `FLOW.QUERY`, and
  index lifecycle, statistics and mandatory scopes. The scenario of 90
  queries and errors matches Elixir apart from one quirk: on one Elixir
  claim path a `claimed` event omits `attributes`. The SDK suite passes 40
  of 60, both query planner tests included.
- Step 13: `FLOW.SCHEDULE.*` (0x0225 to 0x022C), as
  `Ferricstore.Flow.Schedule`. A schedule is an internal Flow of type
  `__ferricstore_schedule`, id `__ferricstore_schedule__:<id>`, in one of
  256 internal partitions, with its definition (Elixir's fields) as an
  encoded payload; public creates and claims refuse that type and id
  prefix. Kinds are one-shot, delay, interval and cron; creation checks
  every option in Elixir's order and texts. A fire claims the schedule
  under a lease, reads the previous target when the overlap policy needs
  it, plans (catch-up `fire_once` coalescing for automatic interval fires,
  `max_fires`, `end_at_ms`, the timestamp limit, `skip`,
  `queue_after_previous` with a doubling retry up to 30 s, `fail_schedule`),
  creates the target with an idempotent id and ownership attributes (an
  HMAC-SHA256 digest keyed by a per-schedule secret; a target id another
  Flow holds is refused), then applies the outcome under the lease; a
  failure part-way releases the schedule, a refusal keeps the lease. The
  node runs schedule commands and a background poll on one schedule thread
  (in `supervision`, so a fire's cross-shard steps are sequential calls);
  it polls every second, also between commands, and FIRE_DUE fires due
  schedules across shards in due order. Schedule records are after-images,
  so replay restores them; terminal schedules are kept from the later of
  command and wall time. Cron expressions resolve wall-clock minutes in
  IANA zones through `jiff` 0.2.28 (system zoneinfo, no bundled data): a
  repeated DST minute gives both instants, a skipped one none, as Elixir's
  `Tz` does. This adds `jiff` to `wf-application`'s reviewed external
  dependencies (`architecture/policy.toml`); cargo-deny passes. Two gaps
  the schedule tests exposed are fixed: FLOW.CREATE now takes a payload of
  any term (kept as this node's term encoding, flagged in `FlowMeta`, read
  back as the term by GET and value reads; its size is this encoding's),
  and FLOW.GET returns the payload, result and error when `payload` or
  `full` is set, up to `payload_max_bytes`, as Elixir's `PayloadReturn`
  does. Snapshot images are version 9. Not built: waking a queued schedule
  when its previous target ends (`wake_queued`; it retries on its backoff
  instead), Elixir's schedule history events and catalog projections. The
  ownership digest differs from Elixir's (another encoding), and schedule
  partitions use this node's bucket hash, not `phash2`. The scenario
  matches Elixir except lines shaped by Elixir's own background scheduler
  and the unknown-option wording; the SDK suite passes 40 of 60, its
  native schedule test now failing on a governance opcode.
- Step 14: `FLOW.EFFECT.*` (0x0240 to 0x0244) and `FLOW.GOVERNANCE.LEDGER`
  (0x0245), as `Governance.Effect` and `Governance.Ledger`. A reservation
  needs the Flow's current lease token and fence; a retry with the same
  `operation_digest` is answered from the stored effect (`already_reserved`,
  before any policy check), another digest is a `GOVERNANCE_CONFLICT`. A
  reserved effect moves once to confirmed, failed or compensated (the same
  status again is `already_applied`, another a conflict, an earlier
  `now_ms` refused), keeping the external id, error, reason and latency
  given. Reservations and moves append ledger events with Elixir's
  decision ids; the ledger lists by time window, newest first on `rev`,
  keeping the newest 1,000 events within 900,000 bytes. Effects are
  decided under the state's resolved governance policy, and a leased Flow
  is `running`, a state no policy may name, while the typed protocol has
  no type-level governance: every effect is decided under Elixir's default
  policy, which refuses a reservation without an idempotency key
  (`GOVERNANCE_EFFECT_DENIED`, recorded in the ledger). Refusals reply
  with status 1 and the decision's fields; Elixir also sends the
  transport's `code: "error"` under the same key, which the SDK decoders
  refuse as a duplicate, so this node sends the decision's code once.
  Effects and the ledger are stored with the Flow (logged as its
  after-image, imaged in snapshots, version 10), so replay restores them
  and retention removes them with it, as Elixir's retention owner does.
  Differences: the policy hash is this node's own (see below), refusal
  event ids are numbered per Flow instead of
  random, a non-string `governance_scope` is refused, and options a command
  does not read are refused (Elixir accepts any known Flow option). The
  scenario matches Elixir apart from those; the SDK suite passes 40 of 60,
  its native test now past the effects and failing on approvals.
- Step 15a: governance records and `FLOW.APPROVAL.*` (0x0246 to 0x0249,
  0x0253), as `Governance.ApprovalStore`. Governance records that belong
  to no Flow (approvals now; circuits, budgets and limits next) live in a
  per-shard store keyed by kind and name, on the shard their name routes
  to as a partition (Elixir's key carries the name's hash tag). A command
  on one record goes through the single-Flow path and is decided in
  `wf-application` from the record's stored form; the shard stores or
  removes the result and logs it as a `Governance` record carrying that
  partition, so replay routes it under any shard count; snapshots carry
  each shard's records in their own section (image version 11). A
  listing asks every shard for its matching records once its round is
  durable and joins the parts. An approval request names a Flow and a
  scope, keeps assignees in order without repeats, and expires after
  `timeout_ms` (which wins) or at `expires_at_ms`, both checked against
  the request's time; it takes one decision, refused with
  `GOVERNANCE_CONFLICT` once terminal, and a decision at or after the
  expiry records the approval as expired and is refused. Listings filter
  by status, scope (a `partition_key` also matches its `partition:`
  scope) and Flow, newest request first, up to 1,000. The scenario
  matches Elixir apart from the unknown-option wording; the SDK suite
  passes 40 of 60, its native test now failing on circuits.
- Step 15b: `FLOW.CIRCUIT.OPEN`, `CLOSE` and `GET` (0x024A to 0x024C), as
  `Governance.CircuitStore`, stored in the governance record store by
  scope. A manual open sets the rule options given (keeping configured
  values for those left out; a new circuit starts from Elixir's defaults)
  and opens the circuit; a close resets it, and closing an unknown scope
  stores a closed circuit with a threshold of one. Each appends the state
  change and the manual action to the event history, newest first, keeping
  64 and, with `window_ms`, those within the window; a time before the
  last change is refused. Rules are checked in Elixir's order with its
  tracking bounds. Recording effect outcomes and half-open probes is not
  built: only effects decided under a policy naming a circuit do that,
  and no reachable policy can (step 14); `expected_review` is not a wire
  option. The scenario matches Elixir apart from the unknown-option
  wording; the SDK suite passes 40 of 60, its native test now failing on
  budgets.
- Step 15c: `FLOW.BUDGET.RESERVE`, `COMMIT`, `RELEASE`, `GET` and `LIST`
  (0x024D, 0x024E, 0x0255, 0x0257, 0x0258), as `Governance.Budget` and
  `BudgetStore`, stored by scope. A first reservation creates the budget
  from its `limit` and `window_ms` (ignored afterwards). A reservation
  holds part of the window's limit (a repeat with the same amount is
  answered from it, another amount or a settled id refused) or is refused
  with `GOVERNANCE_BUDGET_EXHAUSTED` and the time until the window ends. A
  commit settles it with the amount used (moving the window's usage by the
  difference and reporting any overage) and its usage term, bounded as
  Elixir bounds it; a release returns the hold. A window that has passed
  resets the usage and keeps unsettled reservations; settling one from an
  earlier window leaves the usage. The newest 128 settled reservations
  are kept for idempotent repeats. A refusal after a reset or on a new
  budget still stores it. Generated reservation ids are 22 URL-safe
  characters from a digest of the scope and a count instead of random.
  Listings filter by scope or partition and order by scope. The scenario
  matches Elixir apart from the unknown-option wording; the SDK suite
  passes 40 of 60, its native test now failing on limits.
- Step 15d (direct limits): `FLOW.LIMIT.LEASE`, `SPEND`, `RELEASE`, `GET`
  and `LIST` (0x024F to 0x0252, 0x0256), as `Governance.CreditLease` and
  `LimitStore`, stored by scope. A limit is an owner of credits leased per
  node shard; the shard id is checked against this node's shard count
  before the command is routed. A lease grants up to the credits asked
  for from those free (a new owner needs a `limit`), or is refused with
  `GOVERNANCE_LIMIT_EXCEEDED` after marking credits on other leases for
  reclaim; a spend moves available credits into use under generated
  `flr1:<generation>:<batch>:<n>` reservation ids; a release by ids
  returns those in use, and a lease with nothing left in use starts a new
  generation. Expired leases are reclaimed on lease, spend and get (and in
  listings' views); a lower limit drains available credits, lowest shard
  first. A `config_version` guards limit changes (an older one is ignored,
  the same one must agree); a numeric `policy_version` is its own
  configuration version, and the policy fingerprint is SHA-256 of the
  version's Erlang term, so `policy_version_hash` equals Elixir's.
  Reservation ids live with their lease instead of Elixir's paged records
  and cleanup tasks, and batch ids are digests instead of random. Elixir
  answers a spend on a lease marked for reclaim with `ERR flow limit record
  is corrupt`; this node applies the credit rules. The scenario matches
  Elixir apart from the unknown-option wording; the SDK suite passes 40 of
  60, its native test now failing on the overview. The running-Flow limit
  that claims spend from (state governance `limits.running`) is the next
  part.
- Step 15e: `FLOW.GOVERNANCE.OVERVIEW` (0x0254), as
  `Governance.Admin.overview/2`: the approval, budget, limit and circuit
  listings under the same options (each reads the options it knows and
  refuses its own invalid ones, approvals first), joined from every
  shard's records in one pass, with counts of approvals, pending
  approvals, budgets, limits, circuits, open and half-open circuits.
  Circuits are listed open first, then half-open, then closed, each by
  scope; Elixir's wire layer has no circuit status filter, so neither
  does this node. The scenario matches Elixir apart from the
  unknown-option wording. The SDK suite passes 41 of 60: its native
  workflow test (schedules, effects and every governance command) passes;
  the rest are key-value commands, `CREATE_MANY`'s `idempotent` option, a
  LangChain flow test and a topology check.
- Step 15d (state governance): a Flow policy's state may now carry a
  `governance` policy, normalized as `Governance.Policy.normalize/1` does
  (mode, effect rules, audit, named limit, budget and circuit rules,
  approvals; text choices lowered, lists without repeats, rules checked in
  name order, 1 MiB bound) and kept sparse; `nil` removes it, a later set
  deep-merges into it, and `FLOW.POLICY.GET` shows it for the state.
  Effects are still decided under the default policy, since a leased Flow
  is `running`, a state no policy may name. The policy scenario (a full
  policy, merges, removal, empty forms, 24 invalid parts) matches Elixir
  exactly. The running-Flow limit claims take from these rules is next.
- Step 15d (governed claims): a claim with `governance_limit_scope` and
  `governance_shard_id`, or of a type whose policy gives a claimed state
  `governance.limits.running`, is governed as `ClaimDueAPI` governs it. The
  node spends one credit per Flow the claim may take from the limit's
  lease (a policy limit's scope is `flow-running:` and the type's
  SHA-256, leased on demand with the claimed states' lowest limit and
  lease size and the policy generation as its configuration version),
  claims with those credits, each claimed Flow stamped with one (the claim
  logged as the Flow's after-image), and releases the credits no Flow
  took. When no credit can be spent the claim answers with no Flows if
  none could be claimed, and with `GOVERNANCE_LIMIT_EXCEEDED` otherwise. An
  explicit limit is approximate: as `LimitCache` does, a miss spends a
  chunk (four times the claim, at most 1000) and keeps the rest in a
  node-local cache for later claims, falling back to the claim's own
  amount; a policy limit's default `strict_global` spends directly. A Flow
  holding a credit releases it when it leaves `running`, whatever command
  moved it; the release is delivered once that round is durable, and
  replay delivers the releases of the replayed log again (a release of a
  credit already released changes nothing), so no outbox is kept; a
  credit freed just before a snapshot and not yet delivered when the node
  stops is reclaimed when its lease expires. Governed claims run on the
  schedule thread and do not wait (`block_ms` is ignored). Elixir picks a
  policy lease's shard id with `phash2(worker)`; this node with the
  worker's routing slot, so that id differs. Mutations accept the two
  governance options, as Elixir does. The governed-claim scenario matches
  Elixir apart from that shard id.
- Policy hash (after step 15): an effect's `policy_hash` names the policy
  of the Flow's type, as Elixir's does, but in this node's encoding:
  the URL-safe base64 SHA-256 of the default governance policy followed
  by the type's normalized policy, or of the default alone for a type
  without one. Normalization orders state and rule names and builds every
  other map in a fixed order, so equal policies share a hash however
  their maps were given; Elixir's also covers the type's name, so equal
  policies of two types differ there, and its values differ from this
  node's. No SDK or Elixir test compares values (they pass the field
  through, and check that a refusal and its ledger event agree), and this
  node imports no Elixir data, so matching Elixir's bytes buys nothing.
  The hash is computed once per policy, when a shard installs it (about
  2 µs on a set or restore), and the default's once per process, so a
  reservation copies 43 bytes (18 ns) instead of encoding and hashing the
  policy (at least 886 ns before). A reserved effect keeps the hash it
  was decided under. Decisions are unchanged: both servers refuse
  governance on `running` and at a policy's top level, so effects are
  always decided under the default rules. The lease shard id stays the
  routing slot: it only spreads workers over a limit's leases, both
  hashes are stable and spread evenly, neither is secret, and a client
  can name the shard directly.
