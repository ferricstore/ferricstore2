# Elixir workflow bug fixes

Status: A-EB2's exact regression construction is complete under root's separate
assignment. No evaluation/VM, behavior RED/GREEN, source patch or confirmed fix
yet. The separate inline supervisor below is a source-only runtime proposal.

Architecture decision A-EB1 and the separately actual-read coordinator assignment
approve this note and one isolated source checkout. The checkout is
`/Users/yoavgea/repos/ferricstore2-elixir-bugfix`, branch
`codex/workflow-elixir-bugfixes`, initially clean at exact
`66586c596e271408d9184af32eb31cd7218c126e`. Root verified HEAD and clean tracked/
untracked status after creation. The original dirty `/Users/yoavgea/repos/ferricstore`
and the clean pinned `/Users/yoavgea/repos/ferricstore2-elixir-baseline` were not
edited. The latter remains the immutable Rust compatibility input.

The dedicated `elixir_bugfix` owner uses GPT-6.1-sol/max. Its initial write set
is ONLY this note. It reads isolated workflow source and relevant original dirty
diffs, records exact candidates and proposed failing controls, and communicates
with architecture, domain, performance and root through actual note reads and
direct relays. It must obtain the concrete architecture decision and separate
actual-read implementation assignment before each source/test/artifact/API/
resource batch. No Elixir code, helper, test, module, log, VM, build cache or
runtime profile is assigned by the isolation preparation. No commit/push.

The initial candidate is nullable worker-index handling. Source inspection finds
that `flow_claim_indexes.ex:619` uses `Map.get(next, :lease_owner, "")`, which
preserves a present nil, while `flow/keys.ex:598` concatenates the worker into a
binary key. Child-group preparation and terminal exhaustion can preserve a
physical `running` label while clearing lease/next. This is an inspected candidate,
not evidence of an executed failure or successful persisted source shape. The
owner must establish the concrete trigger and correct behavior before a fix.
Rust's deterministic representation decisions cannot silently alter the pinned
source/reference manifests, mappings or progress ledger.

Record source evidence, actual RED/GREEN, exact changed paths, bounded resource
scope, reciprocal review and remaining profile limits below as work proceeds.

## EB2 source-only diagnosis and proposed finite reproduction/patch batch

The owner actually read this complete initial note, Rust AGENTS.md, README.md,
docs/architecture.md, docs/workflow-test-parity.md, complete A-EB1 and root's
separate isolation assignment. Read-only checks confirm the isolated checkout
is clean at the assigned exact commit/branch. No AGENTS.md or CLAUDE.md was found
in this Elixir checkout, apps/ferricstore, or the checked ancestor paths.
docs/testing.md requires one full FerricStore test command per worktree at a
time. No source, test, helper, dependency/cache, log or runtime artifact was
created during this investigation. This existing note is the only write.

### Exact source causal chain, with an earlier spawn failure seam

All locations below refer to the isolated pinned66586c5 source.

1. flow/child_policy.ex:53–60 accepts every nonempty binary success/failure
   target, including "running". flow/mutation_attrs.ex:287–339 accepts any
   optional nonempty wait_state; FlowCreate:399–408 requires a nonempty
   wait_state for all/any, with no reservation of "running".
2. FlowCreate:440–474 prepares the parent's selected wait/success state and
   clears next_run_at_ms, ttl_ms, lease_owner, lease_token and deadline to0.
   Both wait all/any with wait_state "running", and wait none with success
   "running", therefore prepare the exact physical running/unowned shape.
   They do not mint a replacement lease.
3. The earliest SPAWN seam is FlowCreate:89 (cross-shard path:136), before
   flow_apply_parent_update. flow_validate_record_keys in
   FlowHistoryWrites:604–632 calls flow_validate_running_index_keys at:623.
   Its running clause at:683–688 concatenates Map.get(record,:lease_owner,"")
   through Flow.Keys.worker_index_key:598–600. Present nil is retained.
   A runtime exception at this seam is predicted, not observed. No successful
   source persistence of this shape has been established.
4. Separately, FlowTerminal:708–844 prepares a resolved parent's arbitrary
   success/failure target and clears the same lease fields. The terminal
   parent-update paths:388–423 and:606–655 call FlowCreate:563–575
   flow_apply_parent_update without the spawn validator. That calls
   FlowClaimIndexes:650–659, then FlowClaimStateWrites:45–87. Its actual
   running-index reduce at:78 again constructs a worker key from present nil.
   This is a second predicted public-command failure seam.
5. The initially reported FlowClaimIndexes:619–634 helper is used by grouped
   claim index planning at:136–137; it is not the first spawn seam. The old
   helper:607–617, single/bulk transition deletes:1042–1105, generic grouped
   writes in FlowClaimStateWrites:833–869, and FlowHistoryReads:565–579 share
   the nullable-owner assumption. Once unowned running parents can persist,
   their follow-up transition/cancel and generic reindexing need the same rule.
6. LMDB.active_projection_entries/1,2 at flow/lmdb.ex:1530–1564 is an existing
   public pure source seam. Its running helper:1621–1640 builds the same nil
   worker key for a record with deadline0. Full compilation needs only the
   real ValueLocator module for its top-level max_encoded_bytes call; the
   projection path itself calls no product NIF, storage, setup or service.

Direct create/transition admission already rejects entering "running"
(mutation_attrs_test.exs:64–100). That does not establish a child-policy error
contract: child admission separately accepts these bytes. EB2 proposes preserving
that accepted child-policy state and all prepared lease/scheduling values.
It does not infer that direct-create restrictions authorize adding a new child
validation error. Architecture must explicitly decide this interpretation.

### Read-only original dirty-diff check

The original dirty FlowTerminal already changes flow_clear_parent_if_resolved
to take now_ms and schedule nonterminal exhaust targets immediately. Its existing
dirty child-group section adds six related tests and one helper for due successor,
wait-any, cross-shard and terminal controls. This is a separate existing fix;
EB2 will not port, duplicate, weaken or edit it. Our selected tests preserve the
pinned next_run_at_ms=nil preparation and do not assert immediate reclaimability.
No changed lines there guard nullable ownership keys.

Read-only targeted diffs for the proposed FlowClaimIndexes, FlowClaimStateWrites,
FlowHistoryWrites, flow/lmdb.ex and flow/lmdb_test.exs paths are empty.
The original dirty FlowHistoryReads change only rewrites restored cancellation
error_ref; it does not change its running-index writer. Existing dirty
mutation_attrs and LMDBIndexReadTest changes are unrelated cancellation-reason
and prefix-merge work. No original dirty change is copied into the isolation.

### Requested behavior decision and smallest coherent production write set

Propose that running ownership indexes exist only when lease_owner is a nonempty
binary. An absent/nil/empty owner yields no worker OR inflight entry; it still
retains its physical state index and any independently scheduled due/timeout
entries. This is an explicit change to derived-index classification for unowned
running child-policy records, requiring architecture review. The record itself,
wait/exhaust target, run_state, lease/fence, time and history preparation remain
unchanged. No nil-to-empty key, synthetic lease, invented scheduling deadline,
wall-clock expiry filter or new child-policy admission restriction is proposed.
Owned expired leases must still appear in inflight/worker indexes so reclaim
can discover them. Existing owner bytes and index order/scores remain unchanged.

Historical empty-worker/inflight0 cleanup is NOT proved or included. The old
missing/empty-owner clauses could construct those keys, while present nil
predicts an exception. Skipping old deletes under the proposed classification
does not remove any already persisted historical rows. Public reachability,
successful persistence, compatibility deletion, migration and rebuild cleanup
need a separate reachable fixture, exact decision and assignment. EB2 certifies
none of those claims.

The proposed production edits are ONLY these five existing files, all under
apps/ferricstore/lib/ferricstore in the bugfix checkout:

| Existing path | Exact existing seam and proposed edit |
|---|---|
| raft/state_machine/sections/flow_claim_indexes.ex | Guard existing old/new running entry clauses with lease_owner:worker and is_binary(worker) and worker!="" ; use worker directly. Reuse old helper for the single transition delete, and add the same owner guard before the bulk cached delete branch. Preserve terminal reprojection. |
| raft/state_machine/sections/flow_claim_state_writes.ex | Replace the duplicated running entry construction inside flow_claim_put_running_indexes/2 and flow_index_grouped_entries/1 with existing flow_claim_new_running_index_puts/2, so both use that guarded implementation. Existing single/bulk filtering may retain physical running selection; the reduce then emits no ownership entries for an unowned record. |
| raft/state_machine/sections/flow_history_writes.ex | Guard existing flow_validate_running_index_keys/3 running clause with a nonempty binary lease_owner and pass that owner to FlowKeys.worker_index_key. Fallback remains :ok for unowned records. |
| raft/state_machine/sections/flow_history_reads.ex | Guard existing flow_running_index_put/2 running clause with a nonempty binary lease_owner and use it directly; fallback remains :ok. |
| flow/lmdb.ex | Add lease_owner:worker plus the nonempty-binary guard to existing maybe_add_running_active_entries/3; retain integer deadline normalization, entry order and fallback. |

No product module/type/struct/helper/public API/dependency is added.
The shared helper already belongs to the same StateMachine consumer:
state_machine.ex:276–279 uses the index, state-write and history sections.
No source in FlowCreate, FlowTerminal, ChildPolicy, MutationAttrs or Keys changes.
Frozen baseline, Rust snapshots/manifests/mappings/ledgers remain unchanged.

### Proposed isolated actual-source control, full body for review

Request exactly one new helper/probe file in the isolated checkout:
tools/flow-worker-index-regression.exs. It defines no new module or function,
uses no Mix/ExUnit/project setup, and compiles the COMPLETE three named actual
source files into a fresh supported-core VM. It neither extracts nor mirrors a
source function, synthesizes a StateMachine consumer, stubs a product dependency,
nor calls a product NIF. Unrelated missing remote-module compiler warnings remain
visible and are not warnings-as-errors acceptance. Only this pure projection
control is certified by its result. It cannot prove spawn/terminal persistence.

The exact proposed file body is:

~~~elixir
root = Path.expand("..", __DIR__)

sources = [
  {"apps/ferricstore/lib/ferricstore/flow/lmdb/value_locator.ex",
   Ferricstore.Flow.LMDB.ValueLocator},
  {"apps/ferricstore/lib/ferricstore/flow/keys.ex", Ferricstore.Flow.Keys},
  {"apps/ferricstore/lib/ferricstore/flow/lmdb.ex", Ferricstore.Flow.LMDB}
]

for {relative, expected_module} <- sources do
  path = Path.join(root, relative)
  true = File.stat!(path).size <= 65_536
  compiled = Code.compile_file(path)
  true = Enum.map(compiled, &elem(&1, 0)) == [expected_module]
end

base = %{
  id: "parent",
  type: "parent",
  state: "running",
  partition_key: "__flow_auto__:1",
  updated_at_ms: 42,
  next_run_at_ms: nil,
  lease_owner: nil,
  lease_token: nil,
  lease_deadline_ms: 0
}

state_entry =
  {Ferricstore.Flow.Keys.state_index_key("parent", "running", "__flow_auto__:1"),
   "parent", 42}

cases = [
  {"unowned child-policy running", base, [state_entry]},
  {"owned running", Map.merge(base, %{
     lease_owner: "worker-a", lease_token: "lease-a", lease_deadline_ms: 1_000
   }), [
     {Ferricstore.Flow.Keys.worker_index_key("worker-a", "__flow_auto__:1"),
      "parent", 1_000},
     {Ferricstore.Flow.Keys.inflight_index_key("parent", "__flow_auto__:1"),
      "parent", 1_000},
     state_entry
   ]},
  {"owned earlier-deadline running", Map.merge(base, %{
     lease_owner: "worker-a", lease_token: "lease-a", lease_deadline_ms: 1
   }), [
     {Ferricstore.Flow.Keys.worker_index_key("worker-a", "__flow_auto__:1"),
      "parent", 1},
     {Ferricstore.Flow.Keys.inflight_index_key("parent", "__flow_auto__:1"),
      "parent", 1},
     state_entry
   ]}
]

results =
  for {name, record, expected} <- cases do
    try do
      actual = Ferricstore.Flow.LMDB.active_projection_entries(record, due_any?: false)
      if actual == expected do
        IO.puts("PASS " <> name)
        :pass
      else
        IO.puts(:stderr, "FAIL " <> name <> ": unexpected entries")
        :fail
      end
    rescue
      error ->
        formatted = Exception.format(:error, error, __STACKTRACE__)
        if byte_size(formatted) <= 16_384 do
          IO.puts(:stderr, "FAIL " <> name <> ":\n" <> formatted)
          :fail
        else
          IO.puts(:stderr, "INFRA: worker-index exception stack exceeds 16KiB")
          System.halt(2)
        end
    end
  end

supplemental_results =
  for {owner, unowned} <- [
        {"nil", base},
        {"missing", Map.delete(base, :lease_owner)},
        {"empty", Map.put(base, :lease_owner, "")}
      ],
      due_any? <- [false, true],
      scheduled? <- [false, true] do
    name = "unowned " <> owner <> " due_any=" <> to_string(due_any?) <>
           " scheduled=" <> to_string(scheduled?)
    record =
      if scheduled?,
        do: Map.merge(unowned, %{
          next_run_at_ms: 77, priority: 2, created_at_ms: 10, max_active_ms: 100
        }),
        else: unowned
    expected =
      case {scheduled?, due_any?} do
        {false, _} ->
          [state_entry]
        {true, false} ->
          [
            {Ferricstore.Flow.Keys.active_timeout_index_key(),
             Ferricstore.Flow.Keys.state_key("parent", "__flow_auto__:1"), 110},
            {Ferricstore.Flow.Keys.due_key("parent", "running", 2, "__flow_auto__:1"),
             "parent", 77},
            state_entry
          ]
        {true, true} ->
          [
            {Ferricstore.Flow.Keys.active_timeout_index_key(),
             Ferricstore.Flow.Keys.state_key("parent", "__flow_auto__:1"), 110},
            {Ferricstore.Flow.Keys.due_any_key("parent", 2, "__flow_auto__:1"),
             "parent", 77},
            {Ferricstore.Flow.Keys.due_key("parent", "running", 2, "__flow_auto__:1"),
             "parent", 77},
            state_entry
          ]
      end
    try do
      actual = Ferricstore.Flow.LMDB.active_projection_entries(record, due_any?: due_any?)
      if actual == expected do
        IO.puts("PASS " <> name)
        :pass
      else
        IO.puts(:stderr, "FAIL " <> name <> ": unexpected entries")
        :fail
      end
    rescue
      error ->
        formatted = Exception.format(:error, error, __STACKTRACE__)
        if byte_size(formatted) <= 16_384 do
          IO.puts(:stderr, "FAIL " <> name <> ":\n" <> formatted)
          :fail
        else
          IO.puts(:stderr, "INFRA: worker-index exception stack exceeds 16KiB")
          System.halt(2)
        end
    end
  end

failures = Enum.count(results ++ supplemental_results, &(&1 == :fail))
IO.puts("cases=15 failures=" <> Integer.to_string(failures))
System.halt(if failures == 0, do: 0, else: 1)
~~~

Expected preimplementation control: the original nil-owner projection comparison
fails with the real source exception, while both original owned controls pass.
The twelve supplemental nil/missing/empty × due-any false/true × unscheduled/
scheduled comparisons fail; expected summary is cases=15 failures=13. The
original three records/expected rows are unchanged. Present-nil controls retain
the actual exception stack; missing/empty controls expose unwanted ownership
rows by exact list mismatch. This is a prediction, not an observed RED.
A compile/load/runtime-origin failure, warning
overflow or resource refusal is infrastructure failure, never behavior RED.
Expected postpatch control: all fifteen exact comparisons pass, without changing
the control file. Source bytes/hashes and helper bytes must be bound before each
invocation; compiling a proposed function instead of these real files is invalid.

### Four proposed existing-module regression declarations

Request two new tagged tests in existing
apps/ferricstore/test/ferricstore/flow/lmdb_test.exs, within its current
Ferricstore.Flow.LMDBUnitTest module. No new test file/module/helper. Bodies:

~~~elixir
@tag :worker_index_regression
test "active projection omits ownership entries for unowned running records" do
  record = %{
    id: "parent", type: "parent", state: "running",
    partition_key: "__flow_auto__:1", updated_at_ms: 42,
    next_run_at_ms: nil, lease_owner: nil, lease_token: nil, lease_deadline_ms: 0
  }

  expected = [
    {Ferricstore.Flow.Keys.state_index_key("parent", "running", "__flow_auto__:1"),
     "parent", 42}
  ]

  for unowned <- [
        record,
        Map.delete(record, :lease_owner),
        Map.put(record, :lease_owner, "")
      ] do
    assert LMDB.active_projection_entries(unowned, due_any?: false) == expected
    assert LMDB.active_projection_entries(unowned, due_any?: true) == expected

    scheduled = Map.merge(unowned, %{
      next_run_at_ms: 77, priority: 2, created_at_ms: 10, max_active_ms: 100
    })

    assert LMDB.active_projection_entries(scheduled, due_any?: false) == [
             {Ferricstore.Flow.Keys.active_timeout_index_key(),
              Ferricstore.Flow.Keys.state_key("parent", "__flow_auto__:1"), 110},
             {Ferricstore.Flow.Keys.due_key("parent", "running", 2, "__flow_auto__:1"),
              "parent", 77},
             {Ferricstore.Flow.Keys.state_index_key("parent", "running", "__flow_auto__:1"),
              "parent", 42}
           ]

    assert LMDB.active_projection_entries(scheduled, due_any?: true) == [
             {Ferricstore.Flow.Keys.active_timeout_index_key(),
              Ferricstore.Flow.Keys.state_key("parent", "__flow_auto__:1"), 110},
             {Ferricstore.Flow.Keys.due_any_key("parent", 2, "__flow_auto__:1"),
              "parent", 77},
             {Ferricstore.Flow.Keys.due_key("parent", "running", 2, "__flow_auto__:1"),
              "parent", 77},
             {Ferricstore.Flow.Keys.state_index_key("parent", "running", "__flow_auto__:1"),
              "parent", 42}
           ]
  end
