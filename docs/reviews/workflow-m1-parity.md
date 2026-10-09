# M1 runtime registration reconciliation

Owner: parity continuation agent, 2026-10-09. Scope is the parity crate,
collection/reconciliation tooling and progress artifacts. Frozen reference
sources and `workflow-mapping.json` remain immutable. Registration, assertion
links, source execution and Rust execution are distinct evidence. Full parity
continues to fail closed.

## Proposed contract, published before implementation

Keep progress schema 1 and its declaration/scoped assertion identities. Add
registration schema 2 while accepting existing schema 1 as incomplete legacy
accounting. Schema 2 references the hash-checked original
`collection-details.json` and a hash-checked macro-origin artifact; it retains
the exact complement of unmatched **runtime** identities as well as unresolved
**declaration** identities. Each linked case has a `runtime_case_id`; macro
links also have an `origin_id`. Source `source_path`/`source_line` continue to
name the frozen declaration, while the original registration location remains
in the details artifact. Every runtime case, including unmatched cases, is
accounted for exactly once; names, modules and effective tags must equal the
original registration. Neither artifact accepts execution/pass fields.

Runtime identity is SHA-256 of NUL-separated UTF-8 components without a
trailing NUL: commit, original registration path, decimal registration line,
module, full ExUnit name. Linked `case_id` retains the schema-1 identity
(commit, declaration ID, declaration path, declaration line, module, name).
An origin ID hashes the canonical JSON origin row with the ID omitted.

Macro recovery is conservative: parse the actual pinned Elixir AST, identify
the consumer module's **exact** `use` location and named section module, and
only recover literal `test` names in a simple `__using__`/`quote` body (including
literal `describe` blocks). No name-only search, nearest-line fallback,
interpolation guessing or arbitrary macro execution establishes identity.
Origins record both source-file hashes, declaration/test/use lines and exact
text, macro module/definition, consumer module and full runtime name. The
review artifact explicitly approves the source-origin rule and source paths;
ambiguous/dynamic cases stay unmatched. Rust validates hashes, raw case
equality, origin IDs/locations/text and bidirectional accounting. The AST tool
and agent review establish the restricted macro provenance; Rust does not
claim to be an Elixir parser or authenticate artifact authorship.

The origin artifact records explicit case-scope decisions (`included-workflow`
or `pending-case-review`), original helper/profile inputs and required setup,
fixture and profile review gates. Unreviewed/mixed cases receive no exclusion or
pass. Source helpers and effective tags are retained; helper defaults do not
prove required profile execution, and registration runs no setup callbacks.
Unknown or filtered profiles, missing fixtures, unresolved identities and all
unexecuted behavior remain acceptance gaps.

The collector will retain its existing no-autorun behavior and add final
ExUnit profile metadata, helper source hashes and explicit registration-only
setup/fixture warnings. A missing runtime collection guide will document exact
commands, prerequisites, artifacts, this restricted recovery rule and remaining
gates. Generator changes may attach new retry/claim assertions only after
reviewing the owners' actual counterparts and source lineage. File splits
must retain the named active test functions; generator file paths will follow
the real owner layout.

The CLI's full-parity failure will stop claiming that real registration is
absent after it exists. This changes reporting only: there is no passing
full-parity path. No runtime dependencies, root policy or peer writes are
planned; new tooling modules must remain below 600 physical lines.

## Initial evidence and peer contracts

Read the reviewed architecture, ordered plan, parity contract, handoff,
previous parity/domain/types TDD notes and resume coordination. Initial real
collection: 5,843 registered cases in 355 modules; 4,222 exact-location links,
4,203 linked declaration IDs, 1,655 unresolved declarations and 1,621 unmatched
runtime cases. All helper/root loads succeeded; no source cases or setup ran.
Helpers recorded different exclusion defaults across the three umbrella apps;
these remain requirements rather than execution evidence.

Retry owner published its source-derived policy contract in
`workflow-m1-retry.md`; actual implementation and counterpart review are
pending. Claim owner's contract is pending publication. Findings and concrete
red/green commands will be appended here before claiming their outcomes.

