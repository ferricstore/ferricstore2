defmodule Ferricstore.PolicyRuntimeCaptureNormalizedPrefixTest do
  use ExUnit.Case, async: false
  alias Ferricstore.PolicyRuntimeCapture.Input
  alias Ferricstore.PolicyRuntimeCaptureTest, as: SourceTest
  alias Ferricstore.TermCodec

  # Each synthetic opening bank excludes fixture roots and the incoming value tag.
  # These component counters are not a Native permit or proof of input ownership.
  test "metered_normalize_preserves_raw_spines_and_late_invalid_prefix" do
    SourceTest.bootstrap!()
    limits = %{SourceTest.limits() | decoded_words: 1_048_576, pinned_bytes: 67_108_864}
    forecast = %{decoded_words: 200_000, pinned_bytes: 8_388_608}
    {gw, gp} = {1_000, 8_000}
    plain = :binary.copy(<<131, 97, 1>>)
    source = :binary.copy("x", 65_536)
    wire = TermCodec.encode(source)
    backing = <<0, wire::binary, 0>>
    view = binary_part(backing, 1, byte_size(wire))
    mapped = %{encoded: view, source_term: source}
    prepared = [plain, mapped]
    expected = {[plain, view], [nil, mapped]}
    error = {:error, :invalid_donors}
    assert byte_size(wire) == 65_542 and view == wire
    assert :binary.referenced_byte_size(view) == byte_size(backing)
    assert :binary.referenced_byte_size(source) == 65_536

    for {value, raw} <- [
          {[], {[], []}},
          {[plain], {[plain], [nil]}},
          {prepared, expected},
          {[:invalid], error},
          {[plain, mapped, :invalid], error},
          {[plain | :invalid], error},
          {:invalid, error},
          {[Map.put(mapped, :extra, true)], error}
        ] do
      assert Input.normalize(value) == raw
    end

    early = Input.normalize({:metered, [:invalid], limits, forecast, gw, gp})
    late = Input.normalize({:metered, [plain, mapped, :invalid], limits, forecast, gw, gp})
    empty = Input.normalize({:metered, [], limits, forecast, gw, gp})
    single = Input.normalize({:metered, [plain], limits, forecast, gw, gp})
    mixed = Input.normalize({:metered, prepared, limits, forecast, gw, gp})

    for {outcome, raw} <- [
          {early, error},
          {late, error},
          {empty, {[], []}},
          {single, {[plain], [nil]}},
          {mixed, expected}
        ] do
      assert {^raw, words, pins} = outcome
      assert is_integer(words) and gw < words and words < forecast.decoded_words
      assert is_integer(pins) and gp < pins and pins < forecast.pinned_bytes
    end

    assert {^error, early_w, early_p} = early
    assert {^error, late_w, late_p} = late
    assert {late_w - early_w, late_p - early_p} == {32, 256}
    assert {{[^plain, borrowed], [nil, anchor]}, _, _} = mixed
    assert borrowed == view and anchor == mapped
    assert :binary.referenced_byte_size(borrowed) == byte_size(backing)
    assert :binary.referenced_byte_size(anchor.encoded) == byte_size(backing)
    tight_forecast = %{decoded_words: gw, pinned_bytes: gp}

    for {room, spent} <- [{5, 4}, {21, 20}],
        axis <- [:decoded_words, :pinned_bytes, :both] do
      words = if axis == :pinned_bytes, do: limits.decoded_words, else: gw + room
      pins = if axis == :decoded_words, do: limits.pinned_bytes, else: gp + 8 * room
      tight = %{limits | decoded_words: words, pinned_bytes: pins}
      field = if axis == :pinned_bytes, do: :pinned_bytes, else: :decoded_words
      refused = {{:error, {:limit, field}}, gw + spent, gp + 8 * spent}
      assert Input.normalize({:metered, prepared, tight, tight_forecast, gw, gp}) == refused
    end

    assert mapped == %{encoded: view, source_term: source}
    assert :binary.referenced_byte_size(view) == byte_size(backing)
    assert :binary.referenced_byte_size(source) == 65_536
  end

  test "metered_raw_contexts_retains_restored_prefix_without_reencoding" do
    SourceTest.bootstrap!()
    limits = %{SourceTest.limits() | decoded_words: 1_048_576, pinned_bytes: 67_108_864}
    forecast = %{decoded_words: 200_000, pinned_bytes: 8_388_608}
    {gw, gp} = {1_000, 8_000}
    plain = :binary.copy(<<131, 97, 1>>)
    source = :binary.copy("x", 65_536)
    wire = TermCodec.encode(source)
    backing = <<0, wire::binary, 0>>
    view = binary_part(backing, 1, byte_size(wire))
    mapped = %{encoded: view, source_term: source}
    wrong = %{mapped | encoded: :binary.copy(<<131, 97, 7>>)}
    donors = [plain, view]
    anchors = [nil, mapped]
    opaque = make_ref()
    short = [opaque]
    long = [opaque, opaque, opaque, opaque]
    error = {:error, :invalid_donors}
    assert byte_size(wire) == 65_542 and view == wire
    assert :binary.referenced_byte_size(view) == byte_size(backing)
    assert :binary.referenced_byte_size(source) == 65_536

    for {old, old_anchors, suffix, raw} <- [
          {[], [], short, short},
          {[], [], long, long},
          {donors, anchors, short, [plain, mapped | short]},
          {donors, anchors, long, [plain, mapped | long]},
          {[view], [wrong], short, error},
          {donors, [nil, wrong], short, error},
          {donors, [nil], short, error},
          {[], [nil], short, error},
          {[view], [Map.put(mapped, :extra, true)], short, error},
          {donors, anchors, [opaque | :invalid], error}
        ] do
      assert Input.raw_contexts(old, old_anchors, suffix) == raw
    end

    short_mode = {:metered, short, limits, forecast, gw, gp}
    long_mode = {:metered, long, limits, forecast, gw, gp}
    empty = Input.raw_contexts([], [], short_mode)
    empty_long = Input.raw_contexts([], [], long_mode)
    restored = Input.raw_contexts(donors, anchors, short_mode)
    longer = Input.raw_contexts(donors, anchors, long_mode)
    early = Input.raw_contexts([view], [wrong], short_mode)
    late = Input.raw_contexts(donors, [nil, wrong], short_mode)

    invalid =
      Input.raw_contexts(
        donors,
        anchors,
        {:metered, [opaque | :invalid], limits, forecast, gw, gp}
      )

    assert empty == {short, gw + 4, gp + 32}
    assert empty_long == {long, gw + 4, gp + 32}
    assert invalid == {error, gw + 4, gp + 32}

    for {outcome, raw} <- [
          {restored, [plain, mapped | short]},
          {longer, [plain, mapped | long]},
          {early, error},
          {late, error}
        ] do
      assert {^raw, words, pins} = outcome
      assert is_integer(words) and gw < words and words < forecast.decoded_words
      assert is_integer(pins) and gp < pins and pins < forecast.pinned_bytes
    end

    assert {^error, early_w, early_p} = early
    assert {^error, late_w, late_p} = late
    assert {late_w - early_w, late_p - early_p} == {14, 112}
    assert {[^plain, anchor | ^short], short_w, short_p} = restored
    assert {_, long_w, long_p} = longer
    assert {short_w, short_p} == {long_w, long_p}
    assert anchor == mapped and :binary.referenced_byte_size(anchor.encoded) == byte_size(backing)
    tight_forecast = %{decoded_words: gw, pinned_bytes: gp}

    for {room, spent} <- [{5, 4}, {19, 18}],
        axis <- [:decoded_words, :pinned_bytes, :both] do
      words = if axis == :pinned_bytes, do: limits.decoded_words, else: gw + room
      pins = if axis == :decoded_words, do: limits.pinned_bytes, else: gp + 8 * room
      tight = %{limits | decoded_words: words, pinned_bytes: pins}
      field = if axis == :pinned_bytes, do: :pinned_bytes, else: :decoded_words
      refused = {{:error, {:limit, field}}, gw + spent, gp + 8 * spent}
      mode = {:metered, short, tight, tight_forecast, gw, gp}
      assert Input.raw_contexts(donors, anchors, mode) == refused
      empty_expected = if room == 5, do: refused, else: {short, gw + 4, gp + 32}
      assert Input.raw_contexts([], [], mode) == empty_expected
    end

    assert mapped == %{encoded: view, source_term: source}
    assert :binary.referenced_byte_size(view) == byte_size(backing)
    assert :binary.referenced_byte_size(source) == 65_536
  end
end
