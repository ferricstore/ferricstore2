Code.require_file("input/meter.exs", __DIR__)
Code.require_file("input/query.exs", __DIR__)
Code.require_file("input/binding.exs", __DIR__)
Code.require_file("input/normalization.exs", __DIR__)

defmodule Ferricstore.PolicyRuntimeCapture.Input do
  @moduledoc false
  alias Ferricstore.PolicyRuntimeCapture.Native
  alias Ferricstore.PolicyRuntimeCapture.Input.{Meter, Query, Binding, Normalization}

  def decode_admission(donors, limits, retained)
      when (is_map(retained) and is_map_key(retained, :decoded_words) and
              is_map_key(retained, :pinned_bytes)) or
             (is_tuple(retained) and tuple_size(retained) == 4 and elem(retained, 0) == :metered) do
    forecast = Meter.forecast(retained)
    words = forecast.decoded_words
    pins = forecast.pinned_bytes

    with :ok <- preparation_donors(donors, limits, 0, 0),
         :ok <- preparation_room(words, pins, limits),
         backing = preparation_backing(donors, 0, limits.pinned_bytes),
         :ok <- preparation_room(words + 49, pins + backing + 392, limits),
         paid <- Meter.issue(retained, limits, 23, 184),
         %{} <- Meter.forecast(paid),
         result =
           Binding.native_estimate(
             {:preflight, donors},
             remaining_limits(limits, words + 49, pins + backing + 392, 2),
             limits,
             paid,
             :raw
           ),
         state <- Meter.carry(forecast, result, limits),
         %{} <- Meter.forecast(state),
         {:ok, report} <-
           (case result do
              {native, w, p} when is_integer(w) and is_integer(p) -> native
              raw -> raw
            end),
         state <- Meter.issue(state, limits, 11, 88),
         %{} <- Meter.forecast(state) do
      Meter.finish(
        {:ok,
         %{
           decoded_words: words + 49 + 2 * report.decoded_words,
           pinned_bytes: pins + backing + 392 + 2 * report.pinned_bytes
         }},
        state,
        limits
      )
    else
      error -> Meter.finish(error, retained, limits)
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
    original =
      with :ok <- preparation_shape(donors, queries, limits, Meter.forecast(retained)),
           do: original_admission(donors, queries, limits, retained, 1)

    state =
      case original do
        {{:ok, forecast}, _, _} -> Meter.carry(forecast, original, limits)
        {:ok, forecast} -> forecast
        error -> Meter.finish(error, retained, limits)
      end

    with %{} = forecast <- Meter.forecast(state),
         :ok <- preparation_room(forecast.decoded_words + 50, forecast.pinned_bytes + 400, limits),
         paid <- Meter.issue(state, limits, 23, 184),
         %{} <- Meter.forecast(paid),
         result =
           Binding.native_estimate(
             {:encoder_layout, 64},
             remaining_limits(
               limits,
               forecast.decoded_words + 23,
               forecast.pinned_bytes + 184,
               1
             ),
             limits,
             paid,
             :layout
           ),
         state <- Meter.carry(forecast, result, limits),
         %{} <- Meter.forecast(state),
         {:ok, layout} <-
           (case result do
              {native, w, p} when is_integer(w) and is_integer(p) -> native
              raw -> raw
            end),
         budget <- preparation_budget(donors, queries, limits, state, layout),
         %{} <- Meter.forecast(budget),
         bound = Binding.bind_with_layout(donors, limits, budget, source, 1, layout) do
      case bound do
        {{:ok, forecast}, _, _} ->
          with {:metered, _, _, _} = copied <- Meter.carry(forecast, bound, limits),
               {:metered, _, w, p} <- Meter.issue(copied, limits, 8, 64),
               {:metered_copy, copied_queries, _, _, _, _, _} = queried <-
                 copy_queries({:metered_copy, queries, layout, limits, forecast, w, p}),
               {:metered, _, w, p} <- Meter.issue(queried, limits, 8, 64),
               {:metered_copy, copied_donors, _, _, _, _, _} = copied <-
                 copy_donors(donors, {:metered_copy, [], layout, limits, forecast, w, p}),
               {:metered, _, _, _} = finished <- Meter.issue(copied, limits, 5, 40) do
            Meter.finish({:ok, copied_donors, copied_queries, forecast}, finished, limits)
          end

        {:ok, forecast} ->
          copied_queries = copy_queries(queries)
          {:ok, copy_donors(donors, []), copied_queries, forecast}

        error ->
          error
      end
    else
      error -> Meter.finish(error, state, limits)
    end
  end

  def original_admission(donors, queries, limits, retained, copies) when copies in [1, 2] do
    forecast = Meter.forecast(retained)
    words = forecast.decoded_words
    pins = forecast.pinned_bytes

    with :ok <- preparation_room(words, pins, limits),
         :ok <- preparation_room(words + 31 + copies * 29, pins + 248 + copies * 232, limits),
         paid <- Meter.issue(retained, limits, 30, 240),
         %{} <- Meter.forecast(paid),
         result =
           Binding.native_estimate(
             {:preflight, {:anchors, [donors, queries]}},
             remaining_limits(limits, words + 31, pins + 248, copies),
             limits,
             paid,
             :live
           ),
         state <- Meter.carry(forecast, result, limits),
         %{} <- Meter.forecast(state),
         {:ok, report} <-
           (case result do
              {native, w, p} when is_integer(w) and is_integer(p) -> native
              raw -> raw
            end),
         state <- Meter.issue(state, limits, 11, 88),
         %{} <- Meter.forecast(state) do
      Meter.finish(
        {:ok,
         %{
           decoded_words: words + 31 + copies * report.live_words,
           pinned_bytes: pins + 248 + copies * report.live_pinned_bytes
         }},
        state,
        limits
      )
    else
      error -> Meter.finish(error, retained, limits)
    end
  end

  def preparation_shape(donors, %{atoms: atoms, exports: exports}, limits, retained) do
    forecast = Meter.forecast(retained)

    result =
      with bytes when is_integer(bytes) <- Query.query_shape(atoms, exports, limits),
           backing =
             preparation_backing(
               atoms,
               export_backing(exports, 0, limits.pinned_bytes),
               limits.pinned_bytes
             ),
           :ok <-
             check(forecast.pinned_bytes + backing + bytes, limits.pinned_bytes, :pinned_bytes),
           do: preparation_donors(donors, limits, 0, 0)

    Meter.finish(result, retained, limits)
  end

  def preparation_shape(_, _, limits, retained),
    do: Meter.finish({:error, :invalid_queries}, retained, limits)

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
    retained = original
    original = Meter.forecast(retained)
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
           ),
         state <- Meter.issue(retained, limits, 8, 64),
         %{} <- Meter.forecast(state) do
      Meter.carry(
        %{
          decoded_words: words,
          pinned_bytes: original.pinned_bytes + 400 + backing + 8 * extra_words
        },
        state,
        limits
      )
    else
      error -> Meter.finish(error, retained, limits)
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

  defp copy_queries(
         {:metered_copy, %{atoms: atoms, exports: exports}, layout, limits, forecast, _, _} =
           context
       ) do
    with {:metered, _, words, pins} <- Meter.issue(context, limits, 8, 64),
         {:metered_copy, copied_atoms, _, _, _, _, _} = atoms_state <-
           copy_donors(atoms, {:metered_copy, [], layout, limits, forecast, words, pins}),
         {:metered, _, words, pins} <- Meter.issue(atoms_state, limits, 8, 64),
         {:metered_copy, copied_exports, _, _, _, _, _} = exports_state <-
           copy_exports(exports, {:metered_copy, [], layout, limits, forecast, words, pins}),
         {:metered, _, words, pins} <- Meter.issue(exports_state, limits, 16, 128) do
      {:metered_copy, %{atoms: copied_atoms, exports: copied_exports}, layout, limits, forecast,
       words, pins}
    end
  end

  defp copy_queries(%{atoms: atoms, exports: exports}),
    do: %{atoms: copy_donors(atoms, []), exports: copy_exports(exports, [])}

  defp copy_donors(donors, {:metered_copy, acc, layout, limits, forecast, _, _} = context) do
    case donors do
      [] when acc == [] ->
        context

      [] ->
        words = 2 * length(acc) + 8

        with {:metered, _, paid_words, paid_pins} <-
               Meter.issue(context, limits, words, 8 * words),
             do:
               {:metered_copy, :lists.reverse(acc), layout, limits, forecast, paid_words,
                paid_pins}

      [head | tail] ->
        binary = prepared_binary(head)
        size = byte_size(binary)
        output_words = if size <= 64, do: 2 + div(size + 7, 8), else: 8
        payload = if size > 64, do: layout.binary_bytes - 64 + size, else: 0

        retention =
          :binary.referenced_byte_size(binary) + layout.binary_bytes - 64 +
            16 * layout.result_words

        output_pins = 40 + 8 * output_words + payload + retention
        structure_words = 2 + 8 + if(is_map(head), do: 8, else: 0)

        with {:metered, _, _, _} = copied <-
               Meter.issue(context, limits, output_words, output_pins),
             value = :binary.copy(binary),
             {:metered, _, words, pins} <-
               Meter.issue(copied, limits, structure_words, 8 * structure_words) do
          item =
            if is_map(head), do: %{encoded: value, source_term: head.source_term}, else: value

          copy_donors(tail, {:metered_copy, [item | acc], layout, limits, forecast, words, pins})
        end
    end
  end

  defp copy_donors([], acc), do: :lists.reverse(acc)

  defp copy_donors([%{encoded: encoded, source_term: term} | tail], acc),
    do: copy_donors(tail, [%{encoded: :binary.copy(encoded), source_term: term} | acc])

  defp copy_donors([head | tail], acc), do: copy_donors(tail, [:binary.copy(head) | acc])

  defp copy_exports(exports, {:metered_copy, acc, layout, limits, forecast, _, _} = context) do
    case exports do
      [] when acc == [] ->
        context

      [] ->
        words = 2 * length(acc) + 8

        with {:metered, _, paid_words, paid_pins} <-
               Meter.issue(context, limits, words, 8 * words),
             do:
               {:metered_copy, :lists.reverse(acc), layout, limits, forecast, paid_words,
                paid_pins}

      [{module, function, arity} | tail] ->
        module_size = byte_size(module)
        module_words = if module_size <= 64, do: 2 + div(module_size + 7, 8), else: 8
        module_payload = if module_size > 64, do: layout.binary_bytes - 64 + module_size, else: 0

        module_retention =
          :binary.referenced_byte_size(module) + layout.binary_bytes - 64 +
            16 * layout.result_words

        module_pins = 40 + 8 * module_words + module_payload + module_retention
        function_size = byte_size(function)
        function_words = if function_size <= 64, do: 2 + div(function_size + 7, 8), else: 8

        function_payload =
          if function_size > 64, do: layout.binary_bytes - 64 + function_size, else: 0

        function_retention =
          :binary.referenced_byte_size(function) + layout.binary_bytes - 64 +
            16 * layout.result_words

        function_pins = 40 + 8 * function_words + function_payload + function_retention

        with {:metered, _, _, _} = module_state <-
               Meter.issue(context, limits, module_words, module_pins),
             copied_module = :binary.copy(module),
             {:metered, _, _, _} = function_state <-
               Meter.issue(module_state, limits, function_words, function_pins),
             copied_function = :binary.copy(function),
             {:metered, _, words, pins} <- Meter.issue(function_state, limits, 14, 112) do
          copy_exports(
            tail,
            {:metered_copy, [{copied_module, copied_function, arity} | acc], layout, limits,
             forecast, words, pins}
          )
        end
    end
  end

  defp copy_exports([], acc), do: :lists.reverse(acc)

  defp copy_exports([{module, function, arity} | tail], acc),
    do: copy_exports(tail, [{:binary.copy(module), :binary.copy(function), arity} | acc])

  def bind_admission(donors, limits, retained, source, copies) when copies in [1, 2],
    do: Binding.bind_admission(donors, limits, retained, source, copies)

  def normalize(prepared), do: Normalization.normalize(prepared)

  def raw_contexts(donors, anchors, new_prepared),
    do: Normalization.raw_contexts(donors, anchors, new_prepared)

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

  def query(queries, limits), do: Query.query(queries, limits)

  def query_admission(queries, limits, retained),
    do: Query.query_admission(queries, limits, retained)

  def compact_queries(queries, limits, retained),
    do: Query.compact_queries(queries, limits, retained)

  defp check(value, maximum, field),
    do: if(value <= maximum, do: :ok, else: {:error, {:limit, field}})
end