Independent retry API review: compared the proposed borrowed retry-only view
and explicit committed jitter input with pinned `retry_policy.ex:163-178` and
`flow_transition.ex:1075-1185`. The four-level merge, ownership-before-fresh
resolution, attempts/exhaustion choice, logical run state, terminal nil due time,
active `now` due time and nine retry metadata observations agree. The separate
legacy no-op path and retained caller-resolved operation preserve the existing
fencing mappings. No dependency on claim mode, storage installation or a runtime
clock is introduced. No blocking contract finding; arithmetic/jitter bounds,
field preservation and actual source-derived assertions await code review.

## Red evidence, before implementation

Wrote the CLI assertion for honest post-collection failure wording, and five
pure ExUnit AST-recovery tests against an empty-origin scaffold. Observed:

- `cargo +1.99.0 test --locked --offline -p wf-parity-tests --target-dir
  /private/tmp/ferricstore2-m1-parity-agent --test cli
  preserved_artifacts_pass_but_full_workflow_parity_fails`: exit 101, **0 passed,
  1 failed** at the missing accurate execution-gap message. Saved actual output
  `/private/tmp/workflow-m1-parity-cli-red.log`.
- `elixir scripts/tests/runtime-reconciliation-test.exs`: exit 2, **2 passed,
  3 failed**, covering missing literal source origin, literal describe prefix
  and exact-use disambiguation. Dynamic/duplicate/mismatched safeguards passed
  initially and are supplemental controls, not claimed red. Saved
  `/private/tmp/workflow-m1-parity-origin-red.log`.

These ran before the CLI reporting or origin implementation. The tests parse
source bodies without executing them; a deliberately raising fixture body must
remain inert.

## Restricted source-origin review and first green

The pure AST tests now pass **5/5**, no failures, saved at
`/private/tmp/workflow-m1-parity-origin-green.log`. Recovery reads source AST;
it does not load/expand a section or run its deliberately raising test body.
The real candidate scan found **1,621 literal candidates in 88 sections**.
Reviewed actual module/macro/quote headers, original exact consumer `use`
locations/text, first declaration and closing structure for all 88 preserved
sections. The restricted parser checks every recovered literal declaration line
and name against the frozen manifest; contextual full ExUnit names must match
the actual used section's AST, rather than a global name index.

`tests/parity/macro-origin-review.json` approves those explicit source paths,
hashes and consumer uses for identity lineage only. The 19 Flow/command section
modules receive `included-workflow` scope: their workflow regressions remain
required, while mechanism/compatibility dispositions and execution remain open.
All other sections receive `pending-case-review`, including mixed Raft/storage
and legacy dashboard modules; none is excluded or passing. Effective runtime
tags stay byte-equivalent as JSON data; original helper profiles and helper paths
remain in the hash-linked raw details and origin artifact. Setup, fixture review
and mandatory-profile execution retain unexecuted/pending requirements.

Original `runtime-registration.json` and `collection-details.json` are retained
unchanged. Reconciled artifacts are new files. Rust schema-2 validation and
adversarial accounting checks are still pending at this point.

The first restricted reconciliation retained **5,843/5,843** runtime cases,
linked 5,824 distinct frozen declarations and left **34** declarations
unresolved, with **0** unmatched runtime identities. Inspection of the actual
pinned sources found a collection-root gap: 33 remaining declarations are in
two `store/blob_side_channel_test/sections/**` files; their consumer
`store/blob_side_channel_test.exs` exists at the pinned commit but was omitted
from the frozen selected-root inventory. The final remaining ID
`0bc90782a5fe1e2c` is the `test "isolated test"` example inside a support
module's documentation (`support/isolated_instance.ex:19`), not a registered
case. These retain visible unresolved identities; no exclusion/pass has been
fabricated. A pinned, hash-verified additional-root registration is a proposed
next bounded collection improvement; the immutable manifest will not change.

