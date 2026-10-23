defmodule Ferricstore.PolicyRuntimeCapture.Observation do
  @moduledoc false
  alias Ferricstore.PolicyRuntimeCapture.Input
  alias Ferricstore.PolicyRuntimeCapture.Native
  alias Ferricstore.TermCodec

  @doc false
  def observe(state, parent) do
    with :ok <- Input.donor_limit(state.donors, state.limits),
         {:ok, orders} <- orders(state.terms, state.limits),
         native_limits = %{
           state.limits
           | decoded_words: state.limits.decoded_words - state.admission.decoded_words,
             pinned_bytes: state.limits.pinned_bytes - state.admission.pinned_bytes
         },
         {:ok, native} <- Native.capture_terms(state.terms, native_limits),
         :ok <- source_current(state.context.source) do
      capture =
        Map.merge(native, %{
          session_id: state.session_id,
          parent: parent,
          donor_hashes: Enum.map(state.donors, &sha/1),
          query_hash: sha(encode(state.queries)),
          abi: state.context.abi,
          queries: state.observed,
          orders: orders
        })

      with :ok <- output_limit(capture, state.limits) do
        {:ok, Map.put(capture, :identity, sha(encode(capture)))}
      end
    end
  end

  @doc false
  def decode(donors, all, limits, retained) when is_list(donors) do
    with {:ok, raw} <- Input.decode_admission(all, limits, retained),
         admission when is_map(admission) <- decode_budget(donors, limits, raw),
         {:ok, values} <- decode_values(donors, 0, []) do
      {:ok, values, admission}
    end
  end

  def decode(_, _, _, _), do: {:error, :invalid_donors}

  defp decode_budget(donors, limits, retained) do
    extra = 12 * length(donors) + 23
    words = retained.decoded_words + extra

    cond do
      words > limits.decoded_words ->
        {:error, {:limit, :decoded_words}}

      retained.pinned_bytes > limits.pinned_bytes ->
        {:error, {:limit, :pinned_bytes}}

      extra > div(limits.pinned_bytes - retained.pinned_bytes, 8) ->
        {:error, {:limit, :pinned_bytes}}

      true ->
        %{decoded_words: words, pinned_bytes: retained.pinned_bytes + 8 * extra}
    end
  end

  defp decode_values([], _index, values), do: {:ok, Enum.reverse(values)}

  defp decode_values([bytes | tail], index, values) do
    case TermCodec.decode(bytes) do
      {:ok, term} -> decode_values(tail, index + 1, [term | values])
      {:error, reason} -> {:error, {:source_decode, index, reason}}
    end
  end

  defp orders(terms, limits) do
    terms
    |> Enum.with_index()
    |> Enum.reduce_while({:ok, [], 0}, fn {term, i}, {:ok, acc, total} ->
      if is_map(term) do
        sum = Enum.reduce(term, total, fn {key, _}, sum -> external_size(key) + sum end)

        case check(sum, limits.output_bytes, :output_bytes) do
          :ok ->
            row = %{
              donor: i,
              keys: term |> Map.to_list() |> Enum.map(fn {key, _} -> encode(key) end)
            }

            {:cont, {:ok, [row | acc], sum}}

          error ->
            {:halt, error}
        end
      else
        {:cont, {:ok, acc, total}}
      end
    end)
    |> case do
      {:ok, rows, _} -> {:ok, Enum.reverse(rows)}
      error -> error
    end
  end

  @doc false
  def query_extension(state, next, retained) do
    count =
      length(state.queries.atoms) + length(next.atoms) +
        length(state.queries.exports) + length(next.exports)

    with :ok <- check(count, state.limits.facts, :facts) do
      combined = %{
        atoms: Enum.uniq(state.queries.atoms ++ next.atoms),
        exports: Enum.uniq(state.queries.exports ++ next.exports)
      }

      with {:ok, observed, admission} <-
             Input.query_admission(combined, state.limits, retained) do
        same =
          Enum.all?([:atoms, :exports], fn field ->
            Enum.all?(state.observed[field], fn {key, value} -> observed[field][key] == value end)
          end)

        if same,
          do: {:ok, combined, observed, admission},
          else: {:error, :runtime_context_changed}
      end
    end
  end

  defp output_limit(capture, limits),
    do:
      check(
        external_size(Map.put(capture, :identity, <<0::256>>)),
        limits.output_bytes,
        :output_bytes
      )

  defp external_size(value), do: :erlang.external_size(value, [:deterministic])
  defp encode(value), do: TermCodec.encode(value)
  defp sha(bytes), do: :crypto.hash(:sha256, bytes)

  defp check(value, maximum, field),
    do: if(value <= maximum, do: :ok, else: {:error, {:limit, field}})

  defp source_current(%{md5: md5}) do
    if :erlang.get_module_info(TermCodec, :md5) == md5,
      do: :ok,
      else: {:error, :source_code_changed}
  rescue
    _ -> {:error, :source_code_changed}
  end
end
