# Workflow progress gate: parity agent

## Ownership and early schema proposal (2026-10-09)

Owned writes: `crates/wf-parity-tests/**`, new `tests/parity/**` progress
artifacts, and this note. The reference snapshots, original manifest and
`tests/parity/workflow-mapping.json` remain frozen. The coordinator owns actual
ExUnit collection scripts/artifacts. No commit, push, subagent, runtime dependency
or shared configuration change is authorized for this agent.

Read: `AGENTS.md`, `README.md`, `docs/architecture.md`,
`docs/reviews/workflow-tdd-coordination.md`, `docs/workflow-test-parity.md`, and
the existing preservation/full-parity implementation. Existing `require-parity`
has no passing path; this increment keeps that contract.

Proposed **progress schema 1**, coordinator please relay before collecting:

```json
{
  "schema_version": 1,
  "source_commit": "66586c596e271408d9184af32eb31cd7218c126e",
  "reference_manifest_sha256": "7df8c7885a64b6276e84517dffa4f1ead0f592f6d0996b398cdb61551c1bae61",
  "runtime_collection": null,
  "declarations": [
    {
      "source_declaration_id": "<each of all 5858 frozen IDs exactly once>",
      "runtime_case_ids": [],
      "rust_assertions": []
    }
  ]
}
```

`runtime_collection` is optional/null until a real collection exists; otherwise
it is `{ "path": "<checkout-relative JSON artifact>", "sha256": "<64 hex>" }`.
No statuses or execution proof fields are accepted in this ledger. A Rust
assertion mapping is `{ "scenario_id": "...", "rust_file": "...",
"rust_file_sha256": "...", "rust_module": "...", "rust_function": "...",
"assertions": [{ "source_line": 1, "source_text": "...", "rust_line": 1,
"rust_text": "..." }], "limitations": ["..."] }`. Lines/text and Rust file
hashes are checked against actual files, function/module scope and `#[test]`
identity are checked. Registration is separate from source-derived assertions;
neither certifies executed full parity. Execution commands/results belong in
notes and saved logs, not handwritten pass fields.

Proposed **coordinator registration schema 1**:

```json
{
  "schema_version": 1,
  "source_commit": "66586c596e271408d9184af32eb31cd7218c126e",
  "reference_manifest_sha256": "7df8c7885a64b6276e84517dffa4f1ead0f592f6d0996b398cdb61551c1bae61",
  "collection_kind": "exunit-registration-only",
  "collection_complete": false,
  "blockers": ["remaining prerequisites or unresolved declarations"],
  "unresolved_declaration_ids": ["<exact complement of cases' source IDs>"],
  "cases": [
    {
      "case_id": "<identity hash described below>",
      "source_declaration_id": "<frozen 16 hex ID>",
      "source_path": "apps/.../test.exs",
      "source_line": 1,
      "module": "Elixir.Ferricstore.ExampleTest",
      "name": "test full ExUnit name including describe/expanded parameters",
      "tags": {}
    }
  ]
}
```

`source_line` is the frozen declaration's line, not an arbitrary nearby line.
Case ID: SHA-256 of UTF-8 components joined by one NUL byte, **no trailing NUL**:
`source_commit`, `source_declaration_id`, `source_path`, decimal `source_line`,
`module`, `name`. Real ExUnit names carry describe/parameter expansion;
effective tags retain fixture/setup metadata the collector can observe.
Additional metadata should be agreed with this agent before use. Unknown fields
are rejected. A complete collection cannot have zero cases or unresolved IDs;
partial collection must explicitly retain its omissions. The progress ledger's
per-declaration runtime IDs must equal the collection's per-declaration IDs,
without duplicates or cross-source identity reuse.

No collector implementation or fabricated collection is included here. A missing
collection is explicit incomplete reconciliation, never a zero-case parity pass.
The progress validator validates accounting and actual assertion links; semantic
equivalence remains a human review obligation and the full gate stays closed.

## TDD evidence

Before any progress implementation, wrote ten CLI integration tests in
`crates/wf-parity-tests/tests/progress.rs` covering honest missing collection,
missing ledger, dropped/duplicate/fabricated IDs, fake statuses/provenance,
actual assertion links, absent/tampered/comment-only/wrong-scope counterparts,
empty/cross-declaration/nonassertion links, required registration/bidirectionality,
collection hash/scoped identity/duplicates/zero-case completeness, and fake case
results. Fixtures are temporary synthetic validator inputs, never committed
counterpart/execution proof.

