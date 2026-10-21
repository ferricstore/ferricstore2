defmodule Ferricstore.PolicyRuntimeCapture.Input do
  @moduledoc false
  alias Ferricstore.PolicyRuntimeCapture.Native

  def decode_admission(donors, limits, retained) do
    with {:ok, report} <- Native.capture_terms({:preflight, donors}, limits),
         :ok <- check(2 * report.decoded_words, limits.decoded_words, :decoded_words),
         # A new keeper copies the decoded graph. Conservatively retain TWO
         # complete decode envelopes, including their scratch, across transfer.
         bytes = retained + donor_backing(donors) + 2 * report.pinned_bytes,
         :ok <- check(bytes, limits.pinned_bytes, :pinned_bytes) do
      {:ok, %{decoded_words: 2 * report.decoded_words, pinned_bytes: bytes}}
    end
  end

  def compact(donors, limits, retained) when is_list(donors) do
    with :ok <- donor_limit(donors, limits),
         :ok <- check(length(donors), limits.terms, :terms),
         copying = Enum.reduce(donors, 0, &(byte_size(&1) + &2)),
         :ok <-
           check(retained + donor_backing(donors) + copying, limits.pinned_bytes, :pinned_bytes) do
      {:ok, Enum.map(donors, &:binary.copy/1)}
    end
  end

  def compact(_, _, _), do: {:error, :invalid_donors}

  defp donor_backing(donors),
    do: Enum.reduce(donors, 0, &(:binary.referenced_byte_size(&1) + &2))

  def donor_limit(donors, limits), do: donor_limit(donors, limits, 0, 0)
  defp donor_limit([], _limits, _count, _bytes), do: :ok

  defp donor_limit([head | tail], limits, count, bytes) when is_binary(head) do
    with :ok <- check(count + 1, limits.terms, :terms),
         :ok <- check(bytes + byte_size(head), limits.donor_bytes, :donor_bytes),
         do: donor_limit(tail, limits, count + 1, bytes + byte_size(head))
  end

  defp donor_limit(_, _, _, _), do: {:error, :invalid_donors}

  def query(%{atoms: atoms, exports: exports}, limits)
      when is_list(atoms) and is_list(exports) do
    with bytes when is_integer(bytes) <- query_shape(atoms, exports, limits),
         :ok <- query_budget(11, bytes, limits) do
      query_atoms(atoms, exports, %{}, 11, bytes, limits)
    end
  end

  def query(_, _), do: {:error, :invalid_queries}

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

  defp query_shape(atoms, exports, limits) when is_list(atoms) and is_list(exports) do
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

  defp query_shape(_, _, _), do: {:error, :invalid_queries}

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

  defp query_atoms([], exports, results, used, bytes, limits),
    do: query_exports(exports, results, %{}, used, bytes, limits)

  defp query_atoms([name | tail], exports, results, used, bytes, limits) do
    next = used + 117 + query_put_words(map_size(results))

    with :ok <- query_budget(next, bytes, limits) do
      available = existing(name) != :error
      query_atoms(tail, exports, Map.put(results, name, available), next, bytes, limits)
    end
  end

  defp query_exports([], atoms, exports, _used, _bytes, _limits),
    do: {:ok, %{atoms: atoms, exports: exports}}

  defp query_exports(
         [{module_name, function_name, arity} = item | tail],
         atoms,
         results,
         used,
         bytes,
         limits
       ) do
    next = used + 237 + query_put_words(map_size(results))

    with :ok <- query_budget(next, bytes, limits) do
      available =
        case {existing(module_name), existing(function_name)} do
          {{:ok, module}, {:ok, function}} -> :erlang.function_exported(module, function, arity)
          _ -> false
        end

      query_exports(tail, atoms, Map.put(results, item, available), next, bytes, limits)
    end
  end

  defp query_put_words(size) when size < 32, do: 19 + 2 * size
  defp query_put_words(32), do: 111_048
  defp query_put_words(size), do: size + 487

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
