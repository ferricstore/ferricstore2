# Separate candidate data only. Historical captures and counts remain untouched.
Code.require_file("compiler-lineage.exs", __DIR__)

defmodule WorkflowCompilerLineageArtifact do
  alias WorkflowCompilerLineageData, as: Data
  @commit "66586c596e271408d9184af32eb31cd7218c126e"
  @manifest "7df8c7885a64b6276e84517dffa4f1ead0f592f6d0996b398cdb61551c1bae61"
  @normalization "elixir-ast-metadata-hygiene-etf-v1"
  @tools ~w(scripts/collect-elixir-cases.exs scripts/compiler-lineage.exs scripts/compiler-lineage-data.exs scripts/compiler-lineage-verifier.exs scripts/compiler-lineage-artifact.exs scripts/collect-compiler-lineage.exs scripts/verify-compiler-lineage.exs tests/parity/workflow-compiler-lineage.schema.json scripts/case-metadata.exs)
  @native [Code, Module, Macro, JSON, :elixir_compiler]
  @final "tests/parity/collection/compiler-v1"
  @keys ~w(schema_version origin_rule admission source_commit reference_manifest_sha256 source_checkout elixir_version elixir_build_info otp_release normalization collection_details exact_registration collector tools framework compiler_fingerprints elixir_launcher_path elixir_launcher_sha256 mix_lock_sha256 current_inputs limits modules calls registered candidate_origins unresolved_runtime_case_ids unresolved_declaration_ids requirements)
  def limits, do: Data.limits()
  def valid_limits?(value), do: value == limits()
  def unique_bytes(values), do: Data.unique_bytes(values)

  def requirements do
    %{"registration_is_execution" => false, "test_execution" => false,
      "setup_execution" => false, "fixture_review" => "pending",
      "mandatory_profile_execution" => "unexecuted"}
  end

  def check_requirements(metadata) do
    true = valid_limits?(metadata["limits"])
    true = metadata["requirements"] == requirements()
    true = is_list(metadata["registered"]) and metadata["registered"] != []
    true = length(metadata["registered"]) <= limits()["runtime_cases"]
    :ok
  rescue
    _ -> {:error, "invalid fixed limits, registration or non-execution requirements"}
  end

  # This finite writer also accepts synthetic inputs; full verify binds frozen inputs.
  def write(root, directory, proof, details, registration) do
    portable!(directory)
    true = proof.rule == "compiler-constant-using-v1"
    true = Data.valid_budget?(proof.modules, proof.calls)
    true = details["test_execution"] == false and details["setup_execution"] == false
    raw = details["all_registered_cases"]
    true = is_list(raw) and raw != [] and length(raw) <= limits()["runtime_cases"]
    manifest_bytes = read!(root, "tests/reference/workflow-test-manifest.json", limits()["metadata_bytes"])
    manifest = JSON.decode!(manifest_bytes)
    validate_exact!(details, registration, manifest, Data.digest(manifest_bytes))
    framework = native(ExUnit.Case)
    true = framework.bytes == proof.framework.bytes
    natives = Enum.map(@native, &native/1)
    content = Enum.flat_map(proof.modules, &[&1["source_bytes"], &1["compiled_module"]]) |> Enum.reject(&is_nil/1)
    true = unique_bytes(content ++ Enum.map([framework | natives], & &1.bytes)) <= limits()["aggregate_content_bytes"]
    tools = Enum.map(@tools, &file_ref(root, &1, limits()["content_bytes"]))
    registered = observations(raw, proof.registered)
    candidates = proposals(raw, proof, registration, manifest["declarations"])
    {unmatched, unresolved} = complements(raw, registration, candidates, manifest["declarations"])
    metadata = %{
      "schema_version" => 1, "origin_rule" => proof.rule, "admission" => "proposal-only",
      "source_commit" => @commit, "reference_manifest_sha256" => Data.digest(manifest_bytes),
      "source_checkout" => Path.expand(proof.root), "elixir_version" => System.version(),
      "elixir_build_info" => json_value(System.build_info()), "otp_release" => otp(),
      "normalization" => @normalization, "collector" => hd(tools), "tools" => tools,
      "framework" => framework_ref(root, directory, framework),
      "compiler_fingerprints" => Enum.map(natives, &framework_ref(root, directory, &1)),
      "elixir_launcher_path" => launcher(), "elixir_launcher_sha256" => launcher() |> File.read!() |> Data.digest(),
      "mix_lock_sha256" => details["mix_lock_sha256"],
      "current_inputs" => current_inputs(proof.root, manifest, details), "limits" => limits(),
      "modules" => Enum.map(proof.modules, &module_ref(root, directory, &1)),
      "calls" => Enum.map(proof.calls, &encode_call/1), "registered" => registered,
      "candidate_origins" => candidates, "unresolved_runtime_case_ids" => unmatched,
      "unresolved_declaration_ids" => unresolved, "requirements" => requirements()
    }
    details_bytes = json(details)
    registration_bytes = json(registration)
    metadata = metadata |> Map.put("collection_details", ref(directory <> "/collection-details.json", details_bytes))
      |> Map.put("exact_registration", ref(directory <> "/runtime-registration.json", registration_bytes))
    metadata_bytes = json(metadata)
    true = byte_size(details_bytes) + byte_size(registration_bytes) + byte_size(metadata_bytes) <= limits()["capture_metadata_bytes"]
    write!(root, metadata["collection_details"]["path"], details_bytes)
    write!(root, metadata["exact_registration"]["path"], registration_bytes)
    write!(root, directory <> "/compiler-lineage.json", metadata_bytes)
    {:ok, metadata}
  rescue
    error -> {:error, Exception.message(error)}
  end

  def rehydrate(root, path) do
    true = Path.basename(path) == "compiler-lineage.json"
    bytes = read_capture!(root, path, limits()["metadata_bytes"])
    metadata = JSON.decode!(bytes)
    true = MapSet.new(Map.keys(metadata)) == MapSet.new(@keys)
    :ok = check_requirements(metadata)
    true = metadata["schema_version"] == 1 and metadata["origin_rule"] == "compiler-constant-using-v1"
    true = metadata["admission"] == "proposal-only" and metadata["normalization"] == @normalization
    true = length(metadata["modules"]) <= limits()["module_versions"] and length(metadata["calls"]) <= limits()["calls"]
    sequences = Enum.map(metadata["modules"] ++ metadata["calls"], & &1["sequence"])
    true = Enum.all?(sequences, &(is_integer(&1) and &1 > 0)) and length(sequences) == MapSet.size(MapSet.new(sequences))
    true = Enum.map(metadata["tools"], & &1["path"]) == @tools
    true = metadata["collector"] == hd(metadata["tools"])
    directory = Path.dirname(path)
    true = metadata["collection_details"]["path"] == directory <> "/collection-details.json"
    true = metadata["exact_registration"]["path"] == directory <> "/runtime-registration.json"
    validate_content_paths!(metadata, directory)
    Enum.each(metadata["tools"], &read_ref!(root, &1, limits()["content_bytes"]))
    details_bytes = read_capture_ref!(root, metadata["collection_details"], limits()["metadata_bytes"])
    registration_bytes = read_capture_ref!(root, metadata["exact_registration"], limits()["metadata_bytes"])
    true = byte_size(bytes) + byte_size(details_bytes) + byte_size(registration_bytes) <= limits()["capture_metadata_bytes"]
    details = JSON.decode!(details_bytes)
    registration = JSON.decode!(registration_bytes)
    manifest_bytes = read!(root, "tests/reference/workflow-test-manifest.json", limits()["metadata_bytes"])
    true = Data.digest(manifest_bytes) == metadata["reference_manifest_sha256"]
    validate_exact!(details, registration, JSON.decode!(manifest_bytes), Data.digest(manifest_bytes))
    references = content_refs(metadata)
    content = Enum.reduce(references, %{}, fn evidence, cache ->
      value = read_capture_ref!(root, evidence, limits()["content_bytes"])
      case Map.get(cache, evidence["sha256"]) do
        nil ->
          true = Enum.reduce(cache, byte_size(value), fn {_, binary}, total -> total + byte_size(binary) end) <= limits()["aggregate_content_bytes"]
          Map.put(cache, evidence["sha256"], value)
        ^value -> cache
        _ -> raise "duplicate content digest disagreement"
      end
    end)
    expected = MapSet.new(Enum.map(references, & &1["path"]) ++ [path, metadata["collection_details"]["path"], metadata["exact_registration"]["path"]] ++
      Enum.map(details["additional_source_files"] || [], & &1["source_ref"]["path"]))
    actual_directory = physical(root, Path.dirname(path))
    actual_paths = Data.snapshot_paths!(root, actual_directory)
      |> Enum.map(&logical(root, &1))
    true = MapSet.new(actual_paths) == expected
    charged = Enum.reduce(metadata["modules"] ++ metadata["calls"], 0, &(Data.metadata_size(&1) + &2))
    {beam_analysis, _charged} = Map.new(metadata["modules"], fn row -> {row["beam"]["sha256"], Map.fetch!(content, row["beam"]["sha256"])} end)
      |> Data.analysis_cache(charged, &Data.beam/1)
    modules = Enum.map(metadata["modules"], fn row ->
      compiled = case Map.fetch!(beam_analysis, row["beam"]["sha256"]) do {:ok, compiled} -> compiled; _ -> nil end
      definition = if compiled, do: compiled.definition, else: nil
      {:ok, {actual_module, actual_md5}} = :beam_lib.md5(Map.fetch!(content, row["beam"]["sha256"]))
      true = Atom.to_string(actual_module) == row["module"] and hex(actual_md5) == row["loaded_md5"]
      true = row["proof_id"] == Data.term_digest({row["module"], row["source_path"], row["source_sha256"], row["beam"]["sha256"], row["sequence"]})
      definition_matches = not is_nil(compiled) and Data.term_digest(definition) == row["definition_sha256"] and row["definition_matches_debug"] == true
      row |> Map.drop(["source", "beam", "definition_sha256"])
      |> Map.put("source_bytes", if(row["source"], do: Map.fetch!(content, row["source"]["sha256"]), else: nil))
      |> Map.put("compiled_module", Map.fetch!(content, row["beam"]["sha256"]))
      |> Map.put("compiled_module_sha256", row["beam"]["sha256"])
      |> Map.put("loaded_md5", unhex(row["loaded_md5"]))
      |> Map.put("definition", definition) |> Map.put("definition_matches_debug", definition_matches)
    end)
    framework = metadata["framework"]
    proof = %{rule: metadata["origin_rule"], root: metadata["source_checkout"], modules: modules,
      calls: Enum.map(metadata["calls"], &decode_call/1), registered: Enum.map(metadata["registered"], &Map.drop(&1, ["runtime_case_id", "tags"])),
      framework: %{bytes: Map.fetch!(content, framework["beam"]["sha256"]), sha256: framework["beam"]["sha256"], md5: unhex(framework["loaded_md5"])}}
    true = observations(details["all_registered_cases"], proof.registered) == metadata["registered"]
    true = details["test_execution"] == false and details["setup_execution"] == false
    {:ok, proof, metadata}
  rescue
    error -> {:error, Exception.message(error)}
  end

  def verify(root, path) do
    {:ok, proof, metadata} = rehydrate(root, path)
    true = metadata["source_commit"] == @commit and metadata["reference_manifest_sha256"] == @manifest
    true = Data.digest(read!(root, "tests/reference/workflow-test-manifest.json", limits()["metadata_bytes"])) == @manifest
    true = metadata["elixir_version"] == System.version() and metadata["elixir_build_info"] == json_value(System.build_info()) and metadata["otp_release"] == otp()
    true = metadata["elixir_launcher_path"] == launcher() and metadata["elixir_launcher_sha256"] == Data.digest(File.read!(launcher()))
    {head, 0} = System.cmd("git", ["-C", proof.root, "rev-parse", "HEAD"])
    {status, 0} = System.cmd("git", ["-C", proof.root, "status", "--porcelain"])
    true = String.trim(head) == @commit and status == ""
    fingerprints = [metadata["framework"] | metadata["compiler_fingerprints"]]
    true = Enum.map(fingerprints, & &1["module"]) == Enum.map([ExUnit.Case | @native], &Atom.to_string/1)
    Enum.each(Enum.zip(fingerprints, [ExUnit.Case | @native]), fn {record, module} ->
      current = native(module)
      true = record["beam"]["sha256"] == Data.digest(current.bytes) and record["loaded_md5"] == hex(current.md5) and record["source_path"] == current.path
    end)
    inputs = Enum.map(metadata["current_inputs"], & &1["path"])
    true = length(inputs) == MapSet.size(MapSet.new(inputs)) and "mix.lock" in inputs
    Enum.each(metadata["current_inputs"], &read_ref!(proof.root, &1, limits()["content_bytes"]))
    true = metadata["mix_lock_sha256"] == Data.digest(read!(proof.root, "mix.lock", limits()["content_bytes"]))
    manifest = JSON.decode!(read!(root, "tests/reference/workflow-test-manifest.json", limits()["metadata_bytes"]))
    details = JSON.decode!(read_capture_ref!(root, metadata["collection_details"], limits()["metadata_bytes"]))
    registration = JSON.decode!(read_capture_ref!(root, metadata["exact_registration"], limits()["metadata_bytes"]))
    true = metadata["current_inputs"] == current_inputs(proof.root, manifest, details)
    validate_provenance!(root, metadata, details, manifest)
    candidates = proposals(details["all_registered_cases"], proof, registration, manifest["declarations"])
    true = candidates == metadata["candidate_origins"]
    {unmatched, unresolved} = complements(details["all_registered_cases"], registration, candidates, manifest["declarations"])
    true = unmatched == metadata["unresolved_runtime_case_ids"] and unresolved == metadata["unresolved_declaration_ids"]
    {_matches, statistics} = WorkflowCompilerLineage.match_all(proof.registered, proof, manifest["declarations"])
    true = statistics[:invalid_proof] != true
    {:ok, %{"schema_version" => 1, "capture_sha256" => Data.digest(read_capture!(root, path, limits()["metadata_bytes"])),
      "source_commit" => @commit, "elixir_version" => System.version(), "otp_release" => otp(), "normalization" => @normalization,
      "registered_cases" => length(metadata["registered"]), "candidate_origins" => length(candidates),
      "unresolved_runtime_cases" => length(unmatched), "unresolved_declarations" => length(unresolved), "statistics" => json_value(statistics)}}
  rescue
    error -> {:error, Exception.message(error)}
  end

  def publish(root, stage, @final = destination) do
    prefix = @final <> "-staging-#{System.pid()}-"
    true = String.starts_with?(stage, prefix) and Regex.match?(~r/^[1-9][0-9]*$/, String.replace_prefix(stage, prefix, ""))
    false = File.exists?(Path.join(root, destination))
    {:ok, _} = verify(root, stage <> "/compiler-lineage.json")
    metadata = JSON.decode!(read!(root, stage <> "/compiler-lineage.json", limits()["metadata_bytes"]))
    details = JSON.decode!(read_ref!(root, metadata["collection_details"], limits()["metadata_bytes"]))
    details = Map.update!(details, "additional_source_files", fn sources ->
      Enum.map(sources, &Map.update!(&1, "source_ref", fn reference -> relocate(reference, stage, destination) end))
    end)
    details_bytes = json(details)
    replace!(root, stage <> "/collection-details.json", details_bytes)
    metadata = metadata |> Map.update!("modules", fn rows ->
      Enum.map(rows, fn row -> row |> Map.update!("beam", &relocate(&1, stage, destination))
        |> Map.update!("source", fn source -> if source, do: relocate(source, stage, destination), else: nil end) end)
    end) |> Map.update!("framework", &Map.update!(&1, "beam", fn beam -> relocate(beam, stage, destination) end))
      |> Map.update!("compiler_fingerprints", fn rows -> Enum.map(rows, &Map.update!(&1, "beam", fn beam -> relocate(beam, stage, destination) end)) end)
      |> Map.put("collection_details", ref(destination <> "/collection-details.json", details_bytes))
      |> Map.update!("exact_registration", &relocate(&1, stage, destination))
    replace!(root, stage <> "/compiler-lineage.json", json(metadata))
    previous = Process.put({__MODULE__, :stage}, {Path.expand(root), destination, stage})
    try do
      {:ok, _} = verify(root, destination <> "/compiler-lineage.json")
    after
      if previous, do: Process.put({__MODULE__, :stage}, previous), else: Process.delete({__MODULE__, :stage})
    end
    {:error, :enoent} = File.lstat(Path.join(root, destination))
    :ok = File.rename(Data.checked_path!(root, stage), Path.join(root, destination))
    :ok
  rescue
    error -> {:error, Exception.message(error)}
  end

  def publish(_, _, _), do: {:error, "unowned stage or unsupported final capture path"}

  defp relocate(reference, stage, destination) do
    true = String.starts_with?(reference["path"], stage <> "/")
    Map.update!(reference, "path", &String.replace_prefix(&1, stage <> "/", destination <> "/"))
  end

  defp replace!(root, path, bytes), do: File.write!(Data.checked_path!(root, path), bytes)
  defp physical(root, path), do: mapped(root, path, :physical)
  defp logical(root, path), do: mapped(root, path, :logical)
  defp mapped(root, path, direction) do
    root = Path.expand(root)
    case Process.get({__MODULE__, :stage}) do
      {^root, final, stage} ->
        {from, to} = if direction == :physical, do: {final, stage}, else: {stage, final}
        cond do path == from -> to; String.starts_with?(path, from <> "/") -> String.replace_prefix(path, from <> "/", to <> "/"); true -> path end
      _ -> path
    end
  end

  defp read_capture!(root, path, limit), do: read!(root, physical(root, path), limit)
  defp read_capture_ref!(root, reference, limit) do
    value = read_capture!(root, reference["path"], limit)
    true = MapSet.new(Map.keys(reference)) == MapSet.new(["path", "sha256"]) and Data.digest(value) == reference["sha256"]
    value
  end

  defp module_ref(root, directory, row) do
    row |> Map.take(~w(proof_id sequence module module_definition_line source_path source_sha256 definition_matches_debug))
    |> Map.put("source", if(row["source_bytes"], do: blob(root, directory, "sources", "exs", row["source_bytes"]), else: nil))
    |> Map.put("beam", blob(root, directory, "beams", "beam", row["compiled_module"]))
    |> Map.put("loaded_md5", hex(row["loaded_md5"])) |> Map.put("definition_sha256", Data.term_digest(row["definition"]))
  end

  defp framework_ref(root, directory, native), do: %{"module" => native.module, "source_path" => native.path,
    "loaded_md5" => hex(native.md5), "beam" => blob(root, directory, "beams", "beam", native.bytes)}

  defp native(module) do
    Code.ensure_loaded!(module)
    {^module, bytes, path} = :code.get_object_code(module)
    true = Data.bounded?(bytes)
    {:ok, {^module, md5}} = :beam_lib.md5(bytes)
    true = module.module_info(:md5) == md5
    %{module: Atom.to_string(module), bytes: bytes, path: List.to_string(path), md5: md5}
  end

  defp current_inputs(source_root, manifest, details) do
    paths = Enum.map(manifest["files"], & &1["source_path"]) ++ Enum.map(details["additional_source_files"] || [], & &1["source_path"])
    paths = if File.exists?(Path.join(source_root, "mix.lock")), do: ["mix.lock" | paths], else: paths
    paths |> Enum.uniq() |> Enum.sort() |> Enum.map(&file_ref(source_root, &1, limits()["content_bytes"]))
  end

  defp observations(raw, captured) do
    index = Enum.group_by(captured, &key/1)
    Enum.map(raw, fn row ->
      proof = case Map.get(index, key(row), []) do [found] -> found["consumer_proof_id"]; _ -> nil end
      row |> Map.put("runtime_case_id", identity(row)) |> Map.put("consumer_proof_id", proof)
    end)
  end

  defp validate_content_paths!(metadata, directory) do
    Enum.each(metadata["modules"], fn row ->
      case {row["source"], row["source_sha256"], row["source_path"]} do
        {nil, nil, _} -> :ok
        {%{"path" => path, "sha256" => hash}, hash, source_path} when is_binary(source_path) ->
          true = path == directory <> "/sources/#{hash}.exs"
        _ -> raise "inconsistent nullable source facts"
      end
      if row["source_path"], do: portable!(row["source_path"])
      true = is_nil(row["module_definition_line"]) or (is_integer(row["module_definition_line"]) and row["module_definition_line"] > 0)
      true = Regex.match?(~r/^[0-9a-f]{64}$/, row["definition_sha256"])
    end)
    Enum.each(Enum.flat_map(metadata["modules"], &[&1["beam"]]) ++ Enum.map([metadata["framework"] | metadata["compiler_fingerprints"]], & &1["beam"]), fn beam ->
      true = Regex.match?(~r/^[0-9a-f]{64}$/, beam["sha256"]) and beam["path"] == directory <> "/beams/#{beam["sha256"]}.beam"
    end)
  end

  defp validate_provenance!(root, metadata, details, manifest) do
    true = details["schema_version"] == 2 and details["source_commit"] == @commit and details["mix_environment"] == "test"
    true = details["collector_sha256"] == metadata["collector"]["sha256"] and details["mix_lock_sha256"] == metadata["mix_lock_sha256"]
    true = details["elixir_version"] == metadata["elixir_version"] and details["otp_release"] == metadata["otp_release"] and details["fixture_review"] == "not-performed"
    helpers = manifest["files"] |> Enum.filter(&String.ends_with?(&1["source_path"], "/test_helper.exs")) |> Enum.sort_by(& &1["source_path"])
    hashes = Map.new(helpers, &{&1["source_path"], &1["sha256"]})
    true = details["helper_source_sha256"] == hashes and Enum.sort(Map.keys(details["helper_profiles"])) == Enum.map(helpers, & &1["source_path"])
    true = Enum.map(details["helper_results"], & &1["source_path"]) == Enum.map(helpers, & &1["source_path"])
    true = profile?(details["final_global_profile"]) and Enum.all?(Map.values(details["helper_profiles"]), &profile?/1)
    review_ref = details["additional_root_review"]
    true = review_ref["path"] == "tests/parity/collection-root-review.json"
    review = JSON.decode!(read_ref!(root, review_ref, limits()["metadata_bytes"]))
    true = review["source_commit"] == @commit and review["scope_decision"] == "pending-case-review" and review["registration_is_execution"] == false
    sources = details["additional_source_files"]
    true = Enum.map(sources, &Map.take(&1, ["source_path", "sha256"])) == Enum.map(review["files"], &Map.take(&1, ["source_path", "sha256"]))
    Enum.each(sources, fn source ->
      true = source["source_ref"]["path"] == Path.dirname(metadata["collection_details"]["path"]) <> "/source-supplement/" <> source["source_path"]
      true = source["source_ref"]["sha256"] == source["sha256"]
      true = read_capture_ref!(root, source["source_ref"], limits()["content_bytes"]) == read!(metadata["source_checkout"], source["source_path"], limits()["content_bytes"])
    end)
    roots = manifest["files"] |> Enum.map(& &1["source_path"]) |> Enum.filter(&String.ends_with?(&1, "_test.exs"))
    true = Enum.map(details["root_file_results"], & &1["source_path"]) == Enum.sort(Enum.uniq(roots ++ review["roots"]))
  end

  defp profile?(profile) when is_list(profile) do
    length(profile) == 2 and Enum.sort(Enum.map(profile, &hd/1)) == ["exclude", "include"] and
      Enum.all?(profile, &(is_list(&1) and length(&1) == 2 and is_list(Enum.at(&1, 1))))
  end
  defp profile?(_), do: false

  defp validate_exact!(details, registration, manifest, hash) do
    true = MapSet.new(Map.keys(registration)) == MapSet.new(~w(schema_version source_commit reference_manifest_sha256 collection_kind collection_complete blockers unresolved_declaration_ids cases))
    true = registration["schema_version"] == 1 and registration["source_commit"] == @commit and
      registration["reference_manifest_sha256"] == hash and registration["collection_kind"] == "exunit-registration-only"
    raw = details["all_registered_cases"]
    true = is_list(raw) and length(raw) == details["registered_case_count"]
    true = MapSet.size(MapSet.new(raw, &identity/1)) == length(raw)
    Enum.each(raw, fn row ->
      portable!(row["source_path"])
      true = is_integer(row["source_line"]) and row["source_line"] > 0 and is_map(row["tags"])
      true = String.starts_with?(row["module"], "Elixir.") and String.starts_with?(row["name"], "test ")
    end)
    locations = Enum.group_by(manifest["declarations"], &{&1["source_path"], &1["line"]})
    {expected, unmatched} = Enum.reduce(raw, {[], []}, fn row, {exact, unmatched} ->
      case Map.get(locations, {row["source_path"], row["source_line"]}, []) do
        [declaration] ->
          id = Data.digest(Enum.join([@commit, declaration["id"], row["source_path"], Integer.to_string(row["source_line"]), row["module"], row["name"]], <<0>>))
          {[row |> Map.put("source_declaration_id", declaration["id"]) |> Map.put("case_id", id) | exact], unmatched}
        _ -> {exact, [row | unmatched]}
      end
    end)
    expected = Enum.reverse(expected)
    true = Enum.sort_by(registration["cases"], &key/1) == Enum.sort_by(expected, &key/1)
    true = details["unmatched_runtime_cases"] == Enum.reverse(unmatched) and details["matched_case_count"] == length(expected)
    ids = MapSet.new(expected, & &1["source_declaration_id"])
    unresolved = manifest["declarations"] |> Enum.map(& &1["id"]) |> Enum.reject(&MapSet.member?(ids, &1)) |> Enum.sort()
    true = registration["unresolved_declaration_ids"] == unresolved
    blockers = registration["blockers"]
    true = is_list(blockers) and Enum.all?(blockers, &(is_binary(&1) and String.trim(&1) != ""))
    true = is_boolean(registration["collection_complete"]) and registration["collection_complete"] == (blockers == [])
    true = (unmatched == [] and unresolved == []) or not registration["collection_complete"]
    :ok
  end

  defp proposals(raw, proof, registration, declarations) do
    exact = MapSet.new(registration["cases"], &key/1)
    tests = Enum.reject(raw, &MapSet.member?(exact, key(&1)))
    {matches, statistics} = WorkflowCompilerLineage.match_all(tests, proof, declarations)
    true = statistics[:invalid_proof] != true
    Enum.flat_map(matches, fn {row, result} ->
      case result do {:ok, origin} -> [Map.put(origin, "runtime_case_id", identity(row))]; _ -> [] end
    end) |> Enum.sort_by(& &1["runtime_case_id"])
  end

  defp complements(raw, registration, candidates, declarations) do
    linked = MapSet.new(Enum.map(registration["cases"], &identity/1) ++ Enum.map(candidates, & &1["runtime_case_id"]))
    ids = MapSet.new(Enum.map(registration["cases"], & &1["source_declaration_id"]) ++ Enum.map(candidates, & &1["source_declaration_id"]))
    {raw |> Enum.map(&identity/1) |> Enum.reject(&MapSet.member?(linked, &1)) |> Enum.sort(),
      declarations |> Enum.map(& &1["id"]) |> Enum.reject(&MapSet.member?(ids, &1)) |> Enum.sort()}
  end

  defp content_refs(metadata), do: (Enum.flat_map(metadata["modules"], &[&1["source"], &1["beam"]]) ++
    Enum.map([metadata["framework"] | metadata["compiler_fingerprints"]], & &1["beam"])) |> Enum.reject(&is_nil/1)
  defp encode_call(row), do: row |> Map.update!("target_loaded_md5", &hex/1)
  defp decode_call(row), do: row |> Map.update!("target_loaded_md5", &unhex/1)
  defp hex(nil), do: nil
  defp hex(bytes), do: Base.encode16(bytes, case: :lower)
  defp unhex(nil), do: nil
  defp unhex(value), do: Base.decode16!(value, case: :lower)
  defp otp, do: List.to_string(:erlang.system_info(:otp_release))
  defp launcher, do: System.find_executable("elixir") || raise("missing Elixir launcher")
  defp identity(row), do: Data.digest(Enum.join([@commit, row["source_path"], Integer.to_string(row["source_line"]), row["module"], row["name"]], <<0>>))
  defp key(row), do: {row["source_path"], row["source_line"], row["module"], row["name"]}
  defp ref(path, bytes), do: %{"path" => path, "sha256" => Data.digest(bytes)}
  defp file_ref(root, path, limit), do: ref(path, read!(root, path, limit))
  defp json(value), do: JSON.encode!(value) <> "\n"

  defp blob(root, directory, kind, extension, bytes) do
    true = Data.bounded?(bytes)
    path = directory <> "/#{kind}/#{Data.digest(bytes)}.#{extension}"
    if File.exists?(Path.join(root, path)), do: true = read!(root, path, limits()["content_bytes"]) == bytes, else: write!(root, path, bytes)
    ref(path, bytes)
  end

  defp write!(root, path, bytes) do
    portable!(path)
    {:ok, %{type: :directory}} = File.lstat(Path.expand(root))
    parent = path |> Path.split() |> Enum.drop(-1) |> Enum.reduce(Path.expand(root), fn part, parent ->
      directory = Path.join(parent, part)
      case File.lstat(directory) do
        {:error, :enoent} -> File.mkdir!(directory)
        {:ok, %{type: :directory}} -> :ok
      end
      {:ok, %{type: :directory}} = File.lstat(directory)
      directory
    end)
    {:ok, file} = File.open(Path.join(parent, Path.basename(path)), [:write, :exclusive, :binary])
    try do IO.binwrite(file, bytes) after File.close(file) end
  end

  defp read_ref!(root, evidence, limit) do
    true = MapSet.new(Map.keys(evidence)) == MapSet.new(["path", "sha256"])
    value = read!(root, evidence["path"], limit)
    true = Data.digest(value) == evidence["sha256"]
    value
  end

  defp read!(root, path, limit) do
    Data.read_artifact!(root, path, limit)
  end

  defp portable!(path) do
    Data.portable_path!(path)
  end

  defp json_value(value) when is_atom(value), do: if(value in [nil, true, false], do: value, else: Atom.to_string(value))
  defp json_value(value) when is_list(value), do: Enum.map(value, &json_value/1)
  defp json_value(value) when is_tuple(value), do: value |> Tuple.to_list() |> json_value()
  defp json_value(value) when is_map(value), do: Map.new(value, fn {key, item} -> {to_string(key), json_value(item)} end)
  defp json_value(value), do: value
end
