defmodule Ferricstore.PolicyRuntimeCapture.Input.Meter do
  @moduledoc false

  def forecast({:metered, forecast, _, _}), do: forecast
  def forecast({:metered_copy, _, _, _, forecast, _, _}), do: forecast
  def forecast(raw), do: raw

  def issue(
        {:metered_copy, _, _, _, forecast, words, pins} = state,
        limits,
        extra_words,
        extra_pins
      ) do
    unless is_integer(words) and words >= 0 and is_integer(pins) and pins >= 0 and
             words <= limits.decoded_words - 5 and pins <= limits.pinned_bytes - 40,
           do: raise(ArgumentError, "unfunded metered continuation")

    cond do
      not is_integer(extra_words) or not is_integer(extra_pins) or extra_words < 0 or
          extra_pins < 0 ->
        raise ArgumentError, "negative issued cohort"

      words + extra_words + 10 > limits.decoded_words ->
        finish({:error, {:limit, :decoded_words}}, state, limits)

      pins + extra_pins + 80 > limits.pinned_bytes ->
        finish({:error, {:limit, :pinned_bytes}}, state, limits)

      true ->
        {:metered, forecast, words + extra_words + 5, pins + extra_pins + 40}
    end
  end

  def issue({:metered, forecast, words, pins} = state, limits, extra_words, extra_pins) do
    unless is_integer(words) and words >= 0 and is_integer(pins) and pins >= 0 and
             words <= limits.decoded_words - 5 and pins <= limits.pinned_bytes - 40,
           do: raise(ArgumentError, "unfunded metered continuation")

    cond do
      not is_integer(extra_words) or not is_integer(extra_pins) or extra_words < 0 or
          extra_pins < 0 ->
        raise ArgumentError, "negative issued cohort"

      words + extra_words + 10 > limits.decoded_words ->
        finish({:error, {:limit, :decoded_words}}, state, limits)

      pins + extra_pins + 80 > limits.pinned_bytes ->
        finish({:error, {:limit, :pinned_bytes}}, state, limits)

      true ->
        {:metered, forecast, words + extra_words + 5, pins + extra_pins + 40}
    end
  end

  def issue(raw, _limits, _words, _pins), do: raw

  def carry(_forecast, {{:error, _}, words, pins} = outcome, _limits)
      when is_integer(words) and is_integer(pins), do: outcome

  def carry(_forecast, {:unsettled, _result, words, pins} = outcome, _limits)
      when is_integer(words) and is_integer(pins), do: outcome

  def carry({:error, _} = error, state, limits), do: finish(error, state, limits)

  def carry(forecast, {:metered, _, words, pins} = state, limits) do
    unless is_integer(words) and words >= 0 and is_integer(pins) and pins >= 0 and
             words <= limits.decoded_words - 5 and pins <= limits.pinned_bytes - 40,
           do: raise(ArgumentError, "unfunded metered continuation")

    cond do
      words + 10 > limits.decoded_words ->
        finish({:error, {:limit, :decoded_words}}, state, limits)

      pins + 80 > limits.pinned_bytes ->
        finish({:error, {:limit, :pinned_bytes}}, state, limits)

      true ->
        {:metered, forecast, words + 5, pins + 40}
    end
  end

  def carry(forecast, {_result, words, pins} = source, limits)
      when is_integer(words) and is_integer(pins) do
    unless is_integer(words) and words >= 0 and is_integer(pins) and pins >= 0 and
             words <= limits.decoded_words - 5 and pins <= limits.pinned_bytes - 40,
           do: raise(ArgumentError, "unfunded paid outcome")

    cond do
      words + 10 > limits.decoded_words ->
        finish({:error, {:limit, :decoded_words}}, source, limits)

      pins + 80 > limits.pinned_bytes ->
        finish({:error, {:limit, :pinned_bytes}}, source, limits)

      true ->
        {:metered, forecast, words + 5, pins + 40}
    end
  end

  def carry(forecast, _raw, _limits), do: forecast

  def finish(result, {:metered, _, words, pins}, :unsettled),
    do: {:unsettled, result, words + 5, pins + 40}

  def finish(result, {:metered_copy, _, _, _, _, words, pins}, :unsettled),
    do: {:unsettled, result, words + 5, pins + 40}

  def finish({{:error, _}, words, pins} = outcome, _state, _limits)
      when is_integer(words) and is_integer(pins), do: outcome

  def finish({:unsettled, _result, words, pins} = outcome, _state, _limits)
      when is_integer(words) and is_integer(pins), do: outcome

  def finish(_result, {{:error, _}, words, pins} = outcome, _limits)
      when is_integer(words) and is_integer(pins), do: outcome

  def finish(_result, {:unsettled, _previous, words, pins} = outcome, _limits)
      when is_integer(words) and is_integer(pins), do: outcome

  def finish(result, {:metered_copy, _, _, _, _, words, pins} = state, limits) do
    unless is_integer(words) and words >= 0 and is_integer(pins) and pins >= 0 and
             words <= limits.decoded_words - 5 and pins <= limits.pinned_bytes - 40,
           do: raise(ArgumentError, "unfunded metered continuation")

    required = if match?({:error, _}, result), do: 4, else: 9

    cond do
      words + required > limits.decoded_words ->
        finish({:error, {:limit, :decoded_words}}, state, limits)

      pins + 8 * required > limits.pinned_bytes ->
        finish({:error, {:limit, :pinned_bytes}}, state, limits)

      true ->
        {result, words + 4, pins + 32}
    end
  end

  def finish(result, {:metered, _, words, pins} = state, limits) do
    unless is_integer(words) and words >= 0 and is_integer(pins) and pins >= 0 and
             words <= limits.decoded_words - 5 and pins <= limits.pinned_bytes - 40,
           do: raise(ArgumentError, "unfunded metered continuation")

    required = if match?({:error, _}, result), do: 4, else: 9

    cond do
      words + required > limits.decoded_words ->
        finish({:error, {:limit, :decoded_words}}, state, limits)

      pins + 8 * required > limits.pinned_bytes ->
        finish({:error, {:limit, :pinned_bytes}}, state, limits)

      true ->
        {result, words + 4, pins + 32}
    end
  end

  def finish(result, {_previous, words, pins} = source, limits)
      when is_integer(words) and is_integer(pins) do
    unless is_integer(words) and words >= 0 and is_integer(pins) and pins >= 0 and
             words <= limits.decoded_words - 5 and pins <= limits.pinned_bytes - 40,
           do: raise(ArgumentError, "unfunded paid outcome")

    required = if match?({:error, _}, result), do: 4, else: 9

    cond do
      words + required > limits.decoded_words ->
        finish({:error, {:limit, :decoded_words}}, source, limits)

      pins + 8 * required > limits.pinned_bytes ->
        finish({:error, {:limit, :pinned_bytes}}, source, limits)

      true ->
        {result, words + 4, pins + 32}
    end
  end

  def finish(result, _raw, _limits), do: result
end
