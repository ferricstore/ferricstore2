Code.require_file("compiler-lineage-data.exs", __DIR__)

# Logical intake/work ownership, not native RSS or original execution evidence.
defmodule WorkflowCaseMetadata do
  @scalar 16 * 1024 * 1024
  @aggregate 64 * 1024 * 1024
  @chunk 64 * 1024
  def limits do
    %{runtime_cases: 100_000, modules: 10_000, entries: 20_000, depth: 96,
      intake_nodes: 200_000, scan_nodes: 4_000_000, scalar_bytes: @scalar,
      retained_bytes: @aggregate, pair_nodes: 8_000_000, pair_bytes: @aggregate,
      chunk_bytes: @chunk, input_occurrences: 20_000, input_bytes: 256 * 1024 * 1024}
  end
  def new_budget, do: {0, 0, 0}

  def admit(value, {count, visits, retained} = budget) do
    true = is_integer(count) and count in 0..100_000 and is_integer(visits) and visits in 0..4_000_000 and
      is_integer(retained) and retained in 0..@aggregate
    {nodes, _} = scan(value, 0, {0, 0}, {:normalize, min(200_000, 4_000_000 - visits), @scalar})
    normalized = normalize(value)
    bytes = :erlang.external_size(normalized)
    if retained + bytes > @aggregate, do: raise(ArgumentError, "metadata retained byte limit")
    {normalized, put_elem(put_elem(budget, 1, visits + nodes), 2, retained + bytes)}
  end

  def admit_case({test, module, source_root}, {count, visits, retained}) do
    true = is_integer(count) and count >= 0 and is_integer(visits) and visits in 0..4_000_000 and
      is_integer(retained) and retained in 0..@aggregate
    if count >= 100_000, do: raise(ArgumentError, "runtime case count exceeds limit")
    next = {count + 1, visits, retained}
    header = runtime_header(test, module, source_root)
    admit(Map.put(header, "tags", test.tags), next)
  end

  def runtime_header(test, module, source_root) do
    valid = is_map(test) and is_map(Map.get(test, :tags)) and is_atom(module) and not is_nil(module) and
      is_atom(Map.get(test, :name)) and is_binary(source_root) and byte_size(source_root) <= @scalar
    if not valid, do: raise(ArgumentError, "invalid runtime identity")
    file = Map.get(test.tags, :file)
    line = Map.get(test.tags, :line)
    if not (is_binary(file) and byte_size(file) <= @scalar and is_integer(line) and
        line > 0 and line <= 18_446_744_073_709_551_615), do: raise(ArgumentError, "invalid runtime identity")
    # Atoms have a VM-bounded representation; raw binaries are checked first.
    if byte_size(file) + byte_size(source_root) + 2200 > @scalar, do: raise(ArgumentError, "metadata byte limit")
    name = Atom.to_string(test.name)
    module_name = Atom.to_string(module)
    if not (String.valid?(file) and String.valid?(source_root) and String.starts_with?(module_name, "Elixir.") and
        String.starts_with?(name, "test ") and byte_size(name) > 5), do: raise(ArgumentError, "invalid runtime identity")
    path = Path.relative_to(Path.expand(file, source_root), Path.expand(source_root))
    WorkflowCompilerLineageData.portable_path!(path)
    %{"source_path" => path, "source_line" => line, "module" => module_name, "name" => name}
  end

  def failure(file, kind, reason, trace, budget) do
    {data, budget} = admit(%{"source_path" => file, "kind" => kind, "reason" => reason, "stack" => trace}, budget)
    {_, size} = measure_json(data, 0, {0, 0})
    if size > @scalar, do: raise(ArgumentError, "metadata byte limit")
    # Project the final escaped string enclosure without retaining its binary.
    {escaped, _, _, [], _} = emit_json(data, :escaped_size, {0, :crypto.hash_init(:sha256), @scalar - 2, [], 0}, 0)
    {_, _, _, chunks, _} = emit_json(data, :memory, {0, :crypto.hash_init(:sha256), size, [], 0}, 0)
    diagnostic = chunks |> Enum.reverse() |> IO.iodata_to_binary()
    true = escape_size(diagnostic) == escaped and escaped + 2 <= @scalar
    admit(diagnostic, budget)
  end

  def retain(value, role, {count, visits, retained} = budget)
      when role in [:row, :index, :container, :root, :spine_fragment] do
    true = is_integer(count) and count in 0..100_000 and is_integer(visits) and visits in 0..4_000_000 and
      is_integer(retained) and retained in 0..@aggregate
    per_value = if role == :row, do: 200_000, else: 4_000_000
    bytes = if role == :row, do: @scalar, else: @aggregate
    style = if role == :row or role == :root, do: :json, else: :retained
    {nodes, _} = scan(value, 0, {0, 0}, {style, min(per_value, 4_000_000 - visits), bytes})
    size = :erlang.external_size(value)
    if retained + size > @aggregate, do: raise(ArgumentError, "metadata retained byte limit")
    {value, put_elem(put_elem(budget, 1, visits + nodes), 2, retained + size)}
  end

  def cons(head, owned_tail, budget) do
    true = is_list(owned_tail)
    {_, next} = retain([head], :spine_fragment, budget)
    {[head | owned_tail], next}
  end

  def measure_pair(registration, details) do
    true = is_map(registration) and is_map(details) and registration["schema_version"] == 1 and
      registration["collection_kind"] == "exunit-registration-only" and details["schema_version"] == 2 and
      details["test_execution"] == false and details["setup_execution"] == false
    raw_count = bounded_count(details["all_registered_cases"], 100_000)
    matched = bounded_count(registration["cases"], 100_000)
    unmatched = bounded_count(details["unmatched_runtime_cases"], 100_000)
    true = raw_count == details["registered_case_count"] and matched == details["matched_case_count"] and
      matched + unmatched == raw_count
    {nodes, first} = measure_json(registration, 0, {0, 0})
    {nodes, total} = measure_json(details, 0, {nodes, first + 1})
    if total + 1 > @aggregate, do: raise(ArgumentError, "metadata byte limit")
    {first + 1, total - first, nodes}
  end

  def write_pair(root, directory, registration, details) do
    paths = prepare_output(root, directory)
    {first_size, second_size, nodes} = measure_pair(registration, details)
    owned = open_pair(paths, {nil, nil})
    finish_or_cleanup(owned, :run, fn {first, second} ->
      {path, handle} = first
      {written, hash, _, _, visited} = emit_json(registration, handle, {0, :crypto.hash_init(:sha256), first_size, [], 0}, 0)
        |> then(&emit_chunk("\n", handle, &1))
      true = written == first_size
      first_hash = :crypto.hash_final(hash) |> Base.encode16(case: :lower)
      {second_path, second_handle} = second
      {written, hash, _, _, visited} = emit_json(details, second_handle, {0, :crypto.hash_init(:sha256), second_size, [], visited}, 0)
        |> then(&emit_chunk("\n", second_handle, &1))
      true = written == second_size and visited == nodes
      second_hash = :crypto.hash_final(hash) |> Base.encode16(case: :lower)
      true = is_binary(path) and is_binary(second_path)
      {{elem(elem(paths, 0), 0), first_size, first_hash}, {elem(elem(paths, 1), 0), second_size, second_hash}}
    end)
  end

  defp scan(value, depth, {nodes, bytes}, {style, node_limit, byte_limit} = mode) do
    if depth > 96, do: raise(ArgumentError, "metadata depth limit")
    nodes = nodes + 1
    if nodes > node_limit, do: raise(ArgumentError, "metadata node limit")
    {nodes, bytes} = case value do
      value when value in [nil, true, false] -> {nodes, bytes + byte_size(scalar_json(value))}
      value when is_integer(value) or is_float(value) -> {nodes, bytes + byte_size(scalar_json(value))}
      value when is_atom(value) and style != :json ->
        text = Atom.to_string(value)
        {nodes, bytes + escape_size(text) + 2}
      value when is_binary(value) ->
        if byte_size(value) > @scalar, do: raise(ArgumentError, "metadata scalar limit")
        if bytes + byte_size(value) > byte_limit, do: raise(ArgumentError, "metadata byte limit")
        if String.valid?(value) do
          {nodes, bytes + escape_size(value) + 2}
        else
          if style == :json, do: raise(ArgumentError, "unsupported metadata value")
          size = 4 * div(byte_size(value) + 2, 3) + 20
          if depth + 1 > 96, do: raise(ArgumentError, "metadata depth limit")
          {nodes + 3, bytes + size}
        end
      value when is_list(value) ->
        count = bounded_count(value, node_limit - nodes)
        if nodes + count > node_limit, do: raise(ArgumentError, "metadata node limit")
        Enum.reduce(value, {nodes + count, bytes + 2 + max(count - 1, 0)}, fn item, state ->
          scan(item, depth + 1, state, mode)
        end)
      value when is_tuple(value) and style != :json ->
        count = tuple_size(value)
        if count + nodes > node_limit, do: raise(ArgumentError, "metadata node limit")
        scan(Tuple.to_list(value), depth, {nodes, bytes}, mode)
      value when is_map(value) ->
        count = map_size(value)
        if nodes + 2 * count > node_limit, do: raise(ArgumentError, "metadata node limit")
        {nodes, bytes, _} = :maps.fold(fn key, item, {n, b, seen} ->
          {n, b, seen} = if style == :retained do
            {n, b} = scan(key, depth + 1, {n, b}, mode)
            {n, b, seen}
          else
            if is_binary(key) and b + byte_size(key) > byte_limit, do: raise(ArgumentError, "metadata byte limit")
            text = if style == :json do
              if not is_binary(key), do: raise(ArgumentError, "unsupported metadata key")
              json_key(key)
            else json_key(key) end
            if Map.has_key?(seen, text), do: raise(ArgumentError, "normalized metadata key collision")
            if n + 1 > node_limit, do: raise(ArgumentError, "metadata node limit")
            {n + 1, b + escape_size(text) + 2, Map.put(seen, text, true)}
          end
          if b + 1 > byte_limit, do: raise(ArgumentError, "metadata byte limit")
          {n, b} = scan(item, depth + 1, {n, b + 1}, mode)
          {n, b, seen}
        end, {nodes, bytes + 2 + max(count - 1, 0), %{}}, value)
        {nodes, bytes}
      _ -> raise(ArgumentError, "unsupported metadata value")
    end
    if nodes > node_limit, do: raise(ArgumentError, "metadata node limit")
    if bytes > byte_limit, do: raise(ArgumentError, "metadata byte limit")
    {nodes, bytes}
  end

  defp normalize(value) do
    case value do
      value when value in [nil, true, false] -> value
      value when is_atom(value) -> Atom.to_string(value)
      value when is_binary(value) -> if String.valid?(value), do: value, else: %{"binary_base64" => Base.encode64(value)}
      value when is_number(value) -> value
      value when is_list(value) -> Enum.map(value, &normalize/1)
      value when is_tuple(value) -> value |> Tuple.to_list() |> normalize()
      value when is_map(value) -> :maps.fold(fn key, item, acc -> Map.put(acc, json_key(key), normalize(item)) end, %{}, value)
    end
  end

  defp json_key(value) do
    case value do
      nil -> ""
      value when is_atom(value) -> Atom.to_string(value)
      value when is_binary(value) ->
        if byte_size(value) > @scalar, do: raise(ArgumentError, "metadata scalar limit")
        if not String.valid?(value), do: raise(ArgumentError, "unsupported metadata key")
        value
      value when is_integer(value) -> scalar_json(value)
      value when is_float(value) -> scalar_json(value); Float.to_string(value)
      _ -> raise(ArgumentError, "unsupported metadata key")
    end
  end

  defp bounded_count(value, limit) do
    walk = fn walk, value, count ->
      case value do
        [] -> count
        [_ | tail] when count < limit -> walk.(walk, tail, count + 1)
        [_ | _] -> raise(ArgumentError, "metadata node limit")
        _ -> raise(ArgumentError, "unsupported metadata value")
      end
    end
    walk.(walk, value, 0)
  end

  defp measure_json(value, depth, state), do: scan(value, depth, state, {:json, 8_000_000, @aggregate})

  defp escape_size(value) do
    walk = fn walk, value, size ->
      if size + 2 > @scalar, do: raise(ArgumentError, "metadata byte limit")
      case value do
        <<>> -> size
        <<byte, rest::binary>> ->
          extra = cond do
            byte in [34, 92, 8, 9, 10, 12, 13] -> 2
            byte < 32 -> 6
            true -> 1
          end
          walk.(walk, rest, size + extra)
      end
    end
    walk.(walk, value, 0)
  end

  defp scalar_json(value) do
    if is_integer(value) and (value < -9_223_372_036_854_775_808 or value > 18_446_744_073_709_551_615),
      do: raise(ArgumentError, "metadata integer limit")
    encoded = JSON.encode!(value)
    if byte_size(encoded) > 64, do: raise(ArgumentError, "metadata scalar limit")
    encoded
  end

  defp emit_json(value, sink, {count, hash, limit, chunks, nodes}, depth) do
    if depth > 96, do: raise(ArgumentError, "metadata depth limit")
    nodes = nodes + 1
    if nodes > 8_000_000, do: raise(ArgumentError, "metadata node limit")
    state = {count, hash, limit, chunks, nodes}
    cond do
      is_binary(value) ->
        state = emit_chunk("\"", sink, state)
        parts = Stream.unfold(value, fn
          <<>> -> nil
          bytes -> size = min(byte_size(bytes), @chunk); {binary_part(bytes, 0, size), binary_part(bytes, size, byte_size(bytes) - size)}
        end)
        state = Enum.reduce(parts, state, fn part, acc ->
          escaped = for <<byte <- part>> do
            case byte do
              34 -> "\\\""
              92 -> "\\\\"
              8 -> "\\b"
              9 -> "\\t"
              10 -> "\\n"
              12 -> "\\f"
              13 -> "\\r"
              value when value < 32 -> "\\u" <> (value |> Integer.to_string(16) |> String.downcase() |> String.pad_leading(4, "0"))
              value -> value
            end
          end
          emit_chunk(IO.iodata_to_binary(escaped), sink, acc)
        end)
        emit_chunk("\"", sink, state)
      is_map(value) ->
        keys = Map.keys(value) |> Enum.sort()
        state = emit_chunk("{", sink, state)
        {state, _} = Enum.reduce(keys, {state, false}, fn key, {acc, comma} ->
          acc = if comma, do: emit_chunk(",", sink, acc), else: acc
          acc = emit_json(key, sink, acc, depth + 1) |> then(&emit_chunk(":", sink, &1))
          {emit_json(Map.fetch!(value, key), sink, acc, depth + 1), true}
        end)
        emit_chunk("}", sink, state)
      is_list(value) ->
        extra = length(value)
        state = put_elem(state, 4, nodes + extra)
        if nodes + extra > 8_000_000, do: raise(ArgumentError, "metadata node limit")
        state = emit_chunk("[", sink, state)
        {state, _} = Enum.reduce(value, {state, false}, fn item, {acc, comma} ->
          acc = if comma, do: emit_chunk(",", sink, acc), else: acc
          {emit_json(item, sink, acc, depth + 1), true}
        end)
        emit_chunk("]", sink, state)
      true -> emit_chunk(scalar_json(value), sink, state)
    end
  end

  defp emit_chunk(bytes, sink, state) when byte_size(bytes) > @chunk do
    <<part::binary-size(@chunk), rest::binary>> = bytes
    emit_chunk(rest, sink, emit_chunk(part, sink, state))
  end
  defp emit_chunk(bytes, sink, {count, hash, limit, chunks, nodes}) do
    next = count + if(sink == :escaped_size, do: escape_size(bytes), else: byte_size(bytes))
    if next > limit, do: raise(ArgumentError, "metadata byte limit")
    if sink == :escaped_size do
      {next, hash, limit, chunks, nodes}
    else
      chunks = if sink == :memory, do: [bytes | chunks], else: (:ok = IO.binwrite(sink, bytes); chunks)
      {next, :crypto.hash_update(hash, bytes), limit, chunks, nodes}
    end
  end

  defp prepare_output(root, directory) do
    absolute = WorkflowCompilerLineageData.checked_path!(root, directory)
    true = File.lstat!(absolute).type == :directory
    paths = for name <- ["runtime-registration.json", "collection-details.json"], do: {directory <> "/" <> name, Path.join(absolute, name)}
    if Enum.any?(paths, fn {_, path} -> File.lstat(path) != {:error, :enoent} end),
      do: raise(ArgumentError, "metadata targets already exist")
    List.to_tuple(paths)
  end

  defp open_pair({{_, first}, {_, second}}, _) do
    {:ok, handle} = File.open(first, [:write, :binary, :exclusive])
    owned = {{first, handle}, {nil, nil}}
    try do
      {:ok, other} = File.open(second, [:write, :binary, :exclusive])
      {{first, handle}, {second, other}}
    catch
      kind, reason -> finish_or_cleanup(owned, {:failure, kind, reason, __STACKTRACE__}, nil)
    end
  end

  defp finish_or_cleanup({{first_path, first}, {second_path, second}} = owned, :run, operation) do
    result = try do
      result = operation.(owned)
      :ok = File.close(first)
      result
    catch
      kind, reason -> finish_or_cleanup(owned, {:failure, kind, reason, __STACKTRACE__}, nil)
    end
    # The second protected close is outside the first catch: cleanup is not caught twice.
    finish_or_cleanup({{first_path, nil}, {second_path, second}}, {:close_second, result}, nil)
  end
  defp finish_or_cleanup(owned, {:close_second, result}, _) do
    try do
      :ok = File.close(elem(elem(owned, 1), 1))
      result
    catch
      kind, reason -> finish_or_cleanup(owned, {:failure, kind, reason, __STACKTRACE__}, nil)
    end
  end
  defp finish_or_cleanup(owned, {:failure, kind, reason, stack}, _) do
    closed = Enum.map(Tuple.to_list(owned), fn {_, handle} ->
      if is_nil(handle), do: :ok, else: (try do File.close(handle) catch k, r -> {k, r} end)
    end)
    removed = Enum.map(Tuple.to_list(owned), fn {path, _} ->
      if is_nil(path), do: :ok, else: (try do File.rm(path) catch k, r -> {k, r} end)
    end)
    if Enum.all?(closed, &(&1 == :ok)) and
        Enum.all?(removed, &(&1 == :ok or &1 == {:error, :enoent})) do
      :erlang.raise(kind, reason, stack)
    else
      :erlang.raise(:error, {:metadata_pair_cleanup_failed, {kind, reason}, closed ++ removed}, stack)
    end
  end
end
