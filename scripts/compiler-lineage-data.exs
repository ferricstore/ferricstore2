# Pure data inspection. Never evaluate a decoded definition or expand a macro.
defmodule WorkflowCompilerLineageData do
  @max_bytes 16 * 1024 * 1024
  @max_nodes 200_000
  @max_depth 96

  def limits do
    %{"content_bytes" => @max_bytes, "aggregate_content_bytes" => 256 * 1024 * 1024,
      "capture_metadata_bytes" => 64 * 1024 * 1024, "metadata_bytes" => 64 * 1024 * 1024,
      "module_versions" => 10_000, "calls" => 1_000_000, "runtime_cases" => 100_000,
      "stdout_bytes" => 1024 * 1024, "stderr_bytes" => 1024 * 1024,
      "verifier_deadline_ms" => 60_000}
  end

  def unique_bytes(values), do: values |> MapSet.new() |> Enum.reduce(0, &(byte_size(&1) + &2))

  def metadata_size(row),
    do: row |> Map.drop(["source_bytes", "compiled_module", "module_atom"]) |> :erlang.external_size()

  def valid_budget?(modules, calls) do
    bytes = Enum.flat_map(modules, fn row ->
      if is_nil(row["source_bytes"]), do: [row["compiled_module"]], else: [row["source_bytes"], row["compiled_module"]]
    end)
    length(modules) <= limits()["module_versions"] and length(calls) <= limits()["calls"] and
      Enum.all?(bytes, &bounded?/1) and
      unique_bytes(bytes) <= limits()["aggregate_content_bytes"] and
      Enum.reduce(modules ++ calls, 0, &(metadata_size(&1) + &2)) <= limits()["capture_metadata_bytes"]
  rescue
    _ -> false
  end

  def analysis_cache(values, charged, analyse) do
    Enum.reduce(values, {%{}, charged}, fn {key, value}, {cache, total} ->
      parsed = analyse.(value)
      total = total + :erlang.external_size(parsed)
      if total > limits()["capture_metadata_bytes"], do: raise("aggregate parsed analysis limit")
      {Map.put(cache, key, parsed), total}
    end)
  end

  def digest(bytes), do: :crypto.hash(:sha256, bytes) |> Base.encode16(case: :lower)
  def term_digest(term), do: term |> :erlang.term_to_binary() |> digest()
  def bounded?(bytes), do: is_binary(bytes) and byte_size(bytes) <= @max_bytes

  def read_source(path) do
    with {:ok, file} <- File.open(path, [:read, :binary]) do
      try do
        case IO.binread(file, @max_bytes + 1) do
          bytes when is_binary(bytes) and byte_size(bytes) <= @max_bytes -> {:ok, bytes}
          bytes when is_binary(bytes) -> {:error, :content_limit}
          _ -> :unresolved
        end
      after
        File.close(file)
      end
    end
  end

  def read_artifact!(root, path, limit) do
    absolute = checked_path!(root, path)
    {:ok, %{type: :regular}} = File.lstat(absolute)
    {:ok, file} = File.open(absolute, [:read, :binary])
    try do
      bytes = IO.binread(file, limit + 1)
      true = is_binary(bytes) and byte_size(bytes) <= limit
      bytes
    after File.close(file) end
  end

  def checked_path!(root, path) do
    portable_path!(path)
    Enum.reduce(Path.split(path), Path.expand(root), fn part, parent ->
      value = Path.join(parent, part)
      {:ok, stat} = File.lstat(value)
      true = stat.type in [:regular, :directory]
      value
    end)
  end

  def portable_path!(path) do
    true = is_binary(path) and path != "" and Path.type(path) == :relative and
      not String.contains?(path, ["\\", ":"]) and not Regex.match?(~r/[\x00-\x1f\x7f]/, path) and
      Enum.all?(String.split(path, "/"), &(&1 not in ["", ".", ".."]))
  end

  def snapshot_paths!(root, directory) do
    Enum.flat_map(File.ls!(checked_path!(root, directory)), fn name ->
      path = directory <> "/" <> name
      case File.lstat!(checked_path!(root, path)).type do
        :regular -> [path]
        :directory -> snapshot_paths!(root, path)
      end
    end)
  end

  def beam(bytes) when is_binary(bytes) and byte_size(bytes) <= @max_bytes do
    with true <- bounded_debug?(bytes),
         {:ok, {module, [debug_info: {:debug_info_v1, :elixir_erl, {:elixir_v1, data, _}}]}} <-
           :beam_lib.chunks(bytes, [:debug_info]),
         {:ok, {^module, md5}} <- :beam_lib.md5(bytes) do
      definitions = Enum.filter(data.definitions, &match?({{:__using__, 1}, _, _, _}, &1))

      definition =
        case definitions do
          [{{:__using__, 1}, kind, meta, clauses}] -> {:v1, kind, meta, clauses}
          _ -> nil
        end

      {:ok,
       %{
         module: Atom.to_string(module),
         file: data.file,
         module_line: :erl_anno.line(Map.fetch!(data, :anno)),
         md5: md5,
         definition: definition
       }}
    else
      _ -> :unresolved
    end
  rescue
    _ -> :unresolved
  end

  def beam(_), do: :unresolved

  defp bounded_debug?(<<"FOR1", size::32, "BEAM", chunks::binary>> = bytes)
       when size + 8 == byte_size(bytes) do
    case debug_chunks(chunks, [], @max_nodes) do
      [debug] -> bounded_etf?(debug)
      _ -> false
    end
  end

  defp bounded_debug?(_), do: false
  defp debug_chunks(<<>>, found, _), do: found
  defp debug_chunks(_, _, left) when left <= 0, do: []

  defp debug_chunks(<<name::binary-size(4), size::32, rest::binary>>, found, left) do
    padded = size + rem(4 - rem(size, 4), 4)
    case rest do
      <<value::binary-size(size), _padding::binary-size(padded - size), tail::binary>> ->
        debug_chunks(tail, if(name == "Dbgi", do: [value | found], else: found), left - 1)
      _ -> []
    end
  end

  defp debug_chunks(_, _, _), do: []

  defp bounded_etf?(<<131, 80, advertised::32, compressed::binary>>)
       when advertised <= @max_bytes do
    stream = :zlib.open()
    try do
      :ok = :zlib.inflateInit(stream)
      size = inflated_size(stream, compressed, 0, @max_nodes)
      :ok = :zlib.inflateEnd(stream)
      size == advertised
    rescue
      _ -> false
    after
      :zlib.close(stream)
    end
  end

  defp bounded_etf?(<<131, 80, _::binary>>), do: false
  defp bounded_etf?(<<131, _::binary>> = bytes), do: byte_size(bytes) <= @max_bytes
  defp bounded_etf?(_), do: false

  defp inflated_size(_, _, _, left) when left <= 0, do: raise("debug stream work limit")
  defp inflated_size(stream, bytes, total, left) do
    {status, output} = :zlib.safeInflate(stream, bytes)
    total = total + :erlang.iolist_size(output)
    if total > @max_bytes, do: raise("expanded debug content limit")
    case status do
      :finished -> total
      :continue -> inflated_size(stream, <<>>, total, left - 1)
    end
  end

  def quote_body({:v1, :defmacro, meta, [{_, [{name, _, context}], [], expression}]})
      when is_atom(name) and is_atom(context) and name not in [:unquote, :unquote_splicing] do
    with {:ok, body, _} <- decode(expression, 0, @max_nodes) do
      {:ok, meta[:line], body}
    else
      _ -> :unresolved
    end
  end

  def quote_body(_), do: :unresolved

  def normalize(ast) do
    Macro.prewalk(ast, fn
      {name, meta, context} when is_atom(name) and is_list(meta) and is_atom(context) ->
        {name, [], nil}

      {name, meta, args} when is_list(meta) ->
        {name, [], args}

      node ->
        node
    end)
  end

  def source_quotes(bytes, path) when is_binary(bytes) and byte_size(bytes) <= @max_bytes do
    bytes |> Code.string_to_quoted!(file: path) |> block() |> module_quotes()
  rescue
    _ -> []
  end

  def source_quotes(_, _), do: []

  def source_analysis(bytes, path) when is_binary(bytes) and byte_size(bytes) <= @max_bytes do
    nodes = bytes |> Code.string_to_quoted!(file: path) |> block()
    %{quotes: module_quotes(nodes), uses: source_use_index(nodes)}
  rescue
    _ -> %{quotes: [], uses: %{}}
  end

  def source_analysis(_, _), do: %{quotes: [], uses: %{}}

  defp source_use_index(nodes) do
    Enum.reduce(nodes, %{}, fn
      {:defmodule, meta, [_, [do: body]]}, index ->
        lines = source_module_uses([{:defmodule, meta, [nil, [do: body]]}], meta[:line])

        Map.merge(
          source_use_index(block(body)),
          Map.update(index, meta[:line], lines, &(&1 ++ lines)),
          fn _line, a, b -> a ++ b end
        )

      _, index ->
        index
    end)
  end

  def source_uses(bytes, path, module_line) do
    bytes |> Code.string_to_quoted!(file: path) |> block() |> source_module_uses(module_line)
  rescue
    _ -> []
  end

  defp source_module_uses(nodes, module_line) do
    Enum.flat_map(nodes, fn
      {:defmodule, meta, [_, [do: body]]} ->
        own =
          if meta[:line] == module_line do
            Enum.flat_map(block(body), fn
              {:use, use_meta, [{:__aliases__, _, parts} | args]} ->
                if Enum.all?(parts, &is_atom/1) and Enum.all?(args, &Macro.quoted_literal?/1),
                  do: [use_meta[:line]],
                  else: []

              _ ->
                []
            end)
          else
            []
          end

        own ++ source_module_uses(block(body), module_line)

      _ ->
        []
    end)
  end

  def literal_tests(body) do
    with {:ok, tests} <- tests(block(body), "") do
      names = Enum.map(tests, & &1.name)
      if names != [] and length(names) == MapSet.size(MapSet.new(names)), do: tests, else: []
    else
      _ -> []
    end
  end

  defp module_quotes(nodes) do
    Enum.flat_map(nodes, fn
      {:defmodule, _, [_, [do: body]]} ->
        Enum.flat_map(block(body), fn
          {:defmacro, meta,
           [{:__using__, _, [{name, _, context}]}, [do: {:quote, _, [[do: quoted]]}]]}
          when is_atom(name) and is_atom(context) and name not in [:unquote, :unquote_splicing] ->
            if has_unquote?(quoted), do: [], else: [%{line: meta[:line], body: quoted}]

          {:defmodule, _, _} = nested ->
            module_quotes([nested])

          _ ->
            []
        end)

      _ ->
        []
    end)
  end

  defp has_unquote?(body) do
    {_body, found} =
      Macro.prewalk(body, false, fn
        {kind, _, _} = node, _ when kind in [:unquote, :unquote_splicing] -> {node, true}
        node, found -> {node, found}
      end)

    found
  end

  defp tests(nodes, describe) do
    Enum.reduce_while(nodes, {:ok, []}, fn node, {:ok, result} ->
      case node do
        {:test, meta, [name | args]} when is_binary(name) ->
          last = List.last(args)

          if is_list(last) and Keyword.keyword?(last) and Keyword.has_key?(last, :do) do
            {:cont,
             {:ok,
              result ++ [%{name: "test " <> describe <> name, literal: name, line: meta[:line]}]}}
          else
            {:halt, :unresolved}
          end

        {:describe, _, [name, [do: body]]} when is_binary(name) and describe == "" ->
          case tests(block(body), name <> " ") do
            {:ok, nested} -> {:cont, {:ok, result ++ nested}}
            _ -> {:halt, :unresolved}
          end

        {kind, _, [_ | _]} when kind in [:def, :defp, :setup, :setup_all] ->
          {:cont, {:ok, result}}

        {:@, _, [{kind, _, [value]}]} when kind in [:tag, :describetag, :moduletag, :timeout] ->
          if Macro.quoted_literal?(value), do: {:cont, {:ok, result}}, else: {:halt, :unresolved}

        _ ->
          {:halt, :unresolved}
      end
    end)
  end

  defp decode(_, depth, left) when depth > @max_depth or left <= 0, do: :unresolved

  defp decode(value, _, left) when is_atom(value) or is_binary(value) or is_number(value),
    do: {:ok, value, left - 1}

  defp decode({:{}, _, values}, depth, left) when is_list(values) do
    with {:ok, decoded, remaining} <- decode_list(values, depth + 1, left - 1),
         do: {:ok, List.to_tuple(decoded), remaining}
  end

  defp decode({a, b}, depth, left) do
    with {:ok, a, left} <- decode(a, depth + 1, left - 1),
         {:ok, b, left} <- decode(b, depth + 1, left),
         do: {:ok, {a, b}, left}
  end

  defp decode(values, depth, left) when is_list(values),
    do: decode_list(values, depth + 1, left - 1)

  defp decode(_, _, _), do: :unresolved

  defp decode_list(values, depth, left) do
    Enum.reduce_while(values, {:ok, [], left}, fn item, {:ok, values, remaining} ->
      case decode(item, depth, remaining) do
        {:ok, item, remaining} -> {:cont, {:ok, [item | values], remaining}}
        _ -> {:halt, :unresolved}
      end
    end)
    |> case do
      {:ok, values, remaining} -> {:ok, Enum.reverse(values), remaining}
      other -> other
    end
  end

  defp block({:__block__, _, nodes}), do: nodes
  defp block(node), do: [node]
end
