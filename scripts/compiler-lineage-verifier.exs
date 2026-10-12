# A fresh immutable analysis per match batch; no cached caller trust flags.
defmodule WorkflowCompilerLineageVerifier do
  alias WorkflowCompilerLineageData, as: Data
  @rule "compiler-constant-using-v1"

  def match_all(tests, proof, declarations) do
    analysis = prepare(proof, declarations)
    {Enum.map(tests, &{&1, match_case(&1, analysis)}), analysis.statistics}
  rescue
    _ -> {Enum.map(tests, &{&1, unresolved()}), %{invalid_proof: true}}
  end

  defp prepare(proof, declarations) do
    true = proof.rule == @rule
    true = length(proof.modules) <= 10_000 and length(proof.calls) <= 1_000_000
    true = valid_sequences?(proof.modules, proof.calls)
    true = Data.valid_budget?(proof.modules, proof.calls)
    content = Enum.flat_map(proof.modules, &[&1["source_bytes"], &1["compiled_module"]])
      |> Enum.reject(&is_nil/1)
    true = Data.unique_bytes([proof.framework.bytes | content]) <= Data.limits()["aggregate_content_bytes"]
    source_rows = Enum.group_by(proof.modules, & &1["source_path"])

    sources =
      proof.modules
      |> Enum.map(& &1["source_path"])
      |> Enum.filter(&portable?/1)
      |> Enum.uniq()
      |> Map.new(fn path ->
        bytes =
          if portable?(path), do: Data.read_source(Path.join(proof.root, path)), else: :unresolved

        {path,
         case bytes do
           {:ok, bytes} ->
             hash = Data.digest(bytes)
             row = Enum.find(Map.fetch!(source_rows, path), &(&1["source_bytes"] == bytes and &1["source_sha256"] == hash))
             if row, do: %{bytes: row["source_bytes"], sha256: hash}, else: nil
           _ -> nil
         end}
      end)

    true = sources |> Map.values() |> Enum.reject(&is_nil/1) |> Enum.map(& &1.bytes)
      |> Data.unique_bytes() <= Data.limits()["aggregate_content_bytes"]

    charged = Enum.reduce(proof.modules ++ proof.calls, 0, &(Data.metadata_size(&1) + &2))
    charged = charged + :erlang.external_size(Map.drop(proof.framework, [:bytes]))
    {source_analysis, charged} =
      Data.analysis_cache(Map.new(sources, fn {path, source} -> {path, {path, source}} end), charged, fn {path, source} ->
        value =
          if not is_nil(source) do
            source.bytes
            |> Data.source_analysis(path)
            |> Map.update!(:quotes, fn quotes ->
              Enum.map(quotes, &Map.put(&1, :normalized, Data.normalize(&1.body)))
            end)
          else
            %{quotes: [], uses: %{}}
          end

        value
      end)

    {beams, _charged} =
      proof.modules
      |> Enum.group_by(& &1["compiled_module_sha256"])
      |> Map.new(fn {hash, rows} -> {hash, {hash, hd(rows)["compiled_module"]}} end)
      |> Data.analysis_cache(charged, fn {hash, bytes} ->
        if Data.bounded?(bytes) and Data.digest(bytes) == hash, do: Data.beam(bytes), else: :unresolved
      end)

    modules =
      Enum.map(proof.modules, fn row ->
        validate_row(row, proof.root, sources, source_analysis, beams)
      end)

    versions =
      modules
      |> Enum.group_by(& &1.row["module"])
      |> Map.new(fn {name, rows} -> {name, Enum.sort_by(rows, & &1.row["sequence"], :desc)} end)

    %{
      modules: Enum.group_by(modules, & &1.row["proof_id"]),
      versions: versions,
      registered: Enum.group_by(proof.registered, &case_key/1),
      uses:
        proof.calls
        |> Enum.filter(
          &(&1["name"] == "__using__" and &1["arity"] == 1 and
              not is_nil(&1["target_proof_id"]))
        )
        |> Enum.group_by(&location_key/1),
      calls: Enum.group_by(proof.calls, &location_key/1),
      framework: proof.framework,
      valid_framework: valid_framework?(proof.framework),
      declarations:
        Enum.group_by(declarations, &{&1["source_path"], &1["line"], &1["declaration"]}),
      statistics: %{
        source_reads: map_size(sources),
        source_parses: Enum.count(sources, fn {_, value} -> not is_nil(value) end),
        beam_decodes: map_size(beams),
        registered_cases: length(proof.registered),
        module_versions: length(proof.modules),
        macro_events: length(proof.calls)
      }
    }
  end

  defp validate_row(row, root, sources, source_analysis, beams) do
    source = Map.get(sources, row["source_path"])

    with true <- is_integer(row["sequence"]) and row["sequence"] > 0,
         true <-
           not is_nil(source) and source.bytes == row["source_bytes"] and
             source.sha256 == row["source_sha256"],
         true <-
           Data.bounded?(row["compiled_module"]) and
             Data.digest(row["compiled_module"]) == row["compiled_module_sha256"],
         {:ok, compiled} <- Map.get(beams, row["compiled_module_sha256"]),
         true <- compiled.module == row["module"] and compiled.md5 == row["loaded_md5"],
         true <- Path.expand(compiled.file) == Path.join(root, row["source_path"]),
         true <- compiled.module_line == row["module_definition_line"],
         true <-
           compiled.definition == row["definition"] and row["definition_matches_debug"] == true,
         true <-
           row["proof_id"] ==
             Data.term_digest(
               {compiled.module, row["source_path"], row["source_sha256"],
                row["compiled_module_sha256"], row["sequence"]}
             ) do
      source_analysis = Map.fetch!(source_analysis, row["source_path"])

      quoted =
        case Data.quote_body(compiled.definition) do
          {:ok, line, body} ->
            normalized = Data.normalize(body)

            case Enum.filter(
                   source_analysis.quotes,
                   &(&1.line == line and &1.normalized == normalized)
                 ) do
              [quote] ->
                %{
                  line: line,
                  hash: Data.term_digest(normalized),
                  tests: Data.literal_tests(quote.body)
                }

              _ ->
                nil
            end

          _ ->
            nil
        end

      %{
        row: row,
        valid: true,
        quoted: quoted,
        source_uses: Map.get(source_analysis.uses, compiled.module_line, [])
      }
    else
      _ -> %{row: row, valid: false}
    end
  end

  defp match_case(test, analysis) do
    with [registered] <- Map.get(analysis.registered, case_key(test), []),
         [%{valid: true} = consumer] <-
           Map.get(analysis.modules, registered["consumer_proof_id"], []),
         true <-
           consumer.row["module"] == test["module"] and
             consumer.row["source_path"] == test["source_path"],
         true <- latest_version?(analysis, consumer.row, nil),
         [line] <- Enum.filter(consumer.source_uses, &(&1 == test["source_line"])),
         true <- line == test["source_line"],
         [use] <- Map.get(analysis.uses, location_key(test), []),
         true <- use["current_version"] == true,
         [%{valid: true, quoted: quote} = module] <-
           Map.get(analysis.modules, use["target_proof_id"], []),
         true <- not is_nil(quote),
         true <-
           use["kind"] in ["remote_macro", "imported_macro"] and
             use["target_module"] == module.row["module"],
         true <- latest_version?(analysis, module.row, use["sequence"]),
         true <- use["target_loaded_md5"] == module.row["loaded_md5"],
         true <-
           module.row["sequence"] < use["sequence"] and use["sequence"] < consumer.row["sequence"],
         [literal] <- Enum.filter(quote.tests, &(&1.name == test["name"])),
         true <- genuine_test_trace?(analysis, test, use, consumer.row),
         [declaration] <-
           Map.get(
             analysis.declarations,
             {module.row["source_path"], literal.line, literal.literal},
             []
           ) do
      {:ok,
       %{
         "source_declaration_id" => declaration["id"],
         "source_path" => module.row["source_path"],
         "source_line" => literal.line,
         "source_sha256" => module.row["source_sha256"],
         "macro_module" => module.row["module"],
         "macro_definition_line" => quote.line,
         "consumer_module" => test["module"],
         "registration_path" => test["source_path"],
         "registration_line" => test["source_line"],
         "registration_sha256" => consumer.row["source_sha256"],
         "name" => test["name"],
         "compiled_quote_sha256" => quote.hash,
         "source_quote_sha256" => quote.hash,
         "compiled_module_sha256" => module.row["compiled_module_sha256"],
         "macro_proof_id" => module.row["proof_id"],
         "consumer_proof_id" => consumer.row["proof_id"],
         "use_sequence" => use["sequence"],
         "origin_rule" => @rule
       }}
    else
      _ -> unresolved()
    end
  end

  defp genuine_test_trace?(analysis, test, use, consumer) do
    framework = analysis.framework

    calls =
      Enum.filter(
        Map.get(analysis.calls, location_key(test), []),
        &(&1["name"] in ["test", "describe", "setup", "setup_all"] and
            use["sequence"] < &1["sequence"] and &1["sequence"] < consumer["sequence"])
      )

    analysis.valid_framework and
      Enum.any?(calls, &(&1["name"] == "test" and &1["arity"] in [2, 3])) and
      Enum.all?(
        calls,
        &(&1["genuine_case"] == true and
            &1["kind"] in ["imported_macro", "remote_macro"] and
            &1["framework_sha256"] == framework.sha256 and
            &1["target_loaded_md5"] == framework.md5 and
            &1["target_module"] == "Elixir.ExUnit.Case")
      )
  end

  defp valid_framework?(framework) do
    with true <-
           Data.bounded?(framework.bytes) and Data.digest(framework.bytes) == framework.sha256,
         {:ok, compiled} <- Data.beam(framework.bytes),
         true <- compiled.module == "Elixir.ExUnit.Case" and compiled.md5 == framework.md5,
         do: true,
         else: (_ -> false)
  end

  defp latest_version?(analysis, row, before) do
    latest =
      analysis.versions
      |> Map.get(row["module"], [])
      |> Enum.find(fn version -> is_nil(before) or version.row["sequence"] < before end)

    not is_nil(latest) and latest.valid and latest.row["proof_id"] == row["proof_id"]
  end

  defp valid_sequences?(modules, calls) do
    Enum.reduce_while(modules ++ calls, MapSet.new(), fn row, used ->
      sequence = row["sequence"]

      if is_integer(sequence) and sequence > 0 and not MapSet.member?(used, sequence),
        do: {:cont, MapSet.put(used, sequence)},
        else: {:halt, :invalid}
    end) != :invalid
  end

  defp case_key(row), do: {row["source_path"], row["source_line"], row["module"], row["name"]}

  defp location_key(row),
    do: {row["source_path"], row["source_line"], row["consumer_module"] || row["module"]}

  defp portable?(path),
    do:
      is_binary(path) and Path.type(path) == :relative and
        Enum.all?(Path.split(path), &(&1 not in ["", ".", ".."]))

  defp unresolved,
    do: {:unresolved, "no unique verified compiled constant quote and genuine registration"}
end
