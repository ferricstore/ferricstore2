# Implementation plan and acceptance gates

Status: M0 project/test/lint/architecture infrastructure is implemented and its complete local gate passes; runtime ExUnit collection/scope reconciliation and workflow/runtime milestones remain planned. See [the handoff](CONTINUE.md) for results and next steps. This plan authorizes no later implementation by itself. Milestones depend on predecessor evidence, not calendar estimates.

The [existing Elixir workflow test parity contract](workflow-test-parity.md) is mandatory. Source tests and helpers are preserved in tests/reference with a manifest and hash inventory; all Rust counterparts are currently unported. New model/property tests supplement these regressions.

The [dual-transport contract](transport-design.md) adds HTTPS and a dedicated TLS/TCP API over one application authorization/admission service. Both are required for the usable transport milestone, with identical workflow semantics and transport-specific resource/fault tests.

## Before implementation

1. Finalize the workflow parity matrix and one-to-one scenario/assertion mapping against the preserved Elixir source tests, distinguishing baseline from dirty checkout. Record supported, deferred and deliberately dropped commands, explicitly including governance, approvals, budgets, durable effects and durable steps. Decide compatibility target: behavioral parity is proposed; wire/SDK and on-disk compatibility are separate decisions.
2. Validate the module allowlist and settle Raft-specific storage trait ownership without a dependency cycle. Pick the smallest crate set that still enforces domain isolation.
3. Confirm initial durability engine and Raft release together through a future focused spike. Require atomically persisted applied state, command outcome and indexes, crash-safe snapshots, durable log semantics, a proven strong-read barrier and license/security review. No production recommendation based only on README claims.
4. Freeze deterministic claim/FIFO/retry semantics, time behavior and parent/child group placement and atomicity restrictions. Specify behavior after clock jumps and leader changes, trust of leader versus client timestamps, and time advancement on accepted/rejected/duplicate/tick/membership entries. Never promise exactly-once external effects.
5. Decide inline payload limits and replicated blob durability; deferred blob storage must cause a clear request-size rejection, not a dangling reference.
6. Approve schema/version policy, routing key encoding, idempotency retention contract, API consistency modes and projection watermarks, including stable client positions versus local projection generations across restart/snapshot and explicit incompatibility after disaster restore.
7. Complete exact operation/field/error registries following the reviewed TCP framing/negotiation and HTTP endpoint contract; preserve original group/recovery request_scope binding, produce cross-transport intent/dedup vectors, and validate effective resource limits. Validate auth/deadline lifecycle, response reservations, bounded long-poll waiters, slow-client behavior and trusted leader forwarding.
8. Record decisions as ADRs. Add library versions and MSRV only after verifying the selected release. The infrastructure increment creates the Cargo workspace and CI; later runtime milestones still require their evidence gates.

## Ordered delivery

| Milestone | Work | Exit evidence |
|---|---|---|
| M0: policy scaffold | Cargo workspace including wire/http/tcp boundaries; pinned toolchain; rustfmt; Clippy; dependency/security/license policy; architecture checker; CI | Deliberately forbidden dependency, re-export, unsafe block and lint suppression fixtures fail the intended check; supported target/feature matrix is checked |
| M1: pure domain | Identity and bounds; create/claim/transition/retry/complete/fail/lease/cancel; policy and deterministic time rules | Model/property tests cover stale fences, duplicate requests, FIFO ties, retry exhaustion and deterministic replay; no I/O dependencies |
| M2: persistence contract | Transactional authoritative state, applied position, durable event history, idempotency and snapshot formats | Crash between every persistence boundary preserves no partial state; restart/replay matches reference model; corruption fails clearly |
| M3: consensus | Selected Raft adapter, log durability, read barrier, bootstrap/membership, leader routing, shutdown | Three-node fault tests: minority cannot acknowledge writes; failover preserves successful outcomes; old leaders/leases are rejected; snapshot installation and restart recover correctly |
| M4: minimal usable workflow | Typed application ports, shared wire contracts, HTTP + dedicated TCP adapters, common tenant admission/authorization, bounded claim worker and graceful drain | End-to-end create/claim/result with failover and lost response; repeated request returns original outcome; worker effects document idempotency; equivalent HTTP/TCP scenarios and cross-transport retained outcomes pass; malformed/slow clients stay within resource bounds |
| M5: workflow breadth | Durable signals/schedules, child groups/fanout, rewind and policy changes per parity matrix; implement governance/approvals/budgets/effects/steps only if the scope gate selects them | Race tests for cancellation vs completion, signal vs timeout, child completion vs parent reclaim; unsupported cross-group operations rejected |
| M6: queries and UI contracts | History/lineage/list read models, stable cursors, watermark/consistency controls, rebuild | Projection lag cannot change correctness; rebuild equals authoritative events; pagination/isolation and read-your-write tests |
| M7: retention and operations | Bounded cleanup, backups/restores, auth/configuration, metrics/tracing, quotas, rolling upgrades | Restore and upgrade rehearsals; GC cannot remove reachable blobs; idempotency expiry semantics and storage capacity limits documented |
| M8: performance and cache | Baseline measurements; optional PackedGen cache adapter after approval | Same semantic results with cache absent, cold, evicted and rebuilt; memory, tail latency and concurrency/safety evidence justify reuse |

