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
  the logged parent changes, so events are queued only after replay. Not yet:
  children on another shard than their parent (refused with an explicit
  error; the SDK places children in the parent's partition unless told
  otherwise), and cancelling running children when a parent closes. The
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
