# ADR-017: Backpressure

Status: accepted (2026-11-21). Solo decision, step 4 of the 21 Nov
architecture plan the user approved ("Do all of it as you recommended");
no independent review is claimed.

## Context

Nothing in the node had a size limit. A connection's reader decoded and
dispatched every frame as soon as it arrived; each shard's request channel,
the group log's write queue, the completer's queue and each connection's
reply channel grew without bound. A client that pipelines faster than the
disk syncs, or one that never reads its replies, grew the node's memory and
every other client's latency until the process failed.

## Decision

Three limits, each a wait or a refusal before any work runs:

1. **The log's write queue.** `GroupLogLimits::max_queued_bytes` (64 MiB by
   default): an appender waits while the record bytes queued for the writer
   are at the limit, and continues when the writer takes them; a writer
   failure or close wakes it with the error. A slow disk then stalls the
   shard threads, so requests wait in the shard queues, where admission sees
   them, instead of in memory behind the log.
2. **Shard admission.** Each shard counts the items sent to it and not yet
   taken (a batch counts each item; the counter is raised before the send
   and lowered when a round takes the requests). A client request is refused
   with `ServiceError::Busy` when a shard it needs is at its class's limit
   (`Admission`, per shard: 16,384 items for requests that start Flows,
   65,536 for others). Every shard of a request is checked before any part
   is sent, so a refusal never leaves part of a batch done and is always
   safe to retry. Starting Flows is refused first, so the work that finishes
   Flows keeps draining the backlog. The node's own requests (snapshots,
   policy copies, schedules, sweeps, credit releases) and admin reads are
   never refused. The binary's `--max-new-backlog` and `--max-backlog` set
   the two limits.
3. **Per connection.** `ServerInfo::in_flight` (4,096 requests and 64 MiB of
   request bodies by default): a request holds permits for itself and its
   body until its reply is written; without permits the reader waits, so the
   client is held back by TCP flow control. A body over the byte limit waits
   until nothing else on the connection is in flight.

`Busy` is Elixir's busy reply: status 4 and a map with `code` `busy`,
`retryable` and `safe_to_retry` true and `retry_after_ms` 100. The Python
SDK already retries producer writes on it with its backoff policy.

## Consequences

Memory under overload is bounded by the limits: per shard, the backlog
limit's requests; for the log, one queue of `max_queued_bytes` plus the
batch the writer holds; per connection, its in-flight permits. Clients see
`busy` instead of growing latency, and creates are shed before claims and
completions. The counters cost one atomic add per request and one atomic
subtract per shard round.

A connection whose in-flight requests are all parked blocking claims reads
nothing more until one of them returns (at its `block_ms`); a client that
parks thousands of claims on one connection should use more connections.

Not decided here: limits from measured drain rates instead of fixed item
counts, and a node-wide memory budget.
