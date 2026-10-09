# M1 continuation coordination

The user resumed implementation on 2026-10-09, explicitly requesting subagents,
shared-file communication, preservation of the reviewed product architecture,
reciprocal review and no interruption of peer work. Existing uncommitted work on
`codex/workflow-tdd-core` is preserved. All agents use GPT-6.1-sol with reasoning
effort max, as required by `AGENTS.md`.

## Write ownership

- Retry owner: `crates/wf-domain/src/{policy,retry,workflow,history,value,lib}.rs`,
  `crates/wf-domain/src/workflow/**`, existing domain tests and new retry-policy
  tests, and `docs/reviews/workflow-m1-retry.md`. May split its existing large
  workflow and lease-test files into cohesive modules.
- Claim owner: `crates/wf-domain/src/claim.rs`, `crates/wf-domain/src/claim/**`,
  new `crates/wf-domain/tests/claim_selection*.rs` and
  `docs/reviews/workflow-m1-claim.md`. Does not edit existing reducer/test files.
- Parity owner: `crates/wf-parity-tests/**`, new progress/reconciliation artifacts
  under `tests/parity/**`, `scripts/collect-elixir-cases.exs`, new reconciliation
  tooling, `docs/runtime-case-collection.md` and
  `docs/reviews/workflow-m1-parity.md`. Frozen `tests/reference/**` and the
  existing `workflow-mapping.json` remain immutable.
- Coordinator: root policy/configuration, architecture tooling, shared guides,
  integration, handoff and this file. Only the coordinator may commit or push.
- Optimization reviewer: `docs/reviews/workflow-m1-optimizations.md`, plus any
  explicitly agreed tooling-only probes. Reviews actual runtime representations,
  allocations and clones; owners/coordinator implement accepted suggestions.
  No production or peer edits without a separately assigned disjoint scope.

No owner edits a peer's paths. Each publishes proposed API contracts in its own
note before dependent implementation, reads both peer notes, and sends findings
to the owner. Direct messages may notify a peer of an updated file; the actual
finding, review evidence and resolution must be recorded in the owned note.
Agents finish their assigned work without interruption, then perform reciprocal
review. Findings are corrected by the owner and independently re-reviewed.

## Architecture and scope

Read `README.md`, `docs/architecture.md`, `docs/implementation-plan.md`,
`docs/workflow-test-parity.md`, the previous TDD notes and `docs/CONTINUE.md`.
The pure domain remains synchronous and deterministic, depending only on
`wf-types` and standard value facilities. Use borrowed coherent authoritative
views and bounded plans. No I/O, wall clocks, randomness, storage, caches, runtime
dependencies, fake durability or transport success. M2+ gates remain closed.

This continuation adds source-derived retry-policy/exhaustion decisions and
bounded authoritative claim-ordering primitives, and reconciles real ExUnit
runtime identities. It does not certify M1 completion or full workflow parity.
Legacy fencing no-ops remain distinct from explicit request-ID replay.

Observe actual failing test output before implementation and passing output
afterward; distinguish compile-red, behavior-red and supplemental first-pass
checks. Preserve exact source identities/assertions and report remaining gaps.
Use isolated Cargo target directories per owner. Do not run global formatting
over files another owner is editing. Do not weaken lints or architecture rules.

## Source sizing

The coordinator will add a checked project policy: at most 400 physical lines
per production Rust file, and 600 per tooling/test Rust file. These thresholds
are maintainability choices for this repository. Preserved reference snapshots
and explicit parser fixture data are excluded. Split by responsibility rather
than minifying code or omitting useful documentation. Clippy's existing long
function checks remain enforced. Coordinate any new root module classification
with the coordinator instead of editing the allowlist.

## Validation and peer roster

Initial complete gate is running with output in
`/private/tmp/ferricstore2-resume-initial-check.log`. Its result will be recorded
after completion. Final integration must run `bash scripts/check.sh`,
`cargo progress` and `cargo parity` (expected incomplete failure).

Agent IDs and concrete review findings will be appended after assignment.

Assigned agents: `/root/m1_retry`, `/root/m1_claim`, `/root/m1_parity`. Direct
notifications carry file updates; review records require actual file/code reads.

