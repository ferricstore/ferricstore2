defmodule Ferricstore.PolicyRuntimeCapture.Bootstrap do
  @moduledoc false
  alias Ferricstore.PolicyRuntimeCapture.Native
  alias Ferricstore.TermCodec
  @revision "aa5b1d3366762ff8f420f2aa6a52604b481530c9"
  @source_blobs %{
    "OTP_VERSION" => "e5ef00e64583d7a10e5aa68e569c6f9ca866e9e5",
    "erts/vsn.mk" => "82c07c8f6d3e74d134321f85cfd8a87da3b13040",
    "erts/emulator/beam/erl_fun.h" => "0f8fdeba319de236fabed4ccb62908dfa1c59718",
    "erts/emulator/beam/erl_term.h" => "34a5debeb587ea27253d2377a0121d04fd5d435d"
  }
  @codec_sha "fa1f00dfb407569a19b0f36760fec83ba10defeab7178ba7154ed4227d3f5ee6"

  def verify(config) do
    with {:ok, actual} <- Native.runtime_identity(),
         :ok <- check_root(config.otp_root),
         :ok <- source_files(config.source_root),
         {:ok, header_digest} <- manifest(config),
         {:ok, source} <- source_codec(config.elixir_source_root) do
      abi = Map.merge(actual, %{source_revision: @revision, header_digest: header_digest})

      with :ok <- Native.load(config.nif_path, abi),
           ^abi <- Native.abi() do
        {:ok, %{abi: abi, source: source}}
      else
        {:error, _} = error -> error
        _ -> {:error, :abi_mismatch}
      end
    end
  end

  defp source_codec(root) do
    path = Path.join(root, "apps/ferricstore/lib/ferricstore/term_codec.ex")

    with {:ok, bytes} <- bounded_file(path, 8_192),
         true <- Base.encode16(sha(bytes), case: :lower) == @codec_sha do
      prior = Code.compiler_options(ignore_module_conflict: true)

      try do
        [{TermCodec, _}] = Code.compile_string(bytes, path)
      after
        Code.compiler_options(prior)
      end

      {:ok, %{md5: TermCodec.module_info(:md5), source_sha: sha(bytes)}}
    else
      _ -> {:error, {:unsupported_vm, :source_codec_mismatch}}
    end
  end

  defp check_root(root),
    do:
      if(Path.expand(root) == Path.expand(List.to_string(:code.root_dir())),
        do: :ok,
        else: {:error, {:unsupported_vm, :runtime_root_mismatch}}
      )

  defp source_files(root) do
    Enum.reduce_while(@source_blobs, :ok, fn {name, expected}, :ok ->
      case bounded_file(Path.join(root, name), 8_388_608) do
        {:ok, bytes} ->
          hash =
            :crypto.hash(:sha, ["blob ", Integer.to_string(byte_size(bytes)), <<0>>, bytes])
            |> Base.encode16(case: :lower)

          if hash == expected,
            do: {:cont, :ok},
            else: {:halt, {:error, {:unsupported_vm, {:source_mismatch, name}}}}

        _ ->
          {:halt, {:error, {:unsupported_vm, {:source_missing, name}}}}
      end
    end)
  end

  defp manifest(config) do
    with {:ok, raw} <- bounded_file(config.nif_path <> ".manifest.term", 1_048_576),
         {:ok, pairs, digest, native} <- manifest_fields(raw),
         :ok <- verify_headers(pairs, config),
         ^digest <- sha(:erlang.term_to_binary(pairs, [:deterministic])),
         {:ok, bytes} <- bounded_file(config.nif_path <> ".so", 33_554_432),
         ^native <- sha(bytes) do
      {:ok, digest}
    else
      _ -> {:error, {:unsupported_vm, :compiled_manifest_mismatch}}
    end
  end

  # Parse only the generated canonical grammar. No generic ETF decoder,
  # compression expansion, input atom creation or declared-size allocation.
  defp manifest_fields(
         <<131, 116, 4::32, 119, 13, "header_digest", 109, 32::32, digest::binary-size(32), 119,
           7, "headers", 108, count::32, body::binary>>
       )
       when count in 1..4_096 do
    with {:ok, pairs, tail} <- manifest_headers(body, count, nil, []),
         <<106, 119, 13, "native_sha256", 109, 32::32, native::binary-size(32), 119, 15,
           "source_revision", 109, 40::32, revision::binary-size(40)>> <- tail,
         true <- revision == @revision do
      {:ok, pairs, digest, native}
    else
      _ -> :error
    end
  end

  defp manifest_fields(_), do: :error

  defp manifest_headers(tail, 0, _previous, rows), do: {:ok, Enum.reverse(rows), tail}

  defp manifest_headers(<<104, 2, 109, length::32, rest::binary>>, count, previous, rows)
       when length in 1..4_096 do
    case rest do
      <<path::binary-size(length), 109, 32::32, digest::binary-size(32), tail::binary>> ->
        if String.valid?(path) and (previous == nil or previous < path) do
          manifest_headers(tail, count - 1, path, [{path, digest} | rows])
        else
          :error
        end

      _ ->
        :error
    end
  end

  defp manifest_headers(_, _, _, _), do: :error

  defp verify_headers(pairs, config) do
    Enum.reduce_while(pairs, {:ok, 0}, fn
      {name, digest}, {:ok, total}
      when is_binary(name) and is_binary(digest) and byte_size(digest) == 32 ->
        with {:ok, path} <- header_path(name, config),
             {:ok, bytes} <- bounded_file(path, 8_388_608),
             true <- total + byte_size(bytes) <= 67_108_864 and sha(bytes) == digest do
          {:cont, {:ok, total + byte_size(bytes)}}
        else
          _ -> {:halt, :error}
        end

      _, _ ->
        {:halt, :error}
    end)
    |> case do
      {:ok, _} -> :ok
      _ -> :error
    end
  end

  defp header_path(name, config) do
    case String.split(name, "/") do
      [kind | parts] when kind in ["source", "otp"] and parts != [] ->
        root = if kind == "source", do: config.source_root, else: config.otp_root

        Enum.reduce_while(parts, {:ok, root}, fn component, {:ok, parent} ->
          path = Path.join(parent, component)

          if component not in ["", ".", ".."] and not String.contains?(component, "\\") do
            case File.lstat(path) do
              {:ok, %{type: type}} when type in [:regular, :directory] -> {:cont, {:ok, path}}
              _ -> {:halt, :error}
            end
          else
            {:halt, :error}
          end
        end)

      _ ->
        :error
    end
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

  defp sha(bytes), do: :crypto.hash(:sha256, bytes)
end
