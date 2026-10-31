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


## Durable complete core-policy publication

ARCH-M1M2-DURABLE-COMPLETE-CORE-PATCH1-CREATE1 selects complete normalized core
Patch publication through the existing redb owner and neutral transaction ports.
The thirteen private production children and existing seams implement the
selected responsibility split: domain ordered preparation, ports original frame/
reply correspondence, storage actual loading/reindex/publication and node
recovery orchestration. Seven new structs/enums, two traits and the one precisely
approved transparent CompleteCorePolicyPatchPreparation tuple alias introduce
no dependency, alternative backend or public runtime. Existing Full preparation
shares the same reducer and preserves its original fees, ordering and return.

FWCP1 operation tag5 with ClientIntent2 remains distinct from recovery's local
profile2. Response1 accepted bytes are a complete FWPV2 value; rejected bytes
match the closed existing business-message registry. Validation uses the same
full semantic decoder and continuous schema account. Actual smaller profiles
are healthy limit refusals; malformed persisted/source content is corruption.
The old unprofiled outcome API still refuses intent2. Cross-operation retained
readers use a separately bounded validation account under captured store profiles.
A Core operation instead threads its same received charge through every actual
repeat; cross-operation support grants it no reset or credit.

One actual retained point classification occurs before Fresh Patch ownership.
Non-Fresh preserves the original retained result and skips current/context work.
Fresh uses actual current/layout/history context; the recognized closed layouts
contain no migration work rows. The mathematical plan retains the original
rejected query without cloning and seals the same captured predecessor's
optional length/SHA256. Both Full and point paths preserve the independent
retained lifetime. Constructor and consuming backend reject predecessor
substitution, including rejected Fresh. Native hashes are explicitly bounded
by the hard policy cap and remain additional O(P) work outside the logical g
tariff; no physical CPU/latency bound follows. A used-history Option is Some
only after the existing getter succeeds and passes its unchanged range check.
The backend compares Some to its actual immutable configuration after existing
profile/control/response validation and before writes. Early stale-CAS paths
retain None. Mismatches are InvalidChanges and leave a healthy owner usable.

Accepted publication scans every actual target-tenant canonical row, including
unrelated types, running/future/unscheduled records and every valid priority.
A due/FIFO subset cannot certify completion. Each affected lane captures its
original head ID or observed emptiness before mutation; the old scheduled head
tag is removed once. Later rows validate untouched predecessor facts against
that captured state. Only genuine stream exhaustion permits final lane repair.
Canonical records, leases, counters, order and histories remain unchanged by
reindexing. Both old and new recovery-wide catalog passes pay actual key/value
inspection, decoding, owners and engine steps. One Immediate commit publishes
the candidate, resulting indexes, original retained outcome, control and
metadata; uncertain completion fences the owner and returns no acknowledgement.
Rejected Fresh and retained outcomes do not run the catalog/reindex protocol.

The first accepted policy publication upgrades LIVE3/4 to six-table LIVE5. Its
immutable origin contains canonical FWSS3 birth bytes; old4 normalizes only the
version/checksum. FWSS5 carries current placement/policy counts, a distinct
current-catalog domain hash and the origin hash, and requires applied control.
FWSP2 contains exactly manifest/membership/origin metadata with canonical5; old
FWSP1/two-row/canonical3 DATA bytes and IDs remain unchanged. Existing pinned
and anchored DATA remain addressable through mixed-generation custody. Receive/
install adopt actual5 atomically; a live5 target refuses downgrade. A pending
transition prevents accepted policy publication before index mutation; this
adds no permission to serve unresolved paired owners. Node's explicit policy
opt-in performs bounded full borrowed audit and committed replay through the
real driver, including snapshot bases and purged logs.

Resource admission retains distinct header/decoded-command ownership, same-
decoder allowance, original reply copy, schema scratch and cumulative R/S/C
accounts. One copied stable query and owned original-head inventory are admitted
before copying. Both point enum variants are boxed inside the existing logical
phase reserves; ordinary allocator abort remains possible. These logical bounds
certify no native RSS, engine cache, latency, speedup or power-loss theorem.

Tests were frozen after genuine missing-contract and behavioral failures;
additional full-response, predecessor and history controls preceded their
corrections. The handoff separates focused macOS results from final strict/
workspace/native validation. This private component does not complete public
Policy.set/Flow ETF semantics, distributed migration, governing-owner/public
Claim, accepted consensus/transport/strong reads, offline lineage, original
source setup/profile parity, native Linux or the M1–M8 exit gates.


## Selected atomic-start committed recovery

The eight regressions saved in `ae7a0ce` precede this three-function integration.
The existing borrowed Normal header admits only the new exact root1/intent7 pair
as profile7. Global encoded admission comes first; ordinary framing, bounded
identifiers, complete digest and allowed pairing precede HLC/wall validation.
Supported Normal dispatch follows the complete stamp, preserving legacy malformed
stamp precedence. Original-policy delegation and the shared legacy header remain
unchanged. Header success verifies no command body, derived intent, EOF or accepted
consensus authority.

Historical audit calls the full atomic-start preparer under the existing
`W - base - 512` allowance and namespace ordering. Committed replay invokes the
actual dedicated transaction driver, preserving expected-control comparison,
consumed commit, original retained response and completed-prefix semantics.
Existing purge/snapshot/matching-received/fresh-restore callers share these paths;
trusted received snapshots still require their existing caller authority.

Point ownership remains `J = 512 + E`; singleton audit `D` already includes `E`
and decoded scope. Replay still prospectively admits
`base + max(J + Q, 512 + Q + normalW)` before copying scope. The driver bank already
owns `E` and one reserved `L`; the returned outcome drops before scope and the
next point. No helper, owner, queue, response clone, bank, refund, dependency,
layout, public-wire change or new runtime module is introduced.

