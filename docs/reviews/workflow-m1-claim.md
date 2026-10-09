# M1 authoritative claim selection continuation

Owner: claim agent (`/root/m1_claim`), GPT-6.1-sol / max. Ownership is limited
to `wf-domain/src/claim.rs`, its subtree, new `tests/claim_selection*.rs`, and
this note. Existing uncommitted work is preserved; no peer edits, root policy,
dependencies, commits or pushes.

## Contract published before implementation

The existing `claim::Claim` fields and single-record reducer contract remain
unchanged. Add pure selection values beneath the existing claim module:

- `ClaimRecord<'a>` borrows `&WorkflowRecord`, authoritative partition bytes,
  priority 0..2, and exact u128 state-entry sequence. Metadata currently absent
  from the reduced record is explicit authoritative view input, not a client
  hint or a newly invented durable record field.
- `DueOrder<'a>` is a total index key: descending priority, ascending committed
  due time, binary ID, partition, logical state, and type. `LaneKey<'a>` is
  type/logical state/partition. `LaneOrder<'a>` sorts running before queued,
  then exact u128 sequence and binary ID. Running logical state falls back to
  `queued`, as in `FifoLane`; terminal records have no active lane identity.
- `AuthoritativeClaimView<'a>` borrows one coherent group's unique canonical
  `RecordIndex`, ordered parallel due map and FIFO lane maps. The canonical
  index was added through the C-C2 contract refinement recorded below. Ordered
  maps are supplied by the future
  authoritative apply adapter and borrowed, never built by cloning a partition
  inside selection. The complete map for each FIFO lane gives its head
  directly; follower windows cannot stand in for this view. All relevant lane
  heads are read before global ordering, with a separately bounded head budget.
  Completeness, coherence and placement remain adapter obligations; this pure
  function cannot authenticate a supplied snapshot or prove durable indexes.
- `SelectionRequest<'a>` carries group/type/partition and state inclusion/
  exclusion filters, optional priority, explicit committed time, claim limit,
  candidate-scan budget and lane-head budget. Source bounds are priorities 0..2,
  claim limit at most the default 1000, 4096 lane heads and 16384 scanned rows.
  The last two follow the source catalog budgets; exposing these as concrete
  limits is a bounded M1 contract, not parity for every configurable source
  admission option. Empty/duplicate filter components and mismatched groups
  reject before returning a selection.
- `select(view, request, continuation)` returns borrowed selected records with
  `Claim` or explicit expired `Reclaim` action, inspected counts and
  `Complete`, `ClaimLimit`, or `ScanBudget` stop reason. It never mutates the
  authoritative inputs or creates leases. FIFO emits only a lane's actual
  current head and never advances to its follower within the same selection,
  including when the head is live, not due, or filtered. Parallel rows merge
  with eligible FIFO heads by the total key. Invalid consumed index identity,
  unsupported FIFO priority, and duplicate selected scoped identity reject the
  entire call; no partially successful return is exposed.
- A private-field `SelectionContinuation<'a>` retains borrowed index keys,
  group/revision and exact request semantics. It resumes only the same coherent
  snapshot/request, and rejects a changed revision or scope. It is a pure view
  pagination cursor, not a durable public API cursor: after applying mutations,
  a future adapter must select anew from updated authority. Exhaustion is never
  mislabeled empty work. Candidate scans and lane-head reads are distinct bounded
  work counters; head reads can exceed the candidate budget without hiding a
  deep head. Output allocation is bounded by the requested claim limit.

No leader-only candidate list, cache, async/I/O, wall clock, random source,
storage service, durability acknowledgement or request-ID replay is introduced.
Automatic reclaim ratios, timeouts, governance/authorization, policy installation,
FIFO sequence assignment, index transaction maintenance and batch persistence
are separate unimplemented responsibilities. An explicit selection of an expired
running head is narrower than the source's complete reclaim scheduling process.

## Source and scenario lineage

Pinned clean source checkout:
`/Users/yoavgea/repos/ferricstore2-elixir-baseline`, verified HEAD
`66586c596e271408d9184af32eb31cd7218c126e`.

Read `flow/fifo_lane.ex` completely: lane identity, running logical state,
terminal omission, u128 big-endian member and running score -1 versus queued 0.
Read `raft/state_machine/sections/flow_fifo_lane.ex` completely: authoritative
head injection, no follower admission, parallel/FIFO merge and due/id tie-breaks.
Read `flow_claim_due.ex:193-230,308-319`,
`flow_claim_scan.ex:888-1131` and `flow_retention_values.ex:455-618` for priority
bands, state filters, due/ownership and selected-record mutation boundaries.

