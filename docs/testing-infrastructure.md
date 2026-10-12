# Testing infrastructure

M0 provides real infrastructure tests; partial M1 domain behavior now has
source-derived Rust counterparts, separately accounted for by `cargo progress`.
The runtime engine and full workflow parity remain incomplete. Read the
[parity contract](workflow-test-parity.md) before adding source ports.
[Continuation instructions](CONTINUE.md) record current results and gates.

The synthetic compiler-lineage child controls now require Elixir1.19.5 and
Erlang/OTP release28 even during Cargo's tooling-crate tests. The local wrapper
checks these prerequisites; CI installs its pinned Elixir/OTP profile as
documented in [development](development.md). These finite process controls do
not execute original workflow tests, helpers, setup or the original collector.
Compiler registration, pure artifact verification and required original
scenario execution remain separately accounted acceptance steps.

Dependency auditing uses the pinned repository-local cargo-deny binary and
ignored `.tools/advisory-dbs` database/lock directory. Run from the repository
root; normal fetching and all four required checks remain enabled. A cached
database or repaired location alone does not establish a passing audit.

## Isolated helpers

`wf-test-support` has no runtime first-party dependency. It is a tooling boundary
and may enter future tests through dev-dependencies, never production or build
dependencies. Its public APIs use owned values and `Result` for fallible effects;
the crate forbids unsafe and inherits the workspace lints without exceptions.

- `resources::TestResources` owns an isolated tempfile directory. Borrow its
  path while keeping the controller owner alive across a future child-process
  crash/restart. Drop attempts recursive cleanup; consuming `close()` reports
  cleanup errors. This is test resource ownership, not a durable storage adapter.
- `clock::ManualClock` starts from an explicit millisecond value, including
  zero. Clones share atomic state; advance and nondecreasing set reject overflow
  or backwards changes without modifying the value. There is no system clock,
  implicit timer progress or implemented runtime clock port. Future reducers
  must receive explicit committed time rather than read a clock themselves.
- `fault::fault_barrier()` returns a one-shot owned controller and hook. The
  hook acknowledges reaching its point and then pauses. Only `wait_reached()`
  can create `ReachedFault`, whose consuming `release()` resumes the target.
  Channel capacity is one. Drop/disconnection cancels coordination. Both waits
  require a bound in `(0, 60 seconds]`; missing, unhit or unreleased hooks fail.
  Wall time is a watchdog for failed test coordination, never workflow authority.

A reached hook proves that hook executed. Later crash/cluster ports must bind
it to a real target and a documented durability state: precommit,
committed-before-reply, persisted-before-projection/checkpoint or projection
replay as appropriate. Kill and restart the intended process/replica, reopen
the same owned store, require bounded readiness and clean up all tasks/settings.
Operation completion before injection must fail, as must an unhit hook. These
helpers do not implement process crashes, persistence, consensus or fake success.

## Immutable reference check

Run `cargo reference` or
`cargo run -p wf-parity-tests --locked -- check-reference [--root PATH]`.
The checker pins the preserved schema-1 manifest SHA-256
`7df8c7885a64b6276e84517dffa4f1ead0f592f6d0996b398cdb61551c1bae61`
and source commit `66586c596e271408d9184af32eb31cd7218c126e`.

It verifies all 480 baseline files' byte counts and SHA-256 hashes, nine separate
working-tree snapshots' hashes, exact inventory counts, file category totals,
5,858 unique declaration IDs, source ownership/line locations, bidirectional
file-to-declaration references and initial unported statuses. Portable relative
paths reject absolute paths, traversal, empty/dot segments, backslashes, drive
prefixes and control characters. Snapshot paths reject symlinks and non-files.
Missing or unmanifested snapshot files fail. Reads are bounded at 8 MiB for JSON
and 16 MiB per source artifact.

The checker verifies 88 literal `Code.require_file(path, __DIR__)` section edges,
including multiline calls. This is a deliberately narrow static convention,
not an Elixir parser, dynamic dependency discovery or complete fixture/runtime
closure. The snapshot lacks the full pinned application and dependencies.
Neither artifact preservation nor a textual declaration count can establish
registered or executed ExUnit runtime cases.

