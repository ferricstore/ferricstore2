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
NATIVE-HOME1 moves the existing Native definition intact into
`tools/policy_runtime_capture/native.exs`, loaded first by the existing wrapper.
The capture body and subsequent Budget, Bootstrap and Input load order remain
unchanged. This adds one tool-source binding without changing the native ABI,
compiler inputs or runtime behavior.
B2-ARCH-RAW-HOME1 selects `tools/policy_runtime_capture/raw.inc` as an included
part of the same native translation unit. B2-ARCH-INPUT-HOME1 selects
`tools/policy_runtime_capture/input.exs` and `Ferricstore.PolicyRuntimeCapture.Input`
for caller input admission and binding helpers. Their mechanical moves preserve
the original behavior; later owner/encoder changes require separate validation.
These are first-party build inputs, separate from the manifest's OTP header pairs.
The existing `tools/tests/policy_runtime_capture/query_admission.exs` child groups
the selected query, original-control and administrative-frontier regressions;
its module is `Ferricstore.PolicyRuntimeCaptureQueryAdmissionTest`.
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

B2-ARCH-FIRST-CONTROL1-SOURCE1 selects the private
`{:measure_control, actual_value}` request with the same three live report fields.
It measures original administrative roots through an explicit call-local work
stack independently of semantic policy depth. It preserves the old semantic
`measure_terms` behavior and all existing live-value classifications. LIFO child
visitation gives this new mode its own deterministic refusal order. Request and
report storage, declared native control fields, reached heap/backing and every
frontier allocation are prospectively admitted. Checked growth allocates and
copies before freeing the old buffer; cumulative requested bytes retain credit
for every allocation, including clipped growth. Every exit frees the owned
frontier before returning its funded report or refusal. Requested bytes do not
establish allocator overhead, total native stack/RSS or opaque destructor graphs.

Fresh capture preserves source/parent/known-limit checks and scalar input error
order, then measures the complete original limits and verified context before
preparation. Its full word/pin bank seeds preparation and survives the existing
downstream ownership checks. Extensions use the same administrative measurement,
preserving deep ignored metadata without weakening semantic policy-depth limits.
Original limits are retained and measured rather than narrowed to recognized
fields. Earlier budget exchange, crypto/RNG and complete failure/retirement
accounting remain separate duties.

B2-FIRST-BUDGET-PARENT1-SOURCE1 bounds invalid extension parents before the first
budget request: an original 32-byte binary is passed unchanged, and every other
parent is represented by nil in that request. The keeper's identity comparison
retains parent-mismatch precedence; the existing call retains invalid/dead-session
origins. Later calls still use the original parent. A frozen real send-trace
regression precedes this repair and passes with all 26 capture tests. This closes
invalid-parent graph transfer only. Valid-binary backing, per-attempt reply
accounting and fixed message/signal/crypto/retirement accounting remain open.

CAP-WIRE1 closes the first successful reply's ignored-limit graph escape. Original
raw limits retain every upstream control, input, decode and query admission. The
initial normalization admission prospectively adds eleven words/eighty-eight bytes
only for extra-key inputs. After all admission succeeds, one static-key map of the
eight validated scalar caps replaces the keeper's raw limit map; exact eight-key
inputs reuse their map. Previously paid raw graph/copy credit is never refunded.
The compiled literal path and unchanged regression pass on macOS, preserving every
prior passing capture case. This local reply-shape repair does not accumulate
per-attempt debt or establish complete public channel/retirement accounting.

REPLY-DEBT1 adds cumulative ordinary budget-reply accounting in the existing
keeper and normalization functions. The current keeper bank funds each reply
before construction and survives the next request. Initial admission reserves
one closing response, including a possible normal DOWN message. Parent mismatch
precedes source checks; a matching parent checks source before word/pin refusal.
A terminal response ends the keeper and cannot renew that reserve. The frozen
repeated-reply regression now passes on macOS, preserving all preceding passes;
the exhaustion regression advances to its later unmanaged native-entry failure.
This local fixed-packet proof does not settle caller preparation, concurrent
grants, exceptional source checks, encoder credit, ACK or retirement. Actual
compiled bodies and the EXS source receipt are bound separately from the unchanged
native library; Linux compiler/header/layout acceptance remains required.

B2-BUDGET-PROTOCOL1-NATIVE-INGRESS1 selects a staged private admission protocol.
The rejecting interface adds `tools/policy_runtime_capture/budget.inc` to the
same native translation unit, `tools/policy_runtime_capture/budget.exs` with
`Ferricstore.PolicyRuntimeCapture.Budget`, and
`tools/tests/policy_runtime_capture/budget_protocol.exs` with
`Ferricstore.PolicyRuntimeCaptureBudgetProtocolTest`. These join actual tool
source binding and repository input pins. A later instantiated tree header must
join the actual compiler/header closure; its dependency count is not fixed at
the preceding build's count.

