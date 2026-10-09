# Domain M1 TDD record

Owner: domain agent. Writes are restricted to `crates/wf-domain/**` and this
note. Reference: read-only full Elixir checkout at
`66586c596e271408d9184af32eb31cd7218c126e`. No commits, dependency/configuration
changes, or edits to preserved source/tests/mapping are authorized here.

## Published domain contract (before implementation)

The public entry point will be
`workflow::reduce(Option<&WorkflowRecord>, &CommittedCommand) -> Result<TransitionPlan, DomainError>`.
`CommittedCommand` carries the target `RecordId`, committed `TimestampMillis`,
and a typed `Operation`. Create accepts one absent record; other operations
accept one authoritative record of the same identity. No partition state,
candidate selection, I/O, wall clock, random generation, or request-ID store is
in this API. The only runtime dependency remains `wf-types`.

Implemented operations: create, explicit single-record claim, transition,
complete, fail, retry, lease extension, cancellation, and explicit single-record
reclaim. The opaque lease token is caller supplied;
claim increments the authoritative fence. Retry time has **distinct**
`Explicit(TimestampMillis)` and `PolicyResolved(TimestampMillis)` variants.
The latter is the committed caller-resolved scheduling time, not a claim that
this crate implements policy resolution, rounding, jitter, or exhaustion.

The reduced record will expose immutable lifecycle, logical run state,
version, fence, attempts, due/deadline, timestamps, and optional inline
payload/result/error observations. A transition plan contains at most one
record write (with expected previous version) and one new history entry;
accepted legacy duplicates contain neither. The record keeps only the latest
authoritative history witness needed for retry guards, not an entire history
or partition clone. Storage must commit record/history together and serialize
authoritative apply; this reducer alone provides no durability.

Uses the fixed shared API: `RecordId::new(Vec<u8>)/as_bytes()`,
`TimestampMillis::new(u64)/get()/checked_add(u64)`, and
`Fence::new(u64)/get()/checked_next()`. Exact supported legacy rejection
strings will be preserved. Rust arithmetic overflow receives an explicit
deterministic rejection, never wrapping or partial writes.

## Early source findings for peers/coordinator

- All six cases in
  `apps/ferricstore/test/ferricstore/flow/fencing_retry_test.exs` are mandatory.
  Their repeated calls have **no request ID**. Source-derived Rust tests must
  repeat those calls with new committed time and count exactly one event.
- `flow_history_writes.ex:461-472` checks terminal duplicates by matching
  terminal state + fence, and transition duplicates by current target state +
  absent lease + fence. It does **not** compare result/error/payload bytes or
  the old lease token. Adding an intent digest here would change the source.
- `flow_history_writes.ex:474-509` accepts retry duplicates with absent lease
  + same fence and either a matching schedule condition **or** a latest
  authoritative retry event with matching version/fence. An intervening
  queued-to-waiting transition with default `run_at=now` makes the old explicit
  retry stale in case four. The schedule shortcut must not be silently erased.
- `flow_transition.ex:1137-1148` uses explicit `run_at` when supplied;
  otherwise policy resolves time from the new call's `now`. Deduplication
  happens before this resolution affects state. `PolicyResolved` must preserve
  the original record/event even when a duplicate has a different resolved time.
- `flow_history_writes.ex:446-459` validates running/transition lease and fence
  by identity, without a deadline-time comparison. Deadline expiry enables
  reclaim; a token is not automatically revoked by this helper before reclaim.
  Do not invent stronger expiration semantics for this subset.
- `flow_terminal.ex` delegates preparation to the single-record helpers and
  then performs parent/child/governance side effects. Those transactional
  effects are outside this reduced model and remain unported.

## TDD evidence

### Step 1: create and explicit claim (observed red, before behavior)

Wrote `crates/wf-domain/tests/core.rs` and documented data-contract types with a
reducer that rejects every command. Executed:

```text
cargo +1.99.0 test --locked -p wf-domain --test core --target-dir /private/tmp/ferricstore2-domain-target
running 2 tests
test create_records_source_initial_counters_due_time_and_history ... FAILED
test explicit_claim_checks_due_time_and_preserves_logical_run_state ... FAILED
Error: UnsupportedOperation
test result: FAILED. 0 passed; 2 failed; 0 ignored; 0 measured; 0 filtered out
exit code: 101
```

The tests compiled against the peer's shared types and reached the rejecting
reducer. This is behavioral red, not a missing-import/compiler failure. This
entry was written **before** adding create/claim behavior. Fencing tests will
be added after these fixture operations pass so their red failures exercise
the actual missing terminal/transition/retry behavior, not failed fixture setup.

Step 1 green: the same command exited **0**, with **2 passed, 0 failed,
0 ignored**, after create and explicit due-record claim were implemented.
The checks cover initial counters, source event IDs, due boundary, opaque binary
tokens, logical run state, bounded write cardinality, deterministic repeat
decisions and borrowed input immutability.

### Step 2: all six legacy fencing scenarios (observed red, before behavior)

Added `tests/fencing_retry.rs` and the deterministic test-only scenario helper.
Create/claim fixture setup now succeeds; production transition/complete/fail/
retry still reject as unsupported. Executed:

```text
cargo +1.99.0 test --locked -p wf-domain --test fencing_retry --target-dir /private/tmp/ferricstore2-domain-target
running 6 tests
test complete_retry_after_committed_response_loss_is_noop_success ... FAILED
test fail_retry_after_committed_response_loss_is_noop_success ... FAILED
test retry_retry_is_guarded_by_latest_history_event ... FAILED
test same_fence_does_not_allow_different_terminal_effect ... FAILED
test transition_retry_after_committed_response_loss_is_noop_success ... FAILED
test retry_retry_without_explicit_run_at_preserves_original_policy_schedule ... FAILED
Error: UnsupportedOperation
test result: FAILED. 0 passed; 6 failed; 0 ignored; 0 measured; 0 filtered out
exit code: 101
```

Each case failed on its first required operation after successful source-shaped
create/claim. This entry was saved **before** implementing these operations.
The assertions explicitly require no second history/write for duplicates,
preserved committed data/deadline, intervening-transition rejection, and exact
`ERR stale flow lease` for fail-to-complete with the same fence.

Source numeric admission also applies: Flow mutation attrs cap `now_ms`,
`run_at_ms`, lease duration and resulting deadline at 9,007,199,254,740,991.
Shared primitive time/fence types retain all `u64` values; the domain is the
source-compatible command admission boundary for this subset. Version mismatch
for cancel must use `ERR flow changed concurrently` (transition helper line 693),
not an invented version label. This correction will be implemented with cancel.

Step 2 green: `cargo +1.99.0 test --locked -p wf-domain --test core --test
fencing_retry --target-dir /private/tmp/ferricstore2-domain-target` exited **0**:
**2 core + 6 fencing tests passed**, none ignored. This is reducer-level
source-derived behavior, not execution of the pinned storage/partition fixtures.

### Step 3: extension, cancel, explicit reclaim (observed red, before behavior)

Source review established all three are feasible in the reduced model. Added
12 integration tests in `tests/lease_and_guards.rs` before their implementation.
Observed command:

```text
cargo +1.99.0 test --locked -p wf-domain --test lease_and_guards --target-dir /private/tmp/ferricstore2-domain-target
running 12 tests
test extend_replaces_deadline_even_when_shorter_and_retains_token_fence ... FAILED
test expiry_without_reclaim_does_not_revoke_source_lease ... FAILED
test cancel_enforces_guard_order_and_repeated_cancel_is_terminal_rejection ... FAILED
test reclaim_boundary_mints_new_fence_and_rejects_all_old_owner_effects ... FAILED
test active_record_rejects_foreign_token_and_stale_fence ... FAILED
test deadline_ceiling_rejects_arithmetic_before_any_write ... FAILED
left: "ERR unsupported single-record flow operation"
right: "ERR flow changed concurrently"
left: "ERR unsupported single-record flow operation"
right: "ERR stale flow lease"
left: "ERR unsupported single-record flow operation"
right: "ERR flow lease_ms deadline exceeds maximum 9007199254740991"
test result: FAILED. 6 passed; 6 failed; 0 ignored; 0 measured; 0 filtered out
exit code: 101
```

