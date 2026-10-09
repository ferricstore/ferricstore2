# Workflow semantics and Rust parity analysis

Design evidence inspected on 2026-10-09. No implementation or commit is included.

## Evidence boundary

The authoritative server baseline is `ferricstore/ferricstore` commit `66586c596e271408d9184af32eb31cd7218c126e`. All server evidence below was read with `git show` at that revision. The local source tree is dirty, including Flow attributes, query execution, terminal handling, storage, and protocol files; those changes are not baseline behavior. The separate `/Users/yoavgea/repos/ferricstore2` checkout is unrelated and was not modified.

Links below point to immutable GitHub source revisions. SDK evidence is separately pinned to Python commit `368bef8a7220716d4154f30d94c1cbdb743843cd`; it is supplementary client evidence, not proof of server behavior. The Python checkout also has dirty changes. The durable-context excerpt was checked against `git show HEAD` before using it.

## Product scope

Build a durable workflow service whose operations explicitly mutate workflow records. The Elixir SDK describes a Temporal-like call site without Temporal-style replay; user code and external writes remain outside Flow transactions. A Rust rewrite should preserve that distinction. Source: [workflow SDK](https://github.com/ferricstore/ferricstore/blob/66586c596e271408d9184af32eb31cd7218c126e/apps/ferricstore/lib/ferricstore/flow/workflow.ex).

Workflow-only does not mean merely create/claim/complete. The baseline includes retry policy, durable execution modes, scheduling, lineage, child groups, cancellation, rewind, retention, history, payload references, blocking claims, and operational queries. Broader Flow governance, budgets, approvals, effects, and durable SDK steps need an explicit scope decision rather than silent omission. General Redis collections and unrelated compatibility commands are outside the proposed product; any workflow dependency on those internals should become a private typed storage operation.

## Parity matrix

| Surface | Established behavior | Rust design implication |
| --- | --- | --- |
| Workflow declaration | Type is fixed; initial state and per-state defaults are declared; installed policy replaces by default and supports explicit patching | Versioned typed definitions; API validation cannot change declared type |
| Partitioning | Length-prefixed component encoding prevents delimiter collisions; scalar values only | Specify exact byte encoding and float/string normalization before cross-language compatibility |
| Bulk creation | SDK passes per-item partition keys; core groups work by shard | Preserve caller order and per-item results; no global transaction claim |
| Claims | Atomically lease due records and remove due membership; claimed record has `state=running` and distinct `run_state` | Replicated ownership transition; handler dispatch uses `run_state` |
| Priority and scope | Higher priorities drain first by default; a priority option targets a band; multi-state selection has earliest-due coverage | Explicit total ordering and fairness specification, not map iteration order |
| Guards | SDK carries partition, lease, and fencing fields; core checks expected state and running lease | Validated claim capability; revalidate against authoritative state at apply |
| Retry | Clears lease, restores claimed run state, increments attempts, computes backoff; exhaustion may target an active or terminal state | Pure retry decision with overflow bounds and replicated time/random inputs |
| Terminal mutations | Complete/fail/cancel centralize terminal hooks; ordinary transition cannot bypass them | One domain terminal transition path updates lineage, due/inflight membership, and audit events |
| Child groups | `wait:any` succeeds on first successful local child and fails when all local children fail; policies cancel or abandon siblings | Parent resolution and local child mutations form one group-local atomic decision |
| History and values | History queries support ranges/reverse/filters; claim hydration is optional and capped; omitted values retain metadata | Separate metadata selection from payload reads; distinguish absent, omitted, and unavailable values |
| Worker | Optional poll loop dispatches handler outcome into explicit commands | External handlers are outside the Raft apply path; bound concurrency and shutdown |
| Error handling | Handler failure may become retry; a storage mutation exception must not generate a second command | Separate user-code errors, rejected commands, and unknown commit outcomes |

SDK evidence: [workflow contract tests](https://github.com/ferricstore/ferricstore/blob/66586c596e271408d9184af32eb31cd7218c126e/apps/ferricstore/test/ferricstore/flow/workflow_sdk_test.exs), including partition collisions, fixed type, guard forwarding, policy replacement, and storage-failure handling. A claimed SDK object is a convenience view, not durable truth: [Job](https://github.com/ferricstore/ferricstore/blob/66586c596e271408d9184af32eb31cd7218c126e/apps/ferricstore/lib/ferricstore/flow/job.ex).

Core evidence: [claim, child-group, cross-shard create, and lifecycle tests](https://github.com/ferricstore/ferricstore/blob/66586c596e271408d9184af32eb31cd7218c126e/apps/ferricstore/test/ferricstore/flow_test/sections/flow_spawn_children_wait_any_resolves_on_first_successful_child_across_s.exs); [terminal batch and transition guards](https://github.com/ferricstore/ferricstore/blob/66586c596e271408d9184af32eb31cd7218c126e/apps/ferricstore/test/ferricstore/flow_test/sections/flow_fail_many_atomically_fails_one_partition_batch.exs); [priority, cold scheduling, and retry tests](https://github.com/ferricstore/ferricstore/blob/66586c596e271408d9184af32eb31cd7218c126e/apps/ferricstore/test/ferricstore/flow_test/sections/flow_claim_due_drains_higher_priorities_before_lower_priorities_default.exs).

## Correctness invariants

1. Authoritative workflow state and ownership changes enter through committed commands. Selection hints never authorize a claim by themselves. Empty-index optimizations must not hide newly due or hibernated work.
2. Expected state, lease identity, and fencing generation are checked during apply. A stale handler cannot mutate a replacement claim. SDK option forwarding alone is not enforcement; current Elixir Job helpers use defaults that callers can override.
3. `running` and `run_state` are distinct. Retry and handler policy use the logical run state; terminal records are not due work.
4. Atomic batches validate all members before publishing any member's state/index/history changes within their transaction group. Cross-group bulk calls may succeed in one group while another rolls back. Independent batch mode is a separate observable contract.
5. Parent resolution is idempotent and terminal hooks cannot be skipped. Same-group evidence does not establish atomic cross-group fanout; reject unsupported topology until a durable coordination protocol is specified.
6. Reapplying committed commands and installing snapshots must produce the same records, ownership generations, and indexes. Domain decisions cannot read wall clocks, random generators, or external services; values must be chosen and carried in replicated command inputs.
7. Cold projections may lag. Correctness reads and claims must reconcile authoritative changes; the baseline includes counts of pending terminal records before projection flush. UI responses should identify projection freshness rather than imply one instantaneous global snapshot.
8. Payload eviction cannot erase ownership, due membership, or unreconciled lineage. Missing payload bytes are an explicit read error or omission, never a fabricated empty payload.
9. External effects are not exactly-once merely because ownership is fenced. A handler can perform an effect and lose its completion response; effect receivers need idempotency or reconciliation.

The Python [durable context](https://github.com/ferricstore/ferricstore-python/blob/368bef8a7220716d4154f30d94c1cbdb743843cd/src/ferricstore/workflow_durable_context.py) refreshes claim state after advance/step, and records an uncertain step to block further continuation. Preserve this client-visible safety property when specifying Rust/API outcome types; do not automatically retry with a different mutation after an ambiguous response.

## Review of initial crate proposal

Keep `types` small: identifiers, validated primitive values, command/result envelopes, schema versions. Put pure transition decisions and invariants in `domain`; put IO traits in `ports`; put authorization, routing, admission, and command preparation in `application`. `consensus` owns Raft integration; `storage` owns persistence adapters; `projection` owns rebuildable query representations; `api` owns wire adaptation; `worker` owns polling and handler dispatch; `node` composes adapters and lifecycle.

The decisive boundary is execution: application proposes commands, while consensus invokes deterministic domain apply over authoritative state. Application must not run the mutation first and then replicate its result without a defined conflict protocol. Storage and projection must not independently duplicate business rules. Keep library-specific Raft and cache types out of domain and public DTOs. Dependency arrows need a concrete allowlist from the rust-quality review; traits alone do not enforce crate boundaries.

For the UI, use pure transformations from typed read models into view state, with effects at the API/client boundary. Distinguish selected logical state from running lease state, show projected freshness, and handle command rejection or unknown outcome explicitly. Backend domain reuse must not require the frontend to link storage or consensus crates. Whether functional UI means Rust/WASM or functional TypeScript remains a product choice.

## Library constraints relevant to workflow

[OpenRaft 0.9.25 documentation](https://docs.rs/openraft/0.9.25/openraft/) exposes network, log, and state-machine integration surfaces and explicitly describes its pre-1.0 API as unstable. Treat it as a candidate behind consensus adapters; adopting it does not resolve workflow transaction boundaries or storage durability.

[Moka 0.12.16 documentation](https://docs.rs/moka/0.12.16/moka/) provides concurrent synchronous/asynchronous caches, weighted capacity, and expiry, with best-effort bounds. It is a candidate for payload/read acceleration. Workflow leases and scheduling must use authoritative replicated state, not cache expiry. Strict admission needs separate accounting; weighted eviction is not a hard process-memory ceiling. Storage/cache selection belongs to the raft-storage peer's assessment.

## Unknowns and acceptance gates

- Enumerate governance/effect/approval/budget commands before declaring full workflow parity. The inspected basic SDK is not the full product surface.
- Specify FIFO lane membership, priority starvation policy, tie-breaking, and whether ordering is local to partition/state/group. Existing named tests establish examples, not a complete ordering proof.
- Specify group placement, migration, policy distribution, and cross-group fanout behavior. Start with explicit same-group constraints rather than assume distributed atomicity.
- Pin lease expiration boundary, allowed clock skew, timestamp validation, jitter source, fencing overflow, and snapshot/restart behavior.
- Define mutation request IDs, deduplication retention, unknown-outcome reconciliation, durable-step/effect identities, and compatibility errors.
- Define authoritative versus projected query consistency, pagination cursors, retention interaction, deleted-value behavior, and query scope authorization.
- Audit rewind, max-active timeout, retention, history value queries, blocking waiter cancellation, and full protocol/SDK golden tests before promising parity. These surfaces were identified but not exhaustively inspected in this note.
- No binary migration or on-disk compatibility is established. Plan export/import and schema evolution separately from operation compatibility.

Acceptance should combine source-derived golden command fixtures with property tests for stale fencing, replay determinism, batch rollback, terminal hooks, and child resolution. Cluster fault tests must exercise leader loss after commit/before response, delayed projection, restart, snapshots, and stale worker completion. UI acceptance should cover unknown outcomes, omitted payloads, and stale projections.

## Peer exchange record

Peers provided by the coordinator: workflow/Sartre `01a12020-abe9-7c80-81e0-b7ddbba219c1`, raft-storage/Noether `01a12020-ac4d-7f21-bd6c-cf0e94884dfe`, rust-quality/Darwin `01a12020-acb1-7b30-b118-747b9908ed6c`.

Initial direct exchange status: no outbound or inbound direct peer messages occurred. Tool inventory was searched for `send_input`, `spawn_agent`, and `wait_agent`; none was exposed. The parent subsequently authorized shared-file communication and relay. An actual first file-based review and replies to Darwin's prepared questions are recorded in [workflow review](../reviews/workflow-review.md). Noether's note was absent at that inspection. Mutual review remains pending; no peer endorsement is claimed.

Questions prepared for Noether: confirm group-local atomicity and cross-group rejection/coordination; define authoritative state/snapshot/projection checkpoints; keep cache eviction outside lease and due truth. Questions prepared for Darwin: enforce dependency allowlist and absence of framework types in domain; standard lints plus architecture checks; isolate any unavoidable unsafe dependencies. Coordinator should deliver this note through the available peer mechanism and record actual replies and resulting revisions before marking mutual review complete.

Reviewer replacement reported by parent: Noether produced no files and was stopped; no source findings from that agent are available. Parfit `01a1202a-a6c0-7a50-851c-f250f0c2aaba` now owns the bounded Raft architecture/peer-question review. Historical questions transfer to Parfit; final review must attribute actual files to Parfit, not Noether.

Final communication update: Parfit's Raft note/review were received and actually read. [Workflow review](../reviews/workflow-review.md) records acceptance of the five authored Raft answers and two bounded time/watermark clarifications. Workflow-side file-based cross-review with Darwin and Parfit is complete; peer receipt of this final response and implementation validation are not claimed.

Final provenance clarification: the coordinator's targeted baseline read in [source provenance](../source-provenance.md) identifies WARaft as the production backend and leader HLC/wall command stamping. Earlier SDK Ra-shaped prose describes a call path and must not be read as backend selection evidence. OpenRaft remains a migration candidate without legacy log/wire/on-disk compatibility. These targeted findings belong to the coordinator, not Parfit or this final workflow round.

## Current final status

Workflow analysis and actual file-based/parent-relayed cross-review with Darwin and Parfit are complete; see the [completed workflow review and final responses](../reviews/workflow-review.md). The parent reports reading all final cross-reviews and integrating time, identity and watermark wording into architecture, implementation plan and decisions. Earlier pending statements above retain first-round history and do not describe the current workflow-review status. No direct agent messaging, implementation, commit or conformance validation is claimed. Future validation gates remain part of the design.