All eight frozen tests pass in the full 996-test macOS checkpoint, including the
44 older snapshot cases filtered from focused execution. Exact comparison retains
all 988 earlier tests. Independent architecture/performance reviews accept the
three runtime files (maximum 253 physical lines) and their 19 bodies (maximum 91).
Full original profiles/parity, Linux execution, public dispatch and the ordered
consensus/service milestone gates remain required. Correctness smokes supply no
measured allocation, RSS, latency or throughput improvement.


## Binary workflow values and captured lifecycle

VALUE-CONSUMED-BINARY1 creates a fresh explicit value authority in layout7.
FWSS7 retains immutable canonical FWSS3 origin, current policy-catalog identity
and committed value defaults. It has six physical tables and four active
metadata keys. Legacy layouts are not implicitly promoted; core/original policy
and control paths preserve value defaults, while old business/Claim paths that
cannot publish value companions refuse layout7.

Scoped families8/9/10/11 represent immutable stored bytes per reference/birth,
public alias ownership/expiry/digest, current binding and every-history binding.
Nonmagic logical binary bytes are stored directly; a leading FSV2 is escaped
once using the existing borrowed two-part grammar. Required local targets must
exist even when expired. Recovery checks alias identity, length/SHA and birth,
and requires current field presence and canonical stored bytes to agree with
the validated workflow core. Historical rows have no inline value postimage;
their namespace, event/version and reference closure remain the available proof.

The same consumed writer publishes core/index/current/every-history changes,
value rows, retained outcome and control before Immediate commit and reply.
Only actual successful Created postimages capture birth policy/defaults, using
true per-step accepted ordering, including independent source-index holes.
Omitted payload carries, explicit empty/equal values replace, and the selected
terminal/retry result/error omissions clear. Birth provenance stays immutable.
Complete/Fail/Cancel may select an explicit terminal TTL; Retry exhaustion
inherits. All terminal postimages still stamp their deterministic bounded
deadline. Retained replay precedes Fresh-only recapture or current-policy checks.

Ordered reads use one real snapshot and one server clock pair for an eligible
batch, preserve duplicate slots and own returned stored bytes after donor/store
drop. Missing, unsupported, unsafe drift and operational failure remain distinct;
clock failure alone does not fence a healthy database. Recovery uses no clock.
FWSP4 carries three wire metadata rows, while the local descriptor remains local;
row_count is physical data rows plus3. Receive/install/reopen preserve committed
defaults, origin and complete value/core closure without a legacy downgrade.

Logical work and owner admission remain prospective and continuous. The current
closure reserves the source-view tuple, optional array view and optional expected
slice on every path, including None/history. Each reached comparison pays both
byte operands separately from fixed control/hash/key visits. The checks do not
claim native-frame, allocator-capacity, RSS, OOM recovery or measured speed bounds.

The genuine current-presence/current-byte/Retry regressions were saved failing
in5fa3d20 before their corrections. Current component tests and unchanged legacy
compatibility are bound in the [handoff](../CONTINUE.md) and
[reciprocal reviews](../reviews/history-values/architecture.md). The two old
unknown-snapshot producers now use an actually unregistered version and retain
separate known4-malformed controls. Explicit Node operation-8 audit/replay is
implemented as described below; typed/shared/external values, actual hydration/lifecycle maintenance,
original profiles, native and Linux evidence and full M1-M8 acceptance remain
required.

## Operation-8 startup audit and replay

Committed and snapshot startup recovery can opt into the private value command
through `with_workflow_value_limits`. Ordinary construction remains disabled.
The four value ceilings are positive, and the point-count/per-entry-work product
is checked before enabling the profile. The getter reuses the one selected Normal
profile. Private recovery helpers borrow that immutable configuration.

The actual original operation-8 body and log identity reach one consumed storage
driver. Existing scope, full applied-body digest, stamp, clock, membership and
expected-control checks remain. Historical audit performs complete bounded
footer/intent/metadata/EOF validation and drops its decoded owner; it does not
consult current policy or recapture birth. Driver errors remain lossless
`CommittedRecoveryError::Value` errors. Owned outcomes and scope are dropped
before recovery returns scalar control. Legacy preparers and dispatch stay unchanged.

Let `B` include the existing namespace/phase owners plus the target-derived new
option/error-header allowance `F`. All startup paths pay `F`; only operation 8
pays `Qv = size_of<Option<WorkflowValueTransactionLimits>>` before its getter,
including the disabled case. The full point owner `J = 512 + encoded_length`
remains live. Audit admits `B + J + Qv` and gives only the checked residual to
the bounded preparer. Replay admits `B + Qv + J + Q + max(V, C)` before copies,
where `Q` owns the final scope, `V` is the unchanged driver workspace and `C` is
the checked largest actual scope leaf plus a Vec header. Scope construction ends
before driver ownership starts. Value temporary ownership is already inside `V`.
These are logical requested-owner allowances, not native allocation/RSS or timing
measurements; conversion cannot assume exact allocator capacity or an in-place shrink.

The same routes serve local snapshots, matching received snapshots and fresh
restore. Received boundaries remain strictly newer than committed/purged state:
installation acknowledges the snapshot before the retained suffix is committed
and replayed. No guard is relaxed. Full operation-1/5/6/7 value, history, lifecycle,
retained-response, uncertainty, durable-prefix, snapshot and reopen scenarios
execute through these real APIs. The initial rejecting checkpoint is `032939b`;
`da81bd8` preserves the later conversion-budget counterexample before correction.

This startup composition assumes an already durable committed prefix. It is not
a live accepted-service dispatcher, a consensus/read-barrier implementation or
full workflow parity. Typed/shared/external values, public hydration, lifecycle
maintenance, original/native/Linux profiles and all milestone exits remain open.

