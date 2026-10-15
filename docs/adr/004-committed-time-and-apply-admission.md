# ADR-004: committed time and bounded apply admission

Status: selected for implementation after the user's full-completion request,
the coordinator's pinned-source reads and architecture/performance contracts.
Behavioral and adapter validation remain required. This ADR adds no dependency
edge or effect to the pure types/domain crates.

## Evidence and purpose

The architecture requires explicit committed time, immutable apply limits,
distinct client-intent/prepared-entry identity, same-group atomic writes and
retained outcomes before current guards. Its earlier time policy was proposed.
The source `raft/command_stamp.ex:7-60` bounds HLC physical time to 48 bits,
logical time to 16 bits and wall time to 0..physical. `command_time.ex`
keeps committed apply and wall contexts distinct. Those observations do not
claim that the Rust nondecreasing group clock already existed in Elixir.

Source `raft/apply_limits.ex` sets default value size 1,048,576 bytes and hard
maximum 536,870,912 bytes. `raft/apply_context.ex:38` sets default batch count
1,000; `flow/mutation_attrs.ex` caps configurable batch count at 100,000.
Source encoded keys have a 65,535-byte limit, distinct from bare identities;
`flow/storage_scope.ex` bounds encoded isolation scope to 8,192 bytes.
Exact public workflow timestamps remain 0..9,007,199,254,740,991 where source
compatibility requires them. A future network cap may be stricter than embedded
apply limits, and must never silently remove acceptance cases.

## Selected group time rules

Persist group HLC and wall control metadata with the same applied checkpoint as
each entry. Given previous state and a validated committed stamp:

- HLC advances to the lexicographic maximum of previous and input
  `(physical_ms, logical)`.
- Wall control time advances to `max(previous_wall, input_wall)`. It is a
  separate value; no local wall clock is read at replica apply.
- Every admissible stamped accepted mutation, deterministic business rejection,
  retained-request replay, explicit tick and stamped membership/control entry
  uses the same advancement rule.
- An unstamped Raft blank/membership entry advances its applied checkpoint but
  leaves these clocks unchanged. Do not manufacture a local stamp for it.
- A second delivery of an already applied log identity is log replay, not a new
  entry. It does not advance control state or produce writes/history/outcomes.
  Conflicting bytes at an applied identity are corruption, not a business
  rejection. The adapter proves log identity before domain application.

