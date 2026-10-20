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

## Applied membership and neutral control entries

ARCH-M2-RAFT-CONTROL1-CREATE1 selects the following bounded synchronous successor.
Its tests produced missing-contract RED before implementation and now pass with
the complete macOS workspace/native checks recorded in the handoff. Last-applied
membership is distinct from a Raft owner's effective, possibly uncommitted
configuration. Linux and the full product acceptance gates remain open.

Pure ports own `RaftMembership`, `AppliedRaftMembership`, their limits/error and
the borrowed `GroupControlChanges`. Configurations preserve outer order,
duplicates and more than two joint sets. Each nonempty inner voter set and the
node directory are strictly sorted and unique; every voter has a node, and extra
nodes are learners. Zero/MAX node IDs and empty, shared, NUL or multibyte UTF-8
addresses are valid. Validated construction moves input owners without cloning.
Only initial applied state has empty membership; it cannot encode as a command.

`FWBL` and `FWMB` bodies contain magic, u16 version1, u16-length group/recovery
and an explicit stamp flag. A stamped body adds physical u64, logical u16 and
wall u64; no receiver clock supplies this metadata. Blank ends there. Membership
adds u32 configuration count, each u32 voter count and u64 voters, u32 node count,
all sorted u64 node IDs, then parallel u32-length UTF-8 addresses. Integers are big
endian and trailing bytes are invalid. Existing Normal `FWCP` bodies stay exact.
Prepared identity is SHA-256 of the entire actual entry body.

Borrowed validation completes scope, shape, sorting, voter lookup and EOF before
any reserve or typed decode. Voter lookup uses binary search over contiguous
fixed-size node IDs. Apply and strict recovery need no typed membership owner or
lookup map. Let k be configuration count, v total voter occurrences, n node count,
a UTF-8 address bytes and s the stamp flag. Logical ownership and wire sizes are:

```text
U        = 1024 + 512*k + 8*v + 512*n + a
E_blank  = 11 + group_bytes + recovery_bytes + 18*s
E_member = 19 + group_bytes + recovery_bytes + 18*s + 4*k + 8*v + 12*n + a
J        = 512 + E
```

Initial membership has U1024. Producers reserve workspace2048+E for Blank and
2048+U+E for Membership; typed body decoding reserves2048+J+U. Default limits are
64 configurations, 1,000,000 voter occurrences, 65,536 nodes, 1,048,576 bytes per
address, 16,777,216 body bytes and 67,108,864 owned bytes. Every cap is positive;
small positive caps remain valid profiles that can refuse a particular operation.

`GroupControlTransaction` extends the existing actual apply guard. Its immutable
`owner_limits()` returns configured cumulative R/C even when fenced; data reads
and consumed `commit_control` retain normal fatal-error fencing. Same-guard
`applied_membership` returns initial None or the applied full identity, original
stamp and owned membership. Query limits cannot exceed the captured profile;
healthy data above a valid smaller profile produces Limit. Commit revalidates
against its captured profile even when changes were built under a larger one.

`wf-consensus::apply::apply_control_entry` reads these actual budgets before
reads/hash, borrows the body, computes the unchanged control plan from its explicit
stamp and consumes one Immediate commit. Its checked workspace is3072+J+R+C;
the namespace allowance conservatively supports both maximum256-byte IDs.
`read_applied_state` reads control and membership through that same guard with
workspace3072+R+C. Returned control512 and membershipU share R. Encoded output
is charged once inside C; neither function adds an imaginary typed apply owner.
Zero workspace is InvalidLimits; arithmetic overflow or insufficient workspace
is Limit. No supplied argument certifies consensus commitment: the later Raft
owner must persist committed before ordinary application.

The state layout advances to `FWSS2`, preserving its two table names, ordinary
ROWS0..7/key formats and `FWSC1` values. META requires exactly `manifest` and
`membership`. The manifest retains its108+group+recovery shape and hash recipe,
with version2 and a new frame checksum. Atomic genesis installs explicit None
membership with the original cohort. An intact legacy `FWSS1` returns
UnsupportedLayout without migration or mutation.

