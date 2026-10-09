# Infrastructure testing review

Role: testing agent. Shared-file communication plus parent relay; no direct
agent messaging tool is available. Write scopes are test-support, parity
tooling/data, this review and `docs/testing-infrastructure.md`. No commit/push.

## Phase 1 implementation (in progress)

Read AGENTS, README, architecture, library decisions, Rust quality guidance,
workflow parity contract, preserved inventory and harness review. Published
both owned manifests promptly so the coordinator can resolve the workspace.
Pins: tempfile 3.27.0, serde 1.0.229 (derive), serde_json 1.0.151, sha2 0.11.0.
The cached exact source manifests and tempfile ownership/cleanup documentation
were read; RustCrypto/Serde primary documentation consulted. Lock resolution
and advisory/license checks have not yet run.

Test support implements owned temporary directories, a shared explicitly
advanced monotonic clock, and a one-shot bounded acknowledged fault barrier.
No runtime crate dependency, no workflow traits or reducer, no listener/store.
Code and tests for artifact integrity and fail-closed parity follow next.

## Questions for architecture and CI peers / parent

1. Classify all three M0 tools separately: runtime normal/build/optional/target
   dependencies must reject them; dev-only test support may be allowed.
2. CI should run `check-reference` as artifact integrity and report
   `require-parity` as intentionally incomplete, never green workflow parity.
3. Confirm the architecture parser permits effects only in tooling, and still
   inspects these tooling crates for inherited lint policy and forbidden unsafe.
4. Full ExUnit collection needs the pinned application and dependencies outside
   the standalone snapshot; no declaration index or manually passed status can
   close that gate. M0 leaves this evidence absent and every declaration unported.

Reciprocal peer review pending their actual files. No acceptance is inferred.

## Phase 1 implementation published and tested

Both crates, tests and separate M0 mapping/schema/data are now published.
`check-reference` pins the original manifest bytes, verifies all artifact hashes,
counts/category totals/IDs/ownership/lines/path safety and complete manifested
snapshot sets, plus 88 literal section-file edges. `require-parity` validates an
honest unported mapping then always fails in M0. It rejects manual passing
statuses, fake counterpart/runtime IDs, attached evidence and missing lineage.
The mapping contains all 5,858 source IDs as unported; zero runtime cases have
been collected or executed. All `tests/reference` bytes remain unchanged.

Actual checks on Rust 1.99.0:

- `cargo check -p wf-test-support -p wf-parity-tests --all-targets --locked --offline`: pass.
- `cargo clippy -p wf-test-support -p wf-parity-tests --all-targets --locked --offline -- -D warnings`: pass.
- `cargo test -p wf-test-support -p wf-parity-tests --all-targets --locked --offline`: 32 passed, none ignored/filtered.
- `cargo test -p wf-test-support -p wf-parity-tests --doc --locked --offline`: pass, zero doctests in these two crates.

The CLI setup-failure test added during CI reply follows this initial 32-test
run and will be included in the final targeted run. No broad or narrow lint
allowance was needed. No property dependency was added without a real model.

## Phase 2 actual peer/root reads and replies

Read root Cargo.toml/toolchain/aliases and all 15 package manifests; inspected
the production module topology and actual worker/application/API/node crate
roots. Read ADR-002, `docs/CONTINUE.md` and the parent integration record. The
runtime dependencies match the reviewed architecture and currently include no
external runtime libraries or tooling edges. Worker `client` is private with
private HTTP/TCP submodules; domain `value` and ports `client` are approved
contracts. Application internals are private and API raw-port compile-fail
doctests are present. No workflow implementation or listener was found.

Read both peer reviews, architecture manifest/root, CI workflow/check wrapper,
deny.toml and development guide. Architecture was still a crate-root scaffold
at that read; executable policy/fixtures were absent. Its final code review
remains pending actual arrival, not inferred from the proposal.

- **TEST-ROOT-01:** Accept root and handoff separation of infrastructure from
  unfinished M0 runtime collection and workflow behavior. The continuation
  order correctly requires real pinned-application collection first and preserves
  no-ID compatibility separately. No new root/ADR correction requested.
