Code.require_file("compiler-lineage-data.exs", __DIR__)
Code.require_file("compiler-lineage-verifier.exs", __DIR__)
Code.require_file("case-metadata.exs", __DIR__)

# Compile/register in an isolated reference process; matching only reads data.
defmodule WorkflowCompilerLineage do
  alias WorkflowCompilerLineageData, as: Data
  @rule "compiler-constant-using-v1"

  def start(root) do
    if :ets.whereis(__MODULE__) != :undefined, do: :ets.delete(__MODULE__)
    :ets.new(__MODULE__, [:named_table, :public, :ordered_set])
    :ets.insert(__MODULE__, [{:root, Path.expand(root)}, {:content_bytes, 0},
      {:metadata_bytes, 0}, {:module_count, 0}, {:call_count, 0}, {:context_count, 0}, {:runtime_count, 0}])
    Code.ensure_loaded!(ExUnit.Case)
    {ExUnit.Case, bytes, _} = :code.get_object_code(ExUnit.Case)
    bytes = intern(bytes)
    {:ok, framework} = Data.beam(bytes)
    true = current_md5(ExUnit.Case) == framework.md5
    reserve(:metadata_bytes, :erlang.external_size(framework), Data.limits()["capture_metadata_bytes"])

    :ets.insert(__MODULE__, [
      {:framework, Map.merge(framework, %{bytes: bytes, sha256: Data.digest(bytes)})}
    ])

    :ok
  end

  def trace(event, env) do
    if :ets.whereis(__MODULE__) != :undefined, do: observe(event, env)
    :ok
  end

  def snapshot do
    if fetch(:capture_error), do: raise(fetch(:capture_error))
    rows = :ets.tab2list(__MODULE__)
    modules = for {{:module, _}, row} <- rows, do: row
    calls = for {{:call, _}, row} <- rows, do: row
    prefix_count = fn values, limit ->
      walk = fn walk, values, count ->
        case values do
          [] -> count
          [_ | tail] when count < limit -> walk.(walk, tail, count + 1)
          _ -> fail_capture("compiler native list/count limit")
        end
      end
      walk.(walk, values, 0)
    end
    prefix_count.(modules, Data.limits()["module_versions"])

    registered =
      modules
      |> Enum.group_by(& &1["module"])
      |> Enum.flat_map(fn {_name, versions} ->
        latest = Enum.max_by(versions, & &1["sequence"])
        module = latest["module_atom"]

        if function_exported?(module, :__ex_unit__, 0) and
             current_md5(module) == latest["loaded_md5"] do
          native_tests = module.__ex_unit__().tests
          prefix_count.(native_tests, Data.limits()["runtime_cases"] - fetch(:runtime_count))
          reserve(:metadata_bytes, :erlang.external_size([]), Data.limits()["capture_metadata_bytes"])
          registered = Enum.reduce(native_tests, [], fn test, owned_tail ->
            reserve(:runtime_count, 1, Data.limits()["runtime_cases"])
            header = WorkflowCaseMetadata.runtime_header(test, module, fetch(:root))
            path = header["source_path"]
            row = Map.put(header, "consumer_proof_id", if(latest["source_path"] == path, do: latest["proof_id"], else: nil))
            reserve(:metadata_bytes, :erlang.external_size(row), Data.limits()["capture_metadata_bytes"])
            reserve(:metadata_bytes, :erlang.external_size([row]), Data.limits()["capture_metadata_bytes"])
            [row | owned_tail]
          end)
          reserve(:metadata_bytes, :erlang.external_size(registered), Data.limits()["capture_metadata_bytes"])
          # Permutation charge is explicit; contents/headers remain unchanged.
          size = :erlang.external_size(registered)
          reserve(:metadata_bytes, size, Data.limits()["capture_metadata_bytes"])
          ordered = Enum.reverse(registered)
          true = :erlang.external_size(ordered) == size
          ordered
        else
          []
        end
      end)
    reserve(:metadata_bytes, :erlang.external_size(registered), Data.limits()["capture_metadata_bytes"])

    %{
      rule: @rule,
      root: fetch(:root),
      modules: modules,
      calls: calls,
      registered: registered,
      framework: fetch(:framework)
    }
  end

  def match(test, proof, declarations) do
    {[{^test, result}], _statistics} = match_all([test], proof, declarations)
    result
  end

  def match_all(tests, proof, declarations),
    do: WorkflowCompilerLineageVerifier.match_all(tests, proof, declarations)

  defp observe(:start, env) do
    :global.trans({{__MODULE__, :capture_budget}, self()}, fn ->
      if is_nil(fetch({:context, env.lexical_tracker})) do
        reserve(:context_count, 1, Data.limits()["module_versions"])
        reserve(:metadata_bytes, :erlang.external_size(env.file), Data.limits()["capture_metadata_bytes"])
      end
      :ets.insert(__MODULE__, {{:context, env.lexical_tracker}, env.file})
    end)
  end

  defp observe(:stop, env), do: :ets.delete(__MODULE__, {:context, env.lexical_tracker})

  defp observe({:on_module, bytes, _}, env) do
    path = relative(env.file)
    if not Data.bounded?(bytes), do: fail_capture("compiler module content exceeds limit")
    bytes = intern(bytes)
    {:ok, {module, md5}} = :beam_lib.md5(bytes)
    if module != env.module, do: fail_capture("compiler callback QName differs from BEAM")
    source = case if(portable?(path), do: Data.read_source(env.file), else: :unavailable) do
      {:ok, value} -> intern(value)
      {:error, :content_limit} -> fail_capture("compiler source content exceeds limit")
      _ -> nil
    end
    compiled = case Data.beam(bytes) do
      {:ok, value} when value.file == env.file -> value
      _ -> nil
    end
    definition = Module.get_definition(env.module, {:__using__, 1})
    sequence = sequence()
    path = if portable?(path), do: path, else: nil
    source_hash = if source, do: Data.digest(source), else: nil
    row = %{
      "proof_id" => Data.term_digest({Atom.to_string(module), path, source_hash, Data.digest(bytes), sequence}),
      "sequence" => sequence, "module" => Atom.to_string(module), "module_atom" => module,
      "module_definition_line" => compiled && compiled.module_line,
      "source_path" => path, "source_sha256" => source_hash, "source_bytes" => source,
      "compiled_module" => bytes, "compiled_module_sha256" => Data.digest(bytes),
      "loaded_md5" => md5, "definition" => definition,
      "definition_matches_debug" => not is_nil(compiled) and definition == compiled.definition and
        fetch({:context, env.lexical_tracker}) == env.file
    }
    retain(:module, row, [{{:module, sequence}, row}, {{:latest, module}, sequence}])
  end

  defp observe({kind, meta, target, name, arity}, env)
       when kind in [:remote_macro, :imported_macro, :remote_function, :imported_function] and
              name in [:__using__, :test, :describe, :setup, :setup_all] do
    if is_nil(env.function) and portable?(relative(env.file)) do
      framework = fetch(:framework)
      module = case fetch({:latest, target}) do nil -> nil; sequence -> fetch({:module, sequence}) end

      row = %{
        "sequence" => sequence(),
        "kind" => Atom.to_string(kind),
        "source_path" => relative(env.file),
        "source_line" => meta[:line],
        "column" => meta[:column],
        "consumer_module" => Atom.to_string(env.module),
        "target_module" => Atom.to_string(target),
        "name" => Atom.to_string(name),
        "arity" => arity,
        "target_proof_id" => module && module["proof_id"],
        "current_version" => not is_nil(module) and current_md5(target) == module["loaded_md5"],
        "target_loaded_md5" => current_md5(target),
        "genuine_case" =>
          kind in [:remote_macro, :imported_macro] and target == ExUnit.Case and
            current_md5(target) == framework.md5,
        "framework_sha256" => if(target == ExUnit.Case, do: framework.sha256, else: nil)
      }

      retain(:call, row, [{{:call, row["sequence"]}, row}])
    end
  end

  defp observe(_, _), do: :ok

  defp intern(bytes) do
    hash = Data.digest(bytes)
    :global.trans({{__MODULE__, :capture_budget}, self()}, fn ->
      case fetch({:content, hash}) do
        nil ->
          reserve(:content_bytes, byte_size(bytes), Data.limits()["aggregate_content_bytes"])
          :ets.insert(__MODULE__, {{:content, hash}, bytes})
          bytes
        cached when cached == bytes -> cached
        _ -> fail_capture("duplicate content digest disagreement")
      end
    end)
  end

  defp retain(kind, row, entries) do
    count = if kind == :module, do: :module_count, else: :call_count
    limit = if kind == :module, do: "module_versions", else: "calls"
    metadata = Data.metadata_size(row)
    :global.trans({{__MODULE__, :capture_budget}, self()}, fn ->
      reserve(:metadata_bytes, metadata, Data.limits()["capture_metadata_bytes"])
      reserve(count, 1, Data.limits()[limit])
      :ets.insert(__MODULE__, entries)
    end)
  end

  defp reserve(key, bytes, limit) do
    total = fetch(key) + bytes
    if total > limit, do: fail_capture("compiler capture #{key} exceeds #{limit}")
    :ets.insert(__MODULE__, {key, total})
  end

  defp fail_capture(message) do
    :ets.insert(__MODULE__, {:capture_error, message})
    raise message
  end

  defp relative(path), do: Path.relative_to(Path.expand(path), fetch(:root))

  defp portable?(path),
    do:
      is_binary(path) and Path.type(path) == :relative and
        Enum.all?(Path.split(path), &(&1 not in ["", ".", ".."]))

  defp sequence, do: System.unique_integer([:positive, :monotonic])

  defp fetch(key) do
    case :ets.lookup(__MODULE__, key) do
      [{^key, value}] -> value
      [] -> nil
    end
  end

  defp current_md5(module) do
    module.module_info(:md5)
  rescue
    _ -> nil
  end
end
