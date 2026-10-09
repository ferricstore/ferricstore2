# Source AST inspection only: never expand/evaluate a macro, test, setup or loop.
defmodule WorkflowMacroOrigins do
  def index(source, path) do
    ast = Code.string_to_quoted!(source, file: path)
    lines = String.split(source, "\n")

    nodes = block(ast)
    # Top-level aliases can rebind a module declaration itself. This restricted
    # rule does not resolve that lexical environment; retain it as unresolved.
    nodes = if Enum.any?(nodes, &match?({:alias, _, _}, &1)), do: [], else: nodes

    {result, _prefix} = Enum.reduce(nodes, {%{sections: [], uses: []}, []}, fn node, {result, prefix} ->
      result = case node do
        {:defmodule, _, [alias_ast, [do: body]]} ->
          case module_name(alias_ast) do
            nil -> result
            module -> module_index(block(body), module, path, source, lines, prefix, result)
          end
        _ -> result
      end
      {result, prefix ++ [node]}
    end)
    result
  end

  def match(test, indexes, declarations) do
    uses = indexes |> Enum.flat_map(& &1.uses)
    sections = indexes |> Enum.flat_map(& &1.sections)

    with [use_origin] <- Enum.filter(uses, &matches_use?(&1, test)),
         [section] <- Enum.filter(sections, &(&1.module == use_origin.macro_module)),
         [literal] <- Enum.filter(section.tests, &("test " <> &1.name == test["name"])),
         [declaration] <- Enum.filter(declarations, &matches_declaration?(&1, section, literal)) do
      {:ok,
       %{
         "source_declaration_id" => declaration["id"],
         "source_path" => section.path,
         "source_line" => literal.line,
         "source_sha256" => section.sha256,
         "source_text" => literal.source_text,
         "registration_path" => use_origin.path,
         "registration_line" => use_origin.line,
         "registration_sha256" => use_origin.sha256,
         "use_text" => use_origin.use_text,
         "macro_module" => section.module,
         "macro_definition_line" => section.definition_line,
         "consumer_module" => use_origin.consumer_module,
         "name" => test["name"]
       }}
    else
      _ -> {:unresolved, "no unique literal declaration in the exact used section"}
    end
  end

  defp matches_use?(origin, test) do
    origin.path == test["source_path"] and origin.line == test["source_line"] and
      origin.consumer_module == test["module"]
  end

  defp matches_declaration?(declaration, section, literal) do
    declaration["source_path"] == section.path and declaration["line"] == literal.line and
      declaration["declaration"] == literal.literal
  end

  defp module_index(nodes, module, path, source, lines, inherited, result) do
    bindings = nodes |> Enum.filter(&match?({:alias, _, _}, &1)) |> Enum.flat_map(&alias_bindings/1)
    {result, _prefix} = Enum.reduce(nodes, {result, []}, fn node, {result, prefix} ->
      result = case node do
        {:use, meta, [alias_ast]} ->
          target = if rebound?(alias_ast, bindings) or not safe_prefix?(inherited ++ prefix),
            do: nil, else: module_name(alias_ast)
          case target do
            nil -> result
            target ->
              use_origin = %{path: path, line: meta[:line], sha256: digest(source),
                consumer_module: module, macro_module: target,
                use_text: line_text(lines, meta[:line])}
              %{result | uses: [use_origin | result.uses]}
          end
        {:defmacro, meta, [{:__using__, _, [_argument]}, [do: {:quote, _, [[do: body]]}]]} ->
          if safe_prefix?(inherited ++ prefix) do
            section = %{module: module, path: path, sha256: digest(source),
              definition_line: meta[:line], tests: literal_tests(block(body), "", lines)}
            %{result | sections: [section | result.sections]}
          else
            result
          end
        _ -> result
      end
      {result, prefix ++ [node]}
    end)
    result
  end

  # No external scaffold/macro proof is admitted. A preceding use, require,
  # import, call, DSL macro, nested module or dynamic form is therefore unknown.
  # This applies to inherited file nodes as well as the module's own prefix.
  defp safe_prefix?(nodes), do: Enum.all?(nodes, &inert_node?/1)
  defp inert_node?({:alias, _, _} = node), do: :unresolved_alias not in alias_bindings(node)
  defp inert_node?({:@, _, [{name, _, [value]}]})
       when name in [:moduledoc, :tag, :moduletag, :describetag, :timeout],
       do: Macro.quoted_literal?(value)
  defp inert_node?({kind, _, [head, [do: _body]]}) when kind in [:def, :defp],
    do: static_function_head?(head)
  defp inert_node?(_), do: false
  defp static_function_head?({name, _, args}) when is_atom(name) and name not in [:unquote, :use] do
    is_nil(args) or (is_list(args) and Enum.all?(args, fn
      {variable, _, context} when is_atom(variable) and is_atom(context) -> true
      literal -> Macro.quoted_literal?(literal)
    end))
  end
  defp static_function_head?(_), do: false

  defp literal_tests(nodes, describe, lines) do
    Enum.flat_map(nodes, fn
      {:test, meta, [name | args]} when is_binary(name) ->
        if Keyword.keyword?(List.last(args)) and Keyword.has_key?(List.last(args), :do) do
          [%{name: describe <> name, literal: name, line: meta[:line],
            source_text: line_text(lines, meta[:line])}]
        else
          []
        end

      {:describe, _, [name, [do: body]]} when is_binary(name) and describe == "" ->
        literal_tests(block(body), name <> " ", lines)

      _ -> []
    end)
  end

  defp module_name({:__aliases__, _, parts}) do
    parts = if hd(parts) == Elixir, do: tl(parts), else: parts
    if parts != [] and Enum.all?(parts, &is_atom/1), do: "Elixir." <> Enum.join(parts, ".")
  end

  defp module_name(_), do: nil

  defp alias_bindings({:alias, _, [target | options]}) do
    ast = case options do
      [] -> target
      [[as: alias_ast]] -> alias_ast
      [[warn: warn]] when is_boolean(warn) -> target
      _ -> nil
    end
    case ast do
      {:__aliases__, _, parts} -> [List.last(parts)]
      {{:., _, [{:__aliases__, _, _}, :{}]}, _, members} ->
        Enum.flat_map(members, fn
          {:__aliases__, _, parts} -> [List.last(parts)]
          _ -> [:unresolved_alias]
        end)
      _ -> [:unresolved_alias]
    end
  end

  defp rebound?({:__aliases__, _, [Elixir | _]}, _bindings), do: false
  defp rebound?({:__aliases__, _, [first | _]}, bindings),
    do: :unresolved_alias in bindings or first in bindings
  defp rebound?(_, _bindings), do: true

  defp block({:__block__, _, items}), do: items
  defp block(item), do: [item]
  defp line_text(lines, line), do: lines |> Enum.at(line - 1) |> String.trim()
  defp digest(source), do: :crypto.hash(:sha256, source) |> Base.encode16(case: :lower)
end
