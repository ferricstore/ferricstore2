# Infrastructure CI review

Owner: CI/local-policy agent. Scope: `.github/**`, developer check wrapper under
`scripts/**`, `deny.toml`, `docs/development.md`, this review. Shared filesystem
changes only; no agent spawned and no commit/push. Date: 2026-10-09.

Final status: **implementation and independent reciprocal review accepted**.
Historical draft failures below are resolved by the final revision and checks.

## Phase 1 implementation

Read `AGENTS.md`, README, architecture, library decisions, the Rust quality note,
implementation plan, parity contract, and the earlier quality/parity reviews.
Current authorization permits M0 infrastructure only. Published the default
feature, locked, warnings-denied wrapper; Linux/macOS CI invoking the wrapper;
dependency audit policy without advisory ignores; and setup documentation.

Reviewed upstream cargo-deny **0.20.2** (MSRV 1.88.0, edition 2024,
MIT OR Apache-2.0). Parent installation command:

```bash
cargo +1.99.0 install cargo-deny --version 0.20.2 --locked --root .tools
```

No local install was performed by this agent. Wrapper requires the exact local
binary and never skips a missing audit. It runs reference integrity, not
`require-parity`, in the green M0 path. `cargo parity` must still fail.
Sources: [pinned manifest](https://github.com/EmbarkStudios/cargo-deny/blob/0.20.2/Cargo.toml),
[release](https://github.com/EmbarkStudios/cargo-deny/releases/tag/0.20.2),
[private-license semantics](https://embarkstudios.github.io/cargo-deny/checks/licenses/cfg.html),
[advisories](https://embarkstudios.github.io/cargo-deny/checks/advisories/cfg.html),
[CLI](https://embarkstudios.github.io/cargo-deny/cli/common.html).

Checkout **v7.0.1** SHA
`3d3c42e5aac5ba805825da76410c181273ba90b1` independently verified by
`git ls-remote https://github.com/actions/checkout.git refs/tags/v7.0.1`.
Sandbox DNS initially failed; the approved read-only escalation succeeded.
No floating actions, secrets, publish job or write token are requested.

## Questions for reciprocal review

1. Architecture peer: does the real workspace policy reject runtime imports of
   tooling, missing lint inheritance, broad allowances and every classified
   optional/build/target dependency? Confirm the default profile is currently
   feature-free and add a negative fixture for policy drift if needed.
2. Testing peer: confirm `check-reference` exits successfully only after complete
   reference integrity validation and `require-parity` fails on the current
   incomplete manifest. Can CLI negative tests distinguish setup/integrity
   failure from deliberate incomplete parity?
3. Parent: please install the pinned audit binary locally using the command
   above, keep `.tools` ignored, and confirm root warnings/lint policy plus exact
   aliases. Any new third-party license or duplicate-version requirements must
   be explained before policy changes.

## Checks and final disposition

### Phase 2 actual reads and findings

Read both peers' review files, all three tooling manifests, the published
test-support implementations/tests, and parity artifact/reference/mapping/CLI
implementations. Architecture initially contained only the crate-root scaffold;
its executable policy/tests are still pending, so no architecture acceptance is
claimed. Also read root Cargo/toolchain/config/gitignore and the 12 production
manifest/source topology. Root aliases match the required commands, production
sources are empty documented module scaffolds, tooling packages inherit private
package/lint policy, and `.tools` is ignored.

- **CI-01 (testing, blocking until revised):** the actual locked workspace check
  fails because `sha2 0.11.0`'s digest array does not implement `LowerHex` at
  `artifact.rs:15`. Rust 1.99 also warns that `AtomicU64::fetch_update` is
  deprecated; warnings-denied wrapper requires `try_update`. Parent has relayed
  these findings to the test owner. No source outside this agent's scope edited.
  Re-read the owner's explicit encoding / `try_update` correction and reply;
  independently ran its 33 tests successfully. **Resolved.**
- **CI-02 / INF-ROOT-07 (architecture/dependency policy):** actual Cargo metadata
  and `cargo tree --locked --duplicates` show duplicate toml, serde_spanned,
  toml_datetime and winnow versions caused by direct `toml =0.9.8` alongside
  archunit's `cargo_metadata -> cargo-util-schemas -> toml ^0.8.9`. Architecture
  owner should evaluate using `toml =0.8.23` for this M0 manifest parser before
  accepting four avoidable duplicate exceptions. The parser needs ordinary
  current workspace tables, not proposed TOML 1.1 syntax.
  Architecture owner accepted this finding and changed its manifest/policy to
  exact toml 0.8.23, parse-only. Parent refreshed the central lock. Read revised
  manifest/lock and executed the complete strict dependency audit successfully.
  **Resolved.**
- **CI-03 / INF-ROOT-07 (unavoidable current-tool transitive duplicates):** syn
  2.0.119 is required by archunit and thiserror-impl 1.0.69; syn 3.0.6 is required
  by current Serde/ICU derives and thiserror-impl 2.0.21. `cargo-util-schemas
  0.2.0` requires thiserror `^1.0.56`, while archunit/cargo_metadata require
  thiserror `^2`. These disjoint upstream requirements cannot unify in the
  reviewed tool graph. After removing avoidable TOML duplicates, propose only
  exact `syn@2.0.119`, `thiserror@1.0.69`, `thiserror-impl@1.0.69` duplicate
  skips with documented tooling-only chains; duplicates remain denied globally.
  These three exact skips are now published with chain/owner/re-review reasons.
  The wrapper denies `unmatched-skip`, so removal/version changes fail instead
  of leaving obsolete exceptions. No tree-wide or name-wide skip was added.
  Actual cargo-deny source/config review exposed its default exclusion of dev
  duplicate versions. Set `multiple-versions-include-dev = true`, matching the
  required complete tool audit; this makes the three reviewed exceptions apply
  to the real graph instead of suppressing an absent/default-filtered package.
- **CI-04 / INF-ROOT-07 (license):** all 83 resolved third-party package manifests
  inspected through `cargo metadata --locked`. MIT/Apache alternatives satisfy
  every expression except Unicode-3.0-only ICU/Unicode utilities and
  unicode-ident's `(MIT OR Apache-2.0) AND Unicode-3.0`. Add only verified
  `Unicode-3.0`; no BSD/ISC/Unlicense/LLVM/LGPL license is needed merely because
  an OR alternative appears. Cached exact crate license texts will be read
  before changing the allowlist. Exact `Cargo.toml.orig` / normalized manifests
  and LICENSE text from cached pinned packages were subsequently read for the
  tool chain, ICU, unicode-ident, yoke and zerovec. Published only Unicode-3.0
  alongside MIT/Apache-2.0. Actual cargo-deny license failures confirmed that
  precise requirement; the first audit failed bans/licenses and passed sources.
- **CI-05 (root warning gate):** the script sets `CARGO_BUILD_WARNINGS=deny` as
  documented for pinned Cargo 1.99. A deliberate ordinary rustc warning in an
  isolated temporary crate will validate its actual fail behavior, independently
  of Clippy and manifest deny lints. Executed: isolated `pub fn warning_probe`
  with an unused variable fails (exit 101, `warnings are denied by build.warnings
  configuration`) under the wrapper's setting; the same fixture succeeds with
  the warning when the setting is absent. No Clippy or inherited deny lint used.

Read the parent's `docs/CONTINUE.md`, ADR-002 and infrastructure integration
record. They correctly distinguish scaffolds, artifact integrity, future ExUnit
collection and later workflow acceptance. The continuation path is suitable;
the parent should replace in-progress validation text after final review.

Wrapper orchestration probes now pass all six: reject skip arguments; fail on
missing local audit; fail on wrong audit version; propagate Clippy failure and
stop later steps; propagate the final audit's failure without printing success;
run every required command in order with pinned toolchain/warnings/locked
arguments and explicit incomplete parity wording. These use stub commands in
isolated temp directories, proving wrapper control flow only. The first test
harness draft used Bash `-v`, unavailable in macOS system Bash; that harness
expression was corrected to a portable parameter check and all probes rerun.

The initial full wrapper run fails during rustfmt while architecture files are
being published (`source.rs` not yet present). This is a shared-draft state,
not a reason to skip formatting. Final wrapper acceptance awaits the completed
peer draft and TOML dependency unification.

Completed now: `bash -n scripts/check.sh`, `git diff --check`, and real missing
local-audit negative check (exit 1 with explicit install command); Cargo metadata
and duplicate graph reads succeeded. The initial compile needed approved cache
write access after sandbox extraction failure, then exposed CI-01. Additional
wrapper failure, compiler warning, dependency policy, workspace and reciprocal
checks remain in progress. No workflow parity or remote CI execution claimed.

### Reciprocal revision reads and independent execution

Read the test peer's explicit replies and acceptance of the CI wrapper/policy;
its TEST-CI-02 wording mismatch is resolved in its subsequent actual-file read.
Read the complete architecture policy/graph/manifest/source visitor, unit
fixtures, archunit release evaluation, real workspace check and compiler
visibility consumer tests as they arrived. The 15-package classification,
normal/build/tooling restrictions, inactive optional/target dependency checks,
exact external pins/features, root/module visibility and compiler diagnostic
checks address the phase-1 questions. Narrow test allowances have a required
specific Clippy lint, function scope and a substantive reason; there are no
blanket allows in the current source. Normal runtime external libraries remain
unadopted. Parsed fixture Rust is data, never an ordinary Cargo target.

Agree with testing peer's actual **TEST-ARCH-01/02/03** findings after reading the
same code: unchecked missing schema access can panic; the then-allowed macros'
token arguments are opaque to Syn's visitor; and ports::client lacked the
domain-value-like inward facade guard. Architecture owner is fixing these.
Also agree with parent **INF-ROOT-08**: the root has rust_2018_idioms deny but
the checker must protect it from policy drift. Final architecture acceptance
awaits those actual revisions and their negative tests, not the author's intent.

Independently executed `cargo test -p wf-test-support -p wf-parity-tests
--all-targets --locked`: **33 passed**, zero failed/ignored/filtered. This includes
the actual missing-setup, unknown-command, integrity-pass and deliberate
incomplete-parity CLI tests. No workflow case executed.

Parent installed local cargo-deny 0.20.2. After approved advisory-cache access
and parent-only lock refresh, independently executed the wrapper's exact audit:

```bash
.tools/bin/cargo-deny --locked --workspace check --deny unmatched-skip advisories bans licenses sources
```

**Pass: advisories, bans, licenses, sources.** Final lock read has 107 total
packages: 15 first-party and 92 third-party, including the reviewed pins and
exact three duplicate exceptions. The audit includes dev duplicate versions
and dev/build licenses. No lockfile or peer source changed by this agent.
Refreshed locked metadata also contains exactly 92 third-party packages.
Verified all 18 Unicode-only resolved crates' license texts identify Unicode V3;
the nineteenth Unicode-requiring package is unicode-ident's AND expression.
No additional license is needed in the final resolved graph.

Six actual cargo-deny configuration probes passed in temporary files against
the resolved graph: baseline licenses/sources pass; removing MIT/Apache still
rejects third parties despite private exemption; removing private exemption
reports 15 unlicensed private first-party packages; removing the allowed
registry rejects third-party sources; adding a nonexistent exact duplicate skip
fails with `unmatched-skip`. The first source probe harness expected the config
name as its diagnostic code; corrected to the tool's actual `source-not-allowed`
code and reran the full probe set. No policy change was made to satisfy that
harness error. No advisory ignore or license grant to private first-party code.

**CI-06 (architecture strict lint integration, pending):** the next actual
`bash scripts/check.sh` passed formatting and warnings-denied locked workspace
compilation, then failed Clippy in architecture's newly published draft:
`collapsible_if` in source visitor (191, 318/319), `assert_is_empty` in four unit
assertions, and `unnecessary_wraps` in the malformed-source test. Owner should
make the structural/test diagnostic improvements, with no lint suppression.
These are additional concrete gate results; the full wrapper has not passed.
After the schema/macro/client-facade/root-lint revisions appeared, another real
wrapper run again passed formatting/compiler checks but exposed new
`implicit_clone` and `too_many_lines` in the visitor plus the not-yet-revised
assertion/return-style findings. Owner is still completing the draft; hold the
final run until its ready notification to avoid testing files mid-revision.

Additional current-graph negative audit: removed only the three exact duplicate
skips in a temporary policy; cargo-deny exits 2 and reports precisely `syn`,
`thiserror` and `thiserror-impl` duplicates. This confirms the exceptions are
necessary for the reviewed graph and global duplicate denial still operates.

### Final accepted / unresolved record

Actually re-read the corrected policy parser, expression-macro parser/visitor,
client facade guard, root lint requirements and their negative/positive
fixtures, plus the completed architecture enforcement guide. The final code
uses checked schema/package access, recursively visits vec/format/matches
arguments including repeats/patterns/guards, rejects client-facade server
siblings and non-value domain access, and protects rust_2018_idioms deny.
No Clippy allowance was introduced to bypass the earlier draft findings.

**Accepted architecture peer.** TEST-ARCH-01/02/03 and INF-ROOT-08 are resolved
by the actual code and passing negative tests. CI-06 / TEST-ARCH-04 describe
earlier draft Clippy failures; the final strict workspace Clippy check passes.
Twenty architecture tests pass: twelve unit/policy, four archunit release
evaluations, three real compiler visibility tests and one actual workspace
check. The parser/framework limitations in the guide remain honest future
review gates, not current failing checks or workflow-parity evidence.

**Accepted testing peer.** Independently read its final helpers, artifact and
mapping checkers, unit/CLI tests, review replies and testing guide. CI-01 is
resolved. Its 33 tests pass, including real integrity-success, incomplete-parity
failure and distinct missing-setup CLI behavior. The immutable snapshots and
honest unported mapping remain acceptance inputs; no workflow reducer or
fabricated execution path is present.

**Accepted parent roots and handoff.** Independently read root toolchain,
Cargo/lint/alias settings, all manifest/module topology, README, ADR-002,
CONTINUE and integration record. They preserve reviewed dependency directions,
private package/lint inheritance, empty production scaffolds and the separation
between this infrastructure increment, unfinished ExUnit collection and future
workflow behavior. Parent owns final status integration; continuation is
`docs/CONTINUE.md`. No parent/peer files were edited by this role.

**Independent final unified check: pass, exit 0.** Executed
`bash scripts/check.sh` against the corrected shared sources on the native
macOS host. The first complete run passed every compiler/test/doc/reference
stage and failed only at the sandbox's read-only RustSec cache lock; reran the
same complete wrapper with approved cache/network access. It passed formatting,
warnings-denied locked check/Clippy, **53 infrastructure tests**, **3 compile-fail
doctests**, warnings-denied rustdoc, the **20-test** architecture alias,
reference integrity (**480 files / 5,858 textual declarations / 9 dirty inputs /
88 literal section edges**), and **all four** audit checks including
`--deny unmatched-skip`. The repeated architecture alias is not counted as
additional unique tests. `cargo parity` separately exited **1** with the
explicit absent-runtime-collection/counterpart/execution diagnostic.
`git diff --check` passes; `git diff --name-only -- tests/reference` is empty.

**Unresolved current infrastructure findings: none.** Authored Linux/macOS CI
has not run remotely; local verification covers native macOS, not a Linux
execution. Workflow runtime collection, assertion ports and behavioral parity
remain intentionally incomplete in the continuation plan. No code, workflow
or extra tooling work is needed in this assigned scope. No commit/push occurred.

Changed files: `.github/workflows/check.yml`, `scripts/check.sh`, `deny.toml`,
`docs/development.md`, `docs/reviews/infrastructure-ci-review.md`.
