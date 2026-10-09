# Existing Elixir workflow tests are the Rust acceptance baseline

The existing Elixir workflow behavior tests must pass against equivalent Rust operations before workflow parity is declared. Port their scenarios and assertions; do not replace them with a smaller newly invented suite. This is now a required acceptance contract, not an optional source reference.

## What is added now

- [File inventory](workflow-test-inventory.md): preserved source/support files, suggested Rust owners and separate uncommitted inputs.
- [Machine-readable manifest](../tests/reference/workflow-test-manifest.json): source commit, content hashes, navigational test declaration IDs, source locations and initial unported statuses.
- `tests/reference/elixir-baseline/`: byte-identical test/support snapshots from `ferricstore/ferricstore` commit `66586c596e271408d9184af32eb31cd7218c126e`.
- `tests/reference/elixir-working-tree/`: changed local workflow-related test inputs kept separate from the pinned baseline. Review their behavior before accepting a delta.

These files preserve the tests we already use; they are not runnable Rust tests or a self-contained Elixir application. The M0 Rust workspace and test infrastructure now exist; the workflow engine and scenario counterparts do not. Do not claim this suite works against Rust until the actual ports and harness run. The textual declaration index is navigation, not an ExUnit parser or executed-test count. Generated/parameterized tests, macro expansion, module/describe context, setup and tags require actual ExUnit collection in the future test-port phase.

## Non-negotiable port contract

Every in-scope workflow case receives a stable source identity and a Rust counterpart mapping with test path, fixture/scenario ID, assertion mapping, harness layer and execution status. The initial manifest records references only, all unported. Expand template declarations to runtime case identities when collecting ExUnit; retain the original source ID as lineage. No missing mapping, ignored test or unexplained assertion removal can count as passing parity.

Preserve command ordering, input values, returned fields, error conditions, ownership/fencing guards, result cardinality/order, atomic rollback boundaries and observable query/history behavior. Preserve negative tests and bugs/regression cases, not just the happy path. Random clocks/tokens/IDs can differ in representation only where the original assertion permits it. Normalize nondeterministic identities and elapsed time explicitly; never normalize away wrong fencing, ordering, missing data or transaction outcomes.

Some Elixir tests inspect ETS, LMDB, Bitcask, WARaft, process mailboxes or generated DSL internals. Port the workflow guarantee they establish to the selected Rust adapter/harness. Keep equivalent durability, rebuild, boundedness and failure assertions even if physical encodings or callback names differ. Exact legacy bytes are required only where public compatibility is selected; a storage representation change does not justify dropping the regression. Record both the original assertion and its semantic replacement. Tests of unrelated general KV behavior, excluded legacy dashboard UI and obsolete Elixir-specific syntax remain explicit reviewed scope entries; mixed suites are evaluated case by case, never excluded wholesale from a filename heuristic.

Governance, approvals, budgets, effects and durable steps remain inventoried scope gates. Deferring such features means their parity cases are deferred and full workflow parity is incomplete. Do not turn deferred cases into green/skipped CI or reinterpret this requirement as minimal create/claim/complete parity. Behavior changes to included workflow operations need a documented compatibility decision; intentional exclusions must follow the user's workflow-only scope.

## Harness design for the implementation phase

| Existing suite / support | Rust port and preserved behavior |
|---|---|
| `flow/workflow_sdk_test.exs`, `flow_workflow_demo_test.exs` | SDK/worker contract tests: workflow type/defaults, partition encoding, guards, policy installation, handler outcomes and uncertain storage failures |
| `flow_test.exs` plus every required section | Workflow conformance scenarios: lifecycle, ordering, claims/reclaim, retry, batches, child groups, history and retention |
| `flow/invariant_test.exs`, `flow/fencing_retry_test.exs`, `flow/fifo_lane_test.exs` | Domain/application invariants: exclusivity, stale ownership, lane heads, attempts/backoff and deterministic transitions |
| `flow_signal_test.exs`, schedule suites, policy suites | Durable signal/schedule/policy conformance, cancellation/time races and recovery |
| Payload/value/history/lineage/query/retention suites | Authoritative storage/projection tests: absent vs omitted values, bounds, history filters/order, stale projection fallback, rebuild and safe cleanup |
| `flow_crash_matrix_test.exs`, `flow_production_recovery_test.exs`, Raft/WARaft sections | Fault/recovery harness: same injected boundaries and persistence guarantees, translated to Rust storage/consensus |
| Cluster Flow tests and `support/cluster_*` | Real multi-node integration tests with partitions, leader loss, restart and recovered ownership; unit mocks cannot replace these |
| Command/HTTP/native Flow tests | Application/API behavioral contracts; legacy parser/framing specifics require an explicit transport compatibility decision |
| `support/flow_case.ex`, shard/raft helpers, Eventually | Isolated test stores/groups, deterministic injected time, reset/cleanup, bounded eventual assertions and controllable projection/fault barriers |

