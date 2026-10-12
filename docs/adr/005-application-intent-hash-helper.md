# ADR-005: application comparison grammar and hash dependency

Status: implemented pure preparation infrastructure, policy-set and claim-due
registry entries below, with independent code/resource acceptance for the
represented mapping and limits.
The complete registry, runtime service,
committed codec and cross-transport acceptance remain separate gates.
No first-party dependency arrow or public module visibility changes.

The selected [version-1 grammar](../transports/intent-digest.md) belongs to the
private application intent module. Its public commands helper accepts explicitly
typed arguments plus TenantId and a registry-selected operation; it constructs
the exact five-field comparison root. This mathematical comparison helper
confers neither authorization nor durable apply authority. The eventual shared
service must validate the complete operation schema and authorized tenant before
calling it; accepting arbitrary request maps remains prohibited.

TenantId uses tag06 opaque bytes, preserving binary namespace equality. Version
strings and operation names use tag05. Values retain explicit tags, unique
UTF-8-byte-sorted map keys, caller list order, absence/null/empty distinctions,
full u64 and signed binary64 zero. No current stored policy, clock, generated ID,
transport correlation or credential can change the root on retry. Fixed root/map keys,
headers/counts and the 13-byte prefix all count against byte/value/depth budgets.

Use a bounded flat preparation plan of borrowed byte slices and fixed headers;
cache exact container lengths during the checked pass and stream that same plan
to the hasher. Sorting stores only references. Bound count and key bytes before
sorting allocation; use fallible reservation. Allocation failure remains an
operational preparation error, never a deterministic committed rejection.
Initial limits are 1MiB canonical bytes,16,384 encoded values and depth32; the
root is depth1, every map key counts as a string value, and maximum configured
depth is128 to bound recursive preparation. Larger byte/value profiles remain
explicit immutable configuration. These are new preparation resource defaults,
not invented legacy wire limits or excuses for missing source cases.

