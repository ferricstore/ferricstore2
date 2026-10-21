# ADR-008: versioned source-runtime policy capture and import

Status: the four-artifact, tests-first producer prerequisite is selected under
B2-CREATE-PRODUCER1/P1. A producer draft is under native review and is not accepted. Rust runtime
profiles, models, command/receipt formats and storage layouts remain unselected.

## Reason for the decision

The portable policy profile and binary-key ordering cannot establish the native
identity of arbitrary source-runtime terms. Pinned OTP map ordering uses atom
Eterms and, for runtime-affine keys, live node or export identity. A safety-table
ordinal, mathematical permutation or profile checksum does not prove those facts.
The same remote node name and creation can acquire another identity after its
source references disappear. Historical replay must preserve captured facts
without asking a later VM to recreate them.

This work serves required original policy observations and profiles. It does not
make arbitrary future ETF compatibility an implemented public API. Existing
source scenarios, assertions, setup and required profiles remain acceptance
inputs; an unsupported temporary profile is not a permanent scope exclusion.

## Selected producer boundary

The Rust service and pure core remain independent of a resident BEAM. A trusted
compatibility/import tool captures actual source facts. Previously unregistered
runtime identities require that producer; already registered execution and
historical replay must work after it stops. A live per-command BEAM service would
require a separate architecture decision.

The selected files are `tools/policy_runtime_capture.exs`, its native bridge
`tools/policy_runtime_capture.c`, and
`tools/tests/policy_runtime_capture_test.exs`. The two tool modules are
`Ferricstore.PolicyRuntimeCapture` and `Ferricstore.PolicyRuntimeCapture.Native`.
Root owns this ADR, tests, builds, execution and Git. The implementation owner may
write the two tools only after the coordinator freezes genuine failing tests and
explicitly releases implementation. No other first-party helper or module is
selected by the initial decision. B2-ARCH-ADMISSION-TEST1 additionally selects
`tools/tests/policy_runtime_capture/admission.exs` and its admission test module.
B2-ARCH-BOOTSTRAP-HOME1 selects `tools/policy_runtime_capture/bootstrap.exs`
and `Ferricstore.PolicyRuntimeCapture.Bootstrap.verify/1` for the existing
verification responsibility, preserving load order, context and outer rescue.
B2-ARCH-RAW-HOME1 selects `tools/policy_runtime_capture/raw.inc` as an included
part of the same native translation unit. B2-ARCH-INPUT-HOME1 selects
`tools/policy_runtime_capture/input.exs` and `Ferricstore.PolicyRuntimeCapture.Input`
for caller input admission and binding helpers. Their mechanical moves preserve
the original behavior; later owner/encoder changes require separate validation.
These are first-party build inputs, separate from the manifest's OTP header pairs.
The selected private `{:encoder_layout, E}` request to `Native.capture_terms/2`
exposes actual header-derived encoder layouts, with its own bounded control
report. It performs arithmetic only; encoder admission still requires the
selected operating conditions, actual libc correspondence and complete owner sums.
The selected `{:measure_terms, actual_value}` request measures one real graph at
semantic depth zero using the same call-local live traversal. Its three existing
live metrics include the request and bounded report envelope. It emits no facts,
hashes or encoded terms and consumes no donor-byte quota. This supplies actual
graph operands for encoder/copy admission without a synthetic donor or a configured
fact maximum substituted for an actual graph; it does not establish source-byte
binding, deferred-allocation retirement or complete wrapper accounting.

`Capture.bootstrap/1` authenticates the actual VM, source and generated-header
build before `Native.load/2`. There is no unconditional `@on_load`: an unsupported
host must receive a structured refusal before native loading or private access.
`Native.load/2` independently checks the actual BEAM package and build type;
caller-supplied ABI claims cannot establish either. The C load callback checks
available public runtime information against its compiled identity before
inspecting any private Eterm. NIF system information alone does not establish
the package patch version or release build type. `Native.abi/0` reports a
successfully checked build; `Native.capture_terms/2` accepts live decoded terms
and explicit limits. It returns bounded scalar facts and observations, rather
than caller-supplied identity labels.

`Capture.capture_epoch/4` accepts exact donors, queried facts, parent identity and
immutable limits, returning a live session and capture. `extend_epoch/4` checks
that session and expected parent before adding donors/facts. `close_epoch/1`
releases source handles and invalidates continuity. Output binds actual source,
build/session, donor/query bytes and parent. The prototype is not a Rust profile
constructor or an assignment of future production wire bytes.

Map observations are operation-specific. `orders[].keys` records the policy
validation order from `Map.to_list` and `Enum.reduce_while`. On the pinned runtime,
direct `Enum.map`/fold traverses HAMTs in the opposite direction; tests retain
that independent observation. Flatmaps agree. Frozen Rust policy-order vectors
remain unchanged; this producer prototype does not establish every merge or
nested traversal profile.

## Native lifetime and build requirements

Build against isolated exact OTP-28.3.1 release, 64-bit little-endian VM and its
matching generated/private headers. Record source, headers and binary identities;
major-version equality is insufficient. The currently installed OTP28.4.2 /
ERTS16.3.1 must actually refuse. DEBUG builds use a distinct map-hash variant and
must refuse too. Ordinary release map hashing uses seed zero; general salted
hash APIs must not silently substitute for it.

