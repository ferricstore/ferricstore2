# ADR-012: one integration-test binary per crate and lighter debug info

Status: accepted, 2026-11-20. Solo decision on the user's request; no
independent review is claimed.

## Motivation

The user asked whether the workspace is laid out for fast compiling. The
15-crate layering already parallelizes well and production crates have no build
scripts or proc macros. Two costs remained: every integration-test file or
directory was its own binary (134 binaries, 59 in `wf-domain`), and the dev
profile kept full debug info. Linking those binaries was most of the rebuild
after a change low in the graph.

## Decision

- `wf-domain`, `wf-ports`, `wf-application`, `wf-types` and `wf-storage` set
  `autotests = false` and declare one combined target, `tests/main.rs`
  (`<crate>_tests`). It loads every earlier target unchanged as a module:
  `#[path = "<name>.rs"] mod <name>;` or `#[path = "<name>/main.rs"] mod <name>;`.
  No test file moves or changes, so the parity ledger's file hashes stand.
- These stay separate binaries, declared explicitly: targets that start
  themselves as child processes or name their own crate root (`wf-storage`
  `state_store`, `durable_log`, `group_log`; `wf-ports`
  `complete_policy_request`, `workflow_value_effects`), and the five
  `wf-domain` targets that include the shared `support` helper modules, which
  Clippy refuses to load twice in one binary. `wf-node`, the tooling crates and
  single-target crates are unchanged.
- The architecture checker allows a path attribute in a combined root only for
  a top-level module loading its own `<name>.rs` or `<name>/main.rs`; every
  other redirection still needs its audit gate (`src/tests/test_roots.rs`).
- `[profile.dev]` uses `debug = "line-tables-only"` and dependencies build
  without debug info. Panic locations and backtraces keep file and line.

## Measurements (M4 Max, Rust 1.99.0, `cargo test --workspace --all-targets`)

| | before | debug info only | both |
|---|---|---|---|
| clean build of all targets | 24.0 s | 20.7 s | 17.6 s |
| rebuild after a `wf-domain` edit | 9.2 s | 9.4 s | 4.8 s |
| rebuild after a `wf-node` edit | 1.1 s | 1.3 s | 1.0 s |
| full test run | 331 s | | 292 s |
| test binaries | 134 | 134 | 47 |
| `target/` after one build | 4.1 GB | 3.2 GB | 2.8 GB |

Every test still exists: 1,667 tests in the five crates before and after,
compared by name.

## Not changed

The test run, not compiling, is most of `scripts/check.sh`. `state_store`
(102 s wall, 976 s of test time) and `snapshot_recovery` (85 s, 1,305 s) set
the floor; their longest tests are fault-injection runs of 55-71 s. Making them
faster means changing those tests and is a separate decision. CI still has no
build cache and compiles cargo-deny from source on every run.