end

@tag :worker_index_regression
test "active projection retains owned running entries regardless of deadline age" do
  for deadline <- [1, 1_000] do
    record = %{
      id: "parent", type: "parent", state: "running",
      partition_key: "__flow_auto__:1", updated_at_ms: 42,
      next_run_at_ms: nil, lease_owner: "worker-a",
      lease_token: "lease-a", lease_deadline_ms: deadline
    }

    assert LMDB.active_projection_entries(record, due_any?: false) == [
             {Ferricstore.Flow.Keys.worker_index_key("worker-a", "__flow_auto__:1"),
              "parent", deadline},
             {Ferricstore.Flow.Keys.inflight_index_key("parent", "__flow_auto__:1"),
              "parent", deadline},
             {Ferricstore.Flow.Keys.state_index_key("parent", "running", "__flow_auto__:1"),
              "parent", 42}
           ]
  end
end
~~~

Request two new tagged tests in the EXISTING quote of
apps/ferricstore/test/ferricstore/flow_test/sections/flow_spawn_children_rejects_missing_wait_state_waiting_children.exs,
which is already required/used by existing Ferricstore.FlowTest.
No new section/module/registration/helper. Reuse only existing uid,
create_claimed_flow, create_claimed_flow_child, flow_spawn_children_and_get,
flow_create_and_get, flow_complete_and_get, flow_fail_and_get,
flow_cancel_and_get and flow_transition_many_and_get. Ordinary local records,
lists and tuples own the fixtures. Proposed exact bodies:

~~~elixir
@tag :worker_index_regression
test "child wait policies preserve running targets without ownership indexes" do
  for {wait, wait_state} <- [{:all, "running"}, {:any, "running"}, {:none, nil}] do
    parent = uid("flow-unowned-wait-parent")
    child = uid("flow-unowned-wait-child")
    partition = uid("flow-unowned-wait-partition")
    type = uid("flow-unowned-wait-type")
    worker = "parent-worker"
    claimed = create_claimed_flow(parent, partition, type, worker)
    ctx = FerricStore.Instance.get(:default)
    inflight_key = Ferricstore.Flow.Keys.inflight_index_key(type, partition)
    worker_key = Ferricstore.Flow.Keys.worker_index_key(worker, partition)
    shard = Ferricstore.Store.Router.shard_for(ctx, inflight_key)
    {index, lookup} = Ferricstore.Flow.OrderedIndex.table_names(ctx.name, shard)
    native = Ferricstore.Flow.NativeOrderedIndex.get(index, lookup)
    assert {:ok, _} = Ferricstore.Flow.NativeOrderedIndex.score_of(native, inflight_key, parent)
    assert {:ok, _} = Ferricstore.Flow.NativeOrderedIndex.score_of(native, worker_key, parent)

    assert {:ok, waiting} =
             flow_spawn_children_and_get(parent, [%{id: child, type: "child"}],
               group_id: "unowned-running",
               wait: wait,
               wait_state: wait_state,
               on_child_failed: :fail_parent,
               on_parent_closed: :abandon_children,
               exhaust_to: %{success: "running", failure: "failed"},
               partition_key: partition,
               from_state: "running",
               lease_token: claimed.lease_token,
               fencing_token: claimed.fencing_token,
               now_ms: 1_010
             )

    assert waiting.state == "running"
    assert waiting.lease_owner == nil
    assert waiting.lease_token == nil
    assert waiting.lease_deadline_ms == 0
    assert waiting.next_run_at_ms == nil
    assert waiting.fencing_token == claimed.fencing_token
    assert waiting.child_groups["unowned-running"]["resolved"] ==
             if(wait == :none, do: "success", else: nil)
    assert {:ok, created_child} = FerricStore.flow_get(child, partition_key: partition)
    assert created_child.id == child
    assert :miss = Ferricstore.Flow.NativeOrderedIndex.score_of(native, inflight_key, parent)
    assert :miss = Ferricstore.Flow.NativeOrderedIndex.score_of(native, worker_key, parent)

    assert {:ok, cancelled} =
             flow_cancel_and_get(parent,
               partition_key: partition,
               fencing_token: waiting.fencing_token,
               now_ms: 1_020
             )
    assert cancelled.state == "cancelled"
  end
end

@tag :worker_index_regression
test "child exhaustion preserves running targets and supports a later batch transition" do
  partition = uid("flow-unowned-exhaust-partition")
  type = uid("flow-unowned-exhaust-type")
  ctx = FerricStore.Instance.get(:default)
  inflight_key = Ferricstore.Flow.Keys.inflight_index_key(type, partition)
  shard = Ferricstore.Store.Router.shard_for(ctx, inflight_key)
  {index, lookup} = Ferricstore.Flow.OrderedIndex.table_names(ctx.name, shard)
  native = Ferricstore.Flow.NativeOrderedIndex.get(index, lookup)

  parents =
    for outcome <- [:complete, :fail] do
      parent = uid("flow-unowned-exhaust-parent")
      child = uid("flow-unowned-exhaust-child")
      assert {:ok, created} =
               flow_create_and_get(parent,
                 type: type, state: "dispatch", partition_key: partition, now_ms: 1_000
               )
      assert {:ok, _waiting} =
               flow_spawn_children_and_get(parent,
                 [%{id: child, type: "child", run_at_ms: 1_000}],
                 group_id: "unowned-exhaust",
                 wait: :all,
                 wait_state: "waiting_children",
                 on_child_failed: :fail_parent,
                 on_parent_closed: :abandon_children,
                 exhaust_to: %{success: "running", failure: "running"},
                 partition_key: partition,
                 from_state: "dispatch",
                 fencing_token: created.fencing_token,
                 now_ms: 1_010
               )
      claimed_child = create_claimed_flow_child(child, partition, "child-worker")
      result =
        case outcome do
          :complete ->
            flow_complete_and_get(child, claimed_child.lease_token,
              partition_key: partition, fencing_token: claimed_child.fencing_token,
              now_ms: 1_020
            )
          :fail ->
            flow_fail_and_get(child, claimed_child.lease_token,
              partition_key: partition, fencing_token: claimed_child.fencing_token,
              error: "child failed", now_ms: 1_020
            )
        end
      assert {:ok, terminal_child} = result
      assert terminal_child.state == if(outcome == :complete, do: "completed", else: "failed")
      assert {:ok, resolved} = FerricStore.flow_get(parent, partition_key: partition)
      assert resolved.state == "running"
      assert resolved.lease_owner == nil
      assert resolved.lease_token == nil
      assert resolved.lease_deadline_ms == 0
      assert resolved.next_run_at_ms == nil
      assert resolved.child_groups["unowned-exhaust"]["resolved"] ==
               if(outcome == :complete, do: "success", else: "failure")
      assert :miss = Ferricstore.Flow.NativeOrderedIndex.score_of(native, inflight_key, parent)
      resolved
    end

  items = Enum.map(parents, &%{id: &1.id, fencing_token: &1.fencing_token})
  assert {:ok, transitioned} =
           flow_transition_many_and_get(partition, "running", "after_children", items,
             now_ms: 1_030
           )
  assert Enum.map(transitioned, & &1.id) == Enum.map(parents, & &1.id)
  assert Enum.all?(transitioned, &(&1.state == "after_children"))
end
~~~

These are four new declarations with five workflow fixture variants, plus the
standalone fifteen comparisons. They are new regression controls, not additional
executed original source identities or Rust parity credit. Full body API/syntax/
resource review remains pending; bodies have not been created or compiled.

### RED/GREEN order, dependencies, logs and runtime gates

Requested ownership: elixir_bugfix alone edits the five listed existing source
paths, two listed existing test paths, and the single named probe in the isolated
checkout, after recorded architecture approval and root's separate actual-read
assignment. This note remains its sole current write set. No commit/push.

Proposed order: create the unchanged probe and four tests first; obtain actual
standalone behavior RED; obtain actual public workflow behavior RED in the
supported original profile; only then patch production code and repeat both
commands for GREEN. No generic live StateMachine fix is certified by the
projection-only failure. If original prerequisites are unavailable, keep the
live patch pending and publish that limitation. A pure LMDB-only smaller patch
after its real source RED could be separately assigned, without claiming the
workflow bug fixed.

Standalone command from the isolated checkout, through root's approved resolved
Elixir1.20.4/OTP29 launcher and clean supported-core environment:

~~~text
elixir --erl "+S 2:2 +A 2" tools/flow-worker-index-regression.exs
~~~

No Mix, dependency acquisition/copy, product code path, config/test.exs,
test_helper, helper setup, service application or NIF call is needed by this
command. Actual compiled sources total92,599 bytes. The proposed helper is
bounded to8KiB; total file input cap128KiB, each source cap64KiB. The fixed auto
partition avoids invoking crypto hashing in this control. Source-origin/hash
binding, fresh VM, two schedulers, two async threads, wall deadline10s,
owned-group TERM2s/KILL2s cleanup, max64KiB each stdout/stderr and max2MiB log are
requested. Compiler warnings stay visible within those stream bounds; overflow
fails the attempt. No project beam output/cache is written by Code.compile_file.
No fresh output family, collector or tracer is implied.

Exact proposed standalone logs are ONLY:
/private/tmp/ferricstore2-elixir-worker-index-red.log and
/private/tmp/ferricstore2-elixir-worker-index-green.log.
Root's installed Elixir1.20.4/OTP29 supported-core prerequisite is now accepted
in coordination:2606–2641, PID46923 with exact proofs and unchanged1848 inputs.
The owner actually read that acceptance record; it is core evidence only.
Initial probe construction approval does not authorize a VM; exact helper bytes,
resolved launcher/origins and source hashes need the subsequent explicit
runtime decision and root assignment. Elixir1.19.5/OTP28 is below the source
profile and cannot substitute for this acceptance gate.

Proposed actual-source ExUnit command, serially after supported original runtime
and dependency/NIF/profile preparation receive their own exact approval:

~~~text
mix test apps/ferricstore/test/ferricstore/flow_test.exs apps/ferricstore/test/ferricstore/flow/lmdb_test.exs --only worker_index_regression --seed 0 --max-cases 1
~~~

Require actual matched four declarations and successful original setup. Missing
deps, unavailable NIFs, excluded/no-match tests, setup/load exceptions, process
crashes without the target source trace, and compiler failures are infrastructure
failures, not the desired RED. For RED, preserve source exception locations at
the nil-owner seams and assertion context. For GREEN, require four executed
declarations, all fixture assertions preserved, zero failed/ignored, and no
changed control body. Exact proposed logs:
/private/tmp/ferricstore2-elixir-worker-index-regression-red.log and
/private/tmp/ferricstore2-elixir-worker-index-regression-green.log.

That Mix command needs this isolated checkout's own supported dependencies,
compiled full source including the unchanged Rustler native modules, existing
Ferricstore.Application/setup/ShardHelpers, and four test shards. It may invoke
RustlerPrecompiled artifact acquisition during compilation; NO acquisition,
cache/deps copy or build command is authorized here. The current worktree has
no deps or _build directories. A future exact original-runtime proposal must
bind those inputs and all generated paths, including this checkout's
test-results/ from JUnit configuration, application test data, native
artifacts/build output and existing audit formatter output. Do not point it at
original/baseline data or share a live runtime/build cache with peers.
The first small probe deliberately avoids those effects.

A supported full source compile/warnings/format regression and the original
workflow, query/hibernation/rebuild, crash/recovery and mandatory tagged/profile
suites remain necessary after these controls. Their concrete artifacts/resource
batch and runtime acceptance must be separately reviewed; this tiny diagnostic
does not authorize or claim those profiles. No frozen accounting advances.

### Consultation/handoff state

Earlier exact source seam and dirty-diff qualification were relayed directly to
root, architecture and domain. The complete stable EB2 proposal above is now
ready for actual peer reads. Selected behavior, source/test/probe paths and
runtime limits are proposals pending architecture's recorded decision and
root's separate actual-read assignment. No architecture agreement, independent
runtime review, reproduced bug, RED/GREEN or repaired behavior is claimed.

### EB2a exact construction and byte binding after separate assignment

Actually read COMPLETE A-EB2 and coordination:2714–2744, including root's
separately recorded/read CONSTRUCTION-ONLY assignment. Constructed ONLY the
reviewed probe and four declarations in the two named existing test paths.
No source patch, evaluation, formatter, VM/Mix/test command, dependency/cache,
log or commit/push occurred. This supersedes the preceding consultation state
only for that construction; runtime and both staged production patches remain
unassigned.

The construction body is the complete first Elixir fence above, excluding the
single newline immediately before its closing fence. Its actual file has NO
final LF: 4,604 UTF-8 bytes,142 physical lines,SHA256
cd062f66c65cb15f4427e83a93c25c6e4226ad5bd450751998d90f6c3f061344.
The alternative 4,605-byte/7035cc05 hash included the delimiter LF; it was a
DATA extraction convention, never a constructed artifact. All previous smaller
three-comparison byte bindings are superseded. The actual file retains the
original three comparisons and the twelve exact supplementary cells unchanged.

Final constructed test files, with only indentation and declaration separation
needed by their existing module/quote, are:

| Existing relative path | Bytes | SHA256 |
|---|---:|---|
| apps/ferricstore/test/ferricstore/flow/lmdb_test.exs | 29020 | aa5434fe12bf06ac964119086f3452361b02b48e889916a748c7169c1d4ed929 |
| apps/ferricstore/test/ferricstore/flow_test/sections/flow_spawn_children_rejects_missing_wait_state_waiting_children.exs | 43032 | 86306182fc2067ea2625c1203fb8e6da17ee08a5c6cdcd5941b02be28bf0be49 |

The complete resulting diff was actually read. Removing only the two new
declaration blocks from each test recovers every prior byte. HEAD remains
66586c596e271408d9184af32eb31cd7218c126e on codex/workflow-elixir-bugfixes;
status is exactly those two modified tests and the one untracked probe. Actual
source inputs remain:

| Existing relative path | Bytes | SHA256 |
|---|---:|---|
| apps/ferricstore/lib/ferricstore/flow/lmdb/value_locator.ex | 2490 | f9972d458979c284e0477c58ff4baeffbb66880f045b31023d3a91bfd6ec315f |
| apps/ferricstore/lib/ferricstore/flow/keys.ex | 32363 | bf9cbe380b9a59ef275a7efe9f94b8ee86ff10d2a7c4162eeb3a6dc24c6d8349 |
| apps/ferricstore/lib/ferricstore/flow/lmdb.ex | 57746 | 934287a267f3bbed18a2b57e85fbb44de6b2ab40e101df741eac9d2dd2a47ccb |

These controls still have no observed outcome. Predicted unchanged-source RED
is cases=15 failures=13 with two owned passes; GREEN must execute all fifteen
unchanged comparisons. The pure stage cannot certify either live writer, child
workflow persistence, legacy cleanup or source/native/full-profile acceptance.

### EB-SUP1 finite inline standalone RED supervisor proposal

This is a COMPLETE prospective Python program recorded as DATA in this existing
note, not a new .py/helper, syntax check, log or invocation. Root would own its
preparation, exact inline launch, exclusive RED log, actual output read and
cleanup review. elixir_bugfix owns the already constructed Elixir controls and
later separately assigned source stage. No GREEN hash exists yet: a GREEN
supervisor must bind the independently reviewed patched LMDB bytes and named
GREEN log in a separate fixed-source decision. There is no automatic retry.

The fixed local diagnostic pins Python3.14.2, Elixir1.20.4/OTP29.0.5 and
Darwin/arm64. This host restriction belongs ONLY to this run wrapper; the
actual Elixir probe/tests and proposed product guard have no macOS condition.
Linux requires its own >=1.20/OTP29 installed-core/origin acceptance, fixed
executable/source/native pins and actual POSIX supervisor lifecycle controls,
then the same fifteen comparisons and original public/full source profiles.
macOS needs those project/native/public/full profiles too. This proposal
cannot certify Linux, native compiler/RSS performance or either full profile.

Only FOUR Python named functions are proposed, all inline here:
snapshot_inputs (bounded pre/post regular-file hashes), write_record (bounded
exclusive log writes), request_stop (signal flag, never an exception during
Popen construction), and finish_group (owned child group TERM/KILL/reap).
There is no class, custom type, model, new Elixir module/named function, helper
file, cache, package acquisition or dependency. Collections are ordinary dicts,
lists and tuples. The fresh VM adds only the literal inline origin expression
below before requiring the unchanged real probe. That expression is part of
this separate supervisor/source contract, not an amendment to the probe.

Root's prospective outer structured argv is:
PYTHON, "-I", "-B", "-c", EXACT_PROGRAM_TEXT, EXACT_PROGRAM_SHA256.
PYTHON is the absolute 52640-byte/2cbb3ee8 pinned executable below. Before
launch, root must DATA-extract this entire unique marked fence, excluding the
newline immediately before its closing fence, bind its hash and complete argv,
and actually read the extraction. Any compile-only check or source/archive
write is a separate root assignment; it is not implied here. A prospective
preparation archive can use ONLY root's already named compiler trace
/private/tmp/ferricstore2-completion-compiler-trace.log after its own identity/
capacity approval. It is outside this attempt's two-record/2MiB RED log.

