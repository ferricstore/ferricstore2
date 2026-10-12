# Synthetic tooling controls. No original workflow tests/setup are executed.
ExUnit.start(autorun: false)
Code.require_file("../compiler-lineage-artifact.exs", __DIR__)
Code.require_file("../case-metadata.exs", __DIR__)

defmodule WorkflowCompilerArtifactTest do
  use ExUnit.Case, async: false
  alias WorkflowCompilerLineageArtifact, as: Artifact

  setup context do
    if context[:fixture], do: artifact_fixture(), else: :ok
  end

  defp artifact_fixture do
    nonce = System.unique_integer([:positive])
    root = Path.join(System.tmp_dir!(), "wf-compiler-artifact-tests-#{System.pid()}-#{nonce}")
    :ok = File.mkdir(root)
    on_exit(fn -> File.rm_rf!(root) end)
    File.mkdir_p!(Path.join(root, "source"))
    File.mkdir_p!(Path.join(root, "tests/reference"))
    section_name = "CompilerArtifactSection#{nonce}"
    consumer_name = "CompilerArtifactConsumer#{nonce}"
    section = """
    defmodule #{section_name} do
      defmacro __using__(_opts) do
        quote do
          test "synthetic" do
            send(self(), {:artifact_marker, :literal})
          end
        end
      end
    end
    """
    consumer = """
    defmodule #{consumer_name} do
      use ExUnit.Case, register: false
      use #{section_name}
    end
    """
    files = [{"source/section.exs", section}, {"source/consumer.exs", consumer}]
    WorkflowCompilerLineage.start(root)
    previous = Code.compiler_options(tracers: [WorkflowCompilerLineage])
    Enum.each(files, fn {path, bytes} ->
      File.write!(Path.join(root, path), bytes)
      Code.compile_file(Path.join(root, path))
    end)
    Code.compiler_options(previous)
    proof = WorkflowCompilerLineage.snapshot()
    manifest = %{"declarations" => [%{"id" => "synthetic", "source_path" => "source/section.exs", "line" => 4, "declaration" => "synthetic"}],
      "files" => Enum.map(files, fn {path, bytes} -> %{"source_path" => path, "sha256" => WorkflowCompilerLineageData.digest(bytes)} end)}
    File.write!(Path.join(root, "tests/reference/workflow-test-manifest.json"), JSON.encode!(manifest))
    repo = Path.expand("../..", __DIR__)
    for path <- ["scripts/collect-elixir-cases.exs", "scripts/compiler-lineage.exs",
        "scripts/compiler-lineage-data.exs", "scripts/compiler-lineage-verifier.exs",
        "scripts/compiler-lineage-artifact.exs", "scripts/collect-compiler-lineage.exs",
        "scripts/verify-compiler-lineage.exs", "tests/parity/workflow-compiler-lineage.schema.json", "scripts/case-metadata.exs"],
        File.exists?(Path.join(repo, path)) do
      File.mkdir_p!(Path.dirname(Path.join(root, path)))
      File.cp!(Path.join(repo, path), Path.join(root, path))
    end
    [registered] = proof.registered
    raw = registered |> Map.delete("consumer_proof_id") |> Map.put("tags", %{"line" => 3, "file" => Path.join(root, "source/consumer.exs"), "synthetic" => true})
    details = %{"all_registered_cases" => [raw], "registered_case_count" => 1, "matched_case_count" => 0,
      "unmatched_runtime_cases" => [raw], "test_execution" => false, "setup_execution" => false,
      "helper_results" => [], "helper_profiles" => %{}, "final_global_profile" => %{"exclude" => [], "include" => []},
      "mix_lock_sha256" => WorkflowCompilerLineageData.digest(<<>>)}
    registration = %{"schema_version" => 1, "source_commit" => "66586c596e271408d9184af32eb31cd7218c126e",
      "reference_manifest_sha256" => WorkflowCompilerLineageData.digest(File.read!(Path.join(root, "tests/reference/workflow-test-manifest.json"))),
      "collection_kind" => "exunit-registration-only", "collection_complete" => false,
      "blockers" => ["synthetic unmatched location"], "cases" => [], "unresolved_declaration_ids" => ["synthetic"]}
    {:ok, root: root, proof: proof, details: details, registration: registration, manifest: manifest}
  end

  test "metadata normalization preserves raw tags and ordered profiles" do
    value = %{file: "source/consumer.exs", line: 3, bytes: <<255, 0>>,
      context: {:manual, ["é", 1, true, nil]}, profile: [exclude: [:slow], include: [:cluster]]}
    expected = %{"file" => "source/consumer.exs", "line" => 3,
      "bytes" => %{"binary_base64" => "/wA="}, "context" => ["manual", ["é", 1, true, nil]],
      "profile" => [["exclude", ["slow"]], ["include", ["cluster"]]]}
    assert {^expected, {0, visits, bytes}} = WorkflowCaseMetadata.admit(value, WorkflowCaseMetadata.new_budget())
    assert visits > 20 and bytes == :erlang.external_size(expected)
    assert {%{"binary_base64" => "/w=="}, {0, 4, actual}} = WorkflowCaseMetadata.admit(<<255>>, {0, 0, 0})
    assert actual == :erlang.external_size(%{"binary_base64" => "/w=="})
  end

  test "metadata admission rejects depth node and scalar excess before conversion" do
    nested = Enum.reduce(1..96, nil, fn _, tail -> [tail] end)
    assert {^nested, {0, 193, _}} = WorkflowCaseMetadata.admit(nested, {0, 0, 0})
    assert_raise ArgumentError, "metadata depth limit", fn -> WorkflowCaseMetadata.admit([nested], {0, 0, 0}) end
    assert {_, {0, 199_999, _}} = WorkflowCaseMetadata.admit(List.duplicate(0, 99_999), {0, 0, 0})
    assert_raise ArgumentError, "metadata node limit", fn -> WorkflowCaseMetadata.admit(List.duplicate(0, 100_000), {0, 0, 0}) end
    assert_raise ArgumentError, "metadata scalar limit", fn -> WorkflowCaseMetadata.admit(:binary.copy("x", 16 * 1024 * 1024 + 1), {0, 0, 0}) end
    assert_raise ArgumentError, "metadata byte limit", fn -> WorkflowCaseMetadata.admit(<<255>> <> :binary.copy("x", 12 * 1024 * 1024 - 1), {0, 0, 0}) end
    row = %{"source_path" => "x", "source_line" => 1, "module" => "Elixir.X", "name" => "test x", "tags" => %{"file" => "x", "line" => 1}}
    {^row, {0, 15, first}} = WorkflowCaseMetadata.admit(row, {0, 0, 0})
    {list, {0, 32, second}} = WorkflowCaseMetadata.cons(row, [], {0, 15, first})
    assert second == first + :erlang.external_size([row])
    {^list, {0, 49, third}} = WorkflowCaseMetadata.retain(list, :container, {0, 32, second})
    assert third == second + :erlang.external_size(list)
    assert {^list, {0, 66, _}} = WorkflowCaseMetadata.retain(list, :container, {0, 49, third})
    assert_raise ArgumentError, "metadata node limit", fn -> WorkflowCaseMetadata.retain(row, :row, {0, 3_999_999, 0}) end
    assert_raise ArgumentError, "metadata retained byte limit", fn -> WorkflowCaseMetadata.retain(row, :row, {0, 0, 64 * 1024 * 1024}) end
  end

  @tag fixture: true
  test "runtime row admission rejects count excess and invalid identities", context do
    test = %{name: :"test count", tags: %{file: Path.join(context.root, "source/consumer.exs"), line: 3}}
    expected = %{"source_path" => "source/consumer.exs", "source_line" => 3,
      "module" => "Elixir.WorkflowCompilerArtifactTest", "name" => "test count", "tags" => %{"file" => test.tags.file, "line" => 3}}
    budget = Enum.reduce(1..100_000, {0, 0, 0}, fn _, state ->
      {^expected, next} = WorkflowCaseMetadata.admit_case({test, __MODULE__, context.root}, state)
      next
    end)
    assert budget == {100_000, 1_500_000, 100_000 * :erlang.external_size(expected)}
    assert_raise ArgumentError, "runtime case count exceeds limit", fn -> WorkflowCaseMetadata.admit_case({%{}, nil, nil}, budget) end
    assert_raise ArgumentError, "invalid runtime identity", fn -> WorkflowCaseMetadata.admit_case({%{test | tags: %{file: test.tags.file, line: 0}}, __MODULE__, context.root}, {0, 0, 0}) end
    assert_raise ArgumentError, "invalid runtime identity", fn -> WorkflowCaseMetadata.admit_case({%{test | name: "test invalid"}, __MODULE__, context.root}, {0, 0, 0}) end
  end

  test "unsupported metadata and colliding keys fail without protocol formatting" do
    assert_raise ArgumentError, "unsupported metadata key", fn -> WorkflowCaseMetadata.admit(%{URI.parse("https://example.test") => 1}, {0, 0, 0}) end
    assert_raise ArgumentError, "normalized metadata key collision", fn -> WorkflowCaseMetadata.admit(%{:scope => 1, "scope" => 2}, {0, 0, 0}) end
    marker = make_ref()
    callback = fn -> send(self(), {:metadata_callback, marker}) end
    for value <- [callback, self(), marker, <<1::1>>, [1 | 2]] do
      assert_raise ArgumentError, "unsupported metadata value", fn -> WorkflowCaseMetadata.admit(value, {0, 0, 0}) end
    end
    refute_received {:metadata_callback, ^marker}
    for value <- [-9_223_372_036_854_775_808, 18_446_744_073_709_551_615] do
      assert {^value, _} = WorkflowCaseMetadata.admit(value, {0, 0, 0})
    end
    for value <- [-9_223_372_036_854_775_809, 18_446_744_073_709_551_616] do
      assert_raise ArgumentError, "metadata integer limit", fn -> WorkflowCaseMetadata.admit(value, {0, 0, 0}) end
    end
  end

  @tag fixture: true
  test "canonical metadata JSON preserves escapes numbers and deterministic order", context do
    source = "source/consumer.exs"
    reason = "backslash\\ quote\" newline\n é"
    normalized = %{"source_path" => source, "kind" => "error", "reason" => reason, "stack" => []}
    expected = JSON.encode!(normalized)
    {diagnostic, budget} = WorkflowCaseMetadata.failure(source, :error, reason, [], {0, 0, 0})
    assert diagnostic == expected and JSON.decode!(diagnostic) == normalized
    assert budget == {0, 10, :erlang.external_size(normalized) + :erlang.external_size(diagnostic)}

    empty_reason = %{normalized | "reason" => ""}
    raw_base = JSON.encode!(empty_reason)
    enclosed_base = byte_size(JSON.encode!(raw_base))
    available = 16 * 1024 * 1024 - enclosed_base
    backslashes = div(available, 4)
    plain = rem(available, 4)
    boundary_reason = :binary.copy("\\", backslashes) <> :binary.copy("x", plain)
    boundary_map = %{normalized | "reason" => boundary_reason}
    assert enclosed_base + 4 * backslashes + plain == 16 * 1024 * 1024
    {diagnostic, budget} = WorkflowCaseMetadata.failure(source, :error, boundary_reason, [], {0, 0, 0})
    assert byte_size(diagnostic) == byte_size(raw_base) + 2 * backslashes + plain
    assert JSON.decode!(diagnostic) == boundary_map
    assert budget == {0, 10, :erlang.external_size(boundary_map) + :erlang.external_size(diagnostic)}
    assert_raise ArgumentError, "metadata byte limit", fn ->
      WorkflowCaseMetadata.failure(source, :error, boundary_reason <> "x", [], {0, 0, 0})
    end

    collector = WorkflowCompilerLineageData.read_artifact!(context.root, "scripts/collect-elixir-cases.exs", Artifact.limits()["content_bytes"])
    assert is_tuple(Code.string_to_quoted!(collector, file: "scripts/collect-elixir-cases.exs"))
    :ok = File.mkdir(Path.join(context.root, "capture"))
    golden = %{"b" => "é\n\"\\\0", "a" => 1, "n" => [nil, true, false, -1, 1.5]}
    details = Map.merge(context.details, %{"schema_version" => 2, "extra" => golden})
    {reg_size, detail_size, nodes} = WorkflowCaseMetadata.measure_pair(context.registration, details)
    assert nodes > 30
    assert {{"capture/runtime-registration.json", ^reg_size, reg_hash}, {"capture/collection-details.json", ^detail_size, detail_hash}} = WorkflowCaseMetadata.write_pair(context.root, "capture", context.registration, details)
    reg = File.read!(Path.join(context.root, "capture/runtime-registration.json"))
    det = File.read!(Path.join(context.root, "capture/collection-details.json"))
    assert byte_size(reg) == reg_size and byte_size(det) == detail_size
    assert reg_hash == WorkflowCompilerLineageData.digest(reg) and detail_hash == WorkflowCompilerLineageData.digest(det)
    assert JSON.decode!(reg) == context.registration and JSON.decode!(det) == details
    assert String.contains?(det, "\"extra\":{\"a\":1,\"b\":\"é\\n\\\"\\\\\\u0000\",\"n\":[null,true,false,-1,1.5]}")
    Enum.each(["capture/runtime-registration.json", "capture/collection-details.json"], &File.rm!(Path.join(context.root, &1)))
    assert {:ok, _} = Artifact.write(context.root, "capture", context.proof, details, context.registration)
    assert {:error, _} = Artifact.verify(context.root, "capture/compiler-lineage.json")
  end

  @tag fixture: true
  test "paired metadata output rejects excess before opening either file", context do
    :ok = File.mkdir(Path.join(context.root, "capture"))
    details = Map.merge(context.details, %{"schema_version" => 2, "padding_rows" => List.duplicate(%{"padding" => ""}, 4)})
    base = byte_size(JSON.encode!(context.registration)) + byte_size(JSON.encode!(details)) + 2
    fill = 64 * 1024 * 1024 - base
    sizes = [div(fill, 4) + rem(fill, 4), div(fill, 4), div(fill, 4), div(fill, 4)]
    padding = Enum.map(sizes, &%{"padding" => :binary.copy("x", &1)})
    details = %{details | "padding_rows" => padding}
    {registration_size, details_size, _} = WorkflowCaseMetadata.measure_pair(context.registration, details)
    assert registration_size + details_size == 64 * 1024 * 1024
    [first | rest] = padding
    excess = %{details | "padding_rows" => [%{"padding" => first["padding"] <> "x"} | rest]}
    assert_raise ArgumentError, "metadata byte limit", fn -> WorkflowCaseMetadata.write_pair(context.root, "capture", context.registration, excess) end
    assert File.ls!(Path.join(context.root, "capture")) == []
  end

  @tag fixture: true
  test "paired metadata writer preserves existing targets and owned cleanup", context do
    :ok = File.mkdir(Path.join(context.root, "capture"))
    destination = Path.join(context.root, "capture/collection-details.json")
    :ok = File.write(destination, "inert existing JSON target\n", [:exclusive])
    before = {File.ls!(Path.dirname(destination)), WorkflowCompilerLineageData.digest(File.read!(destination))}
    details = Map.put(context.details, "schema_version", 2)
    assert_raise ArgumentError, "metadata targets already exist", fn -> WorkflowCaseMetadata.write_pair(context.root, "capture", context.registration, details) end
    assert {File.ls!(Path.dirname(destination)), WorkflowCompilerLineageData.digest(File.read!(destination))} == before
    assert {:error, :enoent} = File.lstat(Path.join(context.root, "capture/runtime-registration.json"))
    File.rm!(destination)
    assert {{_, _, _}, {_, _, _}} = WorkflowCaseMetadata.write_pair(context.root, "capture", context.registration, details)
    assert JSON.decode!(File.read!(destination)) == details
  end

  test "fixed capture profiles cannot be raised by input metadata" do
    limits = Artifact.limits()
    assert limits["content_bytes"] == 16 * 1024 * 1024
    assert limits["aggregate_content_bytes"] == 256 * 1024 * 1024
    assert limits["capture_metadata_bytes"] == 64 * 1024 * 1024
    assert limits["metadata_bytes"] == 64 * 1024 * 1024
    assert limits["module_versions"] == 10_000
    assert limits["calls"] == 1_000_000
    assert limits["runtime_cases"] == 100_000
    assert limits["stdout_bytes"] == 1024 * 1024
    assert limits["stderr_bytes"] == 1024 * 1024
    assert limits["verifier_deadline_ms"] == 60_000
    refute Artifact.valid_limits?(Map.put(limits, "content_bytes", limits["content_bytes"] + 1))
    refute Artifact.valid_limits?(Map.put(limits, "trusted", true))
  end

  test "actual equal content is charged once and changed bytes remain distinct" do
    assert Artifact.unique_bytes(["abc", "abc", "de"]) == 5
    assert Artifact.unique_bytes(["abc", "abd", "de"]) == 8
    assert Artifact.unique_bytes([]) == 0
  end

  test "zero registration and fabricated execution statuses cannot form a capture" do
    assert {:error, _} = Artifact.check_requirements(%{"registered" => []})
    assert {:error, _} = Artifact.check_requirements(%{"test_execution" => true})
  end

  test "compressed debug data cannot bypass actual expanded-content limits" do
    Code.ensure_loaded!(ExUnit.Case)
    {_, bytes, _} = :code.get_object_code(ExUnit.Case)
    {:ok, _, chunks} = :beam_lib.all_chunks(bytes)
    {~c"Dbgi", debug} = List.keyfind(chunks, ~c"Dbgi", 0)
    {:debug_info_v1, :elixir_erl, {:elixir_v1, data, specs}} = :erlang.binary_to_term(debug)
    expanded = Map.put(data, :unused_padding, :binary.copy("x", 16 * 1024 * 1024 + 1))
    debug = :erlang.term_to_binary({:debug_info_v1, :elixir_erl, {:elixir_v1, expanded, specs}}, [:compressed])
    {:ok, oversized} = :beam_lib.build_module(List.keyreplace(chunks, ~c"Dbgi", 0, {~c"Dbgi", debug}))
    assert byte_size(oversized) < 16 * 1024 * 1024
    assert :unresolved = WorkflowCompilerLineageData.beam(oversized)
  end

  test "compressed debug streams require exact size and complete end of stream" do
    {_, bytes, _} = :code.get_object_code(ExUnit.Case)
    {:ok, _, chunks} = :beam_lib.all_chunks(bytes)
    {~c"Dbgi", debug} = List.keyfind(chunks, ~c"Dbgi", 0)
    compressed = :erlang.term_to_binary(:erlang.binary_to_term(debug), [:compressed])
    <<131, 80, advertised::32, data::binary>> = compressed
    for invalid <- [<<131, 80, (advertised + 1)::32, data::binary>>,
        <<131, 80, advertised::32, binary_part(data, 0, byte_size(data) - 2)::binary>>] do
      {:ok, changed} = :beam_lib.build_module(List.keyreplace(chunks, ~c"Dbgi", 0, {~c"Dbgi", invalid}))
      assert :unresolved = WorkflowCompilerLineageData.beam(changed)
    end
  end

  @tag fixture: true
  test "synthetic round trip preserves bytes, tags and pure origin without full admission", context do
    assert {:ok, metadata} = Artifact.write(context.root, "capture", context.proof, context.details, context.registration)
    assert metadata["admission"] == "proposal-only"
    assert metadata["requirements"]["test_execution"] == false
    assert metadata["registered"] |> hd() |> Map.fetch!("tags") == hd(context.details["all_registered_cases"])["tags"]
    assert {:ok, restored, parsed} = Artifact.rehydrate(context.root, "capture/compiler-lineage.json")
    assert parsed == metadata
    [test] = restored.registered
    assert {:ok, origin} = WorkflowCompilerLineage.match(test, restored, context.manifest["declarations"])
    assert origin["source_declaration_id"] == "synthetic"
    assert {:error, _} = Artifact.verify(context.root, "capture/compiler-lineage.json")
    File.write!(Path.join(context.root, "source/section.exs"), File.read!(Path.join(context.root, "source/section.exs")) <> "\n# changed current input\n")
    assert {:unresolved, _} = WorkflowCompilerLineage.match(test, restored, context.manifest["declarations"])
  end

  @tag fixture: true
  test "tampered content and missing tool input fail the copied-data stage", context do
    assert {:ok, metadata} = Artifact.write(context.root, "capture", context.proof, context.details, context.registration)
    [section] = Enum.filter(metadata["modules"], &(&1["source_path"] == "source/section.exs"))
    File.write!(Path.join(context.root, section["source"]["path"]), "tampered")
    assert {:error, _} = Artifact.rehydrate(context.root, "capture/compiler-lineage.json")
    File.write!(Path.join(context.root, section["source"]["path"]), File.read!(Path.join(context.root, "source/section.exs")))
    assert {:ok, _, _} = Artifact.rehydrate(context.root, "capture/compiler-lineage.json")
    helper = Path.join(context.root, "scripts/compiler-lineage-data.exs")
    original = File.read!(helper)
    File.write!(helper, original <> "\n# changed tool input\n")
    assert {:error, _} = Artifact.rehydrate(context.root, "capture/compiler-lineage.json")
    File.write!(helper, original)
    assert {:ok, _, _} = Artifact.rehydrate(context.root, "capture/compiler-lineage.json")
    File.rm!(Path.join(context.root, "scripts/verify-compiler-lineage.exs"))
    assert {:error, _} = Artifact.rehydrate(context.root, "capture/compiler-lineage.json")
  end

  @tag fixture: true
  test "exact registration cannot assign a declaration to a different physical location", context do
    [raw] = context.details["all_registered_cases"]
    identity = Enum.join([context.registration["source_commit"], "synthetic", raw["source_path"],
      Integer.to_string(raw["source_line"]), raw["module"], raw["name"]], <<0>>)
    forged = raw |> Map.put("source_declaration_id", "synthetic")
      |> Map.put("case_id", WorkflowCompilerLineageData.digest(identity))
    registration = %{context.registration | "cases" => [forged], "unresolved_declaration_ids" => []}
    details = %{context.details | "unmatched_runtime_cases" => []}
    assert {:error, _} = Artifact.write(context.root, "capture", context.proof, details, registration)
  end

  @tag fixture: true
  test "unmatched complement preserves complete original tags and identities", context do
    [raw] = context.details["all_registered_cases"]
    changed = %{raw | "tags" => Map.put(raw["tags"], "synthetic", false)}
    details = %{context.details | "unmatched_runtime_cases" => [changed]}
    assert {:error, _} = Artifact.write(context.root, "capture", context.proof, details, context.registration)
  end

  @tag fixture: true
  test "an unowned stage and unsupported destination cannot be published", context do
    assert {:ok, _} = Artifact.write(context.root, "capture", context.proof, context.details, context.registration)
    before = File.read!(Path.join(context.root, "capture/compiler-lineage.json"))
    assert {:error, _} = Artifact.publish(context.root, "capture", "capture")
    assert File.read!(Path.join(context.root, "capture/compiler-lineage.json")) == before
  end

  @tag fixture: true
  test "existing final capture preserves destination and owned stage", context do
    stage = "tests/parity/collection/compiler-v1-staging-#{System.pid()}-#{System.unique_integer([:positive])}"
    destination = "tests/parity/collection/compiler-v1"
    assert {:ok, _} = Artifact.write(context.root, stage, context.proof, context.details, context.registration)
    File.mkdir_p!(Path.join(context.root, destination))
    assert :ok = File.write(Path.join(context.root, destination <> "/compiler-lineage.json"),
      "inert existing final capture sentinel\n", [:exclusive])
    snapshot = fn directory ->
      WorkflowCompilerLineageData.snapshot_paths!(context.root, directory)
      |> Enum.sort()
      |> Enum.map(fn path ->
        bytes = WorkflowCompilerLineageData.read_artifact!(context.root, path, Artifact.limits()["content_bytes"])
        {path, WorkflowCompilerLineageData.digest(bytes)}
      end)
    end
    before = Enum.map([stage, destination], snapshot)
    assert {:error, reason} = Artifact.publish(context.root, stage, destination)
    assert reason == Exception.message(%MatchError{term: true})
    assert Enum.map([stage, destination], snapshot) == before
  end

  @tag fixture: true
  test "generated JSON reads reject oversized input before decoding or deletion", context do
    assert {:ok, _} = Artifact.write(context.root, "capture", context.proof, context.details, context.registration)
    paths = ["capture/collection-details.json", "capture/runtime-registration.json"]
    limit = Artifact.limits()["capture_metadata_bytes"]
    hash_generated_json = fn path, remaining ->
      absolute = WorkflowCompilerLineageData.checked_path!(context.root, path)
      assert %{type: :regular} = File.lstat!(absolute)
      {:ok, observation} = File.open(absolute, [:read, :binary], fn file ->
        {hash, count} = Stream.repeatedly(fn -> IO.binread(file, 64 * 1024) end)
          |> Enum.reduce_while({:crypto.hash_init(:sha256), 0}, fn
            :eof, state -> {:halt, state}
            bytes, {hash, count} ->
              assert is_binary(bytes) and byte_size(bytes) <= 64 * 1024
              count = count + byte_size(bytes)
              assert count <= limit + 1 and count <= remaining
              {:cont, {:crypto.hash_update(hash, bytes), count}}
          end)
        {path, count, :crypto.hash_final(hash) |> Base.encode16(case: :lower)}
      end)
      observation
    end
    snapshot_generated_json = fn ->
      inventory = WorkflowCompilerLineageData.snapshot_paths!(context.root, "capture") |> Enum.sort()
      {observations, _} = Enum.reduce(paths, {[], 0}, fn path, {rows, total} ->
        {_, count, _} = row = hash_generated_json.(path, limit + 16 - total)
        {[row | rows], total + count}
      end)
      ordinary = Enum.map(inventory -- paths, fn path ->
        bytes = WorkflowCompilerLineageData.read_artifact!(context.root, path, Artifact.limits()["content_bytes"])
        {path, byte_size(bytes), WorkflowCompilerLineageData.digest(bytes)}
      end)
      {inventory, Enum.reverse(observations), ordinary}
    end
    for sizes <- [[limit + 1, 15], [div(limit, 2), div(limit, 2) + 1], [div(limit, 2), div(limit, 2)]] do
      Enum.each(Enum.zip(paths, sizes), fn {path, size} ->
        bytes = "{\"padding\":\"" <> :binary.copy("x", size - 15) <> "\"}\n"
        assert byte_size(bytes) == size
        File.write!(WorkflowCompilerLineageData.checked_path!(context.root, path), bytes)
      end)
      before = snapshot_generated_json.()
      if Enum.sum(sizes) > limit do
        marker = make_ref()
        assert %MatchError{term: false} = assert_raise MatchError, fn ->
          details = WorkflowCompilerLineageData.read_artifact!(context.root, hd(paths), limit)
          registration = WorkflowCompilerLineageData.read_artifact!(context.root, List.last(paths), limit - byte_size(details))
          send(self(), {:decode_delete_started, marker})
          JSON.decode!(details)
          JSON.decode!(registration)
          Enum.each(paths, &File.rm!(Path.join(context.root, &1)))
        end
        refute_received {:decode_delete_started, ^marker}
      else
        details = WorkflowCompilerLineageData.read_artifact!(context.root, hd(paths), limit)
        registration = WorkflowCompilerLineageData.read_artifact!(context.root, List.last(paths), limit - byte_size(details))
        assert byte_size(details) + byte_size(registration) == limit
        assert byte_size(JSON.decode!(details)["padding"]) == hd(sizes) - 15
        assert byte_size(JSON.decode!(registration)["padding"]) == List.last(sizes) - 15
      end
      assert snapshot_generated_json.() == before
    end
  end

  @tag fixture: true
  test "synthetic stage cannot publish without genuine frozen inputs", context do
    stage = "tests/parity/collection/compiler-v1-staging-#{System.pid()}-#{System.unique_integer([:positive])}"
    destination = "tests/parity/collection/compiler-v1"
    assert {:ok, _} = Artifact.write(context.root, stage, context.proof, context.details, context.registration)
    snapshot = fn ->
      WorkflowCompilerLineageData.snapshot_paths!(context.root, stage)
      |> Enum.sort()
      |> Enum.map(fn path ->
        bytes = WorkflowCompilerLineageData.read_artifact!(context.root, path, Artifact.limits()["content_bytes"])
        {path, WorkflowCompilerLineageData.digest(bytes)}
      end)
    end
    before = snapshot.()
    assert {:error, :enoent} = File.lstat(Path.join(context.root, destination))
    assert {:error, verification_reason} = Artifact.verify(context.root, stage <> "/compiler-lineage.json")
    assert {:error, publication_reason} = Artifact.publish(context.root, stage, destination)
    assert publication_reason == Exception.message(%MatchError{term: {:error, verification_reason}})
    assert snapshot.() == before
    assert {:error, :enoent} = File.lstat(Path.join(context.root, destination))
  end

  @tag fixture: true
  test "actual decoded definitions share the aggregate metadata budget", context do
    section = Enum.find(context.proof.modules, &(&1["source_path"] == "source/section.exs"))
    name = String.trim_leading(section["module"], "Elixir.")
    padding = :binary.copy("x", 10 * 1024 * 1024)
    previous = Code.compiler_options(ignore_module_conflict: true)
    versions = for version <- 1..7 do
      source = "defmodule #{name} do\n defmacro __using__(_opts) do\n quote do\n test \"synthetic\" do\n send(self(), {:artifact_marker, :literal})\n byte_size(\"#{padding}#{version}\")\n end\n end\n end\n end\n"
      File.write!(Path.join(context.root, "source/section.exs"), source)
      [{_, bytes}] = Code.compile_file(Path.join(context.root, "source/section.exs"))
      section |> Map.put("sequence", version) |> Map.put("source_bytes", nil)
      |> Map.put("source_path", nil) |> Map.put("source_sha256", nil) |> Map.put("definition", nil)
      |> Map.put("compiled_module", bytes) |> Map.put("compiled_module_sha256", WorkflowCompilerLineageData.digest(bytes))
    end
    Code.compiler_options(previous)
    actual = Enum.reduce(versions, 0, fn row, sum ->
      {:ok, compiled} = WorkflowCompilerLineageData.beam(row["compiled_module"])
      sum + :erlang.external_size(compiled.definition)
    end)
    assert actual > Artifact.limits()["capture_metadata_bytes"]
    assert WorkflowCompilerLineageData.valid_budget?(versions, [])
    proof = %{context.proof | modules: versions, calls: [], registered: []}
    {[], statistics} = WorkflowCompilerLineage.match_all([], proof, [])
    assert statistics[:invalid_proof] == true
  end
end

result = ExUnit.run()
if result.failures != 0, do: System.halt(2)