The five regular Native interfaces are `budget_register/4`, `budget_enter/1`,
`budget_bind/3`, `budget_read/3` and `budget_control/4`. In the rejecting stage,
their loaded bodies return the precreated `unsupported_budget_protocol` atom.
Budget's `start/1`, `request/2`, `permission/1`, `issue/1`, `cancel/1`,
`abandon/1`, `settle/2` and `finish/2` return the literal
`{:error, :unsupported_budget_protocol}`. Capture does not call them yet. No
registry, resource, persistent environment, monitor or message is instantiated
by those bodies. Existing capture behavior and the frozen 28 test bodies remain
unchanged. Compiling this interface establishes no functional protocol success.

The separately released native kernel enforces one active permit with authoritative
consumed debt and disjoint unused credit in both word and pin banks. Public
sessions remain PIDs; private continuations bind the actual resource, epoch,
owner and copied request reference. An owned environment and public NIF copy and
identity APIs preserve the first request's unchanged inner `{:budget, parent}`
tuple. There is no public `enif_get_ref` extractor. The ordinary reference-copy
bound applies to the trusted pinned wrapper's actual `make_ref/0`, rather than
arbitrary external or magic references.

The native-only kernel selection keeps its implementation in `budget.inc`.
Its closed registration remains lookup-visible until the actual keeper DOWN;
an alive closing keeper cannot fall through to unmanaged legacy calls. One
pinned cleanup owner holds the permit through saved-reference clearing and
demonitor calls. A delayed callback cannot clear a newer reference or reopen
a closed session. The destructor only finalizes already unlinked user data.

Every successful native read pays for its report, including a keeper read of
a cancelled bound request. Cancellation prevents another grant; the keeper
still needs the actual consumed bank before settlement. Word and pin checks
are atomic, and unused-credit settlement preserves the unconsumed terminal
floor. The selected native cohorts do not account for the complete BEAM channel.
Direct component regressions belong in
`tools/tests/policy_runtime_capture/native_budget.exs`, using real registered
keepers and owners. Their empty-native opening bank excludes fixture storage;
they establish neither public Capture readiness nor full pipeline accounting.

The native kernel now passes all six frozen component regressions on macOS,
preserving the preceding 26 passing capture cases. The complete suite still has
eight public integration/accounting failures. Its actual strict build binds the
pinned tree header in the compiler and header closure. Parser destinations use
the public `ErlNifUInt64` type; equal-width C integer pointers are not interchangeable.
Bound-state validation precedes saved-owner/reference comparisons and any phase
advance that would prevent the initial bind. Early cancellation cleanup remains
permitted. These source and runtime results do not establish execution of internal
cleanup interleavings, public BEAM constructor accounting or Linux acceptance.

An occupied session's operational Busy refusal precedes reference, monitor and
request construction. A short global registry-lock collision has a separate
same-PID tail retry before those allocations; the real wrapper needs its own
compiled-body evidence. This supplies no fairness or latency guarantee. Admitted
calls preserve parent-before-source checks. Ordinary settlement commits debt
and releases the permit before sending its funded acknowledgment; public return
waits for that acknowledgment. Stale actions cannot release a newer grant, and
unknown issued work or owner loss consumes its unresolved promise and closes.

The separately frozen KernelProfile component batch selects a paired form of
the existing `budget_register/4`: word and pin coordinates each carry
`{spent, entry_extra, terminal_extra}`. The four numeric arguments retain their
native-only contract. The prospective profiled resource uses four immutable
trailing u64 values; numeric userdata stays 192 bytes and profiled userdata is
224, priced through the full aligned resource/monitor expression. Configured
terminal credit must survive registration, reset, reserve and settlement; normal
entry adds its configured promise before owner exposure. These component inputs
do not establish the complete public caller/keeper envelope.

The three regressions were frozen in `a1c8f5e` after a compiled 43-test run
reached the numeric-only kernel's `:stale` refusal. The separately released body
now passes all three, preserving all earlier 40 bodies and the same six public
failures in a 43-test run. Their independent numeric controls, same-operation
pin deltas, cap154 word-reset discriminator and real lifecycle observations
execute against the strict-built pinned kernel. Both configured reset coordinates
are independently source-reviewed. The tests claim no isolated pin-reset
or post-close spent-pair observation. The unchanged numeric form never accesses
the optional tail; reset and close preserve the configured flag and issued debt.
The four existing resource-lifetime functions move verbatim into the private
`budget/lifetime.inc` at the same definition point. Full public E/T, startup/parent/source/
exception accounting, metered failure prefixes and real settlement ACK remain
separate required work.

