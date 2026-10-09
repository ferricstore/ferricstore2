# M1 runtime optimization review

Owner: optimization reviewer (`/root/m1_optimizations`), GPT-6.1-sol / max,
2026-10-09. Scope is this review file; implementation owners/coordinator retain
their disjoint production write sets. No production edits, commits, pushes,
dependencies, lints or architecture allowlist changes are authorized here.

## Review contract, published before suggestions

Review the actual `wf-types` and `wf-domain` runtime code for immutable byte
storage, allocation/clone costs, enum layout, bounded sorting/collections and
selector record comparisons. Preserve public API paths, source bytes and
ordering, absent-versus-empty values, value determinism, admission bounds,
borrowed input preservation and the transactional architecture. No first-party
unsafe or runtime/effect dependency is introduced. `Vec` remains appropriate
for growable builders; converting existing owned vectors to boxed slices may
shrink/reallocate, and a smaller handle does not establish cheaper deep clones.

Findings below distinguish actual code observations, measured local evidence,
proposed changes and unresolved measurements. Representation/layout evidence
does not imply a durable runtime or a production throughput/latency guarantee.
Accepted changes remain owner/coordinator work with retained semantic tests and
independent re-review. A reciprocal owner review and actual independent review
of at least one peer implementation will be recorded before completion.

## Initial actual-code observations

Read `AGENTS.md`, `README.md`, architecture, ordered implementation plan, parity
contract, stopped handoff (the user has now resumed), current coordination, and
the actual retry/claim/parity review records. Read the current primitive and
workflow command/model files. The following observations are from that code,
before implementation suggestions:

- `wf-types/src/identity.rs`: private `RecordId` and `PartitionKey` each retain
  `Vec<u8>` and expose only `&[u8]`. Neither has a post-construction grow path.
  `RecordId::new(Vec<u8>)` currently preserves caller capacity; partition encoding
  already reserves the exact computed length before extending a builder.
- The same partition encoder computes each decimal length with
  `component.len().to_string()` once during sizing and again during output.
  These are two owned decimal strings per component even though the output is
  directly appendable ASCII. Overflow/`AllocationFailed` remain explicit errors.
- `wf-domain/src/workflow.rs`: the private immutable record stores type/state,
  optional run state/payload/result/error as vectors; getters borrow slices.
  Public command structs intentionally expose owned `Vec<u8>` fields, so changing
  their representation would change construction APIs.

## Actual local measurement, before proposals

Coordinator approved the named tooling-only project
`/private/tmp/ferricstore2-m1-optimizations-probe`. It has only path dependencies
on the two actual runtime crates, forbids unsafe, and changes no repository
configuration. Executed Rust 1.99.0 release/locked/offline on local aarch64
(64-bit pointers), exit 0; first raw output is
`/private/tmp/ferricstore2-m1-optimizations-first.log`. Five samples per timing,
median elapsed nanoseconds per operation, with `black_box` and independent
owned values. This is a local focused cost probe under concurrent development,
not an allocator counter or a production benchmark. Three unused-result probe
warnings were subsequently corrected without changing timed operations; a
warning-free repeat is pending.

| Representation | Current measured bytes | Box shadow bytes |
|---|---:|---:|
| `Vec<u8>` / `Box<[u8]>` | 24 | 16 |
| Optional owned byte slice | 24 | 16 |
| `RecordId`, `PartitionKey`, `LeaseToken` handle | 24 each | 16 each |
| Lease, retaining public `worker: Vec<u8>` | 56 | 48 |
| Workflow record | 304 | 240 |

The record shadow repeats all actual fields; its all-Vec layout is also 304,
providing a layout control. The 240-byte alternative boxes its private identity,
type/state/run-state/payload/result/error bytes plus private lease token, while
retaining the public lease worker vector. Actual aggregate layout is compiler/
target-specific; no stable ABI or serializer change is inferred. Other actual
layouts: `Operation` 160, command 192, record write 320, transition plan 568,
history entry 240, `ClaimRecord` 48, `DueOrder` 80, selected claim 32,
continuation 304 and selection result 352 bytes.