Mandatory source-derived fixtures planned for this slice: `flow/fifo_lane_test.exs`
u128 round-trip/ordering, truncation, running logical-state/rank and terminal
omission; `flow/invariant_test.exs:38-110,207-355,410-508,510-625,1102-1147`
lane cardinality/scopes, blocking/reclaim, deep head with 80 followers and global
due/mixed state order; first two declarations in
`flow_test/sections/flow_claim_due_drains_higher_priorities_before_lower_priorities_default.exs`
and first declaration in
`flow_test/sections/claim_due_multi_state_claims_earliest_due_job_instead_first_listed_state.exs`.
Reduced selection assertions will retain exact source IDs/locations and identify
lease/durability/response assertions omitted from this helper.

## Peer communication and contract review

Actually read both `workflow-m1-retry.md` and `workflow-m1-parity.md` on
publication. Retry owner confirmed directly that `Claim`, existing getters and
reducer paths stay stable; its installed value is retry-only and has no mode
API. Selection therefore receives authoritative FIFO mode through distinct
ordered index maps without depending on retry policy internals.

Independently compared retry's proposed contract against
`retry_policy.ex:161-178`, `flow_transition.ex:1075-1185`, and
`flow_history_writes.ex:474-509`: merge precedence, logical run-state lookup,
no-op-first handling and terminal/active exhaustion timing agree. Explicit
committed jitter leaves source hash preparation outside this pure core.
No blocking contract finding. Actual retry/parity code review, independent
verification and any findings/resolutions will follow after owned work completes.

## TDD and validation

Coordinator finding C-C1 accepted during actual source review: process-local
`std::ptr::eq` continuation checks violate the pure standard-library boundary
and value-deterministic decision contract. This owner will remove them after a
regression demonstrates equivalent reconstructed snapshots currently reject.
Continuation identity will be explicit group/revision/request values; revision
must uniquely identify complete coherent contents within the group and cannot
be reused after restoration (recovery namespacing is an adapter obligation).

Related review refinement C-C2, published before the dependent change: add a
borrowed `RecordIndex<'a> = BTreeMap<ScopedRecordId<'a>, ClaimRecord<'a>>` to
`AuthoritativeClaimView`. Each consumed due row and every FIFO head must equal
its unique canonical current record/metadata in this index. This rejects stale
or conflicting same-ID rows across continuation pages without storing a growing
partition history in a cursor. The selector borrows this map and only looks up
bounded consumed IDs; it does not clone or walk the whole authority map. Index
completeness/placement still belongs to the apply adapter. Actual pre-fix red
evidence and resolution will be recorded below.

Before C-C1/C-C2 production changes, added ten boundary/continuation tests.
`cargo +1.99.0 test -p wf-domain --locked --offline --test claim_selection_limits`
observed **8 passed, 2 failed**, exit 101. Failed functions:
`equivalent_reconstructed_snapshot_continuation_uses_values` returned
`StaleContinuation`; `conflicting_scoped_identity_rejects_on_later_continuation_page`
returned success for the second same-partition/ID record. Saved
`/private/tmp/workflow-m1-claim-continuation-red.log`. This evidence was recorded
before owner resolution. The other eight tests are supplemental first-pass
checks, not claimed behavioral red.

Added eight FIFO primitive and seven source selection tests before implementing
the new API. Observed **compile RED**, command `cargo +1.99.0 test -p wf-domain
--locked --offline --test claim_selection --test claim_selection_lane`, exit 101:
the requested claim index/member/selection imports do not exist. Saved output
`/private/tmp/workflow-m1-claim-compile-red.log`. An initial heterogeneous byte
array fixture error was corrected before this recorded run; it is not behavioral
red. Test assertions did not execute in compile red.

Implemented exact FIFO member/lane primitives and bounded parallel selection
first, without FIFO selection behavior. After correcting a temporary-borrow
fixture compile error (not behavioral red), the identical two-test-target
command observed **behavior RED**: selection target **3 passed, 4 failed**.
Failures are `fifo_selects_one_head_per_lane_in_entry_order`,
`fifo_partition_list_selects_only_one_head_per_listed_lane`,
`fifo_deep_head_is_selected_with_eighty_earlier_due_followers` and
`fifo_global_due_order_survives_deep_head_lookup`, where the parallel-only
selector returned zero FIFO records. Saved
`/private/tmp/workflow-m1-claim-fifo-red.log` before head-merge implementation.
Cargo stopped before running the primitive target on this failure.