## Explicit represented-core historical snapshots

The private data codec selects FWHS1: magic `FWHS`, big-endian u16 version1,
u32-length-prefixed FWSR2 postimage and FWSH2 event, followed by exact EOF.
Both nested formats retain their v1 field grammar and tags0–12; only explicit
version2 accepts StepContinued tag13. Normal v1 readers, recovery routing and
store formats remain closed. Encoding or decoding these values grants no store,
read, publication, rewind or snapshot-installation authority.

The neutral `StoredWorkflowHistorySnapshot` owns the complete currently
represented core record, its tenant/partition/ordering and its reduced event,
including resolved retry observations. Namespace, original event-ID version,
latest-event kind/version/fence, occurrence/update time, attempts, state, due
time and lease worker/deadline must agree. Retry observations require Retry
kind. Parsing validates the complete envelope and these relationships while
borrowing the donor, before variable owners are copied. Existing record/history
measurement and emission write directly into one reserved envelope buffer;
there are no temporary encoded record/history vectors.

Independent positive wire, returned-owner and logical workspace ceilings apply.
For complete encoded size E, the encoder admits E+2048. Decoding admits R,
the existing record owner bound plus512 plus each actual variable history
occurrence, then admits E+R+2048 before materialization. Every sum is checked.
These logical accounts do not establish native RSS, allocator capacity or CPU
bounds. The owned-byte cap is for decoding; encoding only owns its output buffer.
All numeric framing is portable big-endian and host-width conversions are checked.

Pinned `apps/ferricstore/lib/ferricstore/flow/codec.ex:291–374` and
`flow/codec/support.ex:201–227` show that original FSH2 history stores lifecycle
state and does not encode logical run_state. This internal full-core postimage
preserves run_state without adding it to the original source history fields.
Rich ActiveRecord extensions, captured lifecycle and actual historical value
provenance remain mandatory companion/capture work; absence is not inferred
from a later current record. This codec alone supplies no complete rich-source
parity or durable Step publication. Actual transactional integration, historical
companion preservation, inquiry/fault/reopen behavior and recovery remain required.

The six rejecting compiled cases were saved in9466fd1 before codec bodies.
Their unchanged complete tails now pass, with strict focused Clippy. Review and
whole-workspace validation evidence live under M2-HISTORY-SNAPSHOT1 in the
coordination record and handoff. Native original-profile execution and independent
Linux/macOS acceptance remain separate gates.

## Step transaction coherence

The pure `GroupApplyChanges` boundary now checks Fresh intent10/response9 against
the independently supplied complete control plan. Accepted means one actual
indexed Step postimage; deterministic indexed rejection means no result count.
Scope, status, full original log identity and committed execution time must
agree, and the complete reply must match the actual decision through the
existing Step response verifier. Structural hard ceilings used by this check
do not pay or replace the narrower captured transaction budgets. Verification
borrows existing response bytes and introduces no variable owner.

Three compiled constructor refusals and a separately failing effect probe were
saved in cb51fd7 before implementation/classification. Their unchanged tails
cover accepted payload presence and grouped units, wrong scope/predecessor,
actual stale/FIFO rejections and original replay/conflict/expiry/exact-log behavior
without a current record. The private Step validator and descendants are pure;
the neighboring backend namespace remains effectful. Old constructor branches
and existing atomic-start/independent validators retain their prior behavior.

This check supplies coherent proposed changes, not consumed storage authority or
proof of durable success. FWSO intent10 admission, schema negotiation, actual
record/index/history/value/outcome/control publication, historical rich/source
companions, inquiry/fault/reopen behavior and recovery remain required.

## Explicit Step retained data

`encode_step_continue_outcome` and `scan_step_continue_outcome` select the
intent10/response9 data profile through explicit APIs. They reuse unchanged
FWSO1 framing/emission and the existing borrowed/owned retained types. Generic
outcome encoders, scanners, policy restoration and normal store/recovery routing
stay closed to Step; an encoded data value grants no publication capability.

Complete framing/EOF, exact namespace/request, expiry, original full log/time,
reply status/count and the whole Step response are validated before any variable
owner. The encoder compares the derived status with the supplied typed receipt.
The caller's actual AdmissionLimits constrain reply fields/bytes independently
of the full wire ceiling. Wire exhaustion remains Limit; legitimate narrower
reply admission remains ProfileLimit. Materialization separately admits512 plus
each actual tenant/group/recovery/request/response byte through the existing
fallible owner conversion. Scanning borrows its encoded donor without retaining
the expected query or allocating response/policy scratch. Encoding reserves one
output buffer through the unchanged shared emitter.

The three compiled refusals were saved in5d59712 before bodies: two healthy
encoder calls and one independent-literal scanner call reject UnsupportedVersion.
Independent accepted266/rejected147-byte vectors frame the already verified
FWST response and a pure rejected-decision control. Their respective owner bounds
are676/557. A pre-runtime transcription correction removes one extra digest byte;
it changes no frozen runtime oracle. Unchanged tails cover complete original
metadata, owned lifetime, every truncation, context/flag corruption, old-profile
closure and exact wire/owner/field limits. This is restoration-format evidence,
not source business-case execution or native/OS acceptance.

Actual negotiated physical schema, full historical context/value companions,
atomic record/index/history/outcome/control publication, inquiry/fault/reopen and
recovery remain required. Do not open the generic formats merely because these
data functions pass; all source/profile/Linux/macOS milestone exits remain open.

## Explicit complete-history storage and Step publication

The consuming `upgrade_history_snapshot_layout` selects FWSS8 from a ready FWSS7
value store. It preserves the entire defaults/origin/alias/binding grammar and
validates the complete current and retained cohorts before and after changing
only the current manifest in one Immediate transaction. Other formats refuse;
format8 is idempotent. Unproved commit consumes the owner and requires reopen.
Construction and snapshot installation never silently select this format.