Required `FWMV1` membership values bind u16-length group/recovery and flag0/1.
None adds only a SHA-256 checksum. Some adds the full24-byte log identity, u64
body length, original `FWMB` body and checksum over the complete preceding frame.
Thus O_none=43+group+recovery and O_some=75+group+recovery+E. The inner body's SHA
is the applied prepared digest, separately from wrapper and log-frame checksums.
The wire cap applies to the inner command; None carries no command body. Before
file/table creation, the captured profile must admit initial U1024 and actual
O_none in C and the engine output limit.

Backend publication compares actual namespace and predecessor before mutation,
fully validates the old borrowed membership, and preserves all other rows.
Replay aborts without changes; Blank publishes only control; Membership publishes
control and the exact original membership body in one Immediate writer. A failed
commit is Uncertain and fences the store; a fatal data read fences its live guard.
Strict open counts the required metadata in recovery rows/bytes/steps. Unapplied
control requires None. Applied membership cannot be ahead of control; equal index
requires full identity, actual body SHA and original stamp equality. Current HLC
and wall must both cover a known membership stamp even when membership is older.

At the applied-membership checkpoint, historical joining and ordered suffix replay
remained successors; the section below records their concrete implementation.
Snapshots/install/purge, real Raft integration, full source parity and native
Linux/macOS acceptance remain required. The earlier checkpoint verifier is not
silently expanded. Logical ownership formulas establish no native RSS or latency
improvement.

## Selected committed startup recovery

ARCH-M2-COMMITTED-RECOVERY1-CREATE1 selects `wf-node::recovery::recover_committed`
over the actual consumed RedbLogStore/RedbStateStore. Private join/replay children
own orchestration only; the existing control and V5/V6/public1 transaction drivers
retain validation, domain decisions and atomic publication. The earlier neutral
checkpoint verifier stays unchanged. Twelve test identities pass after actual
preimplementation missing-contract RED. Final macOS validation passes 745 Rust
tests/89 targets plus six compile-fail examples/15 doc targets, strict quality
checks and the separate unchanged native executable; exact evidence is in the
handoff. Full source parity, Linux and later milestone gates remain open.

`CommittedRecoveryLimits` requires an explicit existing Normal profile, including
its deterministic original outcome TTL, and has no Default. It captures membership
ceilings, per-guard steps/observation-R/control-R/C, cumulative reached-point/body
bytes, per-entry owner J and global workspace W. Positive small profiles may refuse
operations; construction rejects zero fields or J<512. Existing errors retain
their categories through the fixed `CommittedRecoveryError` variants.

Observe A/control and last-applied M under the SAME actual state guard. Extract
only Copy metadata, then drop typed membership and guard before log access. Freeze
the real durable committed C. With no snapshot/purge, A requires C and A<=C, with
full identity equality when their indexes match. Audit every actual point0..A
before any replay write. Fully parse control bodies; supported historical Normal
bodies also use their existing complete prepare/intent/EOF verification. The new
borrowed header helper supplies only profile, scope and original stamp facts.

At A require complete log identity, SHA-256(actual body) and original stamp. A.time
must dominate EVERY historical stamped HLC and wall. Derive the LAST actual FWMB
and compare complete identity/digest/stamp to saved M, including None. Checking
only the saved M point cannot prove this last-membership invariant.

Replay only consecutive points(A,C] through actual existing drivers. Retained
business rejection/replay, request conflict and expiry are successful processing;
their returned control must match the existing plan_control result. Drop the
complete reply, expected scope and point before the next read. Final SAME-guard
control/M observation and unchanged durable C must match progress. Unsupported
policy/Claim/other Normal profiles refuse instead of being skipped; their joined
durable routes remain required. Never inspect, apply or truncate uncommitted rows.