The program reads full E/R beams/apps/native shared-library inventory, the exact
critical launcher/boot/Python/shell pins, all three real source files, unchanged
probe, both constructed test files as nonexecuted DATA, and this carrier both
before and after the one VM. Each runtime file is
<=32MiB; <=4096 input paths with <=4096-byte names; cumulative runtime hash
intake <=256MiB across BOTH snapshots. Actual source+probe is97203 bytes per
snapshot, <=128KiB per snapshot and <=256KiB cumulative; each source<=64KiB and
probe<=8KiB. The two test controls are72052 bytes per snapshot, each<=64KiB,
with their own<=128KiB per snapshot/256KiB cumulative DATA cap; they are not
loaded or evaluated by the pure VM. Carrier is<=128KiB per snapshot/256KiB cumulative; program<=32KiB
and origin expression<=16KiB. Every read is known-size/no-follow with parent/
descriptor/final path identity checks and<=64KiB chunks. These intake and
metadata caps do not bound metadata-walk/VM/compiler native heap or RSS.
The VM's checked File.read!/object-code owners and real source compilation
perform additional reads outside the Python hash counters. Inventory/JSON
owners exist before their serialized size checks; the bounded decoded frame's
pair lists and accepted dictionaries also coexist.

The VM is a new owned session/process group, two schedulers/two async threads,
10s measured from immediately before Popen, including process creation and
origin checks/source compilation/comparisons. Creation itself may be
uninterruptible; its elapsed time counts toward the subsequent deadline check.
This is not an OS hard10s creation or whole-attempt bound. Pre/post hashing
and separate TERM2s/KILL2s cleanup remain outside that clock.
All timeout/output/signal/exception paths enter bounded TERM2s then KILL2s,
reap the direct child and require the group gone. stdin is DEVNULL; stdout and
stderr are separate nonblocking streams with<=64KiB retained EACH. Overflow
retains only the bounded raw prefix, records the refusal and is infrastructure
failure; no incomplete stream can certify RED. Five predicted nil exception
stacks individually<=16KiB could exceed cumulative stderr even before compiler
warnings, so fitting a genuine RED is NOT guaranteed and caps never increase
silently. Exception formatting allocates before its individual size check.
Catchable TERM/INT sets a flag, allowing Popen construction to finish so its
child is tracked and cleaned. Uncatchable supervisor SIGKILL/host failure has
no in-process cleanup proof. The two-record layout persists no child PID/group
identity before its after-record; loss during that interval leaves no early
identity recorded by this program. Root needs separately available actual
ownership/accounting evidence before proposing any later attempt. There is no
automatic retry or completed abnormal-cleanup claim. No cgroup/launchd/extra
supervisor or early-PID record is introduced.

The fresh pre-probe origin frame verifies version/build OTP/root, effective
schedulers/async count, bounded E/R-only code paths/loaded origins, three actual
core object-file/loaded-MD5 matches, no preloaded product modules, no Mix or
registered ExUnit module, and the exact core started applications. It does
not explicitly load crypto or call a product NIF. No cold-crypto assertion is
introduced. The earlier accepted core PID46923 is
still only the prerequisite; this prospective frame is new attempt evidence.
The probe then compiles all complete source files and requires their exact
returned module names. Its System.halt means there is NO post-probe fresh VM
application/module frame. The post snapshot proves unchanged disk inputs;
source/launcher ownership and the bound complete probe establish this narrow
load path, not full project/no-service/native acceptance or adversarial
replace-and-restore resistance. No after-VM loaded-origin proof is fabricated.

ONLY /private/tmp/ferricstore2-elixir-worker-index-red.log would be created,
exclusively0600/no-follow with no overwrite, by root. Before/after JSON records
each<=1MiB, complete log<=2MiB, full raw successful streams encoded losslessly
as hex. The scalar pre-Popen gate reserves the FULL1MiB remaining after-record
ceiling in that file limit; it does not reserve physical storage or guarantee
I/O. The final summary retains failure, after_failure, validation_failure,
log_failure and log_close_failure, using null when absent. Late write/close
failure can follow an after-record containing accepted_red=true, so that flag
alone is insufficient: successful final summary/exit0, both complete records,
fsync/close, unchanged inputs and group-gone are required together.
Existing log/identity change/hash mismatch, failed compilation/origin,
missing summary/controls, stream/stack/input overflow, timeout, signal, residual
group or reap/log failure is infrastructure exit2, never target RED. Target RED
requires exit1, exact two owned PASS lines and cases=15 failures=13, all thirteen
failure labels in order, eight exact missing/empty mismatches, and FIVE real
nil exception blocks each containing Keys and LMDB source frames. A frame
validation/format assumption failure remains infrastructure pending an explicit
source-backed amendment, never a silent retry or claimed bug repair.

A-EB3's parent parser retains object-pair lists using the sole anonymous
lambda pairs:pairs. Before top-level dictionary conversion, it requires ten
typed tuple pairs and exactly ten unique known keys. Each of the three proof
objects requires three typed tuple pairs, three unique known keys and string
values before conversion. The six named scalar fields are exact str; scheduler/
async fields are exact int before comparison to2; started is an exact five-str
list; architecture is<=4096 strict UTF8 bytes; all original value/path/MD5
checks remain. Accepted proof dictionaries replace only those three known
objects in the archived frame. No recursive conversion helper/type is added.

<!-- EB-SUP1-PROGRAM-START -->
~~~python
import hashlib, json, os, pathlib, selectors, signal, stat, subprocess, sys, time

ROOT = pathlib.Path('/Users/yoavgea/repos/ferricstore2-elixir-bugfix')
NOTE = pathlib.Path('/Users/yoavgea/repos/ferricstore2-rust-design/docs/reviews/workflow-completion-elixir-bugs.md')
E = pathlib.Path('/Users/yoavgea/.local/share/mise/installs/elixir/1.20.4-otp-29')
R = pathlib.Path('/Users/yoavgea/.local/share/mise/installs/erlang/29.0.5')
PYTHON = pathlib.Path('/opt/homebrew/Cellar/python@3.14/3.14.2/Frameworks/Python.framework/Versions/3.14/bin/python3.14')
PYTHON_REPORTED = pathlib.Path('/opt/homebrew/opt/python@3.14/bin/python3.14')
PYTHON_LINKS = (
    (pathlib.Path('/opt/homebrew/opt/python@3.14'),
     '../Cellar/python@3.14/3.14.2', 3020353),
    (pathlib.Path('/opt/homebrew/Cellar/python@3.14/3.14.2/bin/python3.14'),
     '../Frameworks/Python.framework/Versions/3.14/bin/python3.14', 3019633),
)
PROBE = ROOT / 'tools/flow-worker-index-regression.exs'
LOG = pathlib.Path('/private/tmp/ferricstore2-elixir-worker-index-green.log')
HOME_VALUE = '/Users/yoavgea'
TMP_VALUE = '/var/folders/n3/4p13zj5n0kjbs8xk2qb4jq080000gn/T/'
SOURCE_PINS = {
    str(ROOT / 'apps/ferricstore/lib/ferricstore/flow/lmdb/value_locator.ex'):
        (2490, 'f9972d458979c284e0477c58ff4baeffbb66880f045b31023d3a91bfd6ec315f'),
    str(ROOT / 'apps/ferricstore/lib/ferricstore/flow/keys.ex'):
        (32363, 'bf9cbe380b9a59ef275a7efe9f94b8ee86ff10d2a7c4162eeb3a6dc24c6d8349'),
    str(ROOT / 'apps/ferricstore/lib/ferricstore/flow/lmdb.ex'):
        (57837, 'fdb703a7bff9fa5dcd260eb70823f067b92f5dd457afbb1d8cadfb4a530edb58'),
    str(PROBE): (4604, 'cd062f66c65cb15f4427e83a93c25c6e4226ad5bd450751998d90f6c3f061344'),
}
TEST_PINS = {
    str(ROOT / 'apps/ferricstore/test/ferricstore/flow/lmdb_test.exs'):
        (29020, 'aa5434fe12bf06ac964119086f3452361b02b48e889916a748c7169c1d4ed929'),
    str(ROOT / 'apps/ferricstore/test/ferricstore/flow_test/sections/flow_spawn_children_rejects_missing_wait_state_waiting_children.exs'):
        (43032, '86306182fc2067ea2625c1203fb8e6da17ee08a5c6cdcd5941b02be28bf0be49'),
}
PINNED = {
    str(PYTHON): '2cbb3ee843d8abcf93ed35bb9102391f18eefccb03d47908547bd8db67bc395c',
    '/bin/sh': 'c0eaf44f9242d5bbc2e14f4e8b7dccc1eff7f2976d64dc18914d5ef9f373e100',
    str(E / 'VERSION'): '2c695e095cd2f2b53fc4c472996a88157a3bd9c7f4df3aebc2dd45aeb179d836',
    str(E / 'bin/elixir'): '5cb23d89a78f75589b06fade3097e217b77e63416d61c1800f674579748c4307',
    str(E / 'lib/elixir/ebin/elixir.app'): '571e4d3a5a07bb859532d31901b4cc7ebec2a613c3a9f0bd343b1414ed286e1e',
    str(E / 'lib/elixir/ebin/Elixir.System.beam'): 'eb9b9a26ee22c7201fbce57d17260b1286586d87f460ba500ea81e2f7523e524',
    str(E / 'lib/elixir/ebin/Elixir.JSON.beam'): 'e4291d5e558c40bcba880dbcb2dfe68f631a1680780cda3a97d4303fed93d2dc',
    str(R / 'bin/erl'): '885a3918bff96c5af168365c4becbf838601f329afc2bac0fb5c3a1e66d4f6cd',
    str(R / 'erts-17.0.5/bin/erl'): '885a3918bff96c5af168365c4becbf838601f329afc2bac0fb5c3a1e66d4f6cd',
    str(R / 'releases/29/OTP_VERSION'): '61e2dccfe26dc8d2f506bb25763451c45d059547777f115dd41d1f8a96c9e89f',
    str(R / 'releases/start_erl.data'): '34914e6002ed5310b5e0c4d0006ac7f0bb91d46b3ceee5bffa0738e53f0b3d9b',
    str(R / 'releases/29/start_clean.boot'): '61542d9b2b2887c1b1e06e9a95a7863b368c8782b0be46db786282647ae1c258',
    str(R / 'erts-17.0.5/bin/erlexec'): 'f2f5732f2072a3d21ba0dd5d582549ab554f6d31803b1654301601879b6b62d1',
    str(R / 'erts-17.0.5/bin/beam.smp'): '72d335e068818a984e9fa6fef1cda5810b02e1b0ce7cefc81456754dfde3bf8a',
    str(R / 'lib/crypto-5.9.2/ebin/crypto.app'): 'f0ab450cb87927b29f3740c6eef2eb14cd36d42e4ba881c0c79f1ff5630f7a1a',
    str(R / 'lib/crypto-5.9.2/ebin/crypto.beam'): 'ff696d7a7abc2c9cfe89663da271d9399acdf73278646ddbffc5d4f0f5ad238b',
    str(R / 'lib/crypto-5.9.2/priv/lib/crypto.so'): '9631aec2e87046d60a9e358a79de730432557ff867bd5cfb18f9ecea78c2ccbd',
    str(R / 'lib/stdlib-8.0.3/ebin/json.beam'): 'be6d8be9ca0a43c0b05492f37a15805d1b584e2de0dd893f9f36d668833c2291',
}
runtime_read_bytes = source_read_bytes = test_read_bytes = carrier_read_bytes = log_written = 0
stop_requested = None
log_identity = None

def request_stop(signum, frame):
    global stop_requested
    stop_requested = signum

def snapshot_inputs(allow_stop=False):
    global runtime_read_bytes, source_read_bytes, test_read_bytes, carrier_read_bytes
    assert allow_stop or stop_requested is None, 'supervisor signal before snapshot'
    assert sys.executable == str(PYTHON_REPORTED)
    assert PYTHON_REPORTED == PYTHON_LINKS[0][0] / 'bin/python3.14'
    assert os.path.normpath(str(PYTHON_LINKS[0][0].parent /
                               PYTHON_LINKS[0][1] / 'bin/python3.14')) == str(PYTHON_LINKS[1][0])
    assert os.path.normpath(str(PYTHON_LINKS[1][0].parent /
                               PYTHON_LINKS[1][1])) == str(PYTHON)
    origin_rows = {}
    origin_parents = sorted({str(parent) for path in (PYTHON, PYTHON_LINKS[0][0], PYTHON_LINKS[1][0])
                             for parent in path.parents})
    assert len(origin_parents) == 13
    for text in origin_parents:
        assert allow_stop or stop_requested is None, 'supervisor signal during Python parents'
        assert len(text.encode('utf-8', 'strict')) <= 4096
        info = pathlib.Path(text).lstat()
        assert stat.S_ISDIR(info.st_mode)
        origin_rows[text] = {'type': 'directory',
                             'identity': [info.st_dev, info.st_ino, info.st_mode,
                                          info.st_mtime_ns, info.st_ctime_ns]}
    for path, expected_target, expected_inode in PYTHON_LINKS:
        assert allow_stop or stop_requested is None, 'supervisor signal during Python links'
        text = str(path)
        assert len(text.encode('utf-8', 'strict')) <= 4096
        info = path.lstat()
        assert stat.S_ISLNK(info.st_mode)
        assert (info.st_dev, info.st_ino, info.st_nlink) == (16777233, expected_inode, 1)
        assert info.st_size == len(expected_target.encode('utf-8', 'strict')) <= 4096
        assert info.st_size <= 256 * 1024 * 1024 - runtime_read_bytes
        target_bytes = os.readlink(os.fsencode(path))
        runtime_read_bytes += len(target_bytes)
        assert runtime_read_bytes <= 256 * 1024 * 1024
        assert len(target_bytes) == info.st_size
        target = target_bytes.decode('utf-8', 'strict')
        assert target == expected_target
        origin_rows[text] = {'type': 'symlink', 'bytes': len(target_bytes), 'target': target,
                             'nlink': info.st_nlink,
                             'identity': [info.st_dev, info.st_ino, info.st_mode,
                                          info.st_mtime_ns, info.st_ctime_ns]}
    assert len(origin_rows) == 15
    for root in (ROOT, E, R, pathlib.Path(HOME_VALUE), pathlib.Path(TMP_VALUE).resolve()):
        assert stat.S_ISDIR(root.lstat().st_mode) and not root.is_symlink()
        for parent in root.parents:
            assert stat.S_ISDIR(parent.lstat().st_mode) and not parent.is_symlink()
    assert not os.path.lexists(pathlib.Path(HOME_VALUE) / '.erlang')
    assert not os.path.lexists(ROOT / '.erlang')
    paths = set(PINNED) | set(SOURCE_PINS) | set(TEST_PINS) | {str(NOTE)}
    assert not set(origin_rows).intersection(paths) and len(paths) + len(origin_rows) <= 4096
    for root in (E, R):
        for pattern in ('lib/**/ebin/*.beam', 'lib/**/ebin/*.app', 'lib/**/priv/lib/*.so'):
            for path in root.glob(pattern):
                assert allow_stop or stop_requested is None, 'supervisor signal during inventory'
                text = str(path)
                assert len(text.encode('utf-8')) <= 4096
                assert text in paths or len(paths) + len(origin_rows) < 4096
                paths.add(text)
    for path in (E / 'lib/elixir/lib/system.ex', E / 'lib/elixir/lib/json.ex',
                 R / 'releases/RELEASES', R / 'releases/29/installed_application_versions',
                 R / 'bin/start.boot', R / 'releases/29/start_clean.script'):
        assert str(path) in paths or len(paths) + len(origin_rows) < 4096
        paths.add(str(path))
    assert not set(origin_rows).intersection(paths) and len(paths) + len(origin_rows) <= 4096
    rows, carrier, phase_source_bytes, phase_test_bytes = dict(origin_rows), b'', 0, 0
    for text in sorted(paths):
        assert allow_stop or stop_requested is None, 'supervisor signal during hash'
        assert len(text.encode('utf-8')) <= 4096
        path = pathlib.Path(text)
        for parent in path.parents:
            assert stat.S_ISDIR(parent.lstat().st_mode) and not parent.is_symlink()
        info = path.lstat()
        limit = (8192 if text == str(PROBE) else 65536) if text in SOURCE_PINS or text in TEST_PINS else (
            131072 if text == str(NOTE) else 32 * 1024 * 1024)
        assert stat.S_ISREG(info.st_mode) and 0 <= info.st_size <= limit
        if text == str(PYTHON):
            assert (info.st_dev, info.st_ino, info.st_nlink) == (16777233, 3020257, 1)
        if text in SOURCE_PINS:
            assert info.st_size <= 262144 - source_read_bytes
            assert info.st_size <= 131072 - phase_source_bytes
        elif text in TEST_PINS:
            assert info.st_size <= 262144 - test_read_bytes
            assert info.st_size <= 131072 - phase_test_bytes
        elif text == str(NOTE):
            assert info.st_size <= 262144 - carrier_read_bytes
        else:
            assert info.st_size <= 256 * 1024 * 1024 - runtime_read_bytes
        identity = (info.st_dev, info.st_ino, info.st_size, info.st_mtime_ns, info.st_ctime_ns)
        fd = os.open(path, os.O_RDONLY | os.O_NOFOLLOW | os.O_CLOEXEC)
        try:
            opened = os.fstat(fd)
            assert stat.S_ISREG(opened.st_mode)
            if text == str(PYTHON):
                assert (opened.st_mode, opened.st_nlink) == (info.st_mode, 1)
            assert (opened.st_dev, opened.st_ino, opened.st_size, opened.st_mtime_ns, opened.st_ctime_ns) == identity
            size, digest = 0, hashlib.sha256()
            while size < info.st_size:
                assert allow_stop or stop_requested is None, 'supervisor signal during read'
                chunk = os.read(fd, min(65536, info.st_size - size))
                assert chunk, 'short input read'
                if text in SOURCE_PINS:
                    assert len(chunk) <= 262144 - source_read_bytes
                    assert len(chunk) <= 131072 - phase_source_bytes
                    source_read_bytes += len(chunk)
                    phase_source_bytes += len(chunk)
                elif text in TEST_PINS:
                    assert len(chunk) <= 262144 - test_read_bytes
                    assert len(chunk) <= 131072 - phase_test_bytes
                    test_read_bytes += len(chunk)
                    phase_test_bytes += len(chunk)
                elif text == str(NOTE):
                    assert len(chunk) <= 262144 - carrier_read_bytes
                    carrier_read_bytes += len(chunk)
                    carrier += chunk
                else:
                    assert len(chunk) <= 256 * 1024 * 1024 - runtime_read_bytes
                    runtime_read_bytes += len(chunk)
                size += len(chunk)
                digest.update(chunk)
            final = os.fstat(fd)
            assert (final.st_dev, final.st_ino, final.st_size, final.st_mtime_ns, final.st_ctime_ns) == identity
            current = path.lstat()
            assert (current.st_dev, current.st_ino, current.st_size, current.st_mtime_ns, current.st_ctime_ns) == identity
            if text == str(PYTHON):
                assert (final.st_mode, final.st_nlink) == (info.st_mode, 1)
                assert (current.st_mode, current.st_nlink) == (info.st_mode, 1)
            assert size == info.st_size
        finally:
            os.close(fd)
        sha256 = digest.hexdigest()
        if text in PINNED:
            assert sha256 == PINNED[text], 'runtime pin mismatch: ' + text
        if text in SOURCE_PINS:
            assert (size, sha256) == SOURCE_PINS[text], 'source/control pin mismatch: ' + text
        if text in TEST_PINS:
            assert (size, sha256) == TEST_PINS[text], 'nonexecuted regression pin mismatch: ' + text
        rows[text] = {'type': 'regular', 'bytes': size, 'sha256': sha256}
        if text == str(PYTHON):
            rows[text]['reported_executable'] = sys.executable
            rows[text]['identity'] = list(identity) + [info.st_mode, info.st_nlink]
    for text, row in origin_rows.items():
        assert allow_stop or stop_requested is None, 'supervisor signal during Python final metadata'
        current = pathlib.Path(text).lstat()
        assert [current.st_dev, current.st_ino, current.st_mode,
                current.st_mtime_ns, current.st_ctime_ns] == row['identity']
        if row['type'] == 'symlink':
            assert current.st_size == row['bytes'] and current.st_nlink == row['nlink']
    current = PYTHON.lstat()
    assert [current.st_dev, current.st_ino, current.st_size, current.st_mtime_ns,
            current.st_ctime_ns, current.st_mode, current.st_nlink] == rows[str(PYTHON)]['identity']
    assert phase_source_bytes == 97294
    assert phase_test_bytes == 72052
    assert rows[str(PYTHON)]['bytes'] == 52640
    assert rows[str(E / 'bin/elixir')]['bytes'] == 6658
    assert rows[str(R / 'bin/erl')]['bytes'] == 1427 and rows['/bin/sh']['bytes'] == 101040
    assert len(json.dumps(rows, sort_keys=True, separators=(',', ':')).encode('ascii')) <= 524288
    return rows, carrier

