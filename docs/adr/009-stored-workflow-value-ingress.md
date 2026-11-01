# ADR-009: explicit stored workflow value ingress

## Strong current-record application query contract

STRONG-RECORD-QUERY1 connects the typed native producer to application queries
through `StrongWorkflowRecordReadPort`. A single authorized read lifecycle must
bind actual scope/placement/recovery, establish a current leader/quorum barrier,
wait for its application and the requested scoped minimum, sample trusted server
visibility time and acquire the coherent existing policy/record observation.
There is deliberately no implementation for a physical store alone. Interface
implementation and ordinary observation fields do not independently prove quorum
or production readiness; accepted consensus integration retains its existing gate.

Client positions borrow explicit group/recovery/index components. Cross-namespace
positions are incompatible, without comparing their numeric index. Empty genesis
is not applied entry zero: a minimum index zero requires an actual last entry.
The application checks the applied floor against full observed barrier identity
and the client minimum, including for absent records. Equal barrier/applied indexes
must match the complete term/leader/index identity. Later applied snapshots rely
on the accepted capability to prove incorporation of the barrier; no numeric
comparison is described as a quorum or history proof.

Typed logical partition/scope uses existing physical routing, then the returned
native point and captured logical scope must match the request. Already physical
caller aliases cannot bypass logical-scope checks. Only terminal retention hides
records at <= trusted server time; active timeout/lease expiry does not invent a
committed transition. Read, authorization, clock, barrier and decoding refusals
never become missing records. Scoped freshness comes from actual applied control.
The existing native result owner moves into application output; hidden records
drop without exposing their body. Fallible bounded scope copies and routing plus
native acquisition/preparation share whole-call work/temporary ceilings. The
output cap conservatively also covers intermediate native preparation before
terminal filtering. These are logical estimates, without allocator/RSS or speed
claims. No new dependency, intent, wire, storage or snapshot format is introduced.

Real metadata-capable native creates/cancellation and scoped reopen execute under
an explicitly controlled capability provider. This exercises the application-to-
storage contract, not a real quorum. Original Flow.get option/return normalization,
payload/named hydration, accepted quorum runtime, transports, projected queries
and complete source/platform/performance acceptance remain required. No source-case
credit follows from these supplemental native controls.

## Native typed query preparation

NATIVE-QUERY-DATA1 composes the coherent record/policy read with explicit-context
original metadata decoding and the existing query field/scope validators. Preserve
native u128 ordering in typed output: the original covering-row internals encode
u64 sequences, so that frame cannot represent every native sequence. No legacy
physical Locator is fabricated. Actual native core/value/lifecycle remains distinct
from the unchanged caller metadata document and from current-policy configuration.
Captured lifecycle and index hints do not replace either authoritative observation.
Missing optional fields stay missing; unknown logical routing is not unscoped.
Both physical and logical scope conversions must agree with the native partition.

Metadata bytes move into one existing document allocation. Bounded transient flat
descriptor vectors borrow binary leaves and reuse complete original field validators.
Verified Complete fields and the existing Original policy index/normalizers supply
at most three attribute names, one state metadata name and actual generation/profile.
Only those small names are copied; full policy backing is dropped after projection.
This is an ownership observation, with no measured speedup or RSS claim.

Whole-parent work includes the completed native read and all later decoding,
validation and policy inspection. Input owners, prospective decoder auxiliary
buffers, descriptor spines, policy scratch and output names share temporary limits;
final output excludes discarded policy backing. The metadata bounded entrypoint
sums requested auxiliary buffers plus relocation conservatively before each central
reserve. Existing parser bodies and old entrypoint work semantics remain shared.
PolicySchemaCharge::with_scratch_limit checks the existing prospective scratch
tariff before allocation, preserving default new() g/p semantics and allowing
zero scratch for allocation-free inspection. Account mechanics move privately to
admission/charge.rs; public paths and dependency boundaries stay unchanged.

This is data preparation, without service authorization, expiry visibility, query
plans/read barriers, native derived-index publication or source-origin proof.
Intent22 remains original caller DATA. Those integrations and full source/platform/
measured-performance acceptance remain required. Ordinary wide-sequence/scoped
input controls are labeled as such and earn no source-case or persisted-wide credit.

## Coherent native current-policy record observation

