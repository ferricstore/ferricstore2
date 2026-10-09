# ferricstore2

Design for a Rust, workflow-only FerricStore runtime. This repository contains no implementation.

The design preserves explicit durable workflow transitions, leases, fencing, retries and workflow queries. It does not port the general Redis-compatible database or replay arbitrary user code.

## Read in this order

1. [Architecture](docs/architecture.md): modules, structs, traits, dependencies and execution model.
2. [Workflow evidence and parity](docs/notes/workflow-analysis.md).
3. [Raft and storage](docs/notes/raft-storage.md).
4. [Library decisions](docs/library-decisions.md) and [cache reuse/quality gates](docs/notes/rust-libraries-quality.md).
5. [Implementation plan](docs/implementation-plan.md).
6. [Review record](docs/reviews/decisions.md) and [source provenance](docs/source-provenance.md).

## Status

Proposed design, with validation gates before implementation. Library choices are recommendations, not benchmark or production-readiness claims. Linters and architecture checks are specified here; they are not installed or running yet.

Source checkout: ferricstore/ferricstore at `66586c596e271408d9184af32eb31cd7218c126e`, plus explicitly identified working-tree observations. Local original Elixir checkouts are preserved. New repository is private pending an explicit publication decision.