def write_record(fd, record):
    global log_written
    data = (json.dumps(record, sort_keys=True, separators=(',', ':')) + '\n').encode('ascii')
    assert len(data) <= 1048576 and len(data) <= 2097152 - log_written
    current = os.fstat(fd)
    assert stat.S_ISREG(current.st_mode) and current.st_nlink == 1
    assert (current.st_dev, current.st_ino) == log_identity and current.st_size == log_written
    named = LOG.lstat()
    assert (named.st_dev, named.st_ino) == log_identity and stat.S_ISREG(named.st_mode)
    offset = 0
    while offset < len(data):
        count = os.write(fd, data[offset:offset + 65536])
        assert count > 0
        offset += count
        log_written += count
    os.fsync(fd)

def finish_group(child):
    actions = []
    for sent_signal, grace in ((signal.SIGTERM, 2.0), (signal.SIGKILL, 2.0)):
        child.poll()
        try:
            os.killpg(child.pid, 0)
        except ProcessLookupError:
            child.wait(timeout=0.05)
            return True, actions
        try:
            os.killpg(child.pid, sent_signal)
        except ProcessLookupError:
            child.wait(timeout=0.05)
            return True, actions
        actions.append(int(sent_signal))
        deadline = time.monotonic() + grace
        while time.monotonic() < deadline:
            child.poll()
            try:
                os.killpg(child.pid, 0)
            except ProcessLookupError:
                child.wait(timeout=0.05)
                return True, actions
            time.sleep(min(0.02, max(0.0, deadline - time.monotonic())))
    child.poll()
    try:
        os.killpg(child.pid, 0)
    except ProcessLookupError:
        child.wait(timeout=0.05)
        return True, actions
    return False, actions

signal.signal(signal.SIGTERM, request_stop)
signal.signal(signal.SIGINT, request_stop)
log_fd = None
try:
    assert sys.version_info[:3] == (3, 14, 2)
    assert sys.executable == str(PYTHON_REPORTED)
    assert os.uname().sysname == 'Darwin' and os.uname().machine == 'arm64'
    assert os.environ.get('HOME') == HOME_VALUE and os.environ.get('TMPDIR') == TMP_VALUE
    assert len(sys.argv) == 2 and len(sys.argv[1]) == 64
    assert all(character in '0123456789abcdef' for character in sys.argv[1])
    before, carrier = snapshot_inputs()
    start_marker = b'<!-- EB-SUP1-PROGRAM-START -->\n~~~python\n'
    end_marker = b'\n~~~\n<!-- EB-SUP1-PROGRAM-END -->'
    assert carrier.count(start_marker) == carrier.count(end_marker) == 1
    program_start = carrier.index(start_marker) + len(start_marker)
    program_end = carrier.index(end_marker, program_start)
    program = carrier[program_start:program_end]
    assert len(program) <= 32768 and hashlib.sha256(program).hexdigest() == sys.argv[1]
    program_text = program.decode('utf-8', 'strict')
    expression = r'''
e = "/Users/yoavgea/.local/share/mise/installs/elixir/1.20.4-otp-29"
r = "/Users/yoavgea/.local/share/mise/installs/erlang/29.0.5"
true = :code.del_path(~c".") in [true, false]
paths = Enum.take(:code.get_path(), 4097)
true = length(paths) <= 4096
true = Enum.all?(paths, fn chars ->
  true = length(Enum.take(chars, 4097)) <= 4096
  path = chars |> List.to_string() |> Path.expand()
  byte_size(path) <= 4096 and (String.starts_with?(path, e <> "/") or String.starts_with?(path, r <> "/"))
end)
loaded = Enum.take(:code.all_loaded(), 4097)
true = length(loaded) <= 4096
true = Enum.all?(loaded, fn {module, origin} ->
  name = Atom.to_string(module)
  false = (name == "Elixir.Mix" or String.starts_with?(name, "Elixir.Mix."))
  false = :erlang.function_exported(module, :__ex_unit__, 0)
  case origin do
    :preloaded -> true
    chars when is_list(chars) ->
      true = length(Enum.take(chars, 4097)) <= 4096
      path = chars |> List.to_string() |> Path.expand()
      byte_size(path) <= 4096 and (String.starts_with?(path, e <> "/") or String.starts_with?(path, r <> "/"))
    _ -> false
  end
end)
for module <- [Ferricstore.Flow.LMDB.ValueLocator, Ferricstore.Flow.Keys, Ferricstore.Flow.LMDB] do
  false = :code.is_loaded(module)
end
true = System.version() == "1.20.4" and System.build_info().otp_release == "29"
true = (:erlang.system_info(:otp_release) |> List.to_string()) == "29"
true = (:erlang.system_info(:version) |> List.to_string()) == "17.0.5"
true = (:code.root_dir() |> List.to_string() |> Path.expand()) == r
{:unix, :darwin} = :os.type()
architecture = :erlang.system_info(:system_architecture) |> List.to_string()
true = byte_size(architecture) <= 4096 and String.starts_with?(architecture, ["aarch64-apple-darwin", "arm64-apple-darwin"])
2 = :erlang.system_info(:schedulers)
2 = :erlang.system_info(:schedulers_online)
2 = :erlang.system_info(:thread_pool_size)
apps = Application.started_applications() |> Enum.map(fn {name, _, _} -> Atom.to_string(name) end) |> Enum.sort()
true = apps == ["compiler", "elixir", "kernel", "logger", "stdlib"]
for module <- [Mix, Mix.Supervisor, Mix.PubSub, Mix.Sync.PubSub] do
  false = :code.is_loaded(module)
  nil = Process.whereis(module)
end
proofs = for {module, path} <- [
  {System, e <> "/lib/elixir/ebin/Elixir.System.beam"},
  {JSON, e <> "/lib/elixir/ebin/Elixir.JSON.beam"},
  {:json, r <> "/lib/stdlib-8.0.3/ebin/json.beam"}
] do
  Code.ensure_loaded!(module)
  true = File.stat!(path).size <= 33_554_432
  actual_chars = :code.which(module)
  true = is_list(actual_chars) and length(Enum.take(actual_chars, 4097)) <= 4096
  ^path = actual_chars |> List.to_string() |> Path.expand()
  {^module, bytes, chars} = :code.get_object_code(module)
  true = is_binary(bytes) and byte_size(bytes) <= 33_554_432
  true = is_list(chars) and length(Enum.take(chars, 4097)) <= 4096
  ^path = chars |> List.to_string() |> Path.expand()
  true = File.read!(path) == bytes
  {:ok, {^module, md5}} = :beam_lib.md5(bytes)
  true = byte_size(md5) == 16 and module.module_info(:md5) == md5
  %{"module" => Atom.to_string(module), "path" => path, "loaded_md5" => Base.encode16(md5, case: :lower)}
end
frame = %{"pid" => System.pid(), "elixir" => System.version(), "otp" => "29", "erts" => "17.0.5",
          "root" => r, "architecture" => architecture, "schedulers" => 2, "async_threads" => 2,
          "started" => apps, "proofs" => proofs}
output = "EB_WORKER_INDEX_RUNTIME_V1 " <> JSON.encode!(frame) <> "\n"
true = byte_size(output) <= 8192
:ok = IO.binwrite(:stdio, output)
Code.require_file("/Users/yoavgea/repos/ferricstore2-elixir-bugfix/tools/flow-worker-index-regression.exs")
'''
    assert len(expression.encode('utf-8')) <= 16384
    environment = {
        'PATH': str(R / 'bin') + ':/usr/bin:/bin', 'HOME': HOME_VALUE, 'TMPDIR': TMP_VALUE,
        'LANG': 'C', 'LC_ALL': 'C', 'ERL_CRASH_DUMP_BYTES': '0', 'ERL_CRASH_DUMP': '/dev/null',
    }
    argv = ['/bin/sh', '-c', 'ulimit -c 0 || exit 2; exec "$@"', 'eb-worker-index-green',
            str(E / 'bin/elixir'), '--no-color', '--erl',
            '+S 2:2 +SDcpu 1 +SDio 1 +A 2 -noinput -boot ' + str(R / 'releases/29/start_clean'),
            '-e', expression]
    for parent in LOG.parents:
        assert stat.S_ISDIR(parent.lstat().st_mode) and not parent.is_symlink()
    log_fd = os.open(LOG, os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW | os.O_CLOEXEC, 0o600)
    log_info = os.fstat(log_fd)
    assert stat.S_ISREG(log_info.st_mode) and stat.S_IMODE(log_info.st_mode) == 0o600
    log_identity = (log_info.st_dev, log_info.st_ino)
    before_hash = hashlib.sha256(json.dumps(before, sort_keys=True, separators=(',', ':')).encode('ascii')).hexdigest()
    write_record(log_fd, {'kind': 'EB-SUP1-before', 'program_source': program_text,
                         'program_sha256': sys.argv[1], 'argv': argv, 'environment': environment,
                         'python': str(PYTHON), 'before': before, 'before_sha256': before_hash})
    assert 2097152 - log_written >= 1048576, 'less than full 1MiB after-record log capacity'
    assert stop_requested is None, 'supervisor signal before Popen'
except BaseException as error:
    refusal = {'accepted_green': False, 'phase': 'before-VM',
               'failure': type(error).__name__ + ': ' + str(error)[:1024]}
    if log_fd is not None:
        try:
            os.close(log_fd)
        except BaseException as close_error:
            refusal['log_close_failure'] = type(close_error).__name__ + ': ' + str(close_error)[:1024]
    print(json.dumps(refusal, separators=(',', ':')))
    sys.exit(2)

child = None
selector = None
streams = {'stdout': bytearray(), 'stderr': bytearray()}
seen = {'stdout': 0, 'stderr': 0}
failure = None
group_gone = False
cleanup_actions = []
try:
    selector = selectors.DefaultSelector()
    vm_deadline = time.monotonic() + 10.0
    child = subprocess.Popen(argv, cwd=ROOT, env=environment, stdin=subprocess.DEVNULL,
                             stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                             start_new_session=True, close_fds=True)
    assert os.getpgid(child.pid) == child.pid and os.getsid(child.pid) == child.pid
    for stream, name in ((child.stdout, 'stdout'), (child.stderr, 'stderr')):
        os.set_blocking(stream.fileno(), False)
        selector.register(stream, selectors.EVENT_READ, name)
    while selector.get_map() or child.poll() is None:
        assert stop_requested is None, 'supervisor signal during VM'
        remaining = vm_deadline - time.monotonic()
        assert remaining > 0, 'owned VM 10s deadline'
        events = selector.select(min(0.05, remaining))
        for key, mask in events:
            try:
                chunk = os.read(key.fileobj.fileno(), 4096)
            except BlockingIOError:
                continue
            if not chunk:
                selector.unregister(key.fileobj)
                continue
            name = key.data
            seen[name] += len(chunk)
            remaining_bytes = 65536 - len(streams[name])
            streams[name].extend(chunk[:remaining_bytes])
            assert len(chunk) <= remaining_bytes, name + ' exceeds 64KiB'
        if not selector.get_map() and child.poll() is None:
            time.sleep(min(0.02, max(0.0, vm_deadline - time.monotonic())))
except BaseException as error:
    failure = type(error).__name__ + ': ' + str(error)[:1024]
finally:
    if child is not None:
        try:
            group_gone, cleanup_actions = finish_group(child)
            if cleanup_actions and failure is None:
                failure = 'residual owned process group after completed streams'
            if not group_gone and failure is None:
                failure = 'owned group survives TERM2s/KILL2s'
        except BaseException as error:
            failure = (failure or '') + ' cleanup: ' + type(error).__name__ + ': ' + str(error)[:1024]
        for stream in (child.stdout, child.stderr):
            if stream is not None:
                try:
                    stream.close()
                except BaseException as error:
                    failure = (failure or '') + ' close: ' + type(error).__name__
    if selector is not None:
        try:
            selector.close()
        except BaseException as error:
            failure = (failure or '') + ' selector-close: ' + type(error).__name__

after, after_failure = {}, None
try:
    after, after_carrier = snapshot_inputs(allow_stop=True)
except BaseException as error:
    after_failure = type(error).__name__ + ': ' + str(error)[:1024]
result = {'pid': None if child is None else child.pid, 'exit': None if child is None else child.returncode,
          'failure': failure, 'group_gone': group_gone, 'cleanup_signals': cleanup_actions,
          'signal': stop_requested, 'after_failure': after_failure,
          'inputs_unchanged': after_failure is None and before == after,
          'stdout': bytes(streams['stdout']).hex(), 'stderr': bytes(streams['stderr']).hex(),
          'raw_encoding': 'hex', 'stream_bytes_seen': seen, 'accepted_green': False}
try:
    assert failure is None and after_failure is None and group_gone and stop_requested is None
    assert before == after and child is not None and child.returncode == 0
    stdout = bytes(streams['stdout']).decode('utf-8', 'strict')
    stderr = bytes(streams['stderr']).decode('utf-8', 'strict')
    lines = stdout.splitlines()
    prefix = 'EB_WORKER_INDEX_RUNTIME_V1 '
    assert lines and lines[0].startswith(prefix) and len(lines[0].encode()) <= 8192
    assert stdout.count(prefix) == 1
    frame_pairs = json.loads(lines[0][len(prefix):], object_pairs_hook=lambda pairs: pairs)
    assert type(frame_pairs) is list and len(frame_pairs) == 10
    assert all(type(pair) is tuple and len(pair) == 2 and type(pair[0]) is str for pair in frame_pairs)
    frame_keys = [pair[0] for pair in frame_pairs]
    assert len(set(frame_keys)) == 10 and set(frame_keys) == {
        'pid', 'elixir', 'otp', 'erts', 'root', 'architecture',
        'schedulers', 'async_threads', 'started', 'proofs'}
    frame = dict(frame_pairs)
    assert all(type(frame[key]) is str for key in ('pid', 'elixir', 'otp', 'erts', 'root', 'architecture'))
    assert (frame['pid'], frame['elixir'], frame['otp'], frame['erts'], frame['root']) == (
        str(child.pid), '1.20.4', '29', '17.0.5', str(R))
    assert type(frame['schedulers']) is int and type(frame['async_threads']) is int
    assert frame['schedulers'] == frame['async_threads'] == 2
    assert len(frame['architecture'].encode('utf-8', 'strict')) <= 4096
    assert frame['architecture'].startswith(('aarch64-apple-darwin', 'arm64-apple-darwin'))
    assert type(frame['started']) is list and len(frame['started']) == 5
    assert all(type(name) is str for name in frame['started'])
    assert frame['started'] == ['compiler', 'elixir', 'kernel', 'logger', 'stdlib']
    expected_proofs = [('Elixir.System', str(E / 'lib/elixir/ebin/Elixir.System.beam')),
                       ('Elixir.JSON', str(E / 'lib/elixir/ebin/Elixir.JSON.beam')),
                       ('json', str(R / 'lib/stdlib-8.0.3/ebin/json.beam'))]
    assert type(frame['proofs']) is list and len(frame['proofs']) == 3
    normalized_proofs = []
    for proof_pairs, expected in zip(frame['proofs'], expected_proofs):
        assert type(proof_pairs) is list and len(proof_pairs) == 3
        assert all(type(pair) is tuple and len(pair) == 2 and type(pair[0]) is str for pair in proof_pairs)
        proof_keys = [pair[0] for pair in proof_pairs]
        assert len(set(proof_keys)) == 3 and set(proof_keys) == {'module', 'path', 'loaded_md5'}
        assert all(type(pair[1]) is str for pair in proof_pairs)
        proof = dict(proof_pairs)
        assert set(proof) == {'module', 'path', 'loaded_md5'}
        assert (proof['module'], proof['path']) == expected and proof['path'] in before
        assert len(proof['loaded_md5']) == 32 and all(c in '0123456789abcdef' for c in proof['loaded_md5'])
        normalized_proofs.append(proof)
    frame['proofs'] = normalized_proofs
    expected_passes = ['PASS unowned child-policy running',
                       'PASS owned running', 'PASS owned earlier-deadline running']
    for owner in ('nil', 'missing', 'empty'):
        for due_any in ('false', 'true'):
            for scheduled in ('false', 'true'):
                expected_passes.append('PASS unowned ' + owner + ' due_any=' + due_any +
                                       ' scheduled=' + scheduled)
    assert lines[1:] == expected_passes + ['cases=15 failures=0']
    assert stdout.endswith('\n') and 'INFRA:' not in stderr and 'FAIL ' not in stderr
    result['runtime_frame'] = frame
    result['accepted_green'] = True
