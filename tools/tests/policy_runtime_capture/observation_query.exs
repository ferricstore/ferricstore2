defmodule Ferricstore.PolicyRuntimeCaptureObservationQueryTest do
  use ExUnit.Case, async: false
  alias Ferricstore.PolicyRuntimeCapture.Input
  alias Ferricstore.PolicyRuntimeCapture.Observation
  alias Ferricstore.PolicyRuntimeCaptureTest, as: SourceTest

  # Fixture roots, callbacks, state and incoming tags are outside each synthetic G.
  # The component opening bank is not a Native permit or proof of root ownership.
  test "metered_query_extension_retains_reached_facts_on_context_change" do
    with_context(fn limits, forecast, gw, gp, _spellings ->
      with_export_fixture(fn fixture, beam ->
        module_name = :binary.copy(Atom.to_string(fixture))
        function_name = :binary.copy("probe")
        key = {module_name, function_name, 0}
        queries = %{atoms: [], exports: [key]}
        empty = %{atoms: [], exports: []}
        assert :erlang.function_exported(fixture, :probe, 0)
        assert {:ok, snapshot} = Input.query(queries, limits)
        assert snapshot == %{atoms: %{}, exports: %{key => true}}
        state = %{queries: queries, observed: snapshot, limits: limits}

        expected_forecast = %{
          decoded_words: forecast.decoded_words + 276,
          pinned_bytes:
            forecast.pinned_bytes + 8 * 276 +
              byte_size(module_name) + byte_size(function_name)
        }

        expected = {:ok, queries, snapshot, expected_forecast}
        assert Observation.query_extension(state, empty, forecast) == expected
        :code.purge(fixture)
        assert :code.delete(fixture)
        refute :erlang.function_exported(fixture, :probe, 0)

        assert Input.query(queries, limits) ==
                 {:ok, %{atoms: %{}, exports: %{key => false}}}

        error = {:error, :runtime_context_changed}
        assert Observation.query_extension(state, empty, forecast) == error

        refused = Observation.query_extension(state, empty, {:metered, forecast, gw, gp})
        assert {^error, words, pins} = refused
        assert is_integer(words) and words >= gw + 78 and words < forecast.decoded_words
        assert is_integer(pins) and pins >= gp + 624 and pins < forecast.pinned_bytes
        assert state == %{queries: queries, observed: snapshot, limits: limits}
        :code.purge(fixture)

        assert {:module, ^fixture} =
                 :code.load_binary(fixture, ~c"observation_query_fixture", beam)

        assert :erlang.function_exported(fixture, :probe, 0)
        assert Input.query(queries, limits) == {:ok, snapshot}
        assert Observation.query_extension(state, empty, forecast) == expected

        retried =
          Observation.query_extension(
            state,
            empty,
            {:metered, forecast, words, pins}
          )

        assert {^expected, next_words, next_pins} = retried
        assert is_integer(next_words) and words < next_words
        assert is_integer(next_pins) and pins < next_pins
        assert next_words < forecast.decoded_words and next_pins < forecast.pinned_bytes
        assert state == %{queries: queries, observed: snapshot, limits: limits}
      end)
    end)
  end

  test "metered_query_extension_preserves_fact_word_order_and_prefix" do
    with_context(fn limits, forecast, gw, gp, {nil_name, true_name} ->
      assert Atom.to_string(nil) == nil_name and Atom.to_string(true) == true_name
      assert :binary.referenced_byte_size(nil_name) == 3
      assert :binary.referenced_byte_size(true_name) == 4
      old = %{atoms: [nil_name], exports: []}
      next = %{atoms: [true_name], exports: []}
      duplicate = %{atoms: [nil_name], exports: []}
      repeated = %{atoms: [true_name, nil_name, true_name], exports: []}
      combined = %{atoms: [nil_name, true_name], exports: []}
      assert {:ok, snapshot} = Input.query(old, limits)
      assert snapshot == %{atoms: %{nil_name => true}, exports: %{}}
      state = %{queries: old, observed: snapshot, limits: limits}
      assert {:ok, observed} = Input.query(combined, limits)
      assert observed == %{atoms: %{nil_name => true, true_name => true}, exports: %{}}

      expected_forecast = %{
        decoded_words: forecast.decoded_words + 294,
        pinned_bytes: forecast.pinned_bytes + 8 * 294 + 7
      }

      expected = {:ok, combined, observed, expected_forecast}
      assert Observation.query_extension(state, next, forecast) == expected
      assert Observation.query_extension(state, repeated, forecast) == expected
      fact_limits = %{limits | facts: 1, decoded_words: forecast.decoded_words}
      fact_state = %{state | limits: fact_limits}
      fact_error = {:error, {:limit, :facts}}
      assert Observation.query_extension(fact_state, duplicate, forecast) == fact_error
      early_limits = %{limits | decoded_words: forecast.decoded_words + 150}
      late_limits = %{limits | decoded_words: forecast.decoded_words + 280}
      early_state = %{state | limits: early_limits}
      late_state = %{state | limits: late_limits}
      word_error = {:error, {:limit, :decoded_words}}

      for limited <- [early_limits, late_limits] do
        assert Input.query_admission(combined, limited, forecast) == word_error

        assert Observation.query_extension(%{state | limits: limited}, next, forecast) ==
                 word_error
      end

      mode = {:metered, forecast, gw, gp}

      assert Input.query_admission(combined, early_limits, mode) ==
               {word_error, gw + 14, gp + 112}

      assert Observation.query_extension(fact_state, duplicate, mode) ==
               {fact_error, gw + 4, gp + 32}

      early = Observation.query_extension(early_state, next, mode)
      late = Observation.query_extension(late_state, next, mode)
      assert {^word_error, early_w, early_p} = early
      assert {^word_error, late_w, late_p} = late

      for {words, pins} <- [{early_w, early_p}, {late_w, late_p}] do
        assert is_integer(words) and gw < words and words < forecast.decoded_words
        assert is_integer(pins) and gp < pins and pins < forecast.pinned_bytes
      end

      assert early_w >= gw + 21 and early_p >= gp + 168
      assert {late_w - early_w, late_p - early_p} == {39, 312}
      assert state == %{queries: old, observed: snapshot, limits: limits}
      assert next == %{atoms: [true_name], exports: []}
    end)
  end

  defp with_context(run) do
    SourceTest.bootstrap!()
    limits = %{SourceTest.limits() | decoded_words: 1_048_576, pinned_bytes: 67_108_864}
    forecast = %{decoded_words: 200_000, pinned_bytes: 8_388_608}
    spellings = {:binary.copy("nil"), :binary.copy("true")}
    run.(limits, forecast, 1_000, 8_000, spellings)
  end

  defp with_export_fixture(run) do
    fixture = Ferricstore.PolicyRuntimeCaptureObservationQueryFixture
    refute :code.is_loaded(fixture)
    previous = Code.compiler_options(ignore_module_conflict: true)

    try do
      [{^fixture, beam}] =
        Code.compile_quoted(
          quote do
            defmodule Ferricstore.PolicyRuntimeCaptureObservationQueryFixture do
              def probe, do: :ok
            end
          end
        )

      run.(fixture, beam)
    after
      try do
        :code.purge(fixture)
        :code.delete(fixture)
        :code.purge(fixture)
      after
        Code.compiler_options(previous)
      end
    end
  end
end