The initial saved-tree wrapper completed with exit 0 before this continuation's
implementation changes: all existing tests, strict formatting/Clippy/docs,
architecture, reference integrity and dependency audits passed. Initial
`check-progress` exited 0 with 5,858 declarations, nine source-derived mappings,
22 assertion links, 4,222 linked registered cases and 1,655 unresolved IDs.
Initial `cargo parity` exited 1 as required. Its frozen M0 diagnostic concerns
the immutable full-parity mapping; partial registration is separately accounted
for by the progress gate. Logs are `/private/tmp/ferricstore2-resume-initial-`{
`check,progress,parity`}`.log`.

Size guard TDD: before adding production checks, three boundary tests actually
failed (0 passed, 3 failed) in `/private/tmp/ferricstore2-size-red.log`; the
separate runtime-module naming test actually failed (0 passed, 1 failed) in
`/private/tmp/ferricstore2-size-naming-red.log`. Both commands used Rust 1.99.0,
locked/offline dependencies and the isolated coordinator target directory.
Implementation followed these observed failures; final green evidence remains
pending. ADR-003 records the policy; the checker has no per-file exemptions.

After implementation the complete architecture library suite passed **16 tests**
(the four new regressions plus twelve retained), no ignored cases, exit 0,
`/private/tmp/ferricstore2-size-green.log`. Strict all-targets architecture Clippy
also passed, `/private/tmp/ferricstore2-size-clippy-green.log`. Its first run
caught three test assertions using `assert!(values.is_empty())`; they now use
`assert_eq!` for useful diagnostics, without suppressing the lint. Final workspace
integration and independent peer review remain pending while owners work.

`cargo progress` now has a shared alias and is included in `scripts/check.sh`,
which CI already invokes on Linux/macOS. This accounting gate does not replace
the fail-closed full-parity gate or imply execution of registered source cases.

## Coordinator source and contract review

Actually read both domain owners' published contracts and pinned
`retry_policy.ex`, `flow_transition.ex:1075-1185` and `fifo_lane.ex`. Retry
resolution/exhaustion remains pure over a borrowed retry-only policy, with
explicit committed jitter. Claim selection remains pure over borrowed coherent
authoritative indexes, with direct FIFO head lookup separate from bounded
candidate scans. Neither introduces a production dependency, storage success,
leader candidate hint or request-ID replay claim. No contract blocker found;
actual implementation review remains pending.

The parity owner's schema-2 contract was read and accepted: immutable raw
registration is retained, exact consumer `use` and restricted literal macro AST
origins may add links, all original runtime identities must remain accounted for,
and ambiguous origins/mixed scope/setup/profile gaps remain explicit. Structural
hash validation does not authenticate origin authorship or certify execution.

Executed original source tests in the separate pinned baseline, after confirming
the parity owner was only parsing AST and not running the application:

- `mix test .../flow/fifo_lane_test.exs .../flow/retry_policy_test.exs`:
  **12 tests, zero failures**, seed 729398, exit 0. Actual output is
  `/private/tmp/ferricstore2-m1-source-fifo-policy.log`.
- `mix test apps/ferricstore/test/ferricstore/flow_test.exs:494:500:501`:
  **56 tests, zero failures, 209 excluded by exact location selection**, seed
  734816, 18.3 seconds, exit 0. These are the actual three consumer section
  locations from raw registration (19 + 13 + 24 cases). Output is
  `/private/tmp/ferricstore2-m1-source-order-retry-sections.log`; JUnit was copied
  to `/private/tmp/ferricstore2-m1-source-order-retry-sections.junit.xml`.

This is executed baseline evidence for the selected cases only; it neither
executes all 5,843 registered cases nor proves any Rust persistence/transport or
full parity. The pinned checkout source remains unchanged and the original dirty
Elixir checkout is untouched.

- `mix test .../flow/invariant_test.exs:38:196:246:312:413:466:510:546:584:1102`:
  **10 tests, zero failures, 20 excluded by exact location selection**, seed
  654031, 2.9 seconds, exit 0. This includes the 80-follower deep-head and
  global-due-order fixtures. Actual output and copied JUnit are
  `/private/tmp/ferricstore2-m1-source-fifo-invariants.log` and `.junit.xml`.