Schema-2 Rust RED recorded before validator changes:
`cargo +1.99.0 test --locked --offline -p wf-parity-tests --target-dir
/private/tmp/ferricstore2-m1-parity-agent --test registration_v2` exited **101**,
**0 passed, 5 failed**, because schema 1 rejects `origin_id` before the new
bidirectional/raw-case/origin/requirement checks can run. This is behavioral
unsupported-schema red; it does not claim that every adversarial branch was
reached. The fixtures copy the actual retained artifacts into isolated temporary
directories and alter only those copies. Saved output:
`/private/tmp/workflow-m1-parity-schema2-red.log`.

**R-P1 accepted from actual coordinator review.** Read its valid compiled
alias-rebinding reproduction and output: runtime selected `OtherRoot` while
the AST matcher wrongly linked the literal original `Review` namespace.
Added two owner regressions before fixing lexical aliases (module-local
rebinding and top-level module-name rebinding). Actual owner RED:
`elixir scripts/tests/runtime-reconciliation-test.exs`, exit 2, **5 passed,
2 failed**, with the wrong original namespace/source ID in both failures.
Saved `/private/tmp/workflow-m1-parity-alias-red.log`. Resolution is pending
immediately after saving this record; candidates/artifacts must be regenerated
after the fix, before structural or source-origin acceptance.

**R-P1 owner resolution implemented:** files with top-level aliases are
conservatively unresolved. Module-level aliases that may rebind the used
namespace (including unknown alias forms) discard that use; literal unrelated
aliases preserve fully scoped origins. The owner suite is now **7 passed,
0 failed**, saved `/private/tmp/workflow-m1-parity-alias-green.log`; coordinator
please independently rerun its original compiled fixture. No guessed alias
resolution or modified peer fixture is used.

The conservative unknown-alias rule also omitted safe literal grouped aliases
in the real pinned consumers. Added a positive unrelated-group control and a
rebound-group negative control before refining that bounded syntax. Observed
RED: **7 passed, 1 failed**, `elixir scripts/tests/runtime-reconciliation-test.exs`,
saved `/private/tmp/workflow-m1-parity-group-alias-red.log`. This is a recovery
omission, not a wrong-source link. Known literal grouped aliases can identify
which local names are bound without resolving or guessing their target; all
other unknown forms remain conservatively unresolved.

## Retry counterpart review

Actually read the complete new `policy.rs`, `retry.rs`, reducer dispatch,
`workflow/retry_transition.rs`, validation/history code, both new retry test
files and their scenario helper. Compared all eight new source cases with
the exact preserved policy section (`396-581`) and priority section (`588-746`).
The 1,800 generated inputs, inclusive limit, four policy layers, source jitter
spread/modulo/final cap ordering, no-op-before-unused policy, explicit-versus-
computed deadlines, attempt/event increments, lease release, payload/error/result
rules and nine scheduled audit observations agree at the represented domain
layer. Source policy installation/readback, query/history wire strings and
durability remain limitations; typed metadata does not certify public bytes.
The original six fencing function identities remain unchanged.

**P-R1 open — active-exhaustion final claim is not observed.** In
`retry_policy_transitions::flow_retry_exhausts_to_configured_active_state`, the
final `scenario.apply(... Operation::Claim ...)` result is discarded and the
following assertion only checks an ID already held before that call. A
`NotEligible` outcome with no claim could satisfy this assertion. The source
`priority section:695-703` requires a singleton successful manual claim plus
the same ID. Owner should assert `Outcome::Changed` and the resulting lease/
logical state, retaining the precise omitted partition/durable-cardinality
limitation. No peer file was changed; final active-claim assertion mapping waits
for owner resolution and independent re-review.

