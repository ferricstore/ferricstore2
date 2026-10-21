defmodule Ferricstore.PolicyRuntimeCaptureAdmissionTest do
  use ExUnit.Case, async: false
  alias Ferricstore.PolicyRuntimeCapture, as: Capture
  alias Ferricstore.PolicyRuntimeCapture.Native
  alias Ferricstore.PolicyRuntimeCaptureTest, as: SourceTest

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
