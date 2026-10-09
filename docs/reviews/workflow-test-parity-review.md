# Workflow test parity: bounded harness review

Status: design review only, 2026-10-09. No engine, tests or CI implemented; no suite executed. This review independently inspected five suites with `git show` from `/Users/yoavgea/repos/ferricstore` at `66586c596e271408d9184af32eb31cd7218c126e`, and read the new [parity contract](../workflow-test-parity.md), inventory introduction and manifest header/sample records. It is not an exhaustive assertion audit, snapshot hash validation, coverage certification or runtime collection. Parent owns those preservation checks and plan changes. Portions of the large invariant suite were read through bounded excerpts; findings below cite inspected cases rather than claiming every case was reviewed.

## Assessment and bounded fixes

The parity document correctly requires source-derived assertions, case-level dispositions, separate working-tree deltas, real crash/cluster profiles and explicit incomplete statuses. Its inventory is a useful preservation starting point, not proof that all workflow behavior has been selected or ported. The following clarifications should be added before implementation; they do not require broadening the current snapshot task.

1. Freeze the retry compatibility decision before porting `fencing_retry_test.exs`. Its calls contain no request IDs. A runner must not silently assign the same Rust request ID to repeated calls to make them pass. Legacy lease/fence retry semantics and new request-ID replay semantics are separate contracts, including rejection after an intervening transition. Record whether a compatibility adapter preserves the former or whether an explicit behavior change remains unresolved.
2. Treat "same injected boundaries" as matched durability states and failure targets, not identical WARaft/LMDB callback names. Specify precommit, committed-before-reply, persisted-before-projection/checkpoint and projection-replay barriers for the chosen Rust adapters. Every required fault must prove it was reached; operation completion before the barrier is a failure.
3. Make the inventory-to-runtime reconciliation bidirectional: every collected in-scope runtime case maps to a declaration/source lineage, and every preserved declaration is classified as runtime case(s), helper/template, or reviewed non-case. Reconcile all required profiles, setup failures and filtered cases. No silent skip, unavailable prerequisite or zero-test run may contribute a passing result.
4. Resolve exact public partition encoding, legacy errors, schedule target IDs and internal-field visibility separately from physical storage representation. The SDK asserts concrete `fpk:` strings. Preserve those bytes when compatibility includes them; changing them needs a stated decision and mapped assertion, not generic nondeterminism normalization.
5. Keep architecture/source-text checks separately classified. Where tests inspect original `lib`, native code or guides, preserve the boundary/safety invariant in Rust architecture checks and document why the original textual assertion cannot run. A source snapshot need not become a complete clone of the Elixir application.

## Source evidence and assertions to retain

All source paths below are relative to the pinned Elixir repository. Links point to the preserved baseline for navigation; the observations were obtained from the pinned git objects.

### SDK and worker contract

Source: [apps/ferricstore/test/ferricstore/flow/workflow_sdk_test.exs](../../tests/reference/elixir-baseline/apps/ferricstore/test/ferricstore/flow/workflow_sdk_test.exs).

This suite uses a fake `Store` and mailbox observations. It proves generated SDK forwarding/defaults and worker behavior, not durable engine semantics. Rust needs a recording client port and typed workflow definition/job fixtures, plus a controllable worker task. Retain:

- `create derives type, state and partition key from attrs`: exact `fpk:8:tenant-a9:invoice-1`, payload/correlation forwarding, declared type and initial state. `derived partition keys cannot collide through component delimiters` distinguishes `a:b,c` from `a,b:c`.
- `workflow commands cannot override the declared flow type`: attempted `type: "forged"` cannot affect create, create-many or child specs. `partition builders reject non-scalar values without raising`: all three builders reject invalid input through the error contract.
- `fanout wraps flow_spawn_children with parent guard defaults and aliases`: preserve partition, from-state, lease and fence **zero**, group ID, wait-state and success/failure aliases. Absence and zero cannot collapse during normalization.
- `claim_due applies state defaults and wraps returned records as jobs`: preserve 60,000 ms lease, payload inclusion, 32,000-byte bound, worker, limit and workflow association. Policy installation distinguishes replacement from explicit patch and FIFO from parallel mode.
- `ok follows on_ok transition and carries lease/fencing guards`, `error applies state retry policy and exhausted target`, terminal completion, and run-once/worker cases: assert which command is issued and its arguments, not merely a successful return.
- `handle does not convert storage mutation failures into a second command`: a transition storage failure propagates; **no retry command** follows. Keep the recording client's negative call assertion. `reclaim_once wraps reclaimed records as jobs` forwards `reclaim_expired: false`.

Elixir macros/process dictionaries/GenServer syntax may change. Command recording, option precedence, guards and the absence of an extra mutation must remain observable.

### Claim/FIFO invariants

Source: [apps/ferricstore/test/ferricstore/flow/invariant_test.exs](../../tests/reference/elixir-baseline/apps/ferricstore/test/ferricstore/flow/invariant_test.exs).