This entry was saved **before** extension/cancel/reclaim production behavior.
Six extra regression/source-observation tests already passed at their first run;
those are additional coverage, not claimed behavioral red. They preserve the
source retry schedule shortcut, latest-history fallback with changed explicit
time, changed duplicate terminal values, original logical retry state, zero
deadlines, binary inputs, identity selection, and legacy errors.

Step 3 green: the full domain Cargo test command exited **0**, with **20 tests
passed** (2 core, 6 mandatory fencing, 12 lease/guard), no ignored tests. Added
three private boundary fixtures for version/fence/attempt overflow; they passed
at their first run and are additional safety coverage, not claimed red. The next
full run observed **23 tests passed**, no failures/ignored. Strict crate Clippy
(`--all-targets -- -D warnings`) and rustdoc (`RUSTDOCFLAGS=-D warnings`) passed.
Clippy's six initial style failures were resolved without suppressions.

### Step 4: unused policy-resolved duplicate time (observed red, before fix)

Self-review found that validating a caller-resolved policy time before the
legacy duplicate guard contradicts `flow_transition.ex:1075-1081`: duplicates
never reach policy resolution. Added a regression with original committed
deadline 9,600 and a duplicate's unused resolved deadline `u64::MAX`.

```text
cargo +1.99.0 test --locked --offline -p wf-domain --test lease_and_guards policy_duplicate_never_validates_an_unused_recomputed_deadline --target-dir /private/tmp/ferricstore2-domain-target
running 1 test
test policy_duplicate_never_validates_an_unused_recomputed_deadline ... FAILED
Error: RunAtTooLarge
test result: FAILED. 0 passed; 1 failed; 0 ignored; 12 filtered out
exit code: 101
```

Saved this red evidence before changing the validation order. Explicit client
`run_at` stays admission-validated before duplicate matching; a caller-resolved
policy time is validated only if a fresh mutation will use it. A legacy no-op
keeps the original affected record, deadline, version, attempts and event.

Step 4 green: the same focused command exited **0**, **1 passed, 0 failed**.

### Step 5: incoming numeric-fence finding T-D1 (red before owner fix)

Read the types peer's completed review and pinned `flow/options.ex:47-64`.
Accepted T-D1: the full-`u64` primitive is correct, but caller guard admission
must cap fencing tokens at 9,007,199,254,740,991 before lookup or duplicates.
Added two failing boundary scenarios before changing production validation.

```text
cargo +1.99.0 test --locked --offline -p wf-domain fence --target-dir /private/tmp/ferricstore2-domain-target
test workflow::tests::legacy_duplicates_cannot_bypass_the_source_fence_input_limit ... FAILED
Error: "duplicate bypassed numeric guard admission"
test result: FAILED. 1 passed; 1 failed; 2 filtered out
exit code: 101

cargo +1.99.0 test --locked --offline -p wf-domain --test lease_and_guards every_represented_fence_guard_is_source_admitted_before_record_lookup --target-dir /private/tmp/ferricstore2-domain-target
test every_represented_fence_guard_is_source_admitted_before_record_lookup ... FAILED
left: "ERR stale flow lease"
right: "ERR flow fencing_token exceeds maximum 9007199254740991"
test result: FAILED. 0 passed; 1 failed; 13 filtered out
exit code: 101
```

The integration case covers every represented guard (transition, complete,
fail, retry, extension, cancel), the accepted ceiling, ceiling+1, `u64::MAX`,
input immutability and validation before missing-record lookup. The private
case covers terminal/transition/retry no-op paths with a restored high fence.
This red record was saved **before** production fence validation changes.

