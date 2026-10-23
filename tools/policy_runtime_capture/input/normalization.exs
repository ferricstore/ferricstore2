defmodule Ferricstore.PolicyRuntimeCapture.Input.Normalization do
  @moduledoc false
  alias Ferricstore.PolicyRuntimeCapture.Input.Meter

  @doc false
  def normalize({:metered, _, _, _, _, _} = context),
    do: normalize(context, [], [])

  def normalize(prepared), do: normalize(prepared, [], [])

  defp normalize({:metered, [], limits, _, _, _} = context, donors, anchors) do
    count = length(donors)
    reversal_words = if count < 2, do: 0, else: 4 * count
    words = reversal_words + 3

    with {:metered, _, _, _} = paid <- Meter.issue(context, limits, words, 8 * words) do
      result = {:lists.reverse(donors), :lists.reverse(anchors)}
      Meter.finish(result, paid, limits)
    end
  end

  defp normalize(
         {:metered, [binary | tail], limits, forecast, _, _} = context,
         donors,
         anchors
       )
       when is_binary(binary) do
    with {:metered, _, words, pins} <- Meter.issue(context, limits, 11, 88) do
      normalize(
        {:metered, tail, limits, forecast, words, pins},
        [binary | donors],
        [nil | anchors]
      )
    end
  end

  defp normalize(
         {:metered, [%{encoded: encoded, source_term: _} = mapped | tail], limits, forecast, _, _} =
           context,
         donors,
         anchors
       )
       when map_size(mapped) == 2 and is_binary(encoded) do
    with {:metered, _, words, pins} <- Meter.issue(context, limits, 11, 88) do
      normalize(
        {:metered, tail, limits, forecast, words, pins},
        [encoded | donors],
        [mapped | anchors]
      )
    end
  end

  defp normalize({:metered, _, limits, _, _, _} = context, _, _),
    do: Meter.finish({:error, :invalid_donors}, context, limits)

  defp normalize([], donors, anchors), do: {:lists.reverse(donors), :lists.reverse(anchors)}

  defp normalize([binary | tail], donors, anchors) when is_binary(binary),
    do: normalize(tail, [binary | donors], [nil | anchors])

  defp normalize([%{encoded: encoded, source_term: _} = mapped | tail], donors, anchors)
       when map_size(mapped) == 2 and is_binary(encoded),
       do: normalize(tail, [encoded | donors], [mapped | anchors])

  defp normalize(_, _, _), do: {:error, :invalid_donors}

  @doc false
  def raw_contexts(donors, anchors, {:metered, prepared, limits, _, _, _} = context) do
    if proper_list?(prepared),
      do: raw_contexts(donors, anchors, context, []),
      else: Meter.finish({:error, :invalid_donors}, context, limits)
  end

  def raw_contexts(donors, anchors, new_prepared) do
    if proper_list?(new_prepared),
      do: raw_contexts(donors, anchors, new_prepared, []),
      else: {:error, :invalid_donors}
  end

  defp raw_contexts([], [], {:metered, prepared, limits, _, _, _} = context, []),
    do: Meter.finish(prepared, context, limits)

  defp raw_contexts([], [], {:metered, prepared, limits, _, _, _} = context, acc) do
    words = 2 * length(acc)

    with {:metered, _, _, _} = paid <- Meter.issue(context, limits, words, 8 * words) do
      Meter.finish(:lists.reverse(acc, prepared), paid, limits)
    end
  end

  defp raw_contexts(
         [binary | donors],
         [nil | anchors],
         {:metered, prepared, limits, forecast, _, _} = context,
         acc
       )
       when is_binary(binary) do
    with {:metered, _, words, pins} <- Meter.issue(context, limits, 9, 72) do
      raw_contexts(
        donors,
        anchors,
        {:metered, prepared, limits, forecast, words, pins},
        [binary | acc]
      )
    end
  end

  defp raw_contexts(
         [binary | donors],
         [%{encoded: encoded, source_term: _} = mapped | anchors],
         {:metered, prepared, limits, forecast, _, _} = context,
         acc
       )
       when map_size(mapped) == 2 and is_binary(binary) and encoded == binary do
    with {:metered, _, words, pins} <- Meter.issue(context, limits, 9, 72) do
      raw_contexts(
        donors,
        anchors,
        {:metered, prepared, limits, forecast, words, pins},
        [mapped | acc]
      )
    end
  end

  defp raw_contexts(_, _, {:metered, _, limits, _, _, _} = context, _),
    do: Meter.finish({:error, :invalid_donors}, context, limits)

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

  defp proper_list?([]), do: true
  defp proper_list?([_ | tail]), do: proper_list?(tail)
  defp proper_list?(_), do: false
end
