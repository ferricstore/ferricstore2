defmodule Ferricstore.PolicyRuntimeCapture.Observation do
  @moduledoc false
  alias Ferricstore.PolicyRuntimeCapture.Input
  alias Ferricstore.PolicyRuntimeCapture.Input.Binding
  alias Ferricstore.PolicyRuntimeCapture.Input.{Meter, Query}
  alias Ferricstore.PolicyRuntimeCapture.Native
  alias Ferricstore.TermCodec

  # Entry/helper control is caller-owned; each new worker entry is funded here.
  @decode_worker_frame_words 32

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
  def decode(donors, all, limits, {:metered, _, _, _} = incoming) when is_list(donors) do
    case Input.decode_admission(all, limits, incoming) do
      {{:ok, preflight}, _, _} = result ->
        with {:metered, _, _, _} = retained <- Meter.carry(preflight, result, limits),
             :ok <- decode_budget(donors, limits, retained),
             {:metered, _, words, pins} <-
               Meter.issue(
                 retained,
                 limits,
                 18 + @decode_worker_frame_words,
                 144 + 8 * @decode_worker_frame_words
               ) do
          extra = 12 * length(donors) + 23

          forecast = %{
            decoded_words: preflight.decoded_words + extra,
            pinned_bytes: preflight.pinned_bytes + 8 * extra
          }

          prepared = :lists.nthtail(length(all) - length(donors), all)

          decode_values(
            {:metered, {donors, prepared}, limits, forecast, words, pins},
            0,
            []
          )
        end

      terminal ->
        terminal
    end
  end

  def decode(_, _, limits, {:metered, _, _, _} = incoming),
    do: Meter.finish({:error, :invalid_donors}, incoming, limits)

  def decode(donors, all, limits, retained) when is_list(donors) do
    with {:ok, raw} <- Input.decode_admission(all, limits, retained),
         admission when is_map(admission) <- decode_budget(donors, limits, raw),
         {:ok, values} <- decode_values(donors, 0, []) do
      {:ok, values, admission}
    end
  end

  def decode(_, _, _, _), do: {:error, :invalid_donors}

  defp decode_budget(donors, limits, {:metered, forecast, _, _} = retained) do
    extra = 12 * length(donors) + 23
    words = forecast.decoded_words + extra

    cond do
      words > limits.decoded_words ->
        Meter.finish({:error, {:limit, :decoded_words}}, retained, limits)

      forecast.pinned_bytes > limits.pinned_bytes ->
        Meter.finish({:error, {:limit, :pinned_bytes}}, retained, limits)

      extra > div(limits.pinned_bytes - forecast.pinned_bytes, 8) ->
        Meter.finish({:error, {:limit, :pinned_bytes}}, retained, limits)

      true ->
        :ok
    end
  end

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

  defp decode_values(
         {:metered, {[], []}, limits, forecast, _, _} = retained,
         _index,
         values
       ) do
    count = length(values)
    words = if(count < 2, do: 0, else: 2 * count) + 4

    with {:metered, _, _, _} = paid <- Meter.issue(retained, limits, words, 8 * words) do
      Meter.finish({:ok, :lists.reverse(values), forecast}, paid, limits)
    end
  end

  defp decode_values(
         {:metered, {[bytes | tail], [prepared | contexts]}, limits, forecast, _, _} = incoming,
         index,
         values
       ) do
    with {:metered, _, _, _} = requested <- Meter.issue(incoming, limits, 5, 40),
         result <-
           Binding.native_estimate({:preflight, [prepared]}, limits, limits, requested, :raw),
         {{:ok, report}, _, _} <- result,
         {:metered, _, words, pins} = retained <- Meter.carry(forecast, result, limits) do
      common_words = report.decoded_words + 18
      common_pins = report.pinned_bytes + 224
      failure_words = 87
      failure_pins = 856
      room_words = common_words + 5 + failure_words + 5 + 22 + @decode_worker_frame_words
      room_pins = common_pins + 40 + failure_pins + 40 + 176 + 8 * @decode_worker_frame_words

      cond do
        words + room_words > limits.decoded_words ->
          Meter.finish({:error, {:limit, :decoded_words}}, retained, limits)

        pins + room_pins > limits.pinned_bytes ->
          Meter.finish({:error, {:limit, :pinned_bytes}}, retained, limits)

        true ->
          with {:metered, _, _, _} = entered <-
                 Meter.issue(retained, limits, common_words, common_pins) do
            case TermCodec.decode(bytes) do
              {:ok, term} when is_binary(prepared) or is_map(prepared) ->
                with {:metered, _, _, _} = returned <- Meter.issue(entered, limits, 6, 88),
                     {:metered, _, next_words, next_pins} <-
                       Meter.issue(
                         returned,
                         limits,
                         12 + @decode_worker_frame_words,
                         96 + 8 * @decode_worker_frame_words
                       ) do
                  decode_values(
                    {:metered, {tail, contexts}, limits, forecast, next_words, next_pins},
                    index + 1,
                    [term | values]
                  )
                end

              {:error, reason} when is_binary(prepared) or is_map(prepared) ->
                with {:metered, _, _, _} = returned <-
                       Meter.issue(entered, limits, failure_words, failure_pins),
                     {:metered, _, _, _} = finished <- Meter.issue(returned, limits, 7, 56) do
                  Meter.finish({:error, {:source_decode, index, reason}}, finished, limits)
                end
            end
          end
      end
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
  def query_extension(state, next, {:metered, _, _, _} = incoming) do
    limits = state.limits

    count =
      length(state.queries.atoms) + length(next.atoms) +
        length(state.queries.exports) + length(next.exports)

    case count <= limits.facts do
      true ->
        with {:metered, atoms, _, _, _, _} = atom_state <-
               query_unique(state.queries.atoms, next.atoms, %{}, [], limits, incoming),
             {:metered, exports, _, _, _, _} = export_state <-
               query_unique(state.queries.exports, next.exports, %{}, [], limits, atom_state),
             {:metered, _, _, _} = paid <- Meter.issue(export_state, limits, 8, 64) do
          combined = %{atoms: atoms, exports: exports}
          result = Input.query_admission(combined, limits, paid)

          case result do
            {{:ok, observed, admission}, _, _} ->
              with {:metered, _, _, _} = resumed <- Meter.carry(admission, result, limits) do
                same =
                  query_same(state.queries.atoms, state.observed.atoms, observed.atoms) and
                    query_same(state.queries.exports, state.observed.exports, observed.exports)

                if same do
                  with {:metered, _, _, _} = finished <- Meter.issue(resumed, limits, 5, 40) do
                    Meter.finish({:ok, combined, observed, admission}, finished, limits)
                  end
                else
                  Meter.finish({:error, :runtime_context_changed}, resumed, limits)
                end
              end

            terminal ->
              terminal
          end
        end

      false ->
        Meter.finish({:error, {:limit, :facts}}, incoming, limits)
    end
  end

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

  defp query_unique([], [_ | _] = next, seen, acc, limits, retained),
    do: query_unique(next, [], seen, acc, limits, retained)

  defp query_unique([], [], _seen, acc, limits, retained) do
    count = length(acc)
    reversal_words = if count < 2, do: 0, else: 2 * count
    words = reversal_words + 7

    with {:metered, forecast, paid_words, paid_pins} <-
           Meter.issue(retained, limits, words, 8 * words) do
      {:metered, :lists.reverse(acc), limits, forecast, paid_words, paid_pins}
    end
  end

  defp query_unique([item | old], next, seen, acc, limits, retained) do
    if :erlang.is_map_key(item, seen) do
      query_unique(old, next, seen, acc, limits, retained)
    else
      words = Query.query_put_words(map_size(seen)) + 2

      with {:metered, _, _, _} = paid <- Meter.issue(retained, limits, words, 8 * words) do
        query_unique(old, next, Map.put(seen, item, true), [item | acc], limits, paid)
      end
    end
  end

  defp query_same([], _old, _new), do: true

  defp query_same([key | tail], old, new) do
    if :erlang.map_get(key, old) == :erlang.map_get(key, new),
      do: query_same(tail, old, new),
      else: false
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
