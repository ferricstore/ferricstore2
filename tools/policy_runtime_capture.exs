defmodule Ferricstore.PolicyRuntimeCapture.Native do
  @moduledoc false
  @package "28.3.1"
  @revision "aa5b1d3366762ff8f420f2aa6a52604b481530c9"
  @loaded_artifact {__MODULE__, :loaded_artifact}

  def load(path, expected) when is_binary(path) and is_map(expected) do
    with {:ok, actual} <- runtime(),
         true <- Enum.all?(actual, fn {key, value} -> Map.get(expected, key) == value end),
         true <- expected[:source_revision] == @revision,
         true <- is_binary(expected[:header_digest]) and byte_size(expected.header_digest) == 32,
         {:ok, identity} <- artifact(path) do
      case :persistent_term.get(@loaded_artifact, nil) do
        nil -> load_artifact(identity, expected)
        {^identity, ^expected} -> if abi() == expected, do: :ok, else: artifact_mismatch()
        _ -> artifact_mismatch()
      end
    else
      {:error, _} = error -> error
      _ -> {:error, :abi_mismatch}
    end
  end

  def load(_, _), do: {:error, :invalid_load}
  def abi, do: {:error, :not_loaded}
  def capture_terms(_, _), do: {:error, :not_loaded}
  def budget_register(_, _, _, _), do: {:error, :not_loaded}
  def budget_enter(_), do: {:error, :not_loaded}
  def budget_bind(_, _, _), do: {:error, :not_loaded}
  def budget_read(_, _, _), do: {:error, :not_loaded}
  def budget_control(_, _, _, _), do: {:error, :not_loaded}

  defp artifact(path) do
    prefix = Path.expand(path)

    case bounded_file(prefix <> ".so", 33_554_432) do
      {:ok, bytes} -> {:ok, {prefix, :crypto.hash(:sha256, bytes)}}
      _ -> {:error, {:native_load, :artifact_unavailable}}
    end
  end

  defp load_artifact({path, _digest} = identity, expected) do
    case :erlang.load_nif(String.to_charlist(path), expected) do
      :ok ->
        with {:ok, ^identity} <- artifact(path), ^expected <- abi() do
          :persistent_term.put(@loaded_artifact, {identity, expected})
          :ok
        else
          _ -> artifact_mismatch()
        end

      {:error, {:reload, _}} ->
        if :persistent_term.get(@loaded_artifact, nil) == {identity, expected} and
             abi() == expected, do: :ok, else: artifact_mismatch()

      {:error, reason} ->
        {:error, {:native_load, reason}}
    end
  end

  defp artifact_mismatch, do: {:error, {:unsupported_vm, :native_artifact_mismatch}}

  defp runtime do
    runtime_identity()
  rescue
    _ -> {:error, {:unsupported_vm, :runtime_identity_unavailable}}
  end

  @doc false
  def runtime_identity do
    root = List.to_string(:code.root_dir())

    {:ok, version} =
      bounded_file(Path.join([root, "releases", System.otp_release(), "OTP_VERSION"]), 256)

    package = String.trim(version)

    actual = %{
      otp_package: package,
      otp_release: System.otp_release(),
      erts_version: List.to_string(:erlang.system_info(:version)),
      build_type: :erlang.system_info(:build_type),
      word_size: :erlang.system_info(:wordsize),
      endianness: :erlang.system_info(:endian)
    }

    if actual == %{
         otp_package: @package,
         otp_release: "28",
         erts_version: "16.2",
         build_type: :opt,
         word_size: 8,
         endianness: :little
       },
       do: {:ok, actual},
       else: {:error, {:unsupported_vm, actual}}
  end

  defp bounded_file(path, maximum) do
    with {:ok, %{type: :regular}} <- File.lstat(path),
         {:ok, file} <- :file.open(String.to_charlist(path), [:read, :binary, :raw]) do
      try do
        with {:ok, info} <- :file.read_file_info(file),
             true <- elem(info, 2) == :regular and elem(info, 1) <= maximum do
          case :file.read(file, maximum + 1) do
            {:ok, bytes} when byte_size(bytes) <= maximum ->
              if :file.read(file, 1) == :eof, do: {:ok, bytes}, else: :error

            :eof ->
              {:ok, <<>>}

            _ ->
              :error
          end
        else
          _ -> :error
        end
      after
        :file.close(file)
      end
    else
      _ -> :error
    end
  end