The actual supplemental collector completed with **5,903 real cases in
356 modules**, all helper/root loads successful, no source cases/setup executed.
It retains the original 5,843-case artifacts unchanged and adds the missing
pinned consumer's 60 cases in a separate output directory. Consumer and omitted
section bytes were independently compared with `git show <pinned>:<path>`;
hashes and explicit collection-root rationale are in
`collection-root-review.json` and the supplemental details. The refined parser
recovers 33 additional frozen declarations from two explicit blob macro uses.
Reconciliation now has **5,876 linked cases, 1,654 reviewed macro origins, one
unresolved documentation declaration, and 27 unmatched runtime identities**
with no frozen navigation ID (new root/previously unpreserved section cases).
The blob section scope remains case-review pending; no blanket workflow/KV
inclusion or exclusion was approved. Source documentation nature is explicit in
`source-declaration-decisions.json`; its frozen ID remains unresolved, never
passing. Final global profile, per-helper profiles/source hashes, setup and
fixture-review warnings are retained separately from source-subset execution.

Additional-root proof RED, before its Rust validation:
`cargo +1.99.0 test --locked --offline -p wf-parity-tests --target-dir
/private/tmp/ferricstore2-m1-parity-agent --test registration_v2
supplemental_root_review_hash_is_required_even_for_unmatched_original_cases`
exited **101**, **0 passed, 1 failed** because a zeroed review digest was
accepted. Saved `/private/tmp/workflow-m1-parity-supplement-review-red.log`.

## Final source and counterpart review before mapping

**P-R1 closed after actual owner correction and independent re-review.**
Re-read the corrected active-exhaustion function and fixture helper. The final
manual claim now asserts `Outcome::Changed`, the retained ID, physical `running`
state, logical `payment_failed` run state and `worker-manual` lease; fixture
preparation also requires a changed initial claim. Compared these observations
again with source assertions 690-703. Independently ran both retry test targets
and all four claim targets: **53 passed, 0 failed, 0 ignored** (19 retry and
34 claim), saved `/private/tmp/workflow-m1-parity-peer-execution.log`. The
source singleton selection/durable claim call remains outside this retry-only
counterpart; no peer files were edited.

Read actual final claim index/request/selection/result/error modules and all
four owned test files, including the canonical record check and reconstructed
value continuation regressions. Re-read the complete original FIFO primitive
module/tests and original FIFO selection section, then compared each of the
twenty proposed frozen declarations with actual selection/lifecycle/primitive
assertions. Complete authoritative lane heads, exact u128 order, running rank,
priority/state/partition filters, due-head injection beyond eighty followers,
global due merge and explicit expired-head reclaim agree in the represented
pure view. Borrowed inputs remain unchanged. C-C1/C-C2 fixes use group/revision/
request values and canonical scoped records, with no pointer or growing cursor
history. No new concrete blocker found in this independently executed scope.
Canonical completeness/revision uniqueness, policy installation, native index
maintenance, public compact fields, automatic reclaim, transactional persistence
and later omitted lifecycle assertions remain explicit adapter/scenario gaps.

Generator update contract: retain progress schema 1 and all frozen IDs. Add
only the eight reviewed retry and twenty reviewed claim declaration identities,
using actual named functions in their owner's split integration test files.
One source declaration has two independently scoped lifecycle counterparts;
one counterpart represents selected observations from two separate source
declarations. Append counterpart records rather than overwriting a source ID,
and keep scenario IDs unique. Assertion counts guard subsequent fixture edits;
every pair is an exact original assertion start and a direct assertion in that
named Rust test. Compound source assertions may link multiple represented
sub-observations, with omitted observations stated in limitations. None is a
claim of complete original-scenario execution or full behavioral parity.

The additional-root proof is now checked even when its cases stay unmatched:
pinned review/digest, portable copied source paths, exact source-set/root subset,
no overlap with frozen reference and copied byte hashes. Owner schema-2 suite
is **6 passed, 0 failed, 0 ignored**, saved
`/private/tmp/workflow-m1-parity-schema2-green-final.log`. The last failure
above is resolved. Coordinator and claim owner independently verified the
actual supplemental source bytes/hashes and validator; claim independently ran
all six schema-2 tests green. Final AST owner suite is **8 passed, 0 failed**;
coordinator and both peers independently ran all eight. Coordinator's unchanged
compiled alias reproduction now stays unresolved, closing R-P1 without a guess.