except BaseException as error:
    result['validation_failure'] = type(error).__name__ + ': ' + str(error)[:1024]
try:
    write_record(log_fd, {'kind': 'EB-SUP1-after', 'result': result, 'after': after,
                         'runtime_read_bytes': runtime_read_bytes, 'source_read_bytes': source_read_bytes,
                         'test_read_bytes': test_read_bytes, 'carrier_read_bytes': carrier_read_bytes})
except BaseException as error:
    result['accepted_green'] = False
    result['log_failure'] = type(error).__name__ + ': ' + str(error)[:1024]
finally:
    try:
        os.close(log_fd)
    except BaseException as error:
        result['accepted_green'] = False
        result['log_close_failure'] = type(error).__name__ + ': ' + str(error)[:1024]
print(json.dumps({'accepted_green': result['accepted_green'], 'pid': result['pid'],
                  'exit': result['exit'], 'group_gone': result['group_gone'],
                  'log': str(LOG), 'log_bytes': log_written,
                  'failure': result.get('failure'), 'after_failure': result.get('after_failure'),
                  'validation_failure': result.get('validation_failure'),
                  'log_failure': result.get('log_failure'),
                  'log_close_failure': result.get('log_close_failure')}, separators=(',', ':')))
sys.exit(0 if result['accepted_green'] else 2)
~~~
<!-- EB-SUP1-PROGRAM-END -->

The supervisor's exit0 would mean ONLY an accepted actual pure target RED,
including cleanup/output/origin binding. It would not mean tests passed or an
Elixir repair occurred. Exit2 is a retained refusal. Root must actually read
both complete log records and decoded bounded streams before assigning any
LMDB guard. The full public workflow RED still precedes the other four live
source edits. No runtime authority follows from this proposal.

### EB-SUP1 original finite SOURCE handoff binding, before A-EB3

The ORIGINAL pre-A-EB3 proposal was actually read completely as DATA. Its
unique marked-fence extraction, excluding the delimiter's final LF,
was exactly25223 UTF-8 bytes/457 physical lines/SHA256
dc541c5158bd3c9996c1430d7f33d6a138b872412d71b90f15da4aa0ad2babd3.
The ACTUAL unchanged origin expression retains both boundary LF bytes:
3647 UTF-8 bytes/SHA256
16044ba7c8e7680e33cd4b15bd18cb7647378f1f8f4f6cd1c0f05c0643869e30,
within16KiB. The earlier3646 DATA count omitted only its leading LF; no
expression byte changes. The original carrier
was66805 bytes before this binding paragraph, within128KiB; root must bind the
then-current complete carrier before actual preparation/invocation. No script
syntax evaluation, compile-only check or program execution occurred here.

The ORIGINAL outer argv's final value was that dc541c51 digest; the amended
source must use its own exact digest after whole-source review. The program
remains RED-only and pins all three original production files, unchanged cd062f66
probe and both EB2a whole-test hashes. Those tests remain nonexecuted DATA in
this smaller run. A second DATA inverse check removed only the exact reviewed
two inserted declaration blocks and matched every original HEAD byte for
both files. The actual probe remains4604/cd062f66/no final LF. Root also
independently read/verified that preservation and all constructed diffs.

Linux/macOS process acceptance must cover this supervisor's own POSIX features:
new-session ownership, TERM then KILL/reap/group-gone, separate nonblocking
pipe draining and cumulative overflow refusal, core-limit and /dev/null crash
routing. Darwin kqueue and Linux epoll/process behavior are distinct local
observations; this unexecuted Darwin wrapper certifies neither. A fixed Linux
counterpart needs its own executable/environment/path/hash decision and actual
supported core/control evidence. Both platforms still require full project
compile/warnings/format, native/setup/public workflow RED/GREEN and the existing
source/profile suites; no authored matrix or old CI result substitutes.

Architecture/root are directly consulted on this WHOLE named program,
additional literal origin expression, source/test/carrier inventory, exact
argv/env, four inline functions and exclusive RED log contract. A subsequent
recorded fixed-source/runtime decision plus root's separate actual-read
assignment must precede any preparation artifact, syntax check, VM or log.
The finite construction/source-proposal phase now returns normally. No
production patch, RED/GREEN, persisted nil-owner shape, legacy cleanup, Linux
acceptance, speed/allocation/RSS gain or frozen parity advancement is claimed.


### EB-SUP1a A-EB3 NOTE-only amendment and whole-source inverse handoff

BEFORE these edits, actually read COMPLETE A-EB3 at architecture:6830 onward,
COMPLETE performance:4591–end, and root's separately recorded/read NOTE-only
assignment at coordination:2892–2918. Also actually re-read the whole original
457-line source/scope in this note. Retained its complete dc541 source as DATA
in the existing tool state; root separately retained that entire original DATA.
Neither copy was syntax-evaluated, archived as a new artifact or invoked.

Amended ONLY four existing inline regions: full1MiB remaining-log scalar gate,
same-body caught pre-VM close preserving the original refusal, exact typed
four-object pairs parsing, and the five existing final diagnostic fields.
Associated prose corrects the literal's leading LF, counts uninterruptible
creation toward the later elapsed check, states absence of a durable early
PID, and distinguishes terminal summary/exit0 from a possibly earlier true
log flag. No fifth named function, recursive converter, class/model/module,
extra record, watchdog or output family was introduced.

The COMPLETE revised program is the sole marked fence above, excluding its
verified delimiter LF: exactly27060 UTF-8 bytes/485 physical lines/SHA256
f1004d2dcea02241fb3661b790f528699bbad1fab2a251a24bc15209842bd152.
It has NO final LF. It remains within32KiB. Outer structured argv is unchanged
except its necessary fixed source/digest values: the exact pinned Python,
"-I", "-B", "-c", WHOLE_REVISED_SOURCE, and
f1004d2dcea02241fb3661b790f528699bbad1fab2a251a24bc15209842bd152.
Child argv/env, ALL pin constants, the entire four-function bodies, all resource
caps/cleanup, two records and true15/13 classification are unchanged.

The raw triple-quoted origin value is byte-identical before/after, retaining
BOTH boundary LF bytes:3647 bytes/SHA256
16044ba7c8e7680e33cd4b15bd18cb7647378f1f8f4f6cd1c0f05c0643869e30.
This is a corrected DATA binding, not a source amendment. The current carrier
was73394 bytes before this handoff/diff, within128KiB; final complete carrier
bytes/hash are relayed after the full publication. Its note content must stay
bound unchanged during any separately assigned attempt.

A DATA-only four-region inverse restores the ENTIRE original source, exactly
25223 bytes/457 lines/dc541c5158bd3c9996c1430d7f33d6a138b872412d71b90f15da4aa0ad2babd3.
Every byte outside these four regions is therefore preserved, not only the
selected origin/flags. The reversible region bindings are:

| Existing region | Original bytes / SHA256 | Revised bytes / SHA256 |
|---|---|---|
| pre-Popen remaining-log scalar gate | 347 / 100a82949a93dc3986997dd02ac70dc0ee756bccb21242af0ff98c1f1586899e | 440 / 82fb16ec7590528a5a629c1a4c4ad902526bcab7596bcff7b8759787c4e437cf |
| pre-VM original cause and caught close | 273 / a432532df13c4186b6d656886e8697655c120470fc631173dcfde30375b55b30 | 453 / 26d88050e68a7b5f09e54c6a6aca9740c10fbdd00836d1822a6da2f1054be464 |
| closed four-object typed pairs parser | 1241 / cb335105b2f33d9321b6faeae68c776e7926ac1d25ffdc93dd4f5b982b9ced09 | 2501 / 699be4f73ed3a534da42055a1219bed31bc79a76b4f7a9e93b8b7b1b3c21f484 |
| five existing final diagnostic fields | 288 / c4c1e9108680ad4fc772771b662b80eb9eeecef086e188bec6ea3aa7d8883ca0 | 592 / ba4ff3387f8d28e8638e69df805d1ac4f82743b2f63049fe83fae6084e5abe03 |

The complete forward unified diff follows. Both whole sources omit a final
LF; the final marker preserves that fact. Its old/new contents are DATA,
not another executable fence or approved runtime/helper:

~~~diff
--- EB-SUP1-dc541c51
+++ EB-SUP1-f1004d2d
@@ -309,12 +309,17 @@
     write_record(log_fd, {'kind': 'EB-SUP1-before', 'program_source': program_text,
                          'program_sha256': sys.argv[1], 'argv': argv, 'environment': environment,
                          'python': str(PYTHON), 'before': before, 'before_sha256': before_hash})
+    assert 2097152 - log_written >= 1048576, 'less than full 1MiB after-record log capacity'
     assert stop_requested is None, 'supervisor signal before Popen'
 except BaseException as error:
+    refusal = {'accepted_red': False, 'phase': 'before-VM',
+               'failure': type(error).__name__ + ': ' + str(error)[:1024]}
     if log_fd is not None:
-        os.close(log_fd)
-    print(json.dumps({'accepted_red': False, 'phase': 'before-VM',
-                      'failure': type(error).__name__ + ': ' + str(error)[:1024]}, separators=(',', ':')))
+        try:
+            os.close(log_fd)
+        except BaseException as close_error:
+            refusal['log_close_failure'] = type(close_error).__name__ + ': ' + str(close_error)[:1024]
+    print(json.dumps(refusal, separators=(',', ':')))
     sys.exit(2)
 
 child = None
@@ -398,22 +403,41 @@
     prefix = 'EB_WORKER_INDEX_RUNTIME_V1 '
     assert lines and lines[0].startswith(prefix) and len(lines[0].encode()) <= 8192
     assert stdout.count(prefix) == 1
-    frame = json.loads(lines[0][len(prefix):])
-    assert set(frame) == {'pid', 'elixir', 'otp', 'erts', 'root', 'architecture',
-                          'schedulers', 'async_threads', 'started', 'proofs'}
+    frame_pairs = json.loads(lines[0][len(prefix):], object_pairs_hook=lambda pairs: pairs)
+    assert type(frame_pairs) is list and len(frame_pairs) == 10
+    assert all(type(pair) is tuple and len(pair) == 2 and type(pair[0]) is str for pair in frame_pairs)
+    frame_keys = [pair[0] for pair in frame_pairs]
+    assert len(set(frame_keys)) == 10 and set(frame_keys) == {
+        'pid', 'elixir', 'otp', 'erts', 'root', 'architecture',
+        'schedulers', 'async_threads', 'started', 'proofs'}
+    frame = dict(frame_pairs)
+    assert all(type(frame[key]) is str for key in ('pid', 'elixir', 'otp', 'erts', 'root', 'architecture'))
     assert (frame['pid'], frame['elixir'], frame['otp'], frame['erts'], frame['root']) == (
         str(child.pid), '1.20.4', '29', '17.0.5', str(R))
+    assert type(frame['schedulers']) is int and type(frame['async_threads']) is int
     assert frame['schedulers'] == frame['async_threads'] == 2
+    assert len(frame['architecture'].encode('utf-8', 'strict')) <= 4096
     assert frame['architecture'].startswith(('aarch64-apple-darwin', 'arm64-apple-darwin'))
+    assert type(frame['started']) is list and len(frame['started']) == 5
+    assert all(type(name) is str for name in frame['started'])
     assert frame['started'] == ['compiler', 'elixir', 'kernel', 'logger', 'stdlib']
     expected_proofs = [('Elixir.System', str(E / 'lib/elixir/ebin/Elixir.System.beam')),
                        ('Elixir.JSON', str(E / 'lib/elixir/ebin/Elixir.JSON.beam')),
                        ('json', str(R / 'lib/stdlib-8.0.3/ebin/json.beam'))]
     assert type(frame['proofs']) is list and len(frame['proofs']) == 3
-    for proof, expected in zip(frame['proofs'], expected_proofs):
+    normalized_proofs = []
+    for proof_pairs, expected in zip(frame['proofs'], expected_proofs):
+        assert type(proof_pairs) is list and len(proof_pairs) == 3
+        assert all(type(pair) is tuple and len(pair) == 2 and type(pair[0]) is str for pair in proof_pairs)
+        proof_keys = [pair[0] for pair in proof_pairs]
+        assert len(set(proof_keys)) == 3 and set(proof_keys) == {'module', 'path', 'loaded_md5'}
+        assert all(type(pair[1]) is str for pair in proof_pairs)
+        proof = dict(proof_pairs)
         assert set(proof) == {'module', 'path', 'loaded_md5'}
         assert (proof['module'], proof['path']) == expected and proof['path'] in before
         assert len(proof['loaded_md5']) == 32 and all(c in '0123456789abcdef' for c in proof['loaded_md5'])
+        normalized_proofs.append(proof)
+    frame['proofs'] = normalized_proofs
     assert lines[1:] == ['PASS owned running', 'PASS owned earlier-deadline running', 'cases=15 failures=13']
     assert stdout.endswith('\n') and 'INFRA:' not in stderr
     expected_failures = [('unowned child-policy running', True)]
@@ -453,5 +477,9 @@
         result['log_close_failure'] = type(error).__name__ + ': ' + str(error)[:1024]
 print(json.dumps({'accepted_red': result['accepted_red'], 'pid': result['pid'],
                   'exit': result['exit'], 'group_gone': result['group_gone'],
-                  'log': str(LOG), 'log_bytes': log_written}, separators=(',', ':')))
+                  'log': str(LOG), 'log_bytes': log_written,
+                  'failure': result.get('failure'), 'after_failure': result.get('after_failure'),
+                  'validation_failure': result.get('validation_failure'),
+                  'log_failure': result.get('log_failure'),
+                  'log_close_failure': result.get('log_close_failure')}, separators=(',', ':')))
 sys.exit(0 if result['accepted_red'] else 2)
\ No newline at end of file
~~~

Only this existing note changed in this finite amendment. No probe/test/
product/source file edit, proposed-source parsing/syntax/compile evaluation,
preparation archive, VM, log, deps/cache/build/acquisition or commit/push.
The unchanged exact fifteen controls, both whole-test bindings and three
production inputs retain EB2a/A-EB2 ownership. This handoff claims only a
source-contract amendment and exact inverse, not parser/ordinary-close test,
RED/GREEN, persisted running-parent shape, legacy-row cleanup, performance/
RSS/native-memory benefit or Linux/macOS source/native/public acceptance.

Root/architecture receive the full revised source/hash/inverse/argv for a NEW
complete fixed-source decision and root's separate actual-read assignment
before syntax/archive/runtime. Linux AND macOS, genuine pure RED before the
separately assigned local LMDB guard, genuine public RED before the other
four live sources, and all supported original/full profiles remain required.
This finite NOTE-only phase returns normally without interrupting peers.

### EB-SUP1b origin-link SOURCE consultation only; f100 remains unchanged

This finite phase follows root's direct NOTE/SOURCE-only order. Actually read
complete AGENTS, README/architecture, coordination:2972–3275 (complete actual
syntax/archive/sole-candidate refusal and post-refusal qualification), and
the whole current 485-line supervisor with its scope/inverse handoff. Consulted
architecture directly before publishing this proposal. Root and architecture
separately clarified applicability: the 400/600 file and 100-function policies
are first-party Rust/Clippy rules. They do not impose a 100-line Python function
cap. No waiver, fifth helper or minification is inferred.

The actual f100 candidate allowance is CONSUMED. Retained exit2 stdout was
exactly {"accepted_red":false,"phase":"before-VM","failure":"AssertionError: "}
plus final LF. The selected RED log was absent; no Popen/Elixir/source/probe
ran. Successful syntax-only stdout EB-SUP1a_SYNTAX_OK and actual 99114-byte
preparation archive remain separate evidence. The existing archive record
SHA256 is f06c5a8a29b2954701da40e3752a7a4a7be2f2c6c2dc1433a4a170d06823a5fa;
actual retained trace is 3573772 bytes / device16777233 / inode125931639 /
8da8154358eeebd8b7d91acc6e00009b9a50df2cb7f790b2f4bb88a286ae3676.
No archive read/write or invocation occurred in this owner phase. The source
order and root's independent ordinary identity DATA identify line207's literal
path assertion as the refusal seam; no traceback/frame logged that line.