Two separately selected keeper-debt regressions now reach real query-context and
combined-output failures in a 45-test run. The first keeper tail preserves the
unchanged entry bank in each case, failing the minimum reached-work increase.
The preceding 43 bodies and their six failure identities remain intact. These
tests use the actual private extension-entry/first-tail interval before another
budget reply, with one issued encoder cohort and an independent native-tuple
minimum. Their later guards reject treating the full copy reservation as spent;
those guards, pin checks, caller-finalization, ACK and retry assertions remain
masked by the first failure. Legacy financial admission is not yet certified as
the native paid bank. Production metering and the complete caller/keeper bridge
remain separate implementation work; no full E/T or Linux acceptance follows.

The selected SourceGuard change rejects an absent pinned source module even when
it could be autoloaded. Two independently reviewed regressions now demonstrate
the old behavior: real Capture and Input calls reload authentic temporary source
code, return success and each produce one observed undefined-function event.
The 47-test run preserves all eight preceding failures and adds only these two.
The required refusal assertion fails before the later unloaded/no-autoload
checks; these are preimplementation results. The separately released body will
replace only the two existing MD5 external calls with direct
`erlang:get_module_info/2` calls, preserving loaded comparisons and error results.
Its residual BADARG and rescue allocations still need source pricing. A later
compiled-guard inspection must bind the actual executing bytecode and exclude
cooked stacktrace/normalization paths before using the raw exception operand in
complete producer costs. This selects no complete public fee or Linux result.

The two SourceGuard tests frozen in `de4aea1` now pass in a 47-test run with the
same eight earlier failures. Both existing guard expressions use the direct BIF;
rescues, Input fallback and all test bodies remain intact. The cases complete
their typed refusal, unloaded-module, empty autoload-event and cleanup checks.
Actual complete-guard disassembly confirms direct constant-argument BIF calls
without cooked stacktrace/normalization paths. Runtime-loaded module MD5s equal
the inspected BEAM identities, and same-path require caching is verified.
This satisfies that compiled premise for the executed run; raw exception,
catch/control and complete public E/T accounting remain separate obligations.

The exact staged contracts and reciprocal reviews are recorded in
[the architecture review](../reviews/workflow-completion-architecture.md) and
[Root's release record](../reviews/workflow-completion-coordination.md).
Real kernel and metered public integration require separate releases after
genuine behavioral failure evidence. Valid 32-byte parent backing before IPC,
exceptional source checks, the complete control/terminal constructor census,
encoder metering, original profiles and both operating systems remain required.
No complete public fee census, source-cost closure or producer acceptance follows
from the rejecting public Budget interface or the native-only kernel.

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

B2-ARCH-CALLER-ORIGINAL-I1-PREFIX1/P1/P2 selects plain-input preparation inside
the existing Input module and session wrapper. Actual original donor/query roots,
including ignored fields and real binary backing, are admitted before any copy.
One cumulative bank prepays the complete compact destination cohort and its
constructors. The private session transfer carries actual originals; the keeper
independently measures both original and prepared roots with sender/receiver
credit and caller construction debt. Metadata measurement and source/destination
copy bounds precede the main transfer. Both word and pin credit survive decoding
and native observation. The source-derived fixed controls and layout bounds are
conservative reservations, not measured process memory or optimization gains.
This selected prefix leaves source-anchor binding, the earlier budget exchange,
signal/timer/crypto/spawn and complete retained-allocation retirement duties open.

The selected anchor extension accepts an exact source-produced donor with its
actual live source term. It preflights that caller-owned term before encoding,
handoff or decoding, binds the actual encoded bytes and preserves ownership
through handoff. Ordinary raw references retain their route; a potentially
resource-bearing raw reference without ownership needs source-anchor context.
The pin quota covers directly known VM object/binary/MagicBinary storage.
Opaque destructor-owned external graphs can outlive their caller through a
retained anchor; their size is not established by MagicBinary.orig_size.
Neither that field nor this quota proves a total native-retention or RSS bound.
The frozen anchor regressions now pass their complete capture, mismatch, retained
context, caller-exit/GC, extension and healthy-retry assertions. Input measures each
actual source term at semantic depth zero before sizing, prepays both sizing and
encoding, and compares actual deterministic bytes before post-binding native RAW.
The keeper repeats binding for new donors and retains parallel source contexts;
historical contexts are restored without re-encoding old anchors.

B2-ARCH-ANCHOR-BINDING1-SOURCE1/P1/P2 selects checked, cumulative encoder bounds
using the actual native layout and source metrics. The normal-release continuation
bound uses the verified positive scheduler entry scaled by32; its byte-copy term
is `ceil(Ecap/32)`. The prospective maximum `Ecap = U + 1023*T` covers implicit
local-node name growth beyond the sizing maximum `U`. Actual emitted bytes still
decide equality. All reached reports, maps, source-MD5 results, trap/context/output
debt and caller/keeper copies remain additive. This does not establish a whole
process or native-memory ceiling. The reviewed sort bound still needs binding to
the executing BEAM provider; trace/control, first-budget, crypto/RNG, failure and
retirement duties remain open. Passing the twenty-test local suite accepts the
functional binding slice, without accepting the complete producer or Linux.

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
