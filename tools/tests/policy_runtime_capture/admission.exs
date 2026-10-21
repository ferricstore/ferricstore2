defmodule Ferricstore.PolicyRuntimeCaptureAdmissionTest do
  use ExUnit.Case, async: false
  alias Ferricstore.PolicyRuntimeCapture, as: Capture
  alias Ferricstore.PolicyRuntimeCapture.Input
  alias Ferricstore.PolicyRuntimeCapture.Native
  alias Ferricstore.PolicyRuntimeCaptureTest, as: SourceTest
  alias Ferricstore.TermCodec

  @controller Path.expand("../policy_runtime_capture_test.exs", __DIR__)
  @generated "/private/tmp/ferricstore2-policy-runtime-capture-otp-28.3.1/native"

  test "original_inputs_remain_funded_through_capture_and_extend" do
    SourceTest.bootstrap!()
    bytes = :binary.copy("x", 600_000)
    empty = %{atoms: [], exports: []}
    queries = Map.put(empty, :ignored, bytes)
    limits = SourceTest.limits()
    tight = %{limits | pinned_bytes: 128_000}
    expected_hash = sha(TermCodec.encode(empty))
    fresh = Capture.capture_epoch([], queries, nil, tight)

    case fresh do
      {:ok, unexpected, _} -> assert :ok = Capture.close_epoch(unexpected)
      _ -> :ok
    end

    assert {:ok, session, first} = Capture.capture_epoch([], empty, nil, tight)

    try do
      assert {:ok, adequate, captured} = Capture.capture_epoch([], queries, nil, limits)

      try do
        wrong_parent = Capture.extend_epoch(session, [], queries, nil)
        extension = Capture.extend_epoch(session, [], queries, first.identity)
        retry = Capture.extend_epoch(session, [], empty, first.identity)
        fresh_status = if match?({:ok, _, _}, fresh), do: :unexpected_success, else: fresh
        extend_status = if match?({:ok, _}, extension), do: :unexpected_success, else: extension
        retry_status = if match?({:ok, _}, retry), do: :ok, else: retry

        assert fresh == {:error, {:limit, :pinned_bytes}},
               "eager original-input results: #{inspect({fresh_status, wrong_parent, extend_status, retry_status}, limit: :infinity)}"

        assert captured.query_hash == expected_hash
        assert captured.queries == %{atoms: %{}, exports: %{}}
        assert captured.donor_hashes == []

        for field <- [:hashes, :atoms, :nodes, :exports, :local_funs, :orders] do
          assert Map.fetch!(captured, field) == []
        end

        assert wrong_parent == {:error, :parent_mismatch}
        assert extension == {:error, {:limit, :pinned_bytes}}
        assert {:ok, again} = retry
        assert again.parent == first.identity
        assert again.session_id == first.session_id
        assert first.query_hash == expected_hash
        assert first.queries == %{atoms: %{}, exports: %{}}
        assert first.donor_hashes == []

        for field <- [
              :donor_hashes,
              :query_hash,
              :queries,
              :orders,
              :hashes,
              :atoms,
              :nodes,
              :exports,
              :local_funs,
              :abi
            ] do
          assert Map.fetch!(again, field) == Map.fetch!(first, field)
        end

        assert queries == %{atoms: [], exports: [], ignored: bytes}
        assert bytes == :binary.copy("x", 600_000)
        assert :binary.referenced_byte_size(bytes) == 600_000
      after
        assert :ok = Capture.close_epoch(adequate)
      end
    after
      assert :ok = Capture.close_epoch(session)
    end
  end

  test "live_measurement_counts_actual_graph_and_report_control" do
    SourceTest.bootstrap!()
    limits = %{SourceTest.limits() | donor_bytes: 0, facts: 0, text_bytes: 0, output_bytes: 0}
    bytes = :binary.copy("x", 4_096)
    graph = %{first: bytes, second: bytes}
    nested = Enum.reduce(1..64, [], fn _, value -> {value} end)
    empty = Native.capture_terms({:measure_terms, []}, limits)
    repeated = Native.capture_terms({:measure_terms, graph}, limits)
    deep = Native.capture_terms({:measure_terms, nested}, limits)
    low_words = Native.capture_terms({:measure_terms, []}, %{limits | decoded_words: 21})
    low_pins = Native.capture_terms({:measure_terms, []}, %{limits | pinned_bytes: 175})

    assert empty == {:ok, %{live_words: 22, live_pinned_bytes: 176, live_terms: 3}},
           "eager live reports: #{inspect({empty, repeated, deep, low_words, low_pins}, limit: :infinity)}"

    assert {:ok, report} = repeated
    assert_live_report!(report, limits)
    assert report.live_pinned_bytes >= 8_192
    assert deep == {:ok, %{live_words: 150, live_pinned_bytes: 1_200, live_terms: 67}}
    assert low_words == {:error, {:limit, :decoded_words}}
    assert low_pins == {:error, {:limit, :pinned_bytes}}

    assert Native.capture_terms({:measure_terms, nested}, %{limits | depth: 63}) ==
             {:error, {:limit, :depth}}

    for malformed <- [:measure_terms, {:measure_terms}, {:measure_terms, [], []}] do
      assert Native.capture_terms(malformed, limits) == {:error, :invalid_terms}

      assert Native.capture_terms(malformed, %{limits | depth: 65}) ==
               {:error, :invalid_limits}
    end

    assert Native.capture_terms({:measure_terms, []}, limits) == empty
    assert Native.capture_terms({:measure_terms, graph}, limits) == repeated
    assert Native.capture_terms({:measure_terms, nested}, limits) == deep
    assert graph == %{first: bytes, second: bytes}
    assert bytes == :binary.copy("x", 4_096)
    assert :binary.referenced_byte_size(bytes) == 4_096
    assert Enum.reduce(1..64, nested, fn _, {value} -> value end) == []
  end

  test "query_result_builders_preflight_control_before_observation" do
    SourceTest.bootstrap!()
    limits = SourceTest.limits()

    queries = %{
      atoms: ["nil", "nil"],
      exports: [{"erlang", "length", 1}, {"erlang", "length", 1}],
      ignored: true
    }

    expected = %{atoms: %{"nil" => true}, exports: %{{"erlang", "length", 1} => true}}
    low_words = Input.query(queries, %{limits | decoded_words: 0})
    low_pins = Input.query(queries, %{limits | pinned_bytes: 0})
    adequate = Input.query(queries, limits)
    names = Enum.map(0..32, fn index -> <<255, index>> end)
    wide = Input.query(%{atoms: names, exports: []}, limits)

    improper =
      try do
        Input.query(%{atoms: ["true" | :bad_tail], exports: []}, limits)
      rescue
        ArgumentError -> :raised_argument_error
      end

    assert low_words == {:error, {:limit, :decoded_words}},
           "eager query returns: #{inspect({low_words, low_pins, adequate, wide, improper})}"

    assert low_pins == {:error, {:limit, :pinned_bytes}}
    assert adequate == {:ok, expected}
    assert wide == {:ok, %{atoms: Map.new(names, &{&1, false}), exports: %{}}}
    assert improper == {:error, :invalid_queries}

    invalid = %{atoms: [7], exports: []}

    assert Input.query(invalid, %{limits | facts: 0, decoded_words: 0}) ==
             {:error, {:limit, :facts}}

    assert Input.query(invalid, %{limits | decoded_words: 0}) == {:error, :invalid_queries}

    assert Input.query(queries, %{limits | text_bytes: 0, decoded_words: 0, pinned_bytes: 0}) ==
             {:error, {:limit, :text_bytes}}

    assert Input.query(%{atoms: :invalid, exports: []}, limits) == {:error, :invalid_queries}
    assert Input.query(%{atoms: [], exports: :invalid}, limits) == {:error, :invalid_queries}
    assert Input.query(%{}, limits) == {:error, :invalid_queries}
    assert Input.query(queries, limits) == {:ok, expected}

    assert queries == %{
             atoms: ["nil", "nil"],
             exports: [{"erlang", "length", 1}, {"erlang", "length", 1}],
             ignored: true
           }

    assert names == Enum.map(0..32, fn index -> <<255, index>> end)
  end

  test "local_wrapper_preflight_uses_bounded_native_stack" do
    SourceTest.bootstrap!()
    assert child("local-chain") == :ok
  end

  test "direct_preflight_enforces_donor_bytes" do
    SourceTest.bootstrap!()
    limits = SourceTest.limits()
    donor = <<131, 106>>

    assert {:error, {:limit, :donor_bytes}} =
             Native.capture_terms({:preflight, [donor]}, %{limits | donor_bytes: 0})

    assert {:ok, report} = Native.capture_terms({:preflight, [donor]}, limits)
    assert report.decoded_words in 0..limits.decoded_words
    assert report.pinned_bytes in 0..limits.pinned_bytes
  end

  test "compressed_and_trailing_manifest_refuse_before_decode" do
    SourceTest.bootstrap!()
    original = SourceTest.config()
    raw = File.read!(original.nif_path <> ".manifest.term")
    fixture = fixture(original, raw <> <<0>>)

    assert Capture.bootstrap(fixture) ==
             {:error, {:unsupported_vm, :compiled_manifest_mismatch}}

    assert child("compressed-manifest") == :ok
    assert {:ok, _} = Capture.bootstrap(original)
  end

  test "reload_binds_actual_native_artifact" do
    abi = SourceTest.bootstrap!()
    original = SourceTest.config()
    original_bytes = File.read!(original.nif_path <> ".so")
    changed_bytes = original_bytes <> <<0>>
    assert sha(changed_bytes) != sha(original_bytes)

    metadata =
      original.nif_path
      |> Kernel.<>(".manifest.term")
      |> File.read!()
      |> :erlang.binary_to_term([:safe])
      |> Map.put(:native_sha256, sha(changed_bytes))

    %{source_revision: _, headers: _, header_digest: _, native_sha256: _} = metadata
    assert map_size(metadata) == 4
    raw = :erlang.term_to_binary(metadata, [:deterministic])
    changed = fixture(original, raw, changed_bytes)

    assert Capture.bootstrap(changed) ==
             {:error, {:unsupported_vm, :native_artifact_mismatch}}

    assert Capture.bootstrap(original) == {:ok, abi}
    assert Native.abi() == abi
  end

  test "combines_query_donor_and_capture_owner_frontiers" do
    SourceTest.bootstrap!()
    query = :binary.copy("q", 600_000)
    donor = :binary.copy(<<131, 109, 199_994::unsigned-big-32, 0::size(199_994)-unit(8)>>)
    assert byte_size(query) == 600_000
    assert :binary.referenced_byte_size(query) == 600_000
    assert byte_size(donor) == 200_000
    assert :binary.referenced_byte_size(donor) == 200_000
    queries = %{atoms: [query], exports: []}
    limits = %{SourceTest.limits() | text_bytes: 1_048_576, output_bytes: 1_048_576}
    assert_raise ArgumentError, fn -> String.to_existing_atom(query) end

    # Warm the same real route before observing the VM's global atom count.
    assert {:ok, warm, _} = Capture.capture_epoch([donor], queries, nil, limits)
    assert :ok = Capture.close_epoch(warm)
    before = :erlang.system_info(:atom_count)

    result = Capture.capture_epoch([donor], queries, nil, %{limits | pinned_bytes: 1_500_000})

    case result do
      {:ok, unexpected, _} -> assert :ok = Capture.close_epoch(unexpected)
      _ -> :ok
    end

    assert result == {:error, {:limit, :pinned_bytes}}

    assert byte_size(query) == 600_000
    assert sha(donor) == sha(<<131, 109, 199_994::unsigned-big-32, 0::size(199_994)-unit(8)>>)
    assert {:ok, keeper, captured} = Capture.capture_epoch([donor], queries, nil, limits)
    assert captured.donor_hashes == [sha(donor)]
    assert captured.queries.atoms[query] == false
    assert :ok = Capture.close_epoch(keeper)
    assert :erlang.system_info(:atom_count) == before
    assert_raise ArgumentError, fn -> String.to_existing_atom(query) end
    assert :binary.referenced_byte_size(query) == 600_000
    assert byte_size(donor) == 200_000
  end

  test "live_preflight_preserves_policy_depth_without_fact_emission" do
    SourceTest.bootstrap!()
    term = Enum.reduce(1..64, nil, fn _, value -> {value} end)
    encoded = TermCodec.encode(term)
    donors = [%{encoded: encoded, source_term: term}]
    queries = %{atoms: [], exports: []}
    roots = {:preflight, {:anchors, [donors, queries]}}
    limits = %{SourceTest.limits() | facts: 0, text_bytes: 0, output_bytes: 0}

    assert {:ok, report} = Native.capture_terms(roots, limits)
    assert_live_report!(report, limits)
    assert report.live_words >= 128
    assert report.live_terms >= 65

    assert Native.capture_terms(roots, %{limits | depth: 63}) ==
             {:error, {:limit, :depth}}

    assert Enum.reduce(1..64, term, fn _, {value} -> value end) == nil
    assert [%{encoded: ^encoded, source_term: ^term}] = donors
    assert queries == %{atoms: [], exports: []}
    assert byte_size(encoded) > 64
  end

  test "live_preflight_counts_pattern_and_original_backings" do
    SourceTest.bootstrap!()
    query = :binary.copy("q", 600_000)
    donor = :binary.copy(<<131, 109, 199_994::unsigned-big-32, 0::size(199_994)-unit(8)>>)
    pattern_bytes = String.duplicate("x", 4_096)
    pattern = :binary.compile_pattern(pattern_bytes)
    encoded = TermCodec.encode(pattern)
    assert byte_size(query) == 600_000
    assert :binary.referenced_byte_size(query) == 600_000
    assert byte_size(donor) == 200_000
    assert :binary.referenced_byte_size(donor) == 200_000
    assert byte_size(encoded) < 1_024
    donors = [donor, %{encoded: encoded, source_term: pattern}]
    queries = %{atoms: [query], exports: []}
    roots = {:preflight, {:anchors, [donors, queries]}}
    limits = %{SourceTest.limits() | facts: 0, text_bytes: 0, output_bytes: 0}

    assert {:ok, report} = Native.capture_terms(roots, limits)
    assert_live_report!(report, limits)
    assert report.live_pinned_bytes >= 804_096

    assert Native.capture_terms(roots, %{limits | pinned_bytes: 700_000}) ==
             {:error, {:limit, :pinned_bytes}}

    assert :binary.match(pattern_bytes, pattern) == {0, 4_096}
    assert :binary.referenced_byte_size(query) == 600_000
    assert :binary.referenced_byte_size(donor) == 200_000
    assert [^donor, %{encoded: ^encoded, source_term: ^pattern}] = donors
    assert queries == %{atoms: [query], exports: []}
  end

  test "post_binding_preflight_checks_each_anchor_and_resets_per_donor" do
    SourceTest.bootstrap!()
    {pattern, ordinary} = reference_owner_fixture()
    encoded = TermCodec.encode(pattern)
    good = %{encoded: encoded, source_term: pattern}
    mixed_term = {pattern, ordinary}
    mixed_encoded = TermCodec.encode(mixed_term)
    mixed = %{encoded: mixed_encoded, source_term: mixed_term}
    limits = SourceTest.limits()

    single = Native.capture_terms({:preflight, [good]}, limits)
    following = Native.capture_terms({:preflight, [good, encoded]}, limits)
    mixed_result = Native.capture_terms({:preflight, [mixed]}, limits)

    assert match?({:ok, _}, single), inspect({single, following, mixed_result})
    {:ok, report} = single
    assert map_size(report) == 2
    assert %{decoded_words: words, pinned_bytes: pins} = report
    assert is_integer(words) and words in 0..limits.decoded_words
    assert is_integer(pins) and pins in 0..limits.pinned_bytes
    assert following == {:error, {:source_anchor_required, 1}}
    assert mixed_result == {:error, {:source_anchor_required, 0}}
    assert :binary.match(String.duplicate("x", 4_096), pattern) == {0, 4_096}
    assert is_reference(ordinary)
    assert good == %{encoded: encoded, source_term: pattern}
    assert mixed == %{encoded: mixed_encoded, source_term: {pattern, ordinary}}
    assert byte_size(mixed_encoded) > byte_size(encoded)
    assert Native.capture_terms({:preflight, [good]}, limits) == single
  end

  test "encoder_layout_uses_pinned_headers_and_charges_control" do
    SourceTest.bootstrap!()
    limits = %{SourceTest.limits() | facts: 0, text_bytes: 0, output_bytes: 0}
    at64 = Native.capture_terms({:encoder_layout, 64}, limits)
    at65 = Native.capture_terms({:encoder_layout, 65}, limits)
    words_low = Native.capture_terms({:encoder_layout, 65}, %{limits | decoded_words: 26})
    pins_low = Native.capture_terms({:encoder_layout, 65}, %{limits | pinned_bytes: 215})

    assert match?({:ok, _}, at64), inspect({at64, at65, words_low, pins_low})
    assert {:ok, first} = at64
    assert {:ok, second} = at65

    for report <- [first, second] do
      assert map_size(report) == 5

      assert %{
               size_result_words: _,
               trap_return_words: _,
               result_words: _,
               binary_bytes: _,
               context_bytes: _
             } = report

      for value <- Map.values(report), do: assert(is_integer(value) and value >= 0)
      assert report.size_result_words > 0
      assert report.trap_return_words > 5
      assert report.result_words > 0
      assert report.context_bytes > 0
    end

    assert first.binary_bytes >= 64
    assert second.binary_bytes == first.binary_bytes + 1
    assert second.size_result_words == first.size_result_words
    assert second.trap_return_words == first.trap_return_words
    assert second.result_words == first.result_words
    assert second.context_bytes == first.context_bytes
    assert words_low == {:error, {:limit, :decoded_words}}
    assert pins_low == {:error, {:limit, :pinned_bytes}}
  end

  defp reference_owner_fixture do
    previous = :erts_debug.set_internal_state(:available_internal_state, true)

    try do
      raw = TermCodec.encode(make_ref())
      assert <<131, 90, 0, 3, _::binary>> = raw
      prefix_size = byte_size(raw) - 12
      <<prefix::binary-size(prefix_size), word0::32, word1::32, word2::32>> = raw
      assert Bitwise.band(word1, 0x30000) == 0

      marked =
        prefix <>
          <<word0::32, Bitwise.bor(word1, 0x20000)::32, Bitwise.bxor(word2, 0x80000000)::32>>

      assert {:ok, ordinary} = TermCodec.decode(marked)
      assert :erts_debug.get_internal_state({:magic_ref, ordinary}) == true
      assert {:bm, resource_ref} = pattern = :binary.compile_pattern(String.duplicate("x", 4_096))
      genuine = :erts_debug.get_internal_state({:magic_ref, resource_ref})
      assert is_tuple(genuine) and tuple_size(genuine) == 3
      {pattern, ordinary}
    after
      :erts_debug.set_internal_state(:available_internal_state, previous)
    end
  end

  defp assert_live_report!(report, limits) do
    assert map_size(report) == 3
    assert %{live_words: words, live_pinned_bytes: pins, live_terms: terms} = report
    assert is_integer(words) and words in 0..limits.decoded_words
    assert is_integer(pins) and pins in 0..limits.pinned_bytes
    assert is_integer(terms) and terms in 0..limits.terms
  end

  def run_child("local-chain") do
    SourceTest.bootstrap!()
    raw = <<131>> <> :binary.copy(<<121, 0, 0, 0, 0>>, 50_000) <> <<97, 1>>
    assert byte_size(raw) == 250_003
    limits = %{SourceTest.limits() | terms: 50_001}
    assert {:ok, report} = Native.capture_terms({:preflight, [raw]}, limits)
    assert report.decoded_words in 0..limits.decoded_words
    assert report.pinned_bytes in 0..limits.pinned_bytes
    emit(:ok)
  end

  def run_child("compressed-manifest") do
    SourceTest.bootstrap!()
    # Build the encoded body directly; never allocate the two-million-item list.
    body = <<108, 2_000_000::unsigned-big-32>> <> :binary.copy(<<106>>, 2_000_000) <> <<106>>
    raw = <<131, 80, byte_size(body)::unsigned-big-32>> <> :zlib.compress(body)
    assert byte_size(raw) < 1_048_576
    config = fixture(SourceTest.config(), raw)
    prior = Process.flag(:max_heap_size, %{size: 1_000_000, kill: true, error_logger: false})

    try do
      assert Capture.bootstrap(config) ==
               {:error, {:unsupported_vm, :compiled_manifest_mismatch}}
    after
      Process.flag(:max_heap_size, prior)
    end

    emit(:ok)
  end

  defp fixture(original, raw, native_bytes \\ nil) do
    suffix = "#{System.pid()}-#{System.unique_integer([:positive, :monotonic])}"
    directory = Path.join(@generated, "admission-#{suffix}")
    File.mkdir_p!(directory)
    prefix = Path.join(directory, "policy_runtime_capture")
    File.write!(prefix <> ".so", native_bytes || File.read!(original.nif_path <> ".so"))
    File.write!(prefix <> ".manifest.term", raw)
    %{original | nif_path: prefix}
  end

  defp child(mode) do
    executable = System.find_executable("elixir") || raise "elixir executable missing"

    {output, status} =
      System.cmd(executable, [@controller, "--admission-child", mode], stderr_to_stdout: true)

    assert status == 0, output

    line =
      output |> String.split("\n") |> Enum.find(&String.starts_with?(&1, "B2_ADMISSION_CHILD:"))

    assert is_binary(line), output

    line
    |> String.replace_prefix("B2_ADMISSION_CHILD:", "")
    |> Base.decode64!()
    |> :erlang.binary_to_term([:safe])
  end

  defp sha(bytes), do: :crypto.hash(:sha256, bytes)

  defp emit(value),
    do: IO.puts("B2_ADMISSION_CHILD:" <> Base.encode64(:erlang.term_to_binary(value)))
end
