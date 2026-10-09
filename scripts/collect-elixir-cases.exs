# Run from the complete pinned Elixir checkout with MIX_ENV=test and its own
# compiled dependency/build directories. This registers tests; it never runs
# ExUnit cases or their setup callbacks. See docs/runtime-case-collection.md.
defmodule WorkflowCaseCollector do
  @source_commit "66586c596e271408d9184af32eb31cd7218c126e"
  @manifest_sha256 "7df8c7885a64b6276e84517dffa4f1ead0f592f6d0996b398cdb61551c1bae61"

  def run([rust_root, output_directory]) do
    source_root = File.cwd!()
    {head, 0} = System.cmd("git", ["rev-parse", "HEAD"])
    if String.trim(head) != @source_commit, do: raise("source checkout is not the pinned commit")
    {_diff, 0} = System.cmd("git", ["diff", "--exit-code", "HEAD", "--"])
    if Mix.env() != :test, do: raise("collector requires MIX_ENV=test with test support compiled")

    manifest_bytes =
      File.read!(Path.join(rust_root, "tests/reference/workflow-test-manifest.json"))

    if digest(manifest_bytes) != @manifest_sha256, do: raise("frozen manifest digest changed")
    manifest = Jason.decode!(manifest_bytes)

    Enum.each(manifest["files"], fn file ->
      if digest(File.read!(file["source_path"])) != file["sha256"],
        do: raise("pinned source bytes disagree with manifest: #{file["source_path"]}")
    end)

    :ok = ExUnit.start(autorun: false)
    initial_modules = test_modules()
    files = manifest["files"] |> Enum.map(& &1["source_path"])
    helpers = Enum.filter(files, &String.ends_with?(&1, "/test_helper.exs")) |> Enum.sort()
    {additional_roots, additional_files, root_review} = additional_sources(rust_root, output_directory)
    roots = (Enum.filter(files, &String.ends_with?(&1, "_test.exs")) ++ additional_roots)
      |> Enum.uniq() |> Enum.sort()

    {helper_results, profiles} =
      Enum.map_reduce(helpers, %{}, fn file, profiles ->
        result = load(file)
        # ExUnit.start/1 in the original helpers inherits autorun:false. Guard
        # against a helper changing it; no ExUnit.run call is made here.
        ExUnit.configure(autorun: false)
        profile = ExUnit.configuration() |> Keyword.take([:exclude, :include]) |> json_value()
        {result, Map.put(profiles, file, profile)}
      end)

    file_results = Enum.map(roots, &load/1)
    final_profile = ExUnit.configuration() |> Keyword.take([:exclude, :include]) |> json_value()
    modules = test_modules() -- initial_modules

    raw_cases =
      modules
      |> Enum.flat_map(fn module ->
        Enum.map(module.__ex_unit__().tests, fn test ->
          file = Path.relative_to(test.tags[:file], source_root)

          %{
            "source_path" => file,
            "source_line" => test.tags[:line],
            "module" => Atom.to_string(module),
            "name" => Atom.to_string(test.name),
            "tags" => json_value(test.tags)
          }
        end)
      end)
      |> Enum.sort_by(&{&1["source_path"], &1["source_line"], &1["module"], &1["name"]})

    declarations = Enum.group_by(manifest["declarations"], &{&1["source_path"], &1["line"]})

    {cases, unmatched} =
      Enum.split_with(raw_cases, fn test ->
        length(Map.get(declarations, {test["source_path"], test["source_line"]}, [])) == 1
      end)

    cases =
      Enum.map(cases, fn test ->
        [declaration] = Map.fetch!(declarations, {test["source_path"], test["source_line"]})
        id = declaration["id"]

        identity =
          Enum.join(
            [
              @source_commit,
              id,
              test["source_path"],
              Integer.to_string(test["source_line"]),
              test["module"],
              test["name"]
            ],
            <<0>>
          )

        test |> Map.put("case_id", digest(identity)) |> Map.put("source_declaration_id", id)
      end)

    registered = MapSet.new(cases, & &1["source_declaration_id"])

    unresolved =
      manifest["declarations"]
      |> Enum.map(& &1["id"])
      |> Enum.reject(&MapSet.member?(registered, &1))
      |> Enum.sort()

    blockers =
      (helper_results ++ file_results)
      |> Enum.filter(&(&1["status"] != "loaded"))
      |> Enum.map(&"#{&1["source_path"]}: #{&1["error"]}")

    blockers =
      blockers ++
        if(unmatched == [],
          do: [],
          else: [
            "#{length(unmatched)} runtime cases need non-exact source-location reconciliation; all retained in collection-details.json"
          ]
        ) ++
        if(unresolved == [],
          do: [],
          else: [
            "#{length(unresolved)} textual declaration IDs lack an exact registered source location; none treated as passed or excluded"
          ]
        )

    registration = %{
      "schema_version" => 1,
      "source_commit" => @source_commit,
      "reference_manifest_sha256" => @manifest_sha256,
      "collection_kind" => "exunit-registration-only",
      "collection_complete" => blockers == [],
      "blockers" => blockers,
      "unresolved_declaration_ids" => unresolved,
      "cases" => cases
    }

    details = %{
      "schema_version" => 2,
      "source_commit" => @source_commit,
      "mix_lock_sha256" => digest(File.read!("mix.lock")),
      "collector_sha256" => digest(File.read!(__ENV__.file)),
      "elixir_version" => System.version(),
      "otp_release" => List.to_string(:erlang.system_info(:otp_release)),
      "mix_environment" => "test",
      "test_execution" => false,
      "setup_execution" => false,
      "fixture_review" => "not-performed",
      "additional_root_review" => root_review,
      "additional_source_files" => additional_files,
      "helper_results" => helper_results,
      "helper_source_sha256" => Map.new(helpers, &{&1, digest(File.read!(&1))}),
      "helper_profiles" => profiles,
      "final_global_profile" => final_profile,
      "root_file_results" => file_results,
      "all_registered_cases" => raw_cases,
      "unmatched_runtime_cases" => unmatched,
      "registered_module_count" => length(modules),
      "registered_case_count" => length(raw_cases),
      "matched_case_count" => length(cases),
      "warning" =>
        "Registration compiles cases and loads original helpers but does not execute tests or setup callbacks. Tags do not certify profile execution."
    }

    File.mkdir_p!(output_directory)
    write_json(Path.join(output_directory, "runtime-registration.json"), registration)
    write_json(Path.join(output_directory, "collection-details.json"), details)

    IO.puts(
      "Registered #{length(raw_cases)} real ExUnit cases in #{length(modules)} modules; exact source links #{length(cases)}, unresolved textual declarations #{length(unresolved)}, unmatched runtime cases #{length(unmatched)}, file/helper blockers #{length(Enum.filter(helper_results ++ file_results, &(&1["status"] != "loaded")))}. No tests executed."
    )
  end

  def run(_),
    do: raise("usage: mix run --no-start collect-elixir-cases.exs RUST_ROOT OUTPUT_DIRECTORY")

  defp additional_sources(rust_root, output_directory) do
    review_path = "tests/parity/collection-root-review.json"
    path = Path.join(rust_root, review_path)
    if File.exists?(path) do
      bytes = File.read!(path)
      review = Jason.decode!(bytes)
      if review["schema_version"] != 1 or review["source_commit"] != @source_commit or
         review["registration_is_execution"] != false or review["scope_decision"] != "pending-case-review",
        do: raise("additional registration roots need explicit pinned/source/scope review")

      sources = Enum.map(review["files"], fn file ->
        source_path = file["source_path"]
        {pinned, 0} = System.cmd("git", ["show", "#{@source_commit}:#{source_path}"])
        if pinned != File.read!(source_path) or digest(pinned) != file["sha256"],
          do: raise("additional source disagrees with pinned git object: #{source_path}")
        target = Path.join([output_directory, "source-supplement", source_path])
        File.mkdir_p!(Path.dirname(target))
        File.write!(target, pinned)
        %{"source_path" => source_path, "sha256" => digest(pinned),
          "source_ref" => %{"path" => Path.relative_to(target, rust_root), "sha256" => digest(pinned)}}
      end)
      paths = MapSet.new(sources, & &1["source_path"])
      if Enum.any?(review["roots"], &(not MapSet.member?(paths, &1))),
        do: raise("additional root source bytes absent from explicit review")
      {review["roots"], sources, %{"path" => review_path, "sha256" => digest(bytes)}}
    else
      {[], [], nil}
    end
  end

  defp load(file) do
    Code.require_file(file)
    %{"source_path" => file, "status" => "loaded"}
  rescue
    error ->
      %{
        "source_path" => file,
        "status" => "blocked",
        "error" => Exception.format(:error, error, __STACKTRACE__)
      }
  catch
    kind, reason ->
      %{
        "source_path" => file,
        "status" => "blocked",
        "error" => Exception.format(kind, reason, __STACKTRACE__)
      }
  end

  defp test_modules do
    :code.all_loaded()
    |> Enum.map(&elem(&1, 0))
    |> Enum.filter(&function_exported?(&1, :__ex_unit__, 0))
    |> Enum.sort()
  end

  defp digest(bytes), do: :crypto.hash(:sha256, bytes) |> Base.encode16(case: :lower)
  defp write_json(path, value), do: File.write!(path, Jason.encode!(value, pretty: true) <> "\n")

  defp json_value(value) when is_atom(value),
    do: if(value in [nil, true, false], do: value, else: Atom.to_string(value))

  defp json_value(value) when is_binary(value),
    do: if(String.valid?(value), do: value, else: %{"binary_base64" => Base.encode64(value)})

  defp json_value(value) when is_number(value), do: value
  defp json_value(value) when is_list(value), do: Enum.map(value, &json_value/1)
  defp json_value(value) when is_tuple(value), do: value |> Tuple.to_list() |> json_value()

  defp json_value(value) when is_map(value),
    do: Map.new(value, fn {key, item} -> {to_string(key), json_value(item)} end)

  defp json_value(value), do: %{"elixir_inspect" => inspect(value, limit: :infinity)}
end

WorkflowCaseCollector.run(System.argv())
