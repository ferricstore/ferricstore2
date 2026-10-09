# Library decisions

These are preferred candidates and evaluation gates, not installed dependencies. Read the [quality/cache assessment](notes/rust-libraries-quality.md) and [Raft/storage assessment](notes/raft-storage.md) for evidence. Pin reviewed releases, features, MSRV and lockfile before implementation; moving documentation links are discovery references.

| Area | Proposed choice | Rationale and boundary |
|---|---|---|
| Runtime | Tokio | Async I/O and supervised tasks in outer crates; bounded queues and storage admission |
| HTTP | Axum | Typed edge handlers; delegate workflow rules to application |
| Consensus | OpenRaft, subject to adapter/fault-test gate | Keep consensus types private; no homegrown Raft; validate durable log, apply, read barrier and membership |
| Durable state | redb as first evaluation candidate | Transactional workflow state/index/outcome batches; prove crash and snapshot contracts before adoption |
| Serialization | Serde + serde_json for public JSON DTOs | Versioned explicit contracts; bounded input; persisted/log format is a separate ADR |
| Typed errors | thiserror | Public domain/application error enums, separate operational context; no untyped string matching |
| Diagnostics | tracing + subscriber chosen during scaffold | Outer crates emit spans/events; redact tokens/payloads; pure domain returns facts rather than logging |
| Worker HTTP client | reqwest, if external worker transport is selected | Reuse client connections; bounded timeout/retry/redirect; retry same request identity, never retry ambiguous effects blindly |
| Cache | Simple safe-facade backend first; evaluate Moka/quick_cache and PackedGen | Cache is disposable derived state; PackedGen conditional on safety and workload gates |
| Properties | proptest | Domain command sequences and independent reference model |
| Concurrent/unsafe checks | Loom and Miri where applicable | Bounded interleavings and UB detection, plus actual concurrency/fault tests |
| Architecture | Cargo graph allowlist + visibility/compile-fail + evaluate archunit | Closest proposed arch_test-style rule library; do not assume syntax analysis catches macros/re-exports |
| Standards | rustfmt, rustc warnings, Clippy all/pedantic with selected restrictions | Enforce baseline conventions and narrowly justified exceptions |
| Dependency policy | cargo-deny | Reviewed advisory, license, source and transitive dependency exceptions |

SerDe provides serialization traits and data-format integration ([official overview](https://serde.rs/)); durable schema stability is our contract, not a guarantee from Serde. [thiserror](https://docs.rs/thiserror/latest/thiserror/) derives standard Error implementations. [tracing](https://docs.rs/tracing/latest/tracing/) supports structured diagnostics; use it at effect boundaries. [reqwest](https://docs.rs/reqwest/latest/reqwest/) is the HTTP client candidate. These choices avoid creating custom infrastructure without a measured need.

The domain external-dependency default is standard library plus approved pure helpers such as thiserror. Serde derives in pure types/domain can be approved when necessary, but wire and persistence structs stay separate from WorkflowRecord. Tokio, Axum, OpenRaft, redb, cache libraries, reqwest and tracing effects are forbidden in domain. Serialization cannot serialize an invalid bypass of validated constructors.

No recommendation here establishes benchmark superiority, security clearance or a validated storage implementation. Use standard safe APIs first; any direct unsafe work requires a documented absence of a viable safe alternative.