NATIVE-RECORD-POLICY1 adds `WorkflowRecordPolicyReadPort` over the existing native
record-capture capability. Core, caller metadata, lifecycle/value binding, control
and the actual type's current policy use one committed read transaction. NotRead,
Absent and Present remain distinct. Historical core selection returns current
policy on this revision; it does not reconstruct historical policy backing or
replace the immutable birth witness. Actual group/recovery/tenant/type select the
policy key. Reuse the existing Complete/Original decoder, type binding and error
lanes through private admitted decoding; no second parser or format is introduced.

The old reader retains its four probes. The extended reader pays one reached
policy probe, aggregate physical/returned/work/temporary owners and schema total
within remaining whole-read work. Actual configured schema ceilings cannot be
raised by this API. Full backing is selected output, with one original-byte copy
and moved typed Complete output, without another whole-policy clone. Current
policy and birth capture may intentionally differ after a policy update.

This is physical data acquisition for native query integration, not original
logical routing/source origin, native derived-index publication, visibility,
authorization or a consensus read barrier. Intent22 caller operands, binding and
snapshot formats remain unchanged. Rich original public/native write preparation
and authoritative metadata/index derivation remain required coupled work.

## Original record metadata data operand

SOURCE-METADATA-CAPTURE1 selects `metadata::record` as the data operand for the
remaining native original-record integration. It reuses the existing FSV2/ETF
encoder and parser. The private envelope is a3-tuple: binary
`wf.record.metadata`, integer1, and a map with at most12 closed binary field
names. Original logical partition/scope, parent/root/correlation, attributes,
state metadata, projection configuration and original lifecycle/history values
retain missing, nil, empty and arbitrary captured value distinctions. Field
contents are opaque data; no business normalization or source validity is
inferred. This is not the original Elixir record wire or a publication proof.

Preparation borrows the existing flat arenas and optional captured initialization
PID. One admitted byte owner moves into a document;12 fixed node-ID slots address
its actual fields without another value tree or field-payload clone. Encoding
and decoding retain original foreign-context distinctions; foreign identities
never execute. Debug output redacts captured values. The decoder requires its
explicit actual compatibility context and never synthesizes source VM tables.

Schema work precedes the shared encoder using its remaining account. The new
crate-private measured decoder returns actual consumed work from the same parser,
then schema inspection continues the same account. Existing public decode bodies,
profiles and fees remain untouched. Preparation separately reserves4096 logical
control/temporary bytes in addition to the shared encoder peak. Decoding retains
the shared moved-input capacity, node, edge, depth and work ceilings plus paid
fixed control. These are logical resource contracts, not allocator/RSS/stack
or measured-performance claims.

Native captured creation/mutation ingress, incarnation/point binding, snapshot
publication/import/recovery, actual scope/routing and exact QueryRow validation
remain required coupled integrations. An absent native sidecar is unknown; it
cannot be replaced with an empty capture. A present empty map is only captured
data and establishes no original source origin. Do not enable a service from
this codec or claim complete original-source capture/parity/platform acceptance.

## Complete record-capture representation

SOURCE-METADATA-BINDING1 selects `state::value::record_capture` for the complete
native data envelope. `FWMC1` contains an explicit big-endian u128 incarnation,
a length-delimited existing FWSR2 core snapshot, a length-delimited original
metadata operand and SHA256 of the complete prefix. The core carries the exact
group/recovery/tenant/physical partition/record ID and version. Scanning requires
all five point components, the exact incarnation and exact core version. A
Created core must agree with its original state-entry sequence; subsequent
versions retain their original birth even when current ordering changes.

Encoding measures and appends the existing core directly into one final output
allocation. It borrows the prepared original metadata owner. Scanning borrows
both slices from one donor without materializing another core or metadata tree.
Separate total-byte, metadata-byte, work and temporary limits apply prospectively;
work charges a conservative 4096 plus 128 times complete wire bytes. Encoding
admits 4096 plus final requested bytes; borrowed scanning admits fixed 4096.
These are logical resource contracts, with no measured speedup or RSS claim.

The metadata slice remains opaque. Its stored discriminator and checksum are
checked, but typed ETF/schema/source validation still requires the original
metadata decoder and explicit compatibility context. A checksum proves no origin
or authorization. This representation does not add a database family or enable
a live profile. Existing manifests, writers, record codecs and recovery continue
to refuse the new format. Native original caller identity, committed ingress,
atomic publication, metadata-aware history/index updates and complete snapshot
installation/recovery are still required coupled integrations. No standalone
metadata attachment API is introduced; missing legacy capture remains unknown.

