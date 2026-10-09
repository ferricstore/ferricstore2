# M1 retry policy continuation

Owner: retry agent (`/root/m1_retry`), GPT-6.1-sol / max. Existing dirty work
is preserved. Write scope follows `workflow-m1-resume-coordination.md`; no peer,
root configuration, preserved reference, commit or push edits.

## Proposed contract, published before implementation

Keep `workflow::reduce(record, command)` and every represented existing operation
compatible. Add `Operation::RetryWithPolicy(PolicyRetry)` and
`workflow::reduce_with_policy(record, command, Option<&InstalledRetryPolicy>)`.
The latter borrows a coherent authoritative policy value supplied by the future
apply adapter; `reduce` uses no installed policy and resolves source defaults.
The policy value is an immutable validated retry-only view, not a storage API or
a claim that policy installation/generation/durability is implemented.

`PolicyRetry` carries the lease/fence guard, optional explicit `run_at`, normalized
command retry override, optional payload/error, and an explicit committed jitter
sample. No clock or hash/random source is consulted. Compatibility preparation
must supply the pinned source `phash2({id, next_attempt, now})` sample when exact
legacy jitter is required; hash production is outside this domain increment.
The concrete sample is `JitterSample(u32)` (pinned `phash2/1` returns a bounded
integer); application will require source-compatible preparation. All jitter
arithmetic uses the explicit sample and source modulo/spread/clamp ordering.

Resolution follows `RetryPolicy.resolve/3`: default, type override, matching
logical run-state override, command override. Retry fields merge independently;
an explicitly supplied backoff is normalized with source defaults before it is
merged. Validate source bounds (max retries 0..1000, delay 0..2592000000, jitter
0..100, nonempty exhaustion target except `running`). Installed policy lookup
uses the record workflow type and logical run state rather than `running`.

Check numeric command admission and explicit retry-time admission before legacy
no-op matching; check no-op before policy resolution, attempts increment or unused
jitter/scheduled-time work. A fresh retry requires current ownership, increments
attempts once, permits attempts <= max_retries, and otherwise exhausts. Scheduled
retry returns to the original logical state. Exhaustion enters the configured
target, with no due time for completed/failed/cancelled or committed `now` for an
active target, even if explicit `run_at` was supplied. Release lease/run-state,
retain fence, update payload only when supplied, replace/clear error, and preserve
the source result semantics. Emit exactly one retry event with all nine source
retry metadata observations. Terminal/active exhausted retry duplicates use the
existing latest-event fallback and emit no new record/history writes.

Source refinement before implementation: generated retry deadlines are `now +
delay` in `RetryPolicy.next_run_at_ms/4`; inspected next-record/key validators do
not impose command-input `run_at` admission on this computed deadline. Preserve
that distinction in the new policy path: explicit `run_at` remains capped at
9007199254740991, while computed deadlines use checked `u64` addition. Arithmetic
overflow is an explicit Rust representation rejection. The old caller-resolved
`RetryTime` path stays compatible with its existing admission contract.

Split `workflow.rs` into command/model, validation, create/claim, lifecycle, retry
and plan responsibilities under its existing subtree; keep production files <=
400 physical lines. Split the existing lease/guard tests by responsibility to
stay <=600 test lines. Public paths/reexports and the six frozen fencing test
functions remain stable. No new root domain module classification is proposed.

## Source lineage and current limits

Pinned checkout: `/Users/yoavgea/repos/ferricstore2-elixir-baseline`, commit
`66586c596e271408d9184af32eb31cd7218c126e`.

- `apps/ferricstore/lib/ferricstore/flow/retry_policy.ex`: defaults, override
  normalization, four-level resolution, attempts, backoff and jitter.
- `apps/ferricstore/lib/ferricstore/raft/state_machine/sections/flow_transition.ex`
  lines 1075-1185: no-op-first retry preparation, attempts, target, values and
  audit metadata.
- `.../flow_history_writes.ex` lines 474-509: schedule shortcut and latest-event
  legacy duplicate fallback.
- Preserved `flow_test/sections/flow_policy_exposes_type_state_retry_retention_defaults.exs`
  lines 396-581 and `flow_claim_due_drains_higher_priorities_before_lower_priorities_default.exs`
  lines 588-746: stored state exhaustion, command precedence, timing, generated
  bounded/deterministic cases, logical-state retry and stable audit metadata.

Storage, policy installation/generation fences, raw Elixir map/keyword parsing,
policy snapshot encoding/size, FIFO insertion, parent/child terminal side effects,
retention and batch atomicity remain explicit omissions. This increment is M1
domain evidence and cannot certify full workflow parity or M1 completion.

