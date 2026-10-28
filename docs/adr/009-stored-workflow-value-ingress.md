# ADR-009: explicit stored workflow value ingress

Status: private codec, explicit physical capability, consumed stored9 publication/
outcome recovery and opted-in Node9 startup audit/replay implemented and locally
verified. Bounded physical history capture and coherent value hydration are
implemented. Pure logical preparation has explicit captured local-fun context;
accepted typed-command and public history services remain required.

The binary command's intent8 interprets supplied payload/result/error bytes as
logical binaries. Publication escapes a leading `FSV2` once. Inferring a typed
value from those bytes would silently change its meaning. The reviewed source
also distinguishes a corrupt storage envelope from malformed typed content,
which history hydration may report as `ERR invalid flow value`.

Intent9 explicitly means that every supplied value contains complete stored
source bytes. It preserves raw binaries, escaped binaries and typed ETF bytes
without decoding, escaping, canonicalizing or constructing a second value owner.
Malformed typed bytes can therefore survive a valid command envelope; this
codec does not grant a successful typed decode or execution capability.
Logical typed APIs require their real preparation adapter before selecting9.

The existing `PreparedWorkflowValueBatch` already owns the operation and source
presence/guard metadata. The new codec reuses that shape and contained modes
1/5/6/7. Its own canonical domain separator and `ClientIntent` version9 bind the
stored bytes and caller presence. Request ID and server preparation context stay
separate from caller identity. Complete frame/footer/EOF and intent validation
remain mandatory. Retagging an8 frame and resealing its footer cannot create9.

Binary8 APIs, frames, identity and resource/error behavior remain unchanged.
Direct decoders reject the other profile. Generic prepared decoding recognizes9
through the same bounded one-operation/metadata ownership path. No additional
payload sidecar, Document clone, Box conversion, dependency or unbounded decoder
is introduced. Stored-byte validation is not a default legacy fallback.

The selected physical capability retains FWSS7 and FWSP4. The existing first
u32 schema field now explicitly selects alias profile1 or2; the binding schema
stays1. Existing create_value_database creates profile1. The additive fresh-only
create_stored_value_database creates profile2, with unchanged configured
defaults, origin and table families. There is no implicit upgrade. Catalog
updates retain the selected profile, and snapshot install requires equal alias
profiles and defaults before changing the target.

FWVA1 records a logical binary; its existing escape-aware comparison remains.
FWVA2 records complete stored source bytes and compares them directly with the
current core field. A private bool corresponds exactly to the alias wire
version. Profile1 refuses FWVA2 even with a valid checksum. Identity, canonical
reference, incarnation, complete length/digest and immutable existence checks
remain mandatory before expiry classification. Content decoding is separate;
malformed ETF may have a valid storage envelope. FWVB1 is unchanged, so carrying
a value retains its actual alias and encoding. No prefix inference or additional
payload owner is introduced.

The additive apply_workflow_stored_value_batch_transaction explicitly requires
profile2 authority before retained or fresh inspection. The old driver stays
8-only. One private driver selects the matching codec, and the actual decoded
intent version travels with both fresh and empty publication plans. The consumed
writer checks that capability again, then atomically publishes immutable/alias/
current/every-history/core/control/outcome changes. Stored parts borrow complete
source bytes directly; binary8 retains its original escaping and charge order.
The existing FWSO response5..8 grammars retain exact count/status/time/position
validation for value intents8|9. No response format number or table changes.

Every9 outcome binds the actual profile2 manifest even when there are no value
rows. Recovery threads its already-open manifest through the existing row walk;
it prospectively admits the complete retained owner, manifest bytes and typed
header before the9-only capability parse. It does not open metadata twice or
infer capability from aliases. The existing original-policy operation tag9
remains independent of stored-value intent version9.

Node with_workflow_stored_value_limits explicitly enables9 and shares the
existing four checked value ceilings. One private bool starts false; ordinary
construction and binary8 opt-in leave9 disabled, while later shared-ceiling
updates preserve an already explicit opt-in. Legacy normal_recovery_header
remains strict. The additive stored_value_recovery_header admits9 only through
the selected Node path, with unchanged original-policy-tag precedence. Every
historical entry still runs its complete matching decoder/canonical/footer/EOF
validation; header recognition alone grants no persistence capability.

The existing startup observation transaction requires actual alias profile2
before auditing or replaying any entry when9 is selected. The additive
RedbStateTransaction::require_stored_workflow_values prospectively admits512
fixed guard/helper bytes, the exact208+group+recovery manifest, and its target-
sized ValueManifest against the same read/temporary accounts, then checks the
real manifest under the checked/fatal-latch boundary. No second transaction,
profile model or payload clone is introduced. Matching9 preparation and the real
consumed driver retain the existing scope-conversion max(V,C), complete profile
headers, reply drop-after-commit and fail-stop/uncertain semantics. Named bool
ownership is admitted only on the selected path; disabled/8 fees stay unchanged.