Approve sha2 **=0.11.0**, default-features=false, no selected optional features,
only for wf-application. This is the same checksum-pinned release already used
by parity tooling, whose existing default feature policy stays separate. The
release manifest declares Rust1.85, MIT OR Apache-2.0 and safe incremental
Digest/Sha256 APIs. Its current primary documentation is
[sha2 0.11.0](https://docs.rs/sha2/0.11.0/sha2/).

This dependency does contain transitive unsafe. Actual cached release source
uses feature-detected AArch64 SHA2 or x86 SHA/SSE intrinsics with software
fallback; even the unrolled software path uses raw reads of round constants.
Digest and crypto-common forbid unsafe internally; block-buffer/hybrid-array
and cpufeatures use their own unsafe for fixed-layout buffers and detection.
First-party code calls only safe incremental APIs and retains unsafe forbid.
No forced target feature/backend configuration is introduced. This source
review is not a formal proof of all third-party code. Lockfile checksums,
dependency/license/advisory checks and Linux/macOS vectors remain mandatory.

Before encoder implementation, an independent Python struct/hashlib generator
produces full canonical byte/SHA256 vectors for every tag, binary tenant,
UTF-8 sorting, maximum u64, nested order and signed zero. Real compile-red
comparison tests and behavior-red architecture policy controls are recorded in
the completion coordination note. Compiler, strict lint/size, exact-byte/vector,
invalid-input and independent peer resource reviews precede acceptance.

## First policy-set registry entry

A-N19 selects `workflow.policy_set` for the existing typed InstallCommand.
The public application helper receives TenantId, this command and immutable
IntentLimits. It computes comparison identity only; it validates no namespace,
authorization, stored policy or durable submission. Source generation/CAS
business constraints remain distinct from mathematical identity encoding.

In particular, mathematical state mode Clear still encodes null even though
the separately reviewed A-N21 source admission rejects it. Omitted normalized
state mode remains valid; comparing a typed command never certifies installation.
Replacement state Remove also retains null in identity while its separate
reviewed A-N24 source admission rejects Replace+Remove. Patch deletion remains
valid; comparison identity preserves either supplied mode and action.

The args map contains workflow_type as opaque bytes, mode as the string patch
or replace, patch as a present map, and expected_generation as an unsigned
value only when present. Some0 remains present. Patch fields use exact names
version, max_active_ms, retry, retention and states. Unchanged fields are omitted,
Clear is present null and Set preserves the full typed value. Version bytes and
unsigned numeric version stay distinct. Maximum active duration is unsigned.

States is always a present list, including empty, derived from the command's
binary-key-sorted map. Each row has state as bytes and change as null for Remove
or a map for the state patch. State mode, retry and retention use the same
omission/null/set rule; mode Set is the string parallel or fifo. This sorting
normalizes the unordered typed state map, preserving every action and binary
key rather than changing a meaningful caller list's order.

Retry Set is a map of optional max_retries unsigned, backoff map and exhausted_to
bytes. A present empty backoff stays an empty map. Its optional kind is the string
none, fixed, linear or exponential; base_ms, max_ms and jitter_pct are unsigned
when present. Retention Set contains optional ttl_ms and history_max_events as
unsigned values. Empty Set maps, Clear null and omission remain different, as
do every Some0 and absent nested scalar. Static map keys use the existing UTF-8
byte order. No current policy/default, prepared value/generation, migration
high-water, generated input, request ID or digest field enters the root.

Preparation reuses the bounded flat borrowed plan. Before processing state rows,
check their actual count against at least five encoded values per row and the
remaining value budget. Check actual state-key byte sums plus required headers
and fixed keys before retaining per-state segments. Every optional field still
receives actual depth/value/byte checks and fallible metadata reservation. This
creates no complete owned normalized tree, cloned policy or canonical frame.
Logical limits do not certify native memory or universal allocation fallibility.

Intent's defaults of 1MiB, 16,384 values and depth32 are distinct from PolicyLimits
and policy snapshot limits. Eventual advertised profiles must accommodate their
required source scenarios explicitly; these mathematical defaults do not claim
to admit every policy valid under a different profile. Unsupported governance
or indexed fields require an explicit outer-schema rejection until implemented.
Seven independent policy-set vectors preceded the encoder, with missing-API
compile RED and eight actual rejecting-stub policy test failures. That policy-entry
acceptance tree's fourteen application tests passed, including all seven exact bytes/hashes and
one-less byte/value/depth limits, generic-slot equivalence, every represented
optional field and state action, binary ordering and borrowed-owner checks.
The eighteen generic and seven committed-policy fixtures remain unchanged.
Architecture and performance independently passed all fourteen with unchanged
scoped inputs and accepted the represented mapping/resource contract. This
single entry does not complete the operation registry or cross-transport gates.

## Implemented public claim-due registry entry

A-N46 selects `workflow.claim_due` as a mathematical caller comparison
predecessor. Implementation and independent acceptance pass under A-N49. The public
application helper receives TenantId, a short-borrowed NormalizedClaimDue and
IntentLimits. Its caller normalizer owns only three reference spines, borrowing
every binary leaf, and reads no policy, definition, clock, storage or membership.
All raw list counts and complete occurrence bytes pass before aliases/dedup or
spine allocation. Default preparation limits are100,000 occurrences per list,
1MiB per component and16MiB aggregate, independently from IntentLimits and the
source64 unique/product/composed-key/authority admission gates.

Fixed equivalent defaults are lease_ms30000, limit1, reclaim_expired=true,
reclaim_ratio25, priority absent and reply records. Payload is the explicit
payload override, otherwise explicit full, otherwise reply==records. State
names keep their last occurrence/order and binary owner; partition/value names
keep their first. Scalar ASCII partition aliases map to auto/any/global; list
aliases remain literal names. State ANY aliases set the separate any flag.
Invalid mixed ANY+names, explicit empty lists/names and zero/MAX scalars remain
mathematically comparable. Neither Any nor Global authorizes a shared-tenant
claim; that source scope rejection remains a separate required admission.

Eleven mandatory args use these exact types: workflow_type and worker Bytes;
states map{any:Bool,names:List<Bytes>}; partitions map{mode:String,keys:List<Bytes>};
lease_ms,limit,reclaim_ratio Unsigned; reclaim_expired Bool; reply_mode String;
payload Bool; values map{mode:String,names:List<Bytes>}. Five optional args are
priority,payload_max_bytes,governance_shard_id,now_ms Unsigned and
governance_limit_scope Bytes. Absent options are omitted; present0 remains.
Partition modes are auto/any/global/only; value modes are omitted/all/only.
Reply strings are records,jobs,jobs_compact,jobs_compact_attrs,
jobs_compact_state,jobs_compact_state_attrs. Static keys retain UTF8 byte order;
names remain normalized caller order and opaque bytes.

Source requested_now_ms becomes optional caller now_ms identity only: absence,
zero and full unsigned values differ; it never supplies committed clock authority
or resolves to current time. Future wire permission/trusted legacy mapping still
requires review. Policy-dependent payload_max_bytes absence remains distinct
from every explicit cap, including0 and65536. Block wait is attempt semantics
and excluded, together with request/transport IDs, credentials, receive/routing
hints, committed stamp/log/digest, selected IDs/tokens/fences, token version,
ordinals/resume and server scan/head/work budgets. No stored policy or generated
preparation value replaces a caller absence.

Before any plan segment or leaf loop, checked arithmetic prechecks the exact
encoded value count45+2*k+sum(the three normalized name lengths), with k the five
optional presences. Full CanonicalPlan counting remains authoritative afterward.
Each list checks count, nine-byte value headers, eight-byte count and complete
leaf byte sums before per-name descriptors. Dedup starts empty output vectors,
using a membership insert once and fallible growth only for retained originals;
no raw-count output reservation, fourth spine, cloned leaf or Box is selected.
These bounds and requested growth are not native capacity/RSS/CPU guarantees.

Twelve independent byte/hash fixtures preceded implementation, with actual
missing-interface RED and passing exact/one-less byte/value/depth controls.
The current application owner passes20 controls and the domain/types owner188
plus one compile-fail doctest; strict Clippy/docs/formatting pass. Architecture
and performance each independently passed8 caller,20 application and1 workspace
control with unchanged828 and826 input bindings respectively. Actual reciprocal
source/resource reads accept this represented mapping. The caller API shares
the array/leaf lifetime, so original input arrays may coexist with normalized
spines; owning new spines does not prove those arrays were freed.
Returned digest versus independent full-byte
fixture hash does not claim direct typed byte emission. Full source admission,
ordinary versus pipeline reclaim scheduling, one-gate atomic parent composition,
timeout/governance/lineage/wait/projection behavior, original profiles and
durability/transport/service gates remain open. No coverage accounting advances
from this entry or its supplemental controls alone.

## Normalized complete replacement intent (A-M1-OUTER2-I1)

Status: the named Stage I creation and tests-first contract has reciprocal
architecture/performance approval. This section records the selected boundary;
authored-body, RED/GREEN, strict and host acceptance remain separate evidence.

The new generation-free Complete-Replace intent version2 belongs to private
`wf-ports::log::complete_policy`. It uses the existing exact ports sha2=0.11.0,
default-features=false and no optional features. Its closed public entry receives
an owned domain CompletePolicyReplaceInput and the caller's actual mutable
PolicySchemaCharge, performs the selected domain preparation, then streams the
accepted owner's actual fields into SHA256. Public Caller getters expose content
and immutable cap copies; they supply no charge identity, paid credit, hash
correspondence, authorization, storage or installation authority. No public
operation hashes an externally admitted Caller.

Comparison bytes start with the 30-byte
`WF-INTENT\0COMPLETE-REPLACE\0V2\0` prefix, then length-prefixed tenant, optional
expected generation and every known complete-policy field in canonical V2 order.
RAW lengths and collection counts use u32 big-endian; numeric scalars retain
their explicit big-endian widths. Raw-byte map order and normalized list order
remain exact. None, supplied empty groups/lists, false and zero remain distinct.
Assigned generation, FWPS/FWPV wrappers, group, recovery, request and committed
time are excluded. This branch performs no normalization, deduplication or merge.
It preserves representable business-invalid input for later policy admission.

The private hasher feeds fields directly to SHA in one interleaved traversal,
with checked running length and no intent-output Vec or reference/sorting spine.
Domain measurement and separate prospective stream/SHA work are conservative
logical charges; their represented-byte coefficients are not a claim of three
physical payload scans or native copy cost. Full frozen literal fixtures precede
codec output. Tests compare their independently computed digests and complete
returned content, with source review of the actual field stream. Ownership
addresses add no allocator, capacity, timing or whole-process memory guarantee.

Application's existing V1 registry, grammar, helpers and exact hash remain
unchanged. ClientIntent::new remains version1-only in Stage I; a private intent2
constructor and prepared framing require the separately selected Stage F.
Original raw ETF/reference hashing, aliases, customer integers, public retry
semantics, coherent authoritative installation and all original profiles remain
acceptance work. Current Linux and macOS executions and matched native
allocation/copy/capacity/live-peak/latency evidence are independently required.

## Original normalized core-Patch intent (A-NEXT1-CONSULT)

Status: this records the selected Stage I creation and tests-first boundary.
Authored source, RED/GREEN, strict checks and host results remain separate.

The core-Patch branch owns the caller's original normalized partial command.
CompleteCorePolicyPatchRequestInput moves scope, request, committed stamp and
CompleteCorePolicyPatchInput. CompleteCorePolicyPatchCaller retains that input
and immutable schema/admission cap copies. The closed wf-ports::log preparation
entry admits raw owned input on its actual received PolicySchemaCharge, hashes
that same immutable Caller privately, then moves Caller and digest into Intent.
No public accepted-Caller hash, debit, credit, mutable accessor or decomposition
API is selected. The owners and private measured view have no derives.

Identity begins with the exact 33-byte WF-INTENT\0COMPLETE-CORE-PATCH\0V2\0
prefix. It includes tenant, original optional guard, workflow type and every
original core Patch field. Unchanged, Clear and Set-empty differ, as do absent
and present-empty partial Backoff, Remove and Patch, zero and maximum
scalars. No omitted component is expanded to a default. Current policy,
migration high-water, captured retention context, resulting VALUE/generation,
group, recovery, request, stamp and log position are excluded. Distinct partial
commands remain distinct even if a particular current policy yields equal
results. An identical partial command retains identity across changed current
state and must be remerged later against coherent ordered authority.

Domain remains synchronous std/wf-types-only and measures actual immutable
operands without SHA or effects. Ports reuses its existing exact sha2=0.11.0,
default-features=false/no optional features dependency. Only the existing
private IntentHasher and its ten methods become pub(super); fields, bodies,
infallible finish and all Complete-Replace behavior remain exact. The actual
stream is one interleaved field-to-SHA traversal, with no intent-output Vec or
reference/sorting spine. Logical measurement, stream and hash coefficients do
not describe three physical scans or native copy/allocation cost.

The full grammar and same-account schedule are recorded in ADR006. Complete
independent bytes/digests and equations precede six readable tests, missing-API
RED, a separately selected rejecting predecessor, behavior RED and production.
All current Complete-Replace controls and goldens remain unchanged. Architecture
checks retain exact hash graph constraints, test the new domain descendant's
value paths and effect refusal, and reject intent leakage through the client
facade. No dependency or source-checker allowance changes.

This Stage I selection does not create ClientIntent2, command framing, restore,
Stage F, ONE retained gate or install authority. Original partial-command replay
and Fresh-only coherent remerge, atomic namespace/policy/catalogue/high-water/
outcome/checkpoint publication, raw/ETF/source-public parity, full profiles and
independent Linux AND macOS execution/performance remain required.
