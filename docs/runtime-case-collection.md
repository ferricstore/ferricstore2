# Runtime case collection and source reconciliation

ExUnit registration, source identity reconciliation, Rust assertion links and
test execution are separate evidence. Collection loads the original helpers
and compiles test modules with `autorun: false`; it never calls `ExUnit.run/0`
or executes test/setup callbacks. Helpers may start the original application,
so use the isolated complete pinned checkout and its own build/data environment.
Full workflow parity remains incomplete and `require-parity` remains fail-closed.

## Pinned inputs and retained outputs

The source commit is `66586c596e271408d9184af32eb31cd7218c126e`. The immutable
manifest is `tests/reference/workflow-test-manifest.json`, SHA-256
`7df8c7885a64b6276e84517dffa4f1ead0f592f6d0996b398cdb61551c1bae61`.
Its 480 preserved files contain 5,858 textual declaration identities. Neither
the reference tree nor frozen `tests/parity/workflow-mapping.json` changes
during collection or progress generation.

The retained original files are
`tests/parity/collection/{collection-details,runtime-registration}.json`.
They registered 5,843 cases in 355 modules: 4,222 exact case links, 1,655
unresolved declarations and 1,621 runtime cases needing source reconciliation.
These inputs remain unchanged, including the raw source locations and tags.

The later collection lives under `tests/parity/collection/supplemental/`.
`collection-root-review.json` explicitly adds the pinned blob test consumer
omitted by the frozen root selection, without editing that selection. Both
additional files were compared with `git show <pinned>:<source_path>` and copied
under `source-supplement/` with source paths and SHA-256 references. The review
explains the omission, retains pending scope and does not certify execution.

The current reconciliation accounts for **5,903 registered cases in 356
modules**, **4,222 exact linked cases**, **zero admitted macro origins**,
**4,203 distinct linked frozen declarations**, **1,655 unresolved declarations**
and **1,681 unmatched runtime identities**. Multiple registered cases can link to
one textual declaration, so these counts are intentionally different.

One unresolved frozen ID, `0bc90782a5fe1e2c`, is a literal usage example inside
`support/isolated_instance.ex` documentation. Its reviewed reference-only nature
is recorded in `source-declaration-decisions.json`; it remains unresolved,
with no passing case invented. Twenty-seven additional runtime cases have no
frozen navigation identity. The other 1,654 unmatched cases have candidate
literal section identities whose macro environments remain unproved. Earlier
literal recovery was withdrawn after compiled alias injection counterexamples;
registration and source bytes remain unchanged. Mixed storage/Raft/dashboard/blob
scope requires case-by-case review: an identity link does not include or exclude
the whole module from workflow acceptance.

## Reproduce registration in the isolated source checkout

The recorded checkout is `/Users/yoavgea/repos/ferricstore2-elixir-baseline`.
It has the pinned `mix.lock`, compiled locked dependencies/test support and
the original native prerequisites. Do not use the dirty developer source tree
or share its `_build`/data directories. Record the actual command, exit status,
Elixir/OTP versions, source revision and resulting hashes. The collector checks
HEAD, tracked source cleanliness, `MIX_ENV=test`, the manifest digest and every
preserved source-file digest before registration.

The following is the command used for the supplemental capture. Preserve old
captures when creating later evidence; this output path denotes the current
reviewed snapshot, not a rolling execution report.

```sh
wf_rust_root=/Users/yoavgea/repos/ferricstore2-rust-design
wf_elixir_root=/Users/yoavgea/repos/ferricstore2-elixir-baseline
cd "$wf_elixir_root"
MIX_ENV=test mix run --no-start "$wf_rust_root/scripts/collect-elixir-cases.exs" \
  "$wf_rust_root" "$wf_rust_root/tests/parity/collection/supplemental"
```

`runtime-registration.json` remains schema 1 and contains only exact
path/line links. `collection-details.json` schema 2 retains **all** registered
cases, unmatched originals, helper/root load results, helper source hashes,
per-helper include/exclude profiles, final global profile, `mix.lock`/collector
hashes and supplemental source/root proof. It explicitly records test/setup
non-execution and fixture review not performed. Failed helper/root loads are
blockers, not passing or excluded cases.

The three umbrella test helpers configure different exclude lists. Sequential
loading changes global ExUnit configuration; the recorded final profile is not
an application-specific acceptance profile. Effective tags and these profiles
must remain visible for later profile/fixture planning. Registration does not
run excluded cases, required profiles, setups or assertions.

