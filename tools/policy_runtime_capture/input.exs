defmodule Ferricstore.PolicyRuntimeCapture.Input do
  @moduledoc false
  alias Ferricstore.PolicyRuntimeCapture.Native
  alias Ferricstore.TermCodec

  def decode_admission(donors, limits, %{decoded_words: words, pinned_bytes: pins}) do
    # Remaining limits20 + request3 + RAW report15 + returned pair/ok11.
    with :ok <- preparation_donors(donors, limits, 0, 0),
         :ok <- preparation_room(words, pins, limits),
         backing = preparation_backing(donors, 0, limits.pinned_bytes),
         :ok <- preparation_room(words + 49, pins + backing + 392, limits),
         native_limits = remaining_limits(limits, words + 49, pins + backing + 392, 2),
         {:ok, report} <- Native.capture_terms({:preflight, donors}, native_limits) do
      {:ok,
       %{
         decoded_words: words + 49 + 2 * report.decoded_words,
         pinned_bytes: pins + backing + 392 + 2 * report.pinned_bytes
       }}
    end
  end

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

  def prepare(donors, queries, limits, retained, source) do
    with :ok <- preparation_shape(donors, queries, limits, retained),
         {:ok, original} <- original_admission(donors, queries, limits, retained, 1),
         # Layout request3 + report27 + this remaining-limit map20.
         :ok <- preparation_room(original.decoded_words + 50, original.pinned_bytes + 400, limits),
         native_limits =
           remaining_limits(limits, original.decoded_words + 23, original.pinned_bytes + 184, 1),
         {:ok, layout} <- Native.capture_terms({:encoder_layout, 64}, native_limits),
         admission when is_map(admission) <-
           preparation_budget(donors, queries, limits, original, layout),
         {:ok, admission} <- bind_with_layout(donors, limits, admission, source, 1, layout) do
      copied_queries = copy_queries(queries)
      {:ok, copy_donors(donors, []), copied_queries, admission}
    else
      {:error, _} = error -> error
    end
  end

  def original_admission(donors, queries, limits, retained, copies) when copies in [1, 2] do
    # Remaining-limit map20 + this pair/ok11. LIVE itself includes29.
    words = retained.decoded_words
    pins = retained.pinned_bytes

    with :ok <- preparation_room(words, pins, limits),
         :ok <- preparation_room(words + 31 + copies * 29, pins + 248 + copies * 232, limits),
         native_limits = remaining_limits(limits, words + 31, pins + 248, copies),
         {:ok, report} <-
           Native.capture_terms({:preflight, {:anchors, [donors, queries]}}, native_limits) do
      {:ok,
       %{
         decoded_words: words + 31 + copies * report.live_words,
         pinned_bytes: pins + 248 + copies * report.live_pinned_bytes
       }}
    end
  end

  defp preparation_shape(donors, %{atoms: atoms, exports: exports}, limits, retained) do
    with bytes when is_integer(bytes) <- query_shape(atoms, exports, limits),
         # Preserve the old scalar query-copy refusal before donor validation.
         backing =
           preparation_backing(
             atoms,
             export_backing(exports, 0, limits.pinned_bytes),
             limits.pinned_bytes
           ),
         :ok <- check(retained.pinned_bytes + backing + bytes, limits.pinned_bytes, :pinned_bytes) do
      preparation_donors(donors, limits, 0, 0)
    end
  end

  defp preparation_shape(_, _, _, _), do: {:error, :invalid_queries}

  defp preparation_donors([], _limits, _count, _bytes), do: :ok

  defp preparation_donors([head | tail], limits, count, bytes) do
    case prepared_binary(head) do
      binary when is_binary(binary) ->
        preparation_donor(tail, binary, limits, count, bytes)

      _ ->
        {:error, :invalid_donors}
    end
  end

  defp preparation_donors(_, _, _, _), do: {:error, :invalid_donors}

  defp preparation_donor(tail, binary, limits, count, bytes) do
    with :ok <- check(count + 1, limits.terms, :terms),
         :ok <- check(bytes + byte_size(binary), limits.donor_bytes, :donor_bytes) do
      preparation_donors(tail, limits, count + 1, bytes + byte_size(binary))
    end
  end

  defp prepared_binary(binary) when is_binary(binary), do: binary

  defp prepared_binary(%{encoded: encoded, source_term: _} = mapped)
       when map_size(mapped) == 2 and is_binary(encoded), do: encoded

  defp prepared_binary(_), do: nil

  defp mapped_count([], count), do: count

  defp mapped_count([head | tail], count),
    do: mapped_count(tail, count + if(is_map(head), do: 1, else: 0))

  defp preparation_budget(donors, %{atoms: atoms, exports: exports}, limits, original, layout) do
    donor_count = length(donors)
    atom_count = length(atoms)
    export_count = length(exports)
    count = donor_count + atom_count + 2 * export_count
    bytes = preparation_bytes(donors, preparation_bytes(atoms, export_bytes(exports, 0)))
    # Accumulator/reversal cells, MFA4, query map8 and result13.
    controls =
      4 * (donor_count + atom_count + export_count) + 4 * export_count + 21 +
        8 * mapped_count(donors, 0)

    words = original.decoded_words + 50 + controls + count * layout.result_words
    backing = count * (layout.binary_bytes - 64 + 8) + bytes

    with :ok <- check(words, limits.decoded_words, :decoded_words),
         :ok <- check(original.pinned_bytes + 400 + backing, limits.pinned_bytes, :pinned_bytes),
         extra_words = controls + count * layout.result_words,
         :ok <-
           check(
             extra_words,
             div(limits.pinned_bytes - original.pinned_bytes - 400 - backing, 8),
             :pinned_bytes
           ) do
      # Return the funded map directly; prepare owns the5-word outer tuple.
      %{
        decoded_words: words,
        pinned_bytes: original.pinned_bytes + 400 + backing + 8 * extra_words
      }
    end
  end

  defp preparation_room(words, pins, limits) do
    with :ok <- check(words, limits.decoded_words, :decoded_words),
         do: check(pins, limits.pinned_bytes, :pinned_bytes)
  end

  defp remaining_limits(limits, words, pins, copies),
    do: %{
      donor_bytes: limits.donor_bytes,
      decoded_words: div(limits.decoded_words - words, copies),
      terms: limits.terms,
      facts: limits.facts,
      text_bytes: limits.text_bytes,
      depth: limits.depth,
      pinned_bytes: div(limits.pinned_bytes - pins, copies),
      output_bytes: limits.output_bytes
    }

  defp preparation_bytes([], bytes), do: bytes

  defp preparation_bytes([head | tail], bytes),
    do: preparation_bytes(tail, bytes + byte_size(prepared_binary(head)))

  defp export_bytes([], bytes), do: bytes

  defp export_bytes([{module, function, _} | tail], bytes),
    do: export_bytes(tail, bytes + byte_size(module) + byte_size(function))

  defp preparation_backing(_, bytes, maximum) when bytes > maximum, do: maximum + 1
  defp preparation_backing([], bytes, _maximum), do: bytes

  defp preparation_backing([head | tail], bytes, maximum) do
    size = :binary.referenced_byte_size(prepared_binary(head))

    if size > maximum - bytes,
      do: maximum + 1,
      else: preparation_backing(tail, bytes + size, maximum)
  end

  defp export_backing(_, bytes, maximum) when bytes > maximum, do: maximum + 1
  defp export_backing([], bytes, _maximum), do: bytes

  defp export_backing([{module, function, _} | tail], bytes, maximum) do
    module_bytes = :binary.referenced_byte_size(module)
    function_bytes = :binary.referenced_byte_size(function)

    if module_bytes > maximum - bytes or function_bytes > maximum - bytes - module_bytes,
      do: maximum + 1,
      else: export_backing(tail, bytes + module_bytes + function_bytes, maximum)
  end

  defp copy_queries(%{atoms: atoms, exports: exports}),
    do: %{atoms: copy_donors(atoms, []), exports: copy_exports(exports, [])}

  defp copy_donors([], acc), do: :lists.reverse(acc)

  defp copy_donors([%{encoded: encoded, source_term: term} | tail], acc),
    do: copy_donors(tail, [%{encoded: :binary.copy(encoded), source_term: term} | acc])

  defp copy_donors([head | tail], acc), do: copy_donors(tail, [:binary.copy(head) | acc])
  defp copy_exports([], acc), do: :lists.reverse(acc)

  defp copy_exports([{module, function, arity} | tail], acc),
    do: copy_exports(tail, [{:binary.copy(module), :binary.copy(function), arity} | acc])

  def bind_admission(donors, limits, retained, source, copies) when copies in [1, 2] do
    with budget when is_map(budget) <- binding_add(retained, limits, 50, 400, copies),
         {:ok, layout} <-
           Native.capture_terms({:encoder_layout, 64}, remaining_limits(limits, 0, 0, 1)),
         do: bind_with_layout(donors, limits, budget, source, copies, layout)
  end

  defp bind_with_layout(donors, limits, retained, source, copies, layout) do
    with budget when is_map(budget) <- binding_add(retained, limits, 0, 0, copies),
         result when is_map(result) <-
           bind_donors(donors, 0, limits, budget, source, copies, layout),
         do: {:ok, result}
  end

  defp bind_donors([], _index, _limits, retained, _source, _copies, _layout), do: retained

  defp bind_donors([binary | tail], index, limits, retained, source, copies, layout)
       when is_binary(binary),
       do: bind_donors(tail, index + 1, limits, retained, source, copies, layout)

  defp bind_donors(
         [%{encoded: encoded, source_term: term} = mapped | tail],
         index,
         limits,
         retained,
         source,
         copies,
         layout
       )
       when map_size(mapped) == 2 and is_binary(encoded) do
    with budget when is_map(budget) <-
           bind_one(encoded, term, index, limits, retained, source, copies, layout),
         do: bind_donors(tail, index + 1, limits, budget, source, copies, layout)
  end

  defp bind_donors(_, _, _, _, _, _, _), do: {:error, :invalid_donors}

  defp bind_one(expected, term, index, limits, retained, source, copies, layout) do
    # Metric42 plus FOUR source-MD5 results, each on-heap16B:4 words/32 pins.
    with measured when is_map(measured) <- binding_add(retained, limits, 58, 464, copies),
         {:ok, metrics} <-
           Native.capture_terms({:measure_terms, term}, remaining_limits(limits, 0, 0, 1)),
         size_pins =
           cap_sum(
             cap_product(8, layout.size_result_words, limits.pinned_bytes),
             metrics.live_pinned_bytes,
             limits.pinned_bytes
           ),
         size_pins =
           cap_sum(
             size_pins,
             cap_product(128, metrics.live_terms, limits.pinned_bytes),
             limits.pinned_bytes
           ),
         sized when is_map(sized) <-
           binding_add(measured, limits, layout.size_result_words, size_pins, copies),
         :ok <- source_current(source),
         maximum = :erlang.external_size(term, [:deterministic]),
         :ok <- source_current(source),
         ecap =
           cap_sum(
             maximum,
             cap_product(1023, metrics.live_terms, limits.pinned_bytes),
             limits.pinned_bytes
           ),
         :ok <- check(ecap, limits.pinned_bytes, :pinned_bytes),
         encoded_budget when is_map(encoded_budget) <-
           encoder_budget(metrics, ecap, layout, sized, limits, copies),
         :ok <- source_current(source),
         encoded = TermCodec.encode(term),
         :ok <- source_current(source),
         :ok <- check(byte_size(encoded), ecap, :pinned_bytes) do
      if encoded == expected, do: encoded_budget, else: {:error, {:source_anchor_mismatch, index}}
    end
  end

  defp encoder_budget(metrics, ecap, layout, retained, limits, copies) do
    wc = limits.decoded_words
    pc = limits.pinned_bytes
    t = metrics.live_terms
    n = div(t - 1, 2)
    y = cap_sum(1, cap_product(11, t, wc), wc)
    y = cap_sum(y, cap_product(2, metrics.live_words, wc), wc)
    y = cap_sum(y, div(ecap, 32) + if(rem(ecap, 32) == 0, do: 0, else: 1), wc)
    y = cap_sum(y, cap_product(n, n, wc), wc)
    y = cap_sum(y, cap_product(3, n, wc), wc)
    h = cap_sum(layout.size_result_words, cap_product(y, layout.trap_return_words, wc), wc)
    h = cap_sum(h, layout.result_words, wc)
    b = cap_sum(layout.binary_bytes - 64, ecap, pc)
    sort = if n == 0, do: 0, else: cap_sum(144, cap_product(192, max(1, n - 15), pc), pc)
    sort = cap_sum(sort, if(n > 16, do: 232, else: 0), pc)
    work = cap_sum(b, cap_product(288, t, pc), pc)
    work = cap_sum(work, cap_product(16, n, pc), pc)
    work = cap_sum(work, max(sort, 16), pc)
    work = max(cap_product(128, t, pc), max(work, cap_product(2, b, pc)))
    pins = cap_sum(cap_product(8, h, pc), layout.context_bytes, pc)
    pins = cap_sum(pins, metrics.live_pinned_bytes, pc)
    binding_add(retained, limits, h, cap_sum(pins, work, pc), copies)
  end

  defp binding_add(retained, limits, words, pins, copies) do
    # Each direct map uses8 words; its3-word slack funds the final ok wrapper.
    words = cap_sum(words, 11, limits.decoded_words)
    pins = cap_sum(pins, 88, limits.pinned_bytes)

    words =
      cap_sum(
        retained.decoded_words,
        cap_product(copies, words, limits.decoded_words),
        limits.decoded_words
      )

    pins =
      cap_sum(
        retained.pinned_bytes,
        cap_product(copies, pins, limits.pinned_bytes),
        limits.pinned_bytes
      )

    with :ok <- preparation_room(words, pins, limits),
         do: %{decoded_words: words, pinned_bytes: pins}
  end

  defp cap_sum(a, b, maximum),
    do: if(a > maximum or b > maximum - a, do: maximum + 1, else: a + b)

  defp cap_product(a, b, maximum),
    do: if(a != 0 and b > div(maximum, a), do: maximum + 1, else: a * b)

  defp source_current(%{md5: md5}) do
    if TermCodec.module_info(:md5) == md5, do: :ok, else: {:error, :source_code_changed}
  rescue
    _ -> {:error, :source_code_changed}
  end

  defp source_current(_), do: {:error, :source_code_changed}

  def normalize(prepared), do: normalize(prepared, [], [])
  defp normalize([], donors, anchors), do: {:lists.reverse(donors), :lists.reverse(anchors)}

  defp normalize([binary | tail], donors, anchors) when is_binary(binary),
    do: normalize(tail, [binary | donors], [nil | anchors])

  defp normalize([%{encoded: encoded, source_term: _} = mapped | tail], donors, anchors)
       when map_size(mapped) == 2 and is_binary(encoded),
       do: normalize(tail, [encoded | donors], [mapped | anchors])

  defp normalize(_, _, _), do: {:error, :invalid_donors}

  def raw_contexts(donors, anchors, new_prepared) do
    if proper_list?(new_prepared),
      do: raw_contexts(donors, anchors, new_prepared, []),
      else: {:error, :invalid_donors}
  end

  defp raw_contexts([], [], new_prepared, acc), do: :lists.reverse(acc, new_prepared)

  defp raw_contexts([binary | donors], [nil | anchors], new_prepared, acc) when is_binary(binary),
    do: raw_contexts(donors, anchors, new_prepared, [binary | acc])

  defp raw_contexts(
         [binary | donors],
         [%{encoded: encoded, source_term: _} = mapped | anchors],
         new_prepared,
         acc
       )
       when map_size(mapped) == 2 and is_binary(binary) and encoded == binary,
       do: raw_contexts(donors, anchors, new_prepared, [mapped | acc])

  defp raw_contexts(_, _, _, _), do: {:error, :invalid_donors}

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
