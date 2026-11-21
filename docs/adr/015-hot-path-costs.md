# ADR-015: Hot-path costs (step 1 of the 21 Nov architecture plan)

Status: accepted (2026-11-21). Solo decisions on the user's request ("Do all
of it as you recommended"); no independent review is claimed.

## Context

On 21 Nov the SDK queue benchmark (`bench_driver.py`, product arm, 400k
Flows, 4 clients, 16 shards, this M4 Max under other load) showed the node
at 12.6 to 13.8 ms of CPU per 1k Flows and 690 to 820 MB RSS, against 9.3
to 9.5 ms and about 595 MB for 7dc4c42 in alternating runs: ADR-014's
history events and catalog cost about 40% CPU. A `sample` profile of a
1.6M-Flow run put the node's busy time in the system allocator (35.5%),
memory copies (13.6%), the device flush (9.9%), SipHash (7.2%), B-tree
updates (5.3%), socket writes (3.9%), history packing (3.1%) and SHA-256
block checksums (2.0%).

The user also asked that the node keep Elixir's features without its
legacy behaviour, choosing the faster design where they differ.

## Decision

Each change below is measured alone with three alternating runs against
the commit before it (`ab.sh` in the parity rig: same driver, same machine,
fresh data directory per run). Numbers are CPU ms per 1k Flows, aggregate
Flows/s and RSS after the run.

1. **mimalloc as the binary's global allocator** (`mimalloc` 0.1.52, MIT,
   no default features, `wf-node` only, declared in `src/main.rs`; tests
   keep the system allocator). Against a0cff0a: 9.0 to 9.4 ms (was 13.4 to
   14.0), 282k to 294k Flows/s (was 265k to 279k), RSS 774 to 788 MB (was
   688 to 790).

2. **foldhash for the shard's hash maps** (`foldhash` 0.2.0, Zlib, `std`
   only; `fast::RandomState`, randomly seeded per map, so a client choosing
   ids cannot predict collisions). Records, queues, shared values, tags,
   policies, FIFO lanes and the catalog use `state::Map`. Zlib joins the
   allowed licences. Against 83cb0b4: 8.3 to 8.6 ms (was 8.9 to 9.3), RSS
   750 to 778 MB (was 770 to 806).

3. **CRC32 group log blocks, encoded in one copy** (block format 2:
   24-byte header with the CRC32 of the header fields and payload, from
   `crc32fast` 1.5.5, MIT or Apache-2.0, hardware-accelerated). Version 1
   sealed each block with SHA-256 and copied records twice; the checksum
   only has to catch torn and damaged writes. A header of another version
   is refused at open (this writer's torn tail always carries its own
   version), so an old log is never cut to nothing; logs of format 1 must
   be discarded (benchmark data only). Against def0fc1: 8.1 to 8.3 ms (was
   8.4 to 8.8), RSS 743 to 759 MB (was 779 to 821).

4. **No deep copy of a Flow's fields per write, fewer catalog
   allocations.** Within noise on the queue path (its Flows have no fields):
   7.9 to 8.3 ms against 8.1 to 8.5.

5. **The node's clock decides, not the client's.** A client's `now_ms`
   moved due times and leases: on 21 Nov a client sending `now_ms` two days
   ahead took a Flow scheduled for the next day and another worker's live
   10-minute lease, on Elixir 6a5f2bc34 and on this node alike. The TCP
   adapter now drops `now_ms` (top level, batch items, children; text
   `NOW` too) before the node reads a command, unless the server runs with
   `--client-clock true` (`ServerInfo::client_clock`), which tests and
   parity probes use. Node-level tests keep passing explicit times through
   the node API, which is not a client boundary.

6. **Hardware CRC32 for routing** (`crc32fast` in `wf-application`):
   auto buckets and slots were computed bit by bit (2.3% of busy CPU after
   the steps above). Same IEEE CRC32, so every bucket and slot is
   unchanged (the routing tests pin Elixir's values). Against be04f39: 7.6
   to 7.9 ms (was 8.0 to 8.1).

7. **Boxed shard entries.** A Flow's entry (about 370 bytes) sat inline in
   the records table: every change moved it out and back, every resize
   moved all of them, and a table after resize holds about as many empty
   entry-sized slots as full ones. Entries are now boxed and keep their box
   across changes. Against bc7ac2c: 7.3 to 7.5 ms (was 7.6 to 7.7), RSS
   547 to 569 MB (was 711 to 771); at 1.6M Flows 10.1 ms and 3.7 GB (was
   10.7 ms and 4.8 GB).

## Consequences

The binary links mimalloc's C code, built by `cc`; cargo-deny passes. At
1.6M Flows mimalloc holds 7 to 10% more memory (4.4 GB against 4.0 to 4.2
GB) for about 25% less CPU (12.0 against 15.5 to 16.1 ms per 1k Flows);
memory per Flow is the subject of the storage redesign.

The SDK integration suite and the parity rig must start the node with
`--client-clock true` where they depend on client times (the SDK's
reclaim test claims 30 s ahead).