- **Reply to architecture question:** all fixture data is under `tests/parity`,
  separate from preserved `tests/reference`; invalid data is exercised in owned
  temp directories. `wf-test-support` is runtime-independent;
  `wf-parity-tests -> wf-test-support` is dev-only. None of these tests compiles
  forbidden fixture Rust as ordinary workspace production code. Please confirm
  source-path references in `include_bytes!` remain allowed tooling effects while
  every tooling crate's lint/unsafe policy is enforced.
- **Reply to CI-01:** parent/CI findings were real: sha2 0.11 returns an array
  without LowerHex and Rust 1.99 deprecates `fetch_update`. Fixed with explicit
  safe hex encoding and `try_update`. The owned check, Clippy and tests now pass.
- **Reply to CI testing question:** actual CLI tests distinguish successful
  integrity from nonzero incomplete parity; an additional setup test requires
  nonzero missing-artifact diagnostics and explicitly rejects an incomplete
  parity diagnostic for that failure. APIs additionally test byte tampering,
  changed preserved statuses, missing/extra snapshots and missing mappings.
- **TEST-CI-01:** Accept the wrapper's locked default-profile checks, warnings
  gate, pinned local audit requirement and integrity-only green path. The CLI
  negative gate is executed in workspace tests, so parity cannot silently become
  green in this increment. CI is read-only and pins checkout. Remote execution
  and the Linux/macOS matrix have not been independently run here.
- **TEST-CI-02 (pending peer revision):** CI's current development guide says
  no duplicate exceptions and MIT/Apache only; its review proposes exact
  tooling-only syn/thiserror exceptions and Unicode-3.0 after removing avoidable
  TOML duplicates. Please keep the final policy and prose synchronized and
  record the actual audit; no audit acceptance is claimed from source reading.

Final reciprocal read/resolution and targeted rerun pending the peer revisions.

## Published revision ready for reciprocal review

Final targeted run after the additional CLI setup test: strict Clippy passes;
33 tests pass (14 parity unit, 6 actual CLI/integration, 13 test-support), zero
ignored/filtered. `cargo reference` passes with 480/5,858/9 artifacts and 88
literal section edges. `cargo parity` exits 1 with the explicit absent
collection/counterpart/execution diagnostic. `git diff --check` passes and
`git diff --name-only -- tests/reference` is empty. No further code change is
currently needed in this write scope; final peer review remains active.

### Architecture policy/graph read as code arrived

Actually read `architecture/policy.toml`, `src/policy.rs`, `src/manifest.rs`,
`src/graph.rs` and the revised crate root. The reviewed production module
classification matches parent paths, including worker private HTTP/TCP client
modules and domain value/client-port restrictions. All three tooling crates
are explicitly classified. Owned dependency pins/features match the policy;
normal/build tooling edges are denied while test-support is dev-only. Declared
metadata is offline/locked and not filtered by host target, which preserves
visibility into inactive edges. Syntax/fixture/visibility files were still
being authored; their acceptance is pending actual reads and execution.

- **TEST-ARCH-01 (blocking error-path correction requested):** in
  `Policy::parse`, `document["schema"]` panics when the field is absent.
  Verified the cached toml 0.9.8 `Value::Index` implementation calls
  `get(index).expect("index not found")`. Please use checked `get` and return
  an error for missing/wrong schema, with a real negative test ensuring malformed
  policy does not panic. The checker API promises inspection failure through
  `Result`; malformed policy must remain a controlled failed check.

Read `src/source.rs` and `tests/workspace.rs` when they arrived. Unsafe, target
roots, source classification, lint suppression and worker direct/alias checks
are implemented; tooling effects such as owned `include_bytes!` fixtures are
permitted while its safety rules remain active. Two concrete supplementary
fixture requests arise from the actual visitor:

- **TEST-ARCH-02:** `visit_macro` permits runtime `vec!`, `format!` and
  `matches!`, but Syn does not visit their token arguments. A pure-domain
  `vec![std::time::SystemTime::now()]` is therefore not inspected by the current
  standard-library path rule; a worker's forbidden wire access can similarly
  hide in an allowed macro argument. Please reject these escape fixtures by
  inspecting supported expression macro tokens or conservatively closing the
  allowance until audited. The general documented macro limitation must not
  imply these explicitly permitted macros are checked when they are not.
- **TEST-ARCH-03:** `wf-domain::value` has an inward facade guard, but
  `wf-ports::client` currently has no analogous guard. A client module can
  `pub use crate::consensus::ConsensusPort`, making worker's approved
  `wf_ports::client` access expose a prohibited capability. Please add a negative
  facade fixture and deny client-module references/re-exports of server
  consensus/log/state/snapshot/authoritative capabilities. Static syntax still
  has broader documented limits, but this explicit sibling re-export is visible.

### CI revision read

Re-read the revised CI review, deny policy, wrapper and development guide.
**TEST-CI-02 resolved at document/policy level:** the final guide now names
verified Unicode-3.0 and exactly three versioned tooling duplicate exceptions;
the wrapper denies unmatched stale skips. CI records actual failure evidence
for the initial bans/license policy and validates Cargo's ordinary warning gate
with an isolated negative/control fixture. Accept this bounded change; actual
final audit/full wrapper remains parent/CI integration work. No advisory ignore,
floating action or runtime library adoption was introduced.

## Subsequent independent reads

Read all 86 production `.rs` files, not only crate roots: after removing blank
and comment lines, every file contains only the unsafe-forbid root attribute
and module declarations. This independently confirms the authorized scaffolding
scope. Re-read the refreshed lock: owned direct pins remain serde 1.0.229,
serde_json 1.0.151, sha2 0.11.0 and tempfile 3.27.0; the architecture parser now
uses toml 0.8.23. No reference preservation bytes changed.

Read the architecture unit tests, all published source/archunit fixtures,
archunit release evaluation and real consumer compiler tests. They test
forbidden declared edges, inactive optional/target/build/dev variants, package
coverage, features, alias/re-export/qualified access, narrow reasoned test
exceptions, malformed source, empty selections, ignore-directive limitations,
and concrete visibility/unsafe compiler errors with positive controls. This
is complementary infrastructure evidence, not workflow behavior. The three
specific visitor/policy findings above still await their revisions/tests.

Read CI's received reply agreeing with TEST-ARCH-01/02/03 and its subsequent
independent execution record: our final 33 tests pass, and its exact wrapper
audit passes advisories/bans/licenses/sources against the refreshed lock with
dev duplicates included. The CI peer also reports six actual dependency-policy
negative/control probes and six wrapper-control probes, plus the ordinary
compiler warning negative/control. These are received peer results, not
independently rerun or remote CI execution by this role. Accept the final CI
code/policy with its recorded versioned exceptions and unchanged integrity-only
green path. Root/handoff acceptance stands. Architecture acceptance remains
pending the explicitly relayed revisions and their actual final code/tests.

## Resolution round: actual architecture revision

Independently re-read the revised `policy.rs`, required root lint checks,
client facade guard, expression macro grammar/visitor, and new macro/client/root
negative fixtures plus positive controls. Executed
`cargo test -p wf-architecture-tests --all-targets --locked --offline`:
**20 passed**, zero failed/ignored/filtered (12 unit, 4 archunit, 3 real compiler
visibility, 1 complete workspace). The original three findings are resolved:

- **TEST-ARCH-01 accepted:** checked `get(schema/packages)` returns errors for
  empty, missing, wrong-type and missing-package policies; actual malformed
  policy test passes without a panic.
- **TEST-ARCH-02 accepted:** parsed comma expressions, vec repeats and matches
  patterns/guards are visited; malformed supported grammar fails closed. The
  actual macro fixture rejects clock/environment/filesystem/network effects
  and worker wire access; pure macro positive controls pass.
- **TEST-ARCH-03 accepted:** client facade references/reexports of sibling server
  capabilities and non-value domain modules are rejected; the explicit
  consensus/state/snapshot alias fixture fails and approved values pass.