## Fail-closed parity gate

Run `cargo parity` or
`cargo run -p wf-parity-tests --locked -- require-parity [--root PATH]`.
It first requires intact references and the separate
[`tests/parity/workflow-mapping.json`](../tests/parity/workflow-mapping.json).
The mapping reconciles every preserved declaration ID to an `unported` entry;
its [schema](../tests/parity/workflow-mapping.schema.json) permits only honest
M0 absence of runtime collection and Rust execution. Missing/unknown/duplicate
IDs, unsupported fields/stages, manually passed statuses, invented runtime
cases/counterparts or attached evidence fail validation.

Even a valid mapping makes `require-parity` return a failure. There is no M0
acceptance branch. A future schema and executable evidence verifier must be
implemented before this gate can accept anything. They must collect real cases
from an isolated checkout of the pinned application, preserve module/describe/
runtime name/parameter identities, effective tags, setup/fixtures and baseline
failures, reconcile source lineage in both directions, map every assertion and
execute actual Rust counterparts under all mandatory profiles. Zero-case
collection, missing prerequisites, setup failure, ignored/filtered required
cases, unhit fault hooks and deferred included behavior keep parity incomplete.

Keep the legacy no-request-ID lease/fence retries separate from explicit
request-ID replay; do not fabricate reused request IDs. Preserve exact equality,
set equality, absent-versus-zero distinctions, public errors/encodings and the
80-follower deep FIFO lane-head assertions in the same call. Map source-allowed
fault outcomes rather than inferring stronger guarantees from test titles.
The preserved mapping remains an immutable M0 absence record. The separate
progress ledger tracks source-derived reducer assertions and real registration
without turning either into full-parity execution proof.

## Progress accounting

Run `cargo progress` or
`cargo run -p wf-parity-tests --locked -- check-progress [--root PATH]`.
The local/CI wrapper requires this check alongside reference and architecture
checks. It verifies equality with the frozen declaration IDs, hash-bound runtime
registration, exact source/test-function identities and active assertion links.
No editable passing status is accepted. Missing/unreviewed runtime origins and
unported assertions remain visible; a valid progress ledger is not an acceptance
certificate. Runtime collection/reconciliation instructions are in
[the collection guide](runtime-case-collection.md).

## Dependency decisions and actual checks

The following exact direct releases were resolved in the parent's Cargo.lock:

| Dependency / MSRV | Owner / enabled features | Reviewed purpose |
|---|---|---|
| tempfile 3.27.0 / 1.63 | test-support; default `getrandom` | standard owned isolated directories and cleanup |
| serde 1.0.229 / 1.56 | parity tooling; default `std`, `derive` | strict manifest/mapping decoding |
| serde_json 1.0.151 / 1.71 | parity tooling; default `std` | bounded JSON artifact decoding |
| sha2 0.11.0 / 1.85 | parity tooling; default `alloc`, `oid` | SHA-256 artifact integrity; explicit lowercase hex encoding |

Exact cached crate manifests/source and relevant primary documentation were
read. All four declare MIT OR Apache-2.0 licensing and support the pinned Rust
1.99 compiler. First-party safety does not assert that dependencies contain no unsafe.
Full transitive license/advisory/source review belongs to the repository-local
pinned cargo-deny policy and integration check; no security-audit pass is
inferred from these pins. Sources: [tempfile ownership](https://docs.rs/tempfile/3.27.0/tempfile/),
[Serde derives](https://docs.rs/serde/1.0.229/serde/),
[RustCrypto SHA-2](https://docs.rs/sha2/0.11.0/sha2/).

Locally executed checks are recorded in the [testing review](reviews/infrastructure-testing-review.md).
The 13 helper tests cover cleanup/isolation/reopen, invalid resource parents,
explicit/concurrent clock advances, unchanged state on rejection, acknowledged
pause/release, no-hook completion, disconnection, watchdog timeouts and invalid
bounds. The parity unit/CLI tests cover known SHA-256 vectors, path/symlink/size
rejections, inventory/ID/closure integrity, byte/hash tampering, missing/extra
files, fake evidence and actual success/failure exit behavior. These are
infrastructure results and supply zero passing workflow counterparts.