All errors drop BOTH owners, including budget refusal. A replay prefix may already
be durable: explicit strict reopen resumes from actual control, with no invented
cross-entry rollback. Return establishes this bounded startup contract, not Raft
commit provenance, effective/uncommitted membership or service readiness.

Let g/r/t be actual namespace/tenant byte lengths, E body bytes, J=512+E,
G=512+g+r and B=2*G+2048+512. The last512 pays retained scalar metadata. Before
observation require B+configured observationR+C<=W. Before a point copy require
B+actual bounded point cap<=W. Full-prefix and suffix share one point/body account;
preflight stop=u128(C.index)+1 against the point cap. Before each point pass
512+min(configuredJ-512,remaining_body_bytes) to the existing bounded read, then
debit actual E after success. There is no wrapping u64 MAX+1 or suffix refund.

Historical Normal uses B+512+D, where its unchanged decoder's
D=4*E+codecB+1024*n+2048 already pays E and decoded scope. Normal replay pays
Q=512+t+g+r BEFORE fallible expected-scope copies, requiring
B+max(actualJ+Q,512+Q+normalW)<=W. Both terms matter when a valid tiny Normal W
is below E. Existing driver W already pays encoded E and reserves cumulative
R+stagingS+C. Control replay instead admits B+actualJ+configured controlR+C;
its existing applier independently checks actual guard budgets. Output O stays
inside C once. All arithmetic is checked; engine pages/cache and native allocator
capacity remain outside these logical fees.

The exact existing no-default SHA pin is a node normal edge. The canonical pinned
redb no-default/std edge is node DEV-only for test observations/corruption, with
negative checks against source/feature/version drift and production leakage.
Narrow purity checks cover `log::common` and descendants while similarly named
effectful neighbors stay distinct. No new engine, registry package or consensus
storage edge is introduced. Snapshots/install/purge, full original setup/profile
parity, runtime integration and independent native Linux/macOS acceptance remain
required; these equations establish no measured latency/RSS gain.

## Selected complete state snapshots

ARCH-M2-STATE-SNAPSHOT1-CREATE1 selects the implemented state snapshot predecessor.
Fifteen substantive identities pass after genuine missing-contract RED, including
actual storage faults and four capture/install process-kill paths. The final macOS
workspace passes 760 Rust tests plus six compile-fail examples; exact scope and
remaining gates are recorded in the handoff. Architecture and performance reviewed
actual source/tests and each other. Setup/layout fixture corrections and equivalent
strict spellings preserve old oracles; they are not product behavioral RED.
Exact artifacts, signatures, defaults, wire and four test-helper precisions are
recorded in the active completion coordination/domain/architecture reviews.

The neutral `StateSnapshotStore` owns capture, current-artifact export and live
installation with an associated private Stage. `SnapshotLimits` separates artifact
rows/encoded bytes/individual row bytes from whole-call visits/bytes/steps, I/O
attempts/quantum, metadata R, temporary C and workspace W. `SnapshotMetadata` owns
complete original control and applied membership; it certifies no Raft commitment
or installation authorization. Storage's non-cloneable `RedbSnapshotStage` consumes
a genuinely empty database and validates the actual incoming stream before it can
be installed or moved into a fresh restored store. No raw database/apply handle
escapes. The path receive form exclusively creates a new file and synchronizes its
parent; opaque-database callers retain those setup obligations.

Select explicit FWSS3 with the existing live META/ROWS and two queryable immutable
snapshot META/ROWS tables. Live metadata requires manifest, membership and an
explicit None/Some snapshot descriptor. None requires empty frozen tables. Some
contains exact original manifest/membership and all captured authority families
0..7, including clocks, fences, leases, inline values, outcomes, history, policies,
placements and indexes. Snapshot-local tables/descriptor are excluded from capture.
Intact FWSS1/2 is fully checked under its old inventory and refused as
UnsupportedLayout; there is no implicit migration or silent FWSS2 widening.

