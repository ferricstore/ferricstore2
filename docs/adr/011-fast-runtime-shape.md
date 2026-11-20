# ADR-011: runtime shape from the fsnp-slice measurements

Status: accepted, 2026-11-20, as the target for M3/M4 runtime work. Solo
decision on the user's request; no independent review is claimed. Each part
still needs its own implementation gate and disk/fault evidence.

## Evidence

The `claude/fsnp-slice` spike (branch `claude/fsnp-slice`,
`spikes/fsnp-slice/README.md`) served the Python SDK's canonical queue
benchmark over the existing native protocol, byte-identical to Elixir for every
frame it used, with replies released only after `fdatasync` (F_FULLFSYNC on
macOS). On an Apple M4 Max, one node, 16 shards, against Elixir `main`
6a5f2bc34 with the same client:

| | Elixir | Spike, log per shard | Spike, one shared log |
|---|---|---|---|
| flows/s, 1 client, 100k flows | 10,307 | 63,020 | 67,530 (client-bound) |
| flows/s, 4 clients, 400k flows | timed out (100k) | 219,954 | 265,654 |
| server CPU per 1k flows | 1,026 ms | 7.1 ms | 4.0 ms |
| RSS | 1.7 GB | 115 MB | 113 MB |

A SIGKILL check lost none of 212,500 acknowledged creates and 53,000
completions. The spike omits projections, history, policies and governance, so
its CPU per flow is a lower bound. Linux and multi-node runs are not measured.

## Decision

1. **The dedicated TCP adapter speaks FSNP v1.** All five existing SDKs speak it
   and the user's goal is a drop-in replacement for the Elixir server. This
   replaces the `FWTP/1` proposal in ADR-001 and `docs/transports/tcp.md` for
   the TCP adapter; HTTP is unchanged. Version 1 accepts only the workflow
   opcodes and control frames that are implemented, refuses every other opcode
   with an explicit error, and never sniffs or falls back to another protocol.
   Byte layouts follow the pinned native document and captured Elixir frames;
   every implemented frame is tested against a captured Elixir byte fixture.
2. **One group-committed log per node.** All shard groups append to one
   node-local log. A writer thread coalesces queued appends, writes them once
   and calls `sync_data`; acknowledgements are released only after that sync.
   On macOS a sync flushes the whole device, so one shared log costs one flush
   where per-group logs cost one each (measured: about 5x more acknowledged
   creates in the same window). Records carry length and checksum; recovery
   stops at the first torn or corrupt record. This replaces per-append
   `Immediate` redb commits (ADR-007) on the write path; the Raft adapter will
   store each group's entries in this log. ADR-007's framing and recovery
   invariants (exact tail, committed prefix, corruption fencing) carry over.
3. **One thread per shard owns that shard's state.** It applies committed
   commands in log order, deterministically, so replay rebuilds state.
   Connection tasks never touch shard state; they send commands and wait for
   the post-sync reply. A claim without a partition plans across shards from
   per-shard ready counts.
4. **No fixed small batch cap.** Batches are bounded by admitted bytes and item
   counts, not by an eight-mutation encoder limit.

## Placement

| Part | Crate and module | Policy change when the code lands |
|---|---|---|
| FSNP header, typed values, workflow request and reply codecs | `wf-api-tcp` (`frame`, private `codec`, `request`, `response`) | none |
| TCP listener and sessions | `wf-api-tcp` (`listener`, `session`) | Tokio as a reviewed external dependency of `wf-api-tcp` and `wf-node` |
| Group-committed log and its writer thread | `wf-storage::log` | writer module added to `thread_owner_modules` and `durable_sync_modules` |
| Shard executor threads | `wf-node::supervision` | added to `thread_owner_modules` |

All new production code is held to the ADR-010 panic ratchet from its first
line: no indexing, slicing or unchecked arithmetic on untrusted bytes.

## Order

1. FSNP codecs, tested against captured Elixir frames.
2. Group-committed log with crash tests (SIGKILL child, torn tail, corrupt
   record) on macOS, then Linux.
3. Shard executor and TCP listener, wired to the application create, claim and
   complete commands, measured with the same SDK benchmark.

Consensus (OpenRaft) integration, replication and the remaining workflow
commands follow their existing milestones.

## Progress

- Step 1 done: `wf-api-tcp` codecs, byte-identical to Elixir for the captured
  replies (0358bc1).
