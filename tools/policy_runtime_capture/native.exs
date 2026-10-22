defmodule Ferricstore.PolicyRuntimeCapture.Native do
  @moduledoc false
  @package "28.3.1"
  @revision "aa5b1d3366762ff8f420f2aa6a52604b481530c9"
  @loaded_artifact {__MODULE__, :loaded_artifact}

  def load(path, expected) when is_binary(path) and is_map(expected) do
    with {:ok, actual} <- runtime(),
         true <- Enum.all?(actual, fn {key, value} -> Map.get(expected, key) == value end),
         true <- expected[:source_revision] == @revision,
         true <- is_binary(expected[:header_digest]) and byte_size(expected.header_digest) == 32,
         {:ok, identity} <- artifact(path) do
      case :persistent_term.get(@loaded_artifact, nil) do
        nil -> load_artifact(identity, expected)
        {^identity, ^expected} -> if abi() == expected, do: :ok, else: artifact_mismatch()
        _ -> artifact_mismatch()
      end
    else
      {:error, _} = error -> error
      _ -> {:error, :abi_mismatch}
    end
  end

  def load(_, _), do: {:error, :invalid_load}
  def abi, do: {:error, :not_loaded}
  def capture_terms(_, _), do: {:error, :not_loaded}
  def budget_register(_, _, _, _), do: {:error, :not_loaded}
  def budget_enter(_), do: {:error, :not_loaded}
  def budget_bind(_, _, _), do: {:error, :not_loaded}
  def budget_read(_, _, _), do: {:error, :not_loaded}
  def budget_control(_, _, _, _), do: {:error, :not_loaded}

  defp artifact(path) do
    prefix = Path.expand(path)

    case bounded_file(prefix <> ".so", 33_554_432) do
      {:ok, bytes} -> {:ok, {prefix, :crypto.hash(:sha256, bytes)}}
      _ -> {:error, {:native_load, :artifact_unavailable}}
    end
  end

  defp load_artifact({path, _digest} = identity, expected) do
    case :erlang.load_nif(String.to_charlist(path), expected) do
      :ok ->
        with {:ok, ^identity} <- artifact(path), ^expected <- abi() do
          :persistent_term.put(@loaded_artifact, {identity, expected})
          :ok
        else
          _ -> artifact_mismatch()
        end

      {:error, {:reload, _}} ->
        if :persistent_term.get(@loaded_artifact, nil) == {identity, expected} and
             abi() == expected, do: :ok, else: artifact_mismatch()

      {:error, reason} ->
        {:error, {:native_load, reason}}
    end
  end

  defp artifact_mismatch, do: {:error, {:unsupported_vm, :native_artifact_mismatch}}

  defp runtime do
    runtime_identity()
  rescue
    _ -> {:error, {:unsupported_vm, :runtime_identity_unavailable}}
  end

  @doc false
  def runtime_identity do
    root = List.to_string(:code.root_dir())

    {:ok, version} =
      bounded_file(Path.join([root, "releases", System.otp_release(), "OTP_VERSION"]), 256)

    package = String.trim(version)

    actual = %{
      otp_package: package,
      otp_release: System.otp_release(),
      erts_version: List.to_string(:erlang.system_info(:version)),
      build_type: :erlang.system_info(:build_type),
      word_size: :erlang.system_info(:wordsize),
      endianness: :erlang.system_info(:endian)
    }

    if actual == %{
         otp_package: @package,
         otp_release: "28",
         erts_version: "16.2",
         build_type: :opt,
         word_size: 8,
         endianness: :little
       },
       do: {:ok, actual},
       else: {:error, {:unsupported_vm, actual}}
  end

  defp bounded_file(path, maximum) do
    with {:ok, %{type: :regular}} <- File.lstat(path),
         {:ok, file} <- :file.open(String.to_charlist(path), [:read, :binary, :raw]) do
      try do
        with {:ok, info} <- :file.read_file_info(file),
             true <- elem(info, 2) == :regular and elem(info, 1) <= maximum do
          case :file.read(file, maximum + 1) do
            {:ok, bytes} when byte_size(bytes) <= maximum ->
              if :file.read(file, 1) == :eof, do: {:ok, bytes}, else: :error

            :eof ->
              {:ok, <<>>}

            _ ->
              :error
          end
        else
          _ -> :error
        end
      after
        :file.close(file)
      end
    else
      _ -> :error
    end
  end
end