Under8, newly written current records use FWSR2 and each new historical row uses
FWHS1 at the existing history key, with the actual intermediate postimage and
ordering. Independent batches use the existing value publication's individual
steps, not final-record ordering. Old FWSR1/FWSH1 rows remain explicit legacy
data; no historical postimage is reconstructed from a current record. Every
retained snapshot cohort is checked using its own parsed layout. FWSP4 transports
either manifest7 or8, so receive retains the actual manifest format. An incoming8
snapshot needs an explicitly8 target, and incoming7 cannot downgrade an8 target.

`apply_step_continue_transaction` uses the actual consumed value/core writer.
Full log/prepared identity and retained replay/conflict/expiry/exact-log decisions
precede Fresh-only current/policy/binding reads. Only layout8 accepts this path.
An accepted Step publishes the rotated lease/fence, logical state/order, actual
values/references and lifecycle, complete history, original response and control
together. It retains captured birth metadata and clears effective TTL/deadline.
Plain Step payloads are logical binary even with stored-value aliases enabled.
The one nonbirth input carries no invented policy/birth witness.

Full history emission uses borrowed fields and one admitted output buffer.
Reduced history/recovery validates the complete borrowed postimage and copies
only the requested event; fixed descriptor workspace2048 and actual encoded/
returned fields are admitted prospectively. These are logical accounting claims,
not native RSS, allocator, throughput or Linux measurements. Generic public data
codecs retain their closed profiles; only explicit store-format dispatch uses
the complete snapshot and Step retained data APIs.

Genuine rejecting tests were saved in10be665 before bodies. Two masked fixture/
inquiry assumptions and the growth-aware fault boundary required documented
post-RED corrections; they do not count as unchanged tests or additional source
credit. All11 focused physical cases and7 history-codec cases pass; strict
focused Clippy passes after flattening the new writer branch. Whole-workspace
verification and adoption evidence belongs in M2-STEP-STORAGE1 and the handoff.
The Node operation10 successor below adds committed startup replay. Rich original
context/attributes, the actual
original Create→Claim→Step→Retry→FIFO chain, native/profile execution, and
independent Linux/macOS milestone exits remain required. This component is not
a completed live service or full workflow parity.

## Explicit Step committed startup recovery

`with_step_continue_limits` independently selects intent10 recovery using the
existing positive, overflow-checked shared value ceilings. Legacy construction,
binary-value selection and stored-value selection leave Step disabled. A private
`StepRecoveryProfile` enum records only that opt-in; it is not persisted-format
authority. The actual storage driver still requires an already upgraded format8
owner. Startup never upgrades state or manufactures a policy/value witness.

The explicit `step_continue_recovery_header` borrows profile/scope/stamp facts
through the shared bounded parser. Its separate stored selector governs intent9.
Old public header calls always select Step=false and retain their exact closed
profiles. Header success does not validate the body. Node audits the full Step
frame with `prepare_step_continue`, including intent, operation, fields, derived
keys and EOF, even when the checkpoint has already applied that historical entry.

Audit prospectively admits the actual entry/body, profile descriptor, remaining
workspace and checked preparation work65536+4096+16*body.len. Shared admission
charges the selected configuration descriptor in every phase; actual Step
profile locals have their separate descriptor fee. Disabled selection retains
old admission. Apply uses the real durable Step transaction and checks full
control identity as other drivers do. One continuous point/body work account
spans the whole committed-prefix call. No wrapping command, second value-batch
decode, reply queue or fabricated current state is introduced.

The shared route also covers snapshot recovery, trusted received boundaries and
purged suffixes. A completed replay prefix remains durable on a later refusal;
an uncertain commit consumes both owners, and reopen validates a complete old
or new cohort. Uncommitted opaque tails are preserved and never interpreted.
Plain Step payloads remain logical binary on both value alias profiles.

Genuine rejection-only state a183712 predates bodies: one actual predecessor
fixture control passes, eight Node scenarios fail at the healthy new selector,
and an independent literal-header case fails at the healthy new header. The
full saved tails were masked. Post-body controls demonstrate three incorrect
test assumptions about event IDs, Retry logical state and older-profile error
ordering; corrections and exact inverses are recorded. All10 focused Node
cases and8 codec cases now pass with strict Clippy. The extra old-format control
is not TDD or source-parity credit. Whole-workspace/adoption evidence belongs in
M2-STEP-NODE1 and the handoff. Original named Create→Claim→Step→Retry→FIFO,
rich captured attributes/context, native/profile/Linux/macOS execution and
complete M1–M8 exits remain mandatory; this is not an accepted live service.

## Named Claim with captured values and complete history

`apply_workflow_value_claim_transaction` uses the existing op3/intent1/response3
Claim format and existing Claim result/error types, with actual value ceilings
and Claim work limits. It requires an explicitly upgraded format8 owner. The old
Claim API keeps its7/8 refusal. Both paths share the original retained gate and
Fresh selection; no new codec or selected-record input is introduced. The
trusted-parent leaf's governance and broader orchestration refusals remain.

One opaque publication plan borrows each actual accepted ScopedPlan and ordering
and carries only its captured scalar binding data. Reads use the actual previous
record version and installed defaults. Claim preserves all immutable value refs,
birth retention/history and policy provenance, and clears effective TTL/deadline
as pinned flow_retention_values.ex527–588 requires. It creates no value or alias.
The consumed writer repeats full Claim/control/plan validation and publishes
record/index/full history, both bindings, original outcome and control together.
Old binding writers reuse the same borrowed-field emitter and accounting.