Reviewed coordinator's actual `size::check`, its call from source inspection
and four boundary regressions. It uses package runtime classification and the
explicit integration-test flag, counts comments/blanks/CRLF/final unterminated
lines, accepts 400/600 and rejects 401/601. A runtime module merely named
`tests` cannot obtain the larger budget. No source/module boundary exemption or
new dependency was found; independent execution follows below.

The registration blocker wording will explicitly describe what this collection
executes; the coordinator's 78 separately executed original cases remain source
subset evidence, not evidence produced by the registration artifacts.

## Final artifacts, execution and reciprocal optimization review

Published `docs/runtime-case-collection.md` with the exact isolated collector,
proposal/reconciliation and progress/AST commands; raw/recovered identity rules,
root omission, pending per-case scope, helper/global profiles, fixtures and
separate subset execution evidence. It explicitly states that `scripts/check.sh`
does not run the standalone Elixir AST suite or original application suites.
The source documentation example is reviewed reference-only evidence, not a
passing runtime test. Original raw captures and all frozen reference/mapping
bytes remain untouched; source checkout `git status --short` is empty.

Regenerated candidates, origins and schema-2 registration after the final parser
revision and accurate collection-only blocker wording. Then regenerated the
source assertion ledger after independent source/counterpart review. Actual
`check-progress` exits **0**: **5,858 frozen entries, 37 source-derived
declarations, 38 counterpart records and 123 exact assertion links**.
The unchanged 9/9/22 partition/fencing records combine with 8/8/52 retry and
20/21/49 claim records. Reconciled counts are **5,903 registered, 5,876 linked,
27 unmatched runtime identities and one unresolved frozen declaration**.
There is no execution status or passed/source-complete field in this ledger.
Saved generator/progress output:
`/private/tmp/workflow-m1-parity-{generator,progress}-final.log`.

Final independently executed checks on the saved owned tree:

- `wf-parity-tests`: **44 passed, 0 failed, 0 ignored** (22 library, 6 CLI,
  10 legacy/progress and 6 schema-2 adversarial tests),
  `/private/tmp/workflow-m1-parity-tests-final.log`.
- Standalone original-source AST navigator regressions: **8 tests, 0 failures**,
  seed 785228, `/private/tmp/workflow-m1-parity-origin-suite-final.log`.
- Complete architecture package: **24 passed, 0 failed, 0 ignored** (16 library,
  4 ArchUnit, 3 visibility/compiler fixtures and 1 workspace gate),
  `/private/tmp/workflow-m1-parity-architecture-final.log`. This independently
  executes the coordinator's four physical sizing boundaries reviewed above.
- Owned all-target strict Clippy and warnings-denied docs exit **0**, logs
  `/private/tmp/workflow-m1-parity-{clippy,doc}-final.log`. Owned Cargo fmt and
  shared `git diff --check` pass. Every owned tooling/test source is at most
  451 physical lines, below the 600-line cap; no global format/policy edit.
- `check-reference` exits **0** with the original 480 files, 5,858 declarations,
  9 separately retained working-tree inputs and 88 frozen closure edges,
  `/private/tmp/workflow-m1-parity-reference-final.log`. This command's M0
  unported inventory is separate from the new partial progress ledger.
- `require-parity` exits **1**, as required, with explicit incomplete full
  source/Rust scope, profiles, setup, fixtures and adapter gaps,
  `/private/tmp/workflow-m1-parity-full-gate-final.log`.

Actually read the optimization review's ranked O1/O2/O3/O4 findings, focused
probe source relevant to decimal conversion/boxing/canonical equality and
retained raw layout/timing conclusions. The note accurately limits local
64-bit medians/layouts and does not claim allocator counts, clone speed or
production throughput. Read the coordinator's accepted final `identity.rs`
and `lease.rs`: stack decimal emission preserves zero/length/binary bytes,
checked length/fallible output reservation and the Vec partition storage;
the buffer is three bytes per usize byte, safely covering decimal digits.
Private boxed IDs/tokens preserve Vec constructors, byte access and derived
value semantics; caller spare-capacity shrink cost is explicit. Public vectors,
full canonical record equality, source test identities and assertion bodies
remain unchanged. O3 broad boxing, O4 borrowed target overlay and canonical-ID
index redesign remain deferred/unaccepted; no implied contract expansion.
Independently executed the **99 retained type/domain tests after O1/O2**,
all passing, zero failures/ignored, saved
`/private/tmp/workflow-m1-parity-post-optimization-domain-types.log`.
No new concrete blocker found in the accepted narrow changes; no peer edits.

