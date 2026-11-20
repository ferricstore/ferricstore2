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