An unapplied control state has no last-entry identity. Its numeric position
accessor returning zero is not proof that log zero was applied. The first entry
must have index zero; only its successful atomic application creates a last
identity. Subsequent entries use checked consecutive indexes and full identity,
digest and stamp replay checks. This preserves the distinction between
`None` and `Some(LogId { index: 0, .. })` in the selected
[OpenRaft 0.9.25 storage contract](https://docs.rs/openraft/0.9.25/openraft/storage/trait.RaftStateMachine.html#tymethod.applied_state).
The [initial membership contract](https://docs.rs/openraft/0.9.25/openraft/docs/cluster_control/cluster_formation/index.html)
and the independently inspected released initialization test both start at zero.

Restoration derives the applied position from the complete restored last-entry
identity and validates that persisted HLC/wall control values dominate its
stamp. The enclosing snapshot/recovery adapter must prove coherent scope,
membership, state, indexes and outcomes; a domain restoration constructor cannot
certify that transaction. An unapplied control state cannot carry any retained
request outcomes, even when the currently queried request is absent.

A matching retained request returns the original accepted/rejected outcome,
original response bytes/time/position and retention deadline. Processing a new
duplicate log entry may advance current control metadata and current applied
position, but never renews the original retention window or acquires another
lease/fence. Different intent preserves the original retained record and
returns an explicit conflict. These positions must remain distinct.

Invalid committed stamp/format metadata is an apply/corruption failure; it
cannot be persisted as an ordinary deterministic workflow rejection and skipped.
Invalid client inputs are rejected before proposal. Valid committed business
precondition failures persist their rejection plus control/applied metadata
atomically. Storage/resource failure does not advance any published checkpoint.

New public mutations prepare explicit execution time from admitted leader
inputs and authoritative group time; client scheduling timestamps are requested
deadlines and do not advance group time. Revalidate the effective execution time
at ordered apply. The selected execution scalar is the nondecreasing group HLC
**physical** component, not the independent wall control maximum. Keep wall
context separate for operations whose source expiry/drift rules use it. The
pure time plan exposes these distinct values for later application
integration. The existing no-request-ID compatibility reducer preserves explicit
source `now_ms` fixtures and its separate duplicate-effect rules; it does not
grant public clients authority over the leader's trusted stamp.

Snapshots/restart preserve both clocks. Disaster restore changes recovery
identity while restoring the monotonic control state and fencing namespace.
Idle expiration progress uses bounded committed ticks; local timer wakeups alone
cannot expire or change authoritative workflows.

## Admission values and identity limits

Types selected for new request namespaces preserve opaque binary bytes:
TenantId maximum 8,192 bytes; GroupId, RecoveryId and RequestId maximum 256
bytes; PartitionId maximum 65,535 bytes. All are nonempty. Group/recovery/request
caps are new protocol resource decisions, not a claim of a legacy wire limit.
Existing RecordId/PartitionKey and legacy command constructors remain stable.
Validating each handle does not certify the composed scope/key fits: validate
the complete actual encoded isolation scope and routed key separately.

Immutable apply configuration carries the exact value/key/batch/command/work
bounds used by every replica. Default value size is 1 MiB and batch count is
1,000, with the source hard maxima above. Default aggregate prepared-command
bound is 16 MiB, a selected initial resource bound; it is not the ordinary
HTTP/TCP encoded request limit. Deployment may configure a reviewed larger
embedded profile within checked arithmetic and workload/resource evidence.
Ordinary transport limits remain their separately defined encoded/decoded caps.
Batch records and children share one item budget. A zero/invalid bound rejects
configuration, rather than disabling protection. No host memory availability
enters a committed business decision.

Policy installation separately uses the source 256 KiB actual encoded snapshot,
snapshot-plus-1,024-byte envelope and 4 MiB snapshot-batch limits. Initial state
cardinality is 16,384, configurable positively through 262,144. This count is a
new immutable resource profile, not a source default. Every state name contains
at least one byte, so the snapshot byte ceiling already gives the conservative
262,144 upper count; actual format overhead makes the real limit smaller.
Source-compatible acceptance profiles needing more than the initial count must
configure it accordingly. Bound installed/patch/final work before cloning and
bind the final typed value to actual private encoded bytes outside the domain.

The represented single-record schema has at most two encoded value occurrences
(payload and result/error); even empty slices count, so byte limits cannot hide
unbounded work. Initial plan-work admission charges four times borrowed variable
bytes plus 1,024 per touched item, using checked arithmetic before cloning.
This is a selected conservative profile for represented copies/history/scalars,
not native layout, measured RSS or an encoded-output certificate. Future schema
extensions must revise the occurrence/copy proof and retain source acceptance.
Actual response bytes require separate outer admission. The snapshot-batch
helper independently caps occurrences at 100,000 as a new finite work profile;
the source has a 4MiB byte cap and may have multiple snapshots per command, so
this occurrence count is not asserted to be a literal source rule.

Admission uses actual borrowed encoded bytes/value slices and item counts,
checked sum/length arithmetic and authoritative placement. Never accept a
caller-provided length as evidence of acquired bytes or authority. Preserve
absent, explicit empty, null and zero according to the selected operation schema.
Opaque external blob references remain rejected until a replicated availability
protocol is implemented; source blob guarantees still require an equivalent
validated port before full parity.

## Verification and deployment gates

Before M1 acceptance, execute independent source-derived admission/model cases
for every boundary and accepted/rejected/request replay/tick/stamped membership/
unstamped membership/log replay class. Demonstrate deterministic equal plans
from identical starting state and committed inputs, unchanged original retained
outcomes, nondecreasing control time, no partial atomic-group writes, arithmetic
overflow rejection and legacy time/lease duplicate compatibility. Root and
architecture/performance peers review the actual implementations.

Before exposing a live service, prepare and test a bounded leader skew/forward-
jump/new-leader admission policy against persisted time and physical lease
safety. This pure ADR does not select an unmeasured clock-skew threshold or
promise physical-time safety from an HLC. Verify idle ticks, leader change,
backward/forward jumps, restart and snapshot/restore with actual adapters.
All transport, storage, cluster and original mandatory profile gates remain
required; unit time plans do not replace them.

## Selected public claim parent migration

Status: selected requirements under
[A-CM1](../reviews/workflow-completion-architecture.md#a-cm1-exact-existing-document-public-claim-migration-decision).
Implementation, source counterpart execution, persistence and platform/profile
acceptance remain open. The pure claim-coordination planner establishes no
governance content, remote credit ownership or public atomic application.

The pinned Elixir source is commit
`66586c596e271408d9184af32eb31cd7218c126e`. Paths below refer to
`apps/ferricstore/lib/ferricstore/`. These inspected source differences require
explicit migration dispositions alongside the original assertions:

1. **One atomic local claim outcome.** `flow/claim_due_api.ex:448` and
   `flow/pipeline_claim_due.ex:261` can execute multiple Router writes during
   reclaim. An error in a later phase does not prove earlier effects absent.
   The Rust public parent inspects retained-request replay once, evaluates all
   phases and effects over one coherent bounded overlay, and publishes one
   final local-group plan. Records, history, correctness indexes, retained
   outcome, control state and checkpoint persist atomically. A work/validation
   failure publishes no prefix writes or fabricated empty success. Refund as
   KnownNoCommit requires authoritative atomic no-commit evidence.
2. **One monotone cursor per original batch.**
   `raft/state_machine/sections/flow_claim_due.ex:269` installs the supplied
   reservation map for each physical entry;
   `raft/state_machine/sections/flow_retention_values.ex:610` consumes IDs when
   binding records. Rust preserves one verified monotone cursor per original
   immutable identified batch across phases and actual bind attempts, including
   source-defined bind-before-skip behavior. A later phase cannot reset the
   cursor or reuse an ID. Refund derives from actual final accepted bindings,
   preserves original reservation order and supports non-prefix used IDs.
   Coalesced filter/allocation planning preserves each original caller's
   separate demand, request and batch authority.
3. **Governance remains authoritative through pipelines.**
   `flow/pipeline_claim_due_command.ex:56` omits governance from its normalized
   grouping identity, and the direct reclaim routing bypasses the ordinary
   spend wrapper. Rust batching cannot bypass complete CURRENT governance
   schema/version/owner checks. A structurally matching projection, value batch
   or Confirmed classification proves none of that authority. Valid explicit
   scope/shard pairs retain their separately validated owner route; partial or
   invalid pairs reject. Different governance identity prevents unsafe
   coalescing. Preserve the distinct ordinary/pipeline phase and quota rules.
4. **Resolve original IDs before fresh spend after unknown.**
   `flow/claim_due_api.ex:534` can auto-lease/retry a spend error;
   `flow/governance/limit_store.ex:149` preserves unknown reservation IDs.
   Rust retains exact original IDs and owner/request identity for authoritative
   inquiry or identified compensation before fresh spend. Same-request lookup
   or equivalent workflow retry does not authorize new credit or reset IDs.
   Cancellation, lost delivery and an absent lookup settle neither earlier
   submission nor external ownership. Local atomic no-commit alone does not
   prove external spend or compensation state. Complete identified-owner and
   outbox protocols establish that separate state before public integration;
   group-local atomicity creates no cross-owner transaction. No fabricated flow
   ID authorizes unused-credit release.

These are selected safety migrations, not claims of source equivalence or
reproduced historical bugs. Retained workflow replay remains before changed
selection/time/policy inputs; current authorization remains before returning
the retained outcome. The [source parity contract](../workflow-test-parity.md)
and [shared-service conformance](../transports/operations-and-conformance.md)
require each affected source identity and assertion to retain its explicit
migration disposition and executed counterpart.

Required controls preserve literal ordinary limit-one/larger reclaim phases,
pipeline per-call ceilings/first-fill order, FIFO timeout/refetch and independent
caller identities. Add late-phase failure after nonempty effects; monotone
cursor/skipped/non-prefix IDs; complete current policy/version/explicit-pair
authority; unknown spend/claim, lost reply and replay after changed inputs;
known atomic no-commit and identified compensation. Real adapter tests inject
persistence failure, restart and crash at every boundary and prove no partial
local writes. Original setup/assertions stay immutable and additional safety
controls remain distinct. Execute the required counterparts and profiles on
BOTH Linux AND macOS; documented requirements and planner GREEN confer no
5858-case coverage or service acceptance.

## Original whole-flow replacement identity and admission

The reviewed A-NEXT-M1-ORIGINAL-FLOW-PARENT1-CREATE1 increment implements the
represented lower replace=true allocator command under existing domain/ports
boundaries. Private FWCP format1/tag8 selects intent registry3. Intent binds the
tenant, exact binary policy key, raw mathematical expected generation and patch;
scope group/recovery, request identity, stamp and current/context/history values
remain separately verified operands. Integer aliases and map permutations have
the same mathematical identity, while list order and field presence remain
distinct. The authenticated sidecar carries captured source row order separately.
Its consistency assumes faithful source capture and supplies no VM-origin proof.

The new frame/intent hard ceiling is96MiB and also obeys supplied decision-byte
admission, whose default is16MiB. Earlier frame/intent registries retain their
existing1MiB limits. Prospective logical work bounds raw occurrences, depth,
visible bytes, compact canonical scratch, every reached read/compare/write and
the actual ports hashes. No caller size, verified mathematical header or prior
payment token bypasses the received account. A single verifier and request gate
precede Fresh; non-Fresh never reads changed policy/context/history inputs.

Fresh reuses one private normalized layout through generation allocation and
direct original-envelope emission. Current indexes are dropped first; the new
parent's263168-span profile reads its own source-valid output without changing
the public4096-span reader. Key, installed bytes and response are real independent
owned data subject to aggregate admission. Physical policy/outcome/control writes
still require the later atomic adapter. Public Policy.set preprocessing/replies,
opaque Other identity, source capture/setup/profile acceptance and independent
Linux execution remain required. Passing567 Rust tests/four compile-fail docs on
macOS establishes this represented component, not full M1 or workflow parity.

## Group transaction and persisted core values

ARCH-NEXT-M2-STATE-CONTRACT1-CREATE1 selects the engine-independent successor.
An exclusive group/recovery transaction supplies shared point reads, true FIFO
heads and an authentic ordered due cursor before consumed commit. Group control
is shared across tenants; records and outcomes additionally bind tenant. The
backend must compare the actual predecessor and affected versions under the same
transaction guard before publishing records, ordering/index/history changes,
new outcomes and control atomically. The immutable changes container validates
mathematical plan consistency; it grants no provenance or durability authority.

Every read/open attempt, including misses, consumes the transaction's cumulative
step allowance. Each inspected candidate is charged before filtering, and the
actual empty end probe consumes a step. Logical byte charges are 512 for control,
512 plus every owned variable byte for materialized records/outcomes, and 128
plus the four binary key components for inspected due keys. Materialized filter
records are charged too. Admission precedes ownership and visible progress;
page reopening refunds nothing. Cursor position includes fully consumed filtered
candidates. BudgetEnd preserves this position and differs from actual RangeEnd.
These limits exclude backend caches/pages, queued jobs and arbitrary caller
clones; they establish no native process memory or CPU guarantee.

Separate BE version1 FWSC/FWSR/FWSO grammars preserve opaque identity bytes,
Option presence and complete original values. FWSC retains both nondecreasing
control time and the exact original last-entry stamp, including backward stamps,
and distinguishes unapplied control from applied index zero. FWSR retains all
core record fields, exact ordering metadata and the explicit latest-event
witness; restoration reuses domain validation, including generated retry due
times above the caller timestamp ceiling. FWSO retains the response once and
derives status through the single ports batch5/response1 parser. Other outcome
families fail closed until separately selected.

ARCH-M2-STATE-POINT-SCANS1-CREATE1 selects immutable borrowed record/outcome
scans before ownership. Record point scans bind the complete expected
group/recovery/tenant/partition/ID; outcomes bind the complete scope/request.
One parsed view feeds both the consuming materializer and the existing owning
decoder, preserving the old wire-only decoder contract without a second wire
parse. Expected query owners are compared and released independently of the
encoded donor. The views carry no payment, storage-origin or authority token.

Each view computes the checked logical charge once: 512 plus every variable
field returned in its owned result. Record group/recovery are excluded because
the returned record does not own them; tenant/partition and every core field
are included. Outcome scope, request and response are all included. Consuming
materialization rejects zero or insufficient ownership caps before its first
field copy. Wire, domain and decoded ownership caps remain distinct. A future
transaction must prospectively charge its cumulative account before invoking
materialization; the borrowed view itself does not mutate that account. Fixed
tariffs and existing identity Vec-to-Box conversion do not certify native layout,
capacity or recoverable allocation behavior.

Each codec takes a positive explicit byte cap. Decode checks actual bounds,
complete framing/EOF, namespace and semantics before constructing owners.
Encode validates and measures before one fallible output reservation. Existing
typed identity conversions retain their allocation behavior. Independent
literal vectors and contract doubles establish only these represented duties.
Engine adoption, real coherent transactions, uncertain-commit fencing/reopen
reconciliation, physical history/index integration, snapshots and native crash
testing on Linux and macOS remain required. KnownAborted requires proof of an
unchanged durable predecessor; Uncertain never authorizes blind retry.

## Physical keys and full history values

ARCH-NEXT-M2-PHYSICAL-FORMATS1-CREATE1 selects pure codecs under storage state,
without selecting an engine. FWPK/version1 separates control, records, outcomes,
due, FIFO and history families. Each opaque component escapes NUL as 00FF and
ends with 0000, preserving binary order and component boundaries. Control binds
group/recovery; other families also bind tenant. Due suffixes preserve inverse
priority, u64 time, ID, partition, state and type. FIFO suffixes preserve lane,
running/queued score, full u128 sequence and ID. History suffixes preserve numeric
event milliseconds followed by the original event ID: equal-time 100-10 precedes
100-2. Canonical unsigned-u64 event IDs are distinct from public cursor limits;
their version must match the value, while occurred_at remains independent.

Decoded ordered keys validate complete framing and expected namespace/selector,
then admit 128 plus raw owned component bytes before one fallible raw-buffer
reservation. The fixed 128 is a logical tariff, not native layout or spare buffer
capacity. Prefixes do not certify complete lanes or authentic range ends. Later
adapters must separately admit encoded seeks, engine bound copies, retained old
positions and old/new overlap within the same authoritative write transaction.

FWSH/version1 preserves full HistoryEntry and optional retry audit, including
absent versus present-empty bytes, both decisions and four backoff kinds. Its
wire cap is explicit and positive. Full borrowed framing/EOF, namespace,
canonical event ID/version and existing retry-policy validation precede copies.
The record codec remains the single event-tag authority. These formats supply no
legacy disk compatibility, public history query, source scenario completion,
physical atomicity or Linux/macOS crash evidence.
