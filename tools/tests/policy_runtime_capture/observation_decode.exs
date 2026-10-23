defmodule Ferricstore.PolicyRuntimeCaptureObservationDecodeTest do
  use ExUnit.Case, async: false
  alias Ferricstore.PolicyRuntimeCapture.Input
  alias Ferricstore.PolicyRuntimeCapture.Observation
  alias Ferricstore.PolicyRuntimeCaptureTest, as: SourceTest
  alias Ferricstore.TermCodec

  # Fixture roots, callbacks, tags and observer copies are outside synthetic G.
  # This arithmetic base is not a Native permit or proof of original ownership.
  test "metered_decode_issues_only_new_values_and_preserves_raw_forecasts" do
    with_context(fn limits, forecast, gw, gp ->
      term = {7, [8, 9]}
      donor = TermCodec.encode(term)
      error = {:error, :invalid_donors}

      for invalid <- [:invalid, [donor | :invalid]] do
        assert Observation.decode(invalid, invalid, limits, forecast) == error
      end

      compressed = :erlang.term_to_binary(:binary.copy(<<17>>, 1_024), compressed: 9)
      assert <<131, 80, _::binary>> = compressed

      for invalid <- [compressed, donor <> <<0>>] do
        assert TermCodec.decode(invalid) == {:error, :invalid_external_term}

        assert Observation.decode([invalid], [invalid], limits, forecast) ==
                 {:error, {:source_decode, 0, :invalid_external_term}}
      end

      results =
        for size <- [1_024, 65_536] do
          historical = :binary.copy(<<17>>, size)
          old = TermCodec.encode(historical)
          all = [old, donor]
          assert :binary.referenced_byte_size(historical) == size
          assert :binary.referenced_byte_size(old) == byte_size(old)
          assert {:ok, preflight} = Input.decode_admission(all, limits, forecast)

          expected_forecast = %{
            decoded_words: preflight.decoded_words + 35,
            pinned_bytes: preflight.pinned_bytes + 280
          }

          expected = {:ok, [term], expected_forecast}
          assert Observation.decode([donor], all, limits, forecast) == expected

          paid =
            with_decode_trace(
              fn -> Observation.decode([donor], all, limits, {:metered, forecast, gw, gp}) end,
              fn result, events ->
                assert [{:trace, worker, :call, _} | _] = events

                assert events == [
                         {:trace, worker, :call, {TermCodec, :decode, [donor]}},
                         {:trace, worker, :return_from, {TermCodec, :decode, 1}, {:ok, term}}
                       ]

                assert {^expected, words, pins} = result
                assert is_integer(words) and gw < words and words < forecast.decoded_words
                assert is_integer(pins) and gp < pins and pins < forecast.pinned_bytes
                assert words <= limits.decoded_words and pins <= limits.pinned_bytes
                {words, pins}
              end
            )

          assert all == [old, donor] and donor == TermCodec.encode(term)
          assert :binary.referenced_byte_size(historical) == size
          assert :binary.referenced_byte_size(old) == byte_size(old)
          {expected_forecast, paid}
        end

      assert [{small_forecast, small_paid}, {large_forecast, large_paid}] = results
      assert small_forecast.pinned_bytes < large_forecast.pinned_bytes
      assert small_paid == large_paid
    end)
  end

  test "metered_decode_retains_completed_prefix_on_late_source_error" do
    with_context(fn limits, forecast, gw, gp ->
      term = List.duplicate(7, 4_096)
      good = TermCodec.encode(term)
      # Standalone decode lacks ATOM_TRANS in its fake distribution context.
      bad = <<131, 82, 0>>
      donors = [good, bad]
      error = {:error, {:source_decode, 1, :invalid_external_term}}
      assert <<131, 107, 4_096::unsigned-big-16, bytes::binary>> = good
      assert bytes == :binary.copy(<<7>>, 4_096)
      assert {:ok, _preflight} = Input.decode_admission(donors, limits, forecast)
      assert TermCodec.decode(bad) == {:error, :invalid_external_term}
      assert Observation.decode(donors, donors, limits, forecast) == error

      with_decode_trace(
        fn -> Observation.decode(donors, donors, limits, {:metered, forecast, gw, gp}) end,
        fn result, events ->
          assert [{:trace, worker, :call, _} | _] = events

          assert events == [
                   {:trace, worker, :call, {TermCodec, :decode, [good]}},
                   {:trace, worker, :return_from, {TermCodec, :decode, 1}, {:ok, term}},
                   {:trace, worker, :call, {TermCodec, :decode, [bad]}},
                   {:trace, worker, :return_from, {TermCodec, :decode, 1},
                    {:error, :invalid_external_term}}
                 ]

          assert {^error, words, pins} = result
          # The completed cons graph and final outcome are disjoint issued cohorts.
          assert is_integer(words) and words >= gw + 8_192 + 4
          assert is_integer(pins) and pins >= gp + 65_536 + 32
          assert words < forecast.decoded_words and pins < forecast.pinned_bytes
          assert words <= limits.decoded_words and pins <= limits.pinned_bytes
        end
      )

      assert donors == [good, bad] and TermCodec.encode(term) == good
      assert bad == <<131, 82, 0>>
    end)
  end

  test "metered_decode_budget_refusal_keeps_preflight_debt_without_decoding" do
    with_context(fn limits, forecast, gw, gp ->
      donor = TermCodec.encode(7)
      donors = [donor, donor]
      assert donor == <<131, 97, 7>>
      assert {:ok, preflight} = Input.decode_admission(donors, limits, forecast)
      assert preflight.decoded_words == forecast.decoded_words + 115
      assert preflight.pinned_bytes > forecast.pinned_bytes
      limited = %{limits | decoded_words: forecast.decoded_words + 161}
      error = {:error, {:limit, :decoded_words}}
      assert Input.decode_admission(donors, limited, forecast) == {:ok, preflight}
      assert Observation.decode(donors, donors, limited, forecast) == error
      mode = {:metered, forecast, gw, gp}

      # This source-derived component prefix is independent of returned forecasts.
      assert Input.decode_admission(donors, limited, mode) ==
               {{:ok, preflight}, gw + 78, gp + 2_432}

      with_decode_trace(
        fn -> Observation.decode(donors, donors, limited, mode) end,
        fn result, events ->
          assert events == []
          assert {^error, words, pins} = result
          assert is_integer(words) and words >= gw + 78 + 4
          assert is_integer(pins) and pins >= gp + 2_432 + 32
          assert words < forecast.decoded_words and pins < forecast.pinned_bytes
          assert words <= limited.decoded_words and pins <= limited.pinned_bytes
        end
      )

      assert donors == [<<131, 97, 7>>, <<131, 97, 7>>]
    end)
  end

  defp with_context(run) do
    SourceTest.bootstrap!()
    limits = %{SourceTest.limits() | decoded_words: 1_048_576, pinned_bytes: 67_108_864}
    forecast = %{decoded_words: 200_000, pinned_bytes: 8_388_608}
    run.(limits, forecast, 1_000, 8_000)
  end

  defp with_decode_trace(call, check) do
    target = {TermCodec, :decode, 1}
    assert {:traced, false} = :erlang.trace_info(target, :traced)
    owner = self()

    {worker, monitor} =
      spawn_monitor(fn ->
        receive do
          :go ->
            result = call.()
            send(owner, {self(), :decode_result, result})

            receive do
              :stop -> :ok
            end

          :stop ->
            :ok
        end
      end)

    collect = fn collect, fence, acc ->
      receive do
        {:trace, ^worker, :call, _} = event -> collect.(collect, fence, [event | acc])
        {:trace, ^worker, :return_from, _, _} = event -> collect.(collect, fence, [event | acc])
        {:trace_delivered, ^worker, ^fence} -> Enum.reverse(acc)
      after
        5_000 -> flunk("decoder trace delivery fence did not arrive")
      end
    end

    {result, events} =
      try do
        assert {:flags, []} = :erlang.trace_info(worker, :flags)

        try do
          assert :erlang.trace_pattern(target, [{:_, [], [{:return_trace}]}], [:local]) == 1
          assert :erlang.trace(worker, true, [:call, {:tracer, owner}]) == 1
          send(worker, :go)

          result =
            receive do
              {^worker, :decode_result, actual} -> actual
            after
              5_000 -> flunk("decoder worker result did not arrive")
            end

          fence = :erlang.trace_delivered(worker)
          {result, collect.(collect, fence, [])}
        after
          if Process.alive?(worker), do: :erlang.trace(worker, false, [:call])
          :erlang.trace_pattern(target, false, [:local])
        end
      after
        send(worker, :stop)

        receive do
          {:DOWN, ^monitor, :process, ^worker, _reason} -> :ok
        after
          1_000 ->
            Process.exit(worker, :kill)
            assert_receive {:DOWN, ^monitor, :process, ^worker, _reason}, 5_000
        end

        receive do
          {^worker, :decode_result, _late_result} -> :ok
        after
          0 -> :ok
        end

        fence = :erlang.trace_delivered(worker)
        _discarded = collect.(collect, fence, [])
        Process.demonitor(monitor, [:flush])
      end

    check.(result, events)
  end
end
