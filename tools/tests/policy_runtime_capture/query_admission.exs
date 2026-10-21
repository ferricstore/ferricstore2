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