Remaining gaps: no full original acceptance/profile run, no execution certificate
in registration, 27 additional runtime identities without frozen navigation,
one documentation-only inventory identity, unresolved per-case mixed scope and
fixture/support obligations. Represented borrowed retry/claim observations do
not implement durable installation/index maintenance/storage/consensus/wire/
worker/query/recovery or complete original scenario semantics. Full parity stays
fail-closed. Ready for coordinator integration, with no commits or pushes.

Coordinator independently completed the full saved-tree wrapper after this
owner's final artifacts/guide publication: **180 Rust all-target tests and three
compile-fail doctests passed**, strict fmt/check/Clippy/docs/architecture/
reference/progress and all four dependency audits passed, exit **0**. Its actual
log is `/private/tmp/ferricstore2-m1-final-check.log`; this is coordinator
execution, distinct from the owner commands above. Coordinator re-read the
final guide, ledger and source/root/origin validators with no blocker, and
independently observed `require-parity` exit **1** with accurate full-scope gaps.

## Final review hold: C-P1/C-P2

**C-P1 accepted before handoff.** Claim's unchanged compiled synthetic fixture
shows a preceding `use AliasBinder` injects `alias OtherRoot, as: Review`:
the later `use Review.First` actually executes `:second`, but the old parser
accepted the first declaration. Root independently reproduced the same class
with `require AliasBinder; AliasBinder.bind()`, both inside a module and
inherited from file top-level. These are actual synthetic wrong-origin
observations, not a claim that a preserved origin currently uses this pattern.
Owner added preceding-use, require/call and unknown-expression regressions
before changes. Actual RED: `elixir scripts/tests/runtime-reconciliation-test.exs`
exits **2**, **11 tests, three failures**, saved
`/private/tmp/workflow-m1-parity-preceding-macro-red.log`. Further inherited
top-level regressions will precede the corresponding fix.

Correction contract, published before dependent implementation: origin rule
becomes `literal-section-origin-v2`. Retain ordered consumer and inherited
file-prefix AST nodes. Unknown preceding uses, require/import/generic macro
calls, nested module/macro definitions, dynamic/unquote forms and unknown
attribute values remain unresolved. Do not execute/expand any macro. Ordinary
statically named function bodies are inert; literal attributes/direct aliases
can be inspected without evaluation. Source-navigation fixtures use an inert
literal attribute instead of assuming `use ExUnit.Case` is harmless.

**No framework scaffold proof is admitted in this increment.** Actual installed
1.19.5 ExUnit.Case debug-source inspection observed its `__using__` return and
registration calls, but this is insufficient proof of the complete inherited/
imported macro environment. Neither Case nor Code.require_file receives a
name-only exception. Current section review records `environment_decision =
unproved`; no source origin may cite such a section. Counts may decrease rather
than assert unknown lineage. Future environment proof needs its own precisely
published source/hash/scope contract and a new reviewed rule, not a silently
expanded allowlist. Raw captures remain unchanged; Rust rejects restoration
of formerly accepted origins under the new unproved section decisions.

**C-P2 accepted from actual final semantic pair review.** Source assertion
156 is empty at 2050; 165 is the successful singleton at 2100; 174 observes
its ID. The old pair `(165, 0)` linked the singleton to Rust's empty-before-due
assertion. Actual manual ledger audit failed before correction, exit **1**,
showing those exact stored texts. Correct pairs are `(156, 0)`, `(165, 1)`,
`(174, 1)`, `(184, 2)`, `(193, 2)`. Rust behavior is correct; this is mapping
data, not a reducer defect. Regenerate only after exact source/fixture review.
Earlier final checks remain actual evidence for their saved pre-C-P1/C-P2 tree;
final provenance acceptance requires regenerated artifacts and green checks.