Legacy lease/fence retries and explicit request-ID retries are separate test surfaces. `fencing_retry_test.exs` repeats calls without request IDs, permits specific duplicate effects with one history event, and rejects an old retry after an intervening transition. The Rust harness must not manufacture a reused request ID to bypass those assertions. Preserve this behavior through the domain/compatibility API: matching duplicate effect under the legacy current-history/guard rules is a no-op; a different effect or stale/intervened operation rejects. Explicit stable request-ID replay remains additional coverage, with its distinct original-outcome guarantee. The compatibility mapping must be finalized before these cases can be marked ported.

Preserve observable partition encodings (including SDK `fpk:` vectors), schedule target IDs/deadlines/counts, public errors, and hidden internal fields through the compatibility contract. Keep exact equality, set equality and absent-versus-zero distinctions as asserted. Physical backend representation may differ; externally observed values do not become arbitrary normalization. Retain the deep FIFO lane-head cases with 80 earlier-due followers: bounded work or partial replies cannot weaken their requirement to locate/select the eligible head in that call. The Rust design should index authoritative lane heads, rather than depend on scanning followers until a budget happens to permit success.

Fault ports map durability states and failure targets, not callback names. Require an acknowledged barrier for precommit, committed-before-reply, persisted-before-projection/checkpoint and projection replay as applicable; an operation finishing before the targeted fault is injected fails the test. Preserve each source test's actual allowed outcomes, not stronger claims inferred from its title.

Build a backend-neutral scenario runner only when implementation starts. Scenarios express normalized typed commands, explicit time advances, injected failure/leader/replica actions, observations and assertions. Run equivalent fixtures against the pinned Elixir reference and Rust implementation where feasible. Use source-derived expected invariants as well as differential comparison: two engines sharing a bug is not proof of correctness. Elixir-only test helpers become Rust harness utilities; do not introduce general KV features just to reuse a setup helper.

Use deterministic clocks/fault hooks rather than replacing a timeout test with an unconstrained sleep. Translate eventual assertions into bounded convergence checks that preserve ordering and durable checkpoints. Fresh stores/cluster identities and unique tenant/workflow IDs prevent shared-state interference; explicitly serialize cases whose real global resource needs it. Preserve tags/environment prerequisites and ensure mandatory parity profiles run, rather than silently omitting cluster/crash/cold-projection suites.

## Parity gate and statuses

The completed Rust suite must track `unported`, `ported-not-run`, `passing`, `failing`, `deferred-scope`, and `excluded-with-reason`. Manifest categories/targets are preliminary routing suggestions; they do not approve exclusions. Missing mappings and unclassified runtime cases fail the parity gate. Included behavior requires passing counterparts and preserved assertions; ignored tests remain incomplete. A reduced milestone may report its own passing subset, but may not claim full workflow parity.

M0 collects runtime ExUnit identities and creates the checked bidirectional mapping: every required collected case maps to source lineage, and every preserved declaration is reconciled to runtime cases, a helper/template, or a reviewed non-case. Record module/describe/parameter identity, effective tags and fixture/setup dependencies; zero-case runs, missing prerequisites, setup failures and filtered mandatory profiles cannot pass. M0 also freezes baseline failure reporting rather than recording a failing reference as a new expected success.

M0 collects runtime ExUnit identities; M1 ports domain scenarios; M2/M3 port persistence/consensus faults; M4 ports worker/API contracts; M5 closes broader workflow scope; M6/M7 port query/retention/recovery cases. Property/model tests, architecture checks and Rust-specific safety tests are additional coverage, not replacements for this suite.

No Elixir or Rust suite was executed for this design change. Snapshot integrity and manifest coverage checks verify preservation only, not test outcomes or behavioral equivalence. [Independent harness review](reviews/workflow-test-parity-review.md) records the bounded source inspection and remaining requirements.
