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
    with :ok <- check(length(atoms) + length(exports), limits.facts, :facts),
         true <- Enum.all?(atoms, &is_binary/1),
         true <- Enum.all?(exports, &valid_export/1) do
      bytes =
        Enum.reduce(atoms, 0, &(byte_size(&1) + &2)) +
          Enum.reduce(exports, 0, fn {m, f, _}, sum -> sum + byte_size(m) + byte_size(f) end)

      with :ok <- check(bytes, limits.text_bytes, :text_bytes) do
        atom_results = Map.new(atoms, &{&1, existing(&1) != :error})

        export_results =
          Map.new(exports, fn {m, f, a} = item ->
            result =
              case {existing(m), existing(f)} do
                {{:ok, module}, {:ok, function}} -> :erlang.function_exported(module, function, a)
                _ -> false
              end

            {item, result}
          end)

        {:ok, %{atoms: atom_results, exports: export_results}}
      end
    else
      {:error, _} = error -> error
      _ -> {:error, :invalid_queries}
    end
  end

  def query(_, _), do: {:error, :invalid_queries}

  def compact_queries(%{atoms: atoms, exports: exports} = queries, limits, retained) do
    with {:ok, _} <- query(queries, limits) do
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

  defp valid_export({m, f, a}),
    do: is_binary(m) and is_binary(f) and is_integer(a) and a in 0..255

  defp valid_export(_), do: false

  defp existing(name) do
    {:ok, String.to_existing_atom(name)}
  rescue
    ArgumentError -> :error
  end

  defp check(value, maximum, field),
    do: if(value <= maximum, do: :ok, else: {:error, {:limit, field}})
end
