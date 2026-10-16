# ADR-007: pinned durable-log storage source

Status: source selection accepted; synchronous log implementation and its
recovery tests are in progress. This decision does not establish a milestone exit.

The first product storage adapter uses redb 4.4.0 from the canonical upstream
repository at commit `7061f7e60d4573311c0636e161d417b93e9fd2b1`, with default
features disabled and only `std` enabled. The commit identifies tree
`77441c23f8807fc79b313375656c4db5b8ed5b82`. The 131 source files and Git objects
were reconstructed and checked against their upstream Git identities. The
unsigned commit supplies content integrity, not publisher-signature verification.
This is an exact Git-source selection, not a crates.io archive selection.

The workspace declaration includes both `version = "=4.4.0"` and the complete
revision. Architecture policy compares the declared source, revision, package
version, features and default-feature setting exactly. Existing registry edges
retain their previous source by default. Policy parsing rejects moving Git refs,
incomplete revisions and other registries. Negative tests cover source, revision,
version, feature, path and layer drift. Dependency policy allows only this Git
repository in addition to the existing registry; unknown sources, advisory,
license and duplicate-version checks remain enforced.

The adapter belongs to `wf-storage`, behind consensus-neutral contracts in
`wf-ports`. Its synchronous operations must run on a later adapter-owned blocking
worker, with serialized vote/log I/O and durable completion before acknowledgment.
No Tokio or OpenRaft dependency is introduced by this source decision. The
concrete log namespace, recovery invariants and bounded entry/read contract require
their separately reviewed implementation and disk/fault tests. Workflow state,
snapshots, consensus integration, Linux validation and complete M2 acceptance
remain required.

Primary sources: [pinned upstream commit](https://github.com/cberner/redb/commit/7061f7e60d4573311c0636e161d417b93e9fd2b1)
and [pinned manifest](https://github.com/cberner/redb/blob/7061f7e60d4573311c0636e161d417b93e9fd2b1/Cargo.toml).
See ARCH-M2-REDB-GIT-SOURCE1-CREATE1 in the architecture review and the
coordination record for executed checks and remaining gates.