FWSP1 has independent group/recovery lengths, metadata count2, row count and total
encoded length, then sorted metadata and sorted unique authority rows. Each row
places u32 key length and u64 value length before either payload. A final SHA-256
covers preceding bytes and genuine EOF is required. The artifact ID hashes the
complete stream including that footer. FWSD1 binds that ID/count/length with a
namespace-bound descriptor checksum. Canonical emission from frozen indexed rows
must reproduce the original DATA after later ordinary applies; it never recaptures
newer live rows under older metadata. Strict reopen validates both cohorts and the
complete retained artifact using indexed semantic probes, without a hidden scratch
path or repeated whole-stream scans for each point.

Capture holds the actual exclusive synchronous state writer through old-artifact
census, replacement, validation and complete caller output. Output error aborts
the uncommitted replacement; final Immediate failure is uncertain and fences even
if all bytes reached the sink. Accepted output is not caller flush/fsync. Live
install retains exclusive validated staging custody while checking actual target
profiles, exact immutable genesis, nondecreasing applied index/HLC/wall/membership
and exact control/membership at equal indexes. One Immediate transaction replaces
live authority and its recoverable artifact together. Stage-source failures abort
without fencing healthy active authority; actual active engine failures keep their
fatal classification. Fresh restore moves the already checked Stage directly.

For every retained namespace G=512+g+r. Before work require
sum(Gs)+2048+configuredR+configuredC<=W; installation pays both namespaces.
Metadata R_actual=G+512+U(M), with complete typed membership U under its existing
metric. Semantic helpers use the actual min(snapshot C,target C), sharing cumulative
temporary, visit and schema accounts without phase refunds. Old-artifact retirement
uses whole-call work and its captured data profile, not the smaller new artifact's
row/length caps. Receive pays the full512+q at each new logical row-buffer high
water; smaller rows reuse it. Every actual I/O attempt is admitted before invocation,
including short calls and Interrupted. Quantum1 costs E output and E+1 input
attempts, including actual EOF. Engine caches/pages/COW/disk, caller buffering and
blocking, and allocator excess are separate from these logical bounds.

The original Elixir concurrent capture requires an acknowledged barrier and a real
queued late write; serialized component custody alone does not complete that
runtime case. Offline interrupted replacement with an older source remains a
separate recovery/new-lineage obligation, not permission for serving-state rollback.
An actual coordinator must establish capture/install commitment provenance before
destructive purge. The following successor now provides local snapshot provenance,
durable last-purged identity, anchor crash ordering and snapshot-aware startup
audit(S,A]/replay(A,C]. Safe repeated anchor advancement and received snapshots
beyond the local log tail remain required. Full original setup/profile parity
and separate Linux/macOS execution remain required.

## Selected protected local purge and snapshot-base recovery

ARCH-M2-PROTECTED-PURGE1-CREATE1 and PRECISION1 implement this successor. Thirty
substantive test identities pass after independent architecture and performance
review and actual preimplementation missing-contract RED. The complete macOS
checkpoint passes 791 Rust tests across 90 targets and six compile-fail examples
across 15 documentation targets, with strict quality checks and the unchanged
separate native executable. PRECISION2 separately audits only three exact test
helper imports, with behavioral checker RED/GREEN and all 39 checker tests passing.

The existing log state gains a paired full purged identity P and artifact ID. Its
old constructor remains unanchored; the new scalar constructor supplies no storage
certificate. Logical last-log identity is the physical tail, or P when empty.
Every mutator preserves this pair, append refuses resurrection at or below P, and
truncate cannot reach P or committed C. Strict reopen proves P<=C, exact identities
at equal indexes, first retained P+1, continuity and the exact retained C/tail.
FWLS1 remains exact when unanchored. FWLS2 requires the anchor plus C and tail,
flags14/15, P24 and ID32 before the existing namespace-bound checksum. FWRM1,
FWRL1, the two-table inventory and physical-only reads remain unchanged.

