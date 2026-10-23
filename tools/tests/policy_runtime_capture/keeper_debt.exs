defmodule Ferricstore.PolicyRuntimeCaptureKeeperDebtTest do
  use ExUnit.Case, async: false
  alias Ferricstore.PolicyRuntimeCapture, as: Capture
  alias Ferricstore.PolicyRuntimeCaptureTest, as: SourceTest
  alias Ferricstore.TermCodec

  # These are the legacy financial banks until the bridge protects their Native R mirror.
  # The local upper is a guard, never an invoice or a public entry/terminal fee.
  @seed_words 132_644
  @keeper_words 17_956
  @tuple_words 12
  @local_upper 8_192
  # One issued K:8*17956+2*68611, plus four Native ok/report tuples.
  @keeper_pins 280_966

  test "keeper_query_context_failure_retains_issued_debt_before_retry" do
    SourceTest.bootstrap!()
    controller = self()
    limits = %{SourceTest.limits() | pinned_bytes: 536_870_912}
    empty = %{atoms: [], exports: []}
    seed = :binary.copy("s", 524_288)
    source = :binary.copy("x", 65_536)
    encoded = TermCodec.encode(source)
    donor = %{encoded: encoded, source_term: source}
    assert :binary.referenced_byte_size(seed) == 524_288
    assert :binary.referenced_byte_size(source) == 65_536
    assert byte_size(encoded) == 65_542

    with_export_fixture(fn fixture, original ->
      queries = %{atoms: [], exports: [{Atom.to_string(fixture), "probe", 0}]}

      with_session([%{encoded: TermCodec.encode(seed), source_term: seed}], queries, limits, fn
        keeper, first ->
          assert first.queries.exports[{Atom.to_string(fixture), "probe", 0}]

          worker = fn ->
            receive do
              :attempt ->
                rejected = Capture.extend_epoch(keeper, [donor], empty, first.identity)
                send(controller, {:keeper_rejected, self(), rejected})

                receive do
                  :retry ->
                    retried = Capture.extend_epoch(keeper, [], empty, first.identity)
                    send(controller, {:keeper_retried, self(), retried})

                    receive do
                      :stop -> :ok
                    end
                end
            end
          end

          with_owner(worker, fn caller ->
            with_keeper_trace(keeper, fn ->
              :code.purge(fixture)
              assert :code.delete(fixture)
              refute :erlang.function_exported(fixture, :probe, 0)
              send(caller, :attempt)
              assert_receive {:keeper_rejected, ^caller, rejected}, 5_000
              assert rejected == {:error, :runtime_context_changed}
              {entry, tail, events} = keeper_entry(keeper)
              s = entry.admission.decoded_words
              t = tail.admission.decoded_words
              assert s >= @seed_words and @keeper_words + @local_upper < s

              assert t >= s + @keeper_words + @tuple_words,
                     "entry=#{s} tail=#{t} error=#{inspect(rejected)}"

              assert t < 2 * s
              assert tail.admission.pinned_bytes >= entry.admission.pinned_bytes + @keeper_pins

              grants =
                for {:trace, ^keeper, :send, {_, {:ok, _, bank, _}}, ^caller} <- events, do: bank

              assert [grant] = grants
              assert s >= grant.decoded_words + @keeper_words
              assert {:trace, keeper, :call, {TermCodec, :encode, [source]}} in events
              assert {:trace, keeper, :return_from, {TermCodec, :encode, 1}, encoded} in events
              assert tail.capture == first and tail.donors == entry.donors
              assert tail.anchors == entry.anchors and tail.queries == entry.queries
              assert tail.observed == entry.observed
              :code.purge(fixture)

              assert {:module, ^fixture} =
                       :code.load_binary(fixture, ~c"keeper_debt_fixture", original)

              assert :erlang.function_exported(fixture, :probe, 0)
              send(caller, :retry)
              assert_receive {:keeper_retried, ^caller, {:ok, next}}, 5_000
              assert next.parent == first.identity and next.session_id == first.session_id
              assert next.donor_hashes == first.donor_hashes and next.queries == first.queries
            end)
          end)
      end)
    end)
  end

  test "keeper_observe_failure_retains_issued_debt_before_retry" do
    SourceTest.bootstrap!()
    controller = self()
    limits = %{SourceTest.limits() | pinned_bytes: 536_870_912, output_bytes: 16_384}
    queries = %{atoms: [], exports: []}
    seed = :binary.copy("s", 524_288)
    source = :binary.copy("x", 65_536)
    encoded = TermCodec.encode(source)
    donor = %{encoded: encoded, source_term: source}
    key_a = :binary.copy("a", 9_000)
    key_b = :binary.copy("b", 9_000)
    map_a = TermCodec.encode(%{key_a => nil})
    map_b = TermCodec.encode(%{key_b => nil})
    assert :binary.referenced_byte_size(seed) == 524_288
    assert :binary.referenced_byte_size(source) == 65_536
    assert :binary.referenced_byte_size(key_a) == 9_000
    assert :binary.referenced_byte_size(key_b) == 9_000
    assert byte_size(TermCodec.encode(key_a)) == 9_006
    assert byte_size(TermCodec.encode(key_b)) == 9_006
    assert 9_006 <= limits.output_bytes and 18_012 > limits.output_bytes

    with_session(
      [%{encoded: TermCodec.encode(seed), source_term: seed}, map_a],
      queries,
      limits,
      fn
        keeper, first ->
          worker = fn ->
            receive do
              :attempt ->
                rejected = Capture.extend_epoch(keeper, [donor, map_b], queries, first.identity)
                send(controller, {:keeper_rejected, self(), rejected})

                receive do
                  :retry ->
                    retried = Capture.extend_epoch(keeper, [], queries, first.identity)
                    send(controller, {:keeper_retried, self(), retried})

                    receive do
                      :stop -> :ok
                    end
                end
            end
          end

          with_owner(worker, fn caller ->
            with_keeper_trace(keeper, fn ->
              send(caller, :attempt)
              assert_receive {:keeper_rejected, ^caller, rejected}, 5_000
              assert rejected == {:error, {:limit, :output_bytes}}
              {entry, tail, events} = keeper_entry(keeper)
              s = entry.admission.decoded_words
              t = tail.admission.decoded_words
              assert s >= @seed_words and @keeper_words + @local_upper < s

              assert t >= s + @keeper_words + @tuple_words,
                     "entry=#{s} tail=#{t} error=#{inspect(rejected)}"

              assert t < 2 * s
              assert tail.admission.pinned_bytes >= entry.admission.pinned_bytes + @keeper_pins

              grants =
                for {:trace, ^keeper, :send, {_, {:ok, _, bank, _}}, ^caller} <- events, do: bank

              assert [grant] = grants
              assert s >= grant.decoded_words + @keeper_words
              assert {:trace, keeper, :call, {TermCodec, :encode, [source]}} in events
              assert {:trace, keeper, :return_from, {TermCodec, :encode, 1}, encoded} in events
              assert {:trace, keeper, :call, {TermCodec, :encode, [key_a]}} in events
              refute {:trace, keeper, :call, {TermCodec, :encode, [key_b]}} in events
              assert tail.capture == first and tail.donors == entry.donors
              assert tail.anchors == entry.anchors and tail.queries == entry.queries
              assert tail.observed == entry.observed
              send(caller, :retry)
              assert_receive {:keeper_retried, ^caller, {:ok, next}}, 5_000
              assert next.parent == first.identity and next.session_id == first.session_id
              assert next.donor_hashes == first.donor_hashes and next.orders == first.orders
            end)
          end)
      end
    )
  end

  defp with_session(donors, queries, limits, run) do
    assert {:ok, keeper, first} = Capture.capture_epoch(donors, queries, nil, limits)

    try do
      run.(keeper, first)
    after
      if Process.alive?(keeper) do
        if Process.info(keeper, :status) == {:status, :suspended},
          do: :erlang.resume_process(keeper)

        :erlang.trace(keeper, false, [:call, :send, :receive])
      end

      assert :ok = Capture.close_epoch(keeper)
    end
  end

  defp with_owner(worker, run) do
    {caller, monitor} = spawn_monitor(worker)

    try do
      assert :erlang.trace(caller, true, [:send, :receive, {:tracer, self()}]) == 1
      result = run.(caller)
      events = fenced_events(caller)

      extensions =
        for {:trace, ^caller, :send, {:capture, ^caller, ref, {:extend, _, _, _}}, keeper} <-
              events,
            do: {ref, keeper}

      assert [{ref, keeper}, {next_ref, next_keeper}] = extensions
      assert next_keeper == keeper and next_ref != ref
      ack = Enum.find_index(events, &match?({:trace, ^caller, :receive, {^ref, :settled}}, &1))

      returned =
        Enum.find_index(
          events,
          &match?({:trace, ^caller, :send, {:keeper_rejected, ^caller, _}, _}, &1)
        )

      budgets =
        for {{:trace, ^caller, :send, {:capture, ^caller, _, {:budget, _}}, ^keeper}, i} <-
              Enum.with_index(events),
            do: i

      assert [_, retry] = budgets
      assert is_integer(ack) and is_integer(returned) and ack < returned and returned < retry
      result
    after
      if Process.alive?(caller) do
        if Process.info(caller, :status) == {:status, :suspended},
          do: :erlang.resume_process(caller)

        :erlang.trace(caller, false, [:send, :receive])
        Process.exit(caller, :kill)
      end

      assert_receive {:DOWN, ^monitor, :process, ^caller, _reason}, 5_000
    end
  end

  defp with_keeper_trace(keeper, run) do
    targets = [{Capture, :extend, 4}, {Capture, :keep, 1}, {TermCodec, :encode, 1}]

    try do
      for target <- targets do
        pattern =
          if target == {TermCodec, :encode, 1}, do: [{:_, [], [{:return_trace}]}], else: true

        assert :erlang.trace_pattern(target, pattern, [:local]) == 1
      end

      assert :erlang.trace(keeper, true, [:call, :send, {:tracer, self()}]) == 1
      run.()
    after
      if Process.alive?(keeper), do: :erlang.trace(keeper, false, [:call, :send])
      for target <- targets, do: :erlang.trace_pattern(target, false, [:local])
    end
  end

  defp fenced_events(tracee) do
    fence = :erlang.trace_delivered(tracee)
    assert_receive {:trace_delivered, ^tracee, ^fence}, 5_000
    take_events(tracee, [])
  end

  defp take_events(tracee, acc) do
    receive do
      {:trace, ^tracee, _, _} = event -> take_events(tracee, [event | acc])
      {:trace, ^tracee, _, _, _} = event -> take_events(tracee, [event | acc])
    after
      0 -> Enum.reverse(acc)
    end
  end

  defp keeper_entry(keeper) do
    {entry, tail, events} =
      Stream.repeatedly(fn ->
        receive do
          {:trace, ^keeper, _, _} = event -> event
          {:trace, ^keeper, _, _, _} = event -> event
        after
          5_000 -> flunk("real keeper entry and first tail trace did not arrive")
        end
      end)
      |> Enum.reduce_while({nil, []}, fn event, {entry, acc} ->
        next = [event | acc]

        case event do
          {:trace, ^keeper, :call, {Capture, :extend, [state, _, _, _]}} when is_nil(entry) ->
            {:cont, {state, next}}

          {:trace, ^keeper, :call, {Capture, :keep, [state]}} when not is_nil(entry) ->
            {:halt, {entry, state, Enum.reverse(next)}}

          _ ->
            {:cont, {entry, next}}
        end
      end)

    {entry, tail, events ++ fenced_events(keeper)}
  end

  defp with_export_fixture(run) do
    before = Code.compiler_options(ignore_module_conflict: true)

    try do
      [{fixture, original}] =
        Code.compile_quoted(
          quote do
            defmodule Ferricstore.PolicyRuntimeCaptureKeeperDebtFixture do
              def probe, do: :ok
            end
          end
        )

      try do
        run.(fixture, original)
      after
        :code.purge(fixture)
        assert {:module, ^fixture} = :code.load_binary(fixture, ~c"keeper_debt_fixture", original)
      end
    after
      Code.compiler_options(before)
    end
  end
end
