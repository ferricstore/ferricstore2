defmodule Ferricstore.PolicyRuntimeCaptureSourceGuardTest do
  use ExUnit.Case, async: false
  alias Ferricstore.PolicyRuntimeCapture, as: Capture
  alias Ferricstore.PolicyRuntimeCapture.Input
  alias Ferricstore.PolicyRuntimeCaptureTest, as: SourceTest
  alias Ferricstore.TermCodec

  test "capture_source_guard_rejects_unloaded_code_without_autoload" do
    with_autoloadable_source(fn unload, _context ->
      queries = %{atoms: [], exports: []}
      assert {:ok, keeper, first} = Capture.capture_epoch([], queries, nil, SourceTest.limits())

      try do
        {{result, loaded}, events} =
          with_undef_trace(keeper, fn ->
            unload.()
            result = Capture.extend_epoch(keeper, [], queries, first.identity)
            {result, :code.is_loaded(TermCodec)}
          end)

        assert result == {:error, :source_code_changed},
               "result=#{inspect(result)} loaded=#{inspect(loaded)} undef=#{length(events)}"

        assert loaded == false
        assert events == []
      after
        assert :ok = Capture.close_epoch(keeper)
      end
    end)
  end

  test "input_source_guard_rejects_unloaded_code_without_autoload" do
    with_autoloadable_source(fn unload, context ->
      donor = %{encoded: TermCodec.encode(7), source_term: 7}
      assert donor.encoded == <<131, 97, 7>>
      limits = SourceTest.limits()
      retained = %{decoded_words: 0, pinned_bytes: 0}

      {{result, loaded}, events} =
        with_undef_trace(self(), fn ->
          unload.()
          result = Input.bind_admission([donor], limits, retained, context.source, 1)
          {result, :code.is_loaded(TermCodec)}
        end)

      assert result == {:error, :source_code_changed},
             "result=#{inspect(result)} loaded=#{inspect(loaded)} undef=#{length(events)}"

      assert loaded == false
      assert events == []
    end)
  end

  defp with_autoloadable_source(run) do
    SourceTest.bootstrap!()
    context = :persistent_term.get({Capture, :verified_bootstrap})

    path =
      Path.join(
        SourceTest.config().elixir_source_root,
        "apps/ferricstore/lib/ferricstore/term_codec.ex"
      )

    bytes = File.read!(path)
    assert :crypto.hash(:sha256, bytes) == context.source.source_sha
    md5 = context.source.md5
    prior = Code.compiler_options(ignore_module_conflict: true)

    original =
      try do
        [{TermCodec, beam}] = Code.compile_string(bytes, path)
        beam
      after
        Code.compiler_options(prior)
      end

    directory =
      Path.join(
        Path.dirname(SourceTest.config().nif_path),
        "source-guard-#{System.pid()}-#{System.unique_integer([:positive, :monotonic])}"
      )

    beam_name = ~c"Elixir.Ferricstore.TermCodec.beam"
    beam_path = Path.join(directory, List.to_string(beam_name))
    code_path = String.to_charlist(directory)
    created = File.mkdir(directory)

    try do
      assert created == :ok
      assert :erlang.get_module_info(TermCodec, :md5) == md5
      assert {:ok, {TermCodec, ^md5}} = :beam_lib.md5(original)
      File.write!(beam_path, original)
      assert true == :code.add_patha(code_path)
      assert :code.where_is_file(beam_name) == String.to_charlist(beam_path)

      unload = fn ->
        :code.purge(TermCodec)
        assert :code.delete(TermCodec)
        :code.purge(TermCodec)
        assert :code.is_loaded(TermCodec) == false
        assert :code.where_is_file(beam_name) == String.to_charlist(beam_path)
      end

      run.(unload, context)
    after
      try do
        if created == :ok, do: :code.del_path(code_path)
      after
        try do
          :code.purge(TermCodec)

          assert {:module, TermCodec} =
                   :code.load_binary(TermCodec, String.to_charlist(path), original)

          assert :erlang.get_module_info(TermCodec, :md5) == md5
        after
          if created == :ok, do: File.rm_rf!(directory)
        end
      end
    end
  end

  defp with_undef_trace(target, run) do
    assert {:module, :error_handler} = :code.ensure_loaded(:error_handler)
    mfa = {:error_handler, :undefined_function, 3}
    assert {:traced, false} = :erlang.trace_info(mfa, :traced)
    assert {:flags, flags} = :erlang.trace_info(target, :flags)
    refute :call in flags
    {tracer, monitor} = spawn_monitor(fn -> take_undef_events([]) end)

    try do
      assert :erlang.trace_pattern(mfa, true, []) == 1
      assert :erlang.trace(target, true, [:call, {:tracer, tracer}]) == 1
      result = run.()
      fence = :erlang.trace_delivered(target)
      assert_receive {:trace_delivered, ^target, ^fence}, 5_000
      batch = make_ref()
      send(tracer, {:collect, self(), batch})
      assert_receive {^batch, events}, 5_000
      {result, events}
    after
      if Process.alive?(target), do: :erlang.trace(target, false, [:call])
      :erlang.trace_pattern(mfa, false, [])
      if Process.alive?(tracer), do: Process.exit(tracer, :kill)
      assert_receive {:DOWN, ^monitor, :process, ^tracer, _reason}, 5_000
    end
  end

  defp take_undef_events(events) do
    receive do
      {:trace, _, :call, {:error_handler, :undefined_function, [TermCodec, :module_info, [:md5]]}} =
          event ->
        take_undef_events([event | events])

      {:trace, _, :call, _} ->
        take_undef_events(events)

      {:collect, caller, ref} ->
        send(caller, {ref, Enum.reverse(events)})
    end
  end
end