Accepted peer integration finding T-D2: the new `error` root module was outside
the frozen classification, and integration-test roots need explicit unsafe
forbids. Resolve inside owned paths by moving the domain value-validation/error
types to the existing `value` module, with `workflow::DomainError` reexported
for the public reducer contract. No root policy/configuration change is needed.
The three integration-test crates will explicitly forbid unsafe code.

Parity P-D1 (source-range comments) was independently found during my assertion
linking and is corrected to exact pinned declarations/end lines and stable IDs.

Step 5 green: pinned locked/offline full domain tests exited **0** with
**26 passed, 0 failed, 0 ignored** (4 unit boundaries, 2 core, 6 mandatory
fencing, 14 lease/input/source guards). T-D1 now rejects oversized guards before
record lookup and before every no-op branch. T-D2 is resolved within owned
paths: the `value` module holds domain errors, `workflow::DomainError` is the
public reexport, and all three integration-test roots explicitly forbid unsafe.
P-D1 is corrected; the fencing counterpart file is stable for ledger regeneration.
Both peer owners/coordinator can re-review these exact owned changes.

## Pinned assertion links and reduced fixture mapping

All six functions are at the root of
`crates/wf-domain/tests/fencing_retry.rs`; ledger `rust_module` is `""` (the
integration-test binary name is not an inline module). The test-only scenario
helper performs source-shaped create/claim with the same committed `now` values,
`queued` state, `payload` and `fencing-worker`. Random identifiers/tokens are
replaced by explicit opaque committed values. Partition selection and durable
history reads are reduced to one affected record and emitted event observations.