Use explicit command time, isolated durable stores and ordered operation fixtures. Representative inspected cases require:

- `states are parallel by default until policy opts a state into fifo`: both flows are returned; preserve set equality where source uses it rather than inventing output order.
- `fifo state claims one flow per partition lane in state-entry order`: `z-...first` precedes `a-...second`, the next claim is empty while held, and completion releases the follower. `fifo retry keeps the same partition lane blocked until the retried flow leaves` preserves the head through a future retry deadline.
- `fifo any-partition claims at most one flow per partition lane` and `fifo partition list claims at most one flow per listed partition lane`: cardinality, lane isolation and excluded-partition behavior survive aggregate routing.
- `fifo pipeline claim batch does not overclaim a single partition lane`: two ordered claims yield first-flow then empty. A batch adapter cannot evaluate both from the same stale view.
- `fifo reclaim picks the expired lane head instead of the queued follower` and the limit-one variant: reclaim the head with the new owner and keep the follower blocked.
- `fifo keeps a cold lane head ahead of a hot due follower`: the test deletes a hot index member directly; port this to a controlled cache/projection absence or authoritative rebuild fixture that still retains durable head truth. Claim remains empty while the head is not due. Do not make index corruption normal runtime behavior.
- `fifo claims a due lane head beyond the bounded follower scan window`: retain the 80 earlier-due followers and claim the actual head. `fifo preserves global due order when a deep lane head is injected` still chooses the earlier eligible head across states. Bounded work cannot become an empty-queue success that loses this guarantee.
- `fifo times out an expired lane head injected beyond the bounded scan window`: first claim is empty, head becomes failed with `reason: "max_active_ms"`, next claim selects the first follower.
- `fifo state-entry order survives restart when ids and timestamps would sort differently`: same timestamps, reverse lexical IDs and reopening the same data directory must still select the first entry.
- `fifo state rejects priority and implicit partition entry` and `fifo state rejects direct running entry`: keep all rejection paths, including transition/start-and-claim/step-continue. `public record returns hide fifo state entry sequence` and `pipeline get hides fifo state entry sequence` retain internal-field hiding across write and read responses.
- `future due work is invisible until its due timestamp`: empty at 9,999, claimed at 10,000. Terminal work is not reclaimed. Completion keeps terminal history queryable.

The value-ref/mget case is an explicit scope decision for the proposed bounded-inline-payload design; it cannot disappear because it shares a file with FIFO tests. Internal reserved group identifiers likewise require a recorded representation/scope decision.

### Lease/fence retry behavior

Source: [apps/ferricstore/test/ferricstore/flow/fencing_retry_test.exs](../../tests/reference/elixir-baseline/apps/ferricstore/test/ferricstore/flow/fencing_retry_test.exs).

`complete retry after committed response loss is a no-op success`, its fail and transition counterparts, and `retry retry without explicit run_at is a no-op while scheduled in original state` each repeat a call with changed `now_ms` and assert exactly one corresponding history event. These tests model a lost response by repeating commands; they do **not** inject network response loss.

`retry retry after committed response loss is guarded by latest history event` first permits duplicate retry with one event, then transitions queued to waiting and requires the old retry to return `ERR stale flow lease`. `same fencing token does not turn a different terminal effect into success` fails then rejects complete with that same token. Preserve operation identity and these negative outcomes. New request-ID replay tests are additional coverage; they cannot replace or reinterpret these baseline calls without a compatibility decision.

### Crash recovery

Source: [apps/ferricstore/test/ferricstore/flow_crash_matrix_test.exs](../../tests/reference/elixir-baseline/apps/ferricstore/test/ferricstore/flow_crash_matrix_test.exs).

The harness isolates persistent data, serializes global fault state, lowers blob spill threshold to 64 bytes, blocks the first matching hook and kills the target process. `await_fault` times out with failure; `await_fault_or_op` also fails if the operation finishes before injection. Rust needs barrier acknowledgement, target-specific kill/restart, same-store reopen, bounded readiness and unconditional cleanup of hooks/tasks/settings.

- `unknown outcome after WARaft commit recovers committed Flow state and hot due index`: recovered workflow must be claimable with the exact payload. Recovery cannot infer absence solely from the missing reply.
- `crash after WARaft apply projection write rebuilds visible Flow state`: the source permits either an existing queued record with exact payload or absence followed by successful recreate; then claim/payload must succeed. Do not claim this case proves mandatory persistence at that hook.
- `crash during blob side-channel write leaves command retryable and blobs collectible`: retry succeeds, payload survives and garbage sweep succeeds. It does not assert an exact reclaimed-blob count. Inline-only Rust storage requires an explicit disposition for this removed side-channel boundary plus a payload durability replacement; blob parity remains deferred if blob support is deferred.
- `LMDB writer crash during flush replays terminal projection from durable truth`: restarted projector flush succeeds and consistent cold terminal query includes the workflow.
- `history projector crash after fsync replays history and preserves values`: source requests `values: true` and checks created/completed event presence after restart. Despite the title, its final predicate does not directly compare hydrated result bytes. Keep its actual assertions; stronger value checks should be identified as additional tests.
- `blob GC crash after live-ref scan does not delete live Flow blobs`: preserve positive live-ref metadata, unchanged authoritative storage position despite altered metrics, and exact completed result bytes after another sweep. Metrics cannot substitute for the authoritative GC safety boundary.