Implemented direct authoritative head lookup and total-order merge. Same command
now **GREEN: 15 passed, 0 failed, 0 ignored** (7 selection + 8 primitive), saved
`/private/tmp/workflow-m1-claim-head-green.log`. Four FIFO failures are resolved;
the deep 80-follower fixture selects its head with candidate scan budget 1 and one direct
head read, and the global-order fixture compares both heads with scan budget 1.
Other passing primitive cases are compile-red-driven rather than claimed
behavioral red. A transient unused-import warning belongs to the retry owner's
ongoing split and was not edited here.

Passing checks do not establish full workflow parity or M1 completion. Cargo
uses isolated `/private/tmp/ferricstore2-m1-claim-target`, locked/offline pinned
toolchain; rustfmt is restricted to owned files.

## Final represented assertion lineage

All paths below are beneath pinned `apps/ferricstore/test/ferricstore/`.
`lane` means `flow/fifo_lane_test.exs`; `invariant` means
`flow/invariant_test.exs`; `priority` and `multi-state` mean the two complete
section paths named above. Declaration IDs are the frozen inventory identities;
assertion lines identify the retained source observations rather than a claim
that every assertion or the complete source scenario is implemented.

| Frozen declaration | Original assertions | Actual Rust counterpart and represented scope |
| --- | --- | --- |
| `7b6561ea0f73c257` | lane:12 | `member_round_trips_exact_128_bit_sequence_and_id`: exact u128 max and ID round-trip. |
| `4808d163b1ee3c1f` | lane:19 | `member_byte_order_preserves_sequence_beyond_float_precision`: adjacent sequences beyond 2^53 preserve byte order. |
| `83dcf64f5941ec87` | lane:34-35 | `lane_rank_preserves_sequence_beyond_float_precision`: first member and queued score; ordered Rust map replaces the native backend. |
| `6cb33b54203c08af` | lane:39-40 | `decoder_rejects_truncation_and_empty_id`: invalid members reject. |
| `f72492d5892f240b` | lane:53-55 | `running_record_remains_in_logical_state_lane`: semantic type/state/partition and member decoding; physical encoded key and colocation omitted. |
| `49ba97e84872f9d0` | lane:79-80,84,87-88 | `running_members_rank_before_queued_even_with_later_sequence`: running/queued scores and complete two-member order. |
| `835390d5953260a3` | lane:125 | `terminal_record_has_no_active_lane_identity`: terminal entry omitted. |
| `9bdbecf61fe86d5f` | priority:26,34,36,44 | `higher_priorities_drain_before_lower_priorities_by_default`: limit-one high then low selection; same-snapshot continuation replaces persisted claim calls. |
| `7d7eaf003a23555b` | priority:65,74,76,85 | `priority_option_targets_one_priority_band`: low-band singleton/ID and high-band ID; high-band singleton is not separately asserted. |
| `ed190d79e3813b5b` | multi-state:32-41 | `multi_state_claims_earliest_due_instead_of_first_listed_state`: limit-one chosen ID, partition and state with original due 100/due 10 ordering; compact response lease fields, token/fence types at 42-43 omitted. |
| `3c9f0622ef767c44` | invariant:68,77,79,96,105 | `fifo_selects_one_head_per_lane_in_entry_order` retains initial singleton/ID; `live_running_head_blocks_follower_and_completion_unblocks_lane` adds running blockage and completed-head exit/reselection. |
| `e339e69ff4c5a0fa` | invariant:156,165,174,184,193 | `retry_preserves_lane_blocking_until_due_and_terminal_exit`: explicit retry run_at 2100, empty-before-due, original-head singleton/ID and completion exit. Initial claim is reducer fixture preparation; source initial selection assertions 136/145 and installed retry policy are not separately represented here. |
| `d66bd07c7dc9d94e` | invariant:223,232,234,243 | `fifo_state_and_partition_lanes_are_independent`: chosen validate/charge IDs under separate state filters; source singleton patterns are not separately asserted. |
| `be6beed30e05ded4` | invariant:267,276 | Same counterpart: exact first-a/first-b head set under queued filter. Source live-blocking assertion 278 and completion/reselection 299/308 are omitted for this declaration. |
| `2c1a0a952f9bef71` | invariant:336,345 | `fifo_partition_list_selects_only_one_head_per_listed_lane`: exact first-a/first-b set excludes c. Live blockage 347 and later separate c claim 356/365 are omitted. |
| `485560e32e405cf1` | invariant:487,497,498,500 | `expired_running_head_is_reclaimed_before_due_follower_with_limit_one`: explicit expired-head Reclaim singleton/ID, reducer replacement worker and blocked follower. Initial claim 477 is fixture preparation; automatic reclaim scheduling is omitted. |
| `ba618b494b159854` | invariant:536-543 | `not_due_head_blocks_eighty_due_followers`: empty selection from complete lane truth; disposable state-index deletion/setup is omitted and the eighty-follower count is supplemental. |
| `74f5fcdcf4328003` | invariant:568,571,574-580 | `fifo_deep_head_is_selected_with_eighty_earlier_due_followers`: head due 2000, followers due 1000, eighty followers and singleton head with scan budget 1. |
| `9f5d395534240b02` | invariant:616-622 | `fifo_global_due_order_survives_deep_head_lookup`: earlier due 1500 lane head wins against due 2000 head behind eighty due 1000 followers, with two direct head reads and scan budget 1. |
| `bea9ae0f6f1e970c` | invariant:1137,1146 | `mixed_fifo_policy_preserves_due_order_for_parallel_states`: limit-one earlier parallel retry ID when the requested FIFO gate is empty; policy installation and payload return omitted. |