## Native captured-metadata data profile

SOURCE-METADATA-NATIVE1 adds explicit intent22 over one verified stored9 batch
and ordered optional metadata caller operands. The FWMD1 codec derives actual
caller identity from verified inner caller identity and every metadata operand's
presence/length/bytes. Generated time, target guards, policy witnesses and
configured defaults remain outside caller identity through the existing inner
codec. Producers must supply original caller data here, never generated record
postimages. An absent operand carries actual prior data; a successful birth
requires an explicit capture. Failed/no-op items cannot attach metadata alone.
The four contained reducer profiles keep their declared response grammars5..8.

Fresh `create_metadata_value_database` selects alias2, full-history layout8 and
binding-format2. Existing manifests and encoded binding-format1 data retain their
meaning. Metadata commands refuse legacy authority before retained inspection.
Legacy value mutations and Claims in metadata authority preserve actual captured
bytes. This is a private native data extension; it does not implement original
public field normalization, logical partition/scope routing or typed ETF/source
validity. Those adapters and their actual compatibility captures remain required.

FWMB1 wraps the unchanged FWVB1 lifecycle/reference bytes, exact core version,
SHA256 of the actual existing FWSR2 core, opaque metadata bytes and checksum.
Current/history keys already select the native point; the core digest covers
its complete namespace, ID, version and ordering without duplicating the core.
Created incarnation must match state-entry sequence. Same-guard readers validate
companions before mutation, while reopen and snapshot validation reject crossed
core digests, missing captures or scalar-only bindings in this profile. A checksum
and native association establish no original logical routing or source origin.
FWMC remains the complete data exchange/inspection format; persistent bindings
use the compact digest representation to avoid another complete stored core.

Existing core indexes, core rows, complete native history, immutable values,
aliases, metadata bindings, retained outcomes and control use one consumed Immediate
transaction. Output/admission failure aborts the whole publication; an uncertain
sync fences the owner and requires reopen/reconciliation. Snapshot export, receive,
install and reopen share complete companion validation and require equal binding
formats; a snapshot cannot implicitly upgrade a scalar-profile target. Scoped
metadata reads use one coherent read snapshot, independent copied-byte/work/
temporary limits and return owned opaque bytes. Legacy capture is unavailable.

The outer frame and nested preparation share one work/owned account. Metadata
selection uses an admitted per-query cache and moves/shares Arc<Vec<u8>> owners
across repeated successful targets. Failed items never update that cache. A bulk
publication reads its immutable manifest format once. Large temporary value
wrappers are borrowed; no measured speedup, allocator/RSS or native Linux result
is claimed. Original metadata-derived indexes, exact QueryRow validation and
public caller/context/routing integration remain open. Accepted Node/Raft/runtime
profile22 dispatch is also required; retained request replay here is native M2
transaction evidence, not accepted consensus startup or production service proof.

## Existing stored-value integration

Status: private codec, explicit physical capability, consumed stored9 publication/
outcome recovery and opted-in Node9 startup audit/replay implemented and locally
verified. Bounded physical history capture and coherent value hydration are
implemented. Pure logical preparation has explicit captured local-fun context;
accepted typed-command and public history services remain required.

The pure FSH2 history codec now consumes explicit captured snapshot fields and
the existing ordered normalized metadata. It emits only per-event data; immutable
id/type/parent/root/correlation values are restored from an explicitly resolved
binary context at decode. This is a data codec, with no read or publication
authority. Source record capture, atom/binary context lookup and record metadata
JSON injection remain outer integration obligations.

One shared grammar measures before fallible reservation, then emits into the
same complete output owner. Arbitrary signed/padded integer captures retain the
source's value+1 varint behavior. Decoding keeps the original ten-byte/70-bit
bound, permissive nonminimal varints and unknown flags, exact EOF, defaults,
nil event and filtered nullable metadata pairs. Malformed frames return empty
fields; operational resources remain errors. Complete validation precedes field
owners. Binary/context fields borrow their inputs; decimal text uses fixed
scratch and fallibly reserved flat owners. Independent input, pair, encoded,
decoded-output, work and requested-temporary limits are prospective.