A reducer simulation can supplement these cases; it cannot demonstrate process termination, durability or projection recovery. Map fault states to concrete storage/consensus adapter barriers before declaring them ported.

### Cluster schedule failover

Source: [apps/ferricstore/test/ferricstore/cluster/flow_schedule_cluster_test.exs](../../tests/reference/elixir-baseline/apps/ferricstore/test/ferricstore/cluster/flow_schedule_cluster_test.exs).

Both cases start three real nodes with one shard, kill the leader, wait for election/readiness and concurrently dispatch from survivors. The source raises if OTP peer support is unavailable; absence is not a passing skip. Rust needs independently killable replicas, known leader targeting, bounded election waits and coordinated dispatch calls against both survivors.

`due schedule can fire once after Raft leader failover`: every dispatch returns no errors; aggregate fired and claimed are each exactly one; target type/payload survive; subsequent dispatch has zero fired/claimed.

`overdue interval catch-up fires once after Raft leader failover`: interval is 5 ms and recovery is ten intervals late. Aggregate fired/claimed are one and coalesced is ten. Target ID is `<prefix>:<due_at_ms>:1`; schedule fire count is one, coalesced count ten and next deadline is recovery plus 5 ms. Dispatch again at the same recovery time produces all zero counts and no errors. Preserve those cardinalities and deadlines. These assertions concern workflow materialization, not exactly-once arbitrary external handler effects.

## Inventory and acceptance limits

The parent reports 480 preserved source/support/fixture files, 5,858 textual declarations and nine separately captured dirty deltas after adding `test/fixtures/flow_query_result_v1.json`; this review does not independently certify those totals or hashes. The golden fixture closure and existing LMDB vectors improve preservation but do not make snapshots a runnable Elixir application. Static require-file closure does not establish dynamic dependencies, complete fixture discovery or full workflow case coverage.

Text matching may omit multiline/generated/macro declarations, count templates rather than instances, or lose module/describe context, inherited tags, setup effects and exclusions. Names and Flow markers can include unrelated cases and miss indirect workflow assertions. Future collection must use the pinned application's actual ExUnit registration and selected profiles, record module/describe/runtime name, source location, parameter identity and effective tags, and retain source lineage when one declaration expands into many cases. Record baseline failures as failures rather than normalizing the expected answer to whatever runs today.

Each mapped case should identify its original assertions, semantic replacements, required fixture/support dependencies, layer, compatibility decision, profile and execution evidence. Every assertion needs a disposition even when several source cases share a Rust scenario. File-level categories—including `excluded-ui-reference`—are routing suggestions, not approved case exclusions. Mixed source-text tests must separate workflow invariants from obsolete textual/mechanism assertions. Dirty deltas stay separate until accepted; they must not silently modify pinned expectations.

For future acceptance, missing runtime mappings, missing required fixtures, ignored tests, filtered required profiles, setup failures, unhit fault hooks and unported/deferred included behavior all keep parity incomplete. Rust-specific model, safety and architecture tests add evidence; they do not erase source regressions. This review supports the design contract subject to the bounded fixes above and makes no passing-parity claim.

## Integration disposition

Final bounded integration check, 2026-10-09: read the updated parity contract, architecture, implementation plan and decisions record; no additional source research or test execution. The review's requested design clarifications are addressed. The parity contract separates no-request-ID lease/fence retry behavior from retained request-ID replay and prohibits artificial reused IDs; preserves exact public encodings/errors, schedule fields, hidden fields and absent-versus-zero distinctions; retains same-call eligible-head selection through the 80-follower FIFO cases; requires acknowledged durability-state/failure-target barriers and actual source-allowed outcomes; and reconciles runtime/source identities bidirectionally with prerequisite, setup, filtering and zero-case failures explicit. Its existing case-level semantic replacement rules address implementation-specific/source-text assertions without requiring an application clone.

Architecture now states the separate legacy retry compatibility path and authoritative deep-head requirement. The implementation plan freezes those contracts before test ports and makes the linked parity contract mandatory. The decisions record accurately attributes this bounded review and keeps preservation evidence separate from behavioral results. No further blocking design correction is identified within this review's scope. The five bounded recommendations are closed at the design-document level; exact compatibility mappings, runtime collection, adapter barrier realization and passing Rust counterparts remain future implementation acceptance gates. Parent-reported integrity checks are acknowledged, not independently rerun or certified here. Review complete; no implementation or commit performed.