Full common/foreign typed ingress, accepted original history services,
shared/named/external availability, authorization, routing,
read barriers and Linux/macOS acceptance remain required. This ADR establishes
neither a public query service nor complete workflow parity.

The additive `RedbStateStore::read_workflow_history_page` captures actual FWSH
events and FWVB historical companions under one read transaction, together with
the matching current record version, current binding and group control. Five
owned models live in `wf-ports::state::history`, re-exported from `state`; backend
tables, guards and transaction types stay private. Legacy layouts explicitly
report unavailable provenance. Value-capable layouts require current/history
companions, matching value presence, bounded versions and immutable birth
provenance. Corrupt rows latch the existing fatal state; resource or stale-control
refusals leave it usable. No persistent format, dependency or writer changes.

Physical ordering remains numeric event-ID milliseconds followed by the original
event-ID bytes. A pure cursor-key encoder accepts the existing canonical u64
grammar, including zero components, without constructing a fake event. Both
directions use exclusive seeks within the complete escaped record prefix. The
page retains the complete validated extra candidate: pinned HistoryRead hydrates
limit+1 candidates before trimming, so discarding its metadata early would change
later behavior. Continuation exists only when that candidate exists and names
the last selected original ID. Core payloads are borrow-validated without copying.

Independent positive ceilings bound selected entries, point/range candidates
(including missing/end probes), cumulative inspected key/value bytes, returned
ownership, continuous work and simultaneous temporary ownership. Checked sizes
and fallible reservations precede copies. The selected Vec reserves limit slots;
lookahead stays in the page's inline Option. Current/historical scalar bindings
reuse the existing materializer. Logical fees cover named table/range/guard/key,
decoded-key, binding and complete output owners, with conservative working
allowances. These are not allocator-capacity, engine-page, RSS or speed claims.

This physical capture carries no read barrier, authorization, lifecycle/expiry
visibility, hot/cold routing or public HistoryAPI guarantee. The additive
hydration composition below retains these physical-capture boundaries.
Architecture checks keep the cursor module pure and the five models out of the
worker client facade; all existing workspace boundaries and size/complexity
limits remain in force.

`read_workflow_history_page_with_values` combines physical capture with actual
sized acquisition, consuming conversion and the existing pure HistoryValues
collector/projector. Metadata, aliases and immutable bytes share the same read
transaction and table owners. References come from actual historical bindings;
the adapter deduplicates typed identities before rendering each unique key once.
Temporary borrowed entry views retain the Page as their source. The result owns
the original page and ordered field writes, indexed over selected entries then
lookahead. No self-reference, current payload clone or stored-buffer clone is
introduced at this boundary; the projector retains its required per-occurrence
decoded ownership.

Four server-only models describe the request, limits, output and structured stage
errors. Disabled hydration and empty pages skip unused value/decoder profiles,
clock and value I/O. Nonempty legacy pages explicitly refuse unavailable
provenance. Stored-size omission includes wrappers and still authenticates full
bytes first. The alias birth must also match the captured binding birth before
expiry or omission; a valid standalone textual alias/immutable pair cannot
override that metadata witness. Malformed typed content becomes the exact field
error through the existing projector, while missing, omitted, unsafe drift,
unavailable compatibility context and operational limits keep their distinctions.

Only the new borrowed value path projects actual per-reference backend read
unavailability as sanitized storage-failure slots. It separately records the
failure and fences the store even if a later stage refuses; immutable corruption
remains a whole-read fatal error. Clock errors have their own stage variant and
alone never fence. Existing standalone full/sized read behavior stays unchanged.
Two existing sized-point helpers move to a private child; the old context wrapper
specializes the added birth witness to None with unchanged charge/error order.
No database/table is reopened and no schema, dependency or writer changes.

Independent stage profiles remain in force. A separate composition account pays
each reached stage's declared maximum work as a nonrefundable cohort plus adapter
work. Peak logical admission covers the live physical context/Page, reference,
index and view owners, actual retained plan charges and each reached stage's
temporary/requested bound. Checked arithmetic and fallible reservations precede
requested allocations. These conservative accounts do not assert exact native
capacity, engine allocation, stack use, RSS or measured speed. Public lifecycle
visibility, read barriers, authorization/routing and final page trimming still
belong to the accepted history service; logical typed preparation and remaining
source/profile/platform acceptance remain required.