M4 is a narrow vertical slice, not complete legacy parity. Declare missing operations until later milestones pass. Expand the MVP only where a real workflow requires a dependency such as child coordination. Before M1/M4 test ports, freeze legacy lease/fence retry compatibility separately from explicit request-ID replay, exact externally observed partition/schedule/error fields, and deep FIFO lane-head lookup guarantees.

## Existing test acceptance

M0 must collect runtime ExUnit cases (including generated cases and section files), preserve tags/setup, and map every included workflow case to a Rust owner and counterpart. Each later milestone ports the corresponding original scenarios and assertions. CI must fail for missing included-case mappings, unported cases in claimed parity scope, ignored counterparts or weakened assertions. Deferred scope and explicit unrelated/UI exclusions remain visible; full workflow parity cannot be claimed while included cases are incomplete. See [inventory](workflow-test-inventory.md) and [port contract](workflow-test-parity.md).

## Test design

- Domain: generated command sequences against a small reference model. Assert lease exclusivity, fence monotonicity, terminal consistency, FIFO eligibility, parent accounting, bounded history and idempotency.
- Replay: identical committed bytes and starting state produce identical events/state on each supported platform. Commands with invalid state preconditions deterministically reject.
- Storage: abrupt process termination, torn/truncated input, atomic transaction failure, WAL recovery, snapshot checksum/version mismatch and interrupted snapshot replacement.
- Cluster: leader loss before/after commit/application/reply; partition and healing; membership changes; lagging replica snapshot; long pause; uncertain client retry; old leader activity; disk-full handling.
- Time: explicit time inputs permit deterministic tests; adapters simulate forward/backward jumps. Replicated logical time and physical-time eligibility need separate assertions.
- Cache/projection: correctness with all caches disabled; lag/rebuild/out-of-order notification; strong read never served from a stale projection; tenant and query cursor boundaries.
- HTTP/TCP API/worker: shared semantic fixtures and legacy parity, fragmented/coalesced TCP frames, HTTP bodies/stream bounds, cross-transport dedup/conflicts, stale-scope retry after restore, tenant denial/auth expiry, schema errors, cancellation/proposal/waiter races, slow readers and byte/permit ownership, bounded backpressure, graceful drain and redacted secrets.
- Latency/admission: offered-load schedules for both transports together; tenant/group skew, active/waiter separation, payload/response expansion, background maintenance, disk stalls and failover. Measure rejection/availability and client-observed deadlines alongside p99; no unconditional success bound or coordinated-omission benchmark.
- Architecture: dependency graph checks and parsed/compiled module visibility checks, with negative fixtures. Regex search alone cannot prove no indirect calls or semantic purity.

Run Miri/loom only on the primitives and concurrent adapters they can meaningfully exercise. Do not replace end-to-end fault tests with unit tests that mirror code.

## Quality requirements

Every first-party crate must inherit workspace lint configuration; omissions fail policy checks. Run formatting, Clippy with warnings denied, compiler/documentation warnings checks, architecture tests and relevant tests. Include workspace default configurations and explicitly enumerated supported feature/target combinations; `--all-features` alone is insufficient when features are mutually exclusive. Check transitive dependencies and license/advisory policy with audited exceptions that have an owner and expiry.

Forbid domain imports of runtime/effect crates and domain clock/random/I/O calls through facade/re-export paths. Forbid first-party unsafe by compiler policy; apply standard Rust naming/API lints and agreed pedantic checks. Prohibit blanket `allow`/`expect` directives: narrow exceptions need a reason, scope, owner and review. Do not adopt every Clippy restriction lint indiscriminately; many conflict or reject normal Rust. Architectural functional purity and durability require additional tests and review, beyond linting.

## Open decisions to close

- Exact compatibility target for existing SDKs and workflow command names.
- Initial logical partition and Raft group count; no automatic resharding assumption.
- Storage backend and crash-consistency proof; whether large blobs are initially supported.
- Raft release, adapter trait ownership and cluster deployment baseline.
- Parent/child group atomicity and limits, FIFO scope and fairness.
- Read/pagination watermark contract and retention windows for outcomes/events.
- Exact operation/field/error registries and safe TCP/HTTP codec integration following ADR-001; selected TLS/dependency releases, cross-transport golden vectors and measured resource defaults.
- Dependency audits, MSRV and supported platforms; PackedGen production API/safety fitness.

These are planned design gates, not claims that the system has already been validated.