Test namespaces and generated IDs are normalized to fixed binary fixture values;
exact state-entry order is supplied as authoritative u128 metadata. Selection
fixtures reconstruct complete test indexes around reducer outputs; this does
not simulate transactional storage. Several create times are reduced to a
common fixture time while preserving the source's due/order counterexample.
The source native multi-head implementation case `eca2202b0f62f28f` (lane:91)
and physical key-colocation case `2d1bd0cb7e6c0b47` (lane:135) are omitted.

Fourteen other tests are supplemental: thirteen selection admission/index/
continuation/scan bounds cases and
`running_class_preserves_u128_sequence_then_binary_id_order`. They are separate
from these twenty source-shaped counterparts and are not new source case IDs.

## Resolved continuation review and final owned checks

Implemented C-C1 using only explicit group/revision/exact request values; a
reconstructed equivalent snapshot now resumes. No pointer/address check or
standard-library path exemption remains. The adapter must ensure a revision
cannot identify different contents, including after recovery. Implemented C-C2
by validating every consumed parallel row and every lane head against the
borrowed canonical scoped record/metadata. A conflicting later-page record now
rejects without growing cursor history or traversing the canonical map.

The pre-fix two failures are now green. Combined four targets first passed
**30 tests**, saved `/private/tmp/workflow-m1-claim-continuation-green.log`.
The five lifecycle cases and the later mixed/three extra bounds cases were
passing first-run coverage, not claimed behavior-red cycles. Final owned result
is **34 passed, 0 failed, 0 ignored**: 8 selection, 5 lifecycle, 8 primitive,
13 bounds/continuation. Coordinator independently re-read the actual fixes,
ran all 34 and recorded C-C1/C-C2 closed in the coordination note; its saved output
is `/private/tmp/ferricstore2-m1-domain-final-independent.log`.

Final full `wf-domain` run: **80 passed, 0 failed, 0 ignored**, including the 46
retained/retry cases. Strict `clippy --all-targets -- -D warnings` passes with
no suppression. Warnings-denied domain documentation and
`complete_workspace_obeys_policy` pass. Saved actual outputs respectively:

- `/private/tmp/workflow-m1-claim-domain-final.log`
- `/private/tmp/workflow-m1-claim-clippy-final.log`
- `/private/tmp/workflow-m1-claim-doc.log`
- `/private/tmp/workflow-m1-claim-architecture.log`

Clippy initially diagnosed the owned tuple comparison helper and several test
style issues; these were owner-corrected without lint exceptions. Domain,
documentation and architecture targets are isolated beneath `/private/tmp`.
Owned rustfmt and `git diff --check` pass. Runtime physical line counts:
claim 37, index 216, request 140, error 60, result 91, selection 220. Test counts:
selection 311, fifo 344, lane 158, limits 442. All satisfy 400/600 respectively.