end

Code.require_file("policy_runtime_capture/budget.exs", __DIR__)
Code.require_file("policy_runtime_capture/bootstrap.exs", __DIR__)
Code.require_file("policy_runtime_capture/input.exs", __DIR__)

defmodule Ferricstore.PolicyRuntimeCapture do
  @moduledoc false
  alias Ferricstore.PolicyRuntimeCapture.Bootstrap
  alias Ferricstore.PolicyRuntimeCapture.Input
  alias Ferricstore.PolicyRuntimeCapture.Native
  alias Ferricstore.TermCodec
  @config_key {__MODULE__, :verified_bootstrap}
  @fields [
    :donor_bytes,
    :decoded_words,
    :terms,
    :facts,
    :text_bytes,
    :depth,
    :pinned_bytes,
    :output_bytes
  ]
  def bootstrap(config) when is_map(config) do
    with {:ok, context} <- Bootstrap.verify(config) do
      :persistent_term.put(@config_key, context)
      {:ok, context.abi}
    end
  rescue
    _ -> {:error, {:unsupported_vm, :bootstrap_inputs}}
  end

  def bootstrap(_), do: {:error, {:unsupported_vm, :bootstrap_inputs}}

  def capture_epoch(donors, queries, parent, limits) do
    with {:ok, context} <- bootstrapped(),
         :ok <- source_current(context.source),
         :ok <- validate_parent(parent),
         :ok <- validate_limits(limits),
         zero = %{decoded_words: 0, pinned_bytes: 0},
         :ok <- Input.preparation_shape(donors, queries, limits, zero),
         {:ok, controls} <- transfer_admission(parent, context, limits, zero, zero),
         {:ok, prepared, queries, caller} <-
           Input.prepare(
             donors,
             queries,
             limits,
             controls,
             context.source
           ),
         :ok <- source_current(context.source),
         {:ok, caller} <- normalization_admission(prepared, limits, caller, true),
         {donors, anchors} when is_list(donors) and is_list(anchors) <- Input.normalize(prepared),
         {:ok, terms, admission} <- decode(donors, prepared, limits, caller),
         {:ok, observed, admission} <- Input.query_admission(queries, limits, admission) do
      id = :crypto.strong_rand_bytes(32)

      state = %{
        context: context,
        limits: limits,
        terms: terms,
        donors: donors,
        anchors: anchors,
        queries: queries,
        observed: observed,
        session_id: id,
        capture: nil,
        admission: admission
      }

      with {:ok, capture} <- observe(state, parent) do
        ready = %{state | capture: capture}
        keeper = spawn(fn -> keep(ready) end)
        {:ok, keeper, capture}
      end
    end
  end

  def extend_epoch(session, donors, queries, parent) do
    budget_parent = if is_binary(parent) and byte_size(parent) == 32, do: parent, else: nil

    with {:ok, limits, retained, source} <- call(session, {:budget, budget_parent}),
         :ok <- source_current(source),
         {:ok, prepared, compact, caller} <-
           Input.prepare(donors, queries, limits, retained, source),
         :ok <- source_current(source),
         {:ok, caller} <- Input.decode_admission(prepared, limits, caller),
         {:ok, _transfer} <- transfer_admission(parent, source, limits, retained, caller) do
      call(session, {:extend, prepared, compact, {parent, donors, queries}})
    end
  end

  def close_epoch(session) when is_pid(session) do
    case call(session, :close) do
      {:error, :closed_session} -> :ok
      result -> result
    end
  end

  def close_epoch(_), do: {:error, :invalid_session}

  defp keep(state) do
    receive do
      {:capture, caller, ref, :close} ->
        send(caller, {ref, :ok})

      {:capture, caller, ref, {:budget, parent}} ->
        result =
          with true <- parent == state.capture.identity,
               :ok <- source_current(state.context.source) do
            {:ok, state.limits, state.admission, state.context.source}
          else
            false -> {:error, :parent_mismatch}
            {:error, _} = error -> error
          end

        send(caller, {ref, result})
        if result != {:error, :source_code_changed}, do: keep(state)

      {:capture, caller, ref, {:extend, donors, queries, original}} ->
        case extend(state, donors, queries, original) do
          {:ok, next} ->
            send(caller, {ref, {:ok, next.capture}})
            keep(next)

          {:error, :source_code_changed} = error ->
            send(caller, {ref, error})

          {:error, _} = error ->
            send(caller, {ref, error})
            keep(state)
        end

      _ ->
        keep(state)
    end
  end

  defp extend(state, donors, queries, {parent, original_donors, original_queries}) do
    with true <- parent == state.capture.identity,
         :ok <- source_current(state.context.source),
         original_rows when is_integer(original_rows) <-
           custody_rows(original_donors, original_queries),
         prepared_rows when is_integer(prepared_rows) <- custody_rows(donors, queries),
         {:ok, original} <-
           Input.original_admission(
             original_donors,
             original_queries,
             state.limits,
             state.admission,
             2
           ),
         {:ok, prepared} <- Input.original_admission(donors, queries, state.limits, original, 2),
         custody when is_map(custody) <-
           caller_custody(prepared, prepared_rows, length(state.donors), state.limits),
         {:ok, retained} <-
           transfer_admission(
             parent,
             state.context.source,
             state.limits,
             state.admission,
             custody
           ),
         {:ok, retained} <-
           Input.bind_admission(donors, state.limits, retained, state.context.source, 2),
         {:ok, retained} <- normalization_admission(donors, state.limits, retained, false),
         {binaries, anchors} when is_list(binaries) and is_list(anchors) <-
           Input.normalize(donors),
         all when is_list(all) <- Input.raw_contexts(state.donors, state.anchors, donors),
         {:ok, queries, observed, retained} <- query_extension(state, queries, retained),
         {:ok, terms, admission} <- decode(binaries, all, state.limits, retained),
         :ok <- source_current(state.context.source) do
      candidate = %{
        state
        | terms: state.terms ++ terms,
          donors: state.donors ++ binaries,
          anchors: state.anchors ++ anchors,
          queries: queries,
          observed: observed,
          admission: admission
      }

      with {:ok, capture} <- observe(candidate, parent),
           do: {:ok, %{candidate | capture: capture}}
    else
      false -> {:error, :parent_mismatch}
      :invalid -> {:error, :invalid_donors}
      {:error, _} = error -> error
    end
  end

  defp extend(_, _, _, _), do: {:error, :invalid_donors}

  defp custody_rows(donors, %{atoms: atoms, exports: exports}) do
    with d when is_integer(d) <- proper_count(donors, 0),
         a when is_integer(a) <- proper_count(atoms, 0),
         e when is_integer(e) <- proper_count(exports, 0),
         do: d + a + e
  end

  defp custody_rows(_, _), do: :invalid
  defp proper_count([], count), do: count
  defp proper_count([_ | tail], count), do: proper_count(tail, count + 1)
  defp proper_count(_, _), do: :invalid

  defp caller_custody(retained, rows, old_donors, limits) do
    # Caller prepare94, RAW49, transfer31, this map8; restored/candidate old spines.
    extra = 182 + 2 * rows + 10 * old_donors

    with :ok <- check(retained.decoded_words + extra, limits.decoded_words, :decoded_words),
         :ok <- check(retained.pinned_bytes, limits.pinned_bytes, :pinned_bytes),
         :ok <- check(extra, div(limits.pinned_bytes - retained.pinned_bytes, 8), :pinned_bytes) do
      %{
        decoded_words: retained.decoded_words + extra,
        pinned_bytes: retained.pinned_bytes + 8 * extra
      }
    end
  end

  defp normalization_admission(prepared, limits, retained, initial?) do
    # Two24-word state versions, normalize tuple3, this pair/ok11, fresh closure3.
    # Keeper additionally prepays query_extension's five-word success tuple.
    extra = 8 * length(prepared) + if(initial?, do: 65, else: 67)

    with :ok <- check(retained.decoded_words + extra, limits.decoded_words, :decoded_words),
         :ok <- check(retained.pinned_bytes, limits.pinned_bytes, :pinned_bytes),
         :ok <- check(extra, div(limits.pinned_bytes - retained.pinned_bytes, 8), :pinned_bytes) do
      {:ok,
       %{
         decoded_words: retained.decoded_words + extra,
         pinned_bytes: retained.pinned_bytes + 8 * extra
       }}
    end
  end

  defp transfer_admission(parent, source, limits, retained, caller) do
    words = caller.decoded_words
    pins = caller.pinned_bytes

    with :ok <- check(words + 61, limits.decoded_words, :decoded_words),
         :ok <- check(pins + 488, limits.pinned_bytes, :pinned_bytes),
         native_limits = %{
           donor_bytes: limits.donor_bytes,
           decoded_words: limits.decoded_words - words - 31,
           terms: 0xFFFF_FFFF,
           facts: limits.facts,
           text_bytes: limits.text_bytes,
           depth: 64,
           pinned_bytes: limits.pinned_bytes - pins - 248,
           output_bytes: limits.output_bytes
         },
         {:ok, report} <-
           Native.capture_terms(
             {:measure_control, [parent, source, limits, retained]},
             native_limits
           ),
         destination_words = words + report.live_words + 14,
         destination_pins = pins + report.live_pinned_bytes + 112,
         total_words = words + report.live_words + 45 + destination_words,
         :ok <- check(total_words, limits.decoded_words, :decoded_words),
         source_pins = pins + report.live_pinned_bytes + 360,
         :ok <- check(source_pins, limits.pinned_bytes, :pinned_bytes),
         :ok <-
           check(destination_pins, div(limits.pinned_bytes - source_pins, 2), :pinned_bytes),
         remaining = limits.pinned_bytes - source_pins - 2 * destination_pins,
         :ok <- check(destination_words, div(remaining, 24), :pinned_bytes) do
      {:ok,
       %{
         decoded_words: total_words,
         pinned_bytes: source_pins + 2 * destination_pins + 24 * destination_words
       }}
    end
  end

  defp call(session, message) when is_pid(session) do
    ref = make_ref()
    monitor = Process.monitor(session)
    send(session, {:capture, self(), ref, message})

    receive do
      {^ref, result} ->
        Process.demonitor(monitor, [:flush])
        result

      {:DOWN, ^monitor, :process, ^session, _} ->
        {:error, :closed_session}
    after
      5_000 ->
        Process.demonitor(monitor, [:flush])
        {:error, :session_unavailable}
    end
  end

  defp call(_, _), do: {:error, :invalid_session}

  defp observe(state, parent) do
    with :ok <- Input.donor_limit(state.donors, state.limits),
         {:ok, orders} <- orders(state.terms, state.limits),
         native_limits = %{
           state.limits
           | decoded_words: state.limits.decoded_words - state.admission.decoded_words,
             pinned_bytes: state.limits.pinned_bytes - state.admission.pinned_bytes
         },
         {:ok, native} <- Native.capture_terms(state.terms, native_limits),
         :ok <- source_current(state.context.source) do
      capture =
        Map.merge(native, %{
          session_id: state.session_id,
          parent: parent,
          donor_hashes: Enum.map(state.donors, &sha/1),
          query_hash: sha(encode(state.queries)),
          abi: state.context.abi,
          queries: state.observed,
          orders: orders
        })

      with :ok <- output_limit(capture, state.limits) do
        {:ok, Map.put(capture, :identity, sha(encode(capture)))}
      end
    end
  end

  defp decode(donors, all, limits, retained) when is_list(donors) do
    with {:ok, raw} <- Input.decode_admission(all, limits, retained),
         admission when is_map(admission) <- decode_budget(donors, limits, raw),
         {:ok, values} <- decode_values(donors, 0, []) do
      {:ok, values, admission}
    end
  end

  defp decode(_, _, _, _), do: {:error, :invalid_donors}

  defp decode_budget(donors, limits, retained) do
    extra = 12 * length(donors) + 23
    words = retained.decoded_words + extra

    cond do
      words > limits.decoded_words ->
        {:error, {:limit, :decoded_words}}

      retained.pinned_bytes > limits.pinned_bytes ->
        {:error, {:limit, :pinned_bytes}}

      extra > div(limits.pinned_bytes - retained.pinned_bytes, 8) ->
        {:error, {:limit, :pinned_bytes}}

      true ->
        %{decoded_words: words, pinned_bytes: retained.pinned_bytes + 8 * extra}
    end
  end

  defp decode_values([], _index, values), do: {:ok, Enum.reverse(values)}

  defp decode_values([bytes | tail], index, values) do
    case TermCodec.decode(bytes) do
      {:ok, term} -> decode_values(tail, index + 1, [term | values])
      {:error, reason} -> {:error, {:source_decode, index, reason}}
    end
  end

  defp orders(terms, limits) do
    terms
    |> Enum.with_index()
    |> Enum.reduce_while({:ok, [], 0}, fn {term, i}, {:ok, acc, total} ->
      if is_map(term) do
        sum = Enum.reduce(term, total, fn {key, _}, sum -> external_size(key) + sum end)

        case check(sum, limits.output_bytes, :output_bytes) do
          :ok ->
            row = %{
              donor: i,
              keys: term |> Map.to_list() |> Enum.map(fn {key, _} -> encode(key) end)
            }

            {:cont, {:ok, [row | acc], sum}}

          error ->
            {:halt, error}
        end
      else
        {:cont, {:ok, acc, total}}
      end
    end)
    |> case do
      {:ok, rows, _} -> {:ok, Enum.reverse(rows)}
      error -> error
    end
  end

  defp query_extension(state, next, retained) do
    count =
      length(state.queries.atoms) + length(next.atoms) +
        length(state.queries.exports) + length(next.exports)

    with :ok <- check(count, state.limits.facts, :facts) do
      combined = %{
        atoms: Enum.uniq(state.queries.atoms ++ next.atoms),
        exports: Enum.uniq(state.queries.exports ++ next.exports)
      }

      with {:ok, observed, admission} <-
             Input.query_admission(combined, state.limits, retained) do
        same =
          Enum.all?([:atoms, :exports], fn field ->
            Enum.all?(state.observed[field], fn {key, value} -> observed[field][key] == value end)
          end)

        if same,
          do: {:ok, combined, observed, admission},
          else: {:error, :runtime_context_changed}
      end
    end
  end

  defp output_limit(capture, limits),
    do:
      check(
        external_size(Map.put(capture, :identity, <<0::256>>)),
        limits.output_bytes,
        :output_bytes
      )

  defp external_size(value), do: :erlang.external_size(value, [:deterministic])
  defp encode(value), do: TermCodec.encode(value)
  defp sha(bytes), do: :crypto.hash(:sha256, bytes)

  defp check(value, maximum, field),
    do: if(value <= maximum, do: :ok, else: {:error, {:limit, field}})

  defp validate_parent(nil), do: :ok
  defp validate_parent(value) when is_binary(value) and byte_size(value) == 32, do: :ok
  defp validate_parent(_), do: {:error, :invalid_parent}

  defp validate_limits(limits) when is_map(limits) do
    if Enum.all?(
         @fields,
         &(is_integer(limits[&1]) and limits[&1] >= 0 and limits[&1] <= 0xFFFF_FFFF)
       ) and limits.depth <= 64, do: :ok, else: {:error, :invalid_limits}
  end

  defp validate_limits(_), do: {:error, :invalid_limits}

  defp bootstrapped do
    case :persistent_term.get(@config_key, nil) do
      nil -> {:error, :not_bootstrapped}
      context -> {:ok, context}
    end
  end

  defp source_current(%{md5: md5}) do
    if TermCodec.module_info(:md5) == md5, do: :ok, else: {:error, :source_code_changed}
  rescue
    _ -> {:error, :source_code_changed}
  end
end
