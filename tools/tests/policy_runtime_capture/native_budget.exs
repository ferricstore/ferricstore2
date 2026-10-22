defmodule Ferricstore.PolicyRuntimeCaptureNativeBudgetTest do
  use ExUnit.Case, async: false
  alias Ferricstore.PolicyRuntimeCapture.Native
  alias Ferricstore.PolicyRuntimeCaptureTest, as: SourceTest

  # Actors and their RPC storage are fixture owners outside the native bank.
  test "native_entry_binds_once_and_enforces_keeper_owner_ref_authority" do
    limits = SourceTest.limits()
    SourceTest.bootstrap!()

    with_native_actor(fn empty ->
      assert native_call(empty, fn -> Native.budget_register(0, 0, 0, 0) end) ==
               :decoded_words

      assert native_call(empty, fn ->
               Native.budget_register(limits.decoded_words, 0, 0, 0)
             end) == :pinned_bytes
    end)

    with_native_keeper(limits, fn keeper, resource, {registered_w, registered_p} ->
      assert native_call(keeper, fn ->
               Native.budget_register(
                 limits.decoded_words,
                 limits.pinned_bytes,
                 registered_w,
                 registered_p
               )
             end) == :session_busy

      with_native_actor(fn owner ->
        {entered, ref} =
          native_call(owner, fn ->
            entered =
              Stream.repeatedly(fn -> Native.budget_enter(keeper) end)
              |> Enum.find(&(&1 != :retry_registry))

            {entered, make_ref()}
          end)

        assert {:ok, ^resource, epoch} = entered
        assert is_integer(epoch) and epoch >= 0 and is_reference(ref)

        with_native_actor(fn other ->
          wrong_ref = native_call(other, fn -> make_ref() end)

          assert native_call(other, fn ->
                   Stream.repeatedly(fn -> Native.budget_enter(keeper) end)
                   |> Enum.find(&(&1 != :retry_registry))
                 end) == :session_busy

          assert native_call(owner, fn ->
                   Stream.repeatedly(fn -> Native.budget_enter(other) end)
                   |> Enum.find(&(&1 != :retry_registry))
                 end) == :unmanaged

          assert native_call(other, fn -> Native.budget_bind(resource, epoch, wrong_ref) end) ==
                   :stale

          assert native_call(keeper, fn -> Native.budget_bind(resource, epoch, wrong_ref) end) ==
                   :stale

          assert native_call(owner, fn -> Native.budget_bind(resource, epoch + 1, ref) end) ==
                   :stale

          assert native_call(owner, fn -> Native.budget_bind(resource, epoch, ref) end) == :ok

          assert native_call(owner, fn -> Native.budget_bind(resource, epoch, wrong_ref) end) ==
                   :stale

          assert native_call(owner, fn -> Native.budget_read(resource, owner, ref) end) == :stale
          assert native_call(keeper, fn -> Native.budget_read(resource, other, ref) end) == :stale

          assert native_call(keeper, fn -> Native.budget_read(resource, owner, wrong_ref) end) ==
                   :stale

          assert {:ok, ^epoch, words, pins} =
                   native_call(keeper, fn -> Native.budget_read(resource, owner, ref) end)

          assert words in registered_w..limits.decoded_words
          assert pins in registered_p..limits.pinned_bytes

          assert native_call(owner, fn ->
                   Native.budget_control(
                     resource,
                     epoch,
                     :reserve,
                     {words, pins, limits.decoded_words - words, limits.pinned_bytes - pins,
                      :offer}
                   )
                 end) == :stale

          assert native_call(keeper, fn ->
                   Native.budget_control(resource, epoch, :begin, nil)
                 end) == :stale
        end)
      end)
    end)
  end

  test "native_repeated_reads_charge_credit_and_keep_terminal_report_single_use" do
    limits = %{SourceTest.limits() | pinned_bytes: 128_000}

    with_native_keeper(limits, fn keeper, resource, _registered ->
      with_native_actor(fn owner ->
        {entered, ref} =
          native_call(owner, fn ->
            entered =
              Stream.repeatedly(fn -> Native.budget_enter(keeper) end)
              |> Enum.find(&(&1 != :retry_registry))

            {entered, make_ref()}
          end)

        assert {:ok, ^resource, epoch} = entered
        assert native_call(owner, fn -> Native.budget_bind(resource, epoch, ref) end) == :ok

        # Each real report consumes at least five words and forty pin bytes.
        {last, denied, count} =
          Enum.reduce_while(1..(div(128_000, 40) + 1), {nil, nil, 0}, fn _, {last, _, count} ->
            result = native_call(keeper, fn -> Native.budget_read(resource, owner, ref) end)

            case result do
              {:ok, ^epoch, words, pins} ->
                if last do
                  {before_w, before_p} = last
                  assert words >= before_w + 5
                  assert pins >= before_p + 40
                end

                assert words <= limits.decoded_words and pins <= limits.pinned_bytes
                {:cont, {{words, pins}, nil, count + 1}}

              :pinned_bytes ->
                {:halt, {last, :pinned_bytes, count}}

              other ->
                flunk("unexpected read outcome: #{inspect(other)}")
            end
          end)

        assert denied == :pinned_bytes and count > 1
        {words, pins} = last

        assert native_call(owner, fn -> Native.budget_control(resource, epoch, :cancel, nil) end) ==
                 :ok

        assert native_call(keeper, fn ->
                 Native.budget_control(resource, epoch, :settle, {words, pins, :retry})
               end) == :ok

        {terminal, terminal_ref} =
          native_call(owner, fn ->
            terminal =
              Stream.repeatedly(fn -> Native.budget_enter(keeper) end)
              |> Enum.find(&(&1 != :retry_registry))

            {terminal, make_ref()}
          end)

        assert {:terminal, ^resource, terminal_epoch} = terminal
        assert terminal_epoch > epoch

        assert native_call(owner, fn ->
                 Native.budget_bind(resource, terminal_epoch, terminal_ref)
               end) == :ok

        assert {:ok, ^terminal_epoch, terminal_w, terminal_p} =
                 native_call(keeper, fn -> Native.budget_read(resource, owner, terminal_ref) end)

        assert terminal_w >= words + 15 and terminal_w <= limits.decoded_words
        assert terminal_p >= pins + 96 and terminal_p <= limits.pinned_bytes

        assert native_call(keeper, fn -> Native.budget_read(resource, owner, terminal_ref) end) ==
                 :stale

        assert native_call(keeper, fn ->
                 Native.budget_control(
                   resource,
                   terminal_epoch,
                   :reserve,
                   {terminal_w, terminal_p, 0, 0, :control}
                 )
               end) == :pinned_bytes

        assert native_call(keeper, fn ->
                 Native.budget_control(resource, terminal_epoch, :close, nil)
               end) == :ok

        assert native_call(owner, fn ->
                 Stream.repeatedly(fn -> Native.budget_enter(keeper) end)
                 |> Enum.find(&(&1 != :retry_registry))
               end) == :closed
      end)
    end)
  end

  test "native_cancel_clears_before_new_epoch_and_rejects_old_actions" do
    limits = SourceTest.limits()

    with_native_keeper(limits, fn keeper, resource, _registered ->
      with_native_actor(fn a ->
        {entered_a, ref_a} =
          native_call(a, fn ->
            entered =
              Stream.repeatedly(fn -> Native.budget_enter(keeper) end)
              |> Enum.find(&(&1 != :retry_registry))

            {entered, make_ref()}
          end)

        assert {:ok, ^resource, epoch_a} = entered_a
        assert native_call(a, fn -> Native.budget_bind(resource, epoch_a, ref_a) end) == :ok

        assert native_call(a, fn -> Native.budget_control(resource, epoch_a, :cancel, nil) end) ==
                 :ok

        with_native_actor(fn b ->
          assert native_call(b, fn ->
                   Stream.repeatedly(fn -> Native.budget_enter(keeper) end)
                   |> Enum.find(&(&1 != :retry_registry))
                 end) == :session_busy

          assert {:ok, ^epoch_a, words, pins} =
                   native_call(keeper, fn -> Native.budget_read(resource, a, ref_a) end)

          assert native_call(keeper, fn ->
                   Native.budget_control(
                     resource,
                     epoch_a,
                     :reserve,
                     {words, pins, limits.decoded_words - words, limits.pinned_bytes - pins,
                      :offer}
                   )
                 end) == :cancelled

          assert native_call(keeper, fn ->
                   Native.budget_control(resource, epoch_a, :settle, {words, pins, :retry})
                 end) == :ok

          {entered_b, ref_b} =
            native_call(b, fn ->
              entered =
                Stream.repeatedly(fn -> Native.budget_enter(keeper) end)
                |> Enum.find(&(&1 != :retry_registry))

              {entered, make_ref()}
            end)

          assert {:ok, ^resource, epoch_b} = entered_b
          assert epoch_b > epoch_a and ref_b != ref_a
          assert native_call(b, fn -> Native.budget_bind(resource, epoch_b, ref_b) end) == :ok

          assert {:ok, ^epoch_b, _, _} =
                   native_call(keeper, fn -> Native.budget_read(resource, b, ref_b) end)

          assert native_call(a, fn -> Native.budget_bind(resource, epoch_a, ref_a) end) == :stale

          for action <- [:begin, :cancel, :abandon] do
            assert native_call(a, fn ->
                     Native.budget_control(resource, epoch_a, action, nil)
                   end) == :stale
          end

          assert native_call(keeper, fn -> Native.budget_read(resource, a, ref_a) end) == :stale

          assert {:ok, ^epoch_b, _, _} =
                   native_call(keeper, fn -> Native.budget_read(resource, b, ref_b) end)
        end)
      end)
    end)
  end

  test "native_owner_down_closes_issued_rights_and_preserves_tombstone" do
    limits = SourceTest.limits()

    with_native_keeper(limits, fn keeper, resource, _registered ->
      with_native_actor(fn owner ->
        {entered, ref} =
          native_call(owner, fn ->
            entered =
              Stream.repeatedly(fn -> Native.budget_enter(keeper) end)
              |> Enum.find(&(&1 != :retry_registry))

            {entered, make_ref()}
          end)

        assert {:ok, ^resource, epoch} = entered
        assert native_call(owner, fn -> Native.budget_bind(resource, epoch, ref) end) == :ok

        assert {:ok, ^epoch, words, pins} =
                 native_call(keeper, fn -> Native.budget_read(resource, owner, ref) end)

        assert native_call(keeper, fn ->
                 Native.budget_control(
                   resource,
                   epoch,
                   :reserve,
                   {words, pins, limits.decoded_words - words, limits.pinned_bytes - pins, :offer}
                 )
               end) == :ok

        assert native_call(owner, fn -> Native.budget_control(resource, epoch, :begin, nil) end) ==
                 :ok

        observer = Process.monitor(owner)
        Process.exit(owner, :kill)
        assert_receive {:DOWN, ^observer, :process, ^owner, _reason}, 5_000

        assert native_call(keeper, fn ->
                 receive do
                   {:budget_closed, closed_resource} -> closed_resource
                 after
                   5_000 -> :notification_timeout
                 end
               end) == resource

        assert Process.alive?(keeper)

        assert native_call(keeper, fn ->
                 Stream.repeatedly(fn -> Native.budget_enter(self()) end)
                 |> Enum.find(&(&1 != :retry_registry))
               end) == :closed

        assert native_call(keeper, fn -> Native.budget_read(resource, owner, ref) end) == :closed

        assert native_call(keeper, fn ->
                 Native.budget_register(limits.decoded_words, limits.pinned_bytes, words, pins)
               end) == :closed
      end)
    end)
  end

  test "native_abandon_and_keeper_close_keep_closed_registration_authority" do
    limits = SourceTest.limits()

    for action <- [:abandon, :close] do
      with_native_keeper(limits, fn keeper, resource, _registered ->
        with_native_actor(fn owner ->
          {entered, ref} =
            native_call(owner, fn ->
              entered =
                Stream.repeatedly(fn -> Native.budget_enter(keeper) end)
                |> Enum.find(&(&1 != :retry_registry))

              {entered, make_ref()}
            end)

          assert {:ok, ^resource, epoch} = entered
          assert native_call(owner, fn -> Native.budget_bind(resource, epoch, ref) end) == :ok

          assert {:ok, ^epoch, words, pins} =
                   native_call(keeper, fn -> Native.budget_read(resource, owner, ref) end)

          assert native_call(keeper, fn ->
                   Native.budget_control(
                     resource,
                     epoch,
                     :reserve,
                     {words, pins, limits.decoded_words - words, limits.pinned_bytes - pins,
                      :offer}
                   )
                 end) == :ok

          assert native_call(owner, fn ->
                   Native.budget_control(resource, epoch, :begin, nil)
                 end) == :ok

          authority = if action == :abandon, do: owner, else: keeper

          assert native_call(authority, fn ->
                   Native.budget_control(resource, epoch, action, nil)
                 end) == :ok

          if action == :abandon do
            assert native_call(keeper, fn ->
                     receive do
                       {:budget_closed, closed_resource} -> closed_resource
                     after
                       5_000 -> :notification_timeout
                     end
                   end) == resource
          end

          assert Process.alive?(keeper)

          assert native_call(owner, fn ->
                   Stream.repeatedly(fn -> Native.budget_enter(keeper) end)
                   |> Enum.find(&(&1 != :retry_registry))
                 end) == :closed

          assert native_call(owner, fn -> Native.budget_bind(resource, epoch, ref) end) == :closed

          assert native_call(keeper, fn ->
                   Native.budget_register(limits.decoded_words, limits.pinned_bytes, words, pins)
                 end) == :closed

          assert native_call(keeper, fn ->
                   Native.budget_control(resource, epoch, :close, nil)
                 end) == :closed
        end)
      end)
    end
  end

  test "native_keeper_down_cannot_close_another_live_registration" do
    limits = SourceTest.limits()

    with_native_keeper(limits, fn keeper_a, resource_a, _registered_a ->
      with_native_keeper(limits, fn keeper_b, resource_b, _registered_b ->
        with_native_actor(fn owner_a ->
          {entered_a, ref_a} =
            native_call(owner_a, fn ->
              entered =
                Stream.repeatedly(fn -> Native.budget_enter(keeper_a) end)
                |> Enum.find(&(&1 != :retry_registry))

              {entered, make_ref()}
            end)

          assert {:ok, ^resource_a, epoch_a} = entered_a

          assert native_call(owner_a, fn ->
                   Native.budget_bind(resource_a, epoch_a, ref_a)
                 end) == :ok

          with_native_actor(fn owner_b ->
            {entered_b, ref_b} =
              native_call(owner_b, fn ->
                entered =
                  Stream.repeatedly(fn -> Native.budget_enter(keeper_b) end)
                  |> Enum.find(&(&1 != :retry_registry))

                {entered, make_ref()}
              end)

            assert {:ok, ^resource_b, epoch_b} = entered_b

            assert native_call(owner_b, fn ->
                     Native.budget_bind(resource_b, epoch_b, ref_b)
                   end) == :ok

            assert {:ok, ^epoch_b, before_w, before_p} =
                     native_call(keeper_b, fn ->
                       Native.budget_read(resource_b, owner_b, ref_b)
                     end)

            observer = Process.monitor(keeper_a)
            Process.exit(keeper_a, :kill)
            assert_receive {:DOWN, ^observer, :process, ^keeper_a, _reason}, 5_000

            assert native_call(owner_a, fn ->
                     Stream.repeatedly(fn -> Native.budget_bind(resource_a, epoch_a, ref_a) end)
                     |> Enum.find(&(&1 != :stale))
                   end) == :closed

            assert native_call(owner_a, fn ->
                     Stream.repeatedly(fn -> Native.budget_enter(keeper_a) end)
                     |> Enum.find(&(&1 != :retry_registry))
                   end) == :closed

            assert Process.alive?(keeper_b)

            assert {:ok, ^epoch_b, after_w, after_p} =
                     native_call(keeper_b, fn ->
                       Native.budget_read(resource_b, owner_b, ref_b)
                     end)

            assert after_w >= before_w + 5 and after_p >= before_p + 40

            assert native_call(owner_b, fn ->
                     Native.budget_control(resource_b, epoch_b, :cancel, nil)
                   end) == :ok

            assert {:ok, ^epoch_b, words, pins} =
                     native_call(keeper_b, fn ->
                       Native.budget_read(resource_b, owner_b, ref_b)
                     end)

            assert native_call(keeper_b, fn ->
                     Native.budget_control(resource_b, epoch_b, :settle, {words, pins, :retry})
                   end) == :ok
          end)
        end)
      end)
    end)
  end

  defp with_native_keeper(limits, run) do
    SourceTest.bootstrap!()

    with_native_actor(fn keeper ->
      registered =
        native_call(keeper, fn ->
          Native.budget_register(limits.decoded_words, limits.pinned_bytes, 0, 0)
        end)

      assert {:ok, resource, words, pins} = registered
      assert is_reference(resource) and is_integer(words) and is_integer(pins)
      assert words in 0..limits.decoded_words and pins in 0..limits.pinned_bytes
      run.(keeper, resource, {words, pins})
    end)
  end

  defp with_native_actor(run) do
    {actor, monitor} = spawn_monitor(fn -> native_loop() end)

    try do
      run.(actor)
    after
      if Process.alive?(actor), do: Process.exit(actor, :kill)
      assert_receive {:DOWN, ^monitor, :process, ^actor, _reason}, 5_000
    end
  end

  defp native_call(actor, run) do
    ref = make_ref()
    send(actor, {:native_call, self(), ref, run})

    receive do
      {:native_reply, ^actor, ^ref, result} -> result
    after
      5_000 -> flunk("owned native actor did not reply")
    end
  end

  defp native_loop do
    receive do
      {:native_call, caller, ref, run}
      when is_pid(caller) and is_reference(ref) and is_function(run, 0) ->
        result = run.()
        send(caller, {:native_reply, self(), ref, result})
        native_loop()
    end
  end
end