The coordinator actually executed the pinned original source subsets: twelve
FIFO primitive/retry-policy cases, fifty-six priority/multi-state/retry section
cases and ten FIFO invariant cases passed (seeds 729398, 734816, 654031). This owner
read the retained source and Rust counterparts; original execution was performed
by the coordinator. Saved source logs use the
`/private/tmp/ferricstore2-m1-source-{fifo-policy,order-retry-sections,fifo-invariants}.log`
names and accompanying copied JUnit artifacts. These are subset executions,
not a full source acceptance run.

## Independent actual peer reviews

### Retry owner

Read actual policy/backoff resolution, explicit jitter values, reducer dispatch,
validation, retry transition, history/no-op planning, both policy test files and
the scenario helper. Compared them with pinned `retry_policy.ex` resolution and
arithmetic, `flow_transition.ex:1075-1185` and
`flow_history_writes.ex:474-509`, plus the actual preserved policy/priority tests.
The four override layers, normalized backoff replacement, inclusive attempts,
jitter spread/modulo/final cap, ownership/no-op ordering, active/terminal
exhaustion and nine typed audit observations agree for the represented domain
contract. Explicit input versus computed deadline admission is source-derived;
checked u64 overflow is an explicit representation boundary. No new concrete
production blocker found. Hash preparation, policy installation, storage/history
wire bytes and transaction effects remain adapter/parity omissions.

Independently re-read **P-R1** after owner correction: the final manual active
state claim now requires `Outcome::Changed`, running state, `payment_failed`
logical state and the `worker-manual` lease. The original ID assertion alone
could previously pass after `NotEligible`; the updated assertion now observes
the source success. Independent actual run of `retry_policy` and
`retry_policy_transitions`: **19 passed, 0 failed, 0 ignored**, including this
case, saved `/private/tmp/workflow-m1-claim-retry-review.log`. P-R1 is closed from
this independent review; no peer file was changed.

### Coordinator architecture guard

Read actual ADR-003, `size.rs`, `source.rs` call and the four size regression
bodies. Physical sizing counts comments/blanks, CRLF and an unterminated final
line; runtime `src/tests.rs` remains capped at 400. Tooling/integration paths use
600 through existing classification, without suppression or generated-source
exemption. Independent architecture library run **16 passed, 0 failed,
0 ignored**, saved `/private/tmp/workflow-m1-claim-size-review.log`. The actual
complete-workspace architecture test is also green, as recorded above. No new
concrete guard finding.

### Parity owner

Read actual registration schema 2, macro-origin validation, runtime accounting,
progress/CLI reporting, restricted AST recovery, reconciliation scripts and the
new adversarial integration/AST tests. The original runtime ID complement and
frozen declaration complement remain distinct; exact raw module/name/tags,
source/use hashes and locations, origin IDs and profile/helper requirements are
checked without asserting execution. Reviewed R-P1 alias closure: rebound or
ambiguous namespaces stay unresolved; literal unrelated grouped aliases do not
permit guessing rebound targets. AST recovery remains parse-only.

Independent actual `elixir scripts/tests/runtime-reconciliation-test.exs`:
**8 tests, 0 failures**, seed 927797, saved
`/private/tmp/workflow-m1-claim-parity-ast-review.log`. Independent
`registration_v2` integration run: **6 passed, 0 failed, 0 ignored**, saved
`/private/tmp/workflow-m1-claim-parity-registration-review.log`. These tests copy
the real retained artifacts into private fixtures. Re-read the actual added
supplemental-root validator/test and `collection-root-review.json`; independently
hashed both added source files against the exact pinned checkout and both match.
Malformed root review hashes reject even for unmatched original cases. No new
concrete blocker found in this reviewed increment. Final artifacts, collection
guide and assertion mapping review await owner publication; none of these
registration checks execute source behavior or establish full parity.

**C-P1 initially open — a preceding use can inject a namespace alias.** After reading the
final guide and actual restricted parser again, constructed and executed this
independent compiled synthetic fixture. `AliasBinder.__using__` injects
`alias OtherRoot, as: Review`; `use Review.First` therefore registers and runs
the OtherRoot body. Actual observed runtime marker is `:second`, while current
`WorkflowMacroOrigins.match` returns an accepted origin with declaration
`first` from `Elixir.Review.First`. The consumer has no direct alias AST node,
so the new direct lexical-alias safeguard does not observe the injected binding.
This is a reproduced wrong source identity, not an unexecuted hypothetical
risk or a claim that a retained pinned origin currently contains this pattern.
Saved the exact note-derived fixture and actual diagnostic output at
`/private/tmp/workflow-m1-claim-macro-injected-alias-repro.exs` and
`/private/tmp/workflow-m1-claim-macro-injected-alias-repro.log`. The diagnostic
exits 0 only when the wrong-source condition is reproduced; this is observed
review evidence, not a passing provenance acceptance test. Owner directly
accepted C-P1 and will publish its pre-fix regression and conservative resolution.

