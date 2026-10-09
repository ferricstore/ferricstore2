# Source provenance and limits

Assessment date: 2026-10-09. This repository is a design artifact, not a copy or branch of the Elixir codebase.

| Source | Revision | Evidence policy |
|---|---|---|
| ferricstore/ferricstore | 66586c596e271408d9184af32eb31cd7218c126e | Workflow/consensus baseline pinned in peer notes; dirty files identified separately |
| local packedgen-rs | 26d1d933619951de93f432fe8e19f1f3f5637abd | Cache assessment examines modified working tree; do not adopt these findings as a clean released artifact |
| ferricstore/ferricstore-python | 368bef8a7220716d4154f30d94c1cbdb743843cd | Supplementary durable-context/worker contract evidence, not server proof |

The original `/Users/yoavgea/repos/ferricstore` and existing Elixir `/Users/yoavgea/repos/ferricstore2` were not modified by this task. The new design checkout is `/Users/yoavgea/repos/ferricstore2-rust-design`, remote `ferricstore/ferricstore2`.

Rust recommendations use primary library documentation linked in the notes. Moving latest-version URLs are not dependency pins. Source tests establish particular contracts; this design does not claim exhaustive behavioral equivalence or test execution. No Rust implementation, benchmark, lint execution against a Rust workspace, storage conformance test or fault test was performed. Future gates explicitly identify that work.

The subagent files record independent analysis and actual file-based/parent-relayed reviews. First-round questions and later responses remain distinguishable; they are not fabricated direct exchanges.

## Coordinator's targeted Raft baseline read

Read with `git show` at the pinned FerricStore commit:

- `apps/ferricstore/lib/ferricstore/raft/backend.ex` fixes the selected production backend to WARaft and delegates writes to WARaftBackend. Do not mistake historical Ra-shaped API names/documentation for the selected production backend.
- `raft/command_clock.ex` delegates stamping to CommandStamp and describes a leader-side HLC plus wall snapshot committed with commands; wrappers route to WARaft and report applied positions.
- `raft/state_machine.ex` documents one independent Raft group per shard, deterministic apply on every replica, Bitcask/ETS state application, replicated clock metadata and checkpoint/release-cursor effects. These module-doc contracts are design intent, not independently measured durability or a complete audit of every apply section.

The Rust proposal migrates semantics to an OpenRaft adapter; it does not claim WARaft wire, membership, log or on-disk compatibility. OpenRaft release/backend/clock/snapshot gates must be validated independently. Replicated logical time in the proposal is a design choice to specify and compare with legacy HLC/wall-time semantics, not a claim that the legacy code already implements that exact policy.

## Existing workflow test baseline follow-up

Preserved 480 committed source/support/golden-fixture files selected by workflow filenames, Flow command/module/tag markers, static section-file closure and shared test support. The navigational manifest indexes 5,858 textual declarations, not a runtime ExUnit count; broad mixed suites require case-level scope review. Nine working-tree test changes are separately captured as uncommitted review inputs.

Checked each baseline snapshot against its exact pinned Git blob identity and SHA-256 manifest hash; checked unique declaration IDs and working-tree snapshot hashes. This verifies source preservation, not tests passing. No Elixir or Rust suite was executed. Runtime generated-case collection and Rust counterpart/assertion mapping remain mandatory before parity can be declared.
