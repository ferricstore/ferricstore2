# ADR-016: Finished Flows on disk

Status: accepted (2026-11-21). Solo decision, step 3 of the 21 Nov
architecture plan the user approved ("Do all of it as you recommended");
no independent review is claimed. Phases land one at a time.

## Context

Every Flow stays in a shard's memory, with its whole history and values,
from creation until its retention ends (7 days by default). On 21 Nov a
finished Flow cost about 1.2 KB at 400k Flows after ADR-015 (1.7 to 2 KB
before it). At 1,000 Flows/s, 7 days of finished Flows is about 600M
Flows: roughly 700 GB of memory. Snapshots encode every Flow on the shard
threads (1M Flows: 330 to 390 ms, 126 MB) and decode them again to check,
so their stalls, file size and memory grow with retained data, and so does
restart time (0.66 s per 1M Flows).

Most retained Flows are finished: they are read (GET, HISTORY, value reads,
QUERY, INFO) but change only by a rewind or their removal.

## Decision

**Phase A: finished Flows leave memory.** When a Flow becomes terminal, the
shard writes its whole image (record, fields, history: the after-image
encoding, `image::encode_one`) to a cold file and keeps only a stub: where
the image is (file, offset, length) and what the indexes need without a
read (type, state, version, update time, removal time). The records table
holds `Hot(Box<Entry>)` or `Cold(Stub)`.

- Reads of a cold Flow (GET, HISTORY, value reads by reference, QUERY rows,
  attribute reads, an idempotent create's comparison, a fencing retry of a
  terminal command) decode its image from the file. A rewind loads it back
  into memory as a hot Flow.
- Demotion happens at the end of a shard round, after every request of the
  round ran, so no command sees a Flow change form mid-round. Replay
  demotes the same way, so a restart ends with the same split.
- Cold files live in `cold/` under the data directory, are written by
  `wf_storage::cold` (append-only, one writer per shard, never reopened for
  writing after a restart), and hold records of `u32` length, CRC32 and
  image. File ids are node-wide, so a stub keeps working when a restart
  changes the shard count. Each shard writes one file per removal hour, so
  Flows that leave together share files; a file is deleted once its last
  stub is removed.
- A snapshot syncs every shard's cold files before its marker, so every
  stub it holds points at durable bytes. Bytes written after it are
  rewritten by replay, which logs no demotion of its own. After replay the
  node deletes cold files no stub names.

Phase A landed on 21 Nov: a first version wrote cold files on the shard
threads, which then spent about half their time blocked in `write`; one
cold writer thread now writes and syncs every queued image and answers,
and a Flow becomes a stub only once its image is durable. Idle after 1.6M
Flows: physical footprint 1.45 GB (was 2.1), RSS 1.9 GB (was 3.5), 822 MB
of cold files; CPU per 1k Flows +22% (8.8 to 9.0 ms against 7.2 to 7.4).

Memory, measured idle after 1.6M Flows with c2b1016: physical footprint
1.57 GB with mimalloc, 745 MB with the system allocator; with
`MIMALLOC_PURGE_DELAY=0`, 885 MB at 7 to 9% more CPU. Mimalloc keeps the
pages a burst freed until their thread allocates again (v3 waits 1 s), so
the idle figure shows the burst's peak, reused by later allocations; live
data is about 465 bytes per finished Flow (key, table slot with stub,
catalog member, removal entry). The default stays; deployments that want
memory back at once can set the variable.

**Phase B: checkpoints of the hot state.** Snapshot images hold hot Flows
and stubs, are written shard by shard without one whole-node buffer, and
are checked by CRC instead of a full decode.

Phase B after phase A: images now hold only hot Flows and small stubs, so
capture stalls, snapshot size and restart time follow the hot state. The
full decode of each image before the log behind it is removed stays: it is
cheap now, and it is what stops an undecodable snapshot from replacing the
log.

**Phase C: value versions.** A replaced named value, payload, result or
error keeps its old version in a value file while history or a rewind can
name it, so a rewind restores it and `FLOW.HISTORY values: true` reads it
(audit gaps G6, G7).

Phase C landed on 21 Nov: `FlowMeta::retained` keeps replaced and dropped
named values and replaced payloads, results and errors (with their term
flag) by version, until the Flow is removed (audit gap G37 revised);
reads by reference, rewind and `FLOW.HISTORY values: true` use them, and
they travel with the Flow's image to disk. Named value versions now rise
above every version the name has had, so a reference never names two
values. A logged after-image carries only the versions its change kept
(24 Nov, perf note P2), and replay adds them to those the Flow had;
snapshots, cold files and the portable image carry every one.

**Phase D: kept value versions of hot Flows on disk.** A running Flow
keeps every replaced version until it is removed (phase C), so a
long-lived Flow that replaces values often grew in memory without bound.

Phase D landed on 24 Nov. Once the versions a running Flow keeps in memory
(each at least 64 bytes; smaller ones cost about as much on disk) pass
`kept_memory_bytes` (16 KiB by default), the end of the round hands them
to the cold writer, in a file group of their own that rolls by size only.
Once durable, each becomes a reference (file, offset, length) that holds
its file until its last copy goes; reads by reference, history values and
rewinds read the file, and the page cache is the cache. A snapshot image
carries the versions in memory in each record and names those on disk in
a section of its own (image version 17); a logged after-image carries only
versions still in memory; a cold image and a portable image read the ones
on disk back, so a finished Flow's file holds its whole image and the
value files can go. Short Flows stay under the budget and never write a
value twice. What stays in memory per version on disk is its key and
reference (about 100 bytes). Measured: 100 Flows replacing 1 KiB payloads
200 times each grew RSS by 10 MB against 27 MB with every version in
memory, for about 3% more time per change.

Cold files and value files now share one lifetime rule: an unused file
waits until a snapshot begun after it became unused is written, since
replay from the last snapshot reads the files it names (see the review
entry of 24 Nov); a replicated member deletes it at once.

## Consequences

Memory per finished Flow becomes its key, stub, catalog member and removal
entry (estimated 200 to 300 bytes) instead of its whole state; reading a
finished Flow costs a file read (usually the page cache). A crash between a
demotion and the next snapshot loses no data: the log still holds the
change and replay writes the image again.

Not decided here: moving stubs themselves to disk (sorted per-file indexes
and filters) for retention beyond what stubs in memory allow.
