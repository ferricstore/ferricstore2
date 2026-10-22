defmodule Ferricstore.PolicyRuntimeCaptureBudgetProtocolTest do
  use ExUnit.Case, async: false
  alias Ferricstore.PolicyRuntimeCapture, as: Capture
  alias Ferricstore.PolicyRuntimeCapture.Budget
  alias Ferricstore.PolicyRuntimeCapture.Native
  alias Ferricstore.PolicyRuntimeCaptureTest, as: SourceTest
  alias Ferricstore.TermCodec

  test "native_entry_refuses_competing_conformant_calls_without_queued_requests" do
    owner = self()
    limits = SourceTest.limits()

    with_session(limits, fn keeper, first, queries ->
      worker = fn ->
        receive do
          :enter ->
            entered =
              Stream.repeatedly(fn -> Native.budget_enter(keeper) end)
              |> Enum.find(&(&1 != :retry_registry))

            send(owner, {:entered, self(), entered})

            receive do
              :bind ->
                {:ok, resource, epoch} = entered
                ref = make_ref()
                monitor = Process.monitor(keeper)
                bound = Native.budget_bind(resource, epoch, ref)
                ticket = {keeper, resource, epoch, ref, monitor}
                send(owner, {:bound, self(), bound, ticket})

                receive do
                  :cancel -> send(owner, {:cancelled, self(), Budget.cancel(ticket)})
                end
            end
        end
      end

      with_owner(worker, fn holder ->
        send(holder, :enter)
        assert_receive {:entered, ^holder, entered}, 5_000
        assert {:ok, resource, epoch} = entered
        assert is_reference(resource) and is_integer(epoch) and epoch >= 0
        send(holder, :bind)
        assert_receive {:bound, ^holder, bound, ticket}, 5_000
        assert bound == :ok
        assert {^keeper, ^resource, ^epoch, ref, monitor} = ticket
        assert is_reference(ref) and is_reference(monitor)

        competitor = fn ->
          receive do
            :attempt ->
              one = Capture.extend_epoch(keeper, [], :invalid_queries, nil)
              two = Capture.extend_epoch(keeper, [], queries, first.identity)
              send(owner, {:competed, self(), one, two})
          end
        end

        with_owner(competitor, fn caller ->
          assert :erlang.trace(caller, true, [:send, {:tracer, owner}]) == 1
          send(caller, :attempt)
          assert_receive {:competed, ^caller, one, two}, 5_000
          assert one == {:error, :session_busy}
          assert two == {:error, :session_busy}
          events = fenced_events(caller)
          refute Enum.any?(events, &match?({:trace, ^caller, :send, _, ^keeper}, &1))
        end)

        send(holder, :cancel)
        assert_receive {:cancelled, ^holder, :ok}, 5_000
        assert {:ok, next} = Capture.extend_epoch(keeper, [], queries, first.identity)
        assert next.parent == first.identity and next.session_id == first.session_id
        assert limits == SourceTest.limits()
        assert queries == %{atoms: [], exports: []}
      end)
    end)
  end

  test "one_real_preparation_grant_blocks_competing_work_until_settlement" do
    owner = self()

    with_session(SourceTest.limits(), fn keeper, first, queries ->
      first_worker = fn ->
        receive do
          :request ->
            requested = Budget.request(keeper, first.identity)
            send(owner, {:requested, self(), requested})

            receive do
              :permission ->
                {:ok, ticket} = requested
                send(owner, {:permitted, self(), Budget.permission(ticket)})

                receive do
                  :cancel ->
                    send(owner, {:cancelled, self(), Budget.cancel(ticket)})

                    receive do
                      :old -> send(owner, {:old, self(), Budget.abandon(ticket)})
                    end
                end
            end
        end
      end

      with_owner(first_worker, fn a ->
        assert :erlang.trace(a, true, [:send, :receive, {:tracer, owner}]) == 1
        assert :erlang.trace(keeper, true, [:send, {:tracer, owner}]) == 1
        send(a, :request)
        assert_receive {:requested, ^a, requested_a}, 5_000
        assert {:ok, ticket_a} = requested_a
        assert {^keeper, resource_a, epoch_a, ref_a, _monitor_a} = ticket_a
        send(a, :permission)
        assert_receive {:permitted, ^a, permission_a}, 5_000
        assert {:ok, caps, _retained, _source} = permission_a
        assert caps == SourceTest.limits()
        fenced_events(a)
        assert :erlang.suspend_process(keeper)
        send(a, :cancel)

        assert_receive {:trace, ^a, :send, {:capture, ^a, ^ref_a, cancellation}, ^keeper},
                       5_000

        refute match?({:budget, _}, cancellation)
        assert :erlang.suspend_process(a)
        assert :erlang.resume_process(keeper)
        assert_receive {:trace, ^keeper, :send, {^ref_a, :settled}, ^a}, 5_000
        fenced_events(keeper)

        second_worker = fn ->
          requested = Budget.request(keeper, first.identity)

          granted =
            case requested do
              {:ok, ticket} -> Budget.permission(ticket)
              error -> error
            end

          send(owner, {:held, self(), requested, granted})

          receive do
            :cancel ->
              {:ok, ticket} = requested
              send(owner, {:released, self(), Budget.cancel(ticket)})
          end
        end

        with_owner(second_worker, fn b ->
          assert_receive {:held, ^b, requested_b, permission_b}, 5_000
          assert {:ok, ticket_b} = requested_b
          assert {:ok, ^caps, _, _} = permission_b
          assert {^keeper, resource_b, epoch_b, ref_b, _monitor_b} = ticket_b
          assert is_reference(resource_a) and is_reference(resource_b)
          assert epoch_b > epoch_a and ref_b != ref_a
          assert :erlang.resume_process(a)
          assert_receive {:cancelled, ^a, :ok}, 5_000
          send(a, :old)
          assert_receive {:old, ^a, :stale}, 5_000

          assert Capture.extend_epoch(keeper, [], queries, first.identity) ==
                   {:error, :session_busy}

          send(b, :cancel)
          assert_receive {:released, ^b, :ok}, 5_000
        end)

        assert {:ok, next} = Capture.extend_epoch(keeper, [], queries, first.identity)
        assert next.parent == first.identity and next.session_id == first.session_id
      end)
    end)
  end

  test "ordinary_preflight_failures_settle_before_immediate_same_parent_retry" do
    owner = self()
    limits = %{SourceTest.limits() | pinned_bytes: 128_000}
    deep = Enum.reduce(1..65, nil, fn _, term -> {term} end)

    with_session(limits, fn keeper, first, queries ->
      donor = %{encoded: TermCodec.encode(deep), source_term: deep}

      worker = fn ->
        receive do
          :attempt ->
            results = [
              Capture.extend_epoch(keeper, [], :invalid_queries, first.identity),
              Capture.extend_epoch(keeper, [], :invalid_queries, first.identity),
              Capture.extend_epoch(keeper, [donor], queries, first.identity),
              Capture.extend_epoch(keeper, [], queries, first.identity)
            ]

            send(owner, {:serial, self(), results})
        end
      end

      with_owner(worker, fn caller ->
        assert :erlang.trace(caller, true, [:send, :receive, {:tracer, owner}]) == 1
        send(caller, :attempt)
        assert_receive {:serial, ^caller, results}, 5_000

        assert [
                 {:error, :invalid_queries},
                 {:error, :invalid_queries},
                 {:error, {:limit, :depth}},
                 {:ok, next}
               ] = results

        assert next.parent == first.identity and next.session_id == first.session_id
        events = fenced_events(caller)

        requests =
          for {:trace, ^caller, :send, {:capture, ^caller, ref, {:budget, parent}}, ^keeper} <-
                events,
              do: {ref, parent}

        assert length(requests) == 4

        assert Enum.all?(requests, fn {ref, parent} ->
                 is_reference(ref) and parent == first.identity
               end)

        for [{ref, _}, {next_ref, _}] <- Enum.chunk_every(requests, 2, 1, :discard) do
          ack = Enum.find_index(events, &(&1 == {:trace, caller, :receive, {ref, :settled}}))

          sent =
            Enum.find_index(
              events,
              &match?(
                {:trace, ^caller, :send, {:capture, ^caller, ^next_ref, {:budget, _}}, ^keeper},
                &1
              )
            )

          assert is_integer(ack) and is_integer(sent) and ack < sent
        end

        wires =
          for {:trace, ^caller, :receive, {_, {:ok, _caps, retained, _source}}} <- events,
              do: retained

        assert length(wires) == 4

        for [before, after_] <- Enum.chunk_every(wires, 2, 1, :discard) do
          assert after_.decoded_words >= before.decoded_words + 11
          assert after_.pinned_bytes >= before.pinned_bytes + 88
        end

        assert Enum.reduce(1..65, deep, fn _, {term} -> term end) == nil
        assert donor == %{encoded: TermCodec.encode(deep), source_term: deep}
        assert queries == %{atoms: [], exports: []}
      end)
    end)
  end

  test "cancel_before_permission_differs_from_abandon_after_work_issue" do
    owner = self()

    with_session(SourceTest.limits(), fn keeper, first, queries ->
      pending_worker = fn ->
        receive do
          :request ->
            requested = Budget.request(keeper, first.identity)
            send(owner, {:pending, self(), requested})

            receive do
              :cancel ->
                {:ok, ticket} = requested
                send(owner, {:cancelled, self(), Budget.cancel(ticket)})
            end
        end
      end

      with_owner(pending_worker, fn caller ->
        assert :erlang.suspend_process(keeper)
        assert :erlang.trace(caller, true, [:send, {:tracer, owner}]) == 1
        send(caller, :request)
        assert_receive {:pending, ^caller, requested}, 5_000
        assert {:ok, ticket} = requested
        assert {^keeper, resource, epoch, ref, _monitor} = ticket
        assert is_reference(resource) and is_integer(epoch)
        events = fenced_events(caller)

        assert Enum.any?(
                 events,
                 &(&1 ==
                     {:trace, caller, :send, {:capture, caller, ref, {:budget, first.identity}},
                      keeper})
               )

        send(caller, :cancel)

        assert_receive {:trace, ^caller, :send, {:capture, ^caller, ^ref, cancellation}, ^keeper},
                       5_000

        refute match?({:budget, _}, cancellation)
        assert :erlang.resume_process(keeper)
        assert_receive {:cancelled, ^caller, :ok}, 5_000
      end)

      assert {:ok, next} = Capture.extend_epoch(keeper, [], queries, first.identity)
      monitor = Process.monitor(keeper)

      issued_worker = fn ->
        requested = Budget.request(keeper, next.identity)

        {granted, issued} =
          case requested do
            {:ok, ticket} ->
              permission = Budget.permission(ticket)

              case permission do
                {:ok, _, _, _} -> {permission, Budget.issue(ticket)}
                error -> {error, error}
              end

            error ->
              {error, error}
          end

        send(owner, {:issued, self(), requested, granted, issued})

        receive do
          :abandon ->
            {:ok, ticket} = requested
            send(owner, {:abandoned, self(), Budget.abandon(ticket)})
        end
      end

      with_owner(issued_worker, fn caller ->
        assert_receive {:issued, ^caller, requested, granted, issued}, 5_000
        assert {:ok, ticket} = requested
        assert {:ok, caps, _, _} = granted
        assert issued == :ok
        assert {^keeper, resource, epoch, ref, _monitor} = ticket
        assert is_reference(resource) and is_integer(epoch) and is_reference(ref)
        assert caps == SourceTest.limits()
        send(caller, :abandon)
        assert_receive {:abandoned, ^caller, :ok}, 5_000
        assert_receive {:DOWN, ^monitor, :process, ^keeper, _reason}, 5_000

        assert Capture.extend_epoch(keeper, [], queries, next.identity) ==
                 {:error, :closed_session}
      end)

      assert queries == %{atoms: [], exports: []}
    end)
  end

  test "late_anchor_mismatch_keeps_issued_encoder_credit" do
    owner = self()
    source = :binary.copy("x", 65_536)

    with_session(SourceTest.limits(), fn keeper, first, queries ->
      expected = TermCodec.encode(source)
      wrong = TermCodec.encode(:binary.copy("y", 65_536))
      assert byte_size(expected) == 65_542 and byte_size(wrong) == 65_542
      assert expected != wrong and :binary.referenced_byte_size(source) == 65_536
      donor = %{encoded: wrong, source_term: source}

      worker = fn ->
        receive do
          :attempt ->
            rejected =
              Capture.extend_epoch(
                keeper,
                [TermCodec.encode(nil), donor],
                queries,
                first.identity
              )

            retried = Capture.extend_epoch(keeper, [], queries, first.identity)
            send(owner, {:binding, self(), rejected, retried})
        end
      end

      with_owner(worker, fn caller ->
        assert :erlang.trace(keeper, true, [:send, {:tracer, owner}]) == 1
        send(caller, :attempt)
        assert_receive {:binding, ^caller, rejected, retried}, 5_000
        events = fenced_events(keeper)
        assert rejected == {:error, {:source_anchor_mismatch, 1}}
        assert {:ok, next} = retried
        assert next.parent == first.identity and next.session_id == first.session_id

        wires =
          for {:trace, ^keeper, :send, wire = {_, {:ok, _, _, _}}, ^caller} <- events, do: wire

        assert [{ref1, {:ok, _, before, _}}, {ref2, {:ok, _, after_, _}}] = wires
        assert is_reference(ref1) and is_reference(ref2) and ref1 != ref2
        # Pinned T3/Iw30/U65542/Ecap68611: Y2243, H17956; pins >=8H+2Ecap.
        assert after_.decoded_words >= before.decoded_words + 17_956
        assert after_.pinned_bytes >= before.pinned_bytes + 280_870
        assert next.donor_hashes == first.donor_hashes
        assert next.query_hash == :crypto.hash(:sha256, TermCodec.encode(queries))
        assert donor == %{encoded: wrong, source_term: source}
        assert byte_size(source) == 65_536 and :binary.referenced_byte_size(source) == 65_536
        assert expected == TermCodec.encode(source)
      end)
    end)
  end

  test "admitted_parent_source_and_consumptive_exhaustion_keep_origins" do
    defaults = SourceTest.limits()

    with_session(defaults, fn keeper, first, queries ->
      assert Capture.extend_epoch(:not_a_pid, [], queries, first.identity) ==
               {:error, :invalid_session}

      {dead, monitor} = spawn_monitor(fn -> :ok end)
      assert_receive {:DOWN, ^monitor, :process, ^dead, :normal}, 5_000

      assert Capture.extend_epoch(dead, [], queries, first.identity) ==
               {:error, :closed_session}

      assert Capture.extend_epoch(keeper, [], :invalid_queries, nil) ==
               {:error, :parent_mismatch}

      with_changed_source(fn ->
        assert Capture.extend_epoch(keeper, [], :invalid_queries, nil) ==
                 {:error, :parent_mismatch}

        assert Capture.extend_epoch(keeper, [], :invalid_queries, first.identity) ==
                 {:error, :source_code_changed}
      end)
    end)

    limits = %{defaults | pinned_bytes: 128_000}

    with_session(limits, fn keeper, first, queries ->
      monitor = Process.monitor(keeper)
      # Every actual success wire owns >=11W/88P, independent of a returned tariff.
      attempts = div(128_000, 88) + 1

      {count, terminal} =
        Enum.reduce_while(1..attempts, {0, nil}, fn _, {seen, _} ->
          case Capture.extend_epoch(keeper, [], :invalid_queries, first.identity) do
            {:error, :invalid_queries} -> {:cont, {seen + 1, nil}}
            other -> {:halt, {seen, other}}
          end
        end)

      assert terminal == {:error, {:limit, :pinned_bytes}}, inspect({count, terminal})
      assert count < attempts
      assert_receive {:DOWN, ^monitor, :process, ^keeper, _reason}, 5_000

      assert Capture.extend_epoch(keeper, [], queries, first.identity) ==
               {:error, :closed_session}

      assert limits == %{defaults | pinned_bytes: 128_000}
      assert queries == %{atoms: [], exports: []}
    end)

    for expected_error <- [:parent_mismatch, :source_code_changed] do
      with_session(limits, fn keeper, first, queries ->
        monitor = Process.monitor(keeper)

        reached_terminal =
          Enum.reduce_while(1..(div(128_000, 88) + 1), false, fn _, _ ->
            entered =
              Stream.repeatedly(fn -> Native.budget_enter(keeper) end)
              |> Enum.find(&(&1 != :retry_registry))

            assert {phase, resource, epoch} = entered
            assert phase in [:ok, :terminal]
            ref = make_ref()
            request_monitor = Process.monitor(keeper)
            assert :ok = Native.budget_bind(resource, epoch, ref)
            ticket = {keeper, resource, epoch, ref, request_monitor}

            if phase == :ok do
              send(keeper, {:capture, self(), ref, {:budget, first.identity}})
              assert {:ok, ^limits, _, _} = Budget.permission(ticket)
              assert :ok = Budget.cancel(ticket)
              {:cont, false}
            else
              result =
                with_changed_source(fn ->
                  parent = if expected_error == :parent_mismatch, do: nil, else: first.identity
                  send(keeper, {:capture, self(), ref, {:budget, parent}})
                  Budget.permission(ticket)
                end)

              assert result == {:error, expected_error}
              assert_receive {:DOWN, ^monitor, :process, ^keeper, _reason}, 5_000

              assert Capture.extend_epoch(keeper, [], queries, first.identity) ==
                       {:error, :closed_session}

              {:halt, true}
            end
          end)

        assert reached_terminal
      end)
    end
  end

  defp with_session(limits, run) do
    SourceTest.bootstrap!()
    queries = %{atoms: [], exports: []}
    assert {:ok, keeper, first} = Capture.capture_epoch([], queries, nil, limits)

    try do
      run.(keeper, first, queries)
    after
      if Process.alive?(keeper) do
        if Process.info(keeper, :status) == {:status, :suspended},
          do: :erlang.resume_process(keeper)

        :erlang.trace(keeper, false, [:send, :receive])
      end

      assert :ok = Capture.close_epoch(keeper)
    end
  end

  defp with_owner(worker, run) do
    {caller, monitor} = spawn_monitor(worker)

    try do
      run.(caller)
    after
      if Process.alive?(caller) do
        if Process.info(caller, :status) == {:status, :suspended},
          do: :erlang.resume_process(caller)

        :erlang.trace(caller, false, [:send, :receive])
      end

      Process.exit(caller, :kill)
      assert_receive {:DOWN, ^monitor, :process, ^caller, _reason}, 5_000
    end
  end

  defp fenced_events(tracee) do
    fence = :erlang.trace_delivered(tracee)
    assert_receive {:trace_delivered, ^tracee, ^fence}, 5_000
    take_events(tracee, [])
  end

  defp take_events(tracee, acc) do
    receive do
      {:trace, ^tracee, :send, _message, _to} = event -> take_events(tracee, [event | acc])
      {:trace, ^tracee, :receive, _message} = event -> take_events(tracee, [event | acc])
    after
      0 -> Enum.reverse(acc)
    end
  end

  defp with_changed_source(run) do
    path =
      Path.join(
        SourceTest.config().elixir_source_root,
        "apps/ferricstore/lib/ferricstore/term_codec.ex"
      )

    before = Code.compiler_options(ignore_module_conflict: true)
    [{TermCodec, original}] = Code.compile_file(path)

    try do
      Code.compile_quoted(
        quote do
          defmodule Ferricstore.TermCodec do
            def decode(_), do: raise("changed source decoder must not run")
          end
        end
      )

      run.()
    after
      :code.purge(TermCodec)
      {:module, TermCodec} = :code.load_binary(TermCodec, String.to_charlist(path), original)
      Code.compiler_options(before)
    end
  end
end
