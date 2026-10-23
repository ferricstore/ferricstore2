defmodule Ferricstore.PolicyRuntimeCapturePaidPrefixTest do
  use ExUnit.Case, async: false
  alias Ferricstore.PolicyRuntimeCapture, as: Capture
  alias Ferricstore.PolicyRuntimeCapture.Input
  alias Ferricstore.PolicyRuntimeCapture.Input.Meter
  alias Ferricstore.PolicyRuntimeCaptureTest, as: SourceTest
  alias Ferricstore.TermCodec

  # These are one issued encoder cohort, not public entry/terminal fees.
  @encoder_words 17_956
  @encoder_pins 280_870

  test "metered_prepare_retains_encoder_prefix_on_late_anchor_mismatch" do
    with_context(fn limits, source, mode ->
      {:metered, forecast, initial_words, initial_pins} = mode
      term = :binary.copy("x", 65_536)
      expected = TermCodec.encode(term)
      backing = <<0, expected::binary, 0>>
      encoded = binary_part(backing, 1, byte_size(expected))
      good = %{encoded: encoded, source_term: term}
      bad = %{encoded: TermCodec.encode(8), source_term: 7}
      queries = %{atoms: [], exports: []}
      error = {:error, {:source_anchor_mismatch, 1}}
      assert :binary.referenced_byte_size(term) == 65_536
      assert byte_size(expected) == 65_542 and encoded == expected
      assert :binary.referenced_byte_size(encoded) == byte_size(backing)
      assert bad.encoded == <<131, 97, 8>>

      raw_success = Input.prepare([good], queries, limits, forecast, source)
      assert {:ok, [compact], ^queries, raw_admission} = raw_success
      assert is_map(raw_admission) and map_size(raw_admission) == 2
      assert compact == good and :binary.referenced_byte_size(compact.encoded) == 65_542
      assert Input.prepare([good, bad], queries, limits, forecast, source) == error

      {words, pins} =
        assert_paid(Input.prepare([good, bad], queries, limits, mode, source), error, mode)

      assert words >= initial_words + @encoder_words
      assert words < initial_words + 2 * @encoder_words
      assert pins >= initial_pins + @encoder_pins

      metered_success = Input.prepare([good], queries, limits, mode, source)
      {_words, copied_pins} = assert_paid(metered_success, raw_success, mode)
      assert {{:ok, [copied], ^queries, _}, _, _} = metered_success
      assert copied == good and :binary.referenced_byte_size(copied.encoded) == 65_542
      # The issued copy adds its payload to the separately issued encoder cohort.
      assert copied_pins >= initial_pins + @encoder_pins + byte_size(expected)
      assert :binary.referenced_byte_size(encoded) == byte_size(backing)
      assert good.source_term == term and bad == %{encoded: <<131, 97, 8>>, source_term: 7}
    end)
  end

  test "metered_bind_issues_one_encoder_cohort_with_two_copy_forecast" do
    with_context(fn limits, source, mode ->
      {:metered, forecast, initial_words, initial_pins} = mode
      term = :binary.copy("x", 65_536)
      wrong = %{encoded: TermCodec.encode(7), source_term: term}
      scalar = %{encoded: TermCodec.encode(7), source_term: 7}
      error = {:error, {:source_anchor_mismatch, 0}}
      assert :binary.referenced_byte_size(term) == 65_536
      assert wrong.encoded == <<131, 97, 7>>
      assert {:ok, raw_admission} = Input.bind_admission([scalar], limits, forecast, source, 1)
      assert is_map(raw_admission) and map_size(raw_admission) == 2

      for copies <- [1, 2] do
        assert Input.bind_admission([wrong], limits, forecast, source, copies) == error
      end

      paid =
        for copies <- [1, 2] do
          assert_paid(
            Input.bind_admission([wrong], limits, mode, source, copies),
            error,
            mode
          )
        end

      assert [one_copy, two_copies] = paid
      assert one_copy == two_copies

      for {words, pins} <- paid do
        assert words >= initial_words + @encoder_words
        assert words < initial_words + 2 * @encoder_words
        assert pins >= initial_pins + @encoder_pins
      end

      assert wrong == %{encoded: <<131, 97, 7>>, source_term: term}
    end)
  end

  test "metered_decode_refusal_preserves_issued_control_without_decoding" do
    with_context(fn limits, _source, mode ->
      {:metered, forecast, initial_words, initial_pins} = mode
      invalid = [<<0>>]
      error = {:error, {:source_decode, 0, :invalid_external_term}}
      assert Input.decode_admission(invalid, limits, forecast) == error
      {words, pins} = assert_paid(Input.decode_admission(invalid, limits, mode), error, mode)
      # Native's two refusal tuples use7W; the final metered triple uses4W.
      assert words >= initial_words + 11
      assert pins >= initial_pins + 88

      results =
        for size <- [1_024, 65_536] do
          term = :binary.copy("x", size)
          encoded = TermCodec.encode(term)
          assert :binary.referenced_byte_size(term) == size
          assert :binary.referenced_byte_size(encoded) == byte_size(encoded)
          raw = Input.decode_admission([encoded], limits, forecast)
          assert {:ok, admission} = raw
          assert is_map(admission) and map_size(admission) == 2
          paid = assert_paid(Input.decode_admission([encoded], limits, mode), raw, mode)
          {admission, paid}
        end

      assert [{small_forecast, small_paid}, {large_forecast, large_paid}] = results
      assert large_forecast.pinned_bytes > small_forecast.pinned_bytes
      assert small_paid == large_paid
      assert invalid == [<<0>>]
    end)
  end

  test "metered_query_refusal_preserves_prior_lookup_debt_and_raw_results" do
    with_context(fn limits, _source, mode ->
      {:metered, forecast, initial_words, initial_pins} = mode
      nil_name = :binary.copy("nil")
      true_name = :binary.copy("true")
      one = %{atoms: [nil_name], exports: []}
      two = %{atoms: [nil_name, true_name], exports: []}
      word_limits = %{limits | decoded_words: forecast.decoded_words + 280}
      fact_limits = %{limits | facts: 1}
      word_error = {:error, {:limit, :decoded_words}}
      fact_error = {:error, {:limit, :facts}}
      assert Atom.to_string(nil) == nil_name and Atom.to_string(true) == true_name
      assert :binary.referenced_byte_size(nil_name) == 3
      assert :binary.referenced_byte_size(true_name) == 4
      assert Atom.to_string(:q) == "q"
      q = :binary.copy("q")
      q_queries = %{atoms: [q], exports: []}
      q_observed = %{atoms: %{q => true}, exports: %{}}
      assert Input.query(q_queries, limits) == {:ok, q_observed}

      assert Input.query_admission(q_queries, limits, %{decoded_words: 100, pinned_bytes: 800}) ==
               {:ok, q_observed, %{decoded_words: 256, pinned_bytes: 2_049}}

      observed = %{atoms: %{nil_name => true}, exports: %{}}

      raw_one =
        {:ok, observed,
         %{
           decoded_words: forecast.decoded_words + 156,
           pinned_bytes: forecast.pinned_bytes + 8 * 156 + byte_size(nil_name)
         }}

      assert Input.query_admission(one, word_limits, forecast) == raw_one
      assert Input.query_admission(two, word_limits, forecast) == word_error
      assert Input.query_admission(two, fact_limits, forecast) == fact_error

      {early_words, early_pins} =
        assert_paid(Input.query_admission(two, fact_limits, mode), fact_error, mode)

      assert early_words >= initial_words + 4 and early_pins >= initial_pins + 32

      {one_words, one_pins} =
        assert_paid(Input.query_admission(one, word_limits, mode), raw_one, mode)

      {late_words, late_pins} =
        assert_paid(Input.query_admission(two, word_limits, mode), word_error, mode)

      # One successful existing lookup and its first dynamic map use at least9W.
      assert late_words >= initial_words + 9 + 4
      assert late_pins >= initial_pins + 72 + 32
      # Only the old final success graph (<=20W/160P) can be absent at refusal.
      assert late_words >= one_words - 20 and late_pins >= one_pins - 160
      assert two == %{atoms: [nil_name, true_name], exports: []}
    end)
  end

  test "metered_success_and_carry_preserve_funded_return_credit" do
    with_context(fn limits, _source, _mode ->
      forecast = %{decoded_words: 100, pinned_bytes: 800}
      mode = {:metered, forecast, 100, 800}
      refusal = {:error, :source_code_changed}

      both = %{limits | decoded_words: 105, pinned_bytes: 840}
      assert Meter.finish(:ok, mode, both) == {{:error, {:limit, :decoded_words}}, 104, 832}

      for axis <- [:decoded_words, :pinned_bytes] do
        five = Map.put(limits, axis, if(axis == :decoded_words, do: 105, else: 840))
        nine = Map.put(limits, axis, if(axis == :decoded_words, do: 109, else: 872))
        fourteen = Map.put(limits, axis, if(axis == :decoded_words, do: 114, else: 912))
        financial = {:error, {:limit, axis}}

        # A final result must remain payable when another operation is refused.
        assert Meter.finish(:ok, mode, five) == {financial, 104, 832}
        success = Meter.finish(:ok, mode, nine)
        assert success == {:ok, 104, 832}
        stopped = Meter.carry(forecast, success, nine)
        assert stopped == {financial, 108, 864}
        assert Meter.carry(forecast, stopped, nine) == stopped
        assert Meter.finish(refusal, stopped, nine) == stopped

        success = Meter.finish(:ok, mode, fourteen)
        assert success == {:ok, 104, 832}
        live = Meter.carry(forecast, success, fourteen)
        assert live == {:metered, forecast, 109, 872}
        assert Meter.finish(refusal, live, fourteen) == {refusal, 113, 904}
        unknown = Meter.finish(refusal, live, :unsettled)
        assert unknown == {:unsettled, refusal, 114, 912}
        assert Meter.carry(forecast, unknown, fourteen) == unknown
        assert Meter.finish(:ok, unknown, fourteen) == unknown

        assert Meter.finish(refusal, mode, five) == {refusal, 104, 832}
        assert Meter.finish(refusal, mode, :unsettled) == {:unsettled, refusal, 105, 840}
      end
    end)
  end

  test "metered_source_guard_refuses_before_unfunded_frame_entry" do
    with_context(fn limits, source, _mode ->
      limits = %{limits | decoded_words: 1_205}
      forecast = %{decoded_words: 0, pinned_bytes: 0}
      mode = {:metered, forecast, 1_000, 8_000}
      donor = %{encoded: TermCodec.encode(7), source_term: 7}
      guard = Ferricstore.PolicyRuntimeCapture.Input.Binding
      owner = self()

      {worker, monitor} =
        spawn_monitor(fn ->
          receive do
            :go ->
              outcome = Input.bind_admission([donor], limits, mode, source, 1)
              send(owner, {:guard_refusal, self(), outcome})
              receive do: (:stop -> :ok)
          end
        end)

      try do
        1 = :erlang.trace_pattern({guard, :source_current, 3}, true, [:local])
        1 = :erlang.trace(worker, true, [:call, {:tracer, self()}])
        send(worker, :go)

        assert_receive {:guard_refusal, ^worker,
                        {{:error, {:limit, :decoded_words}}, 1_200, paid_pins}},
                       5_000

        assert is_integer(paid_pins) and paid_pins >= 8_000
        fence = :erlang.trace_delivered(worker)
        assert_receive {:trace_delivered, ^worker, ^fence}, 5_000
        refute_received {:trace, ^worker, :call, {^guard, :source_current, [_, _, _]}}
      after
        if Process.alive?(worker), do: :erlang.trace(worker, false, [:call])
        :erlang.trace_pattern({guard, :source_current, 3}, false, [:local])
        send(worker, :stop)
        assert_receive {:DOWN, ^monitor, :process, ^worker, _reason}, 5_000
      end
    end)
  end

  defp with_context(run) do
    SourceTest.bootstrap!()
    context = :persistent_term.get({Capture, :verified_bootstrap})
    limits = %{SourceTest.limits() | decoded_words: 1_048_576, pinned_bytes: 67_108_864}
    forecast = %{decoded_words: 200_000, pinned_bytes: 8_388_608}
    # Fixture construction and this component opening bank are not a Native permit.
    run.(limits, context.source, {:metered, forecast, 1_000, 8_000})
  end

  defp assert_paid(outcome, expected, {:metered, forecast, initial_words, initial_pins}) do
    assert {^expected, words, pins} = outcome
    assert is_integer(words) and is_integer(pins)
    assert words >= initial_words and words < forecast.decoded_words
    assert pins >= initial_pins and pins < forecast.pinned_bytes
    {words, pins}
  end
end
