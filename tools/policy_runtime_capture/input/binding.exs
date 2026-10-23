defmodule Ferricstore.PolicyRuntimeCapture.Input.Binding do
  @moduledoc false
  alias Ferricstore.PolicyRuntimeCapture.Input.Meter
  alias Ferricstore.PolicyRuntimeCapture.Native
  alias Ferricstore.TermCodec
  @source_error_pins 40 + 8 * 68 + (40 + 8 * 6) + (40 + 8 * 4) + (40 + 8 * 9)

  @doc false
  def bind_admission(donors, limits, retained, source, copies) when copies in [1, 2] do
    with budget <- binding_add(retained, limits, 50, 400, copies),
         %{} = forecast <- Meter.forecast(budget),
         paid <- Meter.issue(budget, limits, 23, 184),
         %{} <- Meter.forecast(paid),
         result =
           native_estimate(
             {:encoder_layout, 64},
             remaining_limits(limits, 0, 0, 1),
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
            end) do
      bind_with_layout(donors, limits, state, source, copies, layout)
    else
      error -> Meter.finish(error, retained, limits)
    end
  end

  @doc false
  def bind_with_layout(donors, limits, retained, source, copies, layout) do
    with budget <- binding_add(retained, limits, 0, 0, copies),
         %{} <- Meter.forecast(budget),
         result <- bind_donors(donors, 0, limits, budget, source, copies, layout),
         %{} = forecast <- Meter.forecast(result),
         state <- Meter.issue(result, limits, 3, 24),
         %{} <- Meter.forecast(state) do
      Meter.finish({:ok, forecast}, state, limits)
    else
      error -> Meter.finish(error, retained, limits)
    end
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
    with budget <- bind_one(encoded, term, index, limits, retained, source, copies, layout),
         %{} <- Meter.forecast(budget),
         do: bind_donors(tail, index + 1, limits, budget, source, copies, layout)
  end

  defp bind_donors(_, _, limits, retained, _, _, _),
    do: Meter.finish({:error, :invalid_donors}, retained, limits)

  defp bind_one(expected, term, index, limits, retained, source, copies, layout) do
    with measured <- binding_add(retained, limits, 58, 464, copies),
         %{} = forecast <- Meter.forecast(measured),
         paid <- Meter.issue(measured, limits, 23, 184),
         %{} <- Meter.forecast(paid),
         result =
           native_estimate(
             {:measure_terms, term},
             remaining_limits(limits, 0, 0, 1),
             limits,
             paid,
             :metrics
           ),
         state <- Meter.carry(forecast, result, limits),
         %{} <- Meter.forecast(state),
         {:ok, metrics} <-
           (case result do
              {native, w, p} when is_integer(w) and is_integer(p) -> native
              raw -> raw
            end),
         size_pins =
           cap_sum(
             cap_sum(
               cap_product(8, layout.size_result_words, limits.pinned_bytes),
               metrics.live_pinned_bytes,
               limits.pinned_bytes
             ),
             cap_product(128, metrics.live_terms, limits.pinned_bytes),
             limits.pinned_bytes
           ),
         sized <- binding_add(state, limits, layout.size_result_words, size_pins, copies),
         %{} <- Meter.forecast(sized),
         guarded = source_current(source, limits, sized),
         guarded = if(guarded == :ok, do: sized, else: guarded),
         %{} <- Meter.forecast(guarded),
         sized <- Meter.issue(guarded, limits, layout.size_result_words, size_pins),
         %{} <- Meter.forecast(sized),
         maximum = :erlang.external_size(term, [:deterministic]),
         guarded = source_current(source, limits, sized),
         guarded = if(guarded == :ok, do: sized, else: guarded),
         %{} <- Meter.forecast(guarded),
         ecap =
           cap_sum(
             maximum,
             cap_product(1023, metrics.live_terms, limits.pinned_bytes),
             limits.pinned_bytes
           ),
         proof <-
           (case check(ecap, limits.pinned_bytes, :pinned_bytes) do
              :ok -> encoder_budget(metrics, ecap, layout, guarded, limits, copies)
              error -> Meter.finish(error, guarded, limits)
            end),
         budget =
           (case proof do
              {{:metered, _, _, _} = paid, _, _} -> paid
              raw -> raw
            end),
         %{} <- Meter.forecast(budget),
         guarded = source_current(source, limits, budget),
         guarded = if(guarded == :ok, do: budget, else: guarded),
         %{} <- Meter.forecast(guarded),
         issued <-
           (case proof do
              {{:metered, _, _, _}, h, pins} -> Meter.issue(guarded, limits, h, pins)
              _ -> guarded
            end),
         %{} <- Meter.forecast(issued),
         encoded = TermCodec.encode(term),
         guarded = source_current(source, limits, issued),
         guarded = if(guarded == :ok, do: issued, else: guarded),
         %{} <- Meter.forecast(guarded) do
      case check(byte_size(encoded), ecap, :pinned_bytes) do
        :ok ->
          if encoded == expected,
            do: guarded,
            else: Meter.finish({:error, {:source_anchor_mismatch, index}}, guarded, limits)

        error ->
          Meter.finish(error, guarded, limits)
      end
    else
      error -> Meter.finish(error, retained, limits)
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
    operation_pins = cap_sum(pins, work, pc)

    case binding_add(retained, limits, h, operation_pins, copies) do
      {:metered, _, _, _} = budget ->
        with {:metered, _, _, _} = state <- Meter.issue(budget, limits, 4, 32),
             do: {state, h, operation_pins}

      raw ->
        raw
    end
  end

  defp binding_add(retained, limits, words, pins, copies) do
    state = retained
    retained = Meter.forecast(state)
    # The old forecast includes ok slack; only this map is constructed here.
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
         paid <- Meter.issue(state, limits, 8, 64),
         %{} <- Meter.forecast(paid) do
      Meter.carry(%{decoded_words: words, pinned_bytes: pins}, paid, limits)
    else
      error -> Meter.finish(error, state, limits)
    end
  end

  defp cap_sum(a, b, maximum),
    do: if(a > maximum or b > maximum - a, do: maximum + 1, else: a + b)

  defp cap_product(a, b, maximum),
    do: if(a != 0 and b > div(maximum, a), do: maximum + 1, else: a * b)

  defp source_current(%{md5: md5}) do
    if :erlang.get_module_info(TermCodec, :md5) == md5,
      do: :ok,
      else: {:error, :source_code_changed}
  rescue
    _ -> {:error, :source_code_changed}
  end

  defp source_current(_), do: {:error, :source_code_changed}

  defp source_current(%{md5: md5}, limits, {:metered, _, _, _} = incoming) do
    case Meter.issue(incoming, limits, 5, 40) do
      {:metered, _, words, pins} = state ->
        with :ok <- preparation_room(words + 87 + 10, pins + @source_error_pins + 80, limits) do
          branch =
            try do
              if :erlang.get_module_info(TermCodec, :md5) == md5,
                do: :ok,
                else: :source_code_changed
            catch
              :error, :badarg -> :badarg
              :error, _ -> :unsettled
            end

          if branch == :unsettled do
            Meter.finish({:error, :source_code_changed}, state, :unsettled)
          else
            heap_words = if branch == :badarg, do: 87, else: 4
            heap_pins = if branch == :badarg, do: @source_error_pins, else: 32

            with {:metered, _, _, _} = paid <- Meter.issue(state, limits, heap_words, heap_pins) do
              if branch == :ok,
                do: paid,
                else: Meter.finish({:error, :source_code_changed}, paid, limits)
            end
          end
        else
          error -> Meter.finish(error, state, limits)
        end

      outcome ->
        outcome
    end
  end

  defp source_current(_, limits, {:metered, _, _, _} = state),
    do: Meter.finish({:error, :source_code_changed}, state, limits)

  defp source_current(source, _limits, _raw), do: source_current(source)

  @doc false
  def native_estimate(request, native_limits, limits, {:metered, _, words, pins} = state, kind) do
    valid =
      case kind do
        :live -> match?({:preflight, {:anchors, _}}, request)
        :metrics -> match?({:measure_terms, _}, request)
        :raw -> match?({:preflight, donors} when is_list(donors), request)
        :layout -> request == {:encoder_layout, 64}
        _ -> false
      end

    unless valid, do: raise(ArgumentError, "invalid estimator continuation")

    keys =
      case kind do
        :raw -> 2
        :layout -> 5
        _ -> 3
      end

    success_words = 7 + 2 * keys
    success_pins = 40 + 8 * (4 + 2 * keys) + (40 + 8 * 203)
    maximum_pins = max(success_pins, 40 + 8 * 204 + (40 + 8 * 203))

    with :ok <- preparation_room(words + success_words + 19, pins + maximum_pins + 280, limits),
         {:metered, _, _, _} = entered <- Meter.issue(state, limits, 0, 128) do
      result = Native.capture_terms(request, native_limits)

      output_words =
        case result do
          {:ok, report} when is_map(report) and map_size(report) == keys -> success_words
          {:error, {:source_decode, _, _}} -> 7
          {:error, {_, _}} -> 6
          {:error, reason} when is_atom(reason) -> 3
          _ -> nil
        end

      if is_nil(output_words) do
        Meter.finish(result, entered, :unsettled)
      else
        output_pins =
          case result do
            {:ok, _} -> success_pins
            {:error, {:source_decode, _, _}} -> 40 + 8 * 204 + (40 + 8 * 203)
            {:error, {_, _}} -> 2 * (40 + 8 * 203)
            {:error, _} -> 40 + 8 * 203
          end

        with {:metered, _, _, _} = paid <- Meter.issue(entered, limits, output_words, output_pins),
             do: Meter.finish(result, paid, limits)
      end
    else
      error -> Meter.finish(error, state, limits)
    end
  end

  def native_estimate(request, native_limits, _limits, _raw, kind)
      when kind in [:live, :metrics, :raw, :layout],
      do: Native.capture_terms(request, native_limits)

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

  defp preparation_room(words, pins, limits) do
    with :ok <- check(words, limits.decoded_words, :decoded_words),
         do: check(pins, limits.pinned_bytes, :pinned_bytes)
  end

  defp check(value, maximum, field),
    do: if(value <= maximum, do: :ok, else: {:error, {:limit, field}})
end