The trusted low-level `purge_through` capability deletes an inclusive prefix in
one Immediate writer. It requires a nonregressing boundary at or below C and the
exact local boundary; equal P/ID replay is idempotent. One full-wire census checks
the affected prefix and the first retained neighbor before decode/hash, then
guards drop before bounded range deletion and atomic metadata publication. It
does not inspect the separate state database or establish commitment itself.

The state-local pin validates the complete actual immutable artifact before
persisting only its descriptor. FWSD2 is pinned-Some only, with the same 87-byte
layout and checksum domain; all unpinned FWSD1 bytes and FWSP1 DATA remain exact.
The artifact stream excludes the descriptor, so pinning preserves its ID. Scalar
pin/probe uses borrowed validation without a returned Metadata/membership owner;
recovery control-read tariffs remain cumulative C, while typed Metadata still
requires its complete R. Capture/install refuse a validated pin before replacement
or output. Ordinary applies and original-artifact export remain available.

Node consumes both real owners in `purge_snapshot` and `recover_snapshotted`.
Every error drops both; neither a caller-created Metadata value nor a bare pin is
provenance. A current artifact observation supplies only scalar S control,
membership identity/stamp and ID after the full Metadata owner drops. Local purge
requires applied S<=A<=C, exact equal-index identities and independent clock dominance.
With no P, audit 0..S against S's original identity, actual body SHA, stamp, scope,
historical clocks and last membership, including None. Audit (S,A] against A and
its actual membership. Only after this full proof and a fixed-C recheck does the
coordinator durably pin state, then invoke the real log purge. A pin failure leaves
logs intact; uncertain log publication leaves the artifact durably pinned and
only the complete old or complete new log cohort eligible for reconciliation.

With P present, startup requires the exact pinned artifact S=P/ID and seeds the
historical control/membership from its actual frozen data. It audits only (S,A],
then replays (A,C] through the unchanged control and Normal V5/V6/public1 drivers.
Final coherent control/membership and fixed C must agree. Later refusal preserves
an already durable replay prefix for fresh reopen/resume. Required absent DATA
is MissingSnapshot, including with P present; a present but unpinned or mismatched
artifact is AnchorMismatch. Without P or DATA, startup uses the old recovery path.
An initial unapplied snapshot has empty prehistory and cannot authorize purge.
Both old no-purge entry points explicitly refuse P even if A remains retained.

Before each of at most three separate snapshot-method phases, outer B=2G+2560
admits configured snapshot R+C in W as well as the method's own bound. Each phase
retains its actual cumulative method account, so the aggregate is at most three
caps. Metadata drops before other owners. One whole-call log point/body pool
covers audit and replay, with prospective 512+min(J-512,remaining-body) admission.
Recovery counts are C+1 without P or C-P with P, using u128; P=C=MAX requires zero
points. Local purge analogously admits its audit only through A.
Engine range deletion, freed-page vectors/COW/cache, physical RSS and caller
buffering remain separate; these bounds establish no measured speed improvement.

This local fixed-pin predecessor does not complete repeated capture/install/purge
or authorize unpinning. Safe anchor advancement and received-snapshot provenance
past the local log tail remain required before the complete Raft storage contract.
Queued runtime capture, offline recovery/lineage, all original source/setup/profile
cases, real consensus/services and separate native Linux/macOS evidence remain
mandatory. No new dependency, storage-to-consensus edge, second authority owner
or service readiness certificate is selected.

## Selected trusted received log boundary

ARCH-M2-SNAPSHOT-ADVANCE1-LOG-CREATE1 adds the neutral
`RaftLogStore::install_snapshot_boundary(expected, position, prepared, snapshot_id)`
and its concrete redb implementation. The server must first establish accepted
received-snapshot authority and durable state-artifact custody. The two-store
catalog/coordinator and actual accepted Raft callback remain required successors.
This opaque log primitive carries those caller preconditions.