The pure `prepare_original_flow_value` API now accepts borrowed logical nodes,
byte data and edge IDs, distinct from already-encoded stored input. Children
precede parents. It validates every supplied arena, plans exact ETF sizes and
logical depth, then emits one owned complete stored buffer. Root binaries keep
the existing raw/escape distinction; nested binaries use ETF. Integers, finite
float bits, UTF8 atoms, masked bitstrings, tuples, flattened proper/improper
lists and source-ordered maps retain their distinctions. Exact source key order
and semantic duplicate refusal include foreign identity and function data.

Local functions additionally require
`prepare_original_flow_value_with_init_pid`: an explicit PID node ID in the same
bounded arena supplies the encoder's captured initialization process identity.
The ordinary API returns structured `UnsupportedContext` when that capture is
needed. No global is read or guessed. Source emission substitutes this captured
PID for obsolete received creator data, including exact sizing and context
depth. Canonical function entry descriptors must agree on old index and signed
low32-bit uniqueness. The input carries immutable mathematical observations,
not proof of live VM capture, execution capability or ownership of a process.
Capturing real runtime/profile inputs remains an outer adapter obligation.

Private flat plan and map-offset buffers avoid encoded-subterm caches and
mutating shared input edge ranges. Fallible heapsort and explicit comparison/
emission continuations keep traversal iterative, including deep nesting.
Checked continuous work and prospective simultaneous requested ownership cover
named fixed headers, exact plan/order slots and bounded continuation slots.
Comparison backing is released before output emission. Only complete admitted
output lengths are reserved; the result's Vec moves into stored ingress without
a Box/shrink conversion. These accounts do not measure allocator capacity,
native frames, RSS, elapsed time or speedup.

Logical preparation carries no admission, routing or persistence authority and
does not automatically select intent9. The explicit producer still selects the
stored codec and actual profile2 owner. Frozen prepared bytes, rather than a
new runtime lookup, enter canonical identity and committed replay. Existing
binary8/stored9/decoder implementations and source error semantics are unchanged.
The real composition test distinguishes a logical tuple from a binary containing
identical FSV2-prefixed bytes through actual publication, hydration and reopen.
This component does not establish public typed APIs, native VM/profile execution,
Linux acceptance or complete workflow parity.

Root performs the current review solo under the user override. Concrete paths,
TDD stages and validation are recorded in
[the coordination journal](../reviews/workflow-completion-coordination.md).

The additive `decode_original_flow_value_compat` entry point preserves the pinned
FlowCodec `decode_value`/`decode_value_with_user_size` behavior. Valid typed ETF
remains typed. Malformed typed values, including compressed ETF and trailing
bytes, become a binary view of the entire original ETF suffix. Its returned
user size includes the ETF version and excludes only recognized five-byte FSV2
wrappers. Ordinary raw and mode1 escaped binary branches retain their own sizes.
The strict `decode_original_flow_value` entry used by history acquisition keeps
its existing corruption error and work ordering.

Both entries share the same iterative parser. Compatibility retains its moved
input Vec and pays failed parse, partial-spine cleanup, replacement node and
fallback inspection through one continuous budget. Input length and capacity
are checked before parsing. No payload clone, decompression, shrinking or budget
reset is introduced. Missing captures, unsupported profiles and operational
limit/reservation errors propagate; only the malformed-value lane falls back.
The additive API conservatively charges its own logical metadata, without
changing the strict API's existing fees. This pure component adds no runtime,
public ingress, native capture or storage authority.

The additive `prepare_logical_mutation` caller path covers all seven existing
scalar mutation kinds while keeping the original scalar normalizer unchanged.
It bounds item/common options before inspection, charges every original scalar
and logical occurrence, and uses fixed descriptor arrays to reuse the existing
schema/default/guard rules. Local options override common options. Opaque
binary, nil, boolean and i128 captures use the actual logical value encoder;
explicit arenas cover compound and foreign data with optional captured encoder
initialization PID. Supplied nil is distinct from omission and empty binary.

The result owns a complete `CommittedCommand`, optional explicit partition,
priority and original public caller evidence. No input arena or scalar buffer
is borrowed afterward. Complete stored value Vecs move into operation fields;
scalar IDs/state/type/token/partition/retry targets receive admitted fallible
copies through the existing validated constructors. `into_stored_parts` moves
these owners into explicit intent9 construction. An absent partition retains
automatic-partition intent for the outer codec's existing resolver; the domain
does not duplicate that algorithm or grant placement authority.

Aggregate raw admission includes shadowed fields, full logical pools/spines,
optional PID descriptors and the family base partition. Continuous work and
simultaneous requested ownership cover scalar validation/copies, retained
outputs and successive value preparation. Each value receives the remaining
aggregate work/temporary limits and the existing inline ceiling; successful
work is debited before moving its output. Existing identity/token conversion
liability is conservatively included. No payload shrink/Box conversion or
clone, native allocator-capacity/frame/RSS/OOM bound, or measured speedup is
claimed. A supplied stamp remains pure data, without clock or origin authority.