## TDD and peer review evidence

Claim coordination: the retry-only installed view intentionally has no execution
mode API. Claim selection can receive an authoritative `parallel`/`fifo` mode
value in its own snapshot, without depending on retry policy internals. Existing
`Claim`, `WorkflowRecord` getters and public reducer paths will be preserved.

Both peers independently read this contract and the pinned production rules;
their actual notes report no blocking contract finding. Read parity's published
schema-2 origin contract: complete original-runtime accounting and conservative
literal AST origins are appropriate; ambiguous cases and fixture/profile/setup
gates stay incomplete. Actual implementation review remains pending.

### Step 1: policy API compile-red, before implementation

Wrote seven integration tests in `tests/retry_policy.rs` with the preserved
timing, thirty-day and 1,800 generated source inputs; precedence, source numeric
errors, committed jitter/cap ordering and overflow are additional coverage.
Executed before adding any policy API:

```text
cargo +1.99.0 test --locked --offline -p wf-domain --test retry_policy --target-dir /private/tmp/ferricstore2-m1-retry-target
error[E0432]: unresolved imports Backoff, BackoffKind, InstalledRetryPolicy,
RetryOverride, RetryPolicy, resolve
error[E0432]: unresolved import wf_domain::retry::JitterSample
exit 101
```

This is compile-red; assertions did not run. Next add data contracts and rejecting
decision methods so behavior-red tests can exercise missing resolution/timing.

### Step 2: policy behavior-red, before resolution/timing

Added only data-contract structs and deliberately rejecting `resolve` and
`next_run_at` methods. The same command ran all seven tests and exited 101:

```text
running 7 tests
six timing/precedence cases: Error: UnsupportedOperation
source_policy_numeric_bounds_and_exhaustion_targets_reject_exactly: FAILED
left: "ERR unsupported single-record flow operation"
right: "ERR flow retry max_retries must be between 0 and 1000"
test result: FAILED. 0 passed; 7 failed; 0 ignored
```

This is observed behavior-red. Saved before replacing the rejecting methods or
implementing policy validation/installed lookup. Pure defaults are data values,
not successful resolution. Source-shaped workflow transitions are the next
independent red increment after these fixture policies work.

Step 2 green: the identical pinned locked/offline command exited 0 with **7
passed, 0 failed, 0 ignored**. All 1,800 original kind/base/max/jitter/attempt
combinations ran inside the generated test. Supplemental checks exercise the
fixed-vs-linear jitter cap ordering, spread rounding, zero base at `u64::MAX`
attempts (bounded computation), four-level precedence and checked overflow.

Read claim's actual published contract. Existing record/Claim APIs remain stable;
its borrowed authoritative complete FIFO lanes, all-head discovery and private
same-snapshot continuation preserve the intended domain boundary. No blocking
contract finding. Snapshot completeness/placement and apply transaction behavior
remain explicit adapter obligations; actual implementation review is pending.

### Step 3: reducer behavior-red, before policy retry transitions

Wrote nine source-shaped reducer scenarios in `tests/retry_policy_transitions.rs`
and its isolated deterministic fixture helper. Existing create/claim and policy
fixture setup succeeds; only the new retry branch rejects as unsupported.
Executed:

```text
cargo +1.99.0 test --locked --offline -p wf-domain --test retry_policy_transitions --target-dir /private/tmp/ferricstore2-m1-retry-target
running 9 tests
all nine tests: Error: UnsupportedOperation
test result: FAILED. 0 passed; 9 failed; 0 ignored
exit 101
```

Saved before policy retry behavior. Five scenarios retain exact source IDs and
assertions for stored-state exhaustion, command override, logical-state backoff,
active exhaustion and terminal audit metadata. Supplemental cases preserve
inclusive limits, terminal duplicates, explicit-time versus exhaustion,
no-op-before-policy/intervening rejection and payload/error/fence semantics.

Coordinator independently ran the original source section consumers at
`FlowTest:494:500:501`: **56 passed, 0 failed, 209 location-excluded**, seed
734816, 18.3s. Logs/JUnit are
`/private/tmp/ferricstore2-m1-source-order-retry-sections.log` and `.junit.xml`.
This is baseline source-subset evidence; reduced Rust tests still do not prove
policy installation durability, parent/child effects or full workflow parity.

Step 3 green: the identical command exited 0 with **9 passed, 0 failed,
0 ignored**. New policy retries release the lease/run state, retain fence,
increment attempts exactly once, preserve source payload/result/error rules and
carry all nine audit observations. Active/terminal exhaustion, explicit zero
deadline, inclusive attempt limit, source no-op fallback and stale intervention
checks pass. Cohesive workflow submodules preserve all existing public paths.
An owned unused import was removed; strict integrated checks follow the test
split and supplemental admission/boundary coverage.

