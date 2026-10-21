ExUnit.start(autorun: false)

tool = Path.expand("../policy_runtime_capture.exs", __DIR__)
# Run the real test bodies before the selected provider exists: undefined APIs
# are the initial RED, rather than a conditional success or skipped test.
if File.regular?(tool), do: Code.require_file(tool)

defmodule Ferricstore.PolicyRuntimeCaptureTest do
  use ExUnit.Case, async: false
  import Bitwise, only: [bxor: 2]
  alias Ferricstore.PolicyRuntimeCapture, as: Capture
  alias Ferricstore.PolicyRuntimeCapture.Native
  alias Ferricstore.TermCodec

  @u64 0xFFFF_FFFF_FFFF_FFFF
  @c1 0x87C3_7B91_1142_53D5
  @c2 0x4CF5_AD43_2745_937F

  @limits %{
    donor_bytes: 1_048_576,
    decoded_words: 16_777_216,
    terms: 16_384,
    facts: 4_096,
    text_bytes: 262_144,
    depth: 64,
    pinned_bytes: 16_777_216,
    output_bytes: 1_048_576
  }
  @hamt33 [0, 2, 21, 18, 5, 22, 15, 26, 4, 20, 6, 32, 31, 27, 23, 10] ++
            [9, 19, 30, 14, 17, 1, 13, 29, 8, 24, 3, 25, 11, 7, 16, 12, 28]

  test "refuses_vm_headers_word_layout_or_debug_build_outside_exact_pin" do
    claimed = Map.put(config(), :claimed_otp_package, "28.3.1")

    if package() != "28.3.1" or :erlang.system_info(:build_type) != :opt do
      assert {:error, {:unsupported_vm, _}} = Capture.bootstrap(claimed)
    else
      abi = bootstrap!()
      assert abi == Native.abi()
      assert abi.otp_package == "28.3.1"
      assert abi.otp_release == "28"
      assert abi.erts_version == "16.2"
      assert abi.build_type == :opt
      assert abi.word_size == 8
      assert abi.endianness == :little
      assert byte_size(abi.source_revision) == 40
      assert byte_size(abi.header_digest) == 32
      assert {:error, _} = Capture.bootstrap(%{config() | source_root: __DIR__})
      bad_word = child("native-load", [], load_info_environment(%{abi | word_size: 4}))
      assert {:error, _} = bad_word.result

      debug = child("bootstrap", ["--erl", "-emu_type debug"])
      assert debug.build_type == :debug
      assert {:error, {:unsupported_vm, _}} = debug.result

      direct_debug =
        child("native-load", ["--erl", "-emu_type debug"], load_info_environment(abi))

      assert direct_debug.build_type == :debug
      assert {:error, _} = direct_debug.result

      wrong = child("bootstrap", [], wrong_vm_environment!())
      assert wrong.otp_package != "28.3.1"
      assert {:error, {:unsupported_vm, _}} = wrong.result

      direct =
        child("native-load", [], wrong_vm_environment!() ++ load_info_environment(abi))

      assert direct.otp_package != "28.3.1"
      assert {:error, _} = direct.result
    end
  end

  test "captures_atoms_and_31_32_33_map_order_from_same_live_terms" do
    bootstrap!()
    forward = child("forward")
    reverse = child("reverse")
    [first | _] = forward.names
    last = List.last(forward.names)
    assert forward.names == reverse.names
    assert forward.words[first] < forward.words[last]
    assert reverse.words[first] > reverse.words[last]
    assert forward.order_matches == [true, true, true]
    assert reverse.order_matches == [true, true, true]
    assert forward.key_bif_matches == [true, true, true]
    assert reverse.key_bif_matches == [true, true, true]
    assert Enum.all?(forward.word_matches)
    assert Enum.all?(reverse.word_matches)

    missing = "fsc_uninterned_#{System.unique_integer([:positive, :monotonic])}"
    assert_raise ArgumentError, fn -> String.to_existing_atom(missing) end
    queries = %{atoms: ["nil", missing], exports: []}
    donor = encode(%{nil => 1})
    {warm, _} = capture!([donor], queries)
    assert :ok = Capture.close_epoch(warm)
    before = :erlang.system_info(:atom_count)
    {session, captured} = capture!([donor], queries)
    after_count = :erlang.system_info(:atom_count)
    assert :ok = Capture.close_epoch(session)
    assert after_count == before
    assert captured.queries.atoms["nil"] == true
    assert captured.queries.atoms[missing] == false
    assert_raise ArgumentError, fn -> String.to_existing_atom(missing) end
  end

  test "records_release_hashes_and_collision_ties_from_actual_runtime" do
    bootstrap!()
    # Frozen before this producer: independent modular-inversion vectors also
    # retained in complete_policy/native_order.rs. Both complete64 hashes are0.
    first = Base.decode16!("6695B9741D408EF0D2267D0E14DC9065")
    second = Base.decode16!("E613C3EC92D69BC962D74A65908ADDCD")
    assert first < second
    assert {:ok, native} = Native.capture_terms([first, second], @limits)
    assert native.hashes == [0, 0]
    names = Enum.map(1..33, &"s#{String.pad_leading(to_string(&1), 2, "0")}")
    map = Map.new(names, &{&1, 0})
    expected = Enum.map(@hamt33, &Enum.at(names, &1))
    assert Enum.map(Map.to_list(map), &elem(&1, 0)) == expected
    assert Enum.map(map, fn {key, _} -> key end) == Enum.reverse(expected)
    assert Map.keys(map) == expected
    assert_order!(map, expected)

    keys = Enum.take(names, 31) ++ [first, second]
    collision_map = Map.new(keys, &{&1, 0})
    order = Enum.filter(@hamt33, &(&1 < 31)) ++ [32, 31]
    expected = Enum.map(order, &Enum.at(keys, &1))
    assert Enum.take(expected, -2) == [second, first]
    assert Enum.map(Map.to_list(collision_map), &elem(&1, 0)) == expected
    assert Enum.map(collision_map, fn {key, _} -> key end) == Enum.reverse(expected)
    assert Map.keys(collision_map) == expected
    assert_order!(collision_map, expected)
  end

  test "pins_runtime_terms_through_gc_until_explicit_close" do
    bootstrap!()
    donor = runtime_donor()
    {keeper, captured} = capture!([donor])
    assert is_pid(keeper) and Process.alive?(keeper)
    remote = Enum.find(captured.nodes, &(&1.name == "fsc_capture@invalid"))
    assert remote.creation == 1_297
    assert is_integer(remote.hash_input) and remote.hash_input in 0..@u64
    assert captured.exports != []
    assert captured.local_funs != []

    final =
      Enum.reduce(1..3, captured, fn _, prior ->
        assert :erlang.garbage_collect(self())
        assert :erlang.garbage_collect(keeper)

        assert {:ok, next} =
                 Capture.extend_epoch(keeper, [donor], empty_queries(), prior.identity)

        assert Enum.find(next.nodes, &(&1.name == remote.name)) == remote
        assert next.parent == prior.identity
        next
      end)

    {:ok, {pid, external, local}} = TermCodec.decode(donor)
    export = Enum.find(captured.exports, &(&1.module == "Elixir.Enum" and &1.function == "map"))
    assert export.arity == 2
    assert is_integer(export.hash_input) and export.hash_input in 0..@u64
    info = Map.new(:erlang.fun_info(local))

    expected = %{
      module: Atom.to_string(info.module),
      index: info.new_index,
      arity: info.arity,
      uniq: info.new_uniq,
      old_uniq: info.uniq
    }

    assert Enum.any?(captured.local_funs, &(Map.take(&1, Map.keys(expected)) == expected))

    with_internal_hash(fn hash ->
      assert leaf_hash([{:a, 18}, {:b, remote.hash_input}, {:a, 7}]) == hash.(pid)
      assert leaf_hash([{:a, 13}, {:b, export.hash_input}]) == hash.(external)
    end)

    monitor = Process.monitor(keeper)
    assert :ok = Capture.close_epoch(keeper)
    assert_receive {:DOWN, ^monitor, :process, ^keeper, :normal}, 1_000
    refute Process.alive?(keeper)

    assert {:error, :closed_session} =
             Capture.extend_epoch(keeper, [donor], empty_queries(), final.identity)

    # A new lifetime is required. Address reuse is allowed, so never assert
    # that its opaque operand differs from the released one.
    {new_keeper, fresh} = capture!([donor])
    assert fresh.identity != captured.identity
    assert fresh.session_id != captured.session_id
    assert :ok = Capture.close_epoch(new_keeper)

    refute_received :fsc_capture_fun_executed
  end

  test "preserves_source_safe_decode_and_query_origins_without_executing_funs" do
    bootstrap!()
    caller = self()
    local_fun = fn -> send(caller, :fsc_capture_fun_executed) end
    {session, captured} = capture!([encode({&Enum.map/2, local_fun})])
    assert captured.exports != []
    assert captured.local_funs != []
    assert :ok = Capture.close_epoch(session)
    refute_received :fsc_capture_fun_executed

    name = "fsc_absent_#{System.unique_integer([:positive, :monotonic])}"
    unknown = <<131, 119, byte_size(name), name::binary>>
    valid = encode(%{nil => 7})
    compressed = :erlang.term_to_binary(String.duplicate("x", 1_024), compressed: 9)
    assert <<131, 80, _::binary>> = compressed
    # 73/75/internal atom tags and121/local framing are classified against the
    # actual source decoder, rather than declared invalid by a portable table.
    for raw <- [
          unknown,
          encode(make_ref()),
          valid <> <<0>>,
          compressed,
          <<131, 73, 0, 0>>,
          <<131, 75, 0, 0, 0>>,
          <<131, 121, 0>>
        ] do
      case TermCodec.decode(raw) do
        {:error, reason} ->
          assert {:error, {:source_decode, 0, ^reason}} =
                   Capture.capture_epoch([raw], empty_queries(), nil, @limits)

        {:ok, _} ->
          {s, c} = capture!([raw])
          assert c.donor_hashes == [sha(raw)]
          assert :ok = Capture.close_epoch(s)
      end
    end

    active = {"erlang", "is_atom", 1}
    absent = {"erlang", name, 0}
    {s, c} = capture!([valid], %{atoms: ["nil", name], exports: [active, absent]})
    assert c.queries.atoms["nil"] == true
    assert c.queries.atoms[name] == false
    assert c.queries.exports[active] == true
    assert c.queries.exports[absent] == false
    assert_raise ArgumentError, fn -> String.to_existing_atom(name) end
    assert :ok = Capture.close_epoch(s)
  end

  test "rejects_changed_donor_parent_session_or_code_identity" do
    bootstrap!()
    old = encode(%{nil => 1})
    changed = encode(%{nil => 2})
    queries = empty_queries()
    {a, first} = capture!([old])
    {b, same_input} = capture!([old])
    assert first.identity != same_input.identity
    assert first.session_id != same_input.session_id
    assert first.donor_hashes == [sha(old)]
    assert first.query_hash == sha(encode(queries))

    assert {:error, :parent_mismatch} =
             Capture.extend_epoch(b, [changed], queries, first.identity)

    <<byte, rest::binary>> = first.identity
    wrong = <<bxor(byte, 1), rest::binary>>
    assert {:error, :parent_mismatch} = Capture.extend_epoch(a, [changed], queries, wrong)
    assert {:error, :invalid_session} = Capture.extend_epoch(make_ref(), [old], queries, nil)
    assert {:ok, next} = Capture.extend_epoch(a, [changed], queries, first.identity)
    assert next.parent == first.identity
    assert sha(old) in next.donor_hashes
    assert sha(changed) in next.donor_hashes
    assert first.donor_hashes == [sha(old)]
    assert :ok = Capture.close_epoch(a)
    assert :ok = Capture.close_epoch(b)
    assert {:error, :closed_session} = Capture.extend_epoch(a, [old], queries, next.identity)
    assert_source_code_change!()
  end

  test "bounds_traversal_facts_retained_pins_and_output_on_failure" do
    bootstrap!()
    donor = encode({nil, {nil, {nil, 7}}})

    cases = [
      donor_bytes: byte_size(donor) - 1,
      terms: 0,
      facts: 0,
      depth: 1,
      pinned_bytes: 0,
      output_bytes: 0,
      decoded_words: 0,
      text_bytes: 0
    ]

    for {field, maximum} <- cases do
      limits = Map.put(@limits, field, maximum)

      assert {:error, {:limit, ^field}} =
               Capture.capture_epoch([donor], empty_queries(), nil, limits)

      # A failed attempt cannot poison the next adequately admitted session.
      {s, c} = capture!([donor])
      assert c.donor_hashes == [sha(donor)]
      assert :ok = Capture.close_epoch(s)
    end

    {s, c} = capture!([encode(nil)], empty_queries(), %{@limits | output_bytes: 16_384})
    huge = encode(Map.new(1..1_024, &{String.duplicate("x", 48) <> to_string(&1), 0}))

    assert {:error, {:limit, :output_bytes}} =
             Capture.extend_epoch(s, [huge], empty_queries(), c.identity)

    assert {:ok, again} = Capture.extend_epoch(s, [encode(1)], empty_queries(), c.identity)
    assert again.parent == c.identity
    assert again.donor_hashes == c.donor_hashes ++ [sha(encode(1))]
    assert again.orders == []
    assert again.atoms == c.atoms
    assert again.nodes == c.nodes
    assert again.exports == c.exports
    assert again.local_funs == c.local_funs
    with_internal_hash(fn hash -> assert again.hashes == [hash.(nil), hash.(1)] end)
    assert :ok = Capture.close_epoch(s)

    anchor = :binary.compile_pattern(String.duplicate("x", 4_096))
    encoded = encode(anchor)
    assert byte_size(encoded) < 1_024
    anchored = %{encoded: encoded, source_term: anchor}

    assert {:error, {:source_anchor_required, 0}} =
             Capture.capture_epoch([encoded], empty_queries(), nil, @limits)

    assert {:error, {:limit, :pinned_bytes}} =
             Capture.capture_epoch([anchored], empty_queries(), nil, %{
               @limits
               | pinned_bytes: 1_024
             })

    assert {:ok, keeper, captured} =
             Capture.capture_epoch([anchored], empty_queries(), nil, @limits)

    assert captured.donor_hashes == [sha(encoded)]
    assert :ok = Capture.close_epoch(keeper)

    assert {:error, {:source_anchor_mismatch, 0}} =
             Capture.capture_epoch([%{anchored | source_term: 7}], empty_queries(), nil, @limits)

    # An integer supplied directly to the native API is never a pointer input.
    assert {:ok, integers} = Native.capture_terms([0x7FFF_FFFF_FFFF], @limits)
    assert length(integers.hashes) == 1
  end

  def run_child("bootstrap") do
    emit(%{
      otp_package: package(),
      build_type: :erlang.system_info(:build_type),
      result: Capture.bootstrap(config())
    })
  end

  def run_child("native-load") do
    load_info =
      System.fetch_env!("FS_CAPTURE_LOAD_INFO")
      |> Base.decode64!()
      |> :erlang.binary_to_term([:safe])

    emit(%{
      otp_package: package(),
      build_type: :erlang.system_info(:build_type),
      result: Native.load(config().nif_path, load_info)
    })
  end

  def run_child(direction) when direction in ["forward", "reverse"] do
    bootstrap!()
    names = Enum.map(1..33, &"fsc_birth_#{String.pad_leading(to_string(&1), 2, "0")}")
    creation = if direction == "forward", do: names, else: Enum.reverse(names)
    Enum.each(creation, &String.to_atom/1)
    atoms = Enum.map(names, &String.to_existing_atom/1)
    maps = Enum.map([31, 32, 33], &Map.new(Enum.take(atoms, &1), fn key -> {key, 0} end))
    donors = Enum.map(atoms ++ maps, &encode/1)
    {s, c} = capture!(donors)
    words = Map.new(c.atoms, &{&1.name, &1.native_word})

    word_matches =
      with_internal_hash(fn hash ->
        Enum.map(atoms, fn atom ->
          word = Map.fetch!(words, Atom.to_string(atom))
          assert is_integer(word) and word >= 0 and word <= @u64
          mix64(word) == hash.(atom)
        end)
      end)

    actual = Enum.map(33..35, fn i -> Enum.find(c.orders, &(&1.donor == i)).keys end)
    enum_orders = Enum.map(maps, &Enum.map(&1, fn {key, _} -> encode(key) end))
    bif_orders = Enum.map(maps, &Enum.map(Map.keys(&1), fn key -> encode(key) end))
    assert :ok = Capture.close_epoch(s)

    emit(%{
      names: names,
      words: words,
      word_matches: word_matches,
      order_matches:
        Enum.zip_with(actual, List.update_at(enum_orders, 2, &Enum.reverse/1), &(&1 == &2)),
      key_bif_matches: Enum.zip_with(actual, bif_orders, &(&1 == &2))
    })
  end

  defp assert_order!(map, expected) do
    {s, c} = capture!([encode(map)])
    assert [%{donor: 0, keys: keys}] = c.orders
    assert keys == Enum.map(expected, &encode/1)
    assert keys == Enum.map(Map.to_list(map), fn {key, _} -> encode(key) end)
    assert keys == Enum.map(Map.keys(map), &encode/1)
    assert :ok = Capture.close_epoch(s)
  end

  defp assert_source_code_change! do
    path =
      Path.join(config().elixir_source_root, "apps/ferricstore/lib/ferricstore/term_codec.ex")

    before = Code.compiler_options(ignore_module_conflict: true)
    [{TermCodec, original}] = Code.compile_file(path)
    {s, c} = capture!([encode(nil)])

    try do
      Code.compile_quoted(
        quote do
          defmodule Ferricstore.TermCodec do
            def decode(_), do: raise("changed source decoder must not run")
          end
        end
      )

      assert {:error, :source_code_changed} =
               Capture.extend_epoch(s, [<<131, 97, 1>>], empty_queries(), c.identity)
    after
      :code.purge(TermCodec)
      {:module, TermCodec} = :code.load_binary(TermCodec, String.to_charlist(path), original)
      Code.compiler_options(before)
      Capture.close_epoch(s)
    end
  end

  defp runtime_donor do
    caller = self()
    node_name = Atom.to_string(:fsc_capture@invalid)

    remote =
      <<131, 88, 119, byte_size(node_name), node_name::binary, 7::unsigned-big-32,
        0::unsigned-big-32, 1_297::unsigned-big-32>>

    {:ok, pid} = TermCodec.decode(remote)
    encode({pid, &Enum.map/2, fn -> send(caller, :fsc_capture_fun_executed) end})
  end

  defp capture!(donors, queries \\ empty_queries(), limits \\ @limits) do
    assert {:ok, session, captured} = Capture.capture_epoch(donors, queries, nil, limits)
    assert is_pid(session)
    assert byte_size(captured.identity) == 32
    assert byte_size(captured.session_id) == 32
    assert captured.parent == nil
    assert captured.donor_hashes == Enum.map(donors, &sha/1)
    {session, captured}
  end

  def bootstrap! do
    assert {:ok, abi} = Capture.bootstrap(config())

    oracle =
      Path.join(config().elixir_source_root, "apps/ferricstore/lib/ferricstore/term_codec.ex")

    Code.require_file(oracle)
    abi
  end

  def limits, do: @limits

  def config do
    base = "/private/tmp/ferricstore2-policy-runtime-capture-otp-28.3.1"

    %{
      otp_root: System.get_env("FS_CAPTURE_OTP_ROOT") || Path.join(base, "otp/lib/erlang"),
      source_root: System.get_env("FS_CAPTURE_OTP_SOURCE") || Path.join(base, "src"),
      nif_path:
        System.get_env("FS_CAPTURE_NIF") || Path.join(base, "native/policy_runtime_capture"),
      elixir_source_root:
        System.get_env("FS_CAPTURE_ELIXIR_SOURCE") ||
          Path.expand("../../../ferricstore2-elixir-baseline", __DIR__)
    }
  end

  defp package do
    root = :code.root_dir() |> List.to_string()

    File.read!(Path.join([root, "releases", System.otp_release(), "OTP_VERSION"]))
    |> String.trim()
  end

  defp child(mode, prefix \\ [], environment \\ []) do
    elixir = System.find_executable("elixir") || raise "elixir executable missing"

    {output, status} =
      System.cmd(elixir, prefix ++ [__ENV__.file, "--capture-child", mode],
        env: environment,
        stderr_to_stdout: true
      )

    assert status == 0, output

    line =
      output |> String.split("\n") |> Enum.find(&String.starts_with?(&1, "B2_CAPTURE_CHILD:"))

    assert is_binary(line), output

    line
    |> String.replace_prefix("B2_CAPTURE_CHILD:", "")
    |> Base.decode64!()
    |> :erlang.binary_to_term([:safe])
  end

  defp wrong_vm_environment! do
    path =
      System.get_env("FS_CAPTURE_WRONG_PATH") ||
        raise "set FS_CAPTURE_WRONG_PATH to an authentic non-28.3.1 elixir/erl PATH"

    [{"PATH", path}]
  end

  defp load_info_environment(info),
    do: [{"FS_CAPTURE_LOAD_INFO", Base.encode64(:erlang.term_to_binary(info))}]

  # Independent pinned OTP28.3.1 hash schedule (5b8f23bd), compared with the
  # actual VM's reference hash, never a second producer-returned hash.
  defp with_internal_hash(body) do
    old = :erts_debug.set_internal_state(:available_internal_state, true)

    try do
      :erts_debug.get_internal_state({:internal_hash, nil})
      body.(fn term -> :erts_debug.get_internal_state({:internal_hash, term}) end)
    after
      :erts_debug.set_internal_state(:available_internal_state, old)
    end
  end

  defp w64(x), do: Bitwise.band(x, @u64)
  defp rol(x, n), do: w64(Bitwise.bor(Bitwise.bsl(x, n), Bitwise.bsr(x, 64 - n)))

  defp mix64(x) do
    x = w64(bxor(x, Bitwise.bsr(x, 33)) * 0xFF51_AFD7_ED55_8CCD)
    x = w64(bxor(x, Bitwise.bsr(x, 33)) * 0xC4CE_B9FE_1A85_EC53)
    bxor(x, Bitwise.bsr(x, 33))
  end

  defp leaf_hash(events) do
    {a, b, ticks} = Enum.reduce(events, {0, 0, 0}, &leaf_step/2)
    a = bxor(a, ticks)
    b = bxor(b, ticks)
    a = w64(a + b)
    b = w64(b + a)
    a = mix64(a)
    b = mix64(b)
    a = w64(a + b)
    b = w64(b + a)
    bxor(a, b)
  end

  defp leaf_step({:a, word}, {a, b, ticks}) do
    word = w64(rol(w64(word * @c1), 31) * @c2)
    {w64((rol(bxor(a, word), 27) + b) * 5 + 0x52DC_E729), b, ticks + 1}
  end

  defp leaf_step({:b, word}, {a, b, ticks}) do
    word = w64(rol(w64(word * @c2), 33) * @c1)
    {a, w64((rol(bxor(b, word), 31) + a) * 5 + 0x3849_5AB5), ticks + 1}
  end

  defp emit(value),
    do: IO.puts("B2_CAPTURE_CHILD:" <> Base.encode64(:erlang.term_to_binary(value)))

  defp empty_queries, do: %{atoms: [], exports: []}
  defp encode(value), do: TermCodec.encode(value)
  defp sha(bytes), do: :crypto.hash(:sha256, bytes)
end

Code.require_file("policy_runtime_capture/admission.exs", __DIR__)
Code.require_file("policy_runtime_capture/query_admission.exs", __DIR__)

case System.argv() do
  ["--admission-child", mode] ->
    Ferricstore.PolicyRuntimeCaptureAdmissionTest.run_child(mode)

  ["--capture-child", mode] ->
    Ferricstore.PolicyRuntimeCaptureTest.run_child(mode)

  [] ->
    result = ExUnit.run()
    System.halt(if result.failures == 0, do: 0, else: 1)

  _ ->
    raise "unexpected capture-test arguments"
end
