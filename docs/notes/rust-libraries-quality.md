# Rust libraries, cache reuse, and enforceable quality

Design assessment, 2026-10-09. No implementation or dependency installation is authorized by this note.

## Evidence and provenance

FerricStore baseline: `66586c596e271408d9184af32eb31cd7218c126e` in `/Users/yoavgea/repos/ferricstore`. Read baseline files with `git show <sha>:<path>`; the checkout contains extensive uncommitted changes and must not silently become the migration specification. Baseline `mix.exs` declares Credo and its HTTP quality alias requires formatting, warnings-as-errors compilation, strict Credo, and tests. Baseline architecture tests exist at `apps/ferricstore/test/ferricstore/arch_test.exs`, `v2_architecture_test.exs`, `fetch_or_compute_worker_architecture_test.exs`, and corresponding HTTP/server architecture test paths. Their existence motivates executable boundaries; it does not establish that any Rust tool is behaviorally equivalent.

PackedGen baseline HEAD: `26d1d933619951de93f432fe8e19f1f3f5637abd`, checkout `/Users/yoavgea/repos/packedgen-rs`. Findings below describe the **working tree**: `Cargo.toml`, `src/cache.rs`, `src/cache_arena.rs`, README, dependency policy and proof script are modified; `Cargo.lock` and allocation tests are untracked. Pin a reviewed commit and preserve its lock resolution before adoption. No tests, benchmarks or security audits were executed for this design assessment.

## Recommended library shortlist