### Supplemental first-pass checks and source split

Added three integration boundaries and one private restored-record/counter fixture
after core green; these passed on their first run and are not claimed behavior-red.
They check binary state/type names, invalid installed state names, command policy
admission before lookup/no-op, unrelated fresh policy rejection, generated due
time beyond the caller-input ceiling, counter overflow without partial effects,
preserved old results and cleared old errors. Extended the existing high-fence
no-op fixture to include policy retry.

Current full domain run exited 0: **66 passed, 0 failed, 0 ignored**, including
46 retry-owner/retained tests and the 20 currently available claim tests. The
owned subset is 5 unit + 2 core + 6 fencing + 9 lease + 5 legacy duplicate +
8 pure policy + 11 policy transitions. Existing 26 tests are retained; none were
ignored or removed. `workflow.rs` is 245 lines and its seven cohesive subtree
files are <=167 lines. `lease_and_guards.rs` is 486 lines; five legacy retry
regressions moved unchanged into `legacy_retry_guards.rs` (181 lines). All owned
production/test files satisfy the new 400/600 policy after isolated rustfmt.

Warnings-denied `wf-domain` docs pass. The workspace architecture case currently
reports only claim's `selection.rs` unclassified standard-library reference;
strict all-target domain Clippy currently stops at claim's `index.rs:68` tuple
type complexity. These routine integration diagnostics were sent to the claim
owner without editing its files or interrupting its work. Owned lint checks will
be completed after the owner resolves the shared crate diagnostics.

## Exact new source counterparts

The section filenames below are under preserved
`apps/ferricstore/test/ferricstore/flow_test/sections/`.

| Declaration ID | Source case | Rust root function |
|---|---|---|
| `ab0be7e9b8c01099` | policy section:396, stored state exhaustion | `retry_policy_transitions::stored_state_retry_policy_drives_retry_exhaustion_without_command_override` |
| `579ae5ad18ea20b4` | policy section:429, command precedence | `retry_policy_transitions::command_retry_policy_overrides_stored_state_policy` |
| `999b558a2dc276d2` | policy section:466, thirty-day cap | `retry_policy::flow_retry_policy_accepts_thirty_day_backoff_cap` |
| `bd7d579285676091` | policy section:483, standard backoff | `retry_policy::flow_retry_policy_computes_standard_backoff_timing` |
| `ecaed3fd34f388ec` | policy section:533, generated bounded deterministic inputs | `retry_policy::flow_retry_policy_keeps_generated_backoff_values_bounded_and_deterministic` |
| `e1a8cf7f619f136a` | priority section:588, logical-state retry and audit | `retry_policy_transitions::flow_retry_returns_to_claimed_run_state_with_computed_backoff` |
| `78839ca2df54b42f` | priority section:661, active exhaustion | `retry_policy_transitions::flow_retry_exhausts_to_configured_active_state` |
| `537f7338ecf1c648` | priority section:706, terminal audit | `retry_policy_transitions::flow_retry_terminal_exhaustion_keeps_stable_audit_metadata` |

`policy section` means
`flow_policy_exposes_type_state_retry_retention_defaults.exs`; `priority section`
means `flow_claim_due_drains_higher_priorities_before_lower_priorities_default.exs`.
These names are integration-test binaries, not inline modules: ledger
`rust_module` remains empty. Tests retain exact typed state, attempts, timing,
ownership, nine retry audit values and single-record eligibility/cardinality
assertions. Source audit string serialization is represented by typed scalar/
optional values, so public wire bytes are not certified. Random source IDs and
tokens become explicit committed fixture bytes where source equality permits.
Policy installation/readback is a supplied validated authoritative view; event
reads observe emitted history. Neither substitutes for durable adapter parity.

## Incoming review resolutions

**P-R1 accepted and owner-corrected.** Parity independently found that the active
exhaustion case discarded its final claim outcome, so the retained ID assertion
alone would also pass after `NotEligible`. The counterpart now requires
`Outcome::Changed`, `running` state, logical `payment_failed` run state and a
`worker-manual` lease, preserving the source singleton-success/ID assertion at
priority section lines 695-703. Fixed fixture initial-claim cardinality as well;
it now requires a changed plan. Retained exact deterministic worker names for
stored state (`worker-policy`), command override (`worker-policy-override`) and
post-backoff (`worker-charge-b`) source calls. This corrects assertion strength,
not production behavior; a passing new assertion is supplemental, not a claimed
behavior-red cycle. Parity will independently re-read/rerun the correction.

