defmodule Ferricstore.PolicyRuntimeCapture.Input.Query do
  @moduledoc false
  alias Ferricstore.PolicyRuntimeCapture.Input.Meter

  def query(%{atoms: atoms, exports: exports}, limits)
      when is_list(atoms) and is_list(exports) do
    with bytes when is_integer(bytes) <- query_shape(atoms, exports, limits),
         :ok <- query_budget(11, bytes, limits) do
      query_atoms(atoms, exports, %{}, 11, bytes, limits)
    end
  end

  def query(_, _), do: {:error, :invalid_queries}

  def query_admission(queries, limits, {:metered, _, _, _} = retained) do
    case queries do
      %{atoms: atoms, exports: exports} when is_list(atoms) and is_list(exports) ->
        case query_shape(atoms, exports, limits) do
          bytes when is_integer(bytes) ->
            with {:metered, _, _, _} = state <- query_budget(20, bytes, limits, retained) do
              query_atoms(atoms, exports, %{}, 20, bytes, limits, state)
            end

          error ->
            Meter.finish(error, retained, limits)
        end

      _ ->
        Meter.finish({:error, :invalid_queries}, retained, limits)
    end
  end

  def query_admission(%{atoms: atoms, exports: exports}, limits, retained)
      when is_list(atoms) and is_list(exports) do
    with bytes when is_integer(bytes) <- query_shape(atoms, exports, limits),
         :ok <- query_budget(20, bytes, limits, retained) do
      query_atoms(atoms, exports, %{}, 20, bytes, limits, retained)
    end
  end

  def query_admission(_, _, _), do: {:error, :invalid_queries}

  def compact_queries(%{atoms: atoms, exports: exports}, limits, retained) do
    with bytes when is_integer(bytes) <- query_shape(atoms, exports, limits) do
      strings = atoms ++ Enum.flat_map(exports, fn {m, f, _} -> [m, f] end)
      bytes = Enum.reduce(strings, 0, &(:binary.referenced_byte_size(&1) + byte_size(&1) + &2))

      with :ok <- check(retained + bytes, limits.pinned_bytes, :pinned_bytes) do
        {:ok,
         %{
           atoms: Enum.map(atoms, &:binary.copy/1),
           exports: Enum.map(exports, fn {m, f, a} -> {:binary.copy(m), :binary.copy(f), a} end)
         }}
      end
    end
  end

  def compact_queries(_, _, _), do: {:error, :invalid_queries}

  @doc false
  def query_shape(atoms, exports, limits) when is_list(atoms) and is_list(exports) do
    cond do
      not proper_list?(atoms) or not proper_list?(exports) ->
        {:error, :invalid_queries}

      count_left(exports, count_left(atoms, limits.facts)) < 0 ->
        {:error, {:limit, :facts}}

      not atom_queries?(atoms) or not export_queries?(exports) ->
        {:error, :invalid_queries}

      true ->
        remaining = export_text_left(exports, atom_text_left(atoms, limits.text_bytes))

        if remaining < 0,
          do: {:error, {:limit, :text_bytes}},
          else: limits.text_bytes - remaining
    end
  end

  def query_shape(_, _, _), do: {:error, :invalid_queries}

  defp proper_list?([]), do: true
  defp proper_list?([_ | tail]), do: proper_list?(tail)
  defp proper_list?(_), do: false

  defp count_left(_, -1), do: -1
  defp count_left([], remaining), do: remaining
  defp count_left([_ | _], 0), do: -1
  defp count_left([_ | tail], remaining), do: count_left(tail, remaining - 1)

  defp atom_queries?([]), do: true
  defp atom_queries?([name | tail]), do: is_binary(name) and atom_queries?(tail)

  defp export_queries?([]), do: true
  defp export_queries?([item | tail]), do: valid_export(item) and export_queries?(tail)

  defp atom_text_left([], remaining), do: remaining

  defp atom_text_left([name | tail], remaining) do
    size = byte_size(name)
    if size > remaining, do: -1, else: atom_text_left(tail, remaining - size)
  end

  defp export_text_left(_, -1), do: -1
  defp export_text_left([], remaining), do: remaining

  defp export_text_left([{module, function, _} | tail], remaining) do
    module_bytes = byte_size(module)
    function_bytes = byte_size(function)

    if module_bytes > remaining or function_bytes > remaining - module_bytes,
      do: -1,
      else: export_text_left(tail, remaining - module_bytes - function_bytes)
  end

  defp query_atoms(atoms, exports, results, used, bytes, limits),
    do: query_atoms(atoms, exports, results, used, bytes, limits, nil)

  defp query_atoms([], exports, results, used, bytes, limits, {:metered, _, _, _} = state),
    do: query_exports(exports, results, %{}, used, bytes, limits, state)

  defp query_atoms(
         [name | tail],
         exports,
         results,
         used,
         bytes,
         limits,
         {:metered, _, _, _} = retained
       ) do
    next = used + 117 + query_put_words(map_size(results))

    case query_budget(next, bytes, limits, retained) do
      {:metered, _, words, pins} = state ->
        # The explicit mode consumes prepared, aligned spellings only.
        badarg_pins = 16 + (40 + 8 * 68) + (40 + 8 * 6) + (40 + 8 * 4) + (40 + 8 * 9)

        with :ok <- check(words + 89 + 10, limits.decoded_words, :decoded_words),
             :ok <- check(pins + badarg_pins + 80, limits.pinned_bytes, :pinned_bytes) do
          result = existing(name)
          lookup_words = if result == :error, do: 89, else: 5
          lookup_pins = if result == :error, do: badarg_pins, else: 40
          map_words = query_put_words(map_size(results))

          with {:metered, _, _, _} = paid <- Meter.issue(state, limits, lookup_words, lookup_pins),
               {:metered, _, _, _} = paid <- Meter.issue(paid, limits, map_words, 8 * map_words) do
            query_atoms(
              tail,
              exports,
              Map.put(results, name, result != :error),
              next,
              bytes,
              limits,
              paid
            )
          end
        else
          error -> Meter.finish(error, state, limits)
        end

      outcome ->
        outcome
    end
  end

  defp query_atoms([], exports, results, used, bytes, limits, retained),
    do: query_exports(exports, results, %{}, used, bytes, limits, retained)

  defp query_atoms([name | tail], exports, results, used, bytes, limits, retained) do
    next = used + 117 + query_put_words(map_size(results))

    with :ok <- query_budget(next, bytes, limits, retained) do
      available = existing(name) != :error
      query_atoms(tail, exports, Map.put(results, name, available), next, bytes, limits, retained)
    end
  end

  defp query_exports(
         [],
         atoms,
         exports,
         used,
         bytes,
         limits,
         {:metered, forecast, _, _} = retained
       ) do
    # The final old success graph is <=20W/160P. Its live checkpoint matches
    # the next admission checkpoint on a later-row refusal.
    with {:metered, _, _, _} = state <- Meter.issue(retained, limits, 20, 160) do
      result =
        {:ok, %{atoms: atoms, exports: exports},
         %{
           decoded_words: forecast.decoded_words + used,
           pinned_bytes: forecast.pinned_bytes + 8 * used + bytes
         }}

      Meter.finish(result, state, limits)
    end
  end

  defp query_exports(
         [{module_name, function_name, arity} = item | tail],
         atoms,
         results,
         used,
         bytes,
         limits,
         {:metered, _, _, _} = retained
       ) do
    next = used + 237 + query_put_words(map_size(results))

    case query_budget(next, bytes, limits, retained) do
      {:metered, _, words, pins} = state ->
        badarg_pins = 16 + (40 + 8 * 68) + (40 + 8 * 6) + (40 + 8 * 4) + (40 + 8 * 9)

        with :ok <- check(words + 2 * 89 + 10, limits.decoded_words, :decoded_words),
             :ok <- check(pins + 2 * badarg_pins + 80, limits.pinned_bytes, :pinned_bytes) do
          module = existing(module_name)
          function = existing(function_name)

          lookup_words =
            if(module == :error, do: 89, else: 5) + if function == :error, do: 89, else: 5

          lookup_pins =
            if(module == :error, do: badarg_pins, else: 40) +
              if function == :error, do: badarg_pins, else: 40

          available =
            case module do
              {:ok, module_atom} ->
                case function do
                  {:ok, function_atom} ->
                    :erlang.function_exported(module_atom, function_atom, arity)

                  :error ->
                    false
                end

              :error ->
                false
            end

          map_words = query_put_words(map_size(results))

          with {:metered, _, _, _} = paid <- Meter.issue(state, limits, lookup_words, lookup_pins),
               {:metered, _, _, _} = paid <- Meter.issue(paid, limits, map_words, 8 * map_words) do
            query_exports(
              tail,
              atoms,
              Map.put(results, item, available),
              next,
              bytes,
              limits,
              paid
            )
          end
        else
          error -> Meter.finish(error, state, limits)
        end

      outcome ->
        outcome
    end
  end

  defp query_exports([], atoms, exports, _used, _bytes, _limits, nil),
    do: {:ok, %{atoms: atoms, exports: exports}}

  defp query_exports([], atoms, exports, used, bytes, _limits, retained) do
    {:ok, %{atoms: atoms, exports: exports},
     %{
       decoded_words: retained.decoded_words + used,
       pinned_bytes: retained.pinned_bytes + 8 * used + bytes
     }}
  end

  defp query_exports(
         [{module_name, function_name, arity} = item | tail],
         atoms,
         results,
         used,
         bytes,
         limits,
         retained
       ) do
    next = used + 237 + query_put_words(map_size(results))

    with :ok <- query_budget(next, bytes, limits, retained) do
      available =
        case {existing(module_name), existing(function_name)} do
          {{:ok, module}, {:ok, function}} -> :erlang.function_exported(module, function, arity)
          _ -> false
        end

      query_exports(tail, atoms, Map.put(results, item, available), next, bytes, limits, retained)
    end
  end

  defp query_put_words(size) when size < 32, do: 19 + 2 * size
  defp query_put_words(32), do: 111_048
  defp query_put_words(size), do: size + 487

  defp query_budget(words, bytes, limits, {:metered, _, _, _} = retained) do
    case Meter.issue(retained, limits, 0, 0) do
      {:metered, forecast, _, _} = state ->
        case query_budget(words, bytes, limits, forecast) do
          :ok -> state
          error -> Meter.finish(error, state, limits)
        end

      outcome ->
        outcome
    end
  end

  defp query_budget(words, bytes, limits, nil), do: query_budget(words, bytes, limits)

  defp query_budget(words, bytes, limits, retained) do
    word_room = limits.decoded_words - retained.decoded_words
    pin_room = limits.pinned_bytes - retained.pinned_bytes

    cond do
      word_room < 0 or words > word_room -> {:error, {:limit, :decoded_words}}
      pin_room < 0 or bytes > pin_room -> {:error, {:limit, :pinned_bytes}}
      words > div(pin_room - bytes, 8) -> {:error, {:limit, :pinned_bytes}}
      true -> :ok
    end
  end

  defp query_budget(words, bytes, limits) do
    cond do
      words > limits.decoded_words -> {:error, {:limit, :decoded_words}}
      bytes > limits.pinned_bytes -> {:error, {:limit, :pinned_bytes}}
      words > div(limits.pinned_bytes - bytes, 8) -> {:error, {:limit, :pinned_bytes}}
      true -> :ok
    end
  end

  defp valid_export({m, f, a}),
    do: is_binary(m) and is_binary(f) and is_integer(a) and a in 0..255

  defp valid_export(_), do: false

  defp existing(name) do
    {:ok, :erlang.binary_to_existing_atom(name, :utf8)}
  catch
    :error, :badarg -> :error
  end

  defp check(value, maximum, field),
    do: if(value <= maximum, do: :ok, else: {:error, {:limit, field}})
end