The diagnostic compiles only its own synthetic modules in an isolated Elixir
process and explicitly calls its own generated test function; it never invokes
an original source test/setup. ExUnit wraps that function's return as `:ok`, so
the observed body marker is explicit process state rather than its return value.
This corrected diagnostic actually distinguishes the two compiled bodies.

Reproduction (run with `elixir -r scripts/runtime-macro-origins.exs /dev/stdin`):

```elixir
ExUnit.start(autorun: false)
section = fn namespace, marker ->
  """
  defmodule #{namespace}.First do
    defmacro __using__(_) do
      quote do
        test "same origin" do
          Process.put(:review_origin, #{inspect(marker)})
        end
      end
    end
  end
  """
end
binder = """
defmodule AliasBinder do
  defmacro __using__(_) do
    quote do
      alias OtherRoot, as: Review
    end
  end
end
"""
consumer = """
defmodule MacroAliasConsumer do
  use ExUnit.Case
  use AliasBinder
  use Review.First
end
"""
sources = [{section.("Review", :first), "first.exs"},
  {section.("OtherRoot", :second), "second.exs"},
  {binder, "binder.exs"}, {consumer, "consumer.exs"}]
for {source, path} <- sources, do: Code.compile_string(source, path)
registered = hd(MacroAliasConsumer.__ex_unit__().tests)
MacroAliasConsumer."test same origin"(%{})
raw = %{"source_path" => registered.tags.file,
  "source_line" => registered.tags.line,
  "module" => "Elixir." <> inspect(registered.module),
  "name" => Atom.to_string(registered.name)}
indexes = Enum.map(sources, fn {source, path} ->
  WorkflowMacroOrigins.index(source, path)
end)
declarations = [%{"id" => "first", "source_path" => "first.exs",
  "line" => 4, "declaration" => "same origin"},
  %{"id" => "second", "source_path" => "second.exs",
  "line" => 4, "declaration" => "same origin"}]
matched = WorkflowMacroOrigins.match(raw, indexes, declarations)
IO.inspect(Process.get(:review_origin), label: "actual_runtime_origin")
IO.inspect(matched, label: "recovered_origin")
case {Process.get(:review_origin), matched} do
  {:second, {:ok, %{"source_declaration_id" => "first"}}} ->
    IO.puts("WRONG_SOURCE_LINK_REPRODUCED")
  _ -> System.halt(1)
end
```

Parity owner should add a pre-fix regression and either prove the permitted
preceding macro environment cannot rebind the named namespace or conservatively
leave affected uses unresolved. Do not evaluate arbitrary macros during AST
recovery. Owner regenerates hash-bound origin artifacts after its fix and this
reviewer reruns the same compiled fixture independently; no peer file is edited.

**C-P2 initially open — retry FIFO assertion mapping selects the wrong source outcome.**
Read the final generator/guide/ledger and compared each claim source/Rust pair.
For `e339e69ff4c5a0fa`, generator pair `(165, 0)` links source
`assert {:ok, [retried]} =` (due 2100 singleton success) to Rust direct assertion0,
`assert_eq!(selected(... 2_050 ...), SelectedIds::new())` (empty before due).
These are different observations. Exact pinned source line156 is the empty
before-due assertion, line165 is singleton success and line174 is its original
ID. This is an actual published semantic mapping error despite structurally
valid source/hash/macro anchors; no Rust runtime defect is asserted.

Owner should map `(156,0)` for empty-before-due and `(165,1)`/`(174,1)` for the
actual exact singleton/ID vector, retaining `(184,2)`/`(193,2)` for the final
second vector. Regenerate the ledger and counts through the owned generator.
The other inspected claim pairs and explicit limitation scopes agree with this
owner's table, including the intentionally omitted cardinality/wire/persistence
observations. No peer artifact was edited.
Independently executed the pre-correction `check-progress`: exit 0 with 37
source declarations, 38 counterparts and 123 anchors, saved
`/private/tmp/workflow-m1-claim-parity-progress-pre-cp2-review.log`. This confirms
structural/source hash checks pass while that semantic pair is wrong; it is
not a claimed behavior-red test. Owner correction and semantic re-review are
required in addition to another structurally green validator run.

