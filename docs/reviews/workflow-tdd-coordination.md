# Workflow core TDD coordination

The user authorized implementation on 2026-10-09. Work is isolated on
`codex/workflow-tdd-core`; the dirty Elixir working checkout is preserved. The
semantic reference is detached commit
`66586c596e271408d9184af32eb31cd7218c126e` in
`/Users/yoavgea/repos/ferricstore2-elixir-baseline`.

## Ownership and shared contracts

- Types agent: `crates/wf-types/**` and
  `docs/reviews/workflow-tdd-types.md`.
- Domain agent: `crates/wf-domain/**` and
  `docs/reviews/workflow-tdd-domain.md`.
- Parity agent: `crates/wf-parity-tests/**`, new progress data under
  `tests/parity/` (the existing mapping remains frozen), and
  `docs/reviews/workflow-tdd-parity.md`.
- Coordinator: collection scripts/artifacts, shared configuration/policy,
  integration, handoff and this record. Only the coordinator commits/pushes.

All three agents use GPT-6.1-sol with reasoning effort max. They must read
each other's notes and implementations, exchange concrete findings, and record
resolutions. Notes must identify actual failing tests before production code and
passing tests afterward. Peer edits are forbidden; findings go to the owner.

The initial value contract is synchronous and dependency-free:

- `wf_types::identity::RecordId::new(Vec<u8>)` returns a checked nonempty
  binary identifier; `as_bytes()` borrows its bytes. The storage identity ceiling
  is 65,535 bytes; future adapters must also check the composed state-key limit.
- `wf_types::version::TimestampMillis::new(u64)` and `get()` preserve committed
  time, including zero; `checked_add(u64)` rejects overflow.
- `wf_types::version::Fence::new(u64)`, `get()` and `checked_next()` preserve
  monotonic fencing without wrapping.
- Partition-component encoding preserves the original `fpk:` byte-length
  representation, including arbitrary binary components and empty components.
  No UTF-8 or delimiter restrictions are added.

The domain increment implements a single-record deterministic reducer, not an
in-memory partition pretending to be durable storage. Caller-supplied committed
time and lease token are explicit. A successful change describes the affected
record and new history entry; an accepted legacy retry produces no second event.
Explicit request-ID outcome replay remains a separate future application/storage
contract. Lease/fence legacy retries must be derived from the last authoritative
event and reject different terminal operations or intervening mutations.

Collection is registration of real ExUnit tests from the complete pinned
application with its locked dependencies and support modules. It does not execute
tests or prove behavioral parity. Frozen textual declaration IDs stay immutable.
The new progress ledger may report source-derived tests and runtime reconciliation
but must never convert a registration or a manually edited status into a pass.

No runtime crate dependency or architecture boundary changes are planned. Partial
M1 behavior must remain explicit; FIFO selection, persistence, Raft, transports,
policies and wider workflow features need their own executed acceptance evidence.