The UNIQUE PROGRAM fence is still EVERY original 27060-byte/485-line f100 byte,
without final LF:
f1004d2dcea02241fb3661b790f528699bbad1fab2a251a24bc15209842bd152.
Its prior 83296-byte/1487-line carrier/199d2988 is historical: appending this
consultation changes only the carrier, not that fence. Root must bind any new
complete carrier separately before a future approved preparation. The old
f100 attempt cannot be reused with the changed carrier or called again.
The entire earlier f100-to-dc541 inverse remains preserved above and in root's
separately retained DATA. Current tool state also preserves the whole f100 as
DATA; it is not parsed, syntax-evaluated, archived as a new file or invoked.

**Exact selected spelling and chain, proposed.** Keep PYTHON and the outer
canonical launch argv exactly unchanged. Replace the early executable check
with exact string equality to
/opt/homebrew/opt/python@3.14/bin/python3.14. Add only the ordinary immutable
path/tuple constants PYTHON_REPORTED and PYTHON_LINKS shown in the diff.
There is one accepted reported spelling, two exact link targets and one
pinned endpoint; no generic resolve/realpath/samefile fallback or extra OS
acceptance branch. Recheck reported spelling inside BOTH existing snapshots.

| Exact physical link | Exact target bytes (strict UTF8/ASCII) | Device / inode / nlink |
|---|---|---|
| /opt/homebrew/opt/python@3.14 | ../Cellar/python@3.14/3.14.2 (28 bytes) | 16777233 / 3020353 / 1 |
| /opt/homebrew/Cellar/python@3.14/3.14.2/bin/python3.14 | ../Frameworks/Python.framework/Versions/3.14/bin/python3.14 (59 bytes) | 16777233 / 3019633 / 1 |

Lexical normpath ONLY joins the first literal target with bin/python3.14 to
the second exact link, then joins its literal target to the already pinned
PYTHON. The physical ancestor checks forbid any further symlink in those
paths. The canonical endpoint remains
/opt/homebrew/Cellar/python@3.14/3.14.2/Frameworks/Python.framework/Versions/3.14/bin/python3.14,
regular/single-link/device16777233/inode3020257, 52640 bytes and unchanged
SHA256 2cbb3ee843d8abcf93ed35bb9102391f18eefccb03d47908547bd8db67bc395c.
The unchanged no-follow descriptor/size/hash/final named-path checks still
read that canonical endpoint, never the reported alias. Additional exact
endpoint device/inode/nlink admission, opened/final descriptor mode/nlink,
and final endpoint identity assertions are visible in the diff.

**Exact physical parent set.** snapshot_inputs derives this union from the
two physical link paths and canonical endpoint only, requires exactly13,
and lstat-admits every path as a directory. It never enumerates
opt/python@3.14/bin as an independent parent through the alias. Names are:

~~~text
/
/opt
/opt/homebrew
/opt/homebrew/opt
/opt/homebrew/Cellar
/opt/homebrew/Cellar/python@3.14
/opt/homebrew/Cellar/python@3.14/3.14.2
/opt/homebrew/Cellar/python@3.14/3.14.2/bin
/opt/homebrew/Cellar/python@3.14/3.14.2/Frameworks
/opt/homebrew/Cellar/python@3.14/3.14.2/Frameworks/Python.framework
/opt/homebrew/Cellar/python@3.14/3.14.2/Frameworks/Python.framework/Versions
/opt/homebrew/Cellar/python@3.14/3.14.2/Frameworks/Python.framework/Versions/3.14
/opt/homebrew/Cellar/python@3.14/3.14.2/Frameworks/Python.framework/Versions/3.14/bin
~~~

A separate bounded DATA-only lstat/readlink read in this owner phase confirmed
these13 directories, the observed two exact target/inode/size/nlink rows and
canonical endpoint metadata. It did not execute the proposed proof, invoke
the pinned Python identity query again or hash a completed runtime inventory.
The captured metadata observation is consistent with root's retained refusal
diagnosis; it is not a new supervisor/control acceptance.

**Closed metadata shape and both existing records.** Extend ONLY the ordinary
existing input rows, not record count, labels, logging helper or output paths.
Each of the13 directory path keys has exactly type="directory" and identity,
a five-int list ordered device, inode, mode, mtime_ns, ctime_ns.
Each of the2 exact link path keys has type="symlink", bytes (28 or59), target
(the actual strict decoded exact target), nlink=1, and the same five-int
identity order. These15 path keys must be disjoint from regular input paths.
The canonical PYTHON row retains type="regular", bytes=52640 and its unchanged
sha256, and additionally stores reported_executable=sys.executable and
identity, a seven-int list ordered device, inode, size, mtime_ns, ctime_ns,
mode, nlink. Other existing regular row shapes stay byte-for-byte unchanged.

The existing before record still contains canonical python=str(PYTHON);
its before map now contains those15 metadata rows and the expanded canonical
endpoint row. The existing after record's after map has exactly the same
representation. Each successful snapshot final-lstats all15 metadata names
and the endpoint against its captured identities before returning. Directory
identity includes mode/mtime/ctime; link final checks also include size/nlink;
endpoint named/descriptor identity remains bound to its actual hash read.
Existing whole before==after equality therefore checks complete parent/link/
endpoint metadata as well as all existing file hashes. A legitimate unrelated
Homebrew directory metadata change may cause infrastructure refusal; no
filter, fallback, retry or weakened equality is selected. No additional record
or external actual reported-name field is introduced outside those maps.

**Exact accounting and limits.** Successful link target reads contribute
28+59=87 bytes per snapshot, 174 across both, to the EXISTING cumulative
runtime_read_bytes<=256MiB; the canonical52640-byte regular file remains counted
once per existing snapshot. The proof admits known exact link size<=4096 and
remaining cumulative capacity before os.readlink. It asks for bytes via the
bytes path overload, IMMEDIATELY adds the actual returned len before ANY
content/size/UTF8 comparison, then refuses cumulative overflow, wrong size or
wrong target. Mismatch/refusal bytes are therefore counted rather than only
successful expected bytes. An actual return that crosses the cumulative cap
increments the observed counter and refuses; it cannot be accepted evidence.
There is no counter reset or second target read in a successful snapshot.

os.readlink has no Python read-size parameter. Its known-size admission and
subsequent identity checks do not prove a hard physical read/allocation cap
during concurrent link replacement. They establish the accepted stable-input
contract with ordinary TOCTOU/adversarial replace-and-restore qualifications.
The actual byte string exists before its post-read accounting/checks; no heap/
RSS/allocator or adversarial capture gain is claimed. Directory lstat rechecks
add metadata operations, zero file-content bytes. Map/list/set owners likewise
exist before the unchanged serialized<=512KiB check.

The same4096 total input-path cap now INCLUDES15 additional metadata keys.
Every regular inventory growth gate reserves those15 slots; names stay<=4096
strictUTF8 bytes and final row/disjoint checks precede hashing. The existing
512KiB serialized full rows cap includes the new metadata. The regular-file
32MiB cap, all source/probe/test/carrier intake caps, source97203/test72052
phase equalities,64KiB chunks/streams, origin16KiB,10s later elapsed deadline,
TERM2s/KILL2s/session/group/reap handling, exclusive0600 log/two1MiB records/
2MiB total/full1MiB remaining reservation, exact15/13 validation, all original
exception/close/diagnostic fields and no-early-PID/no-retry qualifications
are unchanged. No total file/inventory count is asserted from f100's aborted
snapshot; this is a concrete prospective contract.

**Exact source-only projection and inverse.** The three-region forward diff
below is COMPLETE DATA against the preserved f100 fence. Virtually applying
it constructs 31198 UTF8 bytes /551 physical lines /SHA256
5f2c758d9f1c590109e982c9cd4a521c2b9338218ae5dda4d2165b7753f2423a,
NO final LF, within the existing32KiB source cap and600-line review projection.
This virtual string was never written into the unique PROGRAM fence, parsed,
compiled, evaluated or invoked. Its coherent snapshot_inputs projects156
normally formatted physical lines (current97) /9340 bytes (current5554),
including the existing trailing separator LF. It keeps the same signature and
ownership responsibility: bounded input snapshot including the exact executable
origin proof. request_stop/write_record/finish_group bodies are byte-identical;
there is no fifth function, class/model/type, anonymous helper, loader, dependency
or named new file/output. Retaining four functions follows the clarified Python
applicability; it is not a hidden Rust cap waiver.

Only these3 existing source regions change prospectively; reversing them
recovers EVERY original27060/485/f100 byte. The unchanged version assertion is
included in the small executable-check region to make its inverse unique;
the same proposed equality also appears in snapshot_inputs. An initial
DATA-only inverse check refused that repeated short token before this exact
context was selected; no candidate/code allowance or artifact was consumed.
All pins/child argv/env/probe/tests/source inputs and origin expression are
therefore preserved outside the named regions. Raw origin remains
3647 /16044ba7c8e7680e33cd4b15bd18cb7647378f1f8f4f6cd1c0f05c0643869e30,
including BOTH boundary LF bytes. The complete reversible region bindings are:

| Prospective existing region | Current f100 bytes / SHA256 | Proposed bytes / SHA256 |
|---|---|---|
| exact reported spelling and two literal link constants | 122 / 278c0437c10fe891f712661a11572685b357cf226fcd55746b7b82c56d6f579b | 474 / 7235755cb0f8a9f7a12ecc344e55b780e584527988b4c906944c8c7bdb5c9f7e |
| existing snapshot_inputs bounded alias proof/rows/intake | 5554 / c99cb2a99a02366f11865943aa1d1c8aff66f77d7ba2b69043d29d4fe1caf9d0 | 9340 / b16ce07d1aaf9db0be0566cddbd84f3193f258c49249ed2d26f76ee03021a878 |
| pre-VM exact reported executable assertion | 96 / 70872db6d5f414786462cd48cbcddd55cf9733bb0d20420cef45e2d61098d0df | 96 / c401664e2e967f26f16239195b4f20c255cd579f8b030cd209e22a0fa772ded1 |

~~~diff
--- EB-SUP1a-f1004d2d-UNCHANGED
+++ EB-SUP1b-origin-PROPOSAL-ONLY
@@ -5,6 +5,13 @@
 E = pathlib.Path('/Users/yoavgea/.local/share/mise/installs/elixir/1.20.4-otp-29')
 R = pathlib.Path('/Users/yoavgea/.local/share/mise/installs/erlang/29.0.5')
 PYTHON = pathlib.Path('/opt/homebrew/Cellar/python@3.14/3.14.2/Frameworks/Python.framework/Versions/3.14/bin/python3.14')
+PYTHON_REPORTED = pathlib.Path('/opt/homebrew/opt/python@3.14/bin/python3.14')
+PYTHON_LINKS = (
+    (pathlib.Path('/opt/homebrew/opt/python@3.14'),
+     '../Cellar/python@3.14/3.14.2', 3020353),
+    (pathlib.Path('/opt/homebrew/Cellar/python@3.14/3.14.2/bin/python3.14'),
+     '../Frameworks/Python.framework/Versions/3.14/bin/python3.14', 3019633),
+)
 PROBE = ROOT / 'tools/flow-worker-index-regression.exs'
 LOG = pathlib.Path('/private/tmp/ferricstore2-elixir-worker-index-red.log')
 HOME_VALUE = '/Users/yoavgea'
@@ -55,6 +62,44 @@
 def snapshot_inputs(allow_stop=False):
     global runtime_read_bytes, source_read_bytes, test_read_bytes, carrier_read_bytes
     assert allow_stop or stop_requested is None, 'supervisor signal before snapshot'
+    assert sys.executable == str(PYTHON_REPORTED)
+    assert PYTHON_REPORTED == PYTHON_LINKS[0][0] / 'bin/python3.14'
+    assert os.path.normpath(str(PYTHON_LINKS[0][0].parent /
+                               PYTHON_LINKS[0][1] / 'bin/python3.14')) == str(PYTHON_LINKS[1][0])
+    assert os.path.normpath(str(PYTHON_LINKS[1][0].parent /
+                               PYTHON_LINKS[1][1])) == str(PYTHON)
+    origin_rows = {}
+    origin_parents = sorted({str(parent) for path in (PYTHON, PYTHON_LINKS[0][0], PYTHON_LINKS[1][0])
+                             for parent in path.parents})
+    assert len(origin_parents) == 13
+    for text in origin_parents:
+        assert allow_stop or stop_requested is None, 'supervisor signal during Python parents'
+        assert len(text.encode('utf-8', 'strict')) <= 4096
+        info = pathlib.Path(text).lstat()
+        assert stat.S_ISDIR(info.st_mode)
+        origin_rows[text] = {'type': 'directory',
+                             'identity': [info.st_dev, info.st_ino, info.st_mode,
+                                          info.st_mtime_ns, info.st_ctime_ns]}
+    for path, expected_target, expected_inode in PYTHON_LINKS:
+        assert allow_stop or stop_requested is None, 'supervisor signal during Python links'
+        text = str(path)
+        assert len(text.encode('utf-8', 'strict')) <= 4096
+        info = path.lstat()
+        assert stat.S_ISLNK(info.st_mode)
+        assert (info.st_dev, info.st_ino, info.st_nlink) == (16777233, expected_inode, 1)
+        assert info.st_size == len(expected_target.encode('utf-8', 'strict')) <= 4096
+        assert info.st_size <= 256 * 1024 * 1024 - runtime_read_bytes
+        target_bytes = os.readlink(os.fsencode(path))
+        runtime_read_bytes += len(target_bytes)
+        assert runtime_read_bytes <= 256 * 1024 * 1024
+        assert len(target_bytes) == info.st_size
+        target = target_bytes.decode('utf-8', 'strict')
+        assert target == expected_target
+        origin_rows[text] = {'type': 'symlink', 'bytes': len(target_bytes), 'target': target,
+                             'nlink': info.st_nlink,
+                             'identity': [info.st_dev, info.st_ino, info.st_mode,
+                                          info.st_mtime_ns, info.st_ctime_ns]}
+    assert len(origin_rows) == 15
     for root in (ROOT, E, R, pathlib.Path(HOME_VALUE), pathlib.Path(TMP_VALUE).resolve()):
         assert stat.S_ISDIR(root.lstat().st_mode) and not root.is_symlink()
         for parent in root.parents:
@@ -62,21 +107,22 @@
     assert not os.path.lexists(pathlib.Path(HOME_VALUE) / '.erlang')
     assert not os.path.lexists(ROOT / '.erlang')
     paths = set(PINNED) | set(SOURCE_PINS) | set(TEST_PINS) | {str(NOTE)}
+    assert not set(origin_rows).intersection(paths) and len(paths) + len(origin_rows) <= 4096
     for root in (E, R):
         for pattern in ('lib/**/ebin/*.beam', 'lib/**/ebin/*.app', 'lib/**/priv/lib/*.so'):
             for path in root.glob(pattern):
                 assert allow_stop or stop_requested is None, 'supervisor signal during inventory'
                 text = str(path)
                 assert len(text.encode('utf-8')) <= 4096
-                assert text in paths or len(paths) < 4096
+                assert text in paths or len(paths) + len(origin_rows) < 4096
                 paths.add(text)
     for path in (E / 'lib/elixir/lib/system.ex', E / 'lib/elixir/lib/json.ex',
                  R / 'releases/RELEASES', R / 'releases/29/installed_application_versions',
                  R / 'bin/start.boot', R / 'releases/29/start_clean.script'):
-        assert str(path) in paths or len(paths) < 4096
+        assert str(path) in paths or len(paths) + len(origin_rows) < 4096
         paths.add(str(path))
-    assert len(paths) <= 4096
-    rows, carrier, phase_source_bytes, phase_test_bytes = {}, b'', 0, 0
+    assert not set(origin_rows).intersection(paths) and len(paths) + len(origin_rows) <= 4096
+    rows, carrier, phase_source_bytes, phase_test_bytes = dict(origin_rows), b'', 0, 0
     for text in sorted(paths):
         assert allow_stop or stop_requested is None, 'supervisor signal during hash'
         assert len(text.encode('utf-8')) <= 4096
@@ -87,6 +133,8 @@
         limit = (8192 if text == str(PROBE) else 65536) if text in SOURCE_PINS or text in TEST_PINS else (
             131072 if text == str(NOTE) else 32 * 1024 * 1024)
         assert stat.S_ISREG(info.st_mode) and 0 <= info.st_size <= limit
+        if text == str(PYTHON):
+            assert (info.st_dev, info.st_ino, info.st_nlink) == (16777233, 3020257, 1)
         if text in SOURCE_PINS:
             assert info.st_size <= 262144 - source_read_bytes
             assert info.st_size <= 131072 - phase_source_bytes
@@ -102,6 +150,8 @@
         try:
             opened = os.fstat(fd)
             assert stat.S_ISREG(opened.st_mode)
+            if text == str(PYTHON):
+                assert (opened.st_mode, opened.st_nlink) == (info.st_mode, 1)
             assert (opened.st_dev, opened.st_ino, opened.st_size, opened.st_mtime_ns, opened.st_ctime_ns) == identity
             size, digest = 0, hashlib.sha256()
             while size < info.st_size:
@@ -131,6 +181,9 @@
             assert (final.st_dev, final.st_ino, final.st_size, final.st_mtime_ns, final.st_ctime_ns) == identity
             current = path.lstat()
             assert (current.st_dev, current.st_ino, current.st_size, current.st_mtime_ns, current.st_ctime_ns) == identity
+            if text == str(PYTHON):
+                assert (final.st_mode, final.st_nlink) == (info.st_mode, 1)
+                assert (current.st_mode, current.st_nlink) == (info.st_mode, 1)
             assert size == info.st_size
         finally:
             os.close(fd)
