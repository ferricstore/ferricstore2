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
         {:ok, queries} <- Input.compact_queries(queries, limits, 0),
         {:ok, donors} <- Input.compact(donors, limits, 0),
         {:ok, terms, admission} <- decode(donors, donors, limits),
         {:ok, observed} <- Input.query(queries, limits) do
      id = :crypto.strong_rand_bytes(32)

      state = %{
        context: context,
        limits: limits,
        terms: terms,
        donors: donors,
        queries: queries,
        observed: observed,
        session_id: id,
        capture: nil,
        admission: admission
      }

      with {:ok, capture} <- observe(state, parent) do
        keeper = spawn(fn -> keep(%{state | capture: capture}) end)
        {:ok, keeper, capture}
      end
    end
  end

  def extend_epoch(session, donors, queries, parent) do
    # Ask using only small scalars BEFORE sending any new donor backing.
    with {:ok, limits, retained} <- call(session, {:budget, parent}),
         {:ok, queries} <- Input.compact_queries(queries, limits, retained),
         {:ok, donors} <- Input.compact(donors, limits, retained),
         {:ok, _} <- Input.decode_admission(donors, limits, retained) do
      call(session, {:extend, donors, queries, parent})
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
          if parent == state.capture.identity,
            do: {:ok, state.limits, state.admission.pinned_bytes},
            else: {:error, :parent_mismatch}

        send(caller, {ref, result})
        keep(state)

      {:capture, caller, ref, {:extend, donors, queries, parent}} ->
        case extend(state, donors, queries, parent) do
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

  defp extend(state, donors, queries, parent) do
    with true <- parent == state.capture.identity,
         :ok <- source_current(state.context.source),
         {:ok, queries, observed} <- query_extension(state, queries),
         {:ok, terms, admission} <- decode(donors, state.donors ++ donors, state.limits),
         :ok <- source_current(state.context.source) do
      candidate = %{
        state
        | terms: state.terms ++ terms,
          donors: state.donors ++ donors,
          queries: queries,
          observed: observed,
          admission: admission
      }

      with {:ok, capture} <- observe(candidate, parent),
           do: {:ok, %{candidate | capture: capture}}
    else
      false -> {:error, :parent_mismatch}
      {:error, _} = error -> error
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
           | pinned_bytes: state.limits.pinned_bytes - state.admission.pinned_bytes
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

  defp decode(donors, all, limits) when is_list(donors) do
    with :ok <- Input.donor_limit(all, limits),
         {:ok, admission} <- Input.decode_admission(all, limits, 0) do
      donors
      |> Enum.with_index()
      |> Enum.reduce_while({:ok, []}, fn {bytes, index}, {:ok, acc} ->
        case TermCodec.decode(bytes) do
          {:ok, term} -> {:cont, {:ok, [term | acc]}}
          {:error, reason} -> {:halt, {:error, {:source_decode, index, reason}}}
        end
      end)
      |> case do
        {:ok, values} -> {:ok, Enum.reverse(values), admission}
        error -> error
      end
    end
  end

  defp decode(_, _, _), do: {:error, :invalid_donors}

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

  defp query_extension(state, next) do
    count =
      length(state.queries.atoms) + length(next.atoms) +
        length(state.queries.exports) + length(next.exports)

    with :ok <- check(count, state.limits.facts, :facts) do
      combined = %{
        atoms: Enum.uniq(state.queries.atoms ++ next.atoms),
        exports: Enum.uniq(state.queries.exports ++ next.exports)
      }

      with {:ok, observed} <- Input.query(combined, state.limits) do
        same =
          Enum.all?([:atoms, :exports], fn field ->
            Enum.all?(state.observed[field], fn {key, value} -> observed[field][key] == value end)
          end)

        if same, do: {:ok, combined, observed}, else: {:error, :runtime_context_changed}
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