Observed behavioral RED before implementation:

```text
cargo +1.99.0 test --locked --offline \
  --target-dir /private/tmp/ferricstore2-wf-parity-agent \
  -p wf-parity-tests --test progress
test result: FAILED. 0 passed; 10 failed; 0 ignored
expected "runtime case IDs require collection": unknown subcommand
expected "missing or unknown source declaration IDs": unknown subcommand
```

Full output saved at `/private/tmp/workflow-parity-red.log`. All ten tests ran
and failed on the absent `check-progress` operation or missing specific
diagnostics; rejection tests require precise messages so unknown-subcommand
failure cannot falsely count as coverage. An earlier test-helper compile error
(`LowerHex` unavailable for sha2 0.11 output) was fixed before this run and is
not behavioral red. That first compile accidentally used the default target;
all subsequent Cargo builds use the separate target directory above.

After saving that red record, implemented `check-progress`, comprehensive
declaration equality, optional hash-checked registration/reconciliation, and
source/Rust assertion links. The identical command passed **10 tests, 0 failed,
0 ignored**, with output `/private/tmp/workflow-parity-green-initial.log`.
Counterparts must contain an active `#[test]` function in the exact inline
module scope; lexer excludes comments and literals. Ignored or unsupported
conditional tests cannot count. This lexer deliberately does not expand macros
or resolve external module paths. Source assertion anchors are exact lines
within a declaration's textual region, not an Elixir AST/equivalence proof.

Second behavioral RED, before parser hardening: added five navigation tests.
Pinned `test --lib rust_source::tests` returned **3 passed, 2 failed**;
`nested_function_assertions_do_not_belong_to_the_outer_test` showed that an
uncalled nested helper assertion could incorrectly link to its outer test;
`excessively_nested_inline_modules_are_rejected_before_recursive_navigation`
showed missing bounded recursive navigation. Saved observed output at
`/private/tmp/workflow-parity-parser-red.log` before implementing either fix.

After excluding nested named helper bodies and bounding inline-module recursion
to 64, the complete parity package passed **35 tests** (19 unit, 6 existing CLI,
10 new progress CLI), with no ignored tests. Output:
`/private/tmp/workflow-parity-green.log`.

Peer finding **T-P1 accepted**: actually read the types peer's note and independent
temporary-fixture reproduction. Added an active `#![cfg(test)]` control and
disabled crate/module inner-attribute regression before the fix. Observed RED:
pinned `test --lib crate_and_module_inner_conditional_attributes` returned
**0 passed, 1 failed**, with `counted disabled crate: #![cfg(any())]`.
Evidence: `/private/tmp/workflow-parity-inner-cfg-red.log`. Resolution pending
owner implementation immediately after saving this record.

**T-P1 resolved by this owner**: inner crate/module attributes are now read
separately from outer item attributes; unsupported inner conditions discard
every candidate in that scope. Active `cfg(test)` remains valid. The focused
six-test parser suite is green: **6 passed, 0 failed**, output
`/private/tmp/workflow-parity-inner-cfg-green.log`. Types peer please re-review
the owned fix. The progress binary was rebuilt and the real nine-counterpart
ledger validates with **5858 declarations, 9 source-derived declarations,
9 counterpart mappings, 22 direct assertion links**. Registration is still
absent, all 5858 remain unresolved, and full parity explicitly incomplete.
Output: `/private/tmp/workflow-parity-progress.log`. The new offline generator
only reads manifest/actual test files and attaches reviewed assertion anchors;
it never creates collection or execution proof. Coordinator collection hook:
`python3 tests/parity/generate-progress.py --collection <relative-artifact>`.

## Peer communication and review

Read both peer API/source notes after publication. Types preserves arbitrary
binary `fpk:` components and checked identity/time/fence values. Domain correctly
distinguishes legacy duplicates from request-ID replay, source ownership guards
from automatic deadline revocation, and `PolicyResolved` inputs from installed
policy behavior. Read the full pinned three-case `partition_key_test.exs` and
six-case `fencing_retry_test.exs`; source HEAD matches the expected commit and
the pinned checkout is clean. Actual peer code review remains pending; map only
actual assertions. No peer paths are edited.