@@ -142,6 +195,19 @@
         if text in TEST_PINS:
             assert (size, sha256) == TEST_PINS[text], 'nonexecuted regression pin mismatch: ' + text
         rows[text] = {'type': 'regular', 'bytes': size, 'sha256': sha256}
+        if text == str(PYTHON):
+            rows[text]['reported_executable'] = sys.executable
+            rows[text]['identity'] = list(identity) + [info.st_mode, info.st_nlink]
+    for text, row in origin_rows.items():
+        assert allow_stop or stop_requested is None, 'supervisor signal during Python final metadata'
+        current = pathlib.Path(text).lstat()
+        assert [current.st_dev, current.st_ino, current.st_mode,
+                current.st_mtime_ns, current.st_ctime_ns] == row['identity']
+        if row['type'] == 'symlink':
+            assert current.st_size == row['bytes'] and current.st_nlink == row['nlink']
+    current = PYTHON.lstat()
+    assert [current.st_dev, current.st_ino, current.st_size, current.st_mtime_ns,
+            current.st_ctime_ns, current.st_mode, current.st_nlink] == rows[str(PYTHON)]['identity']
     assert phase_source_bytes == 97203
     assert phase_test_bytes == 72052
     assert rows[str(PYTHON)]['bytes'] == 52640
@@ -204,7 +270,7 @@
 log_fd = None
 try:
     assert sys.version_info[:3] == (3, 14, 2)
-    assert pathlib.Path(sys.executable) == PYTHON
+    assert sys.executable == str(PYTHON_REPORTED)
     assert os.uname().sysname == 'Darwin' and os.uname().machine == 'arm64'
     assert os.environ.get('HOME') == HOME_VALUE and os.environ.get('TMPDIR') == TMP_VALUE
     assert len(sys.argv) == 2 and len(sys.argv[1]) == 64
~~~

**Decision and later stages remain separate.** Consult architecture on this
WHOLE origin/metadata/counter proposal before any unique-fence amendment.
After a concrete recorded source-only decision, root must separately record/
read the exact amendment assignment. A completed amended fence then needs its
own full-source DATA read/binding decision and separate actual preparation,
syntax, archive and at-most-one candidate orders; the old allowances remain
consumed. Prospective outer argv retains the canonical PYTHON,-I,-B,-c layout,
using only the separately accepted WHOLE revised source and matching hash.
No allowance, automatic retry or LMDB source fix follows from this note.

This is Darwin/arm64 diagnostic input identity only. Actual portable Elixir
modules/probe/tests and proposed ownership-row guards acquire no Homebrew/
macOS/platform branch. Linux still requires its separately approved executable/
path/hash/environment and actual supported>=1.20 core plus process/pipe/core-limit/
cleanup controls, same15 comparisons and full source/native/public profiles.
macOS requires those full project/native/public profiles too. Both remain
mandatory. Real pure behavior RED before LMDB-only assignment, real public
workflow RED before the other4 source seams, historical empty-worker cleanup
and all frozen source/parity/profile gates remain open. No performance/RSS,
persisted running-parent/legacy cleanup, Linux or macOS full acceptance,
product patch, RED/GREEN or completed workflow repair is claimed here.

ONLY this existing owned note changes. No unique PROGRAM-fence amendment,
new helper/file/output/log, pinned origin process query, candidate parsing/
syntax/evaluation/archive/VM/Mix/native/project/probe run, probe/test/product
edit, deps/cache/build/acquisition, commit/push or peer write/interrupt.
The complete stable proposal/carrier binding is relayed directly to root and
architecture, and this finite SOURCE consultation returns normally.



## EB-LMDB1 accepted pure RED and assigned guard; EB-SUP1g proposal

Before the edit, this owner actually read COMPLETE coordination3716-3754,
COMPLETE A-EB2 architecture6515-6684, whole current LMDB source (including
bounded reads closing an initial display truncation), both complete saved
canonical RED records, complete893-byte stdout and COMPLETE57052-byte stderr
in four bounded parts. Direct architecture/performance consultation preceded
the finite patch. This phase launches no VM or control.

### Actual RED read and one-function patch

Accepted retained RED is outer0/child1/PID95149/groupgone/accepted_red=true,
no cleanup signal or infrastructure/late cause. Actual15/13: five nil
ArgumentError Keys.worker_index_key/2 keys.ex599 plus LMDB source stacks,
eight ordered missing/empty mismatches; owned deadlines1/1000 pass.
0600/nlink1 RED log /private/tmp/ferricstore2-elixir-worker-index-red.log:
927433/device16777233/inode126551468/SHA256
4feb809c2d99b0b16372bf03422b14e8b8bc5e6fafffeed16cc16e37ee43284f.
Complete423441/503992-byte canonical records have1856 equal maps
(1841regular/13directory/2symlink), canonicalSHA256
aa923a0de75f71d7c7641bd8768a190a351d6688fd20a79f8b51dce3e19796d6.
This phase fully DATA-compared saved maps; root freshly rehashed all1856
current rows equal,77078546 content/link bytes. No own fresh inventory claim.
Complete stdout893/stderr57052 fit64KiB; stderrSHA256
47d495b920bdd682c61af1f99877798f92572c8b92f307532fbda3dab5445160.
Actual typed local1.20.4/29/17.0.5 frame/core proofs/warnings are retained.
Counters runtime153599228/source194406/test144104/carrier219354.
No Mix/native/public/ExUnit control; f100 refusal/5f2 RED consumed, log preserved.

ONLY isolated apps/ferricstore/lib/ferricstore/flow/lmdb.ex::
maybe_add_running_active_entries/3 changes. Present nonempty binary owner
guards BOTH Worker and Inflight; its actual bytes pass directly. Nil/missing/
empty/nonbinary owners use the unchanged fallback. Existing integer deadline,
normalize_ms, owned earlier deadlines, list order/prepend/fallback and every
other source byte stay. State/Due/DueAny/captured-active-timeout entries
remain independently constructed. No owner/token/schedule, child-policy
restriction, wall-clock expiry filter, helper/module/public API or dependency
is added. This is the only currently assigned production repair.

Whole old source57746/1697/SHA256
934287a267f3bbed18a2b57e85fbb44de6b2ab40e101df741eac9d2dd2a47ccb;
whole actual new57837/1700/SHA256
fdb703a7bff9fa5dcd260eb70823f067b92f5dd457afbb1d8cadfb4a530edb58.
Region1621 through the two LF separators before terminal_index_count_key
includes the unchanged fallback. Old659/SHA256
7abfcb408a28a87b0486e1832f9fe04c6346f6f642e2a920b840fd84803f023d;
new750/SHA256
dddb869308092b032fd1b36d2beefc6e8d356e6ee1be8cba86bb86e43359c008.
ONE-region DATA inverse restores EVERY old whole-source byte, independently
equal to git HEAD66586c5. Complete reversible actual patch:

~~~diff
--- lmdb-before.ex
+++ lmdb-after.ex
@@ -1620,15 +1620,18 @@
 
   defp maybe_add_running_active_entries(
          entries,
-         %{state: "running", lease_deadline_ms: lease_deadline_ms} = record,
+         %{
+           state: "running",
+           lease_owner: lease_owner,
+           lease_deadline_ms: lease_deadline_ms
+         } = record,
          partition_key
        )
-       when is_integer(lease_deadline_ms) do
+       when is_binary(lease_owner) and lease_owner != "" and is_integer(lease_deadline_ms) do
     score = normalize_ms(lease_deadline_ms)
     inflight_key = Ferricstore.Flow.Keys.inflight_index_key(record.type, partition_key)
 
-    worker_key =
-      Ferricstore.Flow.Keys.worker_index_key(Map.get(record, :lease_owner, ""), partition_key)
+    worker_key = Ferricstore.Flow.Keys.worker_index_key(lease_owner, partition_key)
 
     [
       {worker_key, record.id, score},
~~~

Current file pins, relative to the isolated checkout:

| Existing file | Bytes | SHA256 |
|---|---:|---|
| apps/ferricstore/lib/ferricstore/flow/lmdb.ex | 57837 | fdb703a7bff9fa5dcd260eb70823f067b92f5dd457afbb1d8cadfb4a530edb58 |
| apps/ferricstore/lib/ferricstore/flow/lmdb/value_locator.ex | 2490 | f9972d458979c284e0477c58ff4baeffbb66880f045b31023d3a91bfd6ec315f |
| apps/ferricstore/lib/ferricstore/flow/keys.ex | 32363 | bf9cbe380b9a59ef275a7efe9f94b8ee86ff10d2a7c4162eeb3a6dc24c6d8349 |
| apps/ferricstore/test/ferricstore/flow/lmdb_test.exs | 29020 | aa5434fe12bf06ac964119086f3452361b02b48e889916a748c7169c1d4ed929 |
| apps/ferricstore/test/ferricstore/flow_test/sections/flow_spawn_children_rejects_missing_wait_state_waiting_children.exs | 43032 | 86306182fc2067ea2625c1203fb8e6da17ee08a5c6cdcd5941b02be28bf0be49 |
| tools/flow-worker-index-regression.exs | 4604 | cd062f66c65cb15f4427e83a93c25c6e4226ad5bd450751998d90f6c3f061344 |

The two regression bodies retain their exact prior68/125 additions and all
older bytes; no test/probe changes in this phase. Whole142-line probe was
read again as DATA to bind the expected GREEN order. git diff --check exits0;
LMDB numstat7/4, existing tests68/0 and125/0, earlier untracked probe remain.
No formatter/parse/syntax/compile/evaluation/Mix/VM/build ran.

The other four planned writer files (FlowClaimIndexes, FlowClaimStateWrites,
FlowHistoryWrites, FlowHistoryReads) remain unmodified pending genuine public
workflow RED and separate assignment. Historical empty-worker/inflight0
cleanup is UNPROVED: skipping newly derived entries/deletes does not remove
persisted historical rows. Public persistence/reachability, transition/cancel,
migration/rebuild/compatibility need their own fixture/decision/run. This
source patch has no GREEN credit yet.

### Complete prospective GREEN source diff; NOT applied

Unique current PROGRAM remains31198 bytes/551 lines/no finalLF/SHA256
5f2c758d9f1c590109e982c9cd4a521c2b9338218ae5dda4d2165b7753f2423a.
The COMPLETE DATA diff below constructs prospective EB-SUP1g30532 bytes/
540 lines/no finalLF/SHA256
534f1a4ae833b95661bb95096c89428b69690358a4d24def256e625a11370670,
within unchanged32KiB and600-line projection. Whole text-only inverse restores
EVERY31198/5f2 byte; neither program is parsed or evaluated. Earlier complete
f100/dc541 inverse/historical refusal and consumed allowances remain.

Exact changes are: existing GREEN logfile selection; actual LMDB tuple and
source-phase equality; shell diagnostic label; all eight accepted_red tokens
become accepted_green; child0; complete15-PASS validator instead of the RED
exception/mismatch validator. Existing canonical record kind strings remain
EB-SUP1-before/EB-SUP1-after. Program/hash, GREEN filename, argv and renamed
acceptance field bind that role. No new helper/type/file/model/probe/test/
record/fallback/collector/output family is proposed.

Six unique region bindings follow (exclude surrounding context/separator
LF except the full validator's finalLF). The additional eight exact
accepted_red tokens are12 bytes each, proposed accepted_green14. Reverse
those eight token changes and the six regions in reverse order for a whole
exact inverse. Full diff includes explicit no-terminal-newline markers.

Six exact region byte counts old->new: log75->77, LMDB tuple75->75,
source equality34->34, shell label22->24, exit condition21->21,
complete validator1273->587. Full validator SHA256
c2a6262aab9dd54182355c873ea0eb5c8caafafe3a3b684376c9421e41c04b44
becomesc6dedbcb049571788e2fd2b5c98e3a7c98099a53a40140d24e2b8a6afacac9c3.
All literal bytes and every one of the eight field-token changes follow in
the complete authoritative diff; whole-source pins bind the full inverse.

~~~diff
--- EB-SUP1b-current-PROGRAM
+++ EB-SUP1g-proposed-PROGRAM
@@ -13,7 +13,7 @@
      '../Frameworks/Python.framework/Versions/3.14/bin/python3.14', 3019633),
 )
 PROBE = ROOT / 'tools/flow-worker-index-regression.exs'
-LOG = pathlib.Path('/private/tmp/ferricstore2-elixir-worker-index-red.log')
+LOG = pathlib.Path('/private/tmp/ferricstore2-elixir-worker-index-green.log')
 HOME_VALUE = '/Users/yoavgea'
 TMP_VALUE = '/var/folders/n3/4p13zj5n0kjbs8xk2qb4jq080000gn/T/'
 SOURCE_PINS = {
@@ -22,7 +22,7 @@
     str(ROOT / 'apps/ferricstore/lib/ferricstore/flow/keys.ex'):
         (32363, 'bf9cbe380b9a59ef275a7efe9f94b8ee86ff10d2a7c4162eeb3a6dc24c6d8349'),
     str(ROOT / 'apps/ferricstore/lib/ferricstore/flow/lmdb.ex'):
-        (57746, '934287a267f3bbed18a2b57e85fbb44de6b2ab40e101df741eac9d2dd2a47ccb'),
+        (57837, 'fdb703a7bff9fa5dcd260eb70823f067b92f5dd457afbb1d8cadfb4a530edb58'),
     str(PROBE): (4604, 'cd062f66c65cb15f4427e83a93c25c6e4226ad5bd450751998d90f6c3f061344'),
 }
 TEST_PINS = {
@@ -208,7 +208,7 @@
     current = PYTHON.lstat()
     assert [current.st_dev, current.st_ino, current.st_size, current.st_mtime_ns,
             current.st_ctime_ns, current.st_mode, current.st_nlink] == rows[str(PYTHON)]['identity']
-    assert phase_source_bytes == 97203
+    assert phase_source_bytes == 97294
     assert phase_test_bytes == 72052
     assert rows[str(PYTHON)]['bytes'] == 52640
     assert rows[str(E / 'bin/elixir')]['bytes'] == 6658
@@ -361,7 +361,7 @@
         'PATH': str(R / 'bin') + ':/usr/bin:/bin', 'HOME': HOME_VALUE, 'TMPDIR': TMP_VALUE,
         'LANG': 'C', 'LC_ALL': 'C', 'ERL_CRASH_DUMP_BYTES': '0', 'ERL_CRASH_DUMP': '/dev/null',
     }
-    argv = ['/bin/sh', '-c', 'ulimit -c 0 || exit 2; exec "$@"', 'eb-worker-index-red',
+    argv = ['/bin/sh', '-c', 'ulimit -c 0 || exit 2; exec "$@"', 'eb-worker-index-green',
             str(E / 'bin/elixir'), '--no-color', '--erl',
             '+S 2:2 +SDcpu 1 +SDio 1 +A 2 -noinput -boot ' + str(R / 'releases/29/start_clean'),
             '-e', expression]
@@ -378,7 +378,7 @@
     assert 2097152 - log_written >= 1048576, 'less than full 1MiB after-record log capacity'
     assert stop_requested is None, 'supervisor signal before Popen'
 except BaseException as error:
-    refusal = {'accepted_red': False, 'phase': 'before-VM',
+    refusal = {'accepted_green': False, 'phase': 'before-VM',
                'failure': type(error).__name__ + ': ' + str(error)[:1024]}
     if log_fd is not None:
         try:
@@ -459,10 +459,10 @@
           'signal': stop_requested, 'after_failure': after_failure,
           'inputs_unchanged': after_failure is None and before == after,
           'stdout': bytes(streams['stdout']).hex(), 'stderr': bytes(streams['stderr']).hex(),
-          'raw_encoding': 'hex', 'stream_bytes_seen': seen, 'accepted_red': False}
+          'raw_encoding': 'hex', 'stream_bytes_seen': seen, 'accepted_green': False}
 try:
     assert failure is None and after_failure is None and group_gone and stop_requested is None
-    assert before == after and child is not None and child.returncode == 1
+    assert before == after and child is not None and child.returncode == 0
     stdout = bytes(streams['stdout']).decode('utf-8', 'strict')
     stderr = bytes(streams['stderr']).decode('utf-8', 'strict')
     lines = stdout.splitlines()
@@ -504,28 +504,17 @@
         assert len(proof['loaded_md5']) == 32 and all(c in '0123456789abcdef' for c in proof['loaded_md5'])
         normalized_proofs.append(proof)
     frame['proofs'] = normalized_proofs
-    assert lines[1:] == ['PASS owned running', 'PASS owned earlier-deadline running', 'cases=15 failures=13']
-    assert stdout.endswith('\n') and 'INFRA:' not in stderr
-    expected_failures = [('unowned child-policy running', True)]
+    expected_passes = ['PASS unowned child-policy running',
+                       'PASS owned running', 'PASS owned earlier-deadline running']
     for owner in ('nil', 'missing', 'empty'):
         for due_any in ('false', 'true'):
             for scheduled in ('false', 'true'):
-                expected_failures.append(('unowned ' + owner + ' due_any=' + due_any +
-                                          ' scheduled=' + scheduled, owner == 'nil'))
-    error_lines = stderr.splitlines()
-    positions = [number for number, line in enumerate(error_lines) if line.startswith('FAIL ')]
-    assert len(positions) == 13
-    for occurrence, (name, is_nil) in enumerate(expected_failures):
-        position = positions[occurrence]
-        expected = 'FAIL ' + name + (':' if is_nil else ': unexpected entries')
-        assert error_lines[position] == expected
-        if is_nil:
-            end = positions[occurrence + 1] if occurrence + 1 < len(positions) else len(error_lines)
-            block = '\n'.join(error_lines[position + 1:end])
-            assert 'Ferricstore.Flow.Keys.worker_index_key/2' in block
-            assert 'flow/keys.ex:' in block and 'flow/lmdb.ex:' in block
+                expected_passes.append('PASS unowned ' + owner + ' due_any=' + due_any +
+                                       ' scheduled=' + scheduled)
+    assert lines[1:] == expected_passes + ['cases=15 failures=0']
+    assert stdout.endswith('\n') and 'INFRA:' not in stderr and 'FAIL ' not in stderr
     result['runtime_frame'] = frame