Inspect only actual live terms using exact-pin types/macros and public NIF
environment/copy lifetime APIs. An integer input is a term, never an address to
dereference. Do not retain a borrowed NIF environment after return or depend on
an unexported private runtime function being dynamically linkable. A source
keeper owns decoded terms throughout a bounded session. Immutable fun fields and
stable live node/export references may supply opaque hash-input integers; Rust
will never dereference those values.

This is per-term capture, not a whole-VM freeze. Safe-decoder/export results are
actual reached observations, not an atomic global-table snapshot. Invalidated
code/environment continuity closes or refuses the session. Closing source pins
requires a new lifetime epoch and current-closure import, even on the same boot.
Historical serialized facts remain immutable after their source handles die.

The existing pinned `Ferricstore.TermCodec.decode/1` owns safe decoding, complete
used length and compression refusal. The tool must use that actual module and
must not create atoms while querying supplied spellings or execute captured funs.
Unsupported tags and source-invalid input need distinct observations; neither
missing context nor an unavailable producer is automatically corrupt data.

Generated source, VM, headers, NIF and test output belong under
`/private/tmp/ferricstore2-policy-runtime-capture-otp-28.3.1`. No binary or copied
OTP tree is committed to the product repository. A patched emulator, another
native API or another first-party artifact requires a new concrete consultation.

## Resource and verification requirements

Limits cover donor bytes, decode/term traversal, stack/depth, facts/text, retained
source pins, scratch and output at their actual simultaneous frontiers. Refusal
and cleanup paths are included. Per-donor bounds do not bound a long-lived union
of epochs. Native heap/IPC ownership is separate from Rust logical work/scratch;
this decision assigns no new Rust tariff or unproved memory coefficient.

The selected `Input.query/2` construction guard receives already validated
remaining word and pin limits. It validates proper shape, facts, item types and
text before observations, then prepays each lookup and persistent-map insertion
with a cumulative bank across both result maps. The insertion bank uses actual
map cardinality, including the 32-to-33 factory transition. Its constants are
conservative bounds from the pinned OTP source for binary and MFA keys, not Rust
fees or measured memory. Total query text also funds sequential alignment scratch.
The caller separately funds the bounded six-word refusal channel. Full caller
ownership subtraction, compaction, transfer and keeper accounting remain open;
the local query guard does not certify those frontiers.

The selected anchor extension accepts an exact source-produced donor with its
actual live source term. It preflights that caller-owned term before encoding,
handoff or decoding, binds the actual encoded bytes and preserves ownership
through handoff. Ordinary raw references retain their route; a potentially
resource-bearing raw reference without ownership needs source-anchor context.
The pin quota covers directly known VM object/binary/MagicBinary storage.
Opaque destructor-owned external graphs can outlive their caller through a
retained anchor; their size is not established by MagicBinary.orig_size.
Neither that field nor this quota proves a total native-retention or RSS bound.
Anchor behavior remains held until its genuinely reached tests fail and freeze.

Seven selected tests cover authentic ABI/version refusal; opposite atom creation
histories and 31/32/33 maps; release hashes and full-width collision ties; source
pins through GC and close; safe decode/query origins without fun execution;
donor/parent/session/code substitution; and bounded failure cleanup. Expected
observations come from actual independent source behavior and frozen literals,
not the producer's own hash/encoder. A missing API/build result is recorded as
such, rather than as executed native behavior. Native macOS and Linux results,
source fidelity and performance measurements remain separate requirements.

## Required later integration

Production profile registration must be serialized before dependent commands.
Pre-submit cap/cutoff refusal may be no-write; an already committed registration
must durably apply its accepted or rejected administrative result and advance
its log/control obligation before retry. Current closure covers every active
runtime policy in the authoritative group/recovery cohort at a protected
snapshot/cutoff, not a caller census.

Historical retained-profile validation precedes active current-profile access.
Used-profile metadata records only a reached dependency; NonFresh, stale,
replace and earlier failures retain their lazy behavior. Persisted references to
missing/inconsistent profile rows are malformed-cohort failures; supplied
unavailable context is a separate operational refusal. Live/historical epochs
remain pinned by policies, outcomes, logs, frozen/received snapshots and future
migration jobs until actual durable release. Scoped borrowed views must own any
temporary scratch explicitly and cannot return self-referential allocations.

These requirements do not allocate future profile/frame/layout version numbers
or select the broader Rust implementation roster. B2 runtime profiles, B3 indexed
effects, B4 durable jobs, all original acceptance and the ordered M1–M8 goal remain.

Primary evidence: [OTP map hash inputs](https://github.com/erlang/otp/blob/OTP-28.3.1/erts/emulator/beam/erl_term_hashing.c),
[node lifetime](https://github.com/erlang/otp/blob/OTP-28.3.1/erts/emulator/beam/erl_node_tables.c#L903-L966),
[immutable fun identity](https://github.com/erlang/otp/blob/OTP-28.3.1/erts/emulator/beam/erl_fun.h)
and [NIF ownership](https://github.com/erlang/otp/blob/OTP-28.3.1/erts/doc/references/erl_nif.md#enif_alloc_env).
Actual source observations, decisions and execution evidence are recorded in
the completion architecture, performance and coordination reviews.