| Frozen declaration | Source assertions | Rust assertions / preserved result |
|---|---|---|
| `8ad5a4d7b9e59aa1` complete duplicate | [first call](/Users/yoavgea/repos/ferricstore2-elixir-baseline/apps/ferricstore/test/ferricstore/flow/fencing_retry_test.exs:45), [duplicate](/Users/yoavgea/repos/ferricstore2-elixir-baseline/apps/ferricstore/test/ferricstore/flow/fencing_retry_test.exs:47), [one completed event](/Users/yoavgea/repos/ferricstore2-elixir-baseline/apps/ferricstore/test/ferricstore/flow/fencing_retry_test.exs:50) | [changed](/Users/yoavgea/repos/ferricstore2-rust-design/crates/wf-domain/tests/fencing_retry.rs:25), [no-op](/Users/yoavgea/repos/ferricstore2-rust-design/crates/wf-domain/tests/fencing_retry.rs:27), [count=1](/Users/yoavgea/repos/ferricstore2-rust-design/crates/wf-domain/tests/fencing_retry.rs:28) |
| `7dee0331572ef4c4` fail duplicate | [first](/Users/yoavgea/repos/ferricstore2-elixir-baseline/apps/ferricstore/test/ferricstore/flow/fencing_retry_test.exs:65), [duplicate](/Users/yoavgea/repos/ferricstore2-elixir-baseline/apps/ferricstore/test/ferricstore/flow/fencing_retry_test.exs:66), [one failed event](/Users/yoavgea/repos/ferricstore2-elixir-baseline/apps/ferricstore/test/ferricstore/flow/fencing_retry_test.exs:67) | [changed](/Users/yoavgea/repos/ferricstore2-rust-design/crates/wf-domain/tests/fencing_retry.rs:44), [no-op](/Users/yoavgea/repos/ferricstore2-rust-design/crates/wf-domain/tests/fencing_retry.rs:46), [count=1](/Users/yoavgea/repos/ferricstore2-rust-design/crates/wf-domain/tests/fencing_retry.rs:47) |
| `57bb245681edfa80` transition duplicate | [first](/Users/yoavgea/repos/ferricstore2-elixir-baseline/apps/ferricstore/test/ferricstore/flow/fencing_retry_test.exs:83), [duplicate](/Users/yoavgea/repos/ferricstore2-elixir-baseline/apps/ferricstore/test/ferricstore/flow/fencing_retry_test.exs:85), [one event](/Users/yoavgea/repos/ferricstore2-elixir-baseline/apps/ferricstore/test/ferricstore/flow/fencing_retry_test.exs:93) | [changed](/Users/yoavgea/repos/ferricstore2-rust-design/crates/wf-domain/tests/fencing_retry.rs:66), [no-op](/Users/yoavgea/repos/ferricstore2-rust-design/crates/wf-domain/tests/fencing_retry.rs:68), [count=1](/Users/yoavgea/repos/ferricstore2-rust-design/crates/wf-domain/tests/fencing_retry.rs:69) |
| `093dafce3f08d6c3` retry/latest history | [first](/Users/yoavgea/repos/ferricstore2-elixir-baseline/apps/ferricstore/test/ferricstore/flow/fencing_retry_test.exs:109), [duplicate](/Users/yoavgea/repos/ferricstore2-elixir-baseline/apps/ferricstore/test/ferricstore/flow/fencing_retry_test.exs:111), [count](/Users/yoavgea/repos/ferricstore2-elixir-baseline/apps/ferricstore/test/ferricstore/flow/fencing_retry_test.exs:118), [intervening transition](/Users/yoavgea/repos/ferricstore2-elixir-baseline/apps/ferricstore/test/ferricstore/flow/fencing_retry_test.exs:120), [exact stale error](/Users/yoavgea/repos/ferricstore2-elixir-baseline/apps/ferricstore/test/ferricstore/flow/fencing_retry_test.exs:127) | [first](/Users/yoavgea/repos/ferricstore2-rust-design/crates/wf-domain/tests/fencing_retry.rs:90), [no-op](/Users/yoavgea/repos/ferricstore2-rust-design/crates/wf-domain/tests/fencing_retry.rs:92), [count](/Users/yoavgea/repos/ferricstore2-rust-design/crates/wf-domain/tests/fencing_retry.rs:96), [transition](/Users/yoavgea/repos/ferricstore2-rust-design/crates/wf-domain/tests/fencing_retry.rs:99), [exact stale](/Users/yoavgea/repos/ferricstore2-rust-design/crates/wf-domain/tests/fencing_retry.rs:117) |
| `21028e29c3421d3a` omitted `run_at` duplicate | [first](/Users/yoavgea/repos/ferricstore2-elixir-baseline/apps/ferricstore/test/ferricstore/flow/fencing_retry_test.exs:147), [duplicate](/Users/yoavgea/repos/ferricstore2-elixir-baseline/apps/ferricstore/test/ferricstore/flow/fencing_retry_test.exs:149), [one event](/Users/yoavgea/repos/ferricstore2-elixir-baseline/apps/ferricstore/test/ferricstore/flow/fencing_retry_test.exs:156) | [first](/Users/yoavgea/repos/ferricstore2-rust-design/crates/wf-domain/tests/fencing_retry.rs:129), [no-op](/Users/yoavgea/repos/ferricstore2-rust-design/crates/wf-domain/tests/fencing_retry.rs:142), [count=1](/Users/yoavgea/repos/ferricstore2-rust-design/crates/wf-domain/tests/fencing_retry.rs:154); distinct caller-resolved 6,100/6,200 preserve original deadline; default policy execution is omitted |
| `eea318ca431453d0` fail then complete | [fail success](/Users/yoavgea/repos/ferricstore2-elixir-baseline/apps/ferricstore/test/ferricstore/flow/fencing_retry_test.exs:164), [complete stale](/Users/yoavgea/repos/ferricstore2-elixir-baseline/apps/ferricstore/test/ferricstore/flow/fencing_retry_test.exs:172) | [fail changed](/Users/yoavgea/repos/ferricstore2-rust-design/crates/wf-domain/tests/fencing_retry.rs:169), [exact stale](/Users/yoavgea/repos/ferricstore2-rust-design/crates/wf-domain/tests/fencing_retry.rs:194), no completed event |