These three commands executed **78 selected original cases**, all passed, with
no claim about excluded cases or mandatory nondefault profiles.

Initial retry-policy implementation review: read actual `policy.rs`,
`retry.rs` and seven policy fixture tests. Per-field merge precedence,
fixed/linear/exponential cap ordering, integer jitter spread/modulo,
zero-delay and bounded large-attempt arithmetic match the inspected source.
Computed-time overflow remains distinct from explicit input admission.
No defect found in this partial review; reducer/exhaustion and claim/parity
implementations still require completed review.

## Coordinator finding R-P1: lexical aliases in macro origins

Read the actual AST provenance tool and its five tests. An independent temporary
fixture compiled two real section macros with the same literal test name. The
consumer uses `alias OtherRoot, as: Review` followed by
`use Review.LiteralSection`. The actual compiled consumer's marker reports
`:second` from `OtherRoot.LiteralSection`; recovery incorrectly returns the
`:first` declaration from `Review.LiteralSection`, because the restricted parser
concatenates alias AST segments without honoring or rejecting lexical rebinding.

Valid reproduction exited 1 with the observed wrong origin in
`/private/tmp/ferricstore2-m1-origin-alias-review.log`; fixture source is the
same basename with `.exs`. An initial invalid nested `as` alias was corrected
before that reproduction and is not behavioral evidence. No peer file changed.
The parity owner was notified to add a pre-fix regression and conservatively
reject affected aliases or resolve them exactly. This finding is open pending
owner correction and independent re-review; it does not assert that any pinned
origin currently contains such a rebinding.

The user also requested an additional optimization-review subagent. All three
implementation slots are active; assign a separate reviewer when the next slot
opens, preserving the explicit no-interruption requirement. Its review must
evaluate allocation/clone behavior and `Box<[T]>`/`Vec<T>` tradeoffs in the actual
code, preserve bounds and determinism, and separate measured results from
representation/code-review observations.

