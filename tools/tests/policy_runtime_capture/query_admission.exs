defmodule Ferricstore.PolicyRuntimeCaptureQueryAdmissionTest do
  use ExUnit.Case, async: false
  alias Ferricstore.PolicyRuntimeCapture, as: Capture
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