Additional source anchors: [create counters/lease/run state](/Users/yoavgea/repos/ferricstore2-elixir-baseline/apps/ferricstore/lib/ferricstore/raft/state_machine/sections/flow_create.ex:259),
[single candidate eligibility](/Users/yoavgea/repos/ferricstore2-elixir-baseline/apps/ferricstore/lib/ferricstore/raft/state_machine/sections/flow_retention_values.ex:475),
[claim write](/Users/yoavgea/repos/ferricstore2-elixir-baseline/apps/ferricstore/lib/ferricstore/raft/state_machine/sections/flow_retention_values.ex:545),
[transition guards](/Users/yoavgea/repos/ferricstore2-elixir-baseline/apps/ferricstore/lib/ferricstore/raft/state_machine/sections/flow_transition.ex:452),
[complete](/Users/yoavgea/repos/ferricstore2-elixir-baseline/apps/ferricstore/lib/ferricstore/raft/state_machine/sections/flow_claim_native_plan.ex:546),
[fail](/Users/yoavgea/repos/ferricstore2-elixir-baseline/apps/ferricstore/lib/ferricstore/raft/state_machine/sections/flow_transition.ex:1325),
[retry preparation](/Users/yoavgea/repos/ferricstore2-elixir-baseline/apps/ferricstore/lib/ferricstore/raft/state_machine/sections/flow_transition.ex:1075),
[duplicate/lease helpers](/Users/yoavgea/repos/ferricstore2-elixir-baseline/apps/ferricstore/lib/ferricstore/raft/state_machine/sections/flow_history_writes.ex:446),
[extension](/Users/yoavgea/repos/ferricstore2-elixir-baseline/apps/ferricstore/lib/ferricstore/raft/state_machine/sections/flow_claim_native_plan.ex:459),
[cancel/guard order](/Users/yoavgea/repos/ferricstore2-elixir-baseline/apps/ferricstore/lib/ferricstore/raft/state_machine/sections/flow_transition.ex:1474),
[history identity versus event time](/Users/yoavgea/repos/ferricstore2-elixir-baseline/apps/ferricstore/lib/ferricstore/raft/state_machine/sections/flow_history_reads.ex:1154),
[numeric fence admission](/Users/yoavgea/repos/ferricstore2-elixir-baseline/apps/ferricstore/lib/ferricstore/flow/options.ex:47).

## Current quality and integration evidence

- Full domain tests: **26 passed, none ignored**, using the pinned locked/offline
  command and `/private/tmp/ferricstore2-domain-target`.
- Warnings-denied workspace rustdoc: **all 15 crates passed**, exit 0; target
  `/private/tmp/ferricstore2-domain-workspace-doc-target`.
- Complete architecture package: **20 passed**, including
  `complete_workspace_obeys_policy`, exit 0; target
  `/private/tmp/ferricstore2-domain-architecture-target`. The new module/unsafe
  integration findings are closed without any shared policy edit.
- Warnings-denied workspace Clippy: domain/type checks passed; the shared run
  exited **101** on parity `src/main.rs:51`, `clippy::if_not_else`. Coordinator
  please relay this routine owned correction to parity. No suppression or peer
  edit performed. Target `/private/tmp/ferricstore2-domain-workspace-clippy-target`.
- After reading the parity owner's correction, repeated the same workspace
  Clippy command: **passed all targets in all 15 crates**, exit **0**. No lint
  suppressions were introduced. The prior shared failure is closed.
- Pinned source `git status --porcelain=v1` remains empty; original dirty Elixir
  checkout was never written or reset. No commits, pushes, spawns or new dependencies.

## Peer review

### Types review (actual code and tests read)

