# ADR-007: pinned durable-log storage source

Status: source selection accepted; scoped synchronous log implementation and
its disk/recovery/fault tests pass on macOS. This decision does not establish a
milestone exit or Linux validation.

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

## Scoped synchronous log contract

ARCH-M2-DURABLE-LOG1-CREATE1 selects consensus-neutral vote, entry, state, limits
and error values plus a synchronous mutable log port. `RedbLogStore` owns its
database and namespace and cannot be cloned. Path constructors configure a 16 MiB
engine cache; constructors consuming an existing Database use that caller's engine
configuration and the same production initialization/recovery path. Async
scheduling and cancellation ownership belong to the later consensus bridge.
After its initial Immediate commit, path creation synchronizes the parent
directory before returning success on Linux/macOS. A bare relative filename uses
parent `.`. Failure at that point is an uncertain initialization. Database-consuming
constructors require caller-owned durable file and directory setup; opening an
existing path does not create or resynchronize its directory entry.

Creation requires an empty database and atomically installs the group/recovery
manifest, scalar state and empty log. Recovery opens existing tables without
creating missing data. It validates the complete bounded log from index zero,
exact tail and committed identities, checksums and table/metadata inventory.
Opaque entry bodies retain their original owners; consuming prepared bytes and
entry parts does not clone their payloads. Public diagnostics expose fixed error
classifications. Explicit engine corruption retains Corrupt through nested
database and transaction errors; unrelated engine failures remain Unavailable.

Append validates the complete batch and its durable overlaps before publication.
Every overlap must match full identity and body; extension follows the exact tail.
Consecutive indices have nondecreasing `(term, leader_node)` under the selected
OpenRaft default leader profile. Higher term with smaller node is valid. Supplied
order conflicts refuse without publication; stored order regression is corruption.
Votes advance lexicographically by `(term, node, committed)`. The committed prefix
can advance only to an exact stored identity, and truncation may remove only its
uncommitted suffix. Rows and scalar state commit together with Immediate
durability. Corrupt, unavailable or uncertain results fence further reads and
writes until a new handle completes recovery; admission, allocation and caller
conflict refusals do not fence the handle.

Two tables use version-1 frames: `wf_raft_meta_v1` contains only `manifest` and
`state`, while `wf_raft_log_v1` maps u64 indices to entries. Manifest framing is
`FWRM`, u16 version, u16-length group and recovery, and SHA-256. State framing is
`FWLS`, u16 version, option flags, optional vote/committed/tail and SHA-256. Entry
framing is `FWRL`, u16 version, full 24-byte identity, u64 body length, body and
SHA-256: 70 bytes plus body. State and entry checksums also bind the length-prefixed
group/recovery namespace with the `wf-raft-log-v1` prefix. Integer fields are big
endian. Parsers reject invalid flags, lengths, identity/key mismatch and trailing
bytes. No OpenRaft or serde types enter the neutral persistence contract.

Limits separately bound entry bytes, append rows/owners, returned page
rows/owners, recovery rows/wire bytes and logical workspace. Defaults admit a
16 MiB entry, 1,000-row/64 MiB append and read page, one-million-row/1 GiB recovery
scan and 256 MiB logical workspace. Limits remain configurable for the complete
encoded application profile. The neutral limits remain backend-independent.
Every redb constructor rejects an entry maximum above 3,221,225,402 bytes before
its own file/table work: the pinned engine permits a 3 GiB value and the entry
frame adds 70 bytes. An unexpected engine ValueTooLarge is an operational Limit.
The exact-boundary tests allocate no large payload. Input/output ownership
charges are 512 bytes per
row plus body lengths; namespace ownership is 512 plus group/recovery lengths,
with a separate 2,048-byte operation allowance. Append additionally charges one
largest encoded entry. These logical fees exclude engine internals and existing
caller allocations; they are not process RSS or latency guarantees.

Read pages return a bounded prefix and independently owned bodies. Admission
failure on the first row is an error, never an empty result. Range handling must
cover excluded/included endpoints and u64 exhaustion without wrapping. Normalize
to u128 half-open bounds: included/excluded starts become x/x+1, included/excluded
ends become x+1/x, and unbounded endpoints become 0/2^64. A lower bound above the
upper is InvalidRange; equal normalized bounds are empty. Purge is
absent until recovery can prove an applied/snapshot boundary. This log slice
does not supply workflow-state publication, membership recovery, snapshots,
consensus or service wiring; those M2/M3 successors remain mandatory.

## Bounded checkpoint startup join

ARCH-M2-CHECKPOINT-JOIN1-CREATE1 selects a read-only startup predecessor for the
current unpurged, no-snapshot stores. `wf-consensus::recovery` depends only on
the existing neutral ports, domain/types and the already pinned sha2 0.11.0
without default features. Exact dependency and module checks remain enforced;
consensus cannot import the concrete storage crate. Node supplies two separate
database owners. This check does not make their transactions atomic together.

The required `RaftLogStore::read_entry(index, maximum_owned_bytes)` returns at
most one existing entry owner. Readiness/fencing precedes caller-cap validation.
Caps below 512 refuse even an absent index; 512 admits an empty body. Borrowed
framing, key identity and declared length precede profile checks. The smaller
of the actual body limit and caller cap minus 512 bounds checksum work and body
copying. Existing adapter read/workspace caps still apply before ownership.
No allocating page fallback, body clone or index increment is required; the
existing page and wire contracts remain unchanged.

`verify_checkpoint` consumes both owners and returns only their pair on success.
Every failure drops both, requiring reopen before reuse. Borrowed expected scope
is checked first. A single state control read uses minimal valid admission,
returned budget 512 and explicit step/temporary caps. Its uncommitted guard ends
before log metadata or point reading. Applied control requires a durable committed
boundary at or above it, complete identity equality at equal index, and the exact
point's full log identity plus SHA-256 of its actual body matching the saved
prepared digest. The FWRL checksum is a different digest. Unapplied state and
verified applied state behind committed are valid and still require ordered
replay; successful verification neither performs replay nor admits service.

Explicit limits separate control steps, temporary C, entry owner J and workspace
W. J is at least 512; defaults are 1/2048/16,777,728/268,435,456 respectively.
Before store acquisition, checked F + max(C,J) must fit W, where
F = 2*(512 + group length + recovery length) + 2048 + 512. The two phases share
no live temporary/entry pool. Each adapter also enforces its actual captured
profile. These are logical first-party ownership/work accounts, excluding engine
pages/cache, allocator overhead and caller owners; no speed or RSS claim follows.

This applied/committed relation is scoped to startup without snapshots or purge.
Pinned OpenRaft 0.9.25 permits applied ahead of its persisted committed observation
during snapshot installation, and its default save_committed is optional/no-op.
The future bridge must explicitly await Immediate committed persistence before
ordinary apply and acknowledge only after durable state application. Atomic
effective-membership/control persistence, suffix replay, snapshot installation,
safe purge, real Raft wiring and both native operating-system gates remain required.
Implementation and execution evidence belong to the handoff and review records.
