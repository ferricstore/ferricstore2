# ferricstore2

Rust implementation of the reviewed workflow-only FerricStore design. The workspace, test support and quality checks are implemented, with a partial pure M1 domain core, source-derived tests, a synchronous durable log and a scoped transactional state backend. Snapshots, Raft integration and HTTP/TCP listeners remain incomplete. Preserved Elixir tests are mandatory acceptance inputs.

The design preserves explicit durable workflow transitions, leases, fencing, retries and workflow queries. It does not port the general Redis-compatible database or replay arbitrary user code.

## Development

Use [the development guide](docs/development.md) for setup and local/CI checks, [architecture enforcement](docs/architecture-enforcement.md) for dependency/module rules and [test infrastructure](docs/testing-infrastructure.md) for source integrity, deterministic helpers and the incomplete workflow-parity gate.

Read [the handoff and next steps](docs/CONTINUE.md) to continue implementation from this foundation.

## Read in this order

1. [Architecture](docs/architecture.md): modules, structs, traits, dependencies and execution model.
2. [HTTP + TCP transport design](docs/transport-design.md): shared contract, detailed protocols, resource limits and [mutual reviews](docs/reviews/transport-review-record.md).
3. [Workflow evidence and parity](docs/notes/workflow-analysis.md).
4. [Raft and storage](docs/notes/raft-storage.md).
5. [Library decisions](docs/library-decisions.md) and [cache reuse/quality gates](docs/notes/rust-libraries-quality.md).
6. [Implementation plan](docs/implementation-plan.md).
7. [Existing Elixir test parity](docs/workflow-test-parity.md) and [test inventory](docs/workflow-test-inventory.md).
8. [Review record](docs/reviews/decisions.md) and [source provenance](docs/source-provenance.md).

## Status

M0 infrastructure, partial M1 domain implementation and scoped M2 persistence, with reviewed architecture and transport contracts. The development guide records executable checks, source-size limits and remaining gaps. Further runtime library choices remain evaluation candidates; component tests and reference registration do not establish complete workflow parity, milestone completion, performance or production readiness.

Source checkout: ferricstore/ferricstore at `66586c596e271408d9184af32eb31cd7218c126e`, plus explicitly identified working-tree observations. Local original Elixir checkouts are preserved. New repository is private pending an explicit publication decision.