Node selects this path separately with `with_workflow_value_claim_limits`; old
Claim/value/Step opt-ins do not enable it. Complete historical audit uses the
existing Claim decoder and real replay invokes the actual value Claim driver.
Selected descriptors, value work/temporary and whole-call point/body budgets
remain explicit through local/received/purged snapshot recovery. No query keys,
record payloads or public receipts are cloned into a new adapter queue.

Rejecting runtime was saved in7ac9f66 before bodies:8 storage and3 Node cases
fail at healthy API/selector refusal. All11 complete tails now pass unchanged,
including the original Create→named Claim→Step→Retry→FIFO sequence with actual
typed Retry error and Original policy activation, both alias profiles, unchanged
immutable rows, retained gates, governance refusal, uncertainty and full snapshot
recovery. This advances durable source-sequence conformance; public Claim parent
orchestration/governance, typed Step maps, rich source context, full original
profile/native/Linux/macOS and service acceptance remain mandatory. It does not
certify full source-case execution, native performance or product completion.

## Stored Step publication and recovery

`apply_stored_step_continue_transaction` explicitly selects intent 11 under
actual format 8 and alias 2. It shares plain Step's retained gate, same-guard
record/policy/binding/index reads, pure decision and consumed transaction. The
existing stored emitter writes supplied payload bytes exactly once; omitted
payload retains its prior reference. Full history, current/history bindings,
original response and applied control remain one Immediate publication. Plain
Step continues to wrap wrapper-looking logical binaries. No physical format,
new owner, key family, record or payload clone is introduced by the selector.

`with_stored_step_continue_limits` selects stored Step independently of plain
Step and generic stored-value recovery. The existing Step profile enum records
disabled/plain/stored/both selection. Full-body startup audit, actual writer
dispatch, prospective profile/workspace admission and whole-call point/body
budgets carry through snapshot, trusted received and purged-suffix recovery.
Earlier successful entries remain committed when a later entry is refused.

Actual rejecting runtime 19b4570 precedes bodies. Five unchanged storage cases
now execute the pinned typed Create/named Claim/Step/stale Complete/Retry trace,
typed value hydration, exact presence, capability/limit refusal, complete
snapshots and delegated uncertain commits. Two codec and three Node cases pass
after the documented two-file fixture/oracle corrections; all eight older codec
cases pass. This is durable component evidence for source fb8106cb0cee10e6,
not original native/profile execution or an accepted public workflow service.
Full workspace and adoption results belong in M2-STORED-STEP1 and the handoff.

Startup cohort validation also requires alias 2 for intent-11 outcomes when no
value or alias row exists. A supplemental real payload-free Create/Claim/Step
regression reproduced acceptance of a checksummed alias-only downgrade before
the existing outcome capability check was extended. It now rejects Corrupt,
preserving every table. The same manifest/owner admission covers this check in
live and retained snapshot cohorts; it does not infer capability from row count.

## Selected identified governing-credit integration

M2-IDENTIFIED-CREDIT1 selects a real stored owner protocol under the existing
exclusive group/recovery transaction. The storage leaf is implemented and its
restart/snapshot/fault controls pass. Explicit Node historical audit/replay,
received/purged suffixes and all112 Node cases now pass. Queued-backlog and
wider expiry-burst progress are corrected and all12 local gates pass; full
source/public/platform batch acceptance remains unfinished. Initial rejecting contracts alone established no
durability or public Claim authority.
The exact artifact selection is in the coordination record. Keep the domain
effect-free and its existing types-only dependency.

Fresh credit-capable genesis uses existing layout8 and alias2 plus a checksummed
manifest extension: after the 60 defaults bytes and before the final SHA256,
append u16 credit_format=1 and u64 actual shard_count in 1..=MAX_EXACT. Absence
retains every previous manifest byte. Unknown extension/format refuses. Older
readers fail their checksum/EOF checks. Existing constructors never silently
enable this capability; new creation publishes it with all genesis facts in one
Immediate transaction. Credit rows or retained credit outcomes without this
capability are corrupt, including empty-owner/no-ID downgrade cases. Snapshot
capture/restore and received cohorts preserve and compare the extension.

Reserve physical families12 owner,13 identified reservation status,14 ordered
reservation page and15 cleanup task. Bind every key/value to actual group,
recovery, tenant, credit scope and applicable shard/epoch/page/sequence/ID.
Owner values detach IDs. Point-read only requested IDs; retained active/released
records prevent recharging while a sibling remains active. Full release retires
the generation and queues its old pages; expiry also queues cleanup. Preserve
the source 1000-ID mutation,256-ID page,256-page history and256-task backlog
ceilings. One cleanup command deletes one real page and advances its cursor.
Canonical reopen must validate the complete owner/page/ID/cleanup closure under
bounded recovery accounts. A later failure publishes no prefix effects.

FWGO1 stores the bounded detached owner. FWGI1 stores original ID, page
backreference and Active/Released status. FWGP1 stores shard, epoch, page and
ordered unique IDs; FWGQ1 stores sequence, shard, epoch and remaining page range.
The reservation key adds big-endian shard/epoch components and original ID to
the four owner components. Page keys add shard/epoch and a numeric page suffix;
cleanup keys add a numeric sequence suffix. Full framing/EOF validation precedes
payload ownership. All old key and absent-extension manifest bytes remain exact.

Recovery proves forward closure from each owner through current lease pages and
contiguous retired tasks to exact ID rows with page backreferences. Page IDs are
unique, so this mapping is injective; every physical page independently needs
current-lease or retired-task coverage. Checked equality of total ID rows and
covered page members excludes orphan IDs without rescanning page membership for
every reverse ID row. Current-generation active counts must equal in_use and
owner-wide live/retired generations are unique. The same existing open recovery
table serves these reads; nested redb table reopening is forbidden.