Conversion probe: 32/32 and 4096/4096 byte length/capacity reused the address;
32/128 and 4096/16384 did not. Converted vectors report capacity equal to length
after round-trip from the box. Create/convert/drop measured 40 versus 23 ns for
the 32/128 case, and 453 versus 296 ns for 4096/16384. These specific observations
do not prove allocator-call counts, RSS savings or a general latency bound.
Rust documents that `into_boxed_slice` discards spare capacity and that exact
length/capacity conversions need no reallocation; an allocator may provide more
capacity than requested. See [the official Vec guarantees and conversion](https://doc.rust-lang.org/std/vec/struct.Vec.html#method.into_boxed_slice).

Deep clone measurements (Vec / Box, ns): 32 bytes 16/16; 4096 bytes 60/91;
65535 bytes 669/716; 1 MiB 12894/14773. The result establishes **no clone speed
benefit** for boxes in this run. Both remain owned deep copies.

Partition encoding of 8-byte components (actual / safe stack-decimal shadow,
ns): 1 component 49/22; 8 components 280/52; 64 components 2040/360. The shadow
retains the checked-length fold, exact fallible reserve and growable Vec builder;
only decimal digit counting/emission changes. Exact byte equality passed for
empty/binary/delimiter components, length boundaries 0/1/9/10/99/100/999/1000/
65535, and `usize::MAX` decimal formatting. This removes the two decimal
String constructions per component; the resulting encoded byte buffer still
allocates. This probe supplements the repository's retained semantic tests and
is not a new claimed TDD cycle.

Selector at 128 consumed parallel rows (shared canonical records / equal
distinct records, microseconds): empty payload 22.8/23.4; 4 KiB 38.4/37.9;
64 KiB 191/200; 1 MiB 2477/2602. Canonical lookup's derived `ClaimRecord`
equality compares the complete record, so work includes admitted value bytes
in addition to bounded row counts. Even the same-record case scales here.
Payload-size admission is explicitly outside the current reducer/selector; no
new byte limit or weakened equality is silently introduced by this review.

## Ranked proposals and exact scope

1. **O1 — recommend now: allocation-free decimal length formatting.**
   `crates/wf-types/src/identity.rs`, `PartitionKey::encode_components`: replace
   both owned `to_string()` calls with pure digit counting and ASCII emission
   through a fixed stack buffer. Keep the Vec builder, checked additions,
   exact fallible reserve, prefix and all component bytes. For supported
   32/64-bit targets a 20-byte buffer covers `usize::MAX`; avoid unchecked
   narrowing and new `std::io`/formatting adapters. Retain all partition key
   equality, boundary, collision and source-vector tests. The focus probe shows
   a meaningful improvement without changing public ownership contracts.
2. **O2 — recommend narrow private wrapper boxes, with costs explicit.**
   `crates/wf-types/src/identity.rs` (`RecordId`) and
   `crates/wf-domain/src/lease.rs` (`LeaseToken`): private `Vec<u8>` becomes
   `Box<[u8]>` after validation, retaining `new(Vec<u8>)`, `as_bytes()` and all
   trait semantics. This saves one machine word per handle locally and drops
   retained caller spare capacity, at the measured possible conversion cost.
   It is a storage-representation improvement, not a clone-speed claim. Retain
   binary/empty/max bounds, ordering/hashing and lease/fencing tests.
   `PartitionKey` boxing is **not yet recommended**: its fallible reserve can
   receive capacity beyond the requested length, so finishing with infallible
   `into_boxed_slice` can introduce a shrink allocation after the reported
   `AllocationFailed` boundary. Keep its builder/storage Vec unless that
   constructor error-contract tradeoff is explicitly resolved.
3. **O3 — defer broad record boxing pending mutation-path measurement.**
   `wf-domain/src/workflow.rs` and its create/lifecycle/retry/plan modules:
   private record bytes could reduce the measured 304-byte handle to 240, but
   reducers currently clone then replace fields and use Vec `clone_from`
   reuse. Box conversion/length-changing replacement may add allocator work;
   the byte clone probe is not favorable. Public command, lease worker,
   history and retry-policy Vec fields must remain unchanged. Measure actual
   reducer workloads before accepting this larger change.

These are proposals for coordinator/owner implementation only. No runtime file
has been changed by this reviewer. Further retry-resolution/clone observations,
remaining measurement gaps and independent/reciprocal review follow below.

## Warning-free repeat and additional ranked observation

Before accepted production edits, the expanded probe compiled without warnings
and exited 0 using the same pinned release/locked/offline command. Raw output:
`/private/tmp/ferricstore2-m1-optimizations-repeat.log`. Actual layouts remain
identical to the initial run. Actual / stack shadow decimal timing (ns) repeats
at 42/18, 254/48 and 1856/289 for 1/8/64 eight-byte components. Separate
conversion/deep-clone timings vary between runs and continue to establish no
general Box speed benefit. Full created-record clone (actual / private-Box
shadow, ns) at payload lengths 32/4096/65535/1 MiB: 62/57, 103/100, 737/720,
12535/12749. This approximate clone-only shadow does not measure state-length
replacement, lease removal, populated result/error or real reducer mixtures;
broad boxing therefore remains deferred.

**O4 — follow-up candidate, below O1/O2: borrow retry target through merge and
allocate the final owned target once.** Actual `wf-domain/src/policy.rs`
`resolve` first allocates the default `failed` target and then performs up to
three `Vec::clone_from` replacements across type/state/command layers. Numeric
fields are Copy; the final exhaustion target could be chosen by borrowing layer
slices, then materialized as the existing public `RetryPolicy.exhausted_to: Vec`
once. A private resolved-view/helper can retain command-validation-first,
unrelated-type rejection, per-field precedence and shared source defaults.
No installed/public value type or public path needs to change.

The warning-free probe mirrors the same lookup, validation and four-layer data,
and asserts equality of actual/shadow results. With target lengths 6/6/6/6,
actual resolution is 30 ns versus 21 ns for borrowed overlay; with growing
lengths 6/10/40/120, 103 versus 20 ns. This is a favorable focused result,
especially for replaced long targets; it does not characterize real policy
length distribution. The exact allocation-call count was not intercepted.
Recommendation is a later narrowly owned follow-up with retained precedence/
numeric/default/backoff/no-op tests; O1/O2 acceptance does not implicitly
authorize O4. Another lower-priority observation is that some reducers clone a
whole record before `next_version` rejects overflow; checking that pure scalar
first would avoid error-path work. Replacing clone-then-discard lease/state/
payload fields requires broader mutation construction review and is deferred.

Coordinator read the measured proposals and **accepted O1 and narrow O2 only**.
Coordinator will edit only `identity.rs` and `lease.rs`. PartitionKey remains
Vec, public Vec fields/full canonical equality stay, and broad record boxing
is deferred. Final accepted-code independent review and remeasurement follow
after coordinator publication.

## Canonical selector review and rejected alternatives

Actually read `claim.rs` and all five child modules. All FIFO heads are obtained
before merging, with <=4096 direct reads; <=16384 merged rows and <=1000 output
records are bounded separately. The frontier is an already-reserved Vec sorted
in place by the specified total key, and the due map is borrowed/ranged rather
than rebuilt. Full-value canonical checks preserve stale/conflicting record and
metadata rejection on every continuation page. `BTreeSet` duplicate checks add
bounded allocation/comparison work; no measured replacement justifies a change.

Claim owner reciprocally read the initial review and explicitly confirmed that
ID/version is not a sufficient content-coherence certificate. **Rejected:**
pointer acceptance, ID/version-only equality, a prefix candidate list, ignoring
FIFO heads under a smaller scan budget, and public Vec-to-Box API changes.
All would change or weaken current contracts. Do not remove required full-value
comparison merely because it costs time.

A future explicitly amended index API could store scoped identity plus ordering/
revision metadata, resolve to canonical `RecordIndex` values, and always return
the canonical record. This avoids index-row record-content comparisons by
construction rather than trusting a stale row's record. Claim owner proposed
this as future representation work requiring stale-content/order/page regressions
and a contract amendment; it is not an accepted M1 production change. Current
index values borrow records, so they do not themselves duplicate payload buffers.

**Rejected/deferred:** Box every vector; box command enum variants; Arc/shared
payload ownership; SmallVec/custom allocators; hash-based unordered selection;
changing source admission bounds; and removing bounded sorting/dedup checks.
Current operation layout alone does not justify an extra allocation per command,
and no allocation/throughput evidence supports those alternatives. Growable
encoding/output/frontier builders remain vectors.

## Independent peer verification

Read the actual claim limits/continuation regressions and compared them with
the reviewed selector behavior. Independently executed all four actual claim
targets with Rust 1.99.0, locked/offline and isolated target directory:
**34 passed (8 selection + 5 FIFO lifecycle + 8 lane + 13 limits), zero failed/
ignored**, exit 0. Saved
`/private/tmp/ferricstore2-m1-optimizations-claim-independent.log`. This covers
the represented pure subset and confirms retained deep-head/order/reconstructed
cursor/current-record protections; it does not prove durable authority indexes
or full workflow parity.

Completed independent source-lineage review: read all four actual claim test
files, complete pinned `flow/fifo_lane.ex` and `flow_fifo_lane.ex`, complete
`flow/fifo_lane_test.exs` and the original invariant cold/deep/global-order cases
at lines 510-624. Exact u128 big-endian bytes, running score before queued,
logical-state lane identity, terminal omission, complete direct heads and due/id
merge agree with the represented Rust subset. The 80-follower fixtures preserve
the head truth rather than rely on a bounded follower window. Native physical
keys/colocation, installation, index transaction behavior, automatic reclaim
and public lease response fields remain explicit omissions. No new concrete
claim behavior blocker was found.

## Accepted implementation closure and reciprocal review

After coordinator publication, actually re-read complete
`crates/wf-types/src/identity.rs` and `crates/wf-domain/src/lease.rs`. O1 changes
only decimal counting/emission; zero emits `0`, forward digits are appended
without normalization, and the original checked sizing/reserve/error paths
remain. The root buffer `[u8; size_of::<usize>() * 3]` has a portable bound:
for B bytes, `usize::MAX < 256^B < 1000^B = 10^(3B)`, so at most 3B decimal
digits fit. The modulo is always 0..9; each loop consumes one remaining decimal
digit before indexing. This requires neither unsafe nor an unchecked narrowing
conversion. The shadow's fixed 20-byte buffer is only a tooling comparison;
accepted runtime uses the portable bound.

O2 converts only validated `RecordId` and `LeaseToken` inputs to private boxes.
Original binary/nonempty/max identity errors precede conversion; same public
Vec-input constructors/slice accessors and owned clone/equality/order/hash
semantics remain. Public command/history/policy/worker vectors are unchanged.
PartitionKey retains its Vec and explicit fallible reserve. No pointer-based
selection acceptance, new domain effect/dependency or shared ownership appears.

Independently executed the actual accepted code:

```text
cargo +1.99.0 test -p wf-types -p wf-domain --locked --offline --target-dir /private/tmp/ferricstore2-m1-optimizations-domain-target
```

**99 retained tests passed (19 types + 80 domain), zero failures/ignored**, exit
0, `/private/tmp/ferricstore2-m1-optimizations-final-types-domain.log`. The type
tests retain all three frozen source partition vectors, decimal/binary/empty
and 65535/65536 boundaries, exact identity errors/order/clone and 30,783
collision-free component lists. Domain tests retain all claim/retry/fencing/
lease/absent-versus-empty observations. No implementation-mirroring repository
test or new TDD claim was added for these representation edits.

The approved tooling `identity_semantics` binary additionally compares current
RecordId hashing with the former Vec-wrapper derivation on binary/max-length
bytes and a caller vector with spare capacity; exact bytes/hash/clone equality
passed without warnings, exit 0. Saved
`/private/tmp/ferricstore2-m1-optimizations-identity-semantics.log`. This verifies
local trait behavior, not a stable cross-version hash encoding contract.

Repeated the same actual release cost/layout probe, warning-free, exit 0:

```text
cargo +1.99.0 run --release --locked --offline --manifest-path /private/tmp/ferricstore2-m1-optimizations-probe/Cargo.toml --bin ferricstore2-m1-optimizations-probe
```

Final raw output: `/private/tmp/ferricstore2-m1-optimizations-final.log`.

| Actual runtime type | Before bytes | After bytes |
|---|---:|---:|
| RecordId / LeaseToken | 24 each | 16 each |
| LeaseGuard | 32 | 24 |
| Lease | 56 | 48 |
| WorkflowRecord | 304 | 288 |
| Operation | 160 | 152 |
| CommittedCommand | 192 | 176 |
| RecordWrite | 320 | 304 |
| TransitionPlan | 568 | 552 |

PartitionKey, history and all claim selector/key/cursor/result layouts retain
their previous measured sizes. Final shadow Vec control is 296 because it now
uses the live boxed-token Lease; the before-change 304-byte control is retained
in both baseline logs and is the valid initial layout comparison.

Actual partition encoding before/after (ns, 1/8/64 eight-byte components):
**42 -> 19, 254 -> 52, 1856 -> 297**. Actual final results match the stack shadow
costs (19/49/298) closely and exact binary/boundary bytes continue to pass. This
supports the focused O1 result. Box conversion/clone timings still vary and do
not establish a clone-speed or production throughput improvement.

Reciprocal review completed through actual shared-file reads: claim owner read
the concrete proposals, probe source/raw results and accepted runtime code and
recorded its findings in `workflow-m1-claim.md`; this reviewer re-read that
actual note. Claim owner independently ran 99 post-change retained tests,
workspace architecture and strict all-target type/domain Clippy green, with no
suppression or blocker. Those are the peer's separately recorded commands,
not claimed as this reviewer's execution. This reviewer independently reviewed
claim implementation/source tests and ran its 34 cases, then re-ran all 99
tests against the accepted root edits. **O1/O2 are closed with no remaining
implementation blocker.**

## Remaining measurement and implementation limits

Only local aarch64/64-bit Rust 1.99.0 was measured. Cross-target layouts,
allocator-call counts/RSS, steady/cold cache behavior, real key/payload/policy
distributions, exact reducer mixtures, tail latency and concurrent runtime
throughput remain unmeasured. Timings use local elapsed medians under concurrent
development and are not confidence intervals or service guarantees. Boxed
constructors may shrink/reallocate caller spare capacity; deep clones continue
to copy bytes. No custom allocator/unsafe interception was used.

PartitionKey and broad record boxing remain deferred. O4 retry-target overlay
is a measured unaccepted future candidate, not a correctness defect or hidden
production edit. Canonical-ID index rows require explicit future API/invariant
review; full record equality remains mandatory today. This review wrote only
its own repository note and approved temporary tooling. Architecture, source
acceptance boundaries, durability gates, M1 incompleteness and fail-closed full
workflow parity remain intact. Ready for coordinator integration and available
for follow-up re-review.

## Additional final publication review: C-P1/C-P2

Coordinator requested an additional independent review after the first full
saved-tree green. This follow-up is limited to actual parity-source provenance
and source/Rust assertion-pair corrections; the accepted O1/O2 runtime review
remains closed. No peer/runtime/shared edits or arbitrary macro expansion are
authorized. Findings, inspected proof/code, adversarial commands and closure
will be recorded here and communicated directly to the owners/coordinator.

Read the current parity/claim/coordinator notes and exact unchanged root
`/private/tmp/ferricstore2-m1-origin-macro-call-repro.exs` and `.log`.
The actual compiled synthetic body reports `:second` after
`require AliasBinder; AliasBinder.bind()`, while the old parser records the
`first` declaration from `Review.First`. The earlier-use owner reproduction
is also concrete wrong-link evidence. Read the current actual parser and its
three new owner regressions; correction/proof publication is awaited before
claiming final acceptance. Unknown prior uses/imports/requires/macro calls
must stay conservative; any ExUnit scaffolding exception must bind exactly
inspected source/provenance, not rely on a trusted module name alone.

Read the root's additional inherited top-level fixture/log
`/private/tmp/ferricstore2-m1-origin-top-macro-repro.{exs,log}`: a prior
file-level `require`/macro call produces the same actual `:second` versus
wrongly recovered `first`. Final review therefore includes both file and module
environments. This is concrete synthetic evidence, not a claim about an actual
pinned consumer's runtime binding.

Independently confirmed C-P2 against the actual preserved
`flow/invariant_test.exs:140-197`, generator entry and complete Rust
`retry_preserves_lane_blocking_until_due_and_terminal_exit` function. Source
156 is empty-at-2050 and belongs to Rust direct assertion 0; source 165/174
are singleton-at-2100 and retained first ID, both represented by exact Rust
singleton/action vector assertion 1; source 184/193 are the second job after
completion at 2201, both represented by vector assertion 2. Current generator
still paired 165 with 0 at this read, so no owner correction was yet certified.
Initial claim/installed policy/durable response omissions remain unchanged.

Coordinator relayed and accepted the owner's revised v2 contract: **no ExUnit
scaffolding proof/name whitelist is admitted**. Unknown ordered in-module and
inherited top-level effects remain unresolved, including prior `ExUnit.Case`
or `Code.require_file` calls whose safety has not been proved here. Lower macro
recovery counts are accepted; all raw cases remain accounted for and the
source-derived Rust assertions remain separate partial evidence. This review
agrees with that bounded choice and does not demand a scaffold proof or complete
static Elixir analysis. Actual v2 code/artifacts and independent closure are
still awaited at this point.

After the owner published its actual v2 contract/code, read the ordered
`safe_prefix?`/inert grammar and synthetic AST tests. The consumer's inherited
file prefix and preceding module nodes are both checked before accepting a
`use`; unknown calls/uses/require/import/nested forms remain unresolved. Literal
attributes and direct literal aliases retain the restricted parse-only path;
no ExUnit or source-loading name whitelist is added.

Independent actual `elixir scripts/tests/runtime-reconciliation-test.exs`
exited **0**, **13 tests, zero failures**, seed 49844, saved
`/private/tmp/ferricstore2-m1-optimizations-origin-v2-suite.log`. The suite's
deliberately raising original test-body string remains inert. Also reran all
three existing compiled synthetic fixtures **without editing their source**:
prior use, in-module require/macro call and inherited file-level require/macro
call. Each actual body still reports `:second`, and each recovered origin is
now `{:unresolved, ...}`. Their original diagnostics exit **1** because they
exit 0 only on the former wrong-link reproduction; these exits are expected
absence-of-wrong-link observations, not falsely reported passing acceptance
commands. Independent logs are
`/private/tmp/ferricstore2-m1-optimizations-origin-{use,call,top}-recheck.log`.
Tested parser/suite digests are retained in
`/private/tmp/ferricstore2-m1-optimizations-origin-v2-tested.sha256`.
Final Rust rule/environment admission, regenerated artifacts/diagnostics and
C-P2 ledger closure remain pending; this is a parser recheck, not full parity.

Coordinator subsequently found a concrete **section-definition environment**
variant in that intermediate v2. Read its exact compiled fixture/log
`/private/tmp/ferricstore2-m1-origin-source-env-repro.{exs,log}`. Unknown inherited
file-level macro rebinding changes which module the first `__using__` section
actually defines; the consumer has only the named section `use`, and the section
itself supplies ExUnit scaffolding. Actual body is `:second`, but section indexing
still names the first AST module literally and falsely recovers `first`.
`module_index` guarded consumer uses but did not yet guard section extraction
under the same inherited/prefix environment. Rust's all-unproved section rule
protects current artifacts, but does not make that matcher/proposal claim sound.
Owner was notified by coordinator to apply the finite conservative check to
section definitions too and record an observed regression. Consumer-only
rechecks above remain valid partial results; full C-P1 closure is held for this
actual additional wrong-link correction and independent unchanged-fixture run.

Read the actual finite correction: `__using__` section extraction now requires
the same `safe_prefix?(inherited ++ prefix)` predicate as a consumer use. The
owner added both inherited section-file and preceding section-module effects
regressions; no macro execution/expansion or scaffold exception was introduced.
Independently ran the updated suite: **15 tests, zero failures**, exit **0**,
seed 900794, saved
`/private/tmp/ferricstore2-m1-optimizations-origin-v2-final-suite.log`.
Reran all four original compiled fixtures unchanged against this parser,
including the section-definition variant: each actual body is `:second` and
each recovered origin is unresolved. As before, their unchanged diagnostics
exit **1** precisely because the old wrong-link reproduction is absent.
Logs are `/private/tmp/ferricstore2-m1-optimizations-origin-{use,call,top,source-env}-final-recheck.log`;
tested source/suite digests are in
`/private/tmp/ferricstore2-m1-optimizations-origin-v2-final-tested.sha256`.
This closes the observed parser wrong-link variants independently; final Rust
admission/artifact and C-P2 publication checks are still reviewed separately.

Read the owner's actual additional RED logs and final published note: the two
section-environment controls formerly returned accepted origins (**15 tests,
two failures**), and restoring a prior hash-consistent origin formerly passed
the Rust validator (**one regression failed**). The C-P2 audit log shows exactly
the old singleton-at-2100/empty-at-2050 mismatch. These are the owner's retained
pre-correction observations, distinct from this reviewer's following execution.

Independently read the final v2 producer, JSON schema, `macro_origin.rs`, all
seven schema-2 adversarial tests, review-only prior-origin fixture and current
collection/progress guide. Both producer and Rust admit no invented scaffold
decision. All **90** section reviews are `unproved`; Rust rejects an origin
citing one before accepting its anchors. The restoration regression uses a real
former row with internally consistent raw identity/complements, rather than
merely a malformed schema. Unknown-rule/scaffold status, requirements/profiles,
tag/duplicate identity, supplemental review hash and dropped unmatched
complement controls remain represented. No name-only ExUnit/loader whitelist
or source macro expansion was added.

Independent locked/offline Rust **`wf-parity-tests`: 45 passed, zero failed or
ignored** (22 library, 6 CLI, 10 progress, 7 schema-2), exit **0**, isolated target
`/private/tmp/ferricstore2-m1-optimizations-parity-target`, saved
`/private/tmp/ferricstore2-m1-optimizations-parity-v2-tests.log`.
Independent `check-progress` and `check-reference` both exit **0**; current
progress is **5,858 frozen declarations, 37 source-derived declarations, 38
counterpart records, 124 assertion links; 5,903 raw registrations, 4,222 linked
cases, 1,681 unmatched runtime identities, 1,655 unresolved declarations**.
Candidates and admitted macro origins are both zero. Reference integrity
retains 480 files, 9 separate working-tree inputs and 88 literal closure edges.
Independent `require-parity` exits **1** with explicit incomplete execution,
profile, setup, fixture and adapter scope. Saved respective outputs:
`/private/tmp/ferricstore2-m1-optimizations-parity-v2-{progress,reference,full-gate}.log`.

Read the complete retained C-P2 source observation and Rust function again,
including each multiline vector assertion. Final generated pairs are source
**156 -> Rust assertion 0 (line 201, empty at 2050); 165/174 -> assertion 1
(line 202, exact first singleton/action at 2100); 184/193 -> assertion 2
(line 219, exact second singleton/action at 2201 after completion at 2200)**.
The generated source/Rust texts and current Rust hash match the actual files.
Initial selection, installed policy and durable/public-response omissions stay
explicit. The added link records the previously omitted empty observation;
it does not certify a whole source scenario or change reducer behavior.
The saved read-only audit checks these pairs and every referenced producer,
review, collection and registration digest against current bytes:
`/private/tmp/ferricstore2-m1-optimizations-parity-v2-artifact-audit.log`.
The current artifact's parser digest equals the independently tested parser.

Also read claim's final `SelectionError` documentation clarification:
allocation failure is host recovery/retry, and invalid canonical authority
requires failure/corruption handling; neither is a durable deterministic business
rejection. Context distinguishes request versus authoritative invalid priority.
No enum, display, public API or selector logic changed; full canonical value
equality remains intact. No new blocker found in this peer clarification.

**C-P1/C-P2 additional independent review is closed with no remaining blocker
for the published conservative increment.** Observed wrong-origin variants
now remain unresolved, original raw registrations are accounted for, and
represented assertion pairs retain their actual semantics. This is bounded
parser/admission/accounting evidence, not complete static Elixir analysis or
full workflow parity. Earlier full-wrapper results refer to their saved older
tree; coordinator integration will repeat its full checks on this final tree.
Accepted O1/O2 optimization review remains closed. Only this owned note and
approved temporary review tooling/logs were written; no peer/runtime edits,
commits, pushes or interruptions.
