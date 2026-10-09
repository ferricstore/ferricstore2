# ADR-003: checked Rust source size limits

Status: accepted for the authorized M1 continuation, 2026-10-09.

## Motivation and decision

The user asked about file sizes alongside linting and architecture quality.
The saved single-record reducer had grown to 768 lines. Add executable sizing
checks so a growing module prompts a cohesive split during implementation.

Limit each production Rust source file to 400 physical lines. Tooling files and
production integration-test/benchmark files have a 600-line limit. Count blank
lines, comments and documentation; LF and CRLF use the same definition, and an
unterminated final line counts. A final newline does not add an empty line.
These limits are repository maintainability choices, not a universal Rust
standard or proof of design quality.

All runtime `src` files retain the production limit, including files named
`tests.rs` and conditional source. A filename cannot grant a larger production
budget. Tooling is classified by the existing architecture package policy;
integration tests and benchmarks use their existing checked target paths.
Preserved Elixir snapshots and explicit parser fixture data are outside the
first-party Rust source selection. No generated production-source exemption or
per-file suppression is introduced.

Keep Clippy's existing `all`/`pedantic` checks, including long functions and
argument counts. Split by responsibility and preserve documentation. Moving
business logic to an adapter, hiding source behind macros, or minifying code to
satisfy a count violates the existing architecture/review requirements.

## Boundaries and migration

No crate dependency or public root module allowlist changes. Private child
modules may divide the reducer while preserving its public command/read/plan
contracts. Existing oversized domain source and test files must be split before
the complete continuation gate can pass. There is no grandfathered exception.

The syntax architecture checker applies sizing to every selected first-party
Rust file; `cargo arch`, workspace tests, and the local/CI wrapper enforce it.
Technical read/parse failures still fail. Runtime behavior, atomicity, FIFO and
durability require their separate tests and review.

## Validation

Observed preimplementation failures cover 400/401 production and 600/601
tooling/integration boundaries, comment/blank-line counting, CRLF, missing final
newline and attempts to acquire a test budget by naming a runtime module
`tests`. Actual red/green commands and final review are recorded in
`docs/reviews/workflow-m1-resume-coordination.md`.