**C-P1 independently closed after the actual v2 correction.** Read the final
parser, all fifteen AST regression bodies, Rust origin validation, seven
registration regressions, regenerated artifacts and current collection guide.
Ordered consumer and section-definition module prefixes plus inherited file
prefixes now reject unknown macro effects. Neither framework names nor loader
names receive a proof exception. All ninety section reviews remain explicitly
`environment_decision=unproved`; the Rust regression rejects restoring a former
origin even with consistent identities, complements and current hashes. No
arbitrary expansion or original test/setup execution was added.

Independently reran the unchanged compiled fixture above: actual marker is
`:second` and recovered origin is now `{:unresolved, ...}`. Its original
wrong-origin diagnostic exits **1**, as expected because it exits zero only on
the defect. A separate review check verified that output and expected status,
exited **0**, and saved
`/private/tmp/workflow-m1-claim-macro-injected-alias-fixed-review.log`. Parent
and optimizer independently reran the unchanged prior-use fixture and the three
compiled require/call, inherited-consumer and source-definition variants; their
executions are separate from this owner's single compiled-fixture run.

Owner supplied real pre-fix regression evidence for the use, inherited and
section-definition failures; independently re-reviewed their actual fixes.
Final independent AST execution: **15 tests, 0 failures**, seed 539240, saved
`/private/tmp/workflow-m1-claim-parity-ast-v2-review.log`. Final independent Rust
parity package execution: **45 passed, 0 failed, 0 ignored**, including all seven
registration regressions, saved
`/private/tmp/workflow-m1-claim-parity-v2-review.log`.

**C-P2 independently closed after the actual mapping correction.** Re-read the
complete source retry scenario and Rust function, the corrected generator pairs
and stored assertion texts. Current source/Rust lines are `156 -> 201`
(empty at 2050), `165/174 -> 202` (exact original singleton/ID at 2100), and
`184/193 -> 219` (exact follower singleton/ID after completion at 2201). The
retained omissions still agree with this note's source table. Stored-pair/count
verification saved
`/private/tmp/workflow-m1-claim-parity-cp2-final-review.log`; semantic equivalence
comes from the actual source/fixture review, not that structural comparison.

Independent final `check-progress` exits **0** with **37 source declarations,
38 counterpart records and 124 assertion links**, saved
`/private/tmp/workflow-m1-claim-parity-progress-v2-review.log`. Current runtime
accounting is **5,903 raw registered cases, 4,222 exact links, zero admitted
macro origins, 1,681 unmatched runtime identities and 1,655 unresolved frozen
declarations**. Earlier literal-recovery counts above belong to the pre-fix
saved tree and are superseded. The lower admitted counts preserve every raw
case and unknown complement; they do not exclude or pass unknown behavior.
The guide/progress prose accurately distinguishes identity, assertion links
and execution. C-P1/C-P2 are closed with no remaining concrete parity-review
blocker; full workflow parity remains incomplete.

### Optimization reviewer

Read the reviewer's actual published contract and initial immutable-byte,
partition-decimal-allocation and public-command API observations. Independently
confirmed current private values expose borrowed slices while command fields
remain owned vectors. Notified the reviewer that canonical full-value comparison
is intentional: ID/version alone is not an accepted coherence certificate, and
process-address acceptance would reopen C-C1. Row/head/output budgets bound
counts; admitted byte comparison cost remains proportional to key/payload bytes.
The domain's composed-key/payload-size admission is an explicit adapter omission.

Reviewer directly agreed to preserve full comparison semantics and measure equal
distinct records/payload scaling. Box handle-size reduction alone does not prove
cheaper allocation or deep cloning; constructor conversion can shrink/reallocate
and must be measured separately. No representation change has been implemented
by this owner.

After actual concrete findings were published, read the complete ranked O1/O2/O3
note, raw first-run probe log and actual approved tooling probe source, alongside
current `identity.rs` and `lease.rs`. The probe keeps record construction and
distinct cloning outside selector timing, observes successful selections, uses
five local samples and `black_box`, compares the decimal shadow's actual bytes
at binary/length boundaries, and separates conversion from deep cloning. Its
numbers are reviewer-run focused measurements under concurrent development,
not independently reproduced allocator counts or production throughput claims.

