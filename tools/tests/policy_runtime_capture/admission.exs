defmodule Ferricstore.PolicyRuntimeCaptureAdmissionTest do
  use ExUnit.Case, async: false
  alias Ferricstore.PolicyRuntimeCapture, as: Capture
  alias Ferricstore.PolicyRuntimeCapture.Native
  alias Ferricstore.PolicyRuntimeCaptureTest, as: SourceTest
  alias Ferricstore.TermCodec

  @controller Path.expand("../policy_runtime_capture_test.exs", __DIR__)
  @generated "/private/tmp/ferricstore2-policy-runtime-capture-otp-28.3.1/native"

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