User requested additional slots. Read the official OpenAI
[Subagents](https://learn.chatgpt.com/docs/agent-configuration/subagents) and
[Developer Settings](https://learn.chatgpt.com/docs/developer-settings)
documentation; the documented project configuration setting
is `agents.max_concurrent_threads_per_session` (excluding the primary). Added
`.codex/config.toml` with six spawned threads for future sessions. The current
tool session still declares four active total slots and exposes no limit-reload
operation; no live capacity increase is claimed or attempted. No security,
model, credential, or unrelated user-global configuration changed.

R-P1 independently closed: after reading the owner's actual two failing alias
regressions and conservative namespace rejection, reran the original compiled
fixture unchanged against the updated actual parser. Actual runtime still reports
`:second`; recovery is now unresolved instead of falsely linking `:first`.
Exit 0, `/private/tmp/ferricstore2-m1-origin-alias-recheck.log`. Owner artifacts
must still be regenerated after the parser update.

Completed actual retry reducer review: read the split command/model, validation,
create/claim, lifecycle, retry and plan modules, retry history, fixture helper,
policy and source-derived transition tests against the pinned retry source.
Guards/admission remain before effects, no-ops retain the existing witness,
scheduled/exhausted targets retain exact source timing, payload/error/fence
semantics and nine typed audit values are preserved. No defect found in the
represented subset; source hooks, retention, policy installation and public
wire encoding remain explicit omissions. Independently executed `wf-domain`
`fencing_retry`, `retry_policy` and `retry_policy_transitions` against the actual
split code: **25 tests passed** (6 + 8 + 11), zero failures/ignored, exit 0,
`/private/tmp/ferricstore2-m1-retry-independent.log`.

Coordinator findings C-C1/C-C2: actual claim code initially used process-local
pointer identity for continuations and reset duplicate checks on every page.
Owner recorded two observed failing regressions before changing behavior.
Resolution now uses explicit group/revision/request values and bounded lookups
against a borrowed unique canonical `RecordIndex`. Read the actual amended
index/selector code and owner red/green note: no pointer path remains, and
canonical records prevent conflicting scoped identities from reappearing on
later pages. Independent final execution/review remains pending owner completion.

C-C1/C-C2 independently closed: reviewed the actual value cursor, canonical
record equality checks, direct lane-head lookup and request/index validation;
executed all four claim targets against that code. **34 tests passed**
(8 selection + 5 blocking/reclaim + 8 lane primitives + 13 limits), zero
failures/ignored, exit 0. The same independent command ran the corrected retry
transition target: **11 passed**, including the strengthened successful final
claim observation from peer finding P-R1. Actual output is
`/private/tmp/ferricstore2-m1-domain-final-independent.log`. These checks cover
the represented pure subset and do not certify authoritative adapter coherence,
batch persistence or full workflow parity.

Independently re-executed the final restricted AST tool suite: **8 tests,
zero failures**, ExUnit seed 254427, exit 0,
`/private/tmp/ferricstore2-m1-origin-final-independent.log`. The suite includes
literal and describe origins, exact-use disambiguation, dynamic/duplicate
rejection and namespace-rebinding regressions; no source test body was executed.

Retry owner completed its work and reciprocal reviews normally. The user-requested
distinct optimization reviewer `/root/m1_optimizations` then started in the freed
slot using GPT-6.1-sol/max. Claim and parity owners were notified to communicate
through review files and review its concrete findings. No agent was interrupted;
the current session still has three active subagents alongside the coordinator.

## Accepted optimization review proposals

Actually read the optimizer's code observations, named safe temporary probe,
warning-free pre-change measurements and ranked O1/O2 recommendations. Root
accepted and implemented only `wf-types/src/identity.rs` and
`wf-domain/src/lease.rs`: partition encoding retains its checked fallible Vec
builder but counts/emits decimal lengths without temporary owned Strings;
private `RecordId` and `LeaseToken` store immutable boxed byte slices after the
same validation, with unchanged Vec constructors and borrowed byte accessors.
The decimal buffer uses `size_of::<usize>() * 3` bytes, covering the decimal
digits of every usize value without narrowing or target-width assumptions.

This representation/cost change preserves behavior; pre-change measurements
and retained semantic tests are used rather than claiming a new behavior-red
TDD cycle. Local 24-to-16-byte wrapper handles do not establish a general clone
or throughput gain, and spare-capacity conversion may reallocate. PartitionKey
boxing, broad record boxing, retry-resolution overlays and canonical-index API
changes remain unaccepted. Full canonical selector equality, safety/dependency
policy and public command/lease Vec fields remain intact. Independent optimizer/
claim re-review and final integration checks are pending.

Accepted-change retained semantic verification passed **99 tests** (19 types +
80 domain), zero failures/ignored, exit 0,
`/private/tmp/ferricstore2-m1-optimizations-accepted-test.log`. The same run
retains all three exact partition vectors, 30,783-list collision check, decimal
boundaries, binary ID limits, ownership/fencing, retry and claim regressions.
No test body or source counterpart identity changed for this optimization.

Claim owner independently re-read accepted O1/O2, including zero formatting,
digit order/buffer sufficiency, checked reserve and opaque boxed-value trait/API
semantics. Its independent post-change **99 tests** and complete-workspace
architecture test passed; no new blocker. Actual owner evidence/closure is in
`workflow-m1-claim.md` and
`/private/tmp/workflow-m1-claim-o1-o2-review.log`. Optimizer measurement/re-review
and parity-owner artifact finalization remain for final integration.

## Saved-tree integration result before C-P1/C-P2

All code/parser/artifact owners published their final stable paths; reviewed the
actual runtime collection guide, generator ledger, supplemental root/origin
validation and optimizer/claim closure. The complete native macOS wrapper
`bash scripts/check.sh` exited **0** with **180 workspace Rust tests**
(19 types + 80 domain + 44 parity + 24 architecture + 13 test support),
**three compile-fail doctests**, zero failures/ignored, strict formatting/
compilation/Clippy/docs, architecture/reference/progress and all four audits.
Actual final output is `/private/tmp/ferricstore2-m1-final-check.log`.

The progress gate reports 5,858 frozen IDs, 37 source-derived declarations,
38 counterpart records, 123 assertion links, 5,903 raw registered cases,
5,876 linked cases, 27 unmatched runtime cases and one unresolved declaration.
`cargo parity` independently exited **1** as expected with accurate remaining
full-scope/profile/setup/fixture/adapter execution gaps,
`/private/tmp/ferricstore2-m1-final-parity.log`. Full parity and M1 remain
incomplete. Frozen inputs have no diff; the isolated pinned baseline remains
source-clean. The updated handoff supersedes the stopped-state totals.

Optimizer independently measured accepted actual code after O1/O2: ID/token
handles 24 to 16 bytes, workflow record 304 to 288 bytes; actual partition
medians for 1/8/64 components changed from 42/254/1856 to 19/52/297 ns.
Its owned note records independent 99-test green, exact opaque-byte/hash/clone
trait checks, reciprocal claim review and target/allocator/workload limitations.
No production throughput, RSS or clone-speed guarantee is inferred.

## Final peer finding C-P1: macro-generated alias environment

After the saved-tree wrapper passed, claim's final source-origin review compiled
an unchanged valid fixture where an earlier `use AliasBinder` injects
`alias OtherRoot, as: Review`. A later `use Review.First` actually uses the
OtherRoot section (`:second` marker), while the restricted tool accepts the
original Review section (`:first`). Direct lexical-alias rejection alone is
insufficient. Fixture/log are
`/private/tmp/workflow-m1-claim-macro-injected-alias-repro.exs` and `.log`.
This is an actual wrong-source reproduction, not an assertion that a pinned
origin is affected. No peer or frozen file was changed by the reviewer.

Owner accepted C-P1 and is recording a pre-fix regression before constraining
or proving preceding macro environments without expansion. Publication is held
for owner correction, parser-bound artifact regeneration and independent
unchanged-fixture review. The previous full wrapper remains valid evidence for
its saved tree; a post-fix complete gate is required. Unknown/dynamic effects
must remain unresolved rather than preserving counts through an unproved
whitelist.

Root independently extended C-P1 with a preceding generic macro call:
`require AliasBinder; AliasBinder.bind()` injects the same alias without a
preceding `use AliasBinder`. Actual runtime is `:second`, while the current
tool links `:first`; exit 0 reproduced the wrong link in
`/private/tmp/ferricstore2-m1-origin-macro-call-repro.exs` and `.log`.
Original peer fixture was preserved. Owner was notified to reject/prove generic
preceding macro/import/require environments, not just earlier uses, and add an
observed regression before correction.

The same independent generic-call fixture was also tested with the require/
macro call at file top-level before the consumer module. It again actually ran
`:second` while accepting `:first`, exit 0,
`/private/tmp/ferricstore2-m1-origin-top-macro-repro.exs` and `.log`.
Owner/reviewer were notified to constrain inherited top-level environments as
well as in-module expressions. These three synthetic reproductions are one
C-P1 class; none asserts a pinned-source case is affected.

After the consumer-prefix v2 tests passed, root tested the same C-P1 class in
the section's own definition environment. A top-level required macro changes
the first section's declared Review namespace to OtherRoot. A second section
defines actual Review through a direct alias; that file is correctly omitted
by the direct-alias guard. The consumer has only `use Review.First`, with
ExUnit scaffolding inside the real quote, so the consumer-prefix gate is not
the cause. The actual body is `:second`, yet the v2 pure matcher labels the
first declaration. Reproduction exited 0 in
`/private/tmp/ferricstore2-m1-origin-source-env-repro.exs` and `.log`.
Root notified the owner/reviewer to apply the same conservative inherited/
ordered-prefix admission to section definitions. Current Rust unproved-section
review protects the actual zero-origin artifacts, but proposals may not
misidentify a section namespace. This is one finite C-P1 correction, not a
demand to implement general static Elixir analysis.

Claim's final assertion-pair review also reported C-P2: source ID
`e339e69ff4c5a0fa` line 165's successful single retry claim at 2100 was linked
to Rust's empty-at-2050 assertion. Production behavior is correct; the evidence
pair is wrong. Source 156 belongs to Rust empty, 165/174 to Rust singleton,
184/193 to Rust two. Parity owner is correcting/regenerating the ledger; peer
and root exact-pair review must close this before publication.

## C-P1/C-P2 closure and post-fix integration

Owner recorded real RED controls before both consumer-prefix and
section-definition-prefix corrections. The v2 AST parser now constrains ordered
module and inherited file prefixes in both places. Unknown effects remain
unresolved; no framework/loader exception, macro expansion or scaffold proof is
admitted. All ninety current section reviews are explicitly `unproved`.
Rust rejects a restored formerly accepted origin with internally consistent
raw identity/complements under that decision. The retained former row is a
review-only adversarial fixture, not an admitted source identity.

Root independently executed the final pure AST suite: **15 tests, zero failures**,
seed **120422**, exit **0**, actual output
`/private/tmp/ferricstore2-m1-root-origin-v2-final-suite.log`.
All four unchanged compiled diagnostics (preceding use, required macro call,
inherited consumer prefix and inherited source-definition prefix) actually
executed `:second` and now recovered unresolved. Their original diagnostic
exits are **1** because the former wrong-link reproduction is absent, not
because compilation failed. Actual outputs are
`/private/tmp/ferricstore2-m1-root-origin-{use,call,top,source-env}-final-recheck.log`.
Optimizer independently confirmed the same four fixtures and final AST suite,
then **45 Rust parity tests** and current hash-bound artifacts.

Root re-read source `invariant_test.exs:135-198`, the complete Rust retry/FIFO
counterpart and its current generated row. Correct C-P2 pairs are source
**156 -> Rust line 201** (empty at 2050), **165/174 -> line 202** (first singleton
and ID at 2100), **184/193 -> line 219** (second singleton and ID at 2201 after
completion at 2200). The actual source/Rust texts and current Rust hash match.
Independent read-only checks also verified producer/review/details/registration
digest references against current bytes. This closes the mapping error without
changing reducer behavior or asserting the omitted policy/initial-selection
observations.

The repeated full native macOS wrapper exited **0** on the corrected tree:
**181 Rust all-target tests** (19 types + 80 domain + 45 parity + 24 architecture
+ 13 test support), zero failed/ignored, and **three compile-fail doctests**.
Strict formatting/compilation/Clippy/documentation, architecture including size
limits, reference/progress and all four dependency audits passed. Actual output:
`/private/tmp/ferricstore2-m1-v2-final-check.log`.
`cargo parity` separately exited **1** with accurate full-scope/profile/setup/
fixture/adapter execution gaps, `/private/tmp/ferricstore2-m1-v2-final-parity.log`.
Earlier greens are saved-tree evidence, superseded for final acceptance by this
post-fix run. The final claim error-documentation clarification is included.

Current accounting is **5,858 frozen IDs; 37 represented source declarations,
38 counterpart records and 124 assertion links; 5,903 raw registered cases,
4,222 exact links to 4,203 distinct frozen declarations, zero admitted macro
origins, 1,681 unmatched runtime cases and 1,655 unresolved declarations**.
No raw identity or frozen ID was dropped, excluded wholesale or marked passing
to obtain these conservative counts. Frozen inputs retain no diff and the
isolated pinned baseline remains source-clean. C-P1/C-P2 are closed for this
bounded increment; M1 and full workflow parity remain incomplete.

Claim and parity owners subsequently published their final note-only closures.
Claim independently ran the final AST fifteen cases and Rust parity forty-five
cases, reviewed actual source-definition guards and full C-P2 assertion bodies,
and verified honest progress complements. Parity independently ran forty-five
Rust cases plus strict Clippy/docs/fmt, the complete C-P2 semantic audit and
all current artifact/original-capture digest checks. No code/data/guide changed
after the stable notification used for root's final wrapper. All reciprocal
reviews are closed; agents returned normally without interruption.

## Reviewed branch and CI

Root committed the reviewed implementation/configuration/artifacts as
`25701439accaeb51d41ffc3fcdcddc5b503f2a8e` and pushed
`codex/workflow-tdd-core` to private `ferricstore/ferricstore2`, without merging
main. [Infrastructure run 37973550388](https://github.com/ferricstore/ferricstore2/actions/runs/37973550388)
completed **success** for that exact SHA: both ubuntu-24.04 and macos-15 jobs
passed the shared locked wrapper. The code/artifact tree is unchanged by the
documentation-only follow-up recording this result. CI runs no original
Elixir application suite or standalone AST suite, and does not certify full
workflow parity. The original dirty checkout remains untouched and the isolated
pinned baseline remains source-clean.
