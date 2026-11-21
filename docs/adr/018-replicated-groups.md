# ADR-018: Replicated groups

Status: accepted (2026-11-21). Solo decision, step 5 of the 21 Nov
architecture plan the user approved ("Do all of it as you recommended");
no independent review is claimed. It follows the reviewed library decision
(`docs/library-decisions.md`: OpenRaft behind `wf-consensus`, no homegrown
Raft, subject to an adapter and fault-test gate). Phases land one at a time.

## Context

The node is one process: its group log, shards and cold files live on one
disk, and losing that machine loses the data and the service. Elixir runs
each shard as a Raft group with replicas on other nodes; the Python SDK's
topology client (`SHARDS`, lanes per range, status 5 `reroute`) and its
cluster failover test (three nodes, stop a range's leader, clients reroute,
KV, Flows and queries survive) expect that: per slot range, one leader node
that serves it, and a new leader with every acknowledged write after a
failure.

ADR-011 chose one group-committed log per node for throughput: one fsync
covers every shard's round (on macOS `F_FULLFSYNC` is device-wide, so a log
per shard costs a sync each). A Raft group per shard would give that up.

## Decision

**A replica set is one Raft group over one group log.** A group holds a set
of slots, all its shards and one log. Its members (three by default) each
keep a full copy: the leader's shard threads decide every request as today
and append the round's changes as one Raft entry; followers apply committed
entries through the replay path, so their shards are a hot copy and a new
leader serves at once. A node can be a member of several groups, each with
its own log and shards, so leaders spread over nodes as Elixir's per-shard
leaders do.

- **Consensus:** OpenRaft 0.10.1 (the released line; MIT or Apache-2.0) in
  `wf-consensus`, its types private there. Storage adapters: the log store
  is the group log with each entry's log id (term, index) and vote kept
  durably, follower suffix truncation and purge before a snapshot; the state
  machine is the group's shards.
- **Commit, not sync, releases replies.** The completer waits for the
  entry's commit (a majority holds it durably) instead of the local sync.
  The leader applied its own entries when it decided them, so applying a
  committed entry there only releases replies; followers decode and replay
  it. A leader that loses leadership drops its uncommitted in-memory state:
  its shards reload from its snapshot and committed log.
- **Leader-only service.** Reads and writes go to the range's leader. A
  member that does not lead a request's slot answers status 5 `reroute`
  (retryable, safe to retry), and the client refreshes `SHARDS`. A leader
  serves reads while its leadership lease holds (OpenRaft's leader lease);
  without it, it refuses as unavailable rather than risk a stale read.
- **Slot map.** Groups and their slot ranges are cluster configuration with
  a version (`route_epoch`); `SHARDS` lists each range with its group's
  current leader. Static configuration first; moving slots between groups
  is a later decision.
- **Clocks.** Committed time stays the leader's wall clock, never earlier
  than the entry before it (`committed_now`), so a new leader whose clock
  is behind does not move time backwards for leases and due times.

## Phases

1. **Bridge and fault gate:** OpenRaft in `wf-consensus` with the group log
   as its log store, durable vote and log completions, truncation and purge;
   an in-process three-member cluster over a test network that drops, delays
   and partitions messages and crashes members, checking that committed
   entries survive and stay identical on every member.
2. **Node integration:** a replica set from configuration, a peer listener,
   commit-released replies, follower apply, `SHARDS` leaders and `reroute`.
   Landed 21 Nov: `ferricstore2 --node-id ID --member
   ID=HOST:PORT/PEER_HOST:PEER_PORT ...` runs a member; the leader's shards
   propose rounds and reply at commit, followers replay committed rounds,
   `SHARDS` names the leader with the term as route epoch, and a lost round
   stops the process (status 75) for its supervisor to restart it.
3. **Snapshots:** log compaction at snapshots and snapshot install (shard
   images and cold files) for a member that fell behind or is new.
4. **Several groups per node** and the SDK's three-node failover test.

## Consequences

A write's latency becomes a round trip to the nearest follower plus its
fsync, in parallel with the leader's own; throughput keeps group commit
because one entry carries a whole shard round. A cluster of three members
survives one member's loss with no acknowledged write lost.

Not decided here: membership changes of a running group, moving slots
between groups, and follower reads.