Inherited consumer-prefix RED was **12 tests, four failures** before its fix,
saved `/private/tmp/workflow-m1-parity-inherited-macro-red.log`. The Rust owner
restoration regression independently observed the old validator accept an
unproved prior macro origin: **0 passed, one failed**, exit **101**, saved
`/private/tmp/workflow-m1-parity-unproved-origin-red.log`. This was actual
accepted reconciliation, not merely an unsupported-schema diagnostic.

Root then found the same C-P1 class in the source definition's inherited
environment: a macro before `defmodule Review.First` changes the actual module
namespace. Its compiled fixture retained actual `:second` versus recovered
first. Added both inherited source-definition and in-module pre-`__using__`
regressions before changing section admission. Owner RED: **15 tests, two
failures**, exit **2**, `/private/tmp/workflow-m1-parity-section-environment-red.log`.
The v2 prefix guard now applies equally to consumer uses and source macro
definitions; unknown inherited/module effects cannot supply a named section.
No original test/setup or arbitrary macro is run by recovery.

Current v2 rule supports no framework/scaffold proof. All ninety source-only
section reviews explicitly remain `environment_decision=unproved`. Both
producer and Rust reject invented environment statuses; Rust rejects restoring
any former origin even with consistent raw identity, complements and current
tool hashes. A single retained former row is a `review_only` adversarial fixture,
not accepted source identity. The original source-anchor adversarial check is
replaced by stronger unsupported-rule/scaffold and unproved-restoration checks;
anchor branches are not claimed reached for rejected unproved rows.

The exact C-P2 audit failed before correction, saved
`/private/tmp/workflow-m1-parity-cp2-mapping-red.log`. Corrected to source
156 -> empty-before-due, 165/174 -> same original singleton/ID at 2100 and
184/193 -> follower after completion. This adds one reviewed assertion link:
**37 source declarations, 38 counterpart records, 124 links** (claim 50).
Regenerated current candidates/origins/registration/ledger retain **5,903 raw
cases, 4,222 exact links, zero admitted macro origins, 1,681 unmatched runtime
identities and 1,655 unresolved frozen declarations**. No rows/tags/identities
were dropped, passed or excluded to obtain these lower proof counts.

Actual final pure v2 suite is **15 tests, zero failures**, seed 476193, saved
`/private/tmp/workflow-m1-parity-environment-v2-green-final.log`. Rust schema-2
suite is **7 passed, zero failed/ignored**, including real complement accounting,
unproved restoration, unknown scaffold status and original root-review checks,
saved `/private/tmp/workflow-m1-parity-environment-validator-green.log`.
`check-progress` exits **0** with the current 37/38/124 and
5903/4222/1681/1655 totals, `/private/tmp/workflow-m1-parity-progress-v2-final.log`.
Guide/progress prose now reports current v2 limits and distinguishes all earlier
saved-tree counts and greens. Final independent peer compiled-fixture reruns
and coordinator integration checks follow on this regenerated tree.

## C-P1/C-P2 final conservative closure

Final owner execution on the stable regenerated tree: **45 Rust parity tests
passed, zero failed or ignored** (22 library, 6 CLI, 10 progress, 7 schema-2),
`/private/tmp/workflow-m1-parity-tests-v2-final.log`. Owned strict all-target
Clippy and warnings-denied docs both exit **0**, saved
`/private/tmp/workflow-m1-parity-{clippy,doc}-v2-final.log`. Owned fmt and shared
`git diff --check` pass. All twenty owned tooling/test source files remain below
600 physical lines; maximum is `reference.rs`, 451 lines. Frozen reference,
mapping and dependency files have no diff; the pinned Elixir checkout remains
source-clean. Code, data and the collection guide are stable; this closure adds
only owner review prose.