- Parent **INF-ROOT-08 accepted:** required rust_2018_idioms deny is checked and
  a weakened isolated root manifest is rejected by an executed negative test.

### Final strict-lint follow-up requested

The additional independent command
`cargo clippy -p wf-architecture-tests --all-targets --locked --offline -- -D warnings`
still fails on the current revision. **TEST-ARCH-04**: remove redundant
`.to_owned()` after `replace()` in `module_name`; split/move the 105-line
`Inspector::path` logic; use diagnostic-preserving empty-vector assertions
(Rust 1.99 `assert_is_empty`) in positive test controls; remove the unnecessary
`CheckResult` return from the predicate-only malformed-policy test. No blanket
allowance is requested. These are concrete lint integration fixes, not new
architectural design objections. All 20 architecture tests already pass.

Final root/handoff and CI code acceptance remains recorded above. Final
architecture code acceptance awaits this last strict-lint correction; no
other unresolved functional finding is identified by this role. Parent will
run the final whole-workspace wrapper. Owned 33 tests/code remain unchanged.

## Final accepted / unresolved record

**Final architecture code accepted.** Re-read the actual strict-lint revision:
`module_name` returns its replacement String directly; path/facade checking is
split; positive assertions retain diagnostic values; the predicate-only test
no longer returns an unnecessary Result. No suppression was added. Independently
executed the final exact targeted Clippy command above: pass. Re-executed the
final architecture all-target test command after this refactor: **20 passed,
zero failed/ignored/filtered**. TEST-ARCH-04 / CI-06 are resolved; their failure
records above describe earlier drafts. TEST-ARCH-01/02/03 and INF-ROOT-08 remain
accepted with the executed negative and positive fixtures.

Read the published final architecture-enforcement guide and architecture
author's complete reciprocal response. It accurately documents parser/graph/
compiler scope, macro grammar inspection, transitive/semantic limits, fixture
separation and conditional future gates. **Final CI code/policy accepted** after
the actual wrapper/config/development/review reads and received independent
audit/negative-probe evidence already recorded. **Final parent root, ADR-002
and continuation/handoff accepted**: all 86 production source files are only
scaffolds, the graph/lint/toolchain/aliases agree with the reviewed boundaries,
and unfinished runtime collection/workflow behavior remain explicit.

The coordinator subsequently reports the complete final
`bash scripts/check.sh` exited 0: formatting, compiler/Clippy with warnings
denied, 53 infrastructure tests and three scaffold compile-fail doctests, docs,
architecture, reference integrity and all four audit checks plus stale-skip
denial. This is a received coordinator result, not another independently run
wrapper by this role. The owned checks and independent architecture checks are
listed above. No unresolved implementation/review finding remains from this
testing role. Remote CI/other-host execution is not claimed.

The remaining **future acceptance gates** are real ExUnit runtime collection,
source/runtime/assertion reconciliation, compatible Rust counterparts, actual
execution under required profiles, real adapter fault/crash/cluster binding,
and all workflow implementations. Every preserved declaration is unported.
`cargo reference` is M0 artifact integrity only; `cargo parity` exits 1. These
scope gates are not waived by the infrastructure pass.

### Changed files owned by this role

- `crates/wf-test-support/Cargo.toml`, `src/lib.rs`, `src/resources.rs`,
  `src/clock.rs`, `src/fault.rs`, `tests/infrastructure.rs`.
- `crates/wf-parity-tests/Cargo.toml`, `src/lib.rs`, `src/main.rs`,
  `src/artifact.rs`, `src/reference.rs`, `src/mapping.rs`, `tests/cli.rs`.
- `tests/parity/README.md`, `workflow-mapping.schema.json`,
  `workflow-mapping.json`.
- `docs/testing-infrastructure.md`, this review.

All 18 owned files are published on the shared filesystem. No parent/peer write
scope, preserved reference byte, source checkout, commit or push was changed
by this role. Continue from `docs/CONTINUE.md`; implementation handoff complete.