Read both peer notes, all five `wf-types` implementation modules and all 19
integration tests; compared binary partition/record guards with the pinned source.
Executed `cargo +1.99.0 test --locked --offline -p wf-types --target-dir
/private/tmp/ferricstore2-domain-review-types-target`: **19 passed, 0 failed,
0 ignored**. Concrete accepted findings: the 65,535-byte identity guard is
separate from composed-key admission; `fpk:` preserves binary and empty
components; lengths use byte counts; checked time/fence arithmetic retains zero
and rejects overflow without effects. No blocker found in the agreed primitive
contract. Composed state-key ownership, bounds and scope remain correctly omitted.

### Parity review: actionable finding D-P1 (owner: parity)

Read `progress.rs`, `registration.rs`, `artifact.rs`, `rust_source.rs` and
`rust_tokens.rs`, plus the parity peer's schema/red evidence note. Identity
accounting, hash-bound scope, duplicate rejection and no handwritten pass status
are correctly distinct from execution. Found a specific false assertion link:
`own_assertion` accepts an inert `assert_eq!` token inside `stringify!` as an
actual Rust assertion of its containing test.

Reproduction (no peer edit; a Rust reviewer harness imported the actual peer
modules by `#[path]` and supplied only their `CheckError` adapter):

```rust
#[test]
fn fake() {
    let _ = stringify!(
        assert_eq!(1, 2)
    );
}
```

`find_test(source, "", "fake")` succeeds and
`check_assertion(source, &body, 4, "assert_eq!(1, 2)")` incorrectly returns `Ok`.
The test performs no assertion. Compiled with `rustc +1.99.0 --edition=2024
--test`, executable `/private/tmp/ferricstore2-domain-parity-review`. Running
`stringified_assertion_must_not_count_as_actual_assertion --nocapture` exited
**101**, with **0 passed, 1 failed** and diagnostic
`inert stringify! token accepted as an actual assertion`.

Requested resolution: owner adds a failing regression and makes the conservative
scanner skip nonassertion macro token trees (at least inert built-ins such as
`stringify!`/`quote!`) when finding executable assertion macros. No full execution
claim is involved; this is an incorrect *actual assertion link* within the
promised syntactic gate. Coordinator relayed D-P1; owner accepted it.

**D-P1 closed after independent re-review.** Read the owner's actual saved
pre-fix two-failure record, updated macro-token-tree and conditional-statement
handling, and active/inert regression tests. Recompiled the original reviewer
harness against those actual updated peer modules; ran the original reproduction
plus all eight peer scanner tests: **9 passed, 0 failed, 0 ignored**, exit **0**.
The original inert `stringify!` assertion now rejects, while direct active
assertions remain accepted. No peer paths were edited. The scanner remains a
conservative source-link check, not macro expansion or execution proof.

## Explicit omissions

No FIFO/lane/partition candidate selection, priority arbitration, installed
policy resolution/exhaustion, parent/child or multi-record transactions,
durability, Raft, effects, schedules, signals, retention, query projections,
transport, tenant/group/recovery admission, or explicit request-ID outcome replay.
This is a partial M1 record reducer, not full workflow parity or all 5,858 tests.
Create is ordinary create only; source idempotent-create matching is omitted.
Source max-active timeout, retention TTLs, attribute/state metadata, lineage,
named/blob values, physical key encoding, composed-key bounds and payload-size
admission are not represented. Inline bytes are a reduced value observation,
not a port of Elixir value encoding. A caller must enforce the omitted bounds
before constructing a service; no broader mode is accepted by an options map.
Group clock progression/ordering, lease-token uniqueness and durable fencing
namespaces remain outer contracts. The history snapshot is reduced, and its
latest witness assumes atomic authoritative record/event persistence. These
limitations are not passing/deferred cases in a full-parity gate.

## Owned files and final handoff

Runtime changes are `src/{lib,workflow,claim,lease,retry,history,value}.rs`.
The overflow/private-boundary fixtures are `src/workflow/tests.rs`.
Integration scenarios are `tests/{core,fencing_retry,lease_and_guards}.rs`, with
test-only `tests/support/mod.rs`. This note records the contract published before
dependent work, five observed red/green increments, six source assertion
mappings, both actual peer reviews, and incoming/outgoing review resolutions.