Eight real rejecting cases were saved in `f217eee` before these bodies. Their
unchanged full tails now pass locally. A separate genuine regression saved in
`bab403e` precedes exact integer/float equality in the pure flag helper; signed
zero, fractions, wide powers and neighboring integers retain the source numeric
comparison without integer-to-float rounding. Wide-carry and complete signed
integer-exponent properties are additional after-body conformance. Float scratch
is fixed and explicitly admitted; the ordinary integer path keeps its old body.
Three independent FSH2 recipes preserve the pinned terminal/ref/meta,
compact/context and multibyte scenarios. The native NIF-versus-Elixir comparison
has not run here; native capture/profiles, service/durability integration and both
platforms remain required. Logical counters imply no native allocation, RSS,
stack, OOM or speed guarantee. The older stored history and value codecs are
unchanged.

The explicit bare external-term APIs now expose the shared `TermCodec` grammar
without interpreting FSV2 storage wrappers. `prepare_original_external_term`
and its initialization-PID variant reuse the same admitted layout, ordering,
scalar/foreign writer and iterative continuations. One literal-header emitter
writes either stored FSV2+2+131 or bare131; binary roots use ETF in the bare entry.
`PreparedOriginalExternalTerm` owns that uncompressed output and its work/peak
metrics, with distinct byte accessors. No intermediate wrapped owner, prefix
drain, rebasing, duplicate encoder or payload clone is introduced.

`decode_original_external_term` moves the same input into the existing strict
census/build/equality parser at version offset0. It checks the selected safe
tables/captures and complete EOF, rejecting compression and trailing bytes.
It never selects stored/raw-binary inference or compatibility fallback.
Existing error/resource vocabulary stays explicit; `InvalidFlowValue` denotes
the shared invalid-term lane here. The document's spans address its complete
original input under either explicitly selected decoder. No existing stored
codec, publication, value interpretation or fee is changed.

The two complete pinned `TermCodecTest` scenarios now run through this actual
shared implementation. Their source/helper snapshots are separately preserved
under `tests/reference/elixir-dependencies`, with hash and closed-inventory
checks. The frozen480-file/5858-declaration primary manifest stays unchanged.
These two dependency cases still require native registration/compiler lineage,
required profiles and final acceptance-inventory consolidation; no fabricated
main-ledger ID or full-parity credit is added. Seven genuine codec refusals and
a real preservation failure were committed in `2c6f9a3`; a later unknown-file
regression was saved in `865af55` before the inventory guard. All complete local
tails pass. Captured foreign data and independently generated valid compressed
fixtures do not establish a native VM/profile, allocation/RSS/speed or platform
theorem. Existing storage and accepted-service gates remain required.

The separate pure history metadata normalizer now covers original
FlowCodecTest442/assertion443 through captured logical inputs. Map enumeration
order, binary/atom keys, nil-to-empty, other atom spellings, arbitrary integer
decimal and finite float text are preserved; unsupported compounds are ignored.
One `OriginalHistoryMetadataField` spine owns numeric/Latin1 conversions and
borrows unchanged binary/UTF8 text through Cow. Input, rows, exact output,
continuous work and simultaneous requested owners have independent bounds.
Selected capture errors remain distinct from ignored classes. No term capture,
storage read, publication, atom interning or public query authority is granted.

Float text uses the source [short notation rules](https://beta.erlang.org/docs/28/apps/erts/erlang.html#float_to_list/2)
and [Ryu halfway rounding](https://github.com/ulfjack/ryu/blob/master/ryu/d2s.c).
An independent formatter comparison caught254 standard-Rust odd/even tie
differences among999504 finite samples; a separate genuine failing case was
committed before correction. Exact normalized binary/decimal midpoint checks
choose the even significand. One standard digit conversion feeds both notation
choices; a normal power-of-two's narrower lower interval must also admit the
neighbor before selecting it. A second genuine failing case precedes that guard.
The extended independent probe reports zero mismatches across1028162 finite
samples, retaining its earlier254 ordinary and two power-boundary failures.
Fixed length is measured before construction. A32-byte private text
buffer suffices, while the conservative control fee stays unchanged. Sampling
and round-trip properties are mathematical evidence, without native VM/profile,
allocator/RSS/OOM, measured speedup or independent Linux certification.

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
