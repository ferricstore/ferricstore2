# Design review and decisions

## Participants and method

The coordinator writes the architecture, implementation plan and library summary. Three independent subagents inspect disjoint areas: Sartre (workflow), Parfit (Raft/storage, replacing Noether), Darwin (Rust/cache/quality). Their notes and cross-review files live in this repository. The first set of agents was lost during server restart before writing files; it is not counted as completed review. Noether's replacement-session research did not produce files before the final review; it was stopped and Parfit received a bounded Raft contract review. Only written completed reviews count.

The replacement agents lack direct send_input capability in their sessions. The coordinator relays questions through messages, and agents read and answer each other's files. This is actual file-based/relayed communication, not a claim of direct agent messaging. Initial notes correctly recorded that review was pending at first draft; subsequent review files record completed exchanges and remaining questions.

## Initial questions relayed

- Workflow to Raft/storage: group-local atomicity, cross-group coordination, authoritative snapshots/projection checkpoints, and cache eviction boundaries.
- Workflow to quality: enforce crate allowlists and no framework types in domain; isolate unavoidable unsafe dependencies.
- Quality to workflow: caches cannot authorize claims; committed inputs supply time; UI-facing transformations stay pure; API/worker contracts remain usable.
- Quality to Raft/storage: consensus owns OpenRaft bridge types while storage stays neutral; validate transactional state and snapshot fences; redb remains conditional.
- Coordinator to all: critique crate count, stale leader claim candidates, first-party unsafe policy, and conflicting provisional allowlists.

## Proposed decisions

1. Functional core with explicit commands/events; external handlers and all I/O stay outside domain and committed reducer.
2. Main architecture table is canonical. Provisional allowlists in notes must reconcile to it. Worker uses a narrow WorkflowClientPort rather than storage or consensus directly.
3. Consensus owns the OpenRaft bridge; storage implements neutral ports. Node wires adapters. No storage-to-consensus edge.
4. Initial claims select deterministically from replicated authoritative indexes during apply. Candidate-hint optimization is deferred.
5. Raft group is the transaction boundary, preserving the inspected baseline; verified colocated logical partitions may participate in bounded atomic operations. Cross-group fanout is unsupported until separately designed.
6. PackedGen is conditional, derived-cache reuse only, with clean-commit/safety/workload gates and a fallback backend.
7. First-party crates start with forbid(unsafe_code). Dependencies may use unsafe and require independent review. An unavoidable direct unsafe adapter needs a specific ADR and proof/testing plan.
8. Standard Rust linting and architecture tests are complementary. Evaluate archunit with negative fixtures; Cargo allowlists and visibility remain mandatory.
9. Exact parity, durable codec, Raft release, storage crash contract and time semantics are pre-implementation gates, not validated claims.

## Review outcome

Workflow and quality first-round reviews agreed on the functional core and raised concrete claim, dedup, sequencing and atomicity issues. The coordinator revised the architecture to group-local atomicity, authoritative apply-time claims, client-intent digests, ordered apply with publication after persistence, durable rejection outcomes, and explicit governance/effect/step scope gates. Parfit answered all saved Raft questions, then workflow and quality read those files and completed final replies. No blocking design contradiction was identified. Remaining time-advancement and recovery-watermark clarifications were integrated as explicit contracts/gates. Open implementation validation gates remain visible even after design review.

## Final exchange and scope of closure

See [workflow final replies](workflow-review.md), [quality final replies](rust-quality-review.md) and [Raft replies](raft-storage-review.md). Each reviewer read the other areas' saved questions and authored answers; the coordinator relayed final files and integrated changes. Direct messaging inside agents was unavailable. This completes the requested file-based cross-review, not unanimous production approval.

Final integration adds authorizing cache reads before lookup, worker capability restrictions, stable client watermarks distinct from projection generations, leader-admitted time inputs, and required time-entry advancement fixtures. The pinned-source coordinator read identifies existing WARaft/HLC/wall semantics; OpenRaft/monotonic time are proposed Rust migration choices. Noether produced no source findings, and none is attributed to it.

Exact parity classification, releases, storage conformance, clock bounds, schema vectors, unsafe dependency audit and workload measurements remain future implementation gates. No implementation was written.