Incoming findings T-D1, T-D2 and P-D1 are resolved by this owner with passing
behavioral and architecture checks. Outgoing D-P1 is resolved by parity and
independently closed using the actual updated peer code. All domain code is
std-only except the fixed shared `wf-types` dependency; no Cargo manifest or
root/policy/lock/config/frozen mapping/reference change was made by this owner.
No production stub returns an accepted unsupported mode. Coordinator should
regenerate the hash-bound new progress ledger against the final fencing file
and retain the explicit reducer/policy/storage limitations above.

## Final cross-owner disposition and collector review

Re-read Parfit's and Zeno's actual final notes after the coordinator's roster
and integration updates. Types ownership is Parfit
(`01a12118-c342-7d23-9381-407204dcfb28`), domain is Cicero
(`01a12118-c3bf-78c0-bd46-fd70d58e2bfd`), and parity is Zeno
(`01a12118-c429-70d1-9e98-e07fa10871c4`). No direct peer-message tool is
available; this owned note and the coordinator handoff carry the relay.

- **T-D1: owner fix complete.** Re-read the final reducer admission at
  `src/workflow.rs:311-321` and the active boundary test at
  `tests/lease_and_guards.rs:283`. Every represented caller fencing guard is
  capped before record lookup or duplicate acceptance. Ceiling+1 and
  `u64::MAX` produce the exact source error; the accepted ceiling still reaches
  ordinary ownership checks. Private restored-record tests cover the legacy
  no-op branches. Saved pre-fix failures and the final **26-test green** remain
  above. Zeno independently read this actual fix and records resolution.
- **T-D2: owner fix complete.** Domain errors live in the classified `value`
  module with `workflow::DomainError` reexported; all three integration-test
  roots explicitly forbid unsafe code. The saved **20 passing architecture
  tests**, including workspace policy, are corroborated by Zeno's final run.
  Parfit's note still has earlier open dispositions for T-D1/T-D2. Coordinator
  should relay these exact code/test locations for Parfit's final independent
  read and closure in its own note; this owner does not edit that note.
- **P-D1 and D-P1: closed.** Zeno's final note confirms corrected pinned
  declaration ranges/IDs and regenerated hashes/assertion links. The original
  inert `stringify!` reproduction and eight actual updated peer scanner tests
  independently passed here, as recorded above. T-P1 is also independently
  closed by Parfit. No outstanding incoming finding remains against domain,
  and both peer implementations have been reviewed using their actual code.

Read the complete parent `scripts/collect-elixir-cases.exs`, the registration
schema, and the actual parity registration validator. Its pinned commit and
manifest/file-byte checks, exact path/line reconciliation, NUL-joined six-field
case identity, unresolved complement and explicit blockers match that schema.
Generated/template locations without a unique frozen declaration remain in
`collection-details.json` and block completeness; the collector does not invent
declaration links. No concrete collector defect was found in this inspection.
Collector files were not written or executed by this owner.

Verified the registration/execution distinction against the actual pinned
helpers and installed Elixir 1.20.4 ExUnit source: `ExUnit.start(autorun: false)`
precedes helper loading, neither original helper opts into autorun, and no
`ExUnit.run`, `async_run` or `await_run` call appears in the inspected test
trees. The collector reports `exunit-registration-only`, stores
`test_execution: false`, preserves helper include/exclude configurations and
warns that tags do not certify profile execution. Loading helpers starts the
server application; that still provides no execution result for any case.

At this read, the parent collection directory/artifacts had not arrived yet.
Zeno owns integration of the real artifact into its ledger through
`generate-progress.py --collection tests/parity/collection/runtime-registration.json`
and subsequent `check-progress`; artifact/hash/manifest/assertion gaps must
remain explicit. Registration, including successful reconciliation of all six
fencing declarations, does not prove their source execution, Rust semantic
equivalence, FIFO selection, policy resolution, durable history, fencing across
storage/restarts, or full workflow parity. The single-record reducer's omissions
above remain unchanged by collection.
