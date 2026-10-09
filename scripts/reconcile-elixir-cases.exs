# Run with the existing pinned Jason ebin path; no Mix compile/application start.
Code.require_file("runtime-macro-origins.exs", __DIR__)

defmodule WorkflowRegistrationReconciler do
  @commit "66586c596e271408d9184af32eb31cd7218c126e"
  @manifest_sha "7df8c7885a64b6276e84517dffa4f1ead0f592f6d0996b398cdb61551c1bae61"
  @details "tests/parity/collection/supplemental/collection-details.json"
  @review "tests/parity/macro-origin-review.json"
  @origins "tests/parity/collection/macro-origins.json"

  def run([root, mode]) when mode in ["--propose", "--reconcile"] do
    manifest_bytes = File.read!(Path.join(root, "tests/reference/workflow-test-manifest.json"))
    if digest(manifest_bytes) != @manifest_sha, do: raise("frozen manifest digest changed")
    manifest = Jason.decode!(manifest_bytes)
    details_bytes = File.read!(Path.join(root, @details))
    details = Jason.decode!(details_bytes)
    if details["source_commit"] != @commit or details["test_execution"] != false,
      do: raise("collection must be pinned real registration-only data")

    indexes = Enum.map(manifest["files"], fn file ->
      path = file["source_path"]
      bytes = File.read!(Path.join(root, "tests/reference/elixir-baseline/" <> path))
      if digest(bytes) != file["sha256"], do: raise("changed source: #{path}")
      if String.ends_with?(path, [".ex", ".exs"]),
        do: WorkflowMacroOrigins.index(bytes, path), else: %{uses: [], sections: []}
    end) ++ Enum.map(details["additional_source_files"] || [], fn file ->
      bytes = File.read!(Path.join(root, file["source_ref"]["path"]))
      if digest(bytes) != file["sha256"] or digest(bytes) != file["source_ref"]["sha256"],
        do: raise("changed supplemental pinned source")
      WorkflowMacroOrigins.index(bytes, file["source_path"])
    end)

    candidates = Enum.flat_map(details["all_registered_cases"], fn test ->
      case WorkflowMacroOrigins.match(test, indexes, manifest["declarations"]) do
        {:ok, origin} -> [Map.put(origin, "runtime_case_id", runtime_id(test))]
        {:unresolved, _} -> []
      end
    end)

    if mode == "--propose" do
      proposals = Enum.group_by(candidates, &{&1["macro_module"], &1["source_path"]})
      |> Enum.map(fn {{module, path}, rows} ->
        %{"macro_module" => module, "source_path" => path,
          "source_sha256" => hd(rows)["source_sha256"], "recoverable_case_count" => length(rows),
          "consumer_uses" => rows |> Enum.map(&Map.take(&1,
            ["registration_path", "registration_line", "registration_sha256", "use_text", "consumer_module"]))
            |> Enum.uniq()}
      end) |> Enum.sort_by(& &1["source_path"])
      write_json(Path.join(root, "tests/parity/collection/macro-origin-candidates.json"), proposals)
      IO.puts("#{length(candidates)} literal candidates in #{length(proposals)} sections; no links approved/generated")
    else
      reconcile(root, manifest, details, details_bytes, candidates)
    end
  end

  def run(_), do: raise("usage: elixir -pa PINNED_JASON_EBIN reconcile-elixir-cases.exs RUST_ROOT <--propose|--reconcile>")

  defp reconcile(root, manifest, details, details_bytes, candidates) do
    review_bytes = File.read!(Path.join(root, @review))
    review = Jason.decode!(review_bytes)
    if review["source_commit"] != @commit or review["origin_rule"] != "literal-section-origin-v2" or
       review["registration_is_execution"] != false, do: raise("unreviewed source-origin rule")
    if Enum.any?(review["sections"], &(&1["environment_decision"] != "unproved")),
      do: raise("no supported macro environment proof is admitted")
    reviewed = Map.new(review["sections"], &{&1["macro_module"], &1})
    origins = Enum.flat_map(candidates, fn origin ->
      case Map.get(reviewed, origin["macro_module"]) do
        %{"source_path" => path, "source_sha256" => sha, "scope_decision" => scope,
          "environment_decision" => "unproved"}
        when scope in ["included-workflow", "pending-case-review"] ->
          if origin["source_path"] != path or origin["source_sha256"] != sha,
            do: raise("reviewed section source changed")
          []
        nil -> []
        _ -> raise("invalid section scope review")
      end
    end) |> Enum.sort_by(& &1["runtime_case_id"])

    by_runtime = Map.new(origins, &{&1["runtime_case_id"], &1})
    declarations = Enum.group_by(manifest["declarations"], &{&1["source_path"], &1["line"]})
    {linked, unmatched} = Enum.reduce(details["all_registered_cases"], {[], []}, fn test, {linked, unmatched} ->
      id = runtime_id(test)
      case Map.get(declarations, {test["source_path"], test["source_line"]}, []) do
        [source] -> {[case_row(test, source, id, nil, "pending-case-review") | linked], unmatched}
        _ ->
          case Map.get(by_runtime, id) do
            nil -> {linked, [id | unmatched]}
            origin ->
              source = %{"id" => origin["source_declaration_id"], "source_path" => origin["source_path"],
                "line" => origin["source_line"]}
              {[case_row(test, source, id, origin["origin_id"], origin["scope_decision"]) | linked], unmatched}
          end
      end
    end)
    linked = Enum.sort_by(linked, & &1["case_id"])
    registered = MapSet.new(linked, & &1["source_declaration_id"])
    unresolved = manifest["declarations"] |> Enum.map(& &1["id"])
      |> Enum.reject(&MapSet.member?(registered, &1)) |> Enum.sort()
    origin_artifact = %{
      "schema_version" => 1, "source_commit" => @commit, "reference_manifest_sha256" => @manifest_sha,
      "origin_rule" => "literal-section-origin-v2", "review" => reference(@review, review_bytes),
      "source_collection" => reference(@details, details_bytes),
      "tools" => Enum.map(["scripts/runtime-macro-origins.exs", "scripts/reconcile-elixir-cases.exs"],
        &reference(&1, File.read!(Path.join(root, &1)))),
      "required_helper_paths" => Enum.map(details["helper_results"], & &1["source_path"]),
      "helper_profiles" => details["helper_profiles"],
      "requirements" => %{"setup_execution" => "unexecuted", "fixture_review" => "pending",
        "mandatory_profile_execution" => "unexecuted"}, "origins" => origins}
    origin_bytes = encode(origin_artifact)
    File.write!(Path.join(root, @origins), origin_bytes)

    blockers = ["Registration only: this collection executes no source cases/setup or required profiles",
      "Macro environments unproved: no framework or file-loader scaffold exception admitted",
      "#{length(unresolved)} frozen declarations unresolved; #{length(unmatched)} runtime identities unmatched",
      "Mixed/unrelated scope and fixture/support review remain pending; no exclusions count as passes"]
    registration = %{
      "schema_version" => 2, "source_commit" => @commit, "reference_manifest_sha256" => @manifest_sha,
      "collection_kind" => "exunit-registration-only", "collection_complete" => false,
      "blockers" => blockers, "unresolved_declaration_ids" => unresolved, "cases" => linked,
      "collection_details" => reference(@details, details_bytes), "macro_origins" => reference(@origins, origin_bytes),
      "unmatched_runtime_case_ids" => Enum.sort(unmatched)}
    write_json(Path.join(root, "tests/parity/collection/reconciled-registration.json"), registration)
    IO.puts("Retained #{details["registered_case_count"]} real registered cases; #{length(linked)} linked cases, " <>
      "#{length(origins)} reviewed literal macro origins, #{length(unresolved)} unresolved declarations, " <>
      "#{length(unmatched)} unmatched runtime cases. No tests/setup executed or parity certified.")
  end

  defp case_row(test, source, runtime_id, origin_id, scope) do
    row = %{"source_declaration_id" => source["id"], "source_path" => source["source_path"],
      "source_line" => source["line"], "module" => test["module"], "name" => test["name"], "tags" => test["tags"],
      "runtime_case_id" => runtime_id, "origin_id" => origin_id, "scope_decision" => scope}
    identity = [@commit, source["id"], source["source_path"], to_string(source["line"]), test["module"], test["name"]]
    Map.put(row, "case_id", digest(Enum.join(identity, <<0>>)))
  end

  defp runtime_id(test), do: digest(Enum.join([@commit, test["source_path"], to_string(test["source_line"]), test["module"], test["name"]], <<0>>))
  defp reference(path, bytes), do: %{"path" => path, "sha256" => digest(bytes)}
  defp digest(bytes), do: :crypto.hash(:sha256, bytes) |> Base.encode16(case: :lower)
  defp encode(value), do: Jason.encode!(value, pretty: true) <> "\n"
  defp write_json(path, value), do: File.write!(path, encode(value))
end

WorkflowRegistrationReconciler.run(System.argv())