**O1 review:** the original encoder really creates two decimal strings per
component. Stack digit counting/emission preserves the checked encoded-length
fold, fallible exact reserve, growable Vec, prefix and opaque component bytes.
Twenty decimal bytes cover the supported 32/64-bit usize range. The shadow's
boundary checks include zero, digit transitions, 65535 and usize max. No concrete
proposal blocker; accepted implementation still requires actual byte/boundary
checks and independent source review.

**O2 review:** `RecordId`/`LeaseToken` are immutable private handles; Vec-input
constructors, borrowed accessors and byte equality/order/hash semantics can be
preserved by a private boxed slice after validation. The actual measured handle
decreases 24 to 16 bytes on this target. Shrinking spare capacity can move/allocate
and conversion timings are worse in those measured cases; larger boxed clones
showed no speed benefit. Keeping these costs explicit makes the narrow storage
proposal reviewable. Retaining `PartitionKey`'s Vec avoids adding an infallible
shrink after its explicit allocation-error boundary. Broad workflow boxing
remains deferred because clone/replacement paths are unmeasured and current
clone measurements do not support it. No command/public Vec or canonical
equality change is justified by this evidence.

Coordinator accepted O1/O2 and owns the actual `identity.rs`/`lease.rs` edits.
This owner will independently read/test those changes after publication; no
claim API/source change is required. Reviewer independently executed all 34
claim cases green and read the actual C-C1/C-C2 protections, saved
`/private/tmp/ferricstore2-m1-optimizations-claim-independent.log`. Its deferred
canonical-ID-row API option preserves full authority by construction and is
explicitly future work rather than a weakened equality check now.

**Accepted implementation independently re-reviewed:** read root's actual
`identity.rs`/`lease.rs` after publication. Decimal length counts zero as one
digit; emission uses `size_of::<usize>() * 3` stack bytes, which covers every
usize value because each byte contributes fewer than three decimal digits.
Digits emit in forward ASCII order after the unchanged checked sizing/fallible
reserve. Both wrappers validate the original input before converting the valid
Vec to a boxed slice. Binary bytes, input constructors, borrow accessors and
value traits are retained; public command/worker and PartitionKey vectors stay
unchanged. No new concrete implementation blocker found.

Independent actual post-change `wf-types` + `wf-domain` execution:
**99 passed, 0 failed, 0 ignored** (19 types + 80 domain), including partition
decimal/binary/collision boundaries, identity bounds/order/clone, all 34 claim
tests and ownership/retry cases. Saved
`/private/tmp/workflow-m1-claim-o1-o2-review.log`. The complete workspace
architecture test also independently passes after the actual changes, saved
`/private/tmp/workflow-m1-claim-o1-o2-architecture-review.log`. These observations
confirm retained semantics/architecture rather than a production speed claim;
post-change cost measurements belong to the optimizer's actual repeated probe.
Strict all-target type/domain Clippy also passes after these changes, saved
`/private/tmp/workflow-m1-claim-o1-o2-clippy-review.log`, without suppression.

## Owned files and handoff boundaries

Owned production paths are `crates/wf-domain/src/claim.rs` and
`crates/wf-domain/src/claim/{index,request,error,result,selection}.rs`. Owned
tests are `crates/wf-domain/tests/claim_selection{,_fifo,_lane,_limits}.rs`.
This file is the only owner-written documentation. No peer/root configuration,
dependency, preserved snapshot, commit or push was changed by this owner.
Existing `Claim` and reducer public contracts remain stable.

**Coordinator resource/corruption clarification accepted.** Updated only the
owned `SelectionError` documentation: host `AllocationFailed` may differ between
replicas, so future replicated apply must recover/retry rather than persist it
as a deterministic workflow business rejection or skip a committed decision.
Invalid authoritative/canonical index contents require adapter corruption/
failure handling as well. `InvalidPriority` has request and authoritative-row
contexts; an adapter cannot classify that variant without its validation context.
No behavior, type shape, dependency, exception or test was changed. Error source
is now 60 physical lines; owned rustfmt check and warnings-denied domain docs
pass, saved `/private/tmp/workflow-m1-claim-error-contract-doc.log`.

Authoritative adapter completeness/placement/revision identity, policy-mode
installation, automatic reclaim ratios/timeouts, FIFO sequence assignment,
transactional index mutation, durable batch outcomes and full source profile/
fixture execution remain unimplemented. The pure claim slice is ready for
coordinator integration; M1 and full parity remain incomplete.