Reserve FWCP1 operation10 / client-intent12 for this explicit private protocol,
with retained response_version10. Preserve the shared request namespace and
retained point gate before changed current owner/configuration/time. Reuse of a
request across another operation must conflict rather than create a second
request namespace. All encoded optionality, IDs/order, scalar/configuration and
EOF facts require full validation and a verified prepared digest. The decoder
owns only bounded request fields/IDs, never historical reservation maps. Its
canonical private response can carry original internal owner fields; public
View mapping must still omit cleanup/fingerprint internals and reservation/page
fields. Known rejection may carry source expiry/reclaim effects; operational
failure creates no retained business response.

One deliberate source safety correction prevents a full queue from blocking
its own cleanup: only Cleanup with existing queued work may defer an expiry
preparation that cannot fit. It deletes at most one existing page and keeps
the expired lease/capacity intact until a later command can enqueue its exact
detached reference. Normal commands retain BacklogFull denial and no prefix
effects. The real regression failed before correction, then proved one-page
drain, safe subsequent expiry enqueue/reclaim and continued canonical recovery.
The matching isolated Elixir change is not natively verified. Bursts exceeding
256 new page generations with no existing queued task use bounded expiry:
Cleanup admits the first256 expired references in existing shard order, retires
only those exact detached generations plus page-free expired leases, and keeps
the remaining expired/live leases intact. Existing normal expiry is unchanged;
normal commands still refuse excess backlog. One cleanup deletes one page.
The two real512-shard failures precede the correction; complete reopen,
mixed-capacity and budget-abort/exact-retry controls now pass. The sorted lease
vector is retained in place using the [documented retain contract](https://doc.rust-lang.org/std/vec/struct.Vec.html#method.retain),
without a second map/vector or sort. No shard ceiling is reduced, and no native
performance or source-profile result is inferred. All12 local gates pass; full
source/public-service/platform acceptance remains unfinished.

Use one consumed writer for owner, reservation/page/cleanup changes, original
response, control and checkpoint. Unknown commit fences the owner and returns no
success. Retried/unknown operations keep exact original IDs and request identity.
Node recognition must be explicitly opted in and audit full bodies; old generic
paths remain closed. Exercise actual persistence faults, restart, snapshot and
received/purged replay before batch acceptance. Authoritative current governance,
workflow-bound release outbox, the atomic public Claim parent, catalog/service
semantics and every required source/profile/Linux/macOS gate remain mandatory.

### Selected credit catalog publication successor

M2-CREDIT-CATALOG1 selects explicit credit_format2 with the same checked actual
shard count in layout8/alias2. Format1 remains the private predecessor. Format2
adds a publication outbox per actual group/recovery/tenant, separate from lease
shard IDs. Family16 stores head/tail (FWCM1);17 stores a monotone sequence and
original credit scope (FWCI1). New owners and their publication intents must
commit with the original retained reply/control through the existing consumed
writer. No observation, counter or plan certifies commit/current authority.

Preserve source65536 pending,256 ack/read page and MAX_EXACT sequence bounds;
head may reach MAX_EXACT+1. Acknowledged retries precede stale cursor refusal.
No metadata/intent map is cloned on mutation. Reopen validates active contiguous
entries, reverse range coverage and owner-to-meta presence with the same open
table and bounded accounts. Missing owners on well-formed intents are preserved
for publisher discard, not promoted to committed owners. Snapshot custody and
Node opt-in compare the exact capability even for an empty cohort. Native ETF,
full durable ack, distributed catalog publication/repair/cleaner and source/
Linux/macOS acceptance remain required; selection alone implements none of them.

The durable catalog ACK successor uses explicit FWCP1 operation11/intent13,
response11 FWCA1, behind actual credit_format2. Its fixed body carries the source
expected_head/up_to controls (positive exact, ordered, at most256 entries).
Retained original response, shared request namespace, control and actual queue
deletions use the same exclusive Immediate writer. Fresh metadata guards never
precede the retained gate. Only StaleCursor/InvalidRange can be retained; corrupt
or operational failures abort, and uncertainty fences without acknowledgement.
Original profile/generic readers remain closed; success and rejection receipts
alone require format2 on reopen. Existing Catalog Node opt-in audits complete
bodies and uses that same writer through startup and snapshot suffixes. The
cross-group publisher must still publish membership successfully before ACK.

Target governance membership is selected as an independent explicit FWSS8
capability, with optional `GC\0\x01` before the manifest checksum (after the
optional credit extension). It does not require an owner shard count. Older
constructors preserve their exact bytes. Physical family18 stores zero-score
binary set membership as group/recovery/tenant/catalog/member components with
fixed `FWGM\0\x01` values. No routing authority is derived from member bytes.
FWCP1 operation12/intent14 and fixed response12 `FWGR\0\x01` use the existing
retained namespace/control gate and one consumed Immediate writer. Reopen and
snapshot compatibility require the explicit capability even when only a receipt
remains. Batches validate every raw member before allocation/deduplication and
permit at most256 members. Reads page only the selected catalog and require the
actual last member to remain present before continuing; a full page supplies a
cursor even if the next read is empty. These selected contracts do not by
themselves implement the publisher, accepted consensus or source parity.

The selected catalog publisher uses a neutral FWGL1 member identity containing
original group/recovery and credit scope; tenant is bound by matching source and
target request scopes. Membership bytes confer no placement authority. The
actual source read owner verifies its namespace, and the supervised submission
capability verifies target/source namespace and authorization. Publication uses
complete retained operation12/intent14/response12 validation before submitting
operation11 ACK. Lost/uncertain/foreign replies never authorize ACK. One selected
source advances round-robin even after operational refusal; counts preserve the
source distinction between processed entries and unique membership inserts.
Combined explicit owner/target genesis uses existing independent manifest fields
in one Immediate commit, without an unlogged runtime upgrade or new family.
Application orchestration depends only on neutral ports. An accepted consensus
adapter remains required; committed-prefix test fixtures do not supply one.

Actual new-owner creation always appends a fresh publication sequence after
validating the exact tail observation. Reusing a matching active tail is unsafe:
a publisher may already have observed it as missing and then ACK that prefix
after creation. A separate fresh sequence survives that ACK. Existing-owner and
retained retries still append nothing. The prior neutral reuse algebra is not
authority to suppress creation publication. The interleaving regression uses a
real committed creation between the coherent owner batch and the old ACK; it
fails before the correction and then publishes the surviving new intent. The
matching pinned Elixir defect is corrected on an isolated unverified branch.


The selected governance collection traverses original fixed256 anchored pages,
loading each configured record separately and preserving only globally ranked
bounded results plus one candidate. A full result never ends traversal. New
comparator-equal records precede retained equals, matching the source's reversed
page/prepended accumulator stable sort. Explicit keys bypass catalog paging.
The result owns the original values and exposes iterators without a second
payload allocation. Read/loader guards and logical result/slot/candidate/page
budgets are explicit; per-read coherence does not imply a Raft strong-read barrier.
No fixed scan-count cap silently changes the source collection semantics.

Existing-owner catalog repair uses the same configured owner/member reads and
supervised complete retained membership validation as publication. Missing
owners create no ghosts. An already present member skips submission. Otherwise
only a complete accepted original reply confirms repair; uncertainty/refusal
must stop a dependent public mutation. The same single encoded member owner is
borrowed for point lookup and any registration. No schema upgrade, direct write
API or accepted-consensus claim follows. Complete public credit parents and the
durable cleaner progress/fairness protocol remain successors.


The immutable governance capability0/1/2 is observed from the manifest already
validated at genesis/open and retained on the exclusive store owner. Node uses
this scalar to reject GC2 under older recovery selection, including empty state,
without adding manifest reads to old tight profiles. Selected GC1/GC2 still
requires the actual manifest under the same transaction. Candidate snapshot
validation cannot mutate this cached identity; exact manifest compatibility
prevents changing it through installation. This scalar grants no authorization.

Selected cleaner progress is an explicit GC2 capability in the existing FWSS8
marker, retaining GC1 bytes/constructors. Family19 binds one derived cursor to
its group/recovery/tenant/catalog. None clears its row; empty binary cursors are
valid distinct values. Closed private operation13/intent15/response13 joins the
set/clear, original retained outcome and applied control in one Immediate commit.
Original retries do not overwrite newer progress. Actual capability and snapshot
compatibility are required even for empty and receipt-only authorities.

Cursor payloads are nonauthoritative hints. Valid rows use FWPC1 and a bounded
length-prefixed cursor. Invalid hint bodies remain stored/snapshotted unchanged
and read as Invalid within real R/C/step bounds; only a committed reset may
remove them. This preserves restartable native progress and command accounting.
Malformed keys/namespaces and authoritative owner/control/outcome data retain
the existing strict fencing policy. Cursor hints never authorize credit deletion:
cleanup must revalidate actual owner/epoch/ID state through committed apply.
The native malformed-owner fairness disposition, full cleaner/public scheduler,
accepted consensus and platform/source acceptance remain separate requirements.


Selected bounded credit cleaner runner uses actual catalog/progress/owner reads
and supervised private Cleanup/membership/progress commands. Default16/max64
attempted commands, two-page pending-owner bursts and4N+4 attempts preserve the
pinned scheduler accounting. Cursor reset/write attempts share that budget; only
confirmed effective progress can wrap. Fresh steps reload committed hints.
Trusted preparation must meet the explicit tick time floor; apply revalidates
actual current owner and committed time. No timer, consensus acknowledgment or
same-group malformed-owner migration is implied by this runner. See the concrete
solo artifact/ownership review in workflow-completion-coordination.md.


Selected revision-checked catalog pruning fixes the executed Missing-read ->
new-owner create/register/ACK -> stale-unregister loss at7da2b8d. Explicit GC3
stores each family18 member with its complete committed registration EntryIdentity
(FWGM2). Fresh registrations rotate that identity even if membership already
exists. A private operation14/intent16/response14 atomically removes only an exact
matching member revision; Missing/Changed retain an accepted no-op reply. Retained
retry cannot remove a later registration, and unrelated member/control changes
do not invalidate this member. GC1/GC2 constructors remain byte-identical and no
runtime upgrade or new table is selected. Older recovery modes refuse GC3.

Candidate member and revision come from one coherent bounded guard. Missing owner
observations must be reconfirmed after that candidate through a freshness-capable
source read; default adapters refuse without it. Production implementations need
the accepted leader/read-barrier lifecycle; current Redb guards establish actual
applied state, not consensus acknowledgment. Existing snapshots bind the exact
capability and canonical member revisions. This is a deliberate safety migration
from the pinned cleaner, whose plain unregister reproduced permanent membership
loss after publication ACK. Concrete artifacts, resource ownership and tests are
approved in the solo coordination review; no full parity/service claim follows.

GC3 private representation refinement: the immutable governance capability in
ValueManifest and ValueGenesisCapabilities is one GovernanceCatalogProfile enum
(Disabled, Membership, Progress, Prune). Dependent membership/progress/prune flags
are queries, preventing invalid combinations. Wire absence and four-byte GC1/2/3
markers, public construction, exact snapshots and validated profile0..3 caching
remain unchanged. This replaces the provisional three governance booleans.

Public credit caller selection: reuse identified2/catalog state and the existing
atomic credit reducer/writer. Explicit FWCP1 operation15/intent17 retains the
existing response10 shape while comparing canonical normalized caller fields,
separately from trusted stamp, observed epoch and generated batch/IDs. Complete
frame SHA binds preparation; fresh apply revalidates current authority. Existing
private operation10 remains closed. Explicit Node selection audits/replays the
public profile. Intent17 receipts require actual credit2/catalog capability even
without owner rows. Full caller orchestration must resolve original outcomes
before fresh source reads/registration/entropy, repair existing membership before
dependent mutations and preserve exact spend IDs on uncertainty. New owners
commit with the existing pending publication before synchronous publication.

Public credit application ownership: a shared immutable Arc<PreparedBytes> joins
caller retry context and supervised execution lifetime without copying payloads.
Original public17 identity and prepared bytes/IDs/stamp survive Unknown; retry
re-admits and re-submits the same frame. Retained inquiry precedes fresh source,
repair and entropy. Original response inspection binds operation/scope/shard/count
and known IDs. Catalog repair can commit before credit proposal; accepted new
owner credit can commit before synchronous publication. Represent those boundaries
explicitly, preserving the atomic core outcome and durable publication outbox;
do not invent a whole-call rollback or fresh spend after uncertainty.

The exact std::sync::Arc type path is permitted only in the neutral public credit
submission port module for this immutable prepared-frame owner. This does not
permit std::sync modules, locks, atomic primitives, threads or effects in pure
code. The architecture checker must retain negative coverage for these paths
and refuse Arc in the remaining pure modules.


Selected workflow release custody extends the existing immutable governance
capability to GC4. The private enum variant WorkflowRelease inherits GC3 and
requires actual layout8, alias2 and identified credit2/catalog with a finite
positive shard count. The four-byte capability position and all GC1/2/3 formats
remain unchanged. Manifest and snapshot comparisons bind the exact version.
Backend capability inspection is not authorization or a proof of remote credit
spend. The old governed-Claim refusal remains until workflow reservation binding
and release enqueue are atomic, audited and recoverable. Custody selection alone
does not implement those writes or the full public Claim/reconciliation parent.

GC4 release data uses four closed physical key families:20 record reservation
binding,21 tenant queue meta,22 sequenced release intent and23 sparse completion.
The binding point includes group/recovery/tenant/partition/record; queue points
include group/recovery/tenant and the exact positive finite sequence. Native
FWGB1 contains a finite positive original reservation epoch and one complete
FWRI1 intent. It is an original record extension, not proof of a current paid
lease. No core WorkflowRecord field is duplicated. Completion is exactly FWRC1.
These are native formats; no ETF byte compatibility is promised.

Canonical recovery requires applied control and actual GC4 for all four families.
Each binding must match the exact live core record and physical partition, with
a valid shard and original time no later than that record's update and control.
Meta proves the complete bounded contiguous pending range exists. Every intent
must be in that range; every completion must refer to an existing in-range intent.
An intent may outlive its workflow record, allowing eventual release after
retention. Missing meta denotes empty only in the absence of queue rows. These
joins run in the shared snapshot/recovery walker; snapshot data has no weaker
parallel validator. Bounded page reads own each native intent once and expose a
borrowed view. Their observed completion and local guard grant no leader barrier
or permission to release remote credit.

Until actual pre-reply release admission and same-writer append exist, legacy
record publishers explicitly refuse bound records and abort the complete cohort.
A validated immutable cached profile supplies only a negative dispatch fast path
for earlier stores; positive4 reads the actual manifest and binding under the
writer. The temporary fence is not a queue-full business result. The governed
Claim refusal remains. Data/schema fixtures and snapshot roundtrips do not prove
Claim, append, ACK, reconciliation, source-case parity or platform acceptance.

The explicit apply_workflow_release_transaction successor now accepts actual
GC4 intent9 workflow mutations. It reuses the retained-first stored-value driver;
legacy entrypoints keep their binding fence. Current final postimages and their
same-guard bindings determine pending releases. Private ReleasePublicationPlan
owns only native intent buffers and bounded index descriptors. Independent
final indices are sorted into original input order. Reservation-ID membership
uses indices into those original buffers, without cloned ID owners. Admission
precedes immutable GroupApplyChanges and value companions. Queue capacity refusal
becomes one retained native-command business rejection; resource/corrupt/exhausted/
engine/uncertain failures remain operational.

The consumed writer repeats actual capability, prepared-entry, binding, original
queue-state, append arithmetic and complete intent/postimage/order checks. Its
sealed VerifiedReleaseWrites borrows the exact live postimages, using sorted
borrowed partition/record references and pointer equality for membership. Only
those postimages bypass the legacy fence. Queue entries/meta, records/indexes,
every successful history/value companion, retained outcome and control commit
together with Immediate durability. A bound Claim remains unsupported; a stored
value frame cannot invent paid authority. Intents use actual postimage update
time, retain source reservation-ID order within this native command, and may
duplicate a previously pending ID from an earlier command. Retained processing
never appends again.

Kind8/code0 is the new whole-command backlog rejection; kind7 remains the old
AtomicStart DirectRunningFifoEntry. Only intent9/response5,6,8 may retain the new
reason. It is forbidden in independent item rows and older response associations.
Its persisted receipt requires actual GC4 even without binding/outbox rows. A
real older GC3 store refuses an injected actual new receipt, and opens after
that incompatible receipt alone is removed. Actual commit uncertainty keeps a
complete old or complete new cohort and fences further use; actual committed
queue/receipt data survives snapshot receive and original retained replay.

This is the selected native storage command contract. Raw public batch command/
write-call granularity and intermediate release counts still need their parent
source audit. Paid governed Claim/binding creation, ACK/mark/drain, Node historical/
received/purged release execution, policy/refund/reconciliation/service wiring,
full source scenarios and native platform/performance acceptance remain open.
