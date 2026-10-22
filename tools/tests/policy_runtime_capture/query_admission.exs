defmodule Ferricstore.PolicyRuntimeCaptureQueryAdmissionTest do
  use ExUnit.Case, async: false
  alias Ferricstore.PolicyRuntimeCapture, as: Capture
  alias Ferricstore.PolicyRuntimeCapture.Input
  alias Ferricstore.PolicyRuntimeCapture.Native
  alias Ferricstore.PolicyRuntimeCaptureTest, as: SourceTest
  alias Ferricstore.TermCodec

  test "combines_prior_input_and_query_builder_budgets" do
    SourceTest.bootstrap!()
    name = :binary.copy("q")
    queries = %{atoms: [name], exports: []}
    limits = SourceTest.limits()
    assert byte_size(name) == 1
    assert :binary.referenced_byte_size(name) == 1
    assert Atom.to_string(:q) == name

    with_captures(
      [Map.put(limits, :decoded_words, 400), Map.put(limits, :pinned_bytes, 3_500), limits],
      queries,
      [],
      fn results ->
        statuses =
          Enum.map(results, fn
            {:ok, _keeper, _captured} -> :ok
            error -> error
          end)

        assert Enum.at(statuses, 0) == {:error, {:limit, :decoded_words}}, inspect(statuses)
        assert Enum.at(statuses, 1) == {:error, {:limit, :pinned_bytes}}, inspect(statuses)
        assert Enum.at(statuses, 2) == :ok
        {:ok, _keeper, captured} = Enum.at(results, 2)
        assert captured.donor_hashes == []
        assert captured.queries == %{atoms: %{name => true}, exports: %{}}
        assert is_boolean(captured.queries.atoms[name])
        assert captured.query_hash == :crypto.hash(:sha256, TermCodec.encode(queries))
        assert queries == %{atoms: [name], exports: []}
        assert byte_size(name) == 1
        assert :binary.referenced_byte_size(name) == 1
      end
    )
  end

  test "accounts_original_limits_owners_across_capture_and_extension" do
    abi = SourceTest.bootstrap!()
    bytes = :binary.copy("x", 600_000)
    queries = %{atoms: [], exports: []}
    defaults = SourceTest.limits()
    extra = Map.put(defaults, :ignored_capture_budget, bytes)
    low = Map.put(extra, :pinned_bytes, 128_000)
    clean = Map.put(defaults, :pinned_bytes, 128_000)
    query_hash = :crypto.hash(:sha256, TermCodec.encode(queries))
    assert byte_size(bytes) == 600_000
    assert :binary.referenced_byte_size(bytes) == 600_000

    with_captures([low, clean, extra], queries, [], fn results ->
      statuses =
        Enum.map(results, fn
          {:ok, _keeper, _captured} -> :ok
          error -> error
        end)

      assert Enum.at(statuses, 0) == {:error, {:limit, :pinned_bytes}}, inspect(statuses)
      assert Enum.at(statuses, 1) == :ok, inspect(statuses)
      assert Enum.at(statuses, 2) == :ok, inspect(statuses)
      {:ok, _clean_keeper, clean_capture} = Enum.at(results, 1)
      {:ok, keeper, first} = Enum.at(results, 2)

      for captured <- [clean_capture, first] do
        assert captured.donor_hashes == []
        assert captured.hashes == []
        assert captured.queries == %{atoms: %{}, exports: %{}}
        assert captured.abi == abi
        assert captured.query_hash == query_hash
      end

      assert extra == Map.put(defaults, :ignored_capture_budget, bytes)
      assert low == Map.put(extra, :pinned_bytes, 128_000)
      assert clean == Map.put(defaults, :pinned_bytes, 128_000)
      assert byte_size(bytes) == 600_000
      assert :binary.referenced_byte_size(bytes) == 600_000
      assert {:ok, next} = Capture.extend_epoch(keeper, [], queries, first.identity)
      assert next.parent == first.identity
      assert next.session_id == first.session_id

      for field <- [
            :abi,
            :query_hash,
            :donor_hashes,
            :queries,
            :hashes,
            :orders,
            :atoms,
            :nodes,
            :exports,
            :local_funs
          ] do
        assert Map.fetch!(next, field) == Map.fetch!(first, field)
      end

      assert extra == Map.put(defaults, :ignored_capture_budget, bytes)
      assert low == Map.put(extra, :pinned_bytes, 128_000)
      assert clean == Map.put(defaults, :pinned_bytes, 128_000)
      assert byte_size(bytes) == 600_000
      assert :binary.referenced_byte_size(bytes) == 600_000
    end)
  end

  test "preserves_ignored_control_depth_across_capture_and_extension" do
    abi = SourceTest.bootstrap!()
    deep = Enum.reduce(1..128, nil, fn _, term -> {term} end)
    defaults = SourceTest.limits()
    limits = Map.put(defaults, :ignored_capture_budget, deep)
    queries = %{atoms: [], exports: []}
    query_hash = :crypto.hash(:sha256, TermCodec.encode(queries))
    assert Enum.reduce(1..128, deep, fn _, {inner} -> inner end) == nil
    assert map_size(defaults) == 8
    assert defaults.depth == 64
    assert Map.delete(limits, :ignored_capture_budget) == defaults

    with_captures([limits], queries, [], fn [result] ->
      assert match?({:ok, _, _}, result), inspect(result, limit: 5, printable_limit: 120)
      {:ok, keeper, first} = result
      assert first.abi == abi
      assert first.query_hash == query_hash
      assert first.donor_hashes == []
      assert first.queries == %{atoms: %{}, exports: %{}}

      for field <- [:hashes, :orders, :atoms, :nodes, :exports, :local_funs] do
        assert Map.fetch!(first, field) == []
      end

      extended = Capture.extend_epoch(keeper, [], queries, first.identity)
      assert match?({:ok, _}, extended), inspect(extended, limit: 5, printable_limit: 120)
      {:ok, next} = extended
      assert next.parent == first.identity
      assert next.session_id == first.session_id

      for field <- [
            :abi,
            :query_hash,
            :donor_hashes,
            :queries,
            :hashes,
            :orders,
            :atoms,
            :nodes,
            :exports,
            :local_funs
          ] do
        assert Map.fetch!(next, field) == Map.fetch!(first, field)
      end

      assert limits == Map.put(defaults, :ignored_capture_budget, deep)
      assert Map.delete(limits, :ignored_capture_budget) == defaults
      assert defaults.depth == 64
      assert queries == %{atoms: [], exports: []}
      assert Enum.reduce(1..128, deep, fn _, {inner} -> inner end) == nil
    end)
  end

  test "query_admission_composes_unmasked_actual_prior_roots" do
    SourceTest.bootstrap!()
    prior = :erlang.make_tuple(52, nil)
    name = :binary.copy("q")
    queries = %{atoms: [name], exports: []}
    actual_roots = [prior, queries]
    defaults = SourceTest.limits()
    assert tuple_size(prior) == 52
    assert Enum.all?(Tuple.to_list(prior), &is_nil/1)
    assert byte_size(name) == 1
    assert :binary.referenced_byte_size(name) == 1
    assert Atom.to_string(:q) == name
    assert {:ok, report} = Native.capture_terms({:measure_terms, actual_roots}, defaults)
    assert Enum.sort(Map.keys(report)) == [:live_pinned_bytes, :live_terms, :live_words]
    assert report.live_words == 92
    assert report.live_pinned_bytes == 736
    assert report.live_words + 8 <= defaults.decoded_words
    assert report.live_pinned_bytes + 64 <= defaults.pinned_bytes

    retained = %{
      decoded_words: report.live_words + 8,
      pinned_bytes: report.live_pinned_bytes + 64
    }

    assert retained == %{decoded_words: 100, pinned_bytes: 800}
    word_limits = Map.put(defaults, :decoded_words, 200)
    pin_limits = Map.put(defaults, :pinned_bytes, 1_500)

    results = [
      Input.query(queries, word_limits),
      Input.query(queries, pin_limits),
      Input.query_admission(queries, word_limits, retained),
      Input.query_admission(queries, pin_limits, retained),
      Input.query_admission(queries, defaults, retained)
    ]

    observed = %{atoms: %{name => true}, exports: %{}}
    diagnostic = inspect(results, limit: 40, printable_limit: 120)
    assert Enum.at(results, 0) == {:ok, observed}, diagnostic
    assert Enum.at(results, 1) == {:ok, observed}, diagnostic
    assert Enum.at(results, 2) == {:error, {:limit, :decoded_words}}, diagnostic
    assert Enum.at(results, 3) == {:error, {:limit, :pinned_bytes}}, diagnostic

    assert Enum.at(results, 4) ==
             {:ok, observed, %{decoded_words: 256, pinned_bytes: 2_049}},
           diagnostic

    assert tuple_size(prior) == 52
    assert Enum.all?(Tuple.to_list(prior), &is_nil/1)
    assert actual_roots == [prior, queries]
    assert queries == %{atoms: [name], exports: []}
    assert byte_size(name) == 1
    assert :binary.referenced_byte_size(name) == 1
    assert retained == %{decoded_words: 100, pinned_bytes: 800}
    assert Map.delete(word_limits, :decoded_words) == Map.delete(defaults, :decoded_words)
    assert Map.delete(pin_limits, :pinned_bytes) == Map.delete(defaults, :pinned_bytes)
  end

  test "control_measurement_bounds_clipped_growth_and_reuses_inputs" do
    SourceTest.bootstrap!()
    tuple = :erlang.make_tuple(3, nil)
    defaults = SourceTest.limits()

    base = %{
      defaults
      | donor_bytes: 0,
        decoded_words: 26,
        terms: 6,
        facts: 0,
        text_bytes: 0,
        depth: 0,
        pinned_bytes: 280,
        output_bytes: 0
    }

    word_limits = %{base | decoded_words: 25}
    pin_limits = %{base | pinned_bytes: 279}
    term_limits = %{base | terms: 5}
    assert tuple_size(tuple) == 3
    assert Enum.all?(Tuple.to_list(tuple), &is_nil/1)

    results =
      Enum.map([word_limits, pin_limits, term_limits, base], fn limits ->
        Native.capture_terms({:measure_control, tuple}, limits)
      end)

    assert results == [
             {:error, {:limit, :decoded_words}},
             {:error, {:limit, :pinned_bytes}},
             {:error, {:limit, :terms}},
             {:ok, %{live_words: 26, live_pinned_bytes: 280, live_terms: 6}}
           ],
           inspect(results, limit: 40, printable_limit: 120)

    assert tuple == {nil, nil, nil}
    assert tuple_size(tuple) == 3
    assert Enum.all?(Tuple.to_list(tuple), &is_nil/1)
    assert defaults == SourceTest.limits()

    assert base == %{
             donor_bytes: 0,
             decoded_words: 26,
             terms: 6,
             facts: 0,
             text_bytes: 0,
             depth: 0,
             pinned_bytes: 280,
             output_bytes: 0
           }

    assert word_limits == %{base | decoded_words: 25}
    assert pin_limits == %{base | pinned_bytes: 279}
    assert term_limits == %{base | terms: 5}
  end

  test "bounds_invalid_parent_before_first_budget_send" do
    SourceTest.bootstrap!()
    limits = SourceTest.limits()
    queries = %{atoms: [], exports: []}
    owner = self()

    with_captures([limits], queries, [], fn [{:ok, keeper, first}] ->
      {caller, monitor} =
        spawn_monitor(fn ->
          parent = :erlang.make_tuple(65_536, nil)

          receive do
            :attempt ->
              result = Capture.extend_epoch(keeper, :invalid_donors, :invalid_queries, parent)

              send(owner, {:parent_attempt, self(), result, tuple_size(parent), elem(parent, 0)})
          end

          receive do
            :retry ->
              result = Capture.extend_epoch(keeper, [], queries, first.identity)
              send(owner, {:parent_retry, self(), result, tuple_size(parent)})
          end

          receive do
            :stop -> :ok
          end
        end)

      try do
        assert :erlang.trace(caller, true, [:send, {:tracer, owner}]) == 1
        send(caller, :attempt)
        assert_receive {:parent_attempt, ^caller, rejected, 65_536, nil}, 5_000
        fence = :erlang.trace_delivered(caller)
        assert_receive {:trace_delivered, ^caller, ^fence}, 5_000

        assert_receive {:trace, ^caller, :send, {:capture, ^caller, _ref, {:budget, wire_parent}},
                        ^keeper},
                       5_000

        assert :erlang.trace(caller, false, [:send]) == 1
        send(caller, :retry)
        assert_receive {:parent_retry, ^caller, retried, 65_536}, 5_000
        {dead, dead_monitor} = spawn_monitor(fn -> :ok end)
        assert_receive {:DOWN, ^dead_monitor, :process, ^dead, :normal}, 5_000
        invalid = Capture.extend_epoch(:not_a_pid, :invalid_donors, :invalid_queries, :bad_parent)
        closed = Capture.extend_epoch(dead, :invalid_donors, :invalid_queries, :bad_parent)

        assert is_nil(wire_parent), "first budget parent is a tuple: #{is_tuple(wire_parent)}"
        assert rejected == {:error, :parent_mismatch}
        assert invalid == {:error, :invalid_session}
        assert closed == {:error, :closed_session}
        assert {:ok, next} = retried
        assert next.parent == first.identity
        assert next.session_id == first.session_id
        assert next.query_hash == :crypto.hash(:sha256, TermCodec.encode(queries))

        for field <- [
              :abi,
              :query_hash,
              :donor_hashes,
              :queries,
              :hashes,
              :orders,
              :atoms,
              :nodes,
              :exports,
              :local_funs
            ] do
          assert Map.fetch!(next, field) == Map.fetch!(first, field)
        end

        assert limits == SourceTest.limits()
        assert queries == %{atoms: [], exports: []}
      after
        if Process.alive?(caller), do: :erlang.trace(caller, false, [:send])
        Process.exit(caller, :kill)
        assert_receive {:DOWN, ^monitor, :process, ^caller, _reason}, 5_000
      end
    end)
  end

  defp with_captures([], _queries, results, body), do: body.(Enum.reverse(results))

  defp with_captures([limits | tail], queries, results, body) do
    result = Capture.capture_epoch([], queries, nil, limits)

    try do
      with_captures(tail, queries, [result | results], body)
    after
      case result do
        {:ok, keeper, _captured} -> assert :ok = Capture.close_epoch(keeper)
        _ -> :ok
      end
    end
  end
end