## Review exact macro provenance, then reconcile

The reconciler reads the retained supplemental details. It uses existing pinned
Jason BEAM files without compiling or starting the application. Run proposals
before reviewing additional section/use origins:

```sh
cd "$wf_rust_root"
elixir -pa "$wf_elixir_root/_build/test/lib/jason/ebin" \
  scripts/reconcile-elixir-cases.exs "$wf_rust_root" --propose
```

`macro-origin-candidates.json` is a proposal only. The conservative
`literal-section-origin-v2` rule navigates a unique exact consumer
module/path/use line, a named direct
`__using__` quote and a literal test/describe name at the exact preserved
declaration path/line. It never expands arbitrary macros, executes test bodies,
guesses nearest lines or joins by test name alone. Dynamic/conditional syntax,
ambiguous identity and lexical namespace aliases that may rebind the use stay
unresolved. Consumer uses and source macro definitions both require inspected
ordered module prefixes and inherited file prefixes. Unknown preceding uses,
require/import/generic macro calls, nested module/macro definitions, callback
attributes and dynamic forms remain unproved. No ExUnit.Case or Code.require_file
exception is admitted by name. Source-navigation fixtures use inert literal
metadata; their manually supplied identities are parser controls, not runtime
workflow executions. Compiled synthetic diagnostics retain actual alias
injection disagreement separately from original source tests.

Human review records precise source hashes and consumer uses in
`macro-origin-review.json`. `included-workflow` is a scope disposition;
`pending-case-review` preserves unresolved semantic scope. Neither value means
passed behavior. All current sections also carry `environment_decision=unproved`;
their origins cannot be admitted or restored, even with consistent hashes.
No scaffold proof is supported in this increment. Future proof needs a new
explicitly reviewed source/hash/scope contract. Reconcile the retained exact
identities and preserve all unproved complements:

```sh
elixir -pa "$wf_elixir_root/_build/test/lib/jason/ebin" \
  scripts/reconcile-elixir-cases.exs "$wf_rust_root" --reconcile
```

This writes separate `macro-origins.json` and schema-2
`reconciled-registration.json`. Original raw locations/tags are retained in the
hash-bound details; no recovered macro location is currently admitted.
Every raw identity is linked exactly once or retained in the unmatched
complement. Every frozen ID is linked or retained in the unresolved complement.

Runtime identity is SHA-256 of pinned commit, original path, decimal line,
module and name joined with single NUL separators, with no trailing NUL.
Linked case identity hashes commit, frozen declaration ID, frozen path, decimal
frozen line, module and name joined in the same way.
Origin identity hashes compact UTF-8 JSON with recursively sorted object keys,
excluding its `origin_id`. Exact text/hash anchors, source/use review, parser
tool hashes, copied supplemental bytes, helpers/profiles and unexecuted
setup/profile/pending-fixture requirements are checked structurally. Hashes do
not authenticate authorship or establish semantic completeness.

## Rust bookkeeping and separate execution evidence

After actual source/counterpart assertion review, regenerate progress and run
the validator. It retains every frozen ID and explicitly scoped omissions:

```sh
python3 tests/parity/generate-progress.py \
  --collection tests/parity/collection/reconciled-registration.json
cargo +1.99.0 run --locked --offline -p wf-parity-tests \
  --target-dir /private/tmp/ferricstore2-m1-parity-agent -- check-progress
elixir scripts/tests/runtime-reconciliation-test.exs
```

`cargo progress` and `scripts/check.sh` gate Rust progress/hash accounting.
The wrapper does **not** execute this standalone Elixir AST suite or the
original application suites. Run and retain those results separately; AST
regressions are parser checks, not original workflow executions. The generator
creates neither collection nor execution proof.

The coordinator separately executed 78 selected pinned original cases:
12 FIFO primitive/retry-policy cases, 56 priority/multi-state/retry section
cases and 10 FIFO invariant cases, all passing. Original commands, seeds,
location exclusions, logs and copied JUnit artifacts are recorded in the
[coordination review](reviews/workflow-m1-resume-coordination.md). These subset
results cannot certify all original cases, mandatory profiles or full parity.
Rust counterpart execution also remains separate from registration and ledger
links. The [owner review](reviews/workflow-m1-parity.md) records actual red/
green parser/validator checks, peer assertion review and remaining gaps.