Completed types code review: read all five runtime modules, crate root, and all
three integration-test files. Exact source partition inputs and expected bytes
are preserved by the three named counterparts. Storage `RecordId` accepts
nonempty opaque bytes through 65,535, and separate composed-key checks include
the prefix; zero/full-u64 constructors and overflow paths agree with the
published contract. Decimal prefix size and fallible output reservation are
checked before encoding. The exhaustive collision and widened-u128 arithmetic
oracles supplement the source tests. **No correctness defect found in this
increment.** Sealed state-key ownership tests remain explicitly unported;
primitive bounds are not mapped as their counterparts. Domain's create/claim
code and tests were also read; terminal/retry behavior is still unavailable at
this review point, so final domain review remains pending.

Domain review now includes all six fencing functions and the scenario harness,
implemented create/claim/transition/complete/fail/retry paths, history, lease,
retry and error modules. Compared duplicate guards with pinned
`raft/state_machine/sections/flow_history_writes.ex:461-509`, record mutations
with `flow_transition.ex:452-503,1080-1148,1325-1365` and completion with
`flow_claim_native_plan.ex:546-597`. The six source assertion sequences preserve
successful first calls, duplicate acceptance and exact one-event cardinality,
intervening-transition stale rejection, and fail-to-complete stale rejection.
History count measures emitted events, not storage; that limitation is attached
to every fencing mapping. The source retry-without-run-at case uses caller
`PolicyResolved` times in Rust; policy execution remains unported.

Read the types peer's actual independent T-P1 re-review: its rebuilt original
fixture now rejects (exit 1), and its 20 library tests passed. This owner also
reran the original fixture and observed the same rejection, saved in
`/private/tmp/workflow-parity-peer-cfg-recheck.log`.

**D-P1 accepted from domain's actual code review and reproduction.** Inert macro
token trees (`stringify!`/`quote!`) could expose a fake assertion token. Added
pre-fix regressions with a direct active assertion control and nested inert
macros, plus a disabled assertion-statement regression found during owner
review. Pinned `test --lib rust_source::tests` observed **6 passed, 2 failed**:
`inert stringify! tokens counted as an assertion` and disabled `cfg(any())`
statement accepted. Saved output `/private/tmp/workflow-parity-macro-red.log`
before implementing either fix. Resolution: pending owner implementation.

**D-P1 resolved by this owner**: nonassertion macro token trees are opaque;
direct assertion macros can link only their own invocation line, and their
argument trees are not recursively interpreted as separate assertions.
Unsupported conditional statements reject the link. The same focused parser
suite passed **8 tests, 0 failed**; saved
`/private/tmp/workflow-parity-macro-green.log`. Domain peer please re-review
this fix against the original `stringify!` reproduction. This conservatively
omits assertions nested in arbitrary macro expansions; none of the nine
reviewed counterparts requires that support.

Actual command execution evidence is separate from registration/mapping. This
agent independently ran the real `wf-types --test partition_key` (**8 passed**,
including the three source counterparts) and real `wf-domain --test
fencing_retry` (**6 passed**). Both used pinned Cargo, locked/offline flags and
this agent's isolated target. Logs:
`/private/tmp/workflow-parity-types-execution.log` and
`/private/tmp/workflow-parity-domain-execution.log`. These prove the reported
Rust subset ran; they do not execute all frozen source cases or prove full
workflow equivalence. No command results/pass fields were written to the ledger.

**P-D1 — inaccurate pinned source ranges in fencing test comments (open).**
The six test comments currently cite ranges 33-54, 57-74, 77-108, 111-158,
161-188 and 191-214. The actual frozen declarations are lines **33, 53, 70,
96, 135, 159**, and the final source file ends at 180. Correct the comments to
the pinned ranges **33-51, 53-68, 70-94, 96-133, 135-157, 159-179**. This
matters because source identity/assertion lineage is an acceptance requirement;
the new ledger already anchors the actual pinned lines. Owner: domain agent;
no peer file was changed here. No behavior defect was found in the six fencing
assertion translations. The types peer's independently found numeric-fence
admission issue remains for the domain owner to resolve.

