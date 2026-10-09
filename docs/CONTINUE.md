# ferricstore2: handoff and next work

Updated: 2026-10-09. Repository: [ferricstore/ferricstore2](https://github.com/ferricstore/ferricstore2), private.
Working checkout: `/Users/yoavgea/repos/ferricstore2-rust-design`.

## Current increment

The user authorized the Rust project infrastructure after the architecture and transport design. This increment creates the workspace, module boundaries, test helpers, lint/dependency policy, architecture checks and CI. Workflow business behavior, Raft/storage/cache adapters, live HTTP/TCP listeners and UI implementation remain later work.

The complete local infrastructure gate passes on macOS with Rust 1.99.0: formatting, compilation, strict Clippy, 53 infrastructure tests, three compile-fail doctests, rustdoc, architecture enforcement, reference integrity and all four dependency audits. `cargo parity` exits 1 with the explicit incomplete-workflow diagnostic. The review findings and reciprocal dispositions are recorded in [the infrastructure review record](reviews/infrastructure-review-record.md). A passing infrastructure gate must never be reported as workflow parity. Linux/macOS GitHub Actions are configured; remote execution is separate evidence from the completed local run.

## What to read

1. [AGENTS.md](../AGENTS.md): scope, safety, dependency rules and honest parity requirements.
2. [Architecture](architecture.md) and [ADR-002](adr/002-project-foundation.md): crate/module boundaries and this foundation increment.
3. [Development](development.md), [architecture enforcement](architecture-enforcement.md), [test infrastructure](testing-infrastructure.md): executable setup and checks.
4. [Workflow test parity](workflow-test-parity.md), [test inventory](workflow-test-inventory.md), [source provenance](source-provenance.md): the existing Elixir acceptance baseline.
5. [Transport design](transport-design.md), [library decisions](library-decisions.md), [Raft/storage notes](notes/raft-storage.md), [cache evaluation](notes/rust-libraries-quality.md).
6. [Implementation plan](implementation-plan.md): ordered milestones and exit evidence.

## Foundation structure

Twelve production crates reserve the reviewed boundaries: `wf-types`, `wf-domain`, `wf-ports`, `wf-application`, `wf-storage`, `wf-consensus`, `wf-projection`, `wf-wire`, `wf-api-http`, `wf-api-tcp`, `wf-worker` and `wf-node`. Their modules are scaffolds, not working workflow APIs.

Three isolated tooling crates cover architecture policy, deterministic test support and reference/parity bookkeeping. Production dependencies cannot include them. All first-party crates inherit workspace lints and forbid unsafe code. Rust is pinned to 1.99.0, edition 2024; dependencies are locked. Clippy, formatting, compiler and rustdoc checks enforce the base standards.

Architecture enforcement uses pinned archunit 0.0.2 plus declared Cargo edges, syntax/module checks, negative fixtures and compiler visibility. Its 20 tests are included in the 53 infrastructure tests above. Its documented limitations still require review of macro expansion, generated code, facades and future dependency capabilities; no syntax library proves complete semantic purity.

Test support provides owned temporary resources, an explicitly advanced clock and bounded fault hooks that acknowledge reaching a test point. A hook acknowledgement proves only that point was reached; later crash/cluster tests must establish actual persistence and Raft guarantees.

The preserved input is 480 baseline files with 5,858 textual declaration IDs plus nine separately identified working-tree snapshots. Those IDs are navigation, not collected ExUnit cases or executed tests. Every workflow declaration remains unported. `cargo reference` checks artifact preservation; `cargo parity` must fail in this increment.

## Run the checks

From the checkout root:

```bash
rustup toolchain install 1.99.0 --profile minimal --component rustfmt --component clippy
cargo +1.99.0 install cargo-deny --version 0.20.2 --locked --root .tools
bash scripts/check.sh
cargo parity
```

The last command is an expected failure until behavioral parity exists. The normal infrastructure wrapper must pass without skipping missing tools or accepting fabricated parity evidence. See the development guide for the supported host/feature matrix and dependency audit policy.

## Continue in this order

1. **Close the remaining M0 acceptance inputs.** Collect real runtime ExUnit cases from an isolated checkout of the pinned Elixir application and its dependencies; reconcile generated cases, section files, tags, setup, fixtures and baseline failures to the preserved declaration IDs. Freeze included workflow scope and explicit deferred/excluded dispositions. The standalone reference snapshot is not a runnable application. Preserve the original dirty checkout.
2. **Freeze compatibility before implementing reducers.** Preserve legacy lease/fence behavior separately from explicit request-ID replay. Record exact external partition, schedule and error values, FIFO tie ordering and lane-head lookup semantics. Do not invent request IDs to make legacy no-ID tests pass.
3. **Implement M1: pure types and domain.** Start with identity/bounds and explicit command/state/outcome models, then source-derived create/claim/lease/transition/retry/complete/fail/cancel scenarios. Use synchronous deterministic functions, bounded read views/write sets, explicit committed time/IDs and checked arithmetic. Port the original assertions and add meaningful model/property/replay tests. Keep I/O, clocks, randomness, cache and runtime types outside the domain.
4. **Evaluate and implement M2/M3: persistence and consensus.** WARaft is the Elixir backend baseline; OpenRaft is a migration candidate, with no promised legacy log/wire compatibility. Validate actual trait ownership through storage-neutral ports before adoption. Require atomic state/history/outcome/applied-position persistence, durable log acknowledgements, crash recovery, snapshots and three-node failure tests. `wf-node` wires storage and consensus; neither adapter imports the other.
5. **Implement M4: one usable workflow path over both transports.** Add narrow application ports, shared wire DTOs, common tenant authorization/admission, HTTPS and the designed dedicated TLS/TCP protocol, then bounded workers and graceful drain. Validate cross-transport retries and retained outcomes, malformed/slow clients and ownership of permits/response bytes.
6. **Expand only against milestone evidence.** M5 covers selected broader workflow behavior; M6 query/UI contracts; M7 retention and operations; M8 measured performance and optional cache adoption. Each included Elixir scenario needs an executed Rust counterpart with its original assertions.

## Decisions that must stay visible

- Mutation retries retain the original request ID and `{group_id, recovery_id}` scope. Timeout/cancellation after submission can leave an unknown outcome and cannot undo Raft. Durable replies follow commit, application and required persistence.
- The domain is the authority for workflow validity. Projections and caches cannot decide claims, leases, FIFO eligibility or authorization.
- Related atomic changes must remain inside one Raft group. Cross-group fanout and live migration require separate designs before support.
- The Rust cache library remains conditional. No unsafe PackedGen reuse is adopted here; measure a safe cache-free baseline first and prove correctness when caching is disabled, cold or evicted.
- No measured performance result or unconditional p99 guarantee exists. Later work must set a bounded-load SLO, admission limits, deadlines and response budgets, then measure client-observed tail latency, rejection and availability under healthy load and faults.

Three requested GPT-6.1-sol/max agents covered architecture, testing and CI, with shared-file communication, coordinator relays and reciprocal review notes. The review record names participants, findings, resolutions and actual validation. Their reviews caught and corrected malformed-policy panic, macro-hidden effects, client-facade capability leakage, lint drift and tooling compatibility/audit problems before integration.