-    result['accepted_red'] = True
+    result['accepted_green'] = True
 except BaseException as error:
     result['validation_failure'] = type(error).__name__ + ': ' + str(error)[:1024]
 try:
@@ -533,19 +522,19 @@
                          'runtime_read_bytes': runtime_read_bytes, 'source_read_bytes': source_read_bytes,
                          'test_read_bytes': test_read_bytes, 'carrier_read_bytes': carrier_read_bytes})
 except BaseException as error:
-    result['accepted_red'] = False
+    result['accepted_green'] = False
     result['log_failure'] = type(error).__name__ + ': ' + str(error)[:1024]
 finally:
     try:
         os.close(log_fd)
     except BaseException as error:
-        result['accepted_red'] = False
+        result['accepted_green'] = False
         result['log_close_failure'] = type(error).__name__ + ': ' + str(error)[:1024]
-print(json.dumps({'accepted_red': result['accepted_red'], 'pid': result['pid'],
+print(json.dumps({'accepted_green': result['accepted_green'], 'pid': result['pid'],
                   'exit': result['exit'], 'group_gone': result['group_gone'],
                   'log': str(LOG), 'log_bytes': log_written,
                   'failure': result.get('failure'), 'after_failure': result.get('after_failure'),
                   'validation_failure': result.get('validation_failure'),
                   'log_failure': result.get('log_failure'),
                   'log_close_failure': result.get('log_close_failure')}, separators=(',', ':')))
-sys.exit(0 if result['accepted_red'] else 2)
\ No newline at end of file
+sys.exit(0 if result['accepted_green'] else 2)
\ No newline at end of file
~~~

Four existing functions only: request_stop/write_record/finish_group retain
their87/717/1036-byte bodies; snapshot_inputs stays156/9340, changing only
97203 to97294. Excluding its blank separatorLF, oldb16ce07d1aaf9db0be0566cddbd84f3193f258c49249ed2d26f76ee03021a878
becomes5b8eaa5df14f4eec51651a4a26ee192591e67769241cc7edcd8579fe8892ae7d.
Origin expression3647/16044ba7c8e7680e33cd4b15bd18cb7647378f1f8f4f6cd1c0f05c0643869e30,
BOTH boundaryLF bytes and typed anonymous unique10/3 pairs parser/proofs
remain byte-identical. No fifth function.

After its sole bounded typed runtime frame, GREEN stdout must be exactly:

~~~text
PASS unowned child-policy running
PASS owned running
PASS owned earlier-deadline running
PASS unowned nil due_any=false scheduled=false
PASS unowned nil due_any=false scheduled=true
PASS unowned nil due_any=true scheduled=false
PASS unowned nil due_any=true scheduled=true
PASS unowned missing due_any=false scheduled=false
PASS unowned missing due_any=false scheduled=true
PASS unowned missing due_any=true scheduled=false
PASS unowned missing due_any=true scheduled=true
PASS unowned empty due_any=false scheduled=false
PASS unowned empty due_any=false scheduled=true
PASS unowned empty due_any=true scheduled=false
PASS unowned empty due_any=true scheduled=true
cases=15 failures=0
~~~

Require terminalLF/child0/no extra stdout/no FAIL or INFRA stderr marker;
bounded compiler warnings remain visible. Unchanged actual15 comparisons:
unscheduled state42; scheduled due_any=false active110,Due77,state42;
true active110,DueAny77,Due77,state42. Both owned deadline1/1000 lists
retain Worker,Inflight,state scores/order. No mirrored test-side projector.

### Exact prospective runtime scope and unexecuted gates

Later outer argv: canonical pinned PYTHON,-I,-B,-c,WHOLE accepted source,
matching534f1a4a hash. All18 runtime pins/exact HOME/TMP remain, Python
Frameworks52640/2cbb endpoint, reported opt spelling,28/59-byte links,
13parent/2link rows,15-slot reservation and87-byte actual debit per snapshot.
Final parent/link/endpoint checks and stable-input/TOCTOU stay: os.readlink
has no read-size parameter, so no physical allocation bound under a race.

Child structured argv remains /bin/sh,-c,'ulimit -c 0 || exit 2; exec "$@"',
eb-worker-index-green, E/bin/elixir,--no-color,--erl,
'+S 2:2 +SDcpu 1 +SDio 1 +A 2 -noinput -boot R/releases/29/start_clean',
-e,WHOLE unchanged3647-byte expression. E/R/HOME/TMP and all seven clean env
keys remain exact: PATH=R/bin:/usr/bin:/bin,HOME=/Users/yoavgea,same TMPDIR,
LANG=C,LC_ALL=C,ERL_CRASH_DUMP_BYTES=0,ERL_CRASH_DUMP=/dev/null.

Source/probe becomes97294 per snapshot/194588 before+after (+91/+182 actual
patch bytes). Tests72052/144104 remain HASHED, not executed. Caps unchanged:
source/test128KiB per phase/256KiB shared, sources64KiB/probe8KiB, carrier
128KiB each/256KiB shared, runtime4096 paths/4096 UTF8 path bytes/32MiB
per file/256MiB shared,512KiB map. Future carrier and complete fresh snapshot
need exact DATA binding; historical1856 rows are not a new fixed future-count
assertion. New before/after snapshots must completely agree.

Only existing named prospective output is
/private/tmp/ferricstore2-elixir-worker-index-green.log, absent now:
exclusive0600/no-follow,two canonical JSONLF<=1MiB each/2MiB total,
fsync/close/full1MiB remaining before Popen. RED is never opened/truncated.
Before retains whole source/hash/argv/env/map; after maps/raw hex/frame/counters.

Session/group PGID/SID=PID,stdinDEVNULL,10s measured,TERM2s/KILL2s/reap/groupgone,
64KiB each stream and all five final bounded causes stay. Popen creation may
be uninterruptible but elapsed creation still counts toward later deadline.
Ordinary allocation/intake limitations,no hard processRSS/early durablePID
remain. Any overflow/input mutation/timeout/signal/origin/validation/log/
close/cleanup/reap failure refuses outer2; no retry/cap increase.
Acceptance needs outer0/child0,complete records,accepted_green=true,
unchanged inputs/groupgone and no late cause together. Flag alone is
insufficient after late write/close failure. No new artifact/stub/output.

Architecture must decide this WHOLE precise amendment/inverse before any
PROGRAM change. Root must separately actual-read and assign that exact
note-source edit. Full actual amended source then needs independent whole
DATA review and concrete separately assigned prep/syntax/archive/at-most-one
candidate stages. No such source amendment/preparation/run is authorized by
this finite patch task. The current fence still describes consumed RED and
cannot match the patched LMDB pin; it is retained history, not a fallback.

Both Linux AND macOS remain mandatory. Production/probe/regression sources
have no OS/path branch. This Darwin/arm64 supervisor is a local diagnostic;
Linux needs separately accepted>=1.20 runtime/path/hash/env/process/pipe/
core-limit/cleanup plus same15 comparisons/full source/native/public profiles.
macOS also still needs full original project/native/public profiles and four
unexecuted ExUnit declarations. Pure GREEN would certify only this LMDB
projection patch; public SPAWN/terminal/state-writer RED, other four seams,
legacy cleanup/full source/regression profiles/frozen parity remain open.

This phase changes ONLY assigned LMDB and appends this existing owned note.
No other source/test/probe/config edit, helper/artifact/parse/formatter/compile/
evaluation/VM/Mix/native/build/deps/cache/acquisition/GREEN/log/capture/commit/
push or peer write. Prior109677-byte/1909-line/d355ebee813cff7bbe536e300cb102bc68da82bcd9199abdfbcd27dfcd313a60
carrier remains unchanged prefix: removing this append restores EVERY byte.
Current whole carrier/unchanged PROGRAM pins are published by complete DATA
readback after append, then relayed to root/architecture/performance.

## EB-SUP1g actual note-only handoff

Read complete A-EB7/root order first; only eight reviewed fence hunks applied. Fixed source: 30,532 bytes / 540 lines / no final LF / SHA256
534f1a4ae833b95661bb95096c89428b69690358a4d24def256e625a11370670.
Old complete 130,147-byte carrier preserved transiently FIRST.
Removing only this handoff and reversing the fence restores EVERY old
carrier byte (2c431cb6a037e7dd8a59d9602c367f67664d69ec510c5e160ee14955d2961eaa)
and every 31,198/5f2 source byte. Four functions, raw origin and history stay.
Fresh six pins match EB-LMDB1; GREEN absent. Separate source/probe
97,294/194,588 and nonexecuted-test 72,052/144,104 budgets retain EACH
128KiB per snapshot / 256KiB across two. Old RED/f100 allowances are consumed.

Request new four-stage consultation: DATA preparation, exact compile-only
stub/argv, existing-trace archive and at most one source-bound GREEN candidate.
Each needs its own root order. No preparation/syntax/archive/evaluation/VM,
log/output, probe/test/product edit or GREEN ran; only this note changed.
Linux AND macOS full original/native/public/ExUnit/parity and legacy gates
remain open. Final carrier/source bindings relayed externally.

## EB-GOV-R1 independent native-readiness finding — 2026-10-11

Finite note-only review complete. The mixed-reclaim declaration remains AUTHORED
ONLY; its unchanged-production native RED and reservation repair/GREEN are open.
The unresolved predecessor is a separately accepted, source-bound native project
construction/startup/output/cleanup batch for the isolated bugfix checkout.
Source acceptance of this declaration supplies no project execution evidence.

I actually read the complete current governance test (all1639 lines), complete
ClaimDueAPI (all1381 lines), complete FlowCase/config/test_helper/Mix definitions,
and the relevant complete claim-pool, candidate/binding, credit-release, cache
dispatch, shard-readiness/flush, application-start/stop and data-cleanup scopes.
Other production whole-file hashing below is custody, not a whole semantic audit.
Fresh protected regular/nlink1/O_NOFOLLOW reads with stable nine-field FD/path
rows, EOF and checked close (receipt0d67f4) confirm:

| Isolated existing path | Bytes | SHA256 |
|---|---:|---|
| apps/ferricstore/test/ferricstore/flow_governance_limit_correctness_test.exs | 55516 | 59c9737a17d11636865e51efd20587ac0b6344694c3c13b80ce10cf0ea7fad99 |
| apps/ferricstore/lib/ferricstore/flow/claim_due_api.ex | 45582 | 945fcecb5218961b8a3186e1ed2eb1e85f585a228f6203215b8245772f1139fa |
| apps/ferricstore/lib/ferricstore/raft/state_machine/sections/flow_claim_due.ex | 47094 | 49b4c2337a49db5a3caaf41b658060011e69ef38765dd383f0ac13a208d27751 |
| apps/ferricstore/lib/ferricstore/raft/state_machine/sections/flow_retention_values.ex | 24654 | b5ebcfd24c7fb7279386767b492f19751cbdfc57dd6d9cccd8532250065fe0a7 |
| apps/ferricstore/lib/ferricstore/flow/governance/credit_lease.ex | 13442 | 38d2d7e372f62bd5f63a76a664901870cf471bba99759bd6167b4f6d07f937af |
| apps/ferricstore/lib/ferricstore/flow/governance/limit_cache.ex | 81336 | 492308b0ea5673161f36f054f780b3ab0f57944f6bbf59d5a7c60b9c84054f38 |

Removing ONLY insertion[13372,15929),2557 bytes/70LF,
SHAe402e3e65f72c50ac7e15e1fdb86f37e5a87b056492137141ae9c51cdbcbdf0b,
recovers EVERY52959 old byte/SHA03e5368a0f6fc82a5ba9f9a024d8b5a04f9c07dc7a7f623dda1975ce3841a7c3.
These identities match ROOT's saved EB-GOV-R1-SOURCE1 reciprocal closure
(coordination24959–25075). No reservation causal source changed. The separate
reviewed LMDB owner guard remains part of the isolated production tree.

### Concrete existing bindings required before the minimal native reproduction

1. Use `/Users/yoavgea/repos/ferricstore2-elixir-bugfix`, its pinned66586c5
   source and current test above, with MIX_ENV=test and its own umbrella inputs.
   apps/ferricstore/mix.exs:10–15 binds `_build`, `deps`, `mix.lock`, configuration
   and Elixir~>1.20; :test compiles the actual test/support at:23. Bind complete
   dependency/SCM/native/build/config/origin inputs and unchanged production
   before/after. Saved coordination24413 reported missing isolated deps/_build/
   test-results; this review did not repeat that environment check.
2. Existing native CI source `.github/workflows/test.yml:10–14,51–56,202–207`
   declares MIX_ENV=test, FERRICSTORE_BUILD_NIF=1, Elixir1.20.4/OTP29.0.5.
   Its Ubuntu/macOS build jobs separately build Bitcask, WAL and native-protocol
   NIFs, then warnings-as-errors source compilation. `bitcask/nif.ex:6–20` and
   `wal_nif.ex:19–33` select real RustlerPrecompiled NIF2.16 artifacts for Linux
   and Darwin. Artifact declarations do not prove available/loadable NIFs;
   concrete build/acquisition/origin/lifecycle still needs its own acceptance.
3. Preserve the actual Ferricstore.Test.FlowCase:8–23 setup: async:false,
   :flow tag, wait_shards_alive and flush_all_keys. ShardHelpers:246–291 performs
   real default-instance WARaft readiness and storage flush; :644–694 is the
   shard wait. Backend.selected/0 is fixed :waraft. Application.start:52–339
   creates real storage/writers/default context and starts WARaft. Config/test:
   13–37 keeps port0 listeners, crypto-generated test root and four shards;
   :54 disables the scheduler; current test:13–20 disables governance caching.
   LimitCache:539–546 and:2765–2767 consequently use actual LimitStore spend.
4. Bind the generated data root, all native/build writes, ephemeral sockets,
   JUnit `test-results/` (config/test:89–95) and actual audit directory
   `/tmp/ferricstore_test_audit/` (audit_formatter:16–34). Its init truncates
   the selected app log. DataDirLifecycle:33–76 stops applications and removes
   eligible generated roots on normal VM exit; :41–43 discards cleanup errors.
   This is no forced-exit/timeout cleanup proof. Process/output bounds and actual
   complete cleanup need the separate concrete launch decision. docs/testing:7
   requires one full FerricStore command per worktree at a time.

The already recorded future argv remains, from the isolated umbrella root:

~~~text
mix test apps/ferricstore/test/ferricstore/flow_governance_limit_correctness_test.exs:409 --seed 0 --max-cases 1 --timeout 120000
~~~

Require the actual named declaration to execute with native storage/Raft/public
calls and reach uniqueness at:455 after ALL phase/version/membership/metadata
controls:413–454. No-match/exclusion/setup/compiler/NIF/socket/load/cleanup
failures cannot count as this behavioral RED. Preserve the same declaration,
profile and source bindings for later separately selected repair/GREEN.

### Source sensitivity, retained holds and independent host limits

No new blocking defect was found in the authored declaration. Limit4/ratio25
selects reclaim1, normal1, fill2 through ClaimDueAPI:472–485. All phases retain
the one spent pool (:498–527); FlowClaimDue:294–303 reinstalls it per apply,
and FlowRetentionValues:463–624 binds its head before key validation. The real
source therefore predicts `[r1,r1,r1,r2]`, and release_ids:647–660 predicts r3/r4
refunded. Those are source predictions; no values or owner counts were observed.

The existing tail at test:460–476 is a composed completion-plus-release check.
It reads in_use only AFTER both calls. A completion release that does nothing,
followed by the explicit identified release freeing one credit, can also yield3.
Thus this tail alone cannot certify completion release or replay idempotence.
The independent uniqueness assertion:455 remains sensitive to the inspected bug.
No assertion/control or source amendment is proposed by this finite review.

Saved actual Darwin A-SUP5 core acceptance (coordination2608–2641) and pure LMDB
RED13/15fail→GREEN15/15 (4213–4242) certify their narrow recorded boundaries.
The four worker-index ExUnit declarations, governance case and original native/
public profiles remain unexecuted. Earlier Mix PubSub TCP:eperm (861–870) was
Elixir1.19.5/OTP28; the supported-core cold-crypto assertion refusal (2308–2321)
also remains historical. Neither is a new supported-project startup observation.

Corrected R-MIX1-R1 three-body SOURCE acceptance (11714–11724; parity7255–7310)
does not close its unexecuted loader/seed/argv/compile/stage/receipt chain or
original setup. Its positive actual C, source origins, ROOT external pre/post
original Git receipts and cleanup gates remain. Separately, bounded R1's final
Darwin attempt failed on missing-docs before assertions (9038–9118); finite G1
closed at268435105/268435456 (11607). Those scopes remain frozen/failed as saved;
later ordinary Rust successes supply no Elixir/native resource certificate.

Linux AND macOS require independent native construction, this genuine RED,
later selected GREEN and full mandatory profiles. Saved desktop-linux socket
access denial and default `/var/run/docker.sock` absence (27436,27873–27876)
are distinct executor findings, with no Linux execution credit. Linux's required
io_uring job (:171–178) is a further profile gate; macOS pure acceptance cannot
close it. No environment check, project job, VM, parser/formatter/compiler/native
run, output artifact, source/test patch, new artifact, commit/push or agent
spawn/interruption occurred here. ONLY this append changes the owned note; all130679 predecessor
bytes/SHA df34b8a99fd6d3ace8b28bdfae93f37d1f9415b478b01c086aeea1e2935ecdd6
and all frozen/original inputs are preserved. This opens no consumed allowance.