One writer loads actual metadata. Exact saved full P/artifact-ID retry succeeds
before comparing the old expected state. Otherwise the complete expected vote,
C, tail and P/ID must match; S must exceed protected C/P and preserve their leader
order. Matching full local S requires SHA-256 of its actual body to equal the
supplied prepared digest. Its later suffix remains intact. A differing full S or
S beyond the tail discards the covered old physical branch. Every successful
publication preserves vote and atomically stores C=P=S, artifact ID, logical tail
and the appropriate deletion in the same Immediate transaction. A logical
boundary has no fabricated physical row, including at MAX.

One borrowed forward scan charges each reached full stored wire before shared
decode/checksum/body hashing. Matching S reaches the inclusive prefix plus one
retained neighbor; conflict/beyond-tail reaches all old rows. Continuity and the
actual committed witness are checked before mutation. Guards end before ranged
deletion. No application key/entry vector or unrelated matching-branch suffix
scan is introduced. Healthy Conflict/Limit leaves the owner usable; actual fatal
or uncertain engine failures retain the existing fence/reopen rules. Native
engine COW/cache/reclamation stays outside logical owner bounds.

Eleven new identities pass after actual preimplementation missing-method RED,
alongside seventeen existing log identities. They cover exact metadata/body
refusals, independent full cohorts, exact/tight prefix-versus-full census, real
read/sync faults and six acknowledged process kills. Strict storage checks pass.
Full regression passes 802 Rust tests across 90 targets and six compile-fail
examples across 15 documentation targets, with workspace strict checks,
warnings-denied rustdoc and native Darwin acceptance. These component results
establish no received state authority, callback readiness, Linux or source-case
parity. Safe repeated state/log advancement remains required.


## Selected repeatable snapshot custody and received composition

ARCH-M2-SNAPSHOT-ADVANCE1-CREATE1 and its recorded precision decisions extend the
existing snapshot/log contract on the same exclusive owners. The new neutral
StateSnapshotTransitionStore capability extends StateSnapshotStore and retains
its Stage type. SnapshotTransition is a Copy mathematical precondition containing
full prior RaftLogState, target LogIdentity, immutable DATA ID and local/received
origin. Its checked constructors establish ordering, not consensus authority.
The only private catalog model contains two optional scalar artifact facts,
current/anchor selectors and an optional transition. No second backend, authority
certificate, generic transaction layer or new dependency is introduced.

Create and receive default to FWSS3/four tables with exact legacy DATA, fixed pin
and budget behavior; strict open preserves the selected persisted layout.
A consuming explicit upgrade validates that
owner, creates only the two alternate frozen tables and atomically publishes LIVE
FWSS4 plus the catalog. Strict open never manufactures or silently upgrades a
layout. LIVE FWSS4 requires all six exact tables; frozen DATA stays FWSS3 and the
FWSP1 wire/ID remains byte-identical. Manifest normalization changes only version
and checksum and admits the actual output length before copying.

The scope-bound FWSD3 catalog is 43..318 bytes with exact EOF. Its prefix is
`FWSD 00 03`, then current/anchor selectors (0 absent,1 slot0,2 slot1). Each slot
has tag0/1 and, when present, rows:u64, encoded_bytes:u64 and ID:32. Pending tag1
adds origin0(local)/1(received), prior-metadata flags vote1/C2/tail4/P8, each
present original field, target identity:24 and ID:32. Prior vote is17 bytes;
C/tail/P identities are24 each and present P also carries its original32-byte ID.
The final SHA-256 covers `wf-state-snapshot-catalog-v1`, u16-length-prefixed group
and recovery bytes, and the full prefix. Unknown tags/flags/selectors, checksum or
EOF disagreement, duplicate IDs and populated unreferenced slots are Corrupt.
Every referenced slot must also pass complete canonical and semantic validation.