The real codec composition distinguishes a typed tuple from a binary containing
its identical ETF bytes. A real consumed9 store also publishes the preserved
order/items payload and hydrates its exact typed bytes after reopen. These are
component tests, without additional source-case ledger credit. Accepted typed
application services, multi-item adapters, native capture, public get/claim,
mandatory profiles and independent platform acceptance remain required.

The additive `encode_logical_mutation_frame` now consumes that caller and an
owned `LogicalMutationFrameContext`, producing one stored9 frame and the same
already-derived client intent. It resolves only an absent partition through
the existing CRC resolver. Scope, request, stamp, defaults, ordered policy
witnesses and the independent source input remain explicit observations.
The original metadata checker refuses contradictory time/presence/policy inputs
instead of repairing them. Freshness, origin, placement and authorization still
belong to the accepted service. This pure function grants no submission power.

`LogicalMutationFrameLimits` adds independent positive ownership/work ceilings.
Admission includes reported upstream preparation, incoming Vec capacities,
identity/token lengths, prospective item/input owners and partition conversion.
Witness count and spine capacity are admitted before witness traversal. Exact
frame measurement precedes its single reservation, and returned capacity is
checked as well. Complete payloads and context owners move without cloning or
another decode/canonical-buffer pass. The old binary8/stored9 entrypoints pass
no additional frame cap and retain their complete encoding order and fees.
These are conservative named-owner/work bounds, not allocator, RSS, native
frame, OOM or measured-speed guarantees. Seven genuine refusals were committed
in `eb0f03d`; all seven full tails now pass, including real auto-partition
publication and history hydration before and after reopen. The original tests
and explicit source/profile/platform acceptance requirements remain unchanged.
An additional review regression was saved failing in `ab39b14`: the enclosing
work ceiling must admit control scans before the witness-capacity loop, then
all buffer work before encoding. It now passes together with exact successful
work and one-unit-cut checks, without changing any of the seven original cases.

The six genuine compiled rejecting cases were saved in `46e53c6` before the
codec body. They now pass, including all four producer modes, independent
ownership/work bounds, exact typed/raw/escaped bytes, corruption, cross-profile
retagging, preparation identity and explicit durable-header refusal. The complete
local run preserves all earlier1,165 cases and adds only these six;1,171 Rust
tests, six documentation tests, both correctness smokes and strict quality gates
pass. These results do not add original scenario/parity or current Linux credit.

The physical predecessor's two alias-codec and four real-store cases were saved
rejecting in b09e499 before the body. All complete codec/corruption/fresh/policy/
binary-carry/reopen/snapshot/cross-profile tails now pass. One previously masked
fixture needed its witness taken from the actually installed policy rather than
the pre-install donor; its complete inverse preserves every assertion. Current
validation passes1,177 Rust cases across104 targets, six documentation tests and
both correctness smokes, retaining all earlier1,171 identities. Formatting,
compilation, strict all-target Clippy, architecture, reference/progress integrity,
documentation and offline dependency checks pass. This establishes physical
capability and binary publication under it, not stored9 publication or full parity.

The following consumed publisher's seven genuine rejecting cases were saved in
5108cf6 before implementation. All seven complete tails now pass byte-exact:
twenty mode/value combinations, mixed8/9 carries with independent source holes
and every-history bindings, terminal payload/result/error values, exact/retained/
conflict/cross-profile behavior, full FWSP4 custody and replay, profile downgrade
without alias rows, four actual resource cuts and immutable corruption. The
full run preserves all earlier1,177 identities and adds only these seven:
1,184 Rust cases across104 targets, six documentation cases and both correctness
smokes. Strict quality and offline dependency checks pass. Fifteen complete
binary8 source inverses and four moved write-helper bodies preserve the prior
path; all new and old storage tests remain saved-R0 exact. These are local
storage results, not Node9 registration, complete Elixir parity or Linux evidence.

The following Node9 predecessor's seven genuine runtime refusals were saved in
96cd5fa before its body. All seven complete tails now pass without any later
fixture or assertion change, including all contained modes and mixed8 traffic,
retained outcomes, independent source holes/every-history bindings, complete
historical corruption and no-value audit, real profile/opt-in refusal, six
resource cuts, actual uncertain commit/reopenable prefix, and all four modes
through local purge, matching receive/install and fresh restore with full FWSP4
custody. Current validation preserves all earlier1,184 identities and adds only
these seven:1,191 Rust cases across104 targets, six documentation cases, both
correctness smokes and strict/offline quality gates. Seven complete production
source inverses preserve disabled/8 behavior. This is local startup composition
evidence, not an accepted service, complete Elixir parity or Linux acceptance.