Final reciprocal review: read domain's corrected six source comments and stable
IDs, current `workflow::reduce` guard admission and `value::DomainError` changes,
and its updated evidence. **P-D1 is resolved** with the exact pinned ranges and
IDs in the actual test file; ledger hashes/anchors were regenerated afterward.
The types peer's T-D1 numeric-fence issue is resolved in the actual reducer:
every represented caller fence is capped before record lookup/no-op checks,
with the exact source error string and boundary coverage. Read domain's
independent **D-P1 re-review**: it rebuilt the original reviewer harness against
the fixed parser and ran the original reproduction plus all eight parser tests,
**9 passed, 0 failed**, no peer edits. T-P1 and D-P1 are independently closed;
both peers' actual code and notes have been reviewed. No outstanding finding
remains against this parity increment.

## Final validation and coordinator handoff

Owned files: modified `crates/wf-parity-tests/src/{lib,main,reference}.rs`; added
`src/{progress,registration,rust_source,rust_tokens}.rs` and
`tests/progress.rs`; added `tests/parity/generate-progress.py`,
`workflow-progress.json`, `workflow-progress.md`,
`workflow-progress.schema.json`, and `workflow-registration.schema.json`;
and this note. No root/policy/lock/configuration, peer, reference or existing
M0 mapping writes. No commits, pushes, agents or dependencies were created.

Final checks all used Cargo **+1.99.0**, locked/offline mode and
`/private/tmp/ferricstore2-wf-parity-agent`:

- Parity package: **38 passed** (22 unit, 6 retained CLI, 10 new progress CLI),
  none ignored. Saved `/private/tmp/workflow-parity-green-final.log`. The
  18 new tests include the saved pre-implementation red cycles above; tests
  that passed on their first run are supplemental checks, not claimed red.
- Full workspace tests: **116 unit/integration + 3 compile-fail doctests
  passed**, no failures/ignored. Saved
  `/private/tmp/workflow-parity-workspace-test.log`.
- Strict workspace Clippy, all targets, `-- -D warnings`: passed. Saved
  `/private/tmp/workflow-parity-workspace-clippy-final.log`. One ordinary
  `if_not_else` CLI style finding was fixed by positive branch ordering;
  no suppression or shared policy edit was needed.
- Warnings-denied workspace docs (`RUSTDOCFLAGS='-D warnings'`, `--no-deps`):
  all **15 crates** passed. Saved `/private/tmp/workflow-parity-workspace-doc.log`.
- Architecture package: **20 passed**, including actual workspace policy.
  Saved `/private/tmp/workflow-parity-architecture.log`.
- Owned formatting and `git diff --check`: passed. Baseline HEAD remains
  exactly pinned and its final `git status --porcelain=v1` is empty. Original
  dirty Elixir checkout was never written or reset.
- `check-reference`: exit **0**, all **480 files / 5858 textual IDs / 88
  static closure edges** intact. Saved `/private/tmp/workflow-parity-reference-final.log`.
- `check-progress`: exit **0** for structural/accounting validation, **5858
  frozen IDs, 9 source-derived declarations, 9 actual test mappings, 22 direct
  assertion links**. Registered case count is explicitly **unavailable** while
  collection is absent, with all **5858 unresolved**. Saved
  `/private/tmp/workflow-parity-progress-final.log`.
- `require-parity`: exit **1**, deliberately incomplete against the frozen
  all-unported M0 evidence. Saved `/private/tmp/workflow-parity-require-final.log`.

Coordinator's real ExUnit collection remains outside this write set and was
not present at this final inventory. The agreed registration schema and
`generate-progress.py --collection <relative-real-artifact>` hook are ready;
after the coordinator supplies collection, regenerate and run `check-progress`
to check its exact source/case sets and identity hashes. No synthetic collection
or execution artifact has been committed as evidence. This is a completed
progress-validation increment, with runtime reconciliation and full parity
explicitly incomplete.

## Explicit omissions

No source or Rust behavioral execution is certified by registration or ledger
validation. No full parity, storage, consensus, FIFO selection, worker, API,
policy, query/recovery or wider workflow capability is claimed by this slice.
Source assertion anchors and conservative Rust syntax checks cannot prove
semantic equivalence or arbitrary test control flow. Macro expansion, external
module resolution, actual source-case execution, mandatory-profile execution,
fixtures/setup completeness, baseline failures and durable adapter behavior
still require their own real evidence. Hashes verify consistency, not artifact
origin. Assertions inside opaque macro expansions are conservatively omitted.
