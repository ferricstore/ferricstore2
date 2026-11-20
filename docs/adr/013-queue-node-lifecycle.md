# ADR-013: Queue node lifecycle: reclaim, blocking claims, retention, snapshots

Status: accepted (2026-11-20)

## Context

ADR-011 step 3 put the queue (create, claim, complete) on shard threads behind
the group-committed log. Four gaps remained:

- The SDK sends `reclaim_expired = true` by default (`protocol_flow_codec.py`
  encodes `payload.get("reclaim_expired", True)`), and Elixir defaults it to
  true with `reclaim_ratio` 25. The node refused every such claim, so only
  clients that turn reclaim off could work.
- Blocking claims (`block_ms`) were refused.
- Completed Flows stayed in memory forever.
- Nothing wrote a snapshot, so `remove_before` never ran: the log grew forever
  and a restart replayed all of it.

Also found while designing blocking claims: a claim without a partition filter
is split over shards by their counts of indexed Flows, and a shard with a zero
share is not asked. A claim for one Flow therefore asked only the first shard
with indexed Flows, and returned nothing when that shard's Flows were scheduled
later even though another shard had a due Flow. A blocking claim would wait
forever on that.

## Decision

1. **Expired-lease reclaim follows Elixir's order.** Each shard indexes leased
   running Flows by lease deadline beside its due index. With reclaim on and a
   positive ratio, a shard claims its share as Elixir's router does: for one
   Flow, a due Flow first and an expired lease second; for more, expired
   leases up to `ceil(share * ratio / 100)` (at least 1), then due Flows, then
   expired leases for the rest. A claim for state `running` takes only expired
   leases. A ratio of 0 takes due Flows first and expired leases for the
   rest. The reducer's `Reclaim` operation decides each one (new lease, next
   fence, logical run state kept). Reclaims are logged with their own record
   tag so older logs still decode. A ratio outside 0 to 100 is refused with
   Elixir's error text.
2. **A claim asks again on shortfall.** The first round keeps the weighted
   split. When it returns fewer Flows than the limit, the node asks every
   shard that may still have some (not asked yet, or filled its share) for
   its count of claimable Flows now. That count is a read that waits for no
   sync. The node then splits the remainder by those counts and claims again,
   for at most four rounds. The same count reports the earliest future due time
   of matching Flows, including lease deadlines when reclaiming.
3. **Blocking claims park in a waiter thread.** `block_ms` of 0 waits until a
   Flow arrives, as in Elixir; above 4,294,967,295 is refused with Elixir's
   text. A blocking claim that finds nothing parks in
   `wf-node::supervision::waiters`. It is retried when a shard applies a create
   of its Flow type, or at the earliest future due time from step 2. At its
   deadline it replies with an empty list. Lost wake-ups are ruled out by a
   node-wide epoch: a shard increments it after a round that made Flows
   claimable, then checks whether anything is parked; a claim reads it before
   its attempt, and parking retries at once if it moved. A connection that
   closes cancels its parked claims, so the claims do not hold the connection
   task. Parked claims are capped at 65,536; one more is refused. Stopping the
   node fails parked claims as unavailable.
4. **Completed Flows are kept for a retention period, then removed.** The
   default is Elixir's terminal retention, 7 days (`--retention-ms`). Each
   shard removes expired terminal Flows at the start of a round, at most
   4,096 a round, and on an idle tick every 250 ms. It logs each removal as an
   `Expire` record listing the removed Flows, so replay removes the same Flows
   at the same log position. After removal the id can be created again.
   Terminal Flows get no other logged change, so the removal record is the
   only one replay needs.
5. **Snapshots let the log drop old segments.** A snapshot thread
   (`wf-node::supervision::snapshots`) starts a snapshot once the log has
   moved `snapshot_segments` segments (default 2, so 256 MiB) past the
   previous snapshot's base:
   1. `rotate()` returns segment N; every earlier append is in lower
      segments.
   2. Every shard is asked to capture. Between two requests, a shard encodes
      its records and logs `Marker { snapshot, shard }` in its own order. Its
      capture is then exactly its records logged before its marker.
   3. When every marker is durable, the thread decodes the whole snapshot
      again and restores every record through the domain, so an unreadable
      snapshot is never written. It then writes `snapshot-<k>` through
      `wf-storage::snapshot`: a temp file, the bytes, a SHA-256 trailer, a
      sync, a rename, and a directory sync.
   4. `remove_before(N)`, then older snapshot files are deleted.

   Opening the node loads the newest snapshot and routes its records to
   shards by partition, so the shard count may change. It then replays the
   log: a record whose owner (its shard under the snapshot's shard count) has
   not yet logged marker k is already in the snapshot and is skipped. Markers
   of other snapshots are ignored. A damaged newest snapshot, or a missing
   marker, refuses to open: the segments it covered may already be gone.
   `QueueNode::snapshot()` runs the same steps on demand.

   The data directory holds `log/` and `snapshots/`.

## Placement

| Part | Crate and module | Policy change |
|---|---|---|
| Reclaim, block and cancellation fields of the queue contract | `wf-application::commands::queue` | none |
| Shortfall rounds and blocking state machine | `wf-node::composition::claim` | none |
| Waiter thread, snapshot thread | `wf-node::supervision::{waiters, snapshots}` | covered by `wf-node::supervision` in `thread_owner_modules` |
| Snapshot encoding | `wf-node::supervision::shard::image` | none |
| Snapshot files | `wf-storage::snapshot` | added to `durable_sync_modules` |

## Consequences

- SDK workers with default options (reclaim on) work against the node.
- A snapshot briefly holds every shard's encoded records in memory, and a
  shard pauses while it encodes its own. Measured in the progress notes.
- Retained terminal Flows stay in memory until they expire. With the 7-day
  default, memory is bounded by throughput times 7 days. Spilling them to
  disk is not part of this ADR.
- Every expiry adds one log record per sweep; one record covers up to 4,096
  Flows.

## Progress

- Decision 1 done: expired-lease reclaim (ADR013-RECLAIM1).
- Decisions 2 and 3 done: shortfall rounds and blocking claims
  (ADR013-BLOCKING1). Parked claims are woken by creates whose type, state and
  priority match; a claim with a partition filter may wake for a create in
  another partition, try, and park again.
- Decision 4 done: retention of terminal Flows (ADR013-RETENTION1).
- Decision 5 done: snapshots (ADR013-SNAPSHOT1). Release build, M4 Max,
  16 shards, 1,000,000 created Flows: a snapshot took 330-390 ms end to end
  (126 MB, about 126 bytes per Flow, written and synced), and reopening the
  node from it took 0.66 s. Shards capture in parallel; each pauses only
  for its own encoding, a part of that time.
- SDK queue benchmark after all five decisions, alternating with the
  previous commit (7dc4c42) on the same busy machine (load average 8 to 10):
  one client, 100,000 Flows, five runs each: median 62,978 flows/s against
  58,182, CPU 9.9 against 9.8 ms per 1,000 Flows, RSS 185 against 162 MB
  (the removal index and leased index). Four clients, 400,000 Flows, two
  runs each: 238,674 and 233,543 against 234,955 and 236,162 flows/s, CPU
  8.8 against 8.0 ms per 1,000 Flows (shortfall rounds when clients race).
  This morning's 75,763 flows/s ran on a quieter machine and is not
  comparable.