Capture/install first reuse an existing identical target; an unrelated old anchor
stays intact. Without an anchor, replacement reuses current (including slot1);
with an anchor it uses the other slot. Every requested NEW DATA cap applies even
to an exact pending retry or reused target. Unrelated OLD DATA and cleanup use
captured supported bounds. Preparation persists intent while retaining both
artifacts. Exact prior log metadata permits forward publication; exact acknowledged
new metadata permits finish. Any other metadata relation refuses. Finish retires
only non-target DATA and clears intent in one Immediate transaction after actual
log acknowledgement. Restart handles both old-log and new-log windows; consumed
failures release neither a service-ready owner nor a provisional acknowledgement.

The local coordinator proves the exact retained original P artifact and actual
pin. With PSome a present current candidate must be applied with S>=P; older or
unapplied candidates are AnchorMismatch. Equal S=P requires complete ControlState
and original artifact ID equality before any clock diagnosis. Higher S must
preserve old P's independent HLC and wall floors, including when post-P bodies
are Unstamped. It audits candidate S before later LIVE A using one cumulative
point/body pool, then performs optional ordered committed replay. With PNone,
an existing state-only pin grants retention only; the actual audit starts0.
Absent required DATA remains MissingSnapshot. Unpurged unapplied local purge
retains its earlier UnappliedSnapshot error.

Trusted received installation checks caller and Stage namespaces, actual old
metadata and LIVE<=old C, plus matching physical boundary identity/body/stamp when
available. It atomically publishes received LIVE, canonical DATA and received
intent, then uses the existing trusted log-boundary primitive and exact finish.
Fresh restore requires actual PNone, moves Stage into state, explicitly upgrades,
persists intent and switches the log; it does not heal a missing old P artifact.
The caller still must bind original DATA to an actual accepted Raft decision.
The accepted callback/runtime, queued capture barrier, strong reads, real peers
and separate offline/new-lineage restore remain successors.

The ready-first scalar capability performs no database work or allocation and
preserves legacy B=3588 and W=1,054,214 for g/z. Mode4 adds fixed512 context,
Bnext=4100. Typed Metadata reserves G+512+U; G=512+group_len+recovery_len and
U(None)=1024, so R=2050 for g/z. Scalar upgrade/observe/prepare/finish support R1
without materializing membership. Storage method W covers G+2048+configured R+
operation C, plus a held Stage's actual G when applicable. Effective temporary C
is min(operation C, actual active-target C), never the Stage's historical C.
Method-local row/full-wire/step/C/schema accounts remain continuous across all
validation, source/target copies and retirement; carried watermarks precede each
side's resumed work. A single schema charge spans the method.

Node outer exact W=Bnext+R+C is1,054,726 for g/z,R2050,C1MiB. Live receive owns
log+state+Stage, adding G514 for exact1,055,240. Fresh restore moves Stage and owns
two namespaces for exact1,054,726. Metadata drops before log points or another
Metadata observation. At most six snapshot method phases use their own complete
method budgets; fresh uses four. Point/body work is shared across all coordinator
audit/replay substeps, including MAX/equal-boundary and tiny Normal-driver cases.
These are logical admission bounds, not RSS, engine cache/COW/disk or speed claims.
Consumed Database cleanup may sync/read after refusal. The resource test calibrates
an actual Stage.metadata fault and requires the earlier outer Limit; a post-drop
fault flag cannot prove absence of engine cleanup I/O.

Thirty original selected tests received genuine missing-contract RED before the
production release. Later real legacy, mirrored-slot and retained-P behavioral
failures led to narrow repairs; post-fix pending-cap controls and the corrected
cleanup witness keep their actual chronology. Focused storage49/node36, strict
checks and separate native Darwin execution pass. Full workspace, both-host,
source setup/profile parity, consensus/services and milestone exits require
separate evidence in the handoff; these facts do not establish full completion.


## Selected durable physical Claim leaf

