# ADR-010: panic lints and runtime effect rules

Status: accepted, 2026-11-20. Solo decision on the user's request; no
independent review is claimed.

## Motivation

The user asked whether the workspace uses good lint rules and architecture
tests before continuing the Rust runtime. The existing gates are strict, but
two gaps matter for a storage server that will parse untrusted network bytes
and run per-shard threads with a group-committed log (the shape measured by
the `claude/fsnp-slice` spike):

1. A panic in a shard or connection task stops that shard. Clippy's default
   and pedantic groups do not flag indexing, slicing, unchecked arithmetic or
   `assert!` inside functions that return `Result`.
2. Nothing kept filesystem access, durable sync calls, thread creation or
   blocking calls in async code in their intended owners.

## Measurements (e29a5f3, Rust 1.99.0)

Candidate restriction lints were run as warnings over all targets. Production
counts (library and binary targets, no test configuration):

| Lint | Production sites |
|---|---|
| `clippy::arithmetic_side_effects` | 946 |
| `clippy::indexing_slicing` | 845 |
| `clippy::panic_in_result_fn` | 0 (3,151 in tests) |
| `clippy::let_underscore_must_use` | 4 (11 in tests) |
| `clippy::string_slice` | 0 (3 in tests) |
| `unreachable`, `get_unwrap`, `unwrap_in_result`, `mem_forget`, `exit`, `fallible_impl_from` | 0 |

`wf-domain` holds 1,170 of the 1,791 indexing/arithmetic sites. Rewriting them
in one change would touch Elixir-parity code without behavioral evidence, so
those two lints get a ratchet instead of a workspace deny.

## Decision

**Workspace denies.** `unreachable`, `string_slice`, `get_unwrap`,
`unwrap_in_result`, `let_underscore_must_use`, `mem_forget`, `exit` and
`fallible_impl_from` join the root lint table and the manifest check. The 19
existing sites were fixed: the parity verifier now reports child cleanup
failures, failed fault tests fold child cleanup errors into the returned error,
test hex decoders no longer slice strings, and two intentional drops (a send to
a receiver that may have timed out, a fault-mode compare-exchange) are named
bindings with a stated reason.

**Production panic ratchet.** `tests/lint/panic-ratchet.py` runs Clippy on
library and binary targets with `arithmetic_side_effects`, `indexing_slicing`
and `panic_in_result_fn` as warnings and compares per-file counts with
`architecture/panic-baseline.json`. A count may not rise and a new file starts
at zero, so all new production code is held to these lints. A count that falls
also fails until `--update` rewrites the baseline, so the file always states
the real remaining debt. `scripts/check.sh` runs the ratchet and its unit tests.
Test code is outside the ratchet: a failed test is the intended outcome of a
panic there.

**Runtime effect ownership.** The syntax checker enforces, for production
sources of runtime packages (tests and tooling are exempt):

| Rule | Policy key | Current owners |
|---|---|---|
| `std::fs` only in approved packages | `filesystem_packages` | `wf-storage` |
| `sync_all` / `sync_data` (method or path) only in approved modules | `durable_sync_modules` | the three existing directory/database sync sites in `wf-storage` |
| `std::thread::{spawn, Builder, scope}` only in approved modules | `thread_owner_modules` | none yet |
| no blocking call inside `async` code | (rule) | n/a |

Blocking means `std::fs`, `std::net`, `std::thread::sleep`, or a non-awaited
`sync_all`, `sync_data`, `recv` or `recv_timeout` call. A method with one of
these names is allowed when it is awaited directly (an async API of the same
name). A synchronous closure or nested function inside async code is outside
async context, so `spawn_blocking(|| ...)`-style offloading passes. Printing is
already refused by the existing runtime macro audit.

Each owner list is required in `architecture/policy.toml`. Adding an owner is a
reviewed policy change; the runtime ADR that introduces shard threads and the
log writer names them.

## Consequences

- New runtime code (TCP codec, log writer, shard executor) is checked for
  indexing, slicing and unchecked arithmetic from its first line.
- Existing `wf-ports` effect-neighbor test controls now assert the filesystem
  finding explicitly instead of an empty list.
- Pay-down of the 1,791 baseline sites is separate work; each fixed file
  shrinks the baseline through `--update`.
- These are syntax checks. They do not prove the absence of indirect effects
  through helper crates; dependency allowlists still govern which crates exist.
