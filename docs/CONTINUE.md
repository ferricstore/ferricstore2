# ferricstore2: implementation handoff

Updated: 2026-10-09. The user resumed the saved increment with explicit
subagents, reciprocal review, shared-file communication and preservation of the
reviewed product architecture. This continuation is verified locally; M1 and
full workflow parity remain incomplete.

Final peer findings C-P1/C-P2 are closed for this conservative increment:
unknown macro environments remain unresolved, and the retry assertion pairing
now matches the observed times/results. Regenerated artifacts, independent
compiled diagnostics and the post-fix full wrapper passed. No macro scaffold
proof or full behavioral parity is claimed.

## Location and saved state

- Project: Rust, workflow-only, private
  [ferricstore/ferricstore2](https://github.com/ferricstore/ferricstore2).
- Checkout: `/Users/yoavgea/repos/ferricstore2-rust-design`.
- Branch: `codex/workflow-tdd-core`.
- The coordinator is preparing the reviewed continuation commit after the final
  green local gate. No new Linux/macOS CI result is claimed yet.
- Main's last handoff commit was `b0151c5`. The foundation Linux/macOS CI passed
  `f2ff13b2da4e47e1fd52f74520fb219efe2de613`
  ([run](https://github.com/ferricstore/ferricstore2/actions/runs/37929778754)).

The original dirty Elixir checkout `/Users/yoavgea/repos/ferricstore` was
preserved. The isolated detached baseline
`/Users/yoavgea/repos/ferricstore2-elixir-baseline` remains source-clean at
`66586c596e271408d9184af32eb31cd7218c126e`; its own locked dependencies/build/data
were used for original registration and selected source executions. Elixir is
the compatibility reference, not the new runtime.

## Implemented pure behavior

`wf-types` has checked opaque binary record IDs, separate composed-key bounds,
exact source `fpk:` partition-component encoding, committed timestamps and
nonwrapping fences. Binary/empty component handling, 65,535-byte limits, zero
values and overflow are tested. Partition encoding retains the exhaustive
collision check over 30,783 small component lists.

`wf-domain` implements deterministic single-record create, explicit claim,
transition, complete/fail, retry, lease extension, cancellation and explicit
expired-lease reclaim. Commands carry explicit committed time and ownership.
Results propose one record write and optional history; they do not establish
durability. Borrowed inputs stay intact. Legacy matching duplicate effects emit
no second event; stale guards, intervening operations and different terminal
effects reject. These legacy rules are separate from unimplemented explicit
request-ID replay.

New retry decisions resolve defaults, installed type, logical-state and command
overrides in source order. None/fixed/linear/exponential backoff, caps, explicit
committed jitter, inclusive attempt limits, scheduled retries, active/terminal
exhaustion and nine typed audit observations have source-derived tests. The
installed retry-only view is supplied authority, not policy-installation
persistence. Source hash preparation and public audit bytes remain adapter gaps.
The old caller-resolved retry-time API remains compatible.

New claim selection borrows coherent authoritative canonical records, parallel
due indexes and complete FIFO lane maps. Exact u128 member ordering, running
heads, descending priority, due/binary-ID ties, state/partition filters and
80-follower deep-head lookup are tested. It reads all represented lane heads
before merging, never skips a blocked head to its follower, and distinguishes
claim-limit/scan-budget exhaustion from complete empty work. Limits are 1,000
selected records, 16,384 scanned candidates and 4,096 direct lane-head reads.
Continuations use group/revision/request values; every consumed row/head must
match its unique canonical scoped record. Equivalent reconstructed snapshots
work; conflicting later-page identities reject. Index completeness, revision/
recovery uniqueness, policy-mode placement and atomic batch application remain
adapter obligations. Selection does not mint leases or persist changes.

Production dependency/module policy is unchanged: types/domain are synchronous
pure values with no runtime, clock, random, storage or cache effects. Storage,
consensus, application, wire, transport and worker crates remain scaffolds.
M2+ predecessor gates remain closed.

## Quality policy and accepted optimizations

Workspace compiler/rustdoc warnings and Clippy `all`/`pedantic` remain denied;
unsafe and blanket suppression remain forbidden. [ADR-003](adr/003-source-size-limits.md)
adds checked limits of 400 physical lines per production Rust file and 600 per
tooling/integration-test/benchmark file, including comments and blank lines.
A runtime module named `tests.rs` still gets 400. Oversized reducer/tests were
split by responsibility; no allowlist exemption was added. `cargo progress` is
now in the shared local/CI wrapper.

The separately requested optimizer reviewed actual runtime code and measured
safe temporary probes before proposing changes. Two accepted changes preserve
public constructors and slice accessors: private `RecordId`/`LeaseToken` use
`Box<[u8]>`, and partition encoding emits decimal lengths through a stack buffer
without temporary decimal Strings. Growable buffers remain Vecs. Independent
post-change code/trait reviews and all 99 type/domain tests passed.

Local aarch64 measurements showed ID/token handles 24 -> 16 bytes, workflow
record 304 -> 288 bytes, and partition encoding medians for 1/8/64 components
42/254/1856 -> 19/52/297 ns. These are focused local observations under concurrent
development, not service/RSS/tail-latency guarantees. Box conversion may shrink/
reallocate spare capacity; no clone-speed gain was established. Broad record or
PartitionKey boxing, retry-target overlays and canonical-ID index redesign are
unaccepted future candidates. Full canonical record equality remains intact.

`.codex/config.toml` requests six spawned subagents for future sessions. The
current session's tool limit remains four active agents including the
coordinator; that live limit was not resized. Agents exchange actual review
files and direct notifications, with disjoint write sets and no interruptions.
Only the coordinator commits/pushes.

## Actual verification and review

The final native macOS `bash scripts/check.sh` exited 0:

- **181 Rust tests**, zero failed/ignored: types 19, domain 80, parity 45,
  architecture 24, test support 13.
- **Three compile-fail doctests** passed.
- Formatting, locked all-target compilation, strict Clippy, warnings-denied
  documentation, architecture/reference/progress checks and all four dependency
  audits passed.

Actual post-fix output: `/private/tmp/ferricstore2-m1-v2-final-check.log`.
`cargo parity` separately exited 1 with accurate full-scope execution gaps:
`/private/tmp/ferricstore2-m1-v2-final-parity.log`. Fifteen standalone Elixir AST
regressions also passed independently (seed 120422); all four unchanged compiled
alias-injection diagnostics now show actual `:second` and recovered unresolved.
The wrapper validates parser/artifact hashes but does not execute that Elixir
suite or original application tests. Earlier green logs describe their saved
pre-fix trees, not final provenance acceptance.

This continuation executed **78 selected original source cases**, all passed:
12 FIFO primitive/retry-policy cases (seed 729398), 56 priority/multi-state/retry
section cases (seed 734816; 209 location-excluded), and 10 FIFO invariant cases
(seed 654031; 20 location-excluded). Commands/logs/copied JUnit are in the
coordination record. The previous saved increment separately executed its three
partition and six fencing cases successfully (seed 885123). These are source
subsets, not execution of all registered cases or mandatory nondefault profiles.

All four subagents used GPT-6.1-sol/max. Real preimplementation compile-red and
behavior-red output is distinguished from supplemental first-pass checks and
optimization measurements. Reciprocal/coordinator review caught and closed
lexical alias wrong-origin links (R-P1), process-address continuation identity
(C-C1), conflicting later-page scoped identities (C-C2), weak final claim
observation (P-R1), unchecked supplemental-root review hashes, macro-generated
aliases in consumer/source definition environments (C-P1), and one incorrect
source/Rust assertion pairing (C-P2). Owners fixed and peers independently
re-reviewed them without weakening policy.

Current evidence and contracts:

- [Coordination/integration](reviews/workflow-m1-resume-coordination.md).
- [Retry policy](reviews/workflow-m1-retry.md).
- [Claim selection](reviews/workflow-m1-claim.md).
- [Runtime reconciliation/assertion review](reviews/workflow-m1-parity.md).
- [Optimization measurements and decisions](reviews/workflow-m1-optimizations.md).

Previous saved-increment notes `workflow-tdd-{coordination,types,domain,parity}.md`
remain historical evidence; they are not the current verification totals.

## Reference accounting and remaining gates

The immutable reference remains **480 files and 5,858 textual declarations**;
`tests/reference/**` and frozen `workflow-mapping.json` were preserved. The
[progress ledger](../tests/parity/workflow-progress.json) now has **37 reviewed
source declarations, 38 counterpart records and 124 assertion links**. Links
retain exact source observations and explicit omitted scenario assertions; they
do not certify complete original scenarios or executed full parity.

The original 5,843-case/355-module capture is retained unchanged. Supplemental
registration adds the omitted pinned blob consumer and copied hash-verified
source evidence. Reconciliation accounts for **5,903 raw cases in 356 modules,
4,222 exact linked cases, zero admitted macro origins, 4,203 linked frozen
declarations, 1,681 unmatched runtime identities and 1,655 unresolved frozen
declarations**. All ninety source section reviews carry unproved macro
environments. Earlier literal recovery was withdrawn after compiled
counterexamples; no ExUnit/loader name-only exception or scaffold proof is
admitted. The unresolved set includes one documentation example.
Every raw identity and frozen ID retains its separate complement. Mixed case
scope, fixtures/setup and mandatory profiles remain pending; no cases were
invented, dropped, excluded wholesale or called passing by registration.
See [the reproducible collection guide](runtime-case-collection.md).

Continue in this order:

1. Inspect this handoff, actual working tree, review notes and current branch/CI.
   Run the complete wrapper after relevant new changes; full parity remains an
   expected failure until required execution and mappings actually complete.
2. Obtain reviewed compiler lineage or precisely scoped macro-environment proof
   before reinstating any macro origins. Review unmatched runtime cases, the
   documentation non-case, mixed workflow scope, setup/fixtures and mandatory
   profiles without editing frozen inputs or treating registration as execution.
   Port remaining original observations.
3. Finish M1: broader scope/value admission, policy installation decisions,
   partition/batch invariants, request-ID replay/model tests, committed encoding
   and deterministic time/hash preparation. The represented pure subset is not
   a completed workflow engine.
4. Pass M2 persistence/recovery gates before M3 consensus and M4 application,
   shared wire, HTTPS/TLS-TCP and worker work. Preserve atomic record/index/
   history/outcome/applied-position writes and uncertainty semantics.
5. Follow later workflow/query/retention milestones and explicitly review any
   deferred optimization before implementation. Commit/push only reviewed,
   verified changes; never reset/clean the original dirty Elixir checkout.

Read [AGENTS.md](../AGENTS.md), [architecture](architecture.md),
[the parity contract](workflow-test-parity.md) and
[the ordered implementation plan](implementation-plan.md). No production-ready
Raft, live Rust listener, complete workflow parity or unconditional p99 claim
exists yet.
