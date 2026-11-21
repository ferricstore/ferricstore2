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

## Consequences

The binary links mimalloc's C code, built by `cc`; cargo-deny passes.