- Step 2 done on macOS: `wf_storage::log::GroupLog`. Blocks carry a SHA-256
  over a domain prefix, header and payload; a batch never spans blocks;
  recovery cuts only a torn last block and refuses a damaged block that
  precedes a valid one. A SIGKILL test lost none of 2,000 acknowledged
  records. Durable records per second with every append waited on, M4 Max,
  F_FULLFSYNC (about 4-5 ms per sync):

  | appending threads x records per batch | records/s | records per sync |
  |---|---|---|
  | 1 x 1 | 239 | 1.0 |
  | 16 x 1 | 2,063 | 8.4 |
  | 16 x 32 | 64,721 | 278 |
  | 64 x 32 | 243,359 | 1,056 |
  | 16 x 500 | 793,197 | 4,211 |

  Segments and failures (added after the first log commit): the log is a
  run of numbered segment files. The writer starts a new segment at the size
  limit (default 128 MiB) or on `rotate()`, which returns the new segment
  number once every earlier append is durable; `remove_before(n)` deletes
  older segments oldest first, so a crash leaves only a missing prefix.
  Only the last segment may lose a torn tail; damage in an earlier segment or
  a missing middle segment refuses to open. Storage sits behind the
  `SegmentStorage` trait (`DirectoryStorage` in production), which tests wrap
  to fail the Nth write (after half the bytes land), sync, segment creation,
  truncation or removal. A failed write, sync or creation fences the log
  until reopen and loses no acknowledged record; a failed truncation refuses
  to open without changing the file; a failed removal is reported, the log
  keeps working and a retry finishes it. A second SIGKILL test uses 1 KiB
  segments so kills land during rotations. Re-measured after the change:
  the same records per sync (8.4, 259, 1,024, 4,000).

  Not done: Linux runs, and the policy that decides when the node rotates
  and which checkpoint lets it remove segments (it belongs with the shard
  executor's snapshots).
- Step 3, first half: `wf_application::commands::queue` defines the queue
  service and the Elixir partition routing (4911d6a). `wf_node::composition::
  QueueNode` implements it: one thread per shard owns its records (keyed by
  partition and id) and a due index per type, state and priority; every
  create, named claim and complete goes through `wf_domain::workflow::reduce`.
  Each accepted change is logged as one single-Flow record, so replay routes
  by partition and survives a shard-count change. A completion thread
  releases a shard's replies, refusals included, only after that shard's
  latest append is durable; shard threads never wait for a sync. A failed
  append stops the shard until restart. Claims without a partition filter
  are split over shards by their due-indexed counts. A SIGKILL test (3,000
  acknowledged creates, three runs) lost none.

  Not done in the node: blocking claims, expired-lease reclaim, FIFO and
  policies, retention of completed Flows, and snapshots (so `remove_before`
  is never called and replay reads the whole log).
- Step 3, TCP listener: `wf_api_tcp::session::serve` runs FSNP sessions on
  Tokio 1.54.1 (`io-util`, `net`, `rt-multi-thread`, `sync`; no macros),
  newly allowed for `wf-api-tcp` and `wf-node`. A reader loop dispatches each
  frame to the queue service without waiting; a writer task keeps each lane's
  replies in request order. The handshake declares the 0.11.4 contract the
  Python SDK checks before it connects, including the `FLOW.QUERY` and
  `FLOW.POLICY.SET` schemas and the query manifest, so the SDK connects; this
  server refuses those commands, and every other unimplemented opcode, with
  an explicit error. Compact codecs are declared only for claim jobs and the
  batch ok-list. Non-independent `FLOW.CREATE_MANY`, blocking claims and
  expired-lease reclaim are refused by name.
- Step 3, node binary and SDK benchmark (b364939): `ferricstore2` serves the
  queue over FSNP. The Python SDK's canonical queue benchmark (`--transport
  many --partition-mode auto --claim-job-only`, 16 workers, 32 producers,
  batches of 500, 16 shards), unchanged, against the release build on the M4
  Max, 2026-11-20 (driver and raw rows: `claude/fsnp-slice` 7ff46e2,
  `results/2026-11-20-macos`):

  | | product | spike, shared WAL | Elixir 6a5f2bc34 |
  |---|---|---|---|
  | flows/s, 1 client, 100k flows | 75,763 (72,805-79,929, 3 rounds) | 71,714 (71,632-75,481) | 11,077 (1 round) |
  | server CPU per 1k flows | 8.6-9.1 ms | 3.7-4.3 ms | 1,077 ms, plus 189 s after the run |
  | RSS | 161-170 MB | 109-115 MB | 1.77 GB |
  | queue p99 | 255-366 ms | 335-403 ms | 3.7 s |
  | flows/s, 4 clients, 400k flows | 272,056 (588 MB) | 296,369 (310 MB) | not rerun (timed out on 2026-11-15) |

  One Python client sits at 85-88% CPU against either Rust server, so the
  1-client rows are client-bound. The product spends about twice the spike's
  CPU per Flow and 1.5-2x its memory: every decision goes through the domain
  reducer, which clones whole records and builds history entries the node
  does not keep yet.
- Next: snapshots so checkpoints can remove log segments, retention of
  completed Flows, blocking claims and reclaim, and the remaining commands.