Owner re-read the complete C-P2 original fixture, all multiline Rust assertions
and final generated row. Exact pairs are **156 -> 201**, **165/174 -> 202** and
**184/193 -> 219**. These retain empty selection at 2050, the exact first
singleton/action at 2100, then the exact second singleton/action at 2201 after
completion at 2200. The current Rust file hash and all stored source/Rust texts
match. Original generated namespaces are represented by fixed role-preserving
fixture IDs; installed policy, initial selection and durable/public adapter
observations stay explicitly omitted. Read-only audit exits **0**, saved
`/private/tmp/workflow-m1-parity-cp2-mapping-green.log`. This closes C-P2's
semantic data-link error, rather than treating a structurally green validator
as a semantic review.

Owner separately checked every current producer/review/details/registration/
progress reference, both supplemental source copies and unchanged original
capture hashes against actual bytes. Audit exits **0**, saved
`/private/tmp/workflow-m1-parity-v2-artifact-audit.log`. Final `check-progress`
and `check-reference` exit **0**; `require-parity` exits **1**, saved respectively
`/private/tmp/workflow-m1-parity-{progress,reference,full-gate}-v2-final.log`.
The former macro rows are not restored. Source-derived Rust assertion evidence
remains an independent partial observation even where runtime identity is
unresolved.

Read the optimization reviewer's actual final code/artifact/semantic-pair
review and coordinator closure. Each independently executed all four unchanged
compiled counterexamples: actual body remains `:second`, recovered origin is
unresolved. Their original diagnostics exit **1** because they exit zero only
on the former wrong-link condition; compilation succeeded. Owner read the
coordinator's actual four output logs,
`/private/tmp/ferricstore2-m1-root-origin-{use,call,top,source-env}-final-recheck.log`.
Coordinator's independent final AST suite is **15 tests, zero failures**,
seed 120422; optimizer's final suite is also **15/0**, seed 900794.
Optimizer independently executed **45/0/0 Rust parity tests**, checked current
hash-bound artifact references and complete C-P2 multiline observations, and
reported no remaining blocker. This closes the represented C-P1 variants under
the bounded no-scaffold-proof rule; it does not claim complete Elixir analysis.

Coordinator repeated the complete native wrapper on this corrected tree:
**181 Rust all-target tests passed**, zero failed/ignored (19 types, 80 domain,
45 parity, 24 architecture, 13 support), and **three compile-fail doctests**.
Strict fmt/check/Clippy/docs, architecture/size limits, reference/progress and
all four dependency audits passed, exit **0**, actual log
`/private/tmp/ferricstore2-m1-v2-final-check.log`. Its separate full gate exits
**1**, `/private/tmp/ferricstore2-m1-v2-final-parity.log`. These regenerated-tree
checks supersede earlier saved-tree evidence for this handoff. The wrapper
still does not run the standalone Elixir AST suite or original application
acceptance/profile suites.

Final accounting: **5,858 frozen declarations; 37 represented declarations,
38 counterpart records and 124 assertion links; 5,903 registered cases,
4,222 exact links to 4,203 distinct declarations, zero admitted macro origins,
1,681 unmatched runtime identities and 1,655 unresolved declarations**.
Raw tags, helpers, per-helper profiles and pending setup/fixtures/scope are
retained. The ninety section environments remain unproved; twenty-seven extra
runtime identities lack frozen navigation, and the documented usage example
remains an unresolved reference-only identity. C-P1/C-P2 are closed for this
conservative increment; M1/full parity and the durable/adapter/profile work
remain incomplete. No peer source edits, commits, pushes or interruptions.

Claim's final independent closure then arrived and was read in its actual
published note. Its unchanged compiled counterexample retains actual `:second`
and unresolved recovery; the defect-only diagnostic's expected exit **1** is
checked by an outer command exiting **0**. Claim independently executed the
final **15/0 AST suite**, seed 539240, and **45/0/0 Rust parity package**, then
checked current progress and exact C-P2 source/multiline Rust pairs. It reports
the same 37/38/124 and 5903/4222/0/1681/1655 accounting, explicit omissions and
no remaining concrete finding. Reciprocal claim, retry, coordinator and
optimization reviews are recorded separately from owner execution evidence.