| Concern | Candidate and decision | Boundary / required evidence |
| --- | --- | --- |
| Async runtime | [Tokio](https://docs.rs/tokio/latest/tokio/), preferred | Adapter/application scheduling only. Bound channels, CPU work and blocking storage admission; `spawn_blocking` alone is not backpressure. |
| HTTP API and UI endpoints | [Axum](https://docs.rs/axum/latest/axum/), preferred | API adapter converts requests into application commands and queries. UI reducers remain pure functions over serializable views; runtime/network types never enter domain. |
| Consensus | [OpenRaft storage interfaces](https://docs.rs/openraft/latest/openraft/storage/), preferred for evaluation | Consensus owns OpenRaft types. Log persistence and state-machine persistence are separate contracts; library adoption does not supply a durable backend or workflow semantics. Confirm selected version's storage API/features and run its storage conformance suite. |
| Durable embedded state | [redb](https://docs.rs/redb/latest/redb/), evaluation candidate | Transactional storage behind ports. Validate atomic workflow/index/applied-position updates, crash recovery, snapshot consistency and fsync behavior before selection. Safe API or Rust implementation does not imply absence of internal unsafe. |
| General bounded cache | [Moka](https://docs.rs/moka/latest/moka/sync/struct.Cache.html), baseline comparator | Weighted capacity is best-effort; separate process memory admission is required. Never use cache expiry as authoritative workflow expiry. |
| Simpler concurrent cache comparator | [quick_cache](https://docs.rs/quick_cache/latest/quick_cache/), benchmark candidate | Compare weight accounting, concurrency, eviction and maintenance under representative workloads. No assumed RSS guarantee. |
| Workflow state properties | [proptest](https://docs.rs/proptest/latest/proptest/) | Generate command sequences and compare against an independent reference model. |
| Concurrency and unsafe evidence | [Loom](https://docs.rs/loom/latest/loom/) and [Miri](https://github.com/rust-lang/miri) | Bounded interleavings and undefined-behavior checks complement review; neither proves production safety. |

Do not choose exact versions from the moving `latest` URLs. During implementation, record reviewed versions, minimum supported Rust version (MSRV), feature sets, license and advisory status in the lockfile and dependency decision. Serialization must have explicit format/schema versions and bounded decoding; a convenient serializer is not a durable compatibility policy.

## PackedGen reuse decision

**Conditional reuse as a replaceable, non-authoritative cache; do not make it the first durability or workflow dependency.** Prototype the simple cache backend first, then compare PackedGen against the same adapter contract and dataset.

Working-tree `Cargo.toml` identifies package `packedgen` 0.1.0, edition 2024, MSRV 1.88, default features empty. `src/lib.rs` rejects non-64-bit targets and exports both `PackedCache` and `DirectPackedCache`. Its large experimental map/cache surface is broader than FerricStore needs; expose a narrow FerricStore adapter, not those types in public domain/application signatures.

`src/cache.rs` has explicit weight admission, TTL, eviction and maintenance configuration. The asynchronous eviction option documents that `PackedCache` currently ignores it. Evaluate the chosen concrete backend rather than attributing one backend's guarantees to every exported cache. Estimated value weight differs from total RSS: account for keys, index generations/overlays, allocator overhead, duplicate replacement values and retired memory held by readers. Reject oversize admission and preserve correctness on cache refusal.

The API is safe to call according to its intended contract, but implementation safety is substantive work. `src/cache.rs` locally allows unsafe operations for handles and arena transfers. `src/cache_arena.rs` uses raw pointers, `UnsafeCell`, `MaybeUninit`, pointer tags, and two reclamation approaches: `seize` for the generational arena and a custom sharded quiescent-state reader protocol for the direct cache. This defeats any claim that the project's Cargo `unsafe_code = deny` means it contains no unsafe code: deny can be locally overridden. Safety comments are evidence of intended invariants, not proof those invariants always hold.

Existing evidence assets are useful: `tests/direct_cache_differential.rs`, `tests/direct_packed_cache.rs`, `tests/direct_cache_allocations.rs`, `tests/packed_cache.rs`, and `tests/cache_concurrency_model.rs`; `scripts/cache_proof.sh` lists formatting, differential tests, concurrency models, Clippy, MSRV and benchmark checks. The Loom model imports the shared epoch protocol but models other state; it defaults to a preemption bound of three. This is partial coverage, not exhaustive verification of the complete production cache.

`docs/DEPENDENCY_POLICY.md` records an exception for unmaintained `fxhash` through `ptr_hash` and requires proving the excepted aliases are unused. `Cargo.toml` pins `ptr_hash` 2.1.1 with a comment explaining an unchecked-read issue in older versions. These are local evidence claims; independently run the advisory audit on the eventual FerricStore lockfile. Do not carry an exception automatically from another repository.

Adoption gates:

1. Identify a clean reviewed commit and stable API/features; audit every reachable unsafe module and dependency. Document allocation, publication, ownership transfer, ABA prevention, guard lifetime, reclamation, panic/destructor reentrancy and `Send`/`Sync` invariants.
2. Run its existing differential/model tests plus Miri-compatible pointer/lifetime tests; stress concurrent replacement/removal/rebuild, cache destruction with retained values, stalled readers and maintenance starvation. Record model bounds and unsupported Miri paths.
3. Benchmark real key/value distributions, misses, skew, churn, oversized values and stalled readers against Moka/quick_cache. Measure p50/p95/p99 latency, throughput, peak RSS, retained bytes and recovery after pressure. Define pass thresholds before measuring; no benchmark victory is claimed here.
4. Adapter returns owned values or `Arc` values and releases cache pins before `.await`. Cache keys include tenant/workflow identity and authoritative revision or generation. Applied writes invalidate/update coherently; snapshots/restarts can discard all cache entries. No stale cache value may grant a lease or validate fencing.
5. Maintain a configuration switch and behavior tests for the fallback backend. Unsafe PackedGen internals remain a dependency risk even when FerricStore adapter code forbids unsafe.

## Architecture tests: equivalent intent, layered enforcement

Use three complementary controls: Cargo dependency allowlists, Rust visibility, and architecture tests. Separate crates make dependency cycles invalid, but Cargo does not enforce the intended direction of an acyclic dependency graph.

Production workspace edges below follow the canonical `docs/architecture.md` (direct internal dependencies only). The earlier narrower API edge and application-dependent worker edge are superseded:

| Crate | Allowed internal dependencies |
| --- | --- |
| types | none |
| domain | types |
| ports | types, domain |
| application | types, domain, ports |
| consensus | types, domain, ports |
| storage | types, domain, ports |
| projection | types, domain, ports |
| api | types, domain, ports, application |
| worker | types, domain, ports |
| node | composition dependencies on adapters and application |

Consensus bridges OpenRaft to domain transitions and durable ports; storage must not depend on consensus. If an OpenRaft storage trait needs implementation over storage, put that bridge in consensus and keep storage's API library-neutral. Projection reads committed facts and exposes views; it cannot be the authority for claims. UI endpoints consume application query contracts; they cannot open storage directly. External workflow execution belongs behind worker/application contracts, outside deterministic apply.

The primary workspace check should inspect `cargo metadata` and fail on any non-allowlisted internal edge, including target-specific and optional dependencies. Distinguish dev edges from production, and prevent a dev dependency from leaking into shipped code. Check feature profiles explicitly and reject missing/unclassified crates. `pub(crate)`/private modules hide implementations; compile-fail examples verify callers cannot access forbidden APIs.

[archunit](https://docs.rs/archunit/latest/archunit/) 0.0.2 is a promising test-only equivalent of the architecture-rule intent: file/layer rules, dependency/cycle checks and strict empty selections. Its documented limitations are syntax-based extraction, no macro expansion or build-generated code analysis, conservative union of `cfg` branches, and file rather than item nodes. Named layer rules ignore endpoints outside declared layers, so add full coverage checks. It is an early release: adopt only after a small fixture evaluation with deliberately forbidden edges, re-exports, aliases, qualified references and generated-code cases. Keep the Cargo graph check authoritative for crate edges.

[arch_lint](https://docs.rs/arch-lint/latest/arch_lint/) is an alternative AST-based linter worth evaluating for local module rules. Do not deploy multiple overlapping architecture frameworks without a demonstrated gap. The Rust `arch_test` documentation endpoint could not be retrieved in this research pass; no selection or maintenance claim is made about it.

## Standard lint and CI policy

[Clippy's official guidance](https://doc.rust-lang.org/clippy/usage.html) supports normal lint groups, warnings-as-errors and targeted exceptions; it explicitly discourages enabling the entire restriction group. Use workspace lint inheritance for every package, with a check that every manifest opts in.

Proposed policy: forbid first-party unsafe in types/domain/ports/application/api/worker/projection/node and storage/consensus unless a later reviewed decision creates a dedicated low-level crate. Use `forbid(unsafe_code)` at crate roots so local allowances cannot bypass it. Still deny `unsafe_op_in_unsafe_fn`, require documented unsafe blocks in any approved low-level exception crate, and inventory transitive unsafe separately. Start Clippy `all` plus `pedantic`, tune specific false positives with scoped reasons, and select restrictions such as production `unwrap_used`/`expect_used` and `await_holding_lock`. Test assertions may have narrow allowances. Avoid blanket lint suppression.

Required future gates: `cargo fmt --all -- --check`; locked workspace check/tests; Clippy over all targets and valid feature combinations with warnings denied; architecture tests/graph audit; doc tests; [cargo-deny](https://embarkstudios.github.io/cargo-deny/) advisory, source, license and dependency policies. Use `cargo clippy ... -- -D warnings` for older toolchains; official docs recommend `CARGO_BUILD_WARNINGS=deny` from Cargo 1.97. Pin the toolchain and choose the supported form rather than assuming that new Cargo exists.

Do not blindly enable mutually exclusive features with `--all-features`; maintain a valid feature matrix including the production profile and supported MSRV. In protected release CI, add crash/restart/snapshot tests, bounded Loom cases and supported Miri checks. Test deterministic replay with injected command timestamps/randomness, lease fencing on retry/leadership changes, idempotency and atomic applied-position persistence. This note specifies future gates; it does not claim any passed.

## Peer communication and review record

Supplied peers: Sartre/workflow `01a12020-abe9-7c80-81e0-b7ddbba219c1`; Noether/raft-storage `01a12020-ac4d-7f21-bd6c-cf0e94884dfe`; Darwin/rust-quality `01a12020-acb1-7b30-b118-747b9908ed6c`.

Initial exchanges: none through direct tools. Discovery found no `send_input`, spawn, or agent communication capability. Subsequent review uses peer-authored files and parent-relayed messages, recorded in `docs/reviews/rust-quality-review.md`. Reading a peer file and writing a reply is file communication; it does not imply that the peer received or accepted the reply. Mutual agreement remains pending until responses arrive.

Review requests prepared for delivery:

- Sartre: confirm cache cannot determine claim/lease/fencing outcomes, authoritative time is carried in replicated inputs, UI reducers remain pure, and proposed API/worker edges cover the workflow contracts.
- Noether: confirm consensus owns OpenRaft/storage bridges without storage-to-consensus dependencies; identify required durable transactions, snapshot fences and cache invalidation boundaries; evaluate redb before accepting it.
- Darwin: challenge architecture checker blind spots and lint inheritance; verify dirty PackedGen findings against a clean candidate commit; agree on unsafe review and benchmark gates.

No peer conclusions, responses, or endorsements have been invented. Main architecture and implementation-plan authors should incorporate the conditional cache decision, allowlist direction and pending review status.

### Final review status

The pending statements above describe first-round history, not current status. Completed file-based and parent-relayed quality cross-review, actual peer replies, Parfit attribution, and bounded provenance checks are recorded in [rust-quality-review.md](../reviews/rust-quality-review.md). The parent reports final architecture/plan/decision integration of time, identity and watermark wording; see [decisions.md](../reviews/decisions.md). Quality review is complete with no blocking material contradiction identified. Implementation, library conformance, safety/performance measurements and source-parity validation remain future gates; none is claimed complete by this design review.