Coordinator actual review found no represented behavior blocker and independently
ran 25 relevant integration tests green. Its minor history documentation finding
is resolved: `EventKind::Retry` now describes both scheduled and exhausted retry.

## Independent final reviews and verification

### Claim owner

Read the actual claim root, all five child modules, all four counterpart files,
the full published contract/red evidence, pinned `flow/fifo_lane.ex`, its complete
test suite and production `flow_fifo_lane.ex` head/merge helpers. Exact u128 member
bytes, running-before-queued lane order, logical-state lane identity, priority /
due / binary ID order, direct complete lane heads and the 80-follower fixtures
agree with the represented source rules. The selector never issues record writes
or claims that supplied indexes are durable.

Independently re-read coordinator findings **C-C1/C-C2** after owner resolution:
`cursor_for` now compares explicit group/revision/request values, with no process
pointer identity. Every consumed due row and FIFO head is checked against the
borrowed unique `RecordIndex` under partition/record ID. This rejects a conflicting
second-page row without a growing cursor history or whole-partition clone. Read
the owner's actual pre-fix 8-pass/2-failure record; independently executed the
resolved reconstruction/conflict tests inside the full domain run. **34 claim
tests passed** (8 selection, 5 lifecycle, 8 lane, 13 limits), none ignored.
The early tuple/std-module integration diagnostics are now resolved; strict
all-target domain Clippy and workspace architecture both pass. No new represented
behavior blocker found. Group/recovery revision identity, index completeness and
atomic index/record apply remain explicit adapter obligations.

### Parity owner

Read actual schema-2 `registration.rs`, `runtime_evidence.rs`, `macro_origin.rs`,
the collector, AST recovery/reconciliation scripts, source-origin review data,
and adversarial registration/AST tests. Every retained original runtime identity
must be linked once or appear in the exact unmatched complement; frozen
declarations retain their separate exact unresolved complement. Tags/module/name
remain equal to the original raw registration; source/use hashes, anchors,
reviewed scope and origin IDs are rechecked. Origin requirements cannot assert
setup, fixture or profile execution. Source data/parser artifacts remain distinct
from behavioral execution and the full gate still has no passing path.

Independently ran `elixir scripts/tests/runtime-reconciliation-test.exs`: **8
passed, 0 failed**, seed 638421. This includes the actual alias-rebinding and
literal grouped-alias resolution after coordinator **R-P1**; deliberately raising
test bodies remain inert because recovery parses AST without evaluation. Also ran
the actual `registration_v2` binary using pinned locked/offline Cargo in this
owner's isolated review target: **5 passed, 0 failed, 0 ignored**, 21.08s. Tests
copy the real retained artifacts into private fixtures; no peer/reference writes
were made. No new concrete blocker found in this reviewed increment; source-root,
scope/fixture/profile and execution gaps remain visible. P-R1's owner correction
is above and remains for the parity peer's independent re-review.

### Coordinator architecture guard

Read actual ADR-003, `size.rs`, source-check integration and all four red-driven
size regression tests. Limits count comments/blanks, CRLF and an unterminated
last line and do not grant production `src/tests.rs` a tooling budget. The rule
keeps dependency/module boundaries and has no generated/suppression exemption.
Independently ran the architecture library: **16 passed, 0 failed, 0 ignored**,
then the resolved `complete_workspace_obeys_policy` test: **1 passed**, exit 0.
No new sizing/architecture finding. These checks measure source organization,
not workflow correctness or durability.

### Final owned green

After P-R1 and current claim-owner fixes, full `wf-domain` tests exited 0:
**80 passed, 0 failed, 0 ignored**, comprising 46 retry-owner/retained plus 34
claim tests. Strict `clippy --all-targets -- -D warnings` also exited 0, with no
suppression. Warnings-denied domain documentation passed. Cargo used
`/private/tmp/ferricstore2-m1-retry-target`, isolated docs and architecture review
targets; formatting touched only owned files with `skip_children=true`.

Owned runtime files: `src/{lib,policy,retry,value,history,workflow}.rs` and
`src/workflow/{command,create_claim,lifecycle,plan,retry_transition,validation,tests}.rs`.
Owned tests: changed `lease_and_guards.rs`, new `legacy_retry_guards.rs`,
`retry_policy.rs`, `retry_policy_transitions.rs` and `retry_policy_support/mod.rs`.
Frozen `fencing_retry.rs` is unchanged, preserving existing progress hashes.
No root configuration, peer implementation, dependency, reference snapshot,
existing mapping, commit or push changed by this owner. The increment is ready
for coordinator integration and further review; M1/full parity remain incomplete.