The selected internal op3/intent1 leaf joins the existing trusted original-parent
digest, private request codec, one retained point gate and an actual exclusive
redb writer. ARCH-M1M2-DURABLE-CLAIM-LEAF1-CREATE1, SOURCE-PRECISION2 and the
recorded test-only Cohort alias decision enumerate the exact artifacts; no public
parent, policy-edit authority, governance fallback, new dependency or layout is
inferred. The thirteen production children preserve the functional core and
neutral storage/node boundaries.

Fresh authority is loaded only after the full first query preflight. Its sealed
rejection pairs only with None; valid preflight pairs only with a real observed
window. The reducer independently repeats complete query admission, so valid
work pays g1+g2. Missing observations differ from observed absence. Complete
reached placement/current/index/head relations must agree, including an
unselected future head's exact Some member and explicit empty-lane placement.
Unsupported token/resume is an outer authority failure before Fresh business
admission and after the retained gate. Present governance, even empty, is an
explicit unsupported route in this first leaf.

Due selection is query-scoped and due<=now. Physical foreign, future and filtered
visits pay operational accounts without consuming semantic B/H. Distinct true
FIFO lanes use complete escaped-prefix successors; running, unscheduled and
future heads remain blockers, and finite explicit empty probes are deduplicated.
B+1 frontier ownership and H+1 canonical-head admission remain prospective.
Observed tag1 members cannot become duplicate parallel candidates.

The decoder admits D=4E+2048+576N after a complete borrowed structural/EOF scan.
R, S and C are separate monotone accounts for actual loaded owners, staging slots
and adapter work/ownership. Candidate key length is a logical C inspection tariff
before parsing, separate from decoded-key R/C; it does not pretend a copied
engine AccessGuard. W includes configured R/S/C plus the admitted live phase,
and O<=A precedes the sole reply allocation. Valid query copies drop before late
response retention admission. These are logical bounds, not engine cache, COW,
native RSS, disk, CPU or latency guarantees.

Consumed publication repeats the captured actual profile, control and complete
response/plan validation, checks original versions and shares the existing atomic
Immediate core/index/history/outcome/control writer. Exact log replay writes
nothing. Duplicate/conflict/expiry classification precedes changed Fresh query,
clock or policy. The original outcome owner moves after successful consumed
commit; uncertain completion fences and returns no reply.

FWCR1/response_version3 retains all six raw row fields, optional payload/result/
error presence, u64 version/fence/deadline/times/attempts, priority and u128
sequence. Every updated_at/token binds stored original_time; accepted rows share
type, worker and deadline. There is no invented created<=updated constraint or
narrowed attempt counter. Hard grammar is Corrupt; a legitimate narrower Claim
response profile returns healthy Limit through a Claim-only mapper. Old routes
keep their error origins. One full scanner pass validates rows, while identity
uniqueness rescans skip already validated nonidentity bytes without copying; its
bounded quadratic comparison has no measured speed claim.

Node construction keeps Claim disabled unless an explicit work profile is
supplied. Applied-history audit prepares the actual original op3 body without
executing current policy; committed suffix replay calls the actual Claim driver
under the existing shared point/body and outer workspace accounts. Snapshot,
strict reopen and protected purge retain the original response metadata.

Tests-first missing-contract RED froze 31 selected identities in e080ef4. Later
peer-found authority/order gaps received genuine assertion RED and narrow fixes.
Two storage setup mistakes (old atomicV5 duplicate-ID refusal and unsorted policy
wire) were corrected transparently before Claim; they are not product-bug RED.
The complete macOS run passes 873 tests across 90 targets and six compile-fail
examples across 15 documentation targets, strict checks, reference/progress,
cached dependency policy and the separate pinned native compatibility executable.
Actual process kills and pre-sync uncertainty are not power-loss/final-flush
proof. Every required original setup/profile, complete public parent/governance,
real policy publication/activation, accepted Raft callbacks/runtime/read barriers,
both-host validation and full M1–M8 acceptance remain mandatory successors.
