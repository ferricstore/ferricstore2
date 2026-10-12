# Invocation needs separately reviewed original-capture authorization.
# Register in the isolated pinned application; never dispatch tests or setup.
Code.require_file("compiler-lineage-artifact.exs", __DIR__)

defmodule WorkflowCompilerCapture do
  @final "tests/parity/collection/compiler-v1"

  def run([root, @final]) do
    root = Path.expand(root)
    source_root = File.cwd!()
    {status, 0} = System.cmd("git", ["status", "--porcelain"])
    true = status == "" and Mix.env() == :test
    false = File.exists?(Path.join(root, @final))
    stage = @final <> "-staging-#{System.pid()}-#{System.unique_integer([:positive])}"
    parent = WorkflowCompilerLineageData.checked_path!(root, Path.dirname(stage))
    :ok = File.mkdir(Path.join(parent, Path.basename(stage)))
    try do
      WorkflowCompilerLineage.start(source_root)
      previous = Code.compiler_options(tracers: [WorkflowCompilerLineage])
      arguments = System.argv()
      try do
        System.argv([root, Path.join(root, stage)])
        Code.require_file("collect-elixir-cases.exs", __DIR__)
      after
        System.argv(arguments)
        Code.compiler_options(previous)
        ExUnit.configure(autorun: false)
      end
      proof = WorkflowCompilerLineage.snapshot()
      details_path = Path.join([root, stage, "collection-details.json"])
      registration_path = Path.join([root, stage, "runtime-registration.json"])
      limit = WorkflowCompilerLineageArtifact.limits()["capture_metadata_bytes"]
      details_bytes = WorkflowCompilerLineageData.read_artifact!(root, stage <> "/collection-details.json", limit)
      registration_bytes = WorkflowCompilerLineageData.read_artifact!(root, stage <> "/runtime-registration.json", limit - byte_size(details_bytes))
      details = JSON.decode!(details_bytes)
      registration = JSON.decode!(registration_bytes)
      File.rm!(details_path)
      File.rm!(registration_path)
      {:ok, metadata} = WorkflowCompilerLineageArtifact.write(root, stage, proof, details, registration)
      written_registration = WorkflowCompilerLineageData.read_artifact!(root, metadata["exact_registration"]["path"], limit)
      true = WorkflowCompilerLineageData.digest(written_registration) == metadata["exact_registration"]["sha256"]
      true = JSON.decode!(written_registration) == registration
      {:ok, _} = WorkflowCompilerLineageArtifact.verify(root, stage <> "/compiler-lineage.json")
      :ok = WorkflowCompilerLineageArtifact.publish(root, stage, @final)
      :ok = assert_original_publication(root, stage, metadata, details, written_registration)
      IO.puts("Compiler candidates captured separately; registration did not execute tests/setup or establish full parity.")
    rescue
      error ->
        File.rm_rf!(Path.join(root, stage))
        reraise error, __STACKTRACE__
    end
  end

  def run(_), do: raise("usage: mix run --no-start collect-compiler-lineage.exs RUST_ROOT #{@final}")

  defp assert_original_publication(root, stage, staged_metadata, original_details, staged_registration_bytes) do
    # No temporary final-to-stage mapping may stand in for the renamed tree.
    true = is_nil(Process.get({WorkflowCompilerLineageArtifact, :stage}))
    {:ok, _} = WorkflowCompilerLineageArtifact.verify(root, @final <> "/compiler-lineage.json")
    {:error, :enoent} = File.lstat(Path.join(root, stage))
    limit = WorkflowCompilerLineageArtifact.limits()["capture_metadata_bytes"]
    metadata_bytes = WorkflowCompilerLineageData.read_artifact!(root, @final <> "/compiler-lineage.json", limit)
    details_bytes = WorkflowCompilerLineageData.read_artifact!(root, @final <> "/collection-details.json", limit - byte_size(metadata_bytes))
    registration_bytes = WorkflowCompilerLineageData.read_artifact!(root, @final <> "/runtime-registration.json", limit - byte_size(metadata_bytes) - byte_size(details_bytes))
    true = registration_bytes == staged_registration_bytes
    metadata = JSON.decode!(metadata_bytes)
    details = JSON.decode!(details_bytes)
    relocate = fn reference ->
      true = String.starts_with?(reference["path"], stage <> "/")
      Map.update!(reference, "path", &String.replace_prefix(&1, stage <> "/", @final <> "/"))
    end
    expected_details = Map.update!(original_details, "additional_source_files", fn sources ->
      Enum.map(sources, &Map.update!(&1, "source_ref", relocate))
    end)
    expected_details_bytes = JSON.encode!(expected_details) <> "\n"
    true = details == expected_details and details_bytes == expected_details_bytes
    expected_metadata = staged_metadata |> Map.update!("modules", fn rows ->
      Enum.map(rows, fn row -> row |> Map.update!("beam", relocate)
        |> Map.update!("source", fn source -> if source, do: relocate.(source), else: nil end) end)
    end) |> Map.update!("framework", &Map.update!(&1, "beam", relocate))
      |> Map.update!("compiler_fingerprints", fn rows -> Enum.map(rows, &Map.update!(&1, "beam", relocate)) end)
      |> Map.update!("exact_registration", relocate)
      |> Map.update!("collection_details", fn reference ->
        relocate.(reference) |> Map.put("sha256", WorkflowCompilerLineageData.digest(details_bytes))
      end)
    true = metadata == expected_metadata
    content = (Enum.flat_map(expected_metadata["modules"], &[&1["source"], &1["beam"]]) ++
      Enum.map([expected_metadata["framework"] | expected_metadata["compiler_fingerprints"]], & &1["beam"]))
      |> Enum.reject(&is_nil/1)
    references = content ++ Enum.map(expected_details["additional_source_files"], & &1["source_ref"])
    expected_tree = MapSet.new(Enum.map(references, & &1["path"]) ++
      [@final <> "/compiler-lineage.json", @final <> "/collection-details.json", @final <> "/runtime-registration.json"])
    true = MapSet.new(WorkflowCompilerLineageData.snapshot_paths!(root, @final)) == expected_tree
    observations = Enum.map(references, fn reference ->
      bytes = WorkflowCompilerLineageData.read_artifact!(root, reference["path"], WorkflowCompilerLineageArtifact.limits()["content_bytes"])
      {reference["path"], WorkflowCompilerLineageData.digest(bytes)}
    end)
    true = observations == Enum.map(references, &{&1["path"], &1["sha256"]})
    :ok
  end

end

WorkflowCompilerCapture.run(System.argv())
