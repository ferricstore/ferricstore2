defmodule Ferricstore.PolicyRuntimeCaptureKernelProfileTest do
  use ExUnit.Case, async: false
  alias Ferricstore.PolicyRuntimeCapture.Native
  alias Ferricstore.PolicyRuntimeCaptureTest, as: SourceTest

  # Component reservation inputs; actor/RPC copies belong to the fixture.
  @entry_words 37
  @entry_pins 296
  @terminal_words 53
  @terminal_pins 1_048_576

  test "profile_registration_preserves_numeric_contracts_and_prices_resource" do
    SourceTest.bootstrap!()
    limits = SourceTest.limits()
    cw = limits.decoded_words
    cp = limits.pinned_bytes

    registered =
      for mode <- [:numeric, :profile] do
        with_native_actor(fn keeper ->
          {words, pins} = if mode == :numeric, do: {11, 19}, else: {{11, 0, 0}, {19, 0, 0}}

          assert {:ok, resource, spent_w, spent_p} =
                   native_call(keeper, fn -> Native.budget_register(cw, cp, words, pins) end)

          assert is_reference(resource) and spent_w == 19 and spent_p > 19
          {spent_w, spent_p}
        end)
      end

    assert [{19, numeric_p}, {19, profile_p}] = registered
    assert profile_p >= numeric_p + 32

    for {words, pins, word_cap, pin_cap, expected} <- [
          {0, 0, 35, cp, :ok},
          {0, 0, 34, cp, :decoded_words},
          {0, 0, cw, 0, :pinned_bytes},
          {{0, @entry_words, @terminal_words}, {0, @entry_pins, @terminal_pins},
           35 + @terminal_words, cp, :ok},
          {{0, @entry_words, @terminal_words}, {0, @entry_pins, @terminal_pins},
           34 + @terminal_words, cp, :decoded_words},
          {{0, @entry_words, @terminal_words}, {0, @entry_pins, @terminal_pins}, cw, 0,
           :pinned_bytes}
        ] do
      with_native_actor(fn keeper ->
        result =
          native_call(keeper, fn -> Native.budget_register(word_cap, pin_cap, words, pins) end)

        if expected == :ok do
          assert {:ok, resource, 8, paid_p} = result
          assert is_reference(resource) and paid_p <= pin_cap
        else
          assert result == expected
        end
      end)
    end

    with_native_actor(fn keeper ->
      for {words, pins} <- [
            {{0, @entry_words, @terminal_words}, 0},
            {0, {0, @entry_pins, @terminal_pins}},
            {{0, @entry_words}, {0, @entry_pins, @terminal_pins}},
            {{0, @entry_words, @terminal_words}, {0, @entry_pins}},
            {{0, -1, @terminal_words}, {0, @entry_pins, @terminal_pins}},
            {{0, 0x1_0000_0000, @terminal_words}, {0, @entry_pins, @terminal_pins}},
            {{0, @entry_words, 0x1_0000_0000}, {0, @entry_pins, @terminal_pins}},
            {{0, @entry_words, @terminal_words}, {0, 0x1_0000_0000, @terminal_pins}},
            {{0, @entry_words, @terminal_words}, {0, @entry_pins, 0x1_0000_0000}}
          ] do
        assert native_call(keeper, fn -> Native.budget_register(cw, cp, words, pins) end) ==
                 :stale

        assert native_call(keeper, fn ->
                 Stream.repeatedly(fn -> Native.budget_enter(self()) end)
                 |> Enum.find(&(&1 != :retry_registry))
               end) == :unmanaged
      end
    end)
  end

  test "profile_entry_settlement_and_reset_preserve_immutable_floors" do
    SourceTest.bootstrap!()
    limits = SourceTest.limits()
    cw = limits.decoded_words
    cp = limits.pinned_bytes

    deltas =
      for profile? <- [false, true] do
        with_native_actor(fn keeper ->
          {wa, pa} =
            if profile?,
              do: {{0, @entry_words, @terminal_words}, {0, @entry_pins, @terminal_pins}},
              else: {0, 0}

          assert {:ok, r, registered_w, registered_p} =
                   native_call(keeper, fn -> Native.budget_register(cw, cp, wa, pa) end)

          with_native_actor(fn owner ->
            with_native_actor(fn next_owner ->
              entries =
                for {current, finish} <- [{owner, :retry}, {next_owner, :commit}] do
                  {:ok, e, ref, w, p} = bound_entry(keeper, current, r)

                  for phase <- if(finish == :retry, do: [:control], else: [:control, :keeper]) do
                    if phase == :keeper do
                      assert native_call(keeper, fn ->
                               Native.budget_control(
                                 r,
                                 e,
                                 :reserve,
                                 {w, p, cw - w, cp - p, :offer}
                               )
                             end) == :ok

                      assert native_call(current, fn ->
                               Native.budget_control(r, e, :begin, nil)
                             end) == :ok

                      assert native_call(keeper, fn ->
                               Native.budget_control(r, e, :settle, {w, p, :ready})
                             end) == :ok

                      assert native_call(owner, fn ->
                               Stream.repeatedly(fn -> Native.budget_enter(keeper) end)
                               |> Enum.find(&(&1 != :retry_registry))
                             end) == :session_busy
                    end

                    # Numeric permits both; each profile coordinate refuses independently.
                    for {uw, up, expected} <- [
                          {27 + @terminal_words - 1, cp - p,
                           if(profile?, do: :decoded_words, else: :ok)},
                          {cw - w, @terminal_pins - 1, if(profile?, do: :pinned_bytes, else: :ok)}
                        ] do
                      assert native_call(keeper, fn ->
                               Native.budget_control(r, e, :reserve, {w, p, uw, up, phase})
                             end) == expected
                    end
                  end

                  if finish == :retry do
                    assert native_call(current, fn ->
                             Native.budget_control(r, e, :cancel, nil)
                           end) == :ok
                  end

                  assert native_call(keeper, fn ->
                           Native.budget_control(r, e, :settle, {w, p, finish})
                         end) == :ok

                  {e, ref, w, p}
                end

              [{first_e, _, first_w, first_p}, {second_e, _, second_w, second_p}] = entries
              assert first_w == registered_w + 15 and second_w == first_w + 15
              assert second_p - first_p == first_p - registered_p
              assert second_e > first_e
              {:ok, third_e, _, third_w, third_p} = bound_entry(keeper, owner, r)
              assert third_e > second_e and third_w == second_w + 15
              assert third_p - second_p == first_p - registered_p

              for {previous, {e, ref, _, _}} <- Enum.zip([owner, next_owner], entries) do
                assert native_call(previous, fn -> Native.budget_bind(r, e, ref) end) == :stale

                assert native_call(keeper, fn ->
                         Native.budget_read(r, previous, ref)
                       end) == :stale
              end

              {first_w - registered_w, first_p - registered_p}
            end)
          end)
        end)
      end

    assert [{15, numeric_p}, {15, profile_p}] = deltas
    # Identical calls leave both supplied entry extras unused, including296 pin bytes.
    assert profile_p == numeric_p
  end

  test "profile_terminal_entry_and_owner_loss_preserve_close_semantics" do
    SourceTest.bootstrap!()
    limits = SourceTest.limits()
    cp = limits.pinned_bytes
    # Registration8 + protected(27+53) + prospective normal(15+37) - one.
    terminal_cap = 8 + 27 + @terminal_words + 15 + @entry_words - 1
    assert terminal_cap == 139

    for {cap, after_retry?} <- [{terminal_cap, false}, {154, true}, {limits.decoded_words, false}] do
      with_native_actor(fn keeper ->
        assert {:ok, r, 8, _} =
                 native_call(keeper, fn ->
                   Native.budget_register(
                     cap,
                     cp,
                     {0, @entry_words, @terminal_words},
                     {0, @entry_pins, @terminal_pins}
                   )
                 end)

        with_native_actor(fn owner ->
          {phase, e, ref, w, p} = bound_entry(keeper, owner, r)

          {phase, e, ref, w, p} =
            if after_retry? do
              assert phase == :ok and w == 23

              assert native_call(owner, fn -> Native.budget_control(r, e, :cancel, nil) end) ==
                       :ok

              assert native_call(keeper, fn ->
                       Native.budget_control(r, e, :settle, {w, p, :retry})
                     end) == :ok

              {next_phase, next_e, next_ref, next_w, _} = next = bound_entry(keeper, owner, r)
              assert next_phase == :terminal and next_e > e and next_ref != ref and next_w == 38
              next
            else
              {phase, e, ref, w, p}
            end

          expected_phase = if cap == limits.decoded_words, do: :ok, else: :terminal
          assert phase == expected_phase and w == if(after_retry?, do: 38, else: 23)

          if phase == :terminal do
            assert native_call(keeper, fn -> Native.budget_read(r, owner, ref) end) == :stale

            assert native_call(keeper, fn ->
                     Native.budget_control(r, e, :settle, {w, p, :retry})
                   end) == :stale

            assert native_call(keeper, fn ->
                     Native.budget_control(r, e, :reserve, {w, p, cap - w, cp - p, :control})
                   end) == :decoded_words

            assert native_call(keeper, fn -> Native.budget_control(r, e, :close, nil) end) == :ok
          else
            assert native_call(keeper, fn ->
                     Native.budget_control(r, e, :reserve, {w, p, 4_096, 2_097_152, :offer})
                   end) == :ok

            assert native_call(owner, fn -> Native.budget_control(r, e, :begin, nil) end) == :ok
            monitor = Process.monitor(owner)
            Process.exit(owner, :kill)
            assert_receive {:DOWN, ^monitor, :process, ^owner, _reason}, 5_000
          end

          assert native_call(keeper, fn ->
                   receive do
                     {:budget_closed, closed_resource} -> closed_resource
                   after
                     5_000 -> :notification_timeout
                   end
                 end) == r

          assert Process.alive?(keeper)

          assert native_call(keeper, fn ->
                   Stream.repeatedly(fn -> Native.budget_enter(self()) end)
                   |> Enum.find(&(&1 != :retry_registry))
                 end) == :closed

          assert native_call(keeper, fn -> Native.budget_read(r, owner, ref) end) == :closed

          assert native_call(keeper, fn ->
                   Native.budget_control(r, e, :settle, {w, p, :retry})
                 end) == :closed
        end)
      end)
    end
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
    # Selective receive preserves the real native budget_closed notification.
    receive do
      {:native_call, caller, ref, run}
      when is_pid(caller) and is_reference(ref) and is_function(run, 0) ->
        result = run.()
        send(caller, {:native_reply, self(), ref, result})
        native_loop()
    end
  end

  defp bound_entry(keeper, owner, resource) do
    {entered, ref} =
      native_call(owner, fn ->
        entered =
          Stream.repeatedly(fn -> Native.budget_enter(keeper) end)
          |> Enum.find(&(&1 != :retry_registry))

        {entered, make_ref()}
      end)

    assert {phase, ^resource, epoch} = entered
    assert phase in [:ok, :terminal]
    assert native_call(owner, fn -> Native.budget_bind(resource, epoch, ref) end) == :ok

    assert {:ok, ^epoch, words, pins} =
             native_call(keeper, fn -> Native.budget_read(resource, owner, ref) end)

    {phase, epoch, ref, words, pins}
  end
end
