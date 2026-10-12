ExUnit.start(autorun: false)
Code.require_file("../compiler-lineage.exs", __DIR__)

defmodule WorkflowCompilerLineageTest do
  use ExUnit.Case, async: false

  setup do
    root =
      Path.join(System.tmp_dir!(), "wf-compiler-lineage-#{System.unique_integer([:positive])}")

    File.mkdir_p!(root)
    WorkflowCompilerLineage.start(root)
    before = Code.compiler_options(tracers: [WorkflowCompilerLineage])

    on_exit(fn ->
      Code.compiler_options(before)
      File.rm_rf!(root)
    end)

    {:ok, root: root}
  end

  for mode <- [:use, :call, :top, :source_environment] do
    @mode mode
    test "actual compiled target resolves #{mode} aliases", %{root: root} do
      {test, declarations, expected_path, consumer} = fixture(root, @mode)
      proof = WorkflowCompilerLineage.snapshot()
      assert {:ok, origin} = WorkflowCompilerLineage.match(test, proof, declarations)
      assert origin["source_declaration_id"] == "second"
      assert origin["source_path"] == expected_path
      assert origin["macro_module"] == consumer.expected_macro
      assert consumer.module.__ex_unit__().tests |> Enum.any?(&(&1.name == :"test same"))
      assert apply(consumer.module, :"test same", [%{}]) == :ok
      assert_received {:compiler_lineage_body, :second}
      assert origin["compiled_quote_sha256"] |> byte_size() == 64
      assert origin["compiled_module_sha256"] |> byte_size() == 64
    end
  end

  test "missing or changed compiled proof remains unresolved", %{root: root} do
    {test, declarations, _, _} = fixture(root, :use)
    proof = WorkflowCompilerLineage.snapshot()

    assert {:unresolved, _} =
             WorkflowCompilerLineage.match(test, %{proof | modules: []}, declarations)

    changed = %{
      proof
      | modules: Enum.map(proof.modules, &Map.put(&1, "source_sha256", String.duplicate("0", 64)))
    }

    assert {:unresolved, _} = WorkflowCompilerLineage.match(test, changed, declarations)
  end

  test "ambiguous declaration and consumer identity cannot be guessed", %{root: root} do
    {test, declarations, _, _} = fixture(root, :call)
    proof = WorkflowCompilerLineage.snapshot()

    assert {:unresolved, _} =
             WorkflowCompilerLineage.match(test, proof, declarations ++ declarations)

    assert {:unresolved, _} =
             WorkflowCompilerLineage.match(
               Map.put(test, "module", "Elixir.Unknown"),
               proof,
               declarations
             )

    assert {:unresolved, _} =
             WorkflowCompilerLineage.match(Map.put(test, "source_line", 999), proof, declarations)
  end

  test "loaded code fingerprints are checked against captured bytecode", %{root: root} do
    {test, declarations, _, _} = fixture(root, :use)
    proof = WorkflowCompilerLineage.snapshot()
    changed = %{proof | modules: Enum.map(proof.modules, &Map.put(&1, "loaded_md5", <<0::128>>))}
    assert {:unresolved, _} = WorkflowCompilerLineage.match(test, changed, declarations)
  end

  test "module proof IDs cannot be invented even with consistent references", %{root: root} do
    {test, declarations, path, _} = fixture(root, :use)
    proof = WorkflowCompilerLineage.snapshot()
    module = Enum.find(proof.modules, &(&1["source_path"] == path))

    changed = %{
      proof
      | modules:
          Enum.map(proof.modules, fn row ->
            if row == module, do: Map.put(row, "proof_id", String.duplicate("0", 64)), else: row
          end),
        calls:
          Enum.map(proof.calls, fn call ->
            if call["target_proof_id"] == module["proof_id"],
              do: Map.put(call, "target_proof_id", String.duplicate("0", 64)),
              else: call
          end)
    }

    assert {:unresolved, _} = WorkflowCompilerLineage.match(test, changed, declarations)
  end

  test "genuine Case trace and full captured module bytes are necessary", %{root: root} do
    {test, declarations, _, _} = fixture(root, :call)
    proof = WorkflowCompilerLineage.snapshot()
    absent = %{proof | calls: Enum.reject(proof.calls, &(&1["name"] == "test"))}
    assert {:unresolved, _} = WorkflowCompilerLineage.match(test, absent, declarations)

    changed = %{
      proof
      | modules: Enum.map(proof.modules, &Map.put(&1, "compiled_module", "bad-beam"))
    }

    assert {:unresolved, _} = WorkflowCompilerLineage.match(test, changed, declarations)
  end

  test "data decoder rejects dynamic code guards and unknown quote directives" do
    alias WorkflowCompilerLineageData, as: Data
    clause = {[], [{:_, [], nil}], [], {:+, [], [1, 2]}}
    assert :unresolved = Data.quote_body({:v1, :defmacro, [line: 1], [clause]})
    assert :unresolved = Data.quote_body({:v1, :defmacro, [line: 1], [clause, clause]})
    assert [] = Data.literal_tests(quote do: use(UnknownNestedSection))
  end

  test "an explicit remote macro call is not a literal source use", %{root: root} do
    {test, declarations, _, _} = fixture(root, :direct)
    proof = WorkflowCompilerLineage.snapshot()
    assert {:unresolved, _} = WorkflowCompilerLineage.match(test, proof, declarations)
  end

  test "a shadowed test macro cannot delegate a substituted body", %{root: root} do
    {test, declarations, consumer} = delegated_fixture(root)
    proof = WorkflowCompilerLineage.snapshot()
    assert apply(consumer, :"test same", [%{}]) == :ok
    assert_received {:delegated_body, :foreign}
    assert {:unresolved, _} = WorkflowCompilerLineage.match(test, proof, declarations)
  end

  test "a named unused binder with an actual constant return is admitted", %{root: root} do
    {test, declarations, path, consumer} = fixture(root, :use, "_opts")
    proof = WorkflowCompilerLineage.snapshot()
    assert {:ok, origin} = WorkflowCompilerLineage.match(test, proof, declarations)
    assert origin["source_declaration_id"] == "second"
    assert origin["source_path"] == path
    assert apply(consumer.module, :"test same", [%{}]) == :ok
    assert_received {:compiler_lineage_body, :second}
  end

  for {field, value} <- [{"target_module", "Elixir.UnrelatedMacro"}, {"kind", "remote_function"}] do
    @changed_field field
    @changed_value value
    test "use #{@changed_field} must agree with the bound compiled macro", %{root: root} do
      {test, declarations, _, _} = fixture(root, :use)
      proof = WorkflowCompilerLineage.snapshot()
      changed = change_use(proof, test, &Map.put(&1, @changed_field, @changed_value))
      assert {:unresolved, _} = WorkflowCompilerLineage.match(test, changed, declarations)
    end
  end

  test "a consumer proof cannot come from another actual module and file", %{root: root} do
    {test, declarations, _, consumer} = fixture(root, :use)
    other = "OtherActualConsumer#{System.unique_integer([:positive])}"

    write_compile(root, "other_consumer.exs", """
    defmodule #{other} do
      use ExUnit.Case
      @moduledoc false
      use #{String.trim_leading(consumer.expected_macro, "Elixir.")}
    end
    """)

    proof = WorkflowCompilerLineage.snapshot()
    other_row = Enum.find(proof.modules, &(&1["source_path"] == "other_consumer.exs"))

    changed = %{
      proof
      | registered:
          Enum.map(proof.registered, fn row ->
            if row["module"] == test["module"],
              do: Map.put(row, "consumer_proof_id", other_row["proof_id"]),
              else: row
          end)
    }

    assert {:unresolved, _} = WorkflowCompilerLineage.match(test, changed, declarations)
  end

  test "all sequence values must be positive integers", %{root: root} do
    {test, declarations, _, _} = fixture(root, :use)
    proof = WorkflowCompilerLineage.snapshot()
    changed = change_use(proof, test, &Map.update!(&1, "sequence", fn value -> value + 0.5 end))
    assert {:unresolved, _} = WorkflowCompilerLineage.match(test, changed, declarations)
  end

  test "sequence values must be unique across all calls and module versions", %{root: root} do
    {test, declarations, _, _} = fixture(root, :use)
    proof = WorkflowCompilerLineage.snapshot()
    case_row = Enum.find(proof.calls, &(&1["name"] == "test"))
    changed = %{proof | calls: [case_row | proof.calls]}
    assert {:unresolved, _} = WorkflowCompilerLineage.match(test, changed, declarations)
  end

  test "the latest preceding actual compiled version overrides a forged current flag", %{
    root: root
  } do
    {_test, declarations, original_path, consumer} = fixture(root, :use)

    original =
      WorkflowCompilerLineage.snapshot().modules
      |> Enum.find(&(&1["source_path"] == original_path))

    section_name =
      consumer.expected_macro |> String.trim_leading("Elixir.") |> String.trim_trailing(".First")

    Code.compiler_options(ignore_module_conflict: true)
    write_compile(root, "shadow.exs", section("", section_name, :shadow, "_opts"))
    shadow_name = "ShadowActualConsumer#{System.unique_integer([:positive])}"

    write_compile(root, "shadow_consumer.exs", """
    defmodule #{shadow_name} do
      use ExUnit.Case
      use #{String.trim_leading(consumer.expected_macro, "Elixir.")}
    end
    """)

    shadow = Module.concat([shadow_name])
    [registered] = shadow.__ex_unit__().tests

    test = %{
      "source_path" => "shadow_consumer.exs",
      "source_line" => registered.tags.line,
      "module" => Atom.to_string(shadow),
      "name" => Atom.to_string(registered.name)
    }

    declarations =
      declarations ++
        [%{"id" => "shadow", "source_path" => "shadow.exs", "line" => 4, "declaration" => "same"}]

    proof = WorkflowCompilerLineage.snapshot()
    assert {:ok, origin} = WorkflowCompilerLineage.match(test, proof, declarations)
    assert origin["source_declaration_id"] == "shadow"
    assert apply(shadow, :"test same", [%{}]) == :ok
    assert_received {:compiler_lineage_body, :shadow}

    changed =
      change_use(proof, test, fn row ->
        row
        |> Map.put("target_proof_id", original["proof_id"])
        |> Map.put("target_loaded_md5", original["loaded_md5"])
        |> Map.put("current_version", true)
      end)

    assert {:unresolved, _} = WorkflowCompilerLineage.match(test, changed, declarations)
  end

  defp change_use(proof, test, edit) do
    %{
      proof
      | calls:
          Enum.map(proof.calls, fn row ->
            if row["name"] == "__using__" and row["source_path"] == test["source_path"] and
                 row["consumer_module"] == test["module"] and
                 row["source_line"] == test["source_line"], do: edit.(row), else: row
          end)
    }
  end

  for unavailable <- [:source, :debug] do
    @unavailable unavailable
    test "a newer version with unavailable #{@unavailable} retains its blocking header", %{root: root} do
      {test, declarations, path, consumer} = fixture(root, :use)
      before = WorkflowCompilerLineage.snapshot()
      original = Enum.find(before.modules, &(&1["source_path"] == path))
      section_name = consumer.expected_macro |> String.trim_leading("Elixir.") |> String.trim_trailing(".First")
      source = section("", section_name, :shadow, "_opts")
      Code.compiler_options(ignore_module_conflict: true)
      if @unavailable == :source, do: File.rm!(Path.join(root, path)), else: File.write!(Path.join(root, path), source)
      previous = Code.compiler_options(debug_info: @unavailable != :debug)
      Code.compile_string(source, Path.join(root, path))
      Code.compiler_options(previous)
      Code.compile_file(Path.join(root, "consumer.exs"))
      proof = WorkflowCompilerLineage.snapshot()
      versions = Enum.filter(proof.modules, &(&1["module"] == original["module"]))
      assert length(versions) == 2
      latest = Enum.max_by(versions, & &1["sequence"])
      assert latest["sequence"] > original["sequence"]
      assert latest["loaded_md5"] == consumer.expected_macro |> String.to_atom() |> apply(:module_info, [:md5])
      assert latest["definition_matches_debug"] == (@unavailable == :source)
      assert latest["module_definition_line"] == if(@unavailable == :source, do: 1, else: nil)
      if @unavailable == :source, do: assert(latest["source_bytes"] == nil), else: assert(latest["source_bytes"] == source)
      assert apply(consumer.module, :"test same", [%{}]) == :ok
      assert_received {:compiler_lineage_body, :shadow}
      assert {:unresolved, _} = WorkflowCompilerLineage.match(test, proof, declarations)
      changed = change_use(proof, test, fn row ->
        row |> Map.put("target_proof_id", original["proof_id"])
        |> Map.put("target_loaded_md5", original["loaded_md5"]) |> Map.put("current_version", true)
      end)
      assert {:unresolved, _} = WorkflowCompilerLineage.match(test, changed, declarations)
    end
  end

  defp fixture(root, mode, binder_name \\ "_") do
    suffix = System.unique_integer([:positive])
    first = "CompilerFirst#{suffix}"
    other = "CompilerOther#{suffix}"
    binder = "CompilerBinder#{suffix}"
    consumer_name = "CompilerConsumer#{suffix}"

    write_compile(root, "binder.exs", """
    defmodule #{binder} do
      defmacro __using__(_) do
        quote do: alias(#{other}, as: #{first})
      end
      defmacro bind() do
        quote do: alias(#{other}, as: #{first})
      end
    end
    """)

    source_prefix =
      if mode == :source_environment, do: "require #{binder}\n#{binder}.bind()\n", else: ""

    first_source = section(source_prefix, first, :first, binder_name)
    second_prefix = if mode == :source_environment, do: "alias #{first}, as: #{other}\n", else: ""
    second_source = section(second_prefix, other, :second, binder_name)
    write_compile(root, "first.exs", first_source)
    write_compile(root, "second.exs", second_source)

    inside =
      case mode do
        :use -> "use #{binder}\n"
        :call -> "require #{binder}\n#{binder}.bind()\n"
        _ -> ""
      end

    top = if mode == :top, do: "require #{binder}\n#{binder}.bind()\n", else: ""

    usage =
      if mode == :direct,
        do: "require #{other}.First\n#{other}.First.__using__([])",
        else: "use #{first}.First"

    consumer_source = """
    #{top}defmodule #{consumer_name} do
      use ExUnit.Case
      #{inside}#{usage}
    end
    """

    write_compile(root, "consumer.exs", consumer_source)
    consumer = Module.concat([consumer_name])
    [registered] = consumer.__ex_unit__().tests

    test = %{
      "source_path" => "consumer.exs",
      "source_line" => registered.tags.line,
      "module" => Atom.to_string(consumer),
      "name" => Atom.to_string(registered.name)
    }

    declarations =
      for {path, source, id} <- [
            {"first.exs", first_source, "first"},
            {"second.exs", second_source, "second"}
          ] do
        line =
          source |> String.split("\n") |> Enum.find_index(&String.contains?(&1, "test \"same\""))

        %{"id" => id, "source_path" => path, "line" => line + 1, "declaration" => "same"}
      end

    expected_macro = if mode == :source_environment, do: first, else: other

    {test, declarations, "second.exs",
     %{module: consumer, expected_macro: "Elixir.#{expected_macro}.First"}}
  end

  defp section(prefix, name, marker, binder_name) do
    """
    #{prefix}defmodule #{name}.First do
      defmacro __using__(#{binder_name}) do
        quote do
          test "same" do
            assert :#{marker} == :#{marker}
            send(self(), {:compiler_lineage_body, :#{marker}})
            :#{marker}
          end
        end
      end
    end
    """
  end

  defp delegated_fixture(root) do
    suffix = System.unique_integer([:positive])
    foreign = "DelegatingTest#{suffix}"
    section = "DelegatedSection#{suffix}"
    consumer = "DelegatedConsumer#{suffix}"

    write_compile(root, "foreign.exs", """
    defmodule #{foreign} do
      defmacro test(name, _body) do
        quote do
          require ExUnit.Case
          ExUnit.Case.test unquote(name) do
            send(self(), {:delegated_body, :foreign})
          end
        end
      end
    end
    """)

    write_compile(root, "section.exs", """
    defmodule #{section} do
      defmacro __using__(_) do
        quote do
          test "same" do
            send(self(), {:delegated_body, :source})
          end
        end
      end
    end
    """)

    write_compile(root, "consumer.exs", """
    defmodule #{consumer} do
      use ExUnit.Case
      import ExUnit.Case, only: []
      import #{foreign}, only: [test: 2]
      use #{section}
    end
    """)

    consumer = Module.concat([consumer])
    [registered] = consumer.__ex_unit__().tests

    test = %{
      "source_path" => "consumer.exs",
      "source_line" => registered.tags.line,
      "module" => Atom.to_string(consumer),
      "name" => Atom.to_string(registered.name)
    }

    declarations = [
      %{
        "id" => "literal-source",
        "source_path" => "section.exs",
        "line" => 4,
        "declaration" => "same"
      }
    ]

    {test, declarations, consumer}
  end

  defp write_compile(root, name, source) do
    path = Path.join(root, name)
    File.write!(path, source)
    Code.compile_file(path)
  end
end

result = ExUnit.run()
if result.failures != 0, do: System.halt(2)
