# Trusted tooling only: copied source/BEAM are inspected as data, never executed.
Code.require_file("compiler-lineage-artifact.exs", __DIR__)

defmodule WorkflowCompilerProofCheck do
  def run([root, path]) do
    case WorkflowCompilerLineageArtifact.verify(Path.expand(root), path) do
      {:ok, report} -> IO.puts(JSON.encode!(report))
      {:error, message} ->
        IO.puts(:stderr, "compiler candidate verification failed: " <> message)
        System.halt(1)
    end
  end

  def run(_) do
    IO.puts(:stderr, "usage: elixir verify-compiler-lineage.exs RUST_ROOT CAPTURE_METADATA_PATH")
    System.halt(1)
  end
end

WorkflowCompilerProofCheck.run(System.argv())
