Code.require_file("../runtime-macro-origins.exs", __DIR__)
ExUnit.start()

defmodule WorkflowMacroOriginsTest do
  use ExUnit.Case, async: true

  @consumer_path "apps/example/test/consumer_test.exs"
  @section_path "apps/example/test/sections/literal.exs"
  @consumer """
  defmodule Example.ConsumerTest do
    @moduledoc false
    use Example.LiteralSection
  end
  """
  @section """
  defmodule Example.LiteralSection do
    defmacro __using__(_opts) do
      quote do
        test "first literal" do
          raise "this test body must never execute during recovery"
        end
        test "second literal" do
          assert true
        end
      end
    end
  end
  """

  defp declaration(line, name, id) do
    %{"source_path" => @section_path, "line" => line, "declaration" => name, "id" => id}
  end

  defp runtime(name, line \\ 3) do
    %{"source_path" => @consumer_path, "source_line" => line,
      "module" => "Elixir.Example.ConsumerTest", "name" => "test " <> name, "tags" => %{}}
  end

  defp indexes(section \\ @section, consumer \\ @consumer) do
    [WorkflowMacroOrigins.index(section, @section_path),
     WorkflowMacroOrigins.index(consumer, @consumer_path)]
  end

  test "literal macro origins preserve original declaration and exact use provenance" do
    declarations = [declaration(4, "first literal", "first"), declaration(7, "second literal", "second")]
    assert {:ok, first} = WorkflowMacroOrigins.match(runtime("first literal"), indexes(), declarations)
    assert first["source_declaration_id"] == "first"
    assert first["source_line"] == 4
    assert first["registration_line"] == 3
    assert first["macro_module"] == "Elixir.Example.LiteralSection"
    assert first["use_text"] == "use Example.LiteralSection"
    assert {:ok, second} = WorkflowMacroOrigins.match(runtime("second literal"), indexes(), declarations)
    assert second["source_declaration_id"] == "second"
    assert second["source_line"] == 7
  end

  test "literal describe contributes its actual ExUnit name" do
    section = String.replace(@section, "test \"first literal\" do", "describe \"context\" do\n      test \"first literal\" do")
      |> String.replace("    test \"second literal\"", "    end\n    test \"second literal\"")
    declarations = [declaration(5, "first literal", "described")]
    assert {:ok, origin} = WorkflowMacroOrigins.match(runtime("context first literal"), indexes(section), declarations)
    assert origin["source_declaration_id"] == "described"
    assert {:unresolved, _} = WorkflowMacroOrigins.match(runtime("first literal"), indexes(section), declarations)
  end

  test "same literal in another section cannot override the exact use location" do
    other = @section |> String.replace("Example.LiteralSection", "Example.OtherSection")
    all = indexes() ++ [WorkflowMacroOrigins.index(other, "apps/example/test/sections/other.exs")]
    assert {:ok, origin} = WorkflowMacroOrigins.match(runtime("first literal"), all,
      [declaration(4, "first literal", "correct")])
    assert origin["source_declaration_id"] == "correct"
    assert {:unresolved, _} = WorkflowMacroOrigins.match(runtime("first literal", 2), all,
      [declaration(4, "first literal", "correct")])
  end

  test "dynamic literal interpolation and conditional quote are unresolved" do
    dynamic = String.replace(@section, "\"first literal\"", "\"first \#{value}\"")
    assert {:unresolved, _} = WorkflowMacroOrigins.match(runtime("first literal"), indexes(dynamic),
      [declaration(4, "first literal", "dynamic")])
    conditional = String.replace(@section, "quote do", "if opts do\n    quote do")
      |> String.replace("\n  end\nend\n", "\n    end\n  end\nend\n")
    assert {:unresolved, _} = WorkflowMacroOrigins.match(runtime("first literal"), indexes(conditional),
      [declaration(5, "first literal", "conditional")])
  end

  test "missing duplicated or mismatched frozen declaration never guesses identity" do
    wrong = declaration(5, "first literal", "wrong-line")
    assert {:unresolved, _} = WorkflowMacroOrigins.match(runtime("first literal"), indexes(), [wrong])
    first = declaration(4, "first literal", "first")
    assert {:unresolved, _} = WorkflowMacroOrigins.match(runtime("first literal"), indexes(), [first, first])
    wrong_name = declaration(4, "different", "wrong-name")
    assert {:unresolved, _} = WorkflowMacroOrigins.match(runtime("first literal"), indexes(), [wrong_name])
  end

  test "lexical alias rebinding of the used namespace is unresolved" do
    consumer = String.replace(@consumer, "  use Example.LiteralSection", "  alias Rebound, as: Example\n  use Example.LiteralSection")
    other = String.replace(@section, "Example.LiteralSection", "Rebound.LiteralSection")
    all = indexes(@section, consumer) ++ [WorkflowMacroOrigins.index(other, "other.exs")]
    assert {:unresolved, _} = WorkflowMacroOrigins.match(runtime("first literal", 4), all,
      [declaration(4, "first literal", "wrong-original-namespace")])
  end

  test "top-level aliases cannot silently change a section or consumer module identity" do
    aliased_section = "alias Rebound, as: Example\n" <> @section
    assert {:unresolved, _} = WorkflowMacroOrigins.match(runtime("first literal"),
      indexes(aliased_section), [declaration(5, "first literal", "wrong-original-module")])
  end

  test "literal grouped aliases retain unrelated namespaces and block bound namespaces" do
    unrelated = String.replace(@consumer, "  use Example.LiteralSection", "  alias Other.{First, Second}\n  use Example.LiteralSection")
    assert {:ok, _} = WorkflowMacroOrigins.match(runtime("first literal", 4), indexes(@section, unrelated),
      [declaration(4, "first literal", "unchanged")])
    rebound = String.replace(unrelated, "Other.{First, Second}", "Other.{First, Example}")
    assert {:unresolved, _} = WorkflowMacroOrigins.match(runtime("first literal", 4), indexes(@section, rebound),
      [declaration(4, "first literal", "wrong-namespace")])
  end

  test "a preceding use cannot hide a macro-injected namespace alias" do
    consumer = String.replace(@consumer, "  use Example.LiteralSection",
      "  use AliasBinder\n  use Example.LiteralSection")
    binder = """
    defmodule AliasBinder do
      defmacro __using__(_) do
        quote do
          alias OtherRoot, as: Example
        end
      end
    end
    """
    all = indexes(@section, consumer) ++ [WorkflowMacroOrigins.index(binder, "binder.exs")]
    assert {:unresolved, _} = WorkflowMacroOrigins.match(runtime("first literal", 4), all,
      [declaration(4, "first literal", "wrong-original-namespace")])
  end

  test "preceding require and macro calls cannot hide injected namespace aliases" do
    consumer = String.replace(@consumer, "  use Example.LiteralSection",
      "  require AliasBinder\n  AliasBinder.bind()\n  use Example.LiteralSection")
    assert {:unresolved, _} = WorkflowMacroOrigins.match(runtime("first literal", 5), indexes(@section, consumer),
      [declaration(4, "first literal", "wrong-original-namespace")])
  end

  test "unknown preceding uses or module expressions cannot prove a safe namespace" do
    for previous <- ["use Unknown.Macro", "unknown_macro()", "import Unknown.Macro"] do
      consumer = String.replace(@consumer, "  use Example.LiteralSection", "  #{previous}\n  use Example.LiteralSection")
      assert {:unresolved, _} = WorkflowMacroOrigins.match(runtime("first literal", 4), indexes(@section, consumer),
        [declaration(4, "first literal", "unproved")])
    end
  end

  test "file top-level macro effects cannot be inherited as a proven namespace" do
    for previous <- ["use AliasBinder", "require AliasBinder\nAliasBinder.bind()", "import Unknown.Macro"] do
      consumer = previous <> "\n" <> @consumer
      offset = previous |> String.split("\n") |> length()
      assert {:unresolved, _} = WorkflowMacroOrigins.match(runtime("first literal", 3 + offset), indexes(@section, consumer),
        [declaration(4, "first literal", "unproved-inherited")])
    end
  end

  test "framework scaffolds file loaders and callback attributes remain unproved" do
    case_use = String.replace(@consumer, "@moduledoc false", "use ExUnit.Case")
    assert {:unresolved, _} = WorkflowMacroOrigins.match(runtime("first literal"), indexes(@section, case_use),
      [declaration(4, "first literal", "unproved-framework")])
    loader = "Code.require_file(\"helper.exs\", __DIR__)\n" <> @consumer
    assert {:unresolved, _} = WorkflowMacroOrigins.match(runtime("first literal", 4), indexes(@section, loader),
      [declaration(4, "first literal", "unproved-loader")])
    callback = String.replace(@consumer, "@moduledoc false", "@on_definition AliasBinder")
    assert {:unresolved, _} = WorkflowMacroOrigins.match(runtime("first literal"), indexes(@section, callback),
      [declaration(4, "first literal", "unproved-callback")])
  end

  test "a section definition cannot inherit an unknown file-level namespace" do
    for previous <- ["require AliasBinder\nAliasBinder.bind()", "use AliasBinder", "unknown_macro()"] do
      section = previous <> "\n" <> @section
      offset = previous |> String.split("\n") |> length()
      assert {:unresolved, _} = WorkflowMacroOrigins.match(runtime("first literal"), indexes(section),
        [declaration(4 + offset, "first literal", "unproved-source-module")])
    end
  end

  test "a section's using definition cannot follow unknown module effects" do
    section = String.replace(@section, "  defmacro __using__", "  unknown_macro()\n  defmacro __using__")
    assert {:unresolved, _} = WorkflowMacroOrigins.match(runtime("first literal"), indexes(section),
      [declaration(5, "first literal", "unproved-source-prefix")])
  end
end
