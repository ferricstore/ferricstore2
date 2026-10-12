# Register in the isolated pinned checkout; never dispatch tests/setup.
# Native compiler/helper/git work retains its separate reviewed process scope.
Code.require_file("case-metadata.exs", __DIR__)

defmodule WorkflowCaseCollector do
  @source_commit "66586c596e271408d9184af32eb31cd7218c126e"
  @manifest_sha256 "7df8c7885a64b6276e84517dffa4f1ead0f592f6d0996b398cdb61551c1bae61"

  def run([rust_root, output_directory]) do
    source_root = File.cwd!()
    {_, budget} = json_value([rust_root, output_directory, source_root], WorkflowCaseMetadata.new_budget())
    rust_root = Path.expand(rust_root)
    output_directory = Path.relative_to(Path.expand(output_directory), rust_root)
    {output_directory, budget} = json_value(output_directory, budget)
    WorkflowCompilerLineageData.portable_path!(output_directory)
    input = {0, 0}
    {head, 0} = System.cmd("git", ["rev-parse", "HEAD"])
    if String.trim(head) != @source_commit, do: raise("source checkout is not the pinned commit")
    {_diff, 0} = System.cmd("git", ["diff", "--exit-code", "HEAD", "--"])
    if Mix.env() != :test, do: raise("collector requires MIX_ENV=test with test support compiled")

    # Ordinary local values, not new module APIs. No growing prefix is scanned.
    count = fn values, limit ->
      walk = fn walk, values, size ->
        case values do
          [] -> size
          [_ | tail] when size < limit -> walk.(walk, tail, size + 1)
          _ -> raise(ArgumentError, "collector list/count limit")
        end
      end
      walk.(walk, values, 0)
    end
    keep = &WorkflowCaseMetadata.retain/3
    cons = &WorkflowCaseMetadata.cons/3
    complete = fn reversed, state, limit ->
      count.(reversed, limit)
      {reversed, state} = keep.(reversed, :container, state)
      before = elem(state, 2)
      {_, state} = keep.(reversed, :container, state)
      ordered = Enum.reverse(reversed)
      true = :erlang.external_size(ordered) == elem(state, 2) - before
      {ordered, state}
    end
    sort = fn values, key, state, limit ->
      count.(values, limit)
      before = elem(state, 2)
      {_, state} = keep.(values, :container, state)
      ordered = Enum.sort_by(values, key)
      true = :erlang.external_size(ordered) == elem(state, 2) - before
      {ordered, state}
    end
    gather = fn values, predicate, project, role, state, limit ->
      count.(values, limit)
      {empty, state} = keep.([], :container, state)
      {rows, state} = Enum.reduce(values, {empty, state}, fn value, {tail, state} ->
        if predicate.(value) do
          {row, state} = keep.(project.(value), role, state)
          cons.(row, tail, state)
        else
          {tail, state}
        end
      end)
      complete.(rows, state, limit)
    end

    {manifest_bytes, input} = read_input(rust_root, "tests/reference/workflow-test-manifest.json", :manifest, input)
    if digest(manifest_bytes) != @manifest_sha256, do: raise("frozen manifest digest changed")
    manifest = Jason.decode!(manifest_bytes)
    count.(manifest["files"], 20_000)
    count.(manifest["declarations"], 20_000)
    {manifest, budget} = keep.(manifest, :index, budget)
    input = Enum.reduce(manifest["files"], input, fn file, input ->
      {bytes, input} = read_input(source_root, file["source_path"], :source, input)
      if digest(bytes) != file["sha256"], do: raise("pinned source bytes disagree with manifest")
      input
    end)

    parent = WorkflowCompilerLineageData.checked_path!(rust_root, Path.dirname(output_directory))
    output = Path.join(parent, Path.basename(output_directory))
    case File.lstat(output) do
      {:error, :enoent} -> :ok = File.mkdir(output)
      {:ok, %{type: :directory}} -> :ok
      _ -> raise(ArgumentError, "collector output is not an owned directory")
    end
    :ok = ExUnit.start(autorun: false)
    {initial_modules, budget} = gather.(test_modules(), fn _ -> true end, & &1, :index, budget, 10_000)
    {initial_modules, budget} = sort.(initial_modules, & &1, budget, 10_000)
    {initial, budget} = keep.(MapSet.new(initial_modules), :index, budget)
    {files, budget} = gather.(manifest["files"], fn _ -> true end, & &1["source_path"], :row, budget, 20_000)
    {helpers, budget} = gather.(files, &String.ends_with?(&1, "/test_helper.exs"), & &1, :row, budget, 20_000)
    {helpers, budget} = sort.(helpers, & &1, budget, 20_000)
    {roots, budget} = gather.(files, &String.ends_with?(&1, "_test.exs"), & &1, :row, budget, 20_000)
    {additional_roots, additional_files, root_review, {input, budget}} =
      additional_sources(rust_root, output_directory, {input, budget})
    if count.(roots, 20_000) + count.(additional_roots, 20_000) > 20_000,
      do: raise(ArgumentError, "collector combined root limit")
    {roots, budget} = Enum.reduce(additional_roots, {roots, budget}, fn root, {tail, state} ->
      {root, state} = keep.(root, :row, state)
      cons.(root, tail, state)
    end)
    {roots, budget} = complete.(roots, budget, 20_000)
    {seen, budget} = keep.(MapSet.new(), :index, budget)
    {empty, budget} = keep.([], :container, budget)
    {roots, seen, budget} = Enum.reduce(roots, {empty, seen, budget}, fn root, {tail, seen, state} ->
      {_, state} = keep.(MapSet.new([root]), :index, state)
      if MapSet.member?(seen, root) do
        {tail, seen, state}
      else
        {tail, state} = cons.(root, tail, state)
        {tail, MapSet.put(seen, root), state}
      end
    end)
    {_, budget} = keep.(seen, :index, budget)
    {roots, budget} = complete.(roots, budget, 20_000)
    {roots, budget} = sort.(roots, & &1, budget, 20_000)
    if count.(helpers, 20_000) + count.(roots, 20_000) > 20_000,
      do: raise(ArgumentError, "collector combined source load limit")

    {profiles, budget} = keep.(%{}, :index, budget)
    {empty, budget} = keep.([], :container, budget)
    {helper_results, profiles, budget} = Enum.reduce(helpers, {empty, profiles, budget}, fn file, {tail, profiles, state} ->
      {result, state} = load(file, state)
      {tail, state} = cons.(result, tail, state)
      ExUnit.configure(autorun: false)
      {profile, state} = json_value(Keyword.take(ExUnit.configuration(), [:exclude, :include]), state)
      {_, state} = keep.(%{file => profile}, :index, state)
      {tail, Map.put(profiles, file, profile), state}
    end)
    {helper_results, budget} = complete.(helper_results, budget, 20_000)
    {profiles, budget} = keep.(profiles, :index, budget)
    {empty, budget} = keep.([], :container, budget)
    {file_results, budget} = Enum.reduce(roots, {empty, budget}, fn file, {tail, state} ->
      {result, state} = load(file, state)
      cons.(result, tail, state)
    end)
    {file_results, budget} = complete.(file_results, budget, 20_000)
    {final_profile, budget} = json_value(Keyword.take(ExUnit.configuration(), [:exclude, :include]), budget)
    {modules, budget} = gather.(test_modules(), &(not MapSet.member?(initial, &1)), & &1, :index, budget, 10_000)
    {modules, budget} = sort.(modules, & &1, budget, 10_000)

    {empty, budget} = keep.([], :container, budget)
    {raw_cases, budget} = Enum.reduce(modules, {empty, budget}, fn module, {tail, state} ->
      tests = module.__ex_unit__().tests
      count.(tests, 100_000 - elem(state, 0))
      Enum.reduce(tests, {tail, state}, fn test, {tail, state} ->
        {row, state} = WorkflowCaseMetadata.admit_case({test, module, source_root}, state)
        cons.(row, tail, state)
      end)
    end)
    {raw_cases, budget} = complete.(raw_cases, budget, 100_000)
    {raw_cases, budget} = sort.(raw_cases, &{&1["source_path"], &1["source_line"], &1["module"], &1["name"]}, budget, 100_000)

    {declarations, budget} = keep.(%{}, :index, budget)
    {declarations, budget} = Enum.reduce(manifest["declarations"], {declarations, budget}, fn declaration, {index, state} ->
      {declaration, state} = keep.(declaration, :row, state)
      {key, state} = keep.({declaration["source_path"], declaration["line"]}, :index, state)
      {_, state} = keep.(%{key => declaration}, :index, state)
      {bucket, state} = case Map.fetch(index, key) do
        {:ok, owned} -> {owned, state}
        :error -> keep.([], :container, state)
      end
      {bucket, state} = cons.(declaration, bucket, state)
      {Map.put(index, key, bucket), state}
    end)
    budget = Enum.reduce(declarations, budget, fn {_key, bucket}, state ->
      {_, state} = keep.(bucket, :container, state)
      state
    end)
    {declarations, budget} = keep.(declarations, :index, budget)
    {matched, budget} = keep.([], :container, budget)
    {unmatched, budget} = keep.([], :container, budget)
    digits = fn value ->
      walk = fn walk, value, width -> if value < 10, do: width, else: walk.(walk, div(value, 10), width + 1) end
      walk.(walk, value, 1)
    end
    {cases, unmatched, budget} = Enum.reduce(raw_cases, {matched, unmatched, budget}, fn test, {matched, unmatched, state} ->
      {key, state} = keep.({test["source_path"], test["source_line"]}, :index, state)
      case Map.get(declarations, key) do
        [declaration] ->
          id = declaration["id"]
          if not (is_binary(id) and byte_size(id) == 16), do: raise(ArgumentError, "invalid declaration identity")
          size = byte_size(@source_commit) + byte_size(id) + byte_size(test["source_path"]) +
            digits.(test["source_line"]) + byte_size(test["module"]) + byte_size(test["name"]) + 5
          if size > 16 * 1024 * 1024, do: raise(ArgumentError, "metadata byte limit")
          {line, state} = json_value(Integer.to_string(test["source_line"]), state)
          # The bounded escaped array upper-bounds the joined string enclosure.
          {parts, state} = json_value([@source_commit, <<0>>, id, <<0>>, test["source_path"], <<0>>, line, <<0>>, test["module"], <<0>>, test["name"]], state)
          {identity, state} = json_value(Enum.join(parts, ""), state)
          {case_id, state} = json_value(digest(identity), state)
          {id, state} = json_value(id, state)
          {row, state} = keep.(test |> Map.put("case_id", case_id) |> Map.put("source_declaration_id", id), :row, state)
          {matched, state} = cons.(row, matched, state)
          {matched, unmatched, state}
        _ ->
          {unmatched, state} = cons.(test, unmatched, state)
          {matched, unmatched, state}
      end
    end)
    {cases, budget} = complete.(cases, budget, 100_000)
    {unmatched, budget} = complete.(unmatched, budget, 100_000)
    {registered, budget} = keep.(MapSet.new(), :index, budget)
    {registered, budget} = Enum.reduce(cases, {registered, budget}, fn row, {index, state} ->
      {id, state} = json_value(row["source_declaration_id"], state)
      {_, state} = keep.(MapSet.new([id]), :index, state)
      {MapSet.put(index, id), state}
    end)
    {registered, budget} = keep.(registered, :index, budget)
    {empty, budget} = keep.([], :container, budget)
    {unresolved, budget} = Enum.reduce(manifest["declarations"], {empty, budget}, fn row, {tail, state} ->
      {id, state} = json_value(row["id"], state)
      if MapSet.member?(registered, id), do: {tail, state}, else: cons.(id, tail, state)
    end)
    {unresolved, budget} = complete.(unresolved, budget, 20_000)
    {unresolved, budget} = sort.(unresolved, & &1, budget, 20_000)

    {results, budget} = keep.(helper_results ++ file_results, :container, budget)
    {empty, budget} = keep.([], :container, budget)
    {blockers, budget} = Enum.reduce(results, {empty, budget}, fn result, {tail, state} ->
      if result["status"] == "loaded" do
        {tail, state}
      else
        if byte_size(result["source_path"]) + byte_size(result["error"]) + 2 > 16 * 1024 * 1024,
          do: raise(ArgumentError, "metadata byte limit")
        {parts, state} = json_value([result["source_path"], ": ", result["error"]], state)
        {blocker, state} = json_value(Enum.join(parts, ""), state)
        cons.(blocker, tail, state)
      end
    end)
    {counts, budget} = json_value(%{"registered_module_count" => count.(modules, 10_000),
      "registered_case_count" => count.(raw_cases, 100_000), "matched_case_count" => count.(cases, 100_000)}, budget)
    unmatched_count = count.(unmatched, 100_000)
    unresolved_count = count.(unresolved, 20_000)
    {blockers, budget} = if unmatched_count == 0 do
      {blockers, budget}
    else
      {warning, budget} = json_value(Integer.to_string(unmatched_count) <> " runtime cases need non-exact source-location reconciliation; all retained in collection-details.json", budget)
      cons.(warning, blockers, budget)
    end
    {blockers, budget} = if unresolved_count == 0 do
      {blockers, budget}
    else
      {warning, budget} = json_value(Integer.to_string(unresolved_count) <> " textual declaration IDs lack an exact registered source location; none treated as passed or excluded", budget)
      cons.(warning, blockers, budget)
    end
    {blockers, budget} = complete.(blockers, budget, 20_002)
    {helper_hashes, budget} = keep.(%{}, :index, budget)
    {helper_hashes, input, budget} = Enum.reduce(helpers, {helper_hashes, input, budget}, fn file, {index, input, state} ->
      {bytes, input} = read_input(source_root, file, :source, input)
      {hash, state} = json_value(digest(bytes), state)
      {_, state} = keep.(%{file => hash}, :index, state)
      {Map.put(index, file, hash), input, state}
    end)
    {helper_hashes, budget} = keep.(helper_hashes, :index, budget)
    {lock, input} = read_input(source_root, "mix.lock", :lock, input)
    collector_path = Path.relative_to(Path.expand(__ENV__.file), rust_root)
    {collector, _input} = read_input(rust_root, collector_path, :tool, input)
    {metadata_fields, budget} = json_value(%{"mix_lock_sha256" => digest(lock), "collector_sha256" => digest(collector),
      "elixir_version" => System.version(), "otp_release" => List.to_string(:erlang.system_info(:otp_release))}, budget)
    registration = %{"schema_version" => 1, "source_commit" => @source_commit,
      "reference_manifest_sha256" => @manifest_sha256, "collection_kind" => "exunit-registration-only",
      "collection_complete" => blockers == [], "blockers" => blockers,
      "unresolved_declaration_ids" => unresolved, "cases" => cases}
    details = Map.merge(metadata_fields, counts) |> Map.merge(%{"schema_version" => 2,
      "source_commit" => @source_commit, "mix_environment" => "test", "test_execution" => false,
      "setup_execution" => false, "fixture_review" => "not-performed", "additional_root_review" => root_review,
      "additional_source_files" => additional_files, "helper_results" => helper_results,
      "helper_source_sha256" => helper_hashes, "helper_profiles" => profiles,
      "final_global_profile" => final_profile, "root_file_results" => file_results,
      "all_registered_cases" => raw_cases, "unmatched_runtime_cases" => unmatched,
      "warning" => "Registration compiles cases and loads original helpers but does not execute tests or setup callbacks. Tags do not certify profile execution."})
    {registration, budget} = keep.(registration, :root, budget)
    {details, _budget} = keep.(details, :root, budget)
    write_json_pair({rust_root, output_directory}, registration, details)
    IO.puts("Registered #{counts["registered_case_count"]} real ExUnit cases in #{counts["registered_module_count"]} modules; exact source links #{counts["matched_case_count"]}, unresolved textual declarations #{unresolved_count}, unmatched runtime cases #{unmatched_count}. No tests executed.")
  end

  def run(_), do: raise("usage: mix run --no-start collect-elixir-cases.exs RUST_ROOT OUTPUT_DIRECTORY")

  defp additional_sources(rust_root, output_directory, {input, budget}) do
    review_path = "tests/parity/collection-root-review.json"
    path = Path.join(rust_root, review_path)
    keep = &WorkflowCaseMetadata.retain/3
    cons = &WorkflowCaseMetadata.cons/3
    {sources, budget} = keep.([], :container, budget)
    {roots, budget} = keep.([], :container, budget)
    if File.exists?(path) do
      {bytes, input} = read_input(rust_root, review_path, :review, input)
      review = Jason.decode!(bytes)
      count = fn values ->
        walk = fn walk, values, size ->
          case values do
            [] -> size
            [_ | tail] when size < 20_000 -> walk.(walk, tail, size + 1)
            _ -> raise(ArgumentError, "additional review list/count limit")
          end
        end
        walk.(walk, values, 0)
      end
      count.(review["files"])
      count.(review["roots"])
      {review, budget} = keep.(review, :index, budget)
      if review["schema_version"] != 1 or review["source_commit"] != @source_commit or
          review["registration_is_execution"] != false or review["scope_decision"] != "pending-case-review",
        do: raise("additional registration roots need explicit pinned/source/scope review")
      {sources, input, budget} = Enum.reduce(review["files"], {sources, input, budget}, fn file, {tail, input, state} ->
        {source_path, state} = json_value(file["source_path"], state)
        {current, input} = read_input(File.cwd!(), source_path, :source, input)
        {pinned, 0} = System.cmd("git", ["show", @source_commit <> ":" <> source_path])
        if pinned != current or digest(pinned) != file["sha256"], do: raise("additional source disagrees with pinned git object")
        {hash, state} = json_value(digest(pinned), state)
        {parts, state} = json_value([output_directory, "/source-supplement/", source_path], state)
        {relative, state} = json_value(Enum.join(parts, ""), state)
        WorkflowCompilerLineageData.portable_path!(relative)
        {reference, state} = keep.(%{"path" => relative, "sha256" => hash}, :row, state)
        {source, state} = keep.(%{"source_path" => source_path, "sha256" => hash, "source_ref" => reference}, :row, state)
        parent = Enum.reduce(Path.split(Path.dirname(relative)), rust_root, fn part, parent ->
          directory = Path.join(parent, part)
          case File.lstat(directory) do
            {:error, :enoent} -> :ok = File.mkdir(directory)
            {:ok, %{type: :directory}} -> :ok
            _ -> raise(ArgumentError, "supplement parent is not a regular directory")
          end
          directory
        end)
        File.write!(Path.join(parent, Path.basename(relative)), pinned, [:exclusive])
        {tail, state} = cons.(source, tail, state)
        {tail, input, state}
      end)
      {sources, budget} = keep.(sources, :container, budget)
      before = elem(budget, 2)
      {_, budget} = keep.(sources, :container, budget)
      sources = Enum.reverse(sources)
      true = :erlang.external_size(sources) == elem(budget, 2) - before
      {paths, budget} = keep.(MapSet.new(), :index, budget)
      {paths, budget} = Enum.reduce(sources, {paths, budget}, fn source, {index, state} ->
        {_, state} = keep.(MapSet.new([source["source_path"]]), :index, state)
        {MapSet.put(index, source["source_path"]), state}
      end)
      {paths, budget} = keep.(paths, :index, budget)
      {roots, budget} = Enum.reduce(review["roots"], {roots, budget}, fn root, {tail, state} ->
        {root, state} = json_value(root, state)
        if not MapSet.member?(paths, root), do: raise("additional root source bytes absent from explicit review")
        cons.(root, tail, state)
      end)
      {roots, budget} = keep.(roots, :container, budget)
      before = elem(budget, 2)
      {_, budget} = keep.(roots, :container, budget)
      roots = Enum.reverse(roots)
      true = :erlang.external_size(roots) == elem(budget, 2) - before
      {reference, budget} = json_value(%{"path" => review_path, "sha256" => digest(bytes)}, budget)
      {roots, sources, reference, {input, budget}}
    else
      {roots, budget} = keep.(roots, :container, budget)
      {sources, budget} = keep.(sources, :container, budget)
      {reference, budget} = json_value(nil, budget)
      {roots, sources, reference, {input, budget}}
    end
  end

  defp load(file, budget) do
    # Catch only original load failure; admission failures fail the collection.
    result = try do
      Code.require_file(file)
      :loaded
    catch
      kind, reason -> {:blocked, kind, reason, __STACKTRACE__}
    end
    case result do
      :loaded -> json_value(%{"source_path" => file, "status" => "loaded"}, budget)
      {:blocked, kind, reason, stack} ->
        {diagnostic, budget} = WorkflowCaseMetadata.failure(file, kind, reason, stack, budget)
        json_value(%{"source_path" => file, "status" => "blocked", "error" => diagnostic}, budget)
    end
  end

  defp test_modules do
    {selected, _} = Enum.reduce(:code.all_loaded(), {[], 0}, fn {module, _}, {tail, size} ->
      if function_exported?(module, :__ex_unit__, 0) do
        if size >= 10_000, do: raise(ArgumentError, "registered module count exceeds limit")
        {[module | tail], size + 1}
      else
        {tail, size}
      end
    end)
    selected
  end

  defp read_input(root, path, role, {occurrences, bytes}) when role in [:manifest, :review, :source, :tool, :lock] do
    true = is_integer(occurrences) and occurrences >= 0 and is_integer(bytes) and bytes in 0..268_435_456
    if occurrences >= 20_000, do: raise(ArgumentError, "collector input occurrence limit")
    cap = if role in [:manifest, :review], do: 8 * 1024 * 1024, else: 16 * 1024 * 1024
    value = WorkflowCompilerLineageData.read_artifact!(root, path, min(cap, 268_435_456 - bytes))
    {value, {occurrences + 1, bytes + byte_size(value)}}
  end

  defp digest(bytes), do: :crypto.hash(:sha256, bytes) |> Base.encode16(case: :lower)
  defp json_value(value, budget), do: WorkflowCaseMetadata.admit(value, budget)
  defp write_json_pair({root, directory}, registration, details), do: WorkflowCaseMetadata.write_pair(root, directory, registration, details)
end

WorkflowCaseCollector.run(System.argv())
