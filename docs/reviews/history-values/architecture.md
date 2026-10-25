# History values architecture review

Root created this approved feature review home after saved metadata f68b22f.
ARCH approved exactly these three document paths at metadata/architecture.md
48629+2154/SHA256 449ec2dcce99d5ccecec16d9524e15ed49dad377e10f75e9bef7771ab14f3515.
This document authorizes no production/test file, model, API, helper or workload.
The complete hydration contract and concrete artifact roster remain pending.

Pinned HistoryValues production is 107 lines/3,445 bytes/SHA256
d287d697ebc67daab0f731beed8572f483f84c4aaad340d67946a1760274ae32.
Its test is 47 lines/1,401 bytes/SHA256
85719d7dbdcbebadd2ed76fa5020b7bba5c2d482ac77ddd5716a88740bc6e041.
Source IDs: 7486b505b2ba580e, bdafcf374348156d, 381f48c9d21d1d52.
All three cases/four observations, async:true/flow/no local setup, remain
unmapped. Complete successful FSV2 typed-ETF decoding and real storage-read/
routing responsibilities are required along with original error/omission cases.
No error-only proxy, fake storage success, private FWPV substitution, unearned
resource authority or full parity claim is selected.

Root owns shared policy/coordination/Git and all compiler/VM/test workloads.
Only the three exact history-values role notes are active append homes after
this custody switch. All metadata, score-bound and legacy notes retain their
original immutable paths and bytes. Source geometry, architecture consultation,
genuine saved RED before BODY, reciprocal review and Linux/macOS duties apply.

## HISTORY-VALUES1 contract audit and compatibility decision

This is a source/contract consultation, not a production creation or activation
release. The three review homes approved at MetaARCH48629+2154 remain the only
new artifacts approved by that decision. Root's explicit cutover makes this
architecture file the sole architecture append home; all prior role notes stay
immutable. No source, test, model, helper, dependency, schema or port has been
created by this review. Root retains all execution and source-write custody.

I read the complete pinned HistoryValues107/3445/d287d697 and its immutable
test47/1401/85719d7d, the complete ReadResult31/1192/54bd71ef and TermCodec22/
fa1f00df, the relevant value wrapper functions in Codec.Support125..180, and
Codec's actual delegations. Support's complete-file identity is680/67ea918f;
Codec's is1145/8e31070c. These full-file hashes do not imply that unrelated
record/sidecar functions were re-audited. I traced ValueStore's actual read,
filter, result restoration, nil-only LMDB fallback and locator/materialization
paths, plus Keys' reference predicate and Router's batch/file-reference/routing
paths. Current complete-file identities are ValueStore727/22731/0c4dcc8333297cc6cfd19757e7010e3dd8fefc6ff775e693e6eeb140537760b2,
Keys937/32363/bf9cbe380b9a59ef275a7efe9f94b8ee86ff10d2a7c4162eeb3a6dc24c6d8349
and SlotMap119/3590/7ba7200b390485a97d0aca9961f35004ea41733f8b1bcf71de0fbc6ee1620cd7.
Source functions, rather than similarly named neighboring helpers, define the
following contract.

History is traversed in input order, and references within each entry are
payload_ref, result_ref, error_ref. First-occurrence uniqueness precedes the
binary/nonempty/65535-byte filter. Invalid nonempty reference strings that are
not actual Keys.value_key? references still receive a missing slot from
ValueStore; a nonbinary, nil or empty field itself is unchanged by the overlay.
The maximum history reference size is Router65535, not ValueStore's separate
public option validator's4096. A larger binary reference produces the exact
kind-specific key-too-large field error without a read of that key.

ValueStore preserves original positions while filtering to actual p/r/e/s/n
value keys. No eligible references means no Router call. An eligible primary
nil alone is considered for fallback; a primary storage failure, binary value
or file reference is not silently replaced by a fallback read. The misleading
private name flow_generated_payload_value_ref? delegates to Keys.value_key?,
so fallback is not limited to the payload kind. Actual fallback groups by the
key-routed LMDB path, decodes values/locators, reads real cold/history/WARaft
locations and materializes blob values. These are effectful storage duties,
not calls to the pure hydration projector.

There is an observable pinned cardinality asymmetry. If all references are
eligible, a short or extra primary list bypasses the exact-sized fallback
guard; HistoryValues later zip-truncates it with the references. With a mixed
eligibility mask, a mismatched returned list instead restores all nil slots.
A non-list source result is not automatically a fabricated per-key failure.
The adjacent ValueHydration helper has a different explicit mismatch policy.
It must not be substituted here. A new real Rust adapter should guarantee an
exact slot for every requested key; the compatibility collector must preserve
the pinned HistoryValues branch behavior when independently testing malformed
source-shaped replies. Any deliberate change requires a separate documented
compatibility decision, not an assumed shared batch helper.

The overlay processes payload, result and error in that order. Missing puts
the kind value to nil and its missing flag to true. Storage failure only puts
the sanitized kind_error; file-reference metadata only puts omitted and the
stored size. A binary larger than max_bytes is omitted using its STORED byte
count, including FSV2 framing. At or below the cap, success puts the decoded
value and stored size; corrupt typed data only puts the invalid-flow error.
Unknown read-result classes are unchanged. None of these branches implicitly
clears old payloads, missing/omitted flags, sizes, errors or unrelated fields.
Empty history and disabled values bypass the context entirely. The file-ref
request threshold is1 when max_bytes<1, otherwise max_bytes+1, using checked
Rust arithmetic with a distinct operational overflow/limit outcome.

FSV2 mode1 strips exactly one five-byte escape header. Mode2 invokes the actual
TermCodec safe ETF decoder and requires complete input consumption. Raw bytes,
incomplete magic and unknown FSV2 modes remain raw bytes. Compressed ETF131,80,
truncated/malformed ETF, unsafe new atoms/exports and trailing data refuse the
typed value. STRING_EXT denotes a byte list, not a binary. Lists can have any
typed tail; integers are arbitrary precision; float bits include signed zero;
map uniqueness uses source exact semantic keys, not encoded-byte equality.

## Actual original evidence read for this contract

I independently read and hashed the ENTIRE Root original-case packet at
8710812+5000/SHA1205669989a9601d9b8333f55e4d6914704d24d73a5fc8ed84f9668094b9d26a.
All three unmodified cases/four observations pass; total3, failures0,
excluded0, skipped0. Their async:true/flow/no-local-setup identity is retained.
The actual Router beam path and max_key_size65535 are shown, with five visible
unsuppressed redefinition warnings. This is original helper evidence; no store
was started and no successful Rust/bulk-I/O behavior follows from it.

I also independently read/hashed Root's complete33 supplemental observations,
8722488+11999/SHAc834ada010d2aa7ad344c90619fa8778b9fe703b2830556faaf3f0c1e283f4f4.
They establish exact FSV2 escape/raw fallbacks, complete safe ETF/error rules,
an184-byte compound with arbitrary-width integer, opaque binary, negative-zero
float, nested map, tuple, proper/improper list and three-bit bitstring, plus
stored184/183 boundaries, file-ref size100, empty-at-zero, stale-field overlay
preservation, ref65535/65536 and no-context disabled/empty controls. Unknown
atom remains absent both before and after refusal. Original reference, PID,
exported function and compiled-pattern identities roundtrip. The extra
function/pattern invocations are real supplemental observations, not new
mandatory workflow assertions or product execution authority.

The complete10 map/float observations at8738388+5309/
SHAa24416931b8a3773149ec3f7c01570bebcdc6ab92c758d1f95255698da58c085
were independently read/hashed. Small/integer/padded-big aliases, Latin1/UTF8
atom aliases and STRING_EXT/list aliases are duplicate map keys and refuse.
Integer1 and float1.0 are distinct. Under pinned OTP28, +0.0 and -0.0 are also
DISTINCT exact map keys; both bit patterns survive. NaN and both infinities
are invalid ETF floats. Rust float ==, numeric coercion, raw encoded-key
comparison or zero canonicalization cannot implement this key predicate.

Root executed all three packets through its existing inline pinned runtime
wrapper; I ran no compiler, VM, benchmark or source workload. Their real Router
and codec inputs do not establish bulk-store integration, full Mix/setup
profiles, Linux, performance or product completion.

## Foreign identity is data, not a new BEAM execution requirement

I initially treated the successful supplemental compiled-pattern operation as
requiring a BEAM resource-owner bridge before common hydration could proceed.
Root correctly rejected that extra product predecessor. This paragraph
supersedes that overstrong direct-message interpretation.

I read docs/workflow-test-parity.md, architecture's purpose/read/storage
boundaries, transport-design50 and HTTP60. The product excludes arbitrary
user-code replay. Public payload/result bytes have an explicit opaque base64
representation; that does not grant a foreign execution engine or generic ETF
public DTO. The immutable manifest contains no term_codec path or compiled
pattern mention. The three actual HistoryValues assertions inspect sanitized
failure, exact stored size and corrupt-value/no-payload behavior. Their source
and identities stay intact; supplemental vectors increase our implementation
evidence without silently expanding the user's runtime scope.

The selected compatibility direction is therefore explicit typed foreign
identity-as-data: PID, port, reference, external function and local closure
remain distinguishable from ordinary data and from one another. Their fields
and source provenance are preserved; they are not collapsed to Other, strings,
ordinary Rust references or executable Rust callbacks. Rust does not promise to
invoke the BEAM resource/function. This is an explicit representation/execution
boundary, not evidence that every existing source profile is passing. A new
wire representation or exclusion of an included case still requires its own
specific mapping/review.

Safe existing-atom and active-export checks remain context-sensitive source
semantics. The pure compatibility codec may consume explicit captured tables,
with honest acquisition preconditions; those tables are not authorization,
storage availability or proof of an actual live VM. It must not manufacture an
allow-any context from the bytes being decoded. Matching-local native atom
indices and LOCAL_EXT need a matching captured decoder context; absent support
returns a DISTINCT operational profile/context outcome. It must not relabel
valid unavailable foreign encoding as InvalidFlowValue or count an unsupported
profile as full parity. That explicit lane can remain open while common typed
data, real publication/read and hydration advance. No BEAM execution dependency
is selected.

Pinned primary cross-checks: OTP28.3.1 erlang.erl1297..1351 documents safe/used;
the locally pinned external.c4466,4937..4965 performs existing atom/export
checks,4740..4789 resolves local references, and5187/6210..6269 handles LOCAL.
The official pinned ETF document was actually fetched via GitHub, blob
59b96db9f6243b193de0d5c505064ec84a13a3a4: MAP481, STRING503, LIST515,
EXPORT662, NEW_FLOAT686 and LOCAL754. Its LOCAL hash is matching-format
checking, not forgery resistance. The unpinned live OTP29 web page was only
discovery context and supplies no OTP28 equivalence claim.

## Complete end state and the next implementable prerequisite

Actual Rust source inspection finds no hidden implementation to activate.
wf-ports/blob.rs, wf-storage/blob.rs, wf-application/queries.rs and
wf-projection/history.rs are three-line reservations. The real
RedbStateTransaction reads core record/policy/outcome/control/due state, not
source value references. FWPK's closed families0..7 and its physical inventory
decoder have no value catalog. FWSH stores reduced typed HistoryEntry values,
not arbitrary source fields or payload/result/error refs. ActiveValueRef itself
explicitly grants no stored availability. A new isolated redb table or an
arbitrary history input fixture cannot manufacture the missing product path.

The dependency sequence is concrete and remains the full feature objective:

1. A genuine generic FSV2/typed-ETF codec under the existing domain value
   responsibility, independent of the closed policy envelope. It owns encoded
   buffers and flat typed node/edge arenas with offsets, avoiding dangling
   borrows into temporary read Vecs, self-reference and recursively owned
   unbounded trees. The output borrows its document, not a dropped descriptor
   array. Captured safety context is checked during decoding; foreign data does
   not require retaining or executing a BEAM owner after the check. All valid
   common roots, aliases and semantic map-key identities above are necessary.
2. Actual workflow-scoped value publication/catalog and routed ordered reads
   through ports/storage. The selected bounded inline design must publish bytes
   before accepting their references, include them in snapshot/restore/recovery
   and retention reachability, and preserve tenant/group/recovery boundaries.
   Route selection is the actual stable hash-tag crc32&1023 plus captured
   slot-to-shard mapping or an explicitly reviewed equivalent Rust placement
   contract, not randomized hashing. Missing, unavailable/corrupt storage and
   proven omission metadata remain distinct until the projector's sanitization.
   Real adapters return exact slots in requested order and execute nil-only
   fallback against real persisted data where that physical fallback exists.
   A selected redb equivalent need not copy LMDB/Bitcask/WARaft byte formats, but
   must preserve their workflow availability/failure/size/recovery guarantees.
3. Application query orchestration over that real read capability and the pure
   domain overlay. It keeps event/field/ref order, source masks/cardinality,
   payload/result/error semantics and all unrelated fields. A coherent local
   snapshot is not a leader/quorum barrier. Public strong reads, accepted
   routing/consensus and projected watermark/authorization retain M2/M3/M4/M6
   predecessor gates; a test callback is not their implementation.

The next concrete codec consult is anchored at existing
crates/wf-domain/src/value.rs plus the candidate private feature
value/original_flow.rs and responsibility children
value/original_flow/{model,context,scan,decode,encode}.rs. These names are
PROPOSALS, not a creation grant. The concrete input/output direction sent to
Root and both peers is:

    decode_original_flow_value(encoded: Vec<u8>,
        context: &OriginalValueContext<'_>, limits: OriginalValueLimits)
        -> Result<OriginalValueDocument, OriginalFlowValueError>

OriginalValueDocument owns the input bytes and flat indexes. A borrowed
OriginalValueRef addresses typed nodes, including foreign identity DATA; no
policy Raw::Other or private FWPV is substituted. Context is an input checked
at this compatibility boundary, not a returned lifetime/BEAM owner requirement.
Source InvalidFlowValue, unsupported matching-local profile, resource limits
and real reservation failure must remain separate lanes. The encoder, if
selected for actual value publication, must preserve the one-level binary
escape and typed-value rules; it is not inferred from a decoder-only proposal.

The exact minimum public node/view/context fields, methods, private byte-family
helper roster, work/depth/item/encoded profiles and grouped test target remain
the immediate named precreation decision. Neither the private policy parser's
64-depth/4096-span operational profile nor the six-file candidate gives that
approval: it returns a policy envelope, excludes LOCAL/native-index profiles
and exposes neither generic value ownership nor a real reader. DOMAIN/PERF
were directly consulted and are refining the concrete data-only inventory.
This is a precise API/artifact selection task, not a new foreign execution
bottleneck or an instruction to build an error-only hydration shortcut.

For the later catalog stage, existing ports/blob.rs and storage/blob.rs are
responsibility homes; catalog/read/write/recovery children require exact
separate selection with the real state schema/snapshot owners. Later
application queries/history_values.rs and domain history/values.rs are also
candidate homes, not a broad permission to create them. Shared policy/ADR and
architecture-negative-test footprints belong to Root after each exact batch.
Natural source400/test600/function100, functional-core purity and disjoint
custody apply to every stage.

The final acceptance inventory must keep the original three names/four
assertions in source order, then execute full successful raw/escaped/typed
values, all exact size/overlay/ref branches, collection/dedup/filter ordering,
the two cardinality branches, nil-only fallback and real routed reads. Actual
persist/reopen/snapshot/recovery/failure tests must prove returned data and
omission metadata come from production storage and preserve scope. Each
implementation stage needs its own callable honest RED saved before BODY,
unchanged-oracle GREEN and ordinary reciprocal source/resource review. No
three-case ledger credit is released from this consultation: all arrays remain
empty until the complete selected hydration route passes. Native eight failures,
initialR/fullE-T/public settlement, Linux and the remaining original/runtime
profiles stay independently open.

## A-HISTORY-VALUES-CODEC1-R0: exact rejecting creation batch

Root has read the preceding direction and requested the finite callable R0
batch now; future parser implementation or catalog/LOCAL work must not hold it.
This decision selects ONLY the following named R0 artifacts and contracts,
subject to Root's explicit writer release/custody record. It does not select a
parser BODY, encoder, live store, new dependency or public activation.

The eight exact source/test paths are:

- EXISTING crates/wf-domain/src/value.rs: only a private original_flow child
  declaration and public reexports of the six models and one function below.
- NEW crates/wf-domain/src/value/original_flow.rs: private model child, its
  reexports, documentation and the one honest rejecting decoder entry.
- NEW crates/wf-domain/src/value/original_flow/model.rs: exactly the six models
  below and four bounds-checked document view methods. No parser helpers.
- NEW crates/wf-domain/tests/original_flow_value/main.rs: common imports and
  private wrappers/scalars/structure/context module declarations only.
- NEW crates/wf-domain/tests/original_flow_value/wrappers.rs.
- NEW crates/wf-domain/tests/original_flow_value/scalars.rs.
- NEW crates/wf-domain/tests/original_flow_value/structure.rs.
- NEW crates/wf-domain/tests/original_flow_value/context.rs.

This obeys the repository's multi-file integration-target directory rule.
There is no flat tests/original_flow_value.rs, empty future parser child,
fixture/probe file, lib seam, Cargo dependency, RawFlowPolicyValue change or
policy-parser relocation. value is already a classified pure root module.
Root owns any actually required shared classification/checker/ADR change after
the source owner stops; no such extra artifact follows by inference here.

Exactly six public models are selected. These are ordinary typed data, not
admission, runtime, storage, safe-decoder or authority certificates.

1. OriginalValueDocument has public encoded: Vec<u8>,
   nodes: Vec<OriginalValueNode>, edges: Vec<usize>, root: usize. It owns every
   input byte and every variable structural spine; nodes contain offsets/IDs,
   never pointers into a moved Vec. It is Debug-only, without derived Clone,
   Eq, an opaque proof flag or a second buffer owner. Public construction may
   fabricate malformed indices, so consumers cannot infer validation from the
   type. Four exact public #[must_use] methods provide checked views:

       root(&self) -> Option<&OriginalValueNode>
       node(&self, index: usize) -> Option<&OriginalValueNode>
       byte_range(&self, range: std::ops::Range<usize>) -> Option<&[u8]>
       edge_range(&self, range: std::ops::Range<usize>) -> Option<&[usize]>

   They use checked slice access and return None for out-of-range or reversed
   ranges/indices. They may be implemented in R0 because they only inspect the
   ordinary model. They do not recursively validate an arbitrary public graph,
   authorize a decode, invoke a resource or carry future credit.
2. OriginalValueNode is a Debug-only enum with these EXACT variants/fields.
   All ranges are std::ops::Range<usize>, with byte ranges into encoded and
   edge ranges into edges; all node IDs are usize. No extra kind/view wrapper,
   iterator, range alias, recursively owned child or Other variant is created.

       EmptyList
       Integer(i64)
       BigInteger { negative: bool, digits: Range<usize> }
       FloatBits(u64)
       Atom { bytes: Range<usize>, utf8: bool }
       Binary { bytes: Range<usize> }
       BitString { bytes: Range<usize>, last_bits: u8 }
       Tuple { children: Range<usize> }
       List { heads: Range<usize>, tail: usize }
       Map { entries: Range<usize> }
       ForeignPid { node: usize, id: u32, serial: u32, creation: u32,
                    local_to_context: bool }
       ForeignPort { node: usize, id: u64, creation: u32,
                     local_to_context: bool }
       ForeignReference { node: usize, creation: u32, words: Range<usize>,
                          local_to_context: bool }
       ForeignExport { module: usize, function: usize, arity: u32 }
       ForeignLocalFun { module: usize, arity: u8, index: i32, uniq: [u8;16],
                         old_index: i64, old_uniq: i64, creator: usize,
                         free: Range<usize> }

   Integer is the normalized i64 mathematical range; wider magnitudes remain
   BigInteger with trimmed little-endian digit bytes and a nonzero-only sign.
   EmptyList is not atom nil. FloatBits represents finite source binary64
   values and preserves both zero signs. Atom keeps its actual Latin1/UTF8
   spelling class (utf8=false/true) and semantic identity is compared across
   aliases. BitString last_bits is1..8 for nonempty data; a byte-aligned
   bitstring may normalize to Binary. Map entries is an alternating key/value
   node-ID range of even length, in parsed input order, with semantic duplicate
   keys rejected. This is a Rust data layout, not promised BEAM map traversal.
   List heads and tail preserve proper/improper semantics; STRING_EXT expands
   as byte-list data, not Binary. A zero-head LIST alias resolves to its tail.
   Foreign reference words are an encoded big-endian u32 byte range; local
   padding/identity normalization is a decoder duty, not resource activation.
   ForeignLocalFun preserves the fields and canonical identity needed by the
   selected source profile. local_to_context is captured equality data only.
   These fields never promise execution, current node authority or resource
   ownership. Any unsupported matching-local identity profile stays explicit.
3. OriginalValueContext<'a> is Clone/Copy/Debug, with public fields:

       safe_atoms: Option<&'a [&'a str]>
       active_exports: Option<&'a [(&'a str, &'a str, u32)]>
       local_node: Option<(&'a str, u32)>
       local_functions: Option<&'a [OriginalValueFunctionIdentity<'a>]>

   None means the corresponding lookup/context is unavailable, not an empty
   closed table. Some supplies the captured mathematical lookup set for this
   explicit compatibility profile. A reached known-absent atom/export in that
   closed set is a source InvalidFlowValue; unavailable required context is
   UnsupportedContext. Entries must be coherent immutable captures; duplicate
   names/exports/functions, name widths and source identity/profile validity
   need explicit validation in the later BODY contract. These inputs do not
   attest a global live VM, grant permission or infer known names from the
   untrusted bytes. No context borrow escapes in OriginalValueDocument.
4. OriginalValueFunctionIdentity<'a> is Clone/Copy/Debug with public module:
   &'a str, index: i32, arity: u8, uniq: [u8;16], old_uniq: i32. It is the
   captured canonical function-identity data, not a callback or callable code.
   No constructor, Default, factory, owner/lease or runtime handle is added.
5. OriginalValueLimits is Clone/Copy/Debug/Eq/PartialEq, with five public usize
   fields: max_encoded_bytes, max_nodes, max_edges, max_depth, max_work_units.
   All must be positive. No Default/constructor or silently selected deployment
   profile is created. Invalid or arithmetically incompatible profiles yield
   InvalidLimits; an exceeded profile or checked resource-count overflow yields
   Limit. Encoded bytes means the ENTIRE moved stored buffer, including any
   FSV2/header bytes. Nodes and edges bound the actual returned Vec lengths;
   depth counts the semantic root as1, raw/escaped binary as1, then actual
   nested typed children. Work units count admitted byte inspection, node/edge
   traversal and context/semantic-key comparison work; repeated comparisons
   are not free. The later exact implementation tariff must match these units
   and be selected before BODY, with no promised linear bound for duplicate
   detection. These profiles are separate from history max_bytes, stored user
   size, native Meter R/F, process heap/RSS and caller input-buffer admission.
6. OriginalFlowValueError is Debug-only with EXACT variants:

       InvalidFlowValue
       UnsupportedContext { tag: u8, offset: usize }
       UnsupportedProfile { tag: u8, offset: usize }
       InvalidLimits
       Limit
       ReserveFailed(std::collections::TryReserveError)

   Offsets refer to the complete moved buffer. The two unsupported lanes
   distinguish unavailable matching context from a known valid/unimplemented
   profile. Neither is corrupt data or an automatic hydration invalid-flow
   field. Real try_reserve errors retain their operational cause. No fabricated
   OOM fixture, panic, copied BEAM exception/stack, Display/Error impl/helper,
   capacity==requested claim or automatic Box/shrink is selected in R0.

Exactly one public decoder signature is selected at value's reexport seam:

    pub fn decode_original_flow_value(
        encoded: Vec<u8>, context: &OriginalValueContext<'_>,
        limits: OriginalValueLimits)
        -> Result<OriginalValueDocument, OriginalFlowValueError>

The R0 body ALWAYS returns Err(InvalidFlowValue), for EVERY input/context/limit.
Use ordinary unused-argument spelling; do not implement header checking,
sentinels, contexts, limits, scanner, numeric parser or partial success yet.
This is intentionally a NEW callable rejecting baseline, not an old function
falsely claimed RED. Document views above remain checked ordinary model access.
The first prefix/type/test source snapshots and actual runtime baseline will
be saved before separate BODY permission.

The earlier generic encode_original_flow_value proposal is NOT selected in
this batch. HistoryValues' direct complete callee is a decoder. Real encoded
catalog publication can move validated stored bytes, while a future caller
value encoder needs its own concrete input/format contract. No encoder stub,
empty encoder child or hidden writer success is authorized merely by this
decoder prerequisite. That staging does not remove the required real producer,
availability, snapshot or full hydration end state.

Exactly ten supplemental Rust test functions, with NO test helper, are selected
from DOMAIN's finite roster. They are not new mandatory upstream cases and do
not populate the three original HistoryValues mappings:

wrappers.rs:
- flow_value_preserves_plain_and_escape1_binary_branches
- flow_value_rejects_compression_trailing_truncation_and_bad_lengths

scalars.rs:
- flow_value_bigints_preserve_sign_truthiness_padding_and_zero
- flow_value_float70_and_float99_preserve_bits_and_reject_nonfinite

structure.rs:
- flow_value_decodes_portable_etf_roots
- flow_value_rejects_duplicate_exact_keys_across_encodings
- flow_value_preserves_distinct_zero_and_integer_float_keys

context.rs:
- flow_value_safe_context_and_foreign_data_keep_operational_lanes
- flow_value_limits_are_distinct_from_invalid_data
- flow_value_document_owns_its_read_buffer

Each keeps source-derived literal byte/bit/error expectations independent of
the decoder. The common-root test preserves the complete184-byte compound and
its individual typed values, nil atom versus empty list, tuples and proper/
improper lists/bitstrings. Wrapper tests include empty/raw/unknown/incomplete
FSV2, one-level escape and exact EOF. Scalar tests include >i128 magnitude,
nonzero sign-byte truthiness, padding/zero and exact signed-zero bits, OLD_FLOAT
grammar and nonfinite refusal. Semantic maps retain integer/atom/list encoding
aliases, pair-order-independent nested map equality, and distinct integer/
float and both zero-sign keys. Use literal independently observed vectors;
parsing an expected value with the SUT or accepting encoded-text placeholders
would not test typed decode.

Context tests prove unavailable versus closed known-absent lookup and explicit
foreign identities AS DATA. They do not invoke a BEAM pattern, fun, PID, port,
reference or invented callback. LOCAL/native-index fixtures exercise the
operational profile lane, without calling it passing support for those source
profiles. Limits tests separate whole-stored-byte caps from the payload return
cap and exercise actual small positive node/edge/depth/work refusal plus zero
invalid profiles. They do not assert an unselected exact work tariff or fake a
reservation failure. The ownership test moves nonstatic encoded input into the
decoder, uses the returned complete content after temporary context descriptor
rows drop, then checks that all leaf ranges resolve inside the returned owned
buffer. Before any pointer assertion require success and nonempty/exact
structural content. It also constructs an ordinary malformed public document
to test the four bounds-checked views without recursive traversal, overflow,
infinite loops or a new test/helper name.

The four original Root packets are golden evidence, not a reason to copy a VM
execution requirement into tests. I additionally read/hashed ENTIRE
8747208+7478/SHA816e08b99eb8f7fc3062028b018e3f1cf2d755ca7597a904dde301d5bce8fdf2:
five bignum and21 OLD_FLOAT observations. Sign2/255 are negative truthy and
zero-width/padded zero canonicalize to0. FLOAT_EXT99 accepts comma decimal,
first-NUL termination with ignored remaining31-byte-field contents, finite
decimal/exponent/underflow and signed zero; integer-only, malformed fraction,
whitespace, junk before NUL, incomplete exponent, overflow and nonfinite text
refuse. The scanner must not inherit the policy parser's stricter sign0/1
check or use unchecked Rust FromStr/whole-field UTF8/zero-padding assumptions.
The later scalar-helper decision must preserve pinned grammar before numeric
conversion and record platform rounding/profile limits separately.

R0 creates NO private parser/frame model/helper/constant or BODY source child.
The proposed families decode_wrapper/scan_term/scan_scalar/scan_sequence/
scan_map/scan_foreign/check_safe_context/same_term/finish_document, iterative
frame shapes and exact source subdivisions remain a separate concrete BODY
decision AFTER real R0 is saved. No broad module grant is inferred from those
family names. All eight selected paths must pass natural production400,
integration600 and function100 after Root's formatter; inability to fit is a
precise geometry consultation, never packed code or dropped coverage.

Direct consultation: DOMAIN supplied the six-model/data-only/ten-test proposal
and withdrew its earlier encoder/live-owner/cache prerequisites. PERF directly
accepts the bounded R0 shape and supplied whole-buffer/depth/work and fabricated
public-index cautions incorporated above. This records their actual messages,
not an invented read of an unpublished note. Root is ready to assign one source
owner, freeze all other inputs, read stopped artifacts, run quality plus actual
callable ten-test RED and SAVE before any BODY. The full catalog/read/hydration,
original3/four links, all profiles/Linux and wider goal remain uncredited.

### Literal R0 refinement before creation: private document storage

Root's immediate custody review prefers private document storage and small
checked views. I SELECT that narrower contract here, superseding ONLY the
public Document field/fabrication wording in the preceding R0 decision.
Nothing in this amendment permits production creation before Root's release.

OriginalValueDocument still owns exactly encoded: Vec<u8>,
nodes: Vec<OriginalValueNode>, edges: Vec<usize>, root: usize, but these four
fields are PRIVATE outside the original_flow feature (pub(super) inside its
model child is permitted for the later decoder). There is no public
constructor, Default, mutable accessor, proof token or seventh model. The
public OriginalValueNode enum and all other six-model fields/traits remain
as selected. Private internal invariants confer no admission/storage/runtime
authority. A standalone fabricated Node cannot mutate a returned document.

The EXACT five public #[must_use] view methods are:

    encoded(&self) -> &[u8]
    root(&self) -> Option<&OriginalValueNode>
    node(&self, index: usize) -> Option<&OriginalValueNode>
    byte_range(&self, range: std::ops::Range<usize>) -> Option<&[u8]>
    edge_range(&self, range: std::ops::Range<usize>) -> Option<&[usize]>

encoded borrows the complete owned stored input unchanged. Every ID/range
method checks the appropriate slice bounds; reversed, overflowing-looking
or out-of-bounds requests yield None, never indexing panics. No traversal,
iterator/cycle validator or new fixture constructor is added. The ordinary
five view bodies are allowed in the R0 model; the decoder remains the same
ALWAYS-InvalidFlowValue rejector.

The SAME ownership test now checks oversized node IDs and reversed/oversized
byte/edge ranges AFTER a successful real decoder result, without constructing
an impossible public document. Its pointer/content/context-drop oracles stay;
the genuine rejecting R0 will mask those runtime tails until BODY. It proves
the owned incoming Vec can outlive temporary descriptors/context rows and that
the returned byte ranges lie inside its owner. It does not manufacture an
authority-bearing document or exercise recursive malformed graph handling.

Full incoming ownership is explicit: max_encoded_bytes bounds BOTH the
complete stored len (all FSV2/ETF bytes included) AND encoded.capacity() bytes
of the moved Vec<u8>. A short slice in an oversized incoming Vec must not
silently retain uncounted spare capacity. This is an operational input-owner
profile, distinct from source stored-size/max_bytes semantics. The same limits
test includes a small valid stored value in an intentionally over-capacity
input Vec and expects Limit, with generous independent structure/work limits.
This is a real input-capacity case, not a forged allocator failure. No copy,
shrink, Box conversion or duplicate accounting owner is authorized to repair
it. Root/input-acquirer ownership precedes this call; the check creates no
native R or public opening credit. Node/edge profiles continue to bound actual
returned lengths and prospective requested spines; allocator overprovision,
scratch/peak, heap/RSS and physical byte performance remain separately
qualified implementation evidence rather than an exact-capacity assertion.

Eight paths, six models, ten names, module/reexport seams and decoder-only
scope are unchanged. Future parser BODY/frame/helper selection follows saved
genuine R0, not a universal design freeze. Both peers were directly informed
of this literal precreation refinement; no new acknowledgment loop is needed.

### A-HISTORY-VALUES-CODEC1-R0-GEOMETRY1: two exact test helpers

During the released R0 authorship, DOMAIN reported the actual in-memory
ordinary-format geometry: full typed assertions for the184-byte/eight-value
compound and five foreign DATA families push their selected test functions
over100 lines. Before either helper is created, I SELECT ONLY:

- tests/original_flow_value/structure.rs private
  assert_compound(document: &OriginalValueDocument).
- tests/original_flow_value/context.rs private
  assert_foreign(document: &OriginalValueDocument, tag: u8).

Paths above are relative to crates/wf-domain. No new file, type, model, API,
fixture or test name is added. The same ten #[test] names and all assertions
remain; main's existing common imports may add the already selected document
name. These two assertion helpers use only the existing checked document
views and independent literal expected node fields/bytes/edges. Require
nonempty/exact cardinality before element checks. Unexpected foreign tags or
classes must fail, not skip the assertions. They call no decoder/encoder or
source oracle to derive expected values and introduce no fee, callback,
storage-success stand-in or allocation-failure harness. This supersedes ONLY
the preceding no-test-helper restriction for these two exact functions.

All twelve test/helper functions must naturally remain<=100 after Root's
formatter, and files<=600; no packing, omission or geometry waiver. DOMAIN
holds these helper creations until Root records this selected delta in its
existing release journal; other already-authorized work continues. This is
the repository's explicit precreation consultation, not an added readiness
acknowledgment or parser BODY release.

## UNAPPROVED CODEC BODY proposal while rejecting R0 is authored

This section is a concrete proposal for the later selection, not permission to
create a parser/file/type/helper or implement a BODY. Root's actual compiled
genuine R0 and SAVE still precede that separate decision. I did not read the
moving R0 files. Public six-model/one-entry/five-view contracts and all ten
selected tests are frozen by their source custody; no encoder, catalog,
hydration, foreign execution API or new external dependency is proposed here.

Root explicitly accepted the encoded-depth clarification and requested it be
relayed to DOMAIN before R0 stopped. I relayed it: depth is entered ENCODED
typed-subterm nesting, root1 and each encoded child+1, including a zero-head
LIST alias wrapper. It is not normalized output-graph depth. STRING's virtual
byte cells need no encoded child frame; its actual expansion still counts all
result nodes/edges and work. Source aliases may normalize in the result while
their real parse nesting remains bounded. This resolves the earlier semantic
wording without adding a field or silently changing a frozen positive test.

### Proposed source cut and private roster

The candidate BODY write set is existing value/original_flow.rs plus NINE
named private children below, all relative to crates/wf-domain/src. The R0
model.rs, public parent value.rs seam and all tests remain unchanged. No child
is created until the later exact Root/ARCH BODY selection.

- value/original_flow/budget.rs: profile/work admission and fallible requested
  owner/stack growth. Private Budget only.
- value/original_flow/context.rs: reached captured-table validation/lookups and
  per-decode virtual function identity. Private ContextState, FunctionBinding.
- value/original_flow/scan.rs: the shared iterative census/build driver.
  Private Census, ScanTarget, Scanner, TermSummary, SummaryKind.
- value/original_flow/scan/frame.rs: private ParseFrame continuations only.
- value/original_flow/scan/scalar.rs: scalar wire families and bounded FLOAT99.
- value/original_flow/scan/sequence.rs: tuple/map/list/STRING continuations.
- value/original_flow/scan/foreign.rs: PID/port/ref/export/closure DATA fields.
- value/original_flow/equality.rs: map fixup and iterative exact-key evaluator.
  Private CompareFrame, CompareStep, ListCursor.
- value/original_flow/equality/scalar.rs: normalized scalar/foreign key equality.

The existing feature root would add only private module declarations, the
public rejecting-body replacement, decode_wrapper and finish_document; private
ValueEncoding distinguishes Raw{start} and Etf{version_offset}. It grants no
capability. There is no generic utils, ranking/hash/interner subsystem, payload
clone, recursive owned node tree, extra output wrapper or live-VM lease.

The exact candidate helper roster/arity below counts explicit arguments, not
self. It is a finite proposed subdivision, not a grant to add unnamed helper
classes/functions during implementation. A later actual selection can simplify
it before creation; a newly needed artifact/helper still receives its concrete
decision. Standard trait derives/library calls are not an unlisted decoder.

- original_flow: decode_wrapper2, finish_document5, and the existing public
  decode_original_flow_value3 body replacement.
- Budget methods: new2, spend1, require_nodes2, require_edges2,
  require_depth1. Free generic helpers reserve_exact3<T>, grow_stack4<T>.
- ContextState::new0; context helpers check_atom5, check_export7,
  local_node6, local_function5, validate_atoms3, validate_exports3,
  validate_functions3, validate_local_node3, valid_name2.
- Scanner methods: new6, run0, take1, begin_node1, emit3, slots1,
  write_edge2, push_frame1, accept_child1. frame::expects_atom1.
- scalar: parse3, big_integer3, normalize_integer4, legacy_float3,
  atom3, binary3.
- sequence: start3, accept3, string_step2.
- foreign: start3, accept3, finish_identity3, finish_export3,
  finish_local_fun3.
- equality: validate_maps2, same_term4, compare_pair3, resume4,
  list_step3, push_comparison3.
- equality/scalar: scalar_equal4, names_equal3, integers_equal4,
  bitstrings_equal4, foreign_equal4, references_equal4, functions_equal4.

The actual parameter groupings are concrete worker-state borrows: Scanner owns
cursor/counts/flat parse frames and borrows encoded bytes, captured context,
ContextState, Budget and ScanTarget for one scan. It owns no reference into
itself and escapes no such borrow. ScanTarget is census-only or mutable
node/edge Vec borrows plus expected Census totals. Census has planned/finished
node counts, edge count and maximum encoded depth. TermSummary holds a completed
node ID, original tag/offset and SummaryKind (Other, Atom span/encoding,
normalized Integer, Pid), so census can enforce foreign child classes without
retaining a whole graph. No scalar payload is cloned to return a summary.

ParseFrame retains ONE compound and its current sibling position: sequence
template/progress, zero-head alias, STRING expansion, node identity, export or
local fun. Templates are the already selected ordinary Node data with fixed
fields/edge ranges filled before publication. Fixed metadata/counters and
TermSummary values are not new owner models. A frame never enqueues all future
children. ContextState holds reached-table validation flags and a bounded Vec
of FunctionBinding metadata (source module span/encoding, index/arity/uniq,
first/captured old identity fields); it contains no callable resource. The
function bindings are private per-decode scratch, bounded by reached nodes and
work, and drop before success returns.

CompareFrame has sequence, list and map-search continuations; CompareStep is
Done(bool) or the next actual node pair plus one continuation. ListCursor is a
node ID/head position, not a copied list. A map continuation stores one left
pair, one right candidate and key/value phase; it never pushes every possible
candidate. No generic callback, recursive comparison, normalized key tree,
hash collision assumption or unbounded native call stack is hidden in it.

### One iterative grammar, exact result counts and ownership order

The proposed operation sequence is:

1. Check positive profile fields and the actual whole incoming stored len AND
   Vec capacity before new scratch/arena work. The owned caller buffer already
   exists; this is an input-owner limit, not retroactive funding or a claim
   that the decoder admitted the earlier I/O/acquisition. Checked actual count
   overflow/exhaustion is Limit; unusable zero profile fields are InvalidLimits.
   An enormous positive ceiling is not an instruction to allocate that ceiling.
2. Classify the wrapper. Raw/unknown/incomplete magic and escape1 require one
   Binary node, no edge and no captured context. Preserve the complete owned
   input; only the node's range changes. Typed mode requires version131 and
   enters the same grammar at that absolute buffer offset. Compression is
   rejected explicitly; all error locations remain whole-buffer offsets.
3. Census with one prospective flat frame per entered encoded level. Check
   counts/minimum remaining wire lengths before a claimed arity can allocate
   future structure. Atom-only source positions are guarded before interpreting
   an arbitrary child (dec_atom is not a generic recursive decoder). Perform
   reached syntax, scalar, safe-context and foreign-shape checks now. Census
   keeps only the current summaries/frames/function-binding metadata. There is
   no free recursive preflight or free input-spine conversion.
4. Drop the retired census parse stack. Reserve the EXACT census node count
   once and edge count once on empty Vecs, fallibly, under the same work budget.
   Empty edge arrays do not require an artificial allocation. Build reruns the
   SAME grammar/branch checks; it does not implement a second independent
   grammar. Context is immutable input; per-decode canonical function metadata
   is shared intentionally, not a second source-runtime mutation.
5. Reserve each compound's contiguous edge slots before its child walk, then
   fill only those admitted slots. Complete nodes in POSTORDER: children get
   IDs before their parent, so all references point backward in this acyclic
   document. Zero-head LIST returns the normalized tail ID; STRING expands
   actual integer heads/list tail using ranges in the original byte owner.
   Its synthetic output nodes are counted even though its encoded depth does
   not grow per byte. No leaf String/Vec or source tree is cloned.
6. Require exact EOF, identical census/build counts, complete slot population
   and one valid root before semantic map publication. Then validate maps in
   increasing postorder node ID, children before parents. Returning a document
   before duplicate-key validation is forbidden. Partial private arrays may
   exist on invalid input and drop on error; no partial successful value or
   authority escapes. Move the original input into the final document once.

Postorder IDs are an internal choice, not a root==0 promise or BEAM map-order
claim. Tests must use the checked root/node/edge/byte views, not manufacture an
index-order oracle. Every returned span/ID/range is checked during construction
and the public views remain bounds-checked. Flat Vec destruction avoids a
recursive input-depth-dependent drop chain.

### Semantic key equivalence without a new optimizer

Pairwise map duplicate checks are the proposed minimum: compare each key to its
earlier keys with iterative same_term and charge each candidate. Nested maps
are already validated before a parent map can compare them as keys. Map equality
matches each left key to one right candidate, then its value, independently of
pair ordering; uniqueness ensures at most one match. Sequence and list
continuations compare one reached child pair at a time. Mismatch unwinds frames
as data, not language recursion. This intentionally non-linear algorithm may
be quadratic or worse on nested structures; the monotone work limit is its
explicit stop. No linear-total-time or benchmark improvement is claimed.

Small/signed/big integer aliases normalize mathematically, including arbitrary
nonzero sign-byte truthiness, padding and unsigned zero. Values in i64 use
Integer, including exact i64::MIN; larger magnitudes retain a borrowed trimmed
little-endian range. UTF8/Latin1 atom names compare as Unicode scalars without
allocating a normalized String. STRING and differently segmented LIST forms
compare head/tail cursors; EmptyList differs from atom nil. Integer/float classes
remain distinct. Finite float bits are exact, including distinct +/-zero keys.
Binary/bitstring comparison uses logical last-bit masks and lengths, not unused
trailing bits or raw ETF tags. Byte and atom comparisons charge every repeated
visit, including those made for duplicate detection.

Foreign identity compares normalized node/name/creation/ID/reference/function
metadata as DATA, not resource execution. Export safe availability is checked
before publication. Local-function identity follows captured or first-created
canonical metadata; creator and ignored wire Size are not key identity.
Canonical function identity, environment and source exact-key rules must be
kept separate from raw wire provenance fields. A genuinely ambiguous valid
foreign-key profile must return UnsupportedProfile, not guessed equality or
InvalidFlowValue. This is a visible remaining profile lane, not a reason to
defer common data/maps or invent a BEAM execution bridge.

### Exact proposed work and scratch tariff

One Budget remains monotonically spent across wrapper/census/build, captured
context checks and semantic key fixup. It never resets between phases. This is
a prospective logical operation ceiling, NOT native Meter issued R, forecast F,
elapsed CPU, allocator latency or whole process/GC/RSS pricing.

The proposed unit table is: one per consumed wire byte per pass; one per
visited/produced node or edge; one per continuation visit/push/pop; one per map
candidate or captured row; and one per additionally inspected scalar/name/bit
byte on EVERY visit. Opaque range consumption is a declared parser-handling
tariff even if the implementation advances by a constant-size checked span.
Additional normalization/UTF8/key comparison visits are not included for free.
Bulk slots/copies reserve their complete checked unit count before the loop.
After a span is bounds-checked, its work is admitted before cursor advance or
data access. A failed range check may return InvalidFlowValue without pretending
an unavailable huge payload was traversed. Work overflow/exhaustion is Limit.
No issued-prefix/financial credit or unexecuted physical-allocation measurement
is inferred from this logical tariff.

Requested allocation work is also explicit: before a reached Vec growth,
reserve_exact charges one allocator-call unit plus target element count *
size_of::<T>(), and a conservative old capacity * size_of::<T>() relocation
allowance when old storage can move. Every product/addition is checked BEFORE
the fallible call. Real try_reserve_exact errors become ReserveFailed. For the
two result arrays, old capacity is zero and the exact requested census counts
are admitted once; pushes remain within that request.

Scratch grows only when a reached push needs it. Proposed grow_stack requests
min(ceiling, max(needed, max(8, old_capacity*2))), using checked arithmetic and
the same prospective target/old-capacity tariff. The parse ceiling is max_depth;
the comparison/virtual-function ceilings are bounded by reached nodes and/or
max_work_units, since each reached task/push spends work. It must not allocate
the full deployment cap in advance or call reserve_exact(1) repeatedly without
admitting relocations. Charge visits/push/pop even when existing capacity avoids
an allocation. Drop retired scratch deliberately. Allocator overprovision can
exceed the request and latency is not bounded by this arithmetic; the proposal
does NOT assert capacity==requested, exact physical-copy cost or whole-memory
ceilings. Incoming retained capacity is independently checked exactly as R0
selected; it cannot be hidden with shrink/Box/copy.

### Captured context, foreign shape and unsupported profiles

No context is read for raw/escaped bytes. At the first reached use of a Some
table, validate that table's names/widths/duplicates/canonical identity rows on
the same account. Cache only that validation result for this immutable decode.
None or an unusable/incoherent supplied table returns UnsupportedContext at the
reached wire tag/offset. It must not masquerade as a closed known-absent atom.
After a coherent Some lookup, known-absent atoms/exports return InvalidFlowValue.
No registry is manufactured from untrusted bytes, no global VM attestation or
new captured-owner API is created, and no context reference escapes success.

Atom payloads require source-valid Latin1/UTF8/counts before lookup. Foreign
node/module/function atom positions enforce the source atom clause before
interpreting an arbitrary term; old fun index/uniq and export arity retain
the source generic-term-then-small-integer guard order. NEW_FUN Size is ignored
as pinned OTP does, free count is source-bounded, and canonical identity data
uses reached captured/first-created entries. Fixed fields and all dynamic
lengths are checked before reads/slots. Source-invalid shapes remain invalid;
genuinely unavailable local validation remains operational.

Known local node context enables the pinned64-bit local PID/port/reference
guards and identity normalization. I read external.c4740..4789 and
erl_bif_unique.h259..277: pid-reference lookup reconstructs/checks an embedded
PID tag, not a live process registry. Ordinary/magic reference identities need
no new execution owner; missing magic lookup falls back to an ordinary source
reference. DATA must not acquire a resource capability from that fact. Padded
reference words and local3/5-word identity need source normalization, not raw
byte equality. A context-dependent/unimplemented source profile stays explicit.

The minimum proposal rejects removed FUN_EXT117, compressed/unknown/malformed
standalone forms and ATOM_CACHE_REF without its unavailable distribution table
according to the pinned standalone TermCodec path. Native atom-index73/75 and
LOCAL121 require a matching native context absent from these public models;
after their available fixed framing checks, return UnsupportedContext, never
pretend malformed data merely because that context is unavailable. Known
unimplemented zero-word reference and empty BIT len0/bits0 native edge profiles
return UnsupportedProfile instead of triggering/guessing the pinned native
underflow branches. These operational lanes do not certify those source profiles
or exclude an included mandatory assertion. They cannot be silently translated
to the hydration invalid-flow field.

### Legacy FLOAT99 is a bounded source grammar, not FromStr alone

The later scalar helper must use the exact pinned pre-NUL grammar: optional
one sign; >=1 ASCII integer digit; '.' OR ','; >=1 fraction digit; optional
complete e/E exponent with optional sign and >=1 digit; end at the first NUL
inside the31-byte field. Ignore bytes AFTER that NUL but still consume the full
field. Validate before converting; normalize comma in a fixed local31-byte
buffer, never mutate encoded. Checked ASCII conversion then finite f64 parsing
preserves signed zero and underflow; nonfinite/overflow refuses. The call's
input handling/conversion charge is admitted before it, and the bounded31-byte
input is not an O(unbounded-number-text) or whole-CPU claim. No external parser
crate or libc call is selected.

Pinned C strtod/radix retry and Rust nearest/ties-even conversion remain a
selected-profile equivalence obligation, with actual macOS goldens and separate
Linux/source-profile validation. The current26 observations close comma,
first-NUL/ignored trailing field bytes and finite range/sign cases. A field
with NO NUL in31 bytes remains unobserved. Before claiming that profile, Root
can run the two already named root/tuple inline controls AFTER authorSTOP;
otherwise propose UnsupportedProfile{tag:99,offset} for that exact boundary,
never read beyond the field or invent InvalidFlowValue. This does not hold
callable R0 or the common complete codec contract on a new native ABI.

I actually read/hashed PERF15885+10645/
6f3707f3ed66c7da6824bc0cb53057653b734b2fb2b22e117d6a251a5c4d20ed.
Its source/ownership/iterative comparison/work/scratch/legacy grammar findings
agree with this proposal. Canonical ranks/hashes are deliberately deferred.
DOMAIN was told to finish its R0 authorship normally; no source was inspected
while moving and no BODY acknowledgement is required. Root will provide
stopped canonical R0, actual runtime RED/SAVE and the next finite selection.
This proposal grants no source creation, full hydration mapping, storage or
public runtime completion, initialR/E-T settlement, measured gain or Linux credit.

## CODEC1 actual stopped R0 source and refined oracle review

I independently read ALL eight canonical files after DOMAIN's STOP and Root's
format/refinement custody. No moving source was read and I ran no compiler,
VM, test, Git operation or source write. Only this architecture note changes.
Actual complete hashes/physical geometry bound the review:

- value.rs185/7825/775cd167e5cf5db1fec65d4527ff0b7323beec14c8987dc0495d5bd469991ac1
- value/original_flow.rs25/920/9885907dfc46bbd99b718d19059fbd197d57799ef64717f40353ab94a678e8f0
- value/original_flow/model.rs233/7691/2f0738de6aa2b49b7220e28cef54f0502ed8884a3ed1ecdfb3d1f74336b1a514
- tests/original_flow_value/main.rs19/817/b5b9bb47ea9966c3f2b1315ba90f041c812cb15ca4cbe3c30198b99319832a49
- wrappers.rs85/2563/42072ba76dfc6215cc3e06822ed6c1bd0fc02919583d051f8ff2b7c2a520ba79
- scalars.rs136/4650/dee1dc4108703728df43f400eb0216a6cdf2199982ac04eed50b74e05b9c14ae
- structure.rs232/10359/57ee2b32c8b859c4090650e77cef6fd3f151bbf17dac758cdcb55c912a09b072
- context.rs302/10522/ce68cc6762c0e2eff5d998f6d99b1a70c63453109ebf3b2f680ac54e35e84972

The two bare child-test filenames above have the same approved target prefix;
all production paths are relative to crates/wf-domain/src and test paths to
crates/wf-domain. These are source-only bindings before the later optional
last-NUL positive fixture refinement and before authoritative runtime results.

My own exact229-byte module/reexport removal restores the ENTIRE old value.rs:
177LF/7596B/SHA6013552c8d2bf51c88663a029cb563ba17b5743fe030225fba5f03a8c44fd47d.
Every old DomainError/InputField/Counter/RetryPolicyField/display byte survives.
No unrelated root-module, closed Raw/policy parser, Cargo, lib or runtime model
change is hidden in that seam. The feature is exactly a25-line public facade
with the new three-argument decoder ALWAYS returning InvalidFlowValue. It does
not check wrappers, values, context, limits or implement a successful branch.
There is no scanner/encoder/private BODY file, helper, frame or constant yet.

The six models and15 node variants match the exact decision. Document's four
fields are private to the feature; five views use checked slice/node access.
No constructor/mutable accessor, self-reference, context lifetime, payload Clone,
recursive tree or proof certificate appears. Function/export/PID/port/ref data
does not promise invocation. Limits explicitly retain whole input length AND
Vec capacity and use encoded subterm depth including zero-head wrappers.
Source InvalidFlowValue, unavailable context/profile, invalid/exhausted limits
and real TryReserveError remain distinct. The public function returns an owned
document type independent of temporary context/string/list descriptor lifetimes;
its ownership test will compile-check that boundary before any successful BODY.

The target has EXACTLY the ten selected #[test] functions and two selected
literal assertion helpers, no fixture helper/type/file. Independent lexical
function spans after Root's formatter are39/41 in wrappers;52/79 in scalars;
60/52/47 plus65-line assert_compound in structure;81/80/50 plus83-line
assert_foreign in context. All files satisfy600 and every function<=100;
production files satisfy400. The enum and checked views remain within their
approved production footprint. Rustfmt's naturally retained long macro guards
are expressions, not packed unrelated statements or waived coverage.

I parsed the literal compound bytes independently of the decoder: all184 bytes
are EXACTLY the original packet's compound, SHA256
87d0e580ac0b3f9ba92c6e3dc696671b3b4a479f09aca4bfed342f20566df5fa.
Its helper first requires a Map with16 edge IDs, then checks all eight key/value
pairs and nested content, including >i128 magnitude, negative-zero bits,
opaque byte255, improper tail, nil/true atoms, nested map/proper list and tuple.
Foreign assertions require each expected variant/field; unexpected classes fail.
Root's pre-first-run creator refinement now checks the creator PID's atom n
as well as PID fields, which closes a real omitted literal assertion.

The negative tests do not stand alone as false success. Duplicate-key controls
first require BOTH standalone keys to decode successfully before constructing
both duplicate orders; nested map pair-order, segmented LIST, STRING, atom and
integer aliases are present. Four distinct numeric map keys require exact
eight-edge cardinality and all four value/bit pairs. Integer tests include
i64 endpoints, sign2/255 truthiness and wider/padded values. Raw/escape branches
require exact bytes/owner preservation. Context tests separate known-absent
from unavailable and exercise explicit operational local profiles. Limits
first require a valid result, then five small-positive/zero limits, real spare
incoming capacity and encoded LIST0-wrapper depth2 refusal/depth3 success.
The lifetime test moves a nonstatic Vec, drops temporary context String/arrays,
requires exact nonempty tuple content, and then checks the SAME input pointer,
leaf offset/pointer and checked invalid/reversed/empty ranges. No vacuous all,
fake OOM, SUT-derived expected value, test I/O or callback success is used.

I also actually read/hashed Root's four FLOAT99 boundary observations at
8757813+4392/SHA0373edb42e9a1e94e572e220177e8c46f0e470dc3d04e5c285b66cd5d7c5febf.
Both selected no-NUL31-byte root/tuple cases return InvalidExternalTerm;
NUL-before-nonUTF8-tail and NUL at the final field byte return finite1.0 with
3ff0000000000000 bits. Source pins/actual Router65535 and all five visible
redefinition warnings remain. This closes the precise pending no-NUL operand
for the selected bounded profile. It supersedes the UNAPPROVED BODY proposal's
fallback UnsupportedProfile99 recommendation: the implemented scalar rule must
match these actual invalid and valid vectors, never read beyond31 bytes or
require UTF8/zero padding after the first NUL. It does not claim universal libc
rounding, Linux execution or source validity for arbitrary undefined contexts.

Root added three same-float-function controls BEFORE first Rust execution:
nonUTF8 after NUL positive plus no-NUL root and nested negatives. The fourth
original last-byte-NUL positive is not yet in the current Rust loop. PERF and
I recommended adding30-byte text ('1.' plus28 zeros) to that SAME positive
table, so resize appends NUL exactly at field byte31. Root accepted that as
an ordinary non-gating test refinement ONLY after active88847 terminates;
there is no current refined hash or runtime credit for it here. No new name,
helper/model/file or parser change is proposed.

The fixture's NEW_FUN Size0 is intentional source compatibility, not a missing
validation oracle: I read pinned external.c5046/6157, whose decoder and sizer
explicitly ignore the faulty Size hint. Other fixed/free/creator fields remain
checked by the typed assertions. No BEAM function/resource invocation is added
to the Rust requirement.

I actually whole-read/hash-checked DOMAIN1486+1567/5c78cf75b09d983bb3191319e17ef0daa28456731e6fa2c9a79822e41f6cc829
and3053+1553/b02ac9a394611f2e47f165aa595fad2f5cf140f8dbc3bf8955dda26cbe1be337.
Their author-only source hashes/census precede Root format/oracle refinements;
I bind current files above instead of promoting the stale author hashes.
PERF independently read the same current eight files, verified the whole229B
inverse/184B golden and reports no source/API/ownership/oracle defect. Its
written current source receipt will be read ordinarily when available; that
is not a new Root readiness gate.

Current conclusion: no scoped source or test defect. Root's88847 quality and
actual target run remain the only authoritative execution path, still pending
in this receipt. Predicted rejecting behavior is not executed RED. Original
three HistoryValues arrays remain empty; the ten supplemental codec tests do
not supply real catalog/publication/read/hydration, full profiles/Linux or any
native8/initialR/E-T/public settlement closure. All BODY proposals remain
UNAPPROVED until genuine compiled R0 is saved and separately selected.

## CODEC1 final canonical R0 and genuine behavioral result

The preceding source-only review is completed here with the actual latest
three test refinements, equivalent private visibility spelling and authoritative
runtime. No source/test/shared-policy change or workload was performed by me.

I actually read/hashed technical Clippy packet8763034+2615/
8c7e20f33b7ce113b7a7d5adea5ca2e7f9fe34b61b13510493407fbc54ffc462.
Its four errors are two doc_markdown and two chunks_exact_to_as_chunks findings;
no behavior ran. Root corrected only the two HistoryValues enclosures and two
even-cardinality array-slice loops, then inserted the agreed last-byte-NUL
positive BEFORE any behavior execution. I independently bound all three changes:

- main19/821/2445d36f8baeaa33fd378abbfe6ee69dcb30ddf73d005b9dc0f0407d32d41b90:
  remove four backtick bytes ->ENTIRE817/b5b9bb47.
- structure232/10372/8b85c338520ccd5d0dbc7607d0f2262322a6370f4cc5b4d435c3d23886c163a4:
  reverse both as_chunks substitutions, including the zip's necessary iter(),
  removes13 bytes ->ENTIRE10359/57ee2b32. Both loops have already asserted exact
  even lengths, so fixed pairs/remainder behavior is unchanged.
- scalars137/4718/ad000d6f5e76ab70925d6ef7795b944423b1fc03702d6dfc67cdf891e782e56d:
  remove the68-byte/one-line positive tuple ->ENTIRE4650/dee1dc41. Its text is
  EXACTLY30 bytes; resize makes the first NUL the31st field byte, literal
  expected bits3ff0000000000000. The float function is now80 lines; overall
  ten-test/two-helper maximum remains83.

I also whole-read/hash-checked actual architecture packet8766859+5902/
ff72e533363503ec103fc31087f8ce675f732c6d325f0284ff98803774ae1300:
51 architecture cases passed and only complete_workspace_obeys_policy failed.
Actual source.rs200..208 blanket-matches first segment super even for child
field visibility; the message does not prove a real public module exposure.
Root changed ONLY four equivalent visibility spellings, after that terminal,
from pub(super) to pub(in crate::value::original_flow). Both name the SAME
model-parent ancestor, preserving feature-private fields and the public API.
Current model233/7791/23a3e61e175edb82f060eaffb1862f60da97dc5404cd1f32f95cda395e024423
has my own exact100-byte/four-spelling inverse to ENTIRE7691/2f0738de.
No checker/policy/permission/type/test was weakened. This is technical feedback,
not the decoder's behavioral RED. Future value-subtree imports/visibilities
must honor the actual crate::value::original_flow path rule, with no super or
closed-policy import shortcut/checker exception inferred from the BODY proposal.

All other canonical source/test bytes remain the previous reviewed hashes:
value775cd167, feature9885907d, wrappers42072ba7, contextce68cc67. Decoder still
always returns InvalidFlowValue; there is still no parser/helper/encoder BODY.
The eight current files and unchanged checked views/private owner preserve the
exact approved R0 contract. The complete parent229-byte inverse/184-byte golden
and all twelve test/helper names remain accepted.

I independently read and hashed the ENTIRE final eight-command packet at
8772786+13059/SHA2173ded078a3e2ade47c514a52c0496597ad76ea3e8464caf36358804be634f3,
including every header and exit/unchanged footer. Inputs are1577/
efad1205f603f064b307faa9f70469c003fe33d7d59296de8f0457e7bafcdc91 throughout.
Fmt, domain all-target compile, strict Clippy, architecture52, reference480,
progress120/132/550 and whitespace all EXIT0. STEP8 compiled and ran the actual
original_flow_value target, EXIT101/UNCHANGED true. This is not a missing
symbol, compile, formatting, classification or filtered-test baseline.

The entire target packet8781257+4563/
SHAd17da2c93fef371973b9ed26f6e72941d6c416d301347188fabe5d176d64d4ae
was independently read/hashed. I extracted all ten #[test] names from the
actual four child files and independently compared their full module/name
Counter to runtime output: EXACT conservation, one status each. Result is
1PASS/9FAILED and zero ignored/measured/filtered. The only pass is
wrappers::flow_value_rejects_compression_trailing_truncation_and_bad_lengths;
its complete negative loop executes against the intentional all-invalid stub.

The actual first failures are wrappers35 on empty valid raw input;
scalars33 on small integer1 and98 on positive-zero FLOAT70; structure39 on
the complete compound,103 on the first valid singleton key,145 on the four
distinct numeric keys; context45 on the first valid PID,102 on the initial
valid limits control,194 on the initial owned-document success check.
All report actual Err(InvalidFlowValue) or failed is_ok. Later original-value
classes, alias duplicates, numeric boundaries/legacy floats, foreign fields/
context lanes, resource/depth/capacity refusals and pointer/range tails are
HONESTLY MASKED. In particular, lifetime types compile but runtime borrowing/
pointer preservation is not yet demonstrated, and the new last-NUL positive
has not reached a decoder call after the earlier zero-float failure. The
helper-source review and original VM vectors do not substitute for that future
successful Rust execution.

I actually read PERF26530+6916/370d89e14c498c0e3fbabe3a87b2dd6fe83cd48c1901ddcccd5291f586017fc0,
which independently accepts the same pre-refinement source/inverses/geometry/
golden/oracles. Its subsequent direct messages bind the latest spelling/lint/
last-NUL changes and report agreement. It also actually read my UNAPPROVED
BODY38132+19089/cd3e251a and agrees on iterative ownership/work/key comparison.
One precise later helper implementation rule is explicit: charge the selected
TOTAL target capacity but pass try_reserve_exact additional=target-len, not
target-capacity, and call only when current capacity is insufficient. That
corrects no current R0 source and grants no BODY/helper permission. A final
written PERF outcome can be read ordinarily without another acknowledgment
loop or a Root SAVE gate.

Conclusion: the latest source/oracles remain accepted with no scoped defect,
and the rejecting baseline now has genuine compiled behavioral RED. Root owns
the actual SAVE and a SEPARATE concrete BODY selection/release; the earlier
private roster/tariff is still UNAPPROVED. No ledger source credit was added:
the three HistoryValues arrays remain empty in677d/120-132-550. The progress
packet explicitly retains runtime5903/4222/1681/1655 and incomplete parity.
Complete successful codec BODY, real publication/catalog/routed reads/full
hydration, profiles/Linux, native8 and initialR/E-T/public settlement remain
required. This finite review does not declare the full goal complete.

## A-HISTORY-VALUES-CODEC1-BODY1: exact post-RED selection

Root reports the genuine compiled R0 is SAVED locally as0f60bb1 BEFORE this
BODY. I already independently read its whole13059/2173ded0 packet, target4563/
d17da2c9 and exact ten-case1PASS/9FAIL Counter, and final written PERF4503/
c08f93f4. Current source models and all ten tests/two helpers are frozen.
This decision SELECTS the exact bounded implementation contract below for
Root's subsequent recorded release; it does not itself transfer source custody
or authorize execution. No new source has been created by this review.

### Exact write set and ownership boundary

ADOPT the concrete iterative algorithm/private roster in38132+19089/cd3e251a
with ONLY the precise resolutions in this section. The BODY write set is TEN
production paths under crates/wf-domain/src:

1. EXISTING value/original_flow.rs: replace only rejecting decoder BODY/docs;
   add the selected private module declarations, ValueEncoding and two helpers.
2. NEW value/original_flow/budget.rs.
3. NEW value/original_flow/context.rs.
4. NEW value/original_flow/scan.rs.
5. NEW value/original_flow/scan/frame.rs.
6. NEW value/original_flow/scan/scalar.rs.
7. NEW value/original_flow/scan/sequence.rs.
8. NEW value/original_flow/scan/foreign.rs.
9. NEW value/original_flow/equality.rs.
10. NEW value/original_flow/equality/scalar.rs.

FROZEN in full: value.rs185/775cd167 and its reexports; model.rs233/23a3e61e
including six models/five checked views; all five target files2445d36f,
42072ba7,ad000d6f,8b85c338,ce68cc67; Cargo/lib/policy/checker/ADR/ledger and
every unrelated source/fixture. No public signature, field, variant, trait,
view, error lane, test/oracle/name/helper change is included. There is NO
encoder, catalog, storage reader/writer, application hydration, fixture/probe,
unsafe/FFI/runtime dependency, generic hashing/interner or new public wrapper.
The three original HistoryValues arrays remain empty through this prerequisite.

All helper imports and restricted visibility paths must spell their real
crate::value::original_flow ancestry. No super path or crate::policy shortcut
may evade the actual value-facade rule; no checker relaxation is authorized.
Where a sibling needs a worker, use pub(in crate::value::original_flow), not
crate-wide/public visibility. Private model.rs ownership stays exactly as R0.

The source owner verifies every held member/absence before and after authoring,
then stops ALL ten buffers and its own handoff note before Root reads/formats.
Only Root executes format/compiler/tests/VM/Git and journals shared files.
Production files<=400 and functions<=100 after ordinary formatting are mandatory.
If the exact cut cannot fit naturally or needs another named artifact/helper,
send that concrete delta BEFORE creation; do not pack, drop branches or expand
an allowed module by inference.

### Exact private model/helper inventory

Exactly THIRTEEN named private worker/control types are selected, with the
roles/shapes defined in the proposal: ValueEncoding; Budget; ContextState;
FunctionBinding; Census; ScanTarget; Scanner; TermSummary; SummaryKind;
ParseFrame; CompareFrame; CompareStep; ListCursor. Standard local tuples,
ranges, Options and ordinary variables implement their fixed metadata; they
do not grant an extra named carrier/alias/callback. No type retains a reference
into its own movable buffer or owns recursive children/continuations.

ValueEncoding is Raw{start} or Etf{version_offset}. Budget holds immutable
limits and a checked monotone work count. ContextState holds reached-table
validation flags and a bounded Vec of FunctionBinding fixed metadata, with no
runtime lease. Census contains planned/finished node counts, edge count and
max encoded depth. ScanTarget is census-only or mutable result Vec borrows plus
expected Census. Scanner borrows one immutable input/context, ContextState,
Budget and target while owning cursor/counts/one flat ParseFrame Vec. Its
temporary borrow lifetimes may be distinct; none escapes in the public result.
TermSummary is a completed ID/tag/absolute offset and SummaryKind (Other,
Atom span/encoding, normalized Integer, Pid), not a cloned subtree. ParseFrame
holds one sequence/alias/STRING/identity/export/local-fun continuation and its
current sibling metadata. CompareFrame/CompareStep hold one sequence/list/map
candidate continuation/result, and ListCursor holds node/head position. The
types' fixed fields may implement those stated roles; no dynamic payload clone
or unbounded queue is permitted. Copy derives are limited to fixed scalar/
tuple metadata; owned scratch and public nodes/documents are not Clone.

The EXACT helper names/arities, excluding self, are ADOPTED from the proposal
with one clarity rename grow_stack -> grow_scratch, because it also grows
bounded function-binding scratch. No hidden constructor/helper methods are
implied:

- Feature root: decode_wrapper2, finish_document5, and the existing public
  decode_original_flow_value3 BODY replacement.
- Budget: new2, spend1, require_nodes2, require_edges2, require_depth1;
  free reserve_exact3<T> and grow_scratch4<T>.
- ContextState::new0; context::check_atom5, check_export7, local_node6,
  local_function5, validate_atoms3, validate_exports3, validate_functions3,
  validate_local_node3, valid_name2.
- Scanner: new6, run0, take1, begin_node1, emit3, slots1, write_edge2,
  push_frame1, accept_child1; frame::expects_atom1.
- scan/scalar: parse3, big_integer3, normalize_integer4, legacy_float3,
  atom3, binary3.
- scan/sequence: start3, accept3, string_step2.
- scan/foreign: start3, accept3, finish_identity3, finish_export3,
  finish_local_fun3.
- equality: validate_maps2, same_term4, compare_pair3, resume4,
  list_step3, push_comparison3.
- equality/scalar: scalar_equal4, names_equal3, integers_equal4,
  bitstrings_equal4, foreign_equal4, references_equal4, functions_equal4.

These are worker-state borrows and fixed metadata arguments, not public APIs.
For example, check_atom takes context/state/input/completed summary/budget;
check_export uses the same borrowed state plus module/function summaries and
arity/site metadata, retaining the actual export tag offset. Local function
metadata may group its node template and module summary in an ordinary tuple,
so census never needs a graph lookup. Fixed-width reads use the one take method
and checked standard conversions, not unnamed reader/accessor methods.

Only immutable literal grammar/profile constants are included: FSV2 bytes,
ETF version131, actual ETF discriminator values, atom-char/free-count255,
reference-word cap5/local3/pid5, old creation0..3, source64-bit small integer
range[-2^59,2^59), local PID/ref/port bit guards, FLOAT99 width31 and initial
scratch batch8. These may remain inline match/arithmetic literals. There is no
mutable registry, new deployment default, secretly broader source cap or
additional named model disguised as a constant table.

### Scanner, final arenas and exact error precedence

Use ONE grammar in census and build, one continuously spent Budget for the
whole call. All five zero profile fields refuse InvalidLimits before input
work. Positive operational ceilings are not all preallocated or multiplied
in advance; checked actual requested counts/products/total work refuse Limit.
Check complete stored len AND the moved Vec's capacity against encoded cap
before new owners. The caller owned that buffer already; no initialR or
acquisition authority is invented by this check.

Wrapper raw/unknown/incomplete/escape1 branches produce one Binary node and
no context lookup, preserving the original buffer and exact one-level range.
Typed mode validates version131 and exact EOF; compressed input refuses.
Census performs checked lexical/safe/scalar/foreign validation with one
prospectively grown continuation per encoded level and one current sibling.
Guard atom-only foreign fields before parsing arbitrary children. Check an
arity's minimum possible remaining wire lengths before reserving impossible
future slots; malformed/truncated lengths are InvalidFlowValue, whereas real
resource-count/work exhaustion is Limit. No unavailable giant payload is
charged as a physically executed read or allocated on its header's authority.

Entered encoded tag depth is root1/child+1, INCLUDING LIST0 aliases. A
STRING's synthetic integer/list nodes count as real model nodes/edges/work
but do not invent an additional encoded child tag per byte. begin_node admits
wire visit/depth; it must not conflate visited encoded terms with normalized
result-node counts. A zero-head LIST emits no extra semantic container and
resolves to its tail, while retaining actual encoded parse depth and work.

Drop retired census parse frames before final arrays. Perform one checked
fallible exact request for actual node count and one for edge count; reserve
no fictitious zero-length owner. Build reuses the same grammar and expected
counts. Reserve each compound's contiguous edge-ID slots before its child
walk; fill all slots and emit nodes in POSTORDER, children before parent.
STRING expansion, empty lists/tuples/maps, alias tails and foreign free values
must have identical census/build counts. At completion require complete slots,
exact totals/EOF and one root. No leaf copy, recursive owned tree or partial
successful document escapes. Context/function scratch drops before return;
the result owns the unchanged incoming Vec and flat arrays once.

Map validation is bottom-up in postorder IDs, retaining original public edge
order. Pairwise earlier-key checks call the iterative evaluator. Its one
candidate/sequence/list continuation at a time is admitted before push; no
recursive semantic equality, all-future-child enqueue, hash or rank model is
selected. Nested maps compare key/value sets independently of pair order after
their own uniqueness is established. Lists compare head/tail cursors across
STRING/segmented/zero-head aliases; improper tail types remain distinct.
Integer aliases normalize including sign truthiness, padding, zero and exact
i64::MIN; wider digits borrow the source range. Atom Latin1/UTF8 aliases compare
Unicode scalars without allocating Strings. Integer/float differ and finite
float bits preserve distinct +/-zero. Bit comparison masks unused last bits
and compares logical length, not raw encoded-tag equality. The nonlinear
candidate/structural work must terminate at max_work_units rather than claim
linear total CPU. Source-invalid duplicates return InvalidFlowValue only after
actual semantic comparison, never from a string/Raw::Other proxy.

### Captured tables and the bounded foreign-data profile

None denotes unavailable context, coherent Some denotes the closed captured
lookup set. Validate a reached table once for name widths/duplicate or
contradictory rows on the same Budget. An unusable Some capture returns
UnsupportedContext at that reached tag/absolute offset; it is not a fabricated
known-absent atom/export. After usable capture, known absence is the source
InvalidFlowValue. Unused tables are not traversed for a raw binary. Some is
mathematical compatibility input with explicit acquisition preconditions,
not proof of a live VM, authorization or availability. Never derive an
allow-any registry from encoded bytes.

Normal typed foreign DATA includes the selected PID/port/reference/export/
NEW_FUN families. Local node context is needed when local-versus-foreign
normalization/guards are reached; missing context is UnsupportedContext at
the containing tag, not falsely local=false. Safe atom/export guards preserve
source order. Old local-fun index/uniq and export arity use generic-term then
source-small-integer checks; NEW_FUN Size is IGNORED exactly as pinned source
and frozen Size0 test require. Source free count and old creation/ID/ref limits
are enforced; source64-bit small range is an explicit pinned profile, not a
host-usize guess. Node/function identity data is never invoked.

Canonical local-function entry key is(module,index,arity,uniq), with captured
or first-created old_uniq reused consistently during this decode. The state is
private pure metadata, not a published mutation of any VM table. Creator and
wire old_index are provenance rather than function-key identity. There is one
precise conservative foreign profile boundary: if DIFFERENT full entry keys
share(module,index,canonical old_uniq), pinned flatmap comparison and ordinary
function identity can diverge. Detect that at reached captured-table/virtual
binding validation, while the actual112 source offset is known. Return
UnsupportedContext for such unusable captured tables or UnsupportedProfile
for a newly encountered unsupported collision; do not guess equality, relabel
it corruption or add an offset arena/public model. Ordinary admitted function
identity plus environment remains supported, including the frozen five-family
data fixture. This restricted foreign edge is explicit incomplete profile
coverage, not a common-codec or BEAM execution predecessor.

Local PID/ref guards are static data checks under the captured node. Pinned
pid-reference reconstruction checks an embedded PID tag rather than a live
process registry; missing magic lookup falls back to reference identity and
does not create a Rust resource capability. Normalize retained/padded reference
words before exact comparison. No live owner/lease/NIF/foreign interpreter is
created. These are data compatibility rules; execution remains outside product
scope and representation/profile dispositions stay explicit.

Supported common tag families are97/98/110/111 integers;70/99 floats;
100/115/118/119 atoms;109 and valid nonempty77 bits;104/105/106/107/108/116
compound/list/map; and the normal foreign families101/114/90,103/88,102/89/120,
113/112 with the stated contexts/guards. Removed FUN117, malformed/unknown
standalone tags, compressed content and standalone cache-ref82 without a
distribution table are source-invalid. Native indices73/75 and LOCAL121 cannot
be validated with these captures: after available fixed framing/bounds checks,
return UnsupportedContext at the reached tag. Zero-word reference and empty
BIT len0/bits0 native edge profiles use UnsupportedProfile, not an invented
Source-invalid or dangerous native underflow path. Other malformed bit/length
fields still refuse normally. These exact unimplemented/context boundaries
never become hydration InvalidFlowValue fields or passing full profiles.

### Exact prospective work/reservation contract

ADOPT the proposal's concrete unit table: one per consumed wire byte per pass;
one per node/edge produced or visited; one per continuation visit/push/pop;
one per map candidate or context row; and one per additionally inspected
normalization/equality byte on EACH visit. Opaque span handling is a declared
logical tariff, not measured physical rereading. Admit checked bulk loop units
before initialization/consumption, and actual repeated scalar/context/key work
before each visit. Work/count overflow and exhaustion are Limit. No reset,
future refund, native Meter R/F, issued allocation or whole CPU/RSS theorem is
inferred from this purely operational ceiling.

reserve_exact3<T> accepts TOTAL target element count. If actual capacity is
already sufficient, do not allocate. Otherwise check target*sizeof(T) and
old_capacity*sizeof(T), admit one allocation-call unit plus those requested
owner/possible-relocation bytes BEFORE the fallible operation, then call
Vec::try_reserve_exact with ADDITIONAL=target-len, NOT target-capacity. Capacity
arithmetic/host byte-size overflow is Limit; an actual reserve error retains
ReserveFailed. Result arrays start empty and make their one counted request.
Initializer/ID writes also spend their actual selected visit units.

grow_scratch4<T> is reached only for needed=len+1 beyond current capacity.
Request min(ceiling,max(needed,max(8,old_capacity*2))) with checked arithmetic,
using that same total-target/additional API and relocation tariff. Parse frames
are capped by encoded max_depth; equality tasks and function bindings by reached
nodes/work as stated in the proposal, with every actual push funded first.
Do not reserve a whole deployment ceiling, enqueue all candidate pairs, use
one-slot reallocations without relocation admission, or permit len to exceed
an actual admitted request. Retired scratch drops before its next phase when
possible. Allocator excess capacity/latency is explicitly outside exact logical
counts; no capacity==requested, shrink, Box conversion, copied whole input or
fake OOM harness is authorized. Whole incoming Vec capacity remains independently
bounded exactly by the frozen profile/test.

### FLOAT99 resolution and unchanged-oracle validation

The earlier no-NUL fallback in the UNAPPROVED proposal is SUPERSEDED. The
actual4392/0373edb4 controls and frozen scalar137/ad000d6f require no-NUL31
root/nested fields to return InvalidFlowValue. A first NUL at ANY field byte,
including the final byte, terminates the numeric grammar; bytes after it need
not be UTF8/zero. Consume the full31-byte field regardless. Never read beyond
it or mutate the owned input.

Validate optional single sign, >=1 ASCII digits, dot OR comma, >=1 fractional
digits, optional COMPLETE e/E exponent with optional sign and >=1 digits, then
the first NUL. Normalize comma in a fixed31-byte local buffer, checked ASCII
str conversion and finite f64 parsing only AFTER the full prefix grammar.
Reject overflow/nonfinite/malformed input, preserve signed zero and accepted
underflow. This is not ScoreBound's prefix grammar or unchecked FromStr on
untrusted text. The bounded standard conversion call is part of the selected
logical handling tariff, not a proof of every internal CPU/stack/allocator
instruction. Pinned Rust nearest/ties-even and actual macOS bit vectors support
the selected profile; universal libc/FENV and Linux/source-profile equivalence
remain separate actual validation, not a new dependency/VM gate here.

Root next assigns one author ONLY these ten production paths, with every
frozen model/test/root/shared member held. All buffers STOP before canonical
format/independent full source/resource review. Root runs the SAME ten test
names/oracles and required checks; no expected bit, error, context, size,
depth/capacity or pointer assertion is weakened for GREEN. Actual results must
show the old negative pass preserved and exactly nine positive failures repaired,
then ordinary full-workspace/source-profile/Linux evidence remains required.
No original3 HistoryValues mapping or real I/O/public runtime completion is
earned by the codec callee alone. Root must record/read this selection before
release; there is no added generic readiness acknowledgment.

## ARCH-HVAL-CATALOG1: bounded next real-route proposal; NOT a creation release

Root's current BODY1 author retains sole moving decoder custody. I did not read
those moving sources or execute a workload, build, VM, Git or shared-file change.
This entry proposes the next storage boundary only; it selects no artifact for
creation and grants no encoder, catalog, application or public runtime authority.
The decoder's saved genuine R0 remains 0f60bb1. All three original HistoryValues
rust_assertions arrays remain empty until the complete real route is tested.

The next useful vertical slice is generated workflow values published by the
EXISTING accepted mutation and consumed state transaction, then read back through
an authentic ordered scoped session. It is not a second blob database, an
independent seeding API, or exposure of only the current inline record. Current
WorkflowRecord already owns payload/result/error; FWSR encode_fields/put_record
persists them, restoration moves them, and DecodedRequest.value_bytes(index)
borrows actual prepared command fields. FWSH currently lacks the corresponding
value references, while ActiveValueRef/ActiveTimeoutPlan remain pure descriptions.
Ports/blob.rs, storage/blob.rs, application/queries.rs and projection/history.rs
are still three-line reservations. The missing durable relation is historical,
generated and shared reference -> exact stored bytes + independent expiry.

Evidence reused and freshly checked: pinned HistoryValues107/d287d697 and test
47/85719d7d, real original3 packet5000/12056699, supplemental11999/c834ada0,
ValueStore's actual filtering/fallback, Keys' actual grammar, and real Router
routing. Fresh source reads include FlowValuePayloadTest19..220 and673..839,
flow_create652..667, flow_values103..109/311..345/414..501. Source chooses a NEW
versioned ref on FIELD PRESENCE, even if bytes equal; otherwise explicit/old ref
survives. Writes visit payload/result/error in that order. New terminal values
receive terminal_retention_until_ms; terminal refresh explicitly leaves OLD refs'
original retention untouched. The existing generated payload must remain mget
readable AFTER terminal record removal. Batch and created/claimed/completed
history observations require intermediate refs, not just final records.

### Producer and representation contract

Keep every existing opaque command Vec and public Base64 payload LOGICAL BINARY.
The publication adapter uses source binary encoding: unchanged bytes unless they
begin FSV2, in which case prepend exactly FSV2 + escape tag1. Strip-once decoding
then returns the original logical bytes, including an existing FSV2-looking value.
Never treat an arbitrary existing Vec as trusted ETF or double-encode it. The
catalog row itself holds source-stored bytes without a binary-only payload schema,
so a later separately versioned original-encoded/typed command input can use the
same row and reader. That later input requires an explicit prepared/log/schema
and intent contract, source-derived encoding/validation tests and common typed
publication; BODY1 does not authorize an encoder. No synthetic BEAM execution
bridge is a prerequisite for these data/storage responsibilities.

A private WorkflowValuePublication attachment to RecordBatchApplyPlan is the
candidate producer boundary. It owns only bounded item/kind/presence/version/
expiry descriptors and indices, not self-borrows into the plan or a second copy
of payloads. Construct it in verified Fresh decision paths from the actual
Operation and prepared bytes, check the matching accepted postimage, and refuse
incoherent attachments. This attachment confers no authorization by itself.
IMPORTANT: value_bytes is POSITIONAL: Cancel slot0 is error, Complete/Fail slot1
is result/error, Retry slot1 is error. fresh::stage_values flattens Option slots;
its flattened ValueRow position cannot determine kind or field presence. Neither
postimage byte equality nor final_indices is sufficient provenance.

For every real successful history step, evolve current refs using this explicit
supplied-field mask, generate the source key from actual partition/id/kind and
resulting version, and stage its exact stored bytes/expiry. Carry unsupplied refs
through claim/retry and no-input steps. Preserve all intermediate same-ID writes
by joining IndependentIndexedPlan.history_plans(), not only final_indices().
Publish final current bindings, EVERY new history binding, value rows, core/index
changes, outcome and control in the SAME existing redb write transaction and
Durability::Immediate commit. Rejection/no-op/replay/conflict/expiry/log replay
must not publish a new value or ref; uncertain commit retains the existing fence.
Equal supplied bytes still create a new versioned ref. No successful response
may precede the complete commit, and catalog rows are not a preupload authority.

### Exact candidate responsibility inventory for later consultation

These are named candidates, NOT a blanket implementation/file/helper release:

* Existing wf-ports/src/blob.rs: StoredFlowValue (exact stored encoded owner and
  per-value expiry), StoredWorkflowValueRefs (three optional complete ref byte
  owners), StoredHistoryEntry (core HistoryEntry + those bindings), ValueReadKey,
  ValueReadLimits, ValueReadSlot, ValueReadBatch, WorkflowValueReader. ValueReadKey
  borrows partition + reference; ordinary types carry no placement/read authority.
  Candidate read_values(&self, scope:&RequestScope, keys:&[ValueReadKey<'_>],
  at:TimestampMillis, maximum_inline:usize, limits:ValueReadLimits)
  -> Result<ValueReadBatch,StateTransactionError> is a real-session capability.
* Proposed private ports children blob/reference.rs, blob/encoding.rs and
  blob/publication.rs: source canonical reference grammar/rendering; binary-only
  prefix escape; the private accepted-publication attachment respectively.
  Reuse existing sha2 allowance in ports rather than add a domain dependency.
* Existing log/record_batch.rs plus its actual Fresh/independent/public-mutation/
  atomic-start/claim producers and state/transaction.rs: derive/check descriptors,
  carry current stored refs, and join the consumed transaction. Exact per-file
  edits/helpers must be frozen after a complete small source cut, not inferred
  from this responsibility list. Claim's carried refs must be covered too.
* Proposed storage/state/key/value.rs and storage/state/value/blob.rs: one
  scope-bound catalog key family and complete canonical value frame; existing
  state/key.rs/physical.rs and state/value.rs seams. Proposed physical family8
  and FWSV1 frame are reserved CANDIDATES only, not adopted format numbers here.
* Proposed storage/state/redb/values.rs plus existing commit.rs,
  commit/independent.rs and publication.rs: same-writer catalog publication and
  authentic bounded read implementation. Existing storage/blob.rs may reexport
  the reviewed concrete capability, never a standalone database/bootstrap.
* Profiled stored-record/history extensions use existing state/value/record.rs,
  record/scan.rs and history.rs, with StoredWorkflowRecord carrying explicit
  bindings. Candidate FWSR2/FWSH2 must preserve old-format entry points and make
  legacy-unavailable refs distinct from a known empty binding set. Keep core
  WorkflowRecord's existing inline semantics; do not silently replace its fields.
* Later application/queries/history_values.rs under the existing queries.rs seam
  calls the real reader and codec; projection/history.rs gains only an explicitly
  selected observation/watermark adapter. Neither reserved boundary is an excuse
  to manufacture I/O success or claim a full public query route now.

Source canonical keys use the SHA256/base64url partition tag and exact p/r/e/s/n
families, canonical bounded version spelling and65535 history key ceiling, not
ValueStore's4096 option cap or a new random hash. Initial generated p/r/e writes
are the smallest real slice. External/shared/named refs and their actual routing
remain REQUIRED follow-ons. A caller-provided partition/tag is never permission:
the adapter must verify scope, actual placement and the key's tag; a tag cannot
be reversed into an invented partition. Full cross-group reads need the real
routing/strong-read service, not a success callback or an unrestricted table scan.

### Snapshot, recovery, retention and compatibility gates

Physical decoder currently closes families0..7. Current snapshot/format.rs
ALREADY emits FWSP3 for FWSS6 and FWSP2 for FWSS5. I checked this actual current
code; an older FWSS5 maximum is false. Existing recovery checks table inventory,
manifest/origin/membership/control, semantic rows, indices and retained cohorts;
it rejects unknown families/layouts. Therefore a new catalog family and record/
history frames require ONE explicitly versioned layout transition, complete row
validation, cohort capture/receive/install/restore and snapshot format support.
Candidate FWSS7/FWSP4 is only a future registry choice; changing a header or
6->7 range alone is insufficient. Preserve pinned/alternate snapshot cohorts and
protected log-purge recovery. Snapshot/catalog.rs already names SNAPSHOT cohorts;
it is not an existing workflow-value catalog. Use the same rows/metadata custody,
not an unvalidated extra table which existing inventory rejects.

On a value-enabled layout, legacy plans with no publication attachment must not
silently omit reference maintenance. Decode/replay the real prepared entry to
produce effects, or explicitly refuse the missing capability/profile. Old
checkpoint inline bytes cannot reconstruct discarded earlier versions/history.
A reviewed replay/backfill from actual retained source evidence or an explicit
unsupported upgrade is required; never fabricate historical refs from today's
postimage. Restart, replay and snapshot restore must preserve identical ref bytes,
stored bytes and expiry, including intermediate versions and duplicate requests.

Expiry belongs to EACH value. Terminal cleanup cannot cascade-delete an older
unexpired generated payload just because its record/history is removed. History
trim likewise is not proof of unreachability. New terminal writes require the
actual captured retention witness; pure ActiveRecord has such fields, but the
reduced durable core does not yet carry that complete captured metadata. Do not
invent expiry from current wall time or re-resolve a changed policy. The first
nonterminal binary publication/read slice can use authentic no-expiry writes;
terminal-value expiry, captured-retention persistence and cleanup must be joined
before enabling the complete terminal/public profile. Shared/named TTL/ref-count/
orphan and retained-snapshot ownership remain explicit later lifecycle duties.
Do not implement a convenience record-delete cascade or unbounded silent GC.

### Ordered reads, owners and meaningful validation

Implement WorkflowValueReader on the REAL stable scoped transaction first. A
local authentic revision is not a quorum read barrier; public strong reads still
require M2/M3 accepted application/consensus authority and applied position. Return
the actual control/read watermark, never a fabricated freshness certificate.
ValueReadBatch has one aligned slot for EVERY requested key (duplicates included)
and owned buffers indexed by slots. Candidate slots are Missing, Bytes(owner
index), Omitted(stored byte length), StorageFailure. Scope/profile/admission
failure remains operational; unavailable data is not a business invalid payload.
Redb inline metadata can implement omission without inventing a filesystem path.
A future external-file profile needs real key/location/length/integrity/retry and
availability proof before activation; no local-only preupload or fake file refs.

The application collects refs in history order x[payload,result,error], first
occurrence dedups and filters the exact source classes, then uses real routing.
Invalid ValueStore key grammar yields its source nil positions. A real port may
guarantee exact cardinality; the compatibility overlay must still preserve the
source's zip-truncation behavior for an actually supplied mismatched result list,
not silently import adjacent ValueHydration's stricter batch-error contract.
Storage failure differs from missing until the exact redacted field rule. Keep
all stale unrelated/payload/error/omitted/missing flags except the source Map.put
updates. Size is STORED bytes including FSV2, never decoded payload size.

Fetch dedup does not eliminate decode ownership. BODY1 decoder consumes Vec;
retain source per-occurrence decoding and prospectively admit/copy the reached
encoded operand for EACH occurrence. Moving one unique owner repeatedly is
impossible. No decoded-document cache/shared-owner API is selected here. Returned
documents own their moved buffers; no borrowing from a dropped read batch or
self-referential result. Count full capacities, result slots, unique buffer owners,
reference keys, frame/envelope overhead, prospective temporary copies and all
scans under shared monotonic transaction R/C/work accounts before allocation or
I/O progress. max_bytes is an observation omission rule, not a total I/O/RSS cap.
Use fallible bounded reservations; no clone, hidden refunds, capacity==requested,
whole allocator/engine-page/cache or speed theorem is earned.

Next source-cut/TDD proposal is a grouped real-adapter target under
wf-node/tests/workflow_values/{main,publication,read,recovery,retention}.rs (only
files containing selected cases would be created). Candidate cases: accepted
create publishes binary refs/history atomically; same-ID batch preserves all
intermediate versions; carried refs survive claim/retry while equal replacement
changes ref; real ordered duplicate/missing/omission reads; replay/restart and
snapshot receive/install/restore preserve bytes/refs; terminal cleanup preserves
old unexpired payload while new terminal values obey their own expiry. The last
case requires the captured-retention predecessor above. Use actual filesystem,
prepared log and consumed state adapters, failure boundaries and preserved
source scenarios/setup; do not test fabricated callback successes. Freeze a
small exact artifact/type/helper/oracle batch before authoring; real callable
RED must be saved before its BODY. No new empty shells are approved here.

PERF direct reciprocal consultation agrees on the same accepted-operation join,
intermediate-history requirement, source-neutral catalog row, aligned ordered
owners and separately admitted per-occurrence decode copies. Its positional
ValueRow and value-enabled legacy-plan warnings are incorporated explicitly.
The written corresponding peer proposal will be ordinary-read once available;
there is no readiness or appendix-ack gate on Root's independent codec review.
The full endpoint still includes typed common publication, real full-value/history
routing and hydration, original setup/profile cases, snapshot/restore/lifecycle,
Linux/macOS, M2/M3 gates and the old eight native obligations. No partial catalog
or successful helper alone earns the three HistoryValues mappings or full parity.

## ARCH-HVAL-BODY1 geometry delta: identity_fields3 only

DOMAIN's concrete precreation request and Root's matching assignment select ONE
additional private helper in the ALREADY approved
crates/wf-domain/src/value/original_flow/scan/foreign.rs. I reviewed the supplied
fixed responsibility/signature and the saved BODY helper contract; I did NOT read
any moving decoder source or run formatting/compiler/VM/Git. Other BODY work may
continue. Root must whole-read this exact delta and release it by message before
creation; Root journals after author STOP to preserve the held coordination file.
There is no additional readiness acknowledgment or broader BODY release.

Select private identity_fields3 with the same existing Scanner lifetimes/error
alias as its caller:

identity_fields(scan: &mut Scanner<...>, tag: u8, words: usize)
  -> Result<(u32, u64, u32, std::ops::Range<usize>), E>

The tuple is creation, id, serial, reference-byte range. It is fixed ordinary
metadata, not a new model, public type, owner or authority. For a family which
does not use a component, preserve the caller's existing neutral representation
and do not add wire reads for that unused component. Reference ranges remain
absolute ranges into the unchanged encoded donor; do not allocate/copy a new
reference buffer or construct a foreign node in this helper.

Move ONLY fixed-width wire-field reading from finish_identity3: PID/port/ref
ordering, the actual tag-specific old/new field widths, checked numeric
conversions and checked reference word-byte arithmetic. Every consumed byte
still goes through the SAME Scanner::take1 and shared monotonic Budget in the
SAME source order. This split may not pre-read later bytes, skip charges, reset
a budget, add a second read, move malformed/truncation precedence, or replace a
prospective take/spend with an uncharged index. No lookup, local classification,
context table walk, node emit, arena/stack growth or other allocation is added.
Checked conversion/arithmetic failures keep their already selected error lane.
There is no fee reduction or new constant implied by moving stack metadata.

finish_identity3 retains safe/local-context checks and their order, old/local
ID/ref limits, source reference normalization, zero-reference profile handling
and emission. The raw wire-range helper must not enforce a new semantic limit or
normalize an identity earlier than that caller's original reached checks. Both
functions must naturally remain <=100 physical lines and foreign.rs <=400 after
Root's ordinary canonical formatter; no packing/waiver or hidden third helper.
All other selected helpers, the thirteen private BODY models, public six-model
API, frozen ten tests/two assertion helpers, actual profile/error/context and
encoded-depth/work/resource contracts remain unchanged. No new file, model,
alias, API, test, decoder cache, encoder or catalog permission follows.

Ordinary prior peer closure also completed in this assigned turn: I actually
whole/hash-read HvalPERF37949+12323/
77291c18be33e800bc564a85b4f8dd57e7c05fbccd845678bdd42a52c67087ad.
It agrees with catalog proposal15139/b37e5990, including logical-binary escape,
operation-kind provenance, intermediate histories, legacy gates and admitted
per-occurrence copies. Preserve its useful distinction: recovery checks the
catalog ENVELOPE/namespace/length/expiry/relations without requiring a live safe
atom/export context; malformed typed payload may later yield the exact invalid-
flow field, while corrupt storage/read failure yields the separate storage-error
field. Omission may bypass payload decoding. Existing real state_store filesystem/
delegated fault tests are suitable first component evidence; later node/query
integration remains required. Its insert_reserve opportunity is UNSELECTED, with
no reduced fees, gain or new source authority. No substantive peer disagreement
or further appendix/ACK loop remains.

## ARCH-HVAL-BODY1 canonical review and exact geometry/Clippy delta

I actually read COMPLETE stopped canonical BODY source in all ten released files,
the frozen233-line model, parent value.rs and all five frozen test files, and
DOMAIN4606+1753/c2fd256b23477ea30c0cfcac0ad3e18e8f60cd3b002b9f2645730ca967562041.
No source/test/shared metadata/build/VM/Git action was performed. Root owns the
now-stopped source and subsequent canonical changes. Only this note is written.

Canonical files actually bound before any delta (LF/bytes/SHA256):
- original_flow.rs146/4732/57883ecc605ae12c1dab2abc8f37db76713e035154eb9745929c7e88498bdfdf
- budget.rs115/3255/9871a2342c0b13c98bafb5b7bb6bdcdbb9c945d352a0323fa818d0073e782917
- context.rs353/10301/760758c0ec00a26c997ec6027431bb1a7f407dae61d7a56814acad97c78c9850
- scan.rs271/9328/6322fb932dc0f68acafc222d38349c7fd780db66052f89793eeb071834b7bf32
- scan/frame.rs66/1508/d96d54107d095e0d30e048ea0bdd6c31f39bff5a4a8a1fef0bfe6d196c4084a1
- scan/scalar.rs284/9155/bee3c40e350aa1423c5bc3b33ddaac325bd19ce19d9271090c4330c35d186f1a
- scan/sequence.rs193/5381/223f55c6bc276b6e30e04e63f3a27a2959ecb502060d195115e9d60187db6272
- scan/foreign.rs480/14303/4f7e95821881c51a32199e5b4514c7cc9c5ffd2bf42d9a5951f9b42ec7e2c233
- equality.rs332/10103/eac64e51f9c531f91ef11da41866fe4dbb71c47acf5254922d9c40f6aa59c0a0
- equality/scalar.rs342/9967/7c7433f6587cca8d3fd8534d2ed6c9046c8809ce932c4e99d4667a196ec40676
All paths are under crates/wf-domain/src/value; child names are relative to
original_flow/. My independent frozen-hash checks reproduce parent775cd167,
model23a3e61e and test2445d36f/42072ba7/ad000d6f/8b85c338/ce68cc67 EXACTLY.
Root's complete R0-root/nine-absence inverse is attributed to Root/DOMAIN; I do
not claim an independent byte reconstruction from the changed BODY prose.

### Confirmed violations and SELECTED narrow source delta

My independent physical-line brace census finds58 functions/13 private BODY
models, maximum112LF. Confirmed: context::local_function242..353=112;
foreign::start8..109=102; finish_identity193..304=112; foreign.rs480>400.
These are real400/100 violations despite ten passing tests. The first strict
Clippy packet6485/36df3e6a was actually whole/hash-read: five needless_bitwise_bool,
local_function nested tuple complexity, two too_many_lines, two needless frame
value arguments and one FLOAT_EXT doc-markdown diagnostic. No suppression,
packing, deleted blank-line trick, type-only lint alias or waived limit is chosen.

Root may adopt exactly this responsibility delta AFTER the actual terminal;
recorded selection precedes creation. It adds ONE private file and THREE helpers:

1. NEW crates/wf-domain/src/value/original_flow/scan/foreign/identity.rs receives
   the existing finish_identity3 and identity_fields3. Existing foreign.rs gets
   only its private `mod identity` seam and fully qualified import/call update.
   Use explicit crate::value::original_flow paths, never super. Narrow identity
   worker visibility to its foreign ancestor if desired; no public reexport or
   module/model change is implied. Existing identity_fields3 retains exactly its
   previously selected fixed wire-fields contract and tuple output.

2. NEW private normalize_local_reference4 in THAT identity.rs:
   (scan:&mut Scanner<...>, words:usize, first:u32, range:Range<usize>)
   -> Result<Range<usize>,E>.
   Move ONLY the current LOCAL reference block foreign.rs238..268: first18-bit
   limit, five-word PID marker/embedded-ID test and local three-versus-five-word
   retained range normalization. The caller invokes it only AFTER local_node
   resolves true. All actual spend4 calls precede the same reached byte reads;
   no lookup, emit, allocation, new profile or normalization of foreign refs.
   The first word was already read/paid by finish_identity and is not re-read.
   Range arithmetic stays bounded by the validated<=5 words/take range; checked
   arithmetic is acceptable without changing any reached valid input outcome.
   finish_identity keeps wire read, old creation/ref guards, zero-ref profile,
   context/local classification, PID/port limits and final emit in that order.

3. NEW private start_local_fun3 in EXISTING scan/foreign.rs:
   (scan:&mut Scanner<...>, tag:u8, offset:usize)->Result<(),E>.
   Move the current112-only start header29/field conversion/count/minimum-node/
   slots/ParseFrame::LocalFun initialization block14..57. It obtains the same
   current next_depth and performs the same funded take/slots/frame operations
   in the same order, with Size deliberately ignored. Existing start dispatches
   this branch then returns None. No extra model, reader, frame or public API.
   Other identity/export start branches remain literal except ordinary dispatch.

4. NEW private check_function_collisions5 in EXISTING context.rs:
   (context:&C<'_>, state:&ContextState, input:&[u8],
    info:(TermSummary,i32,u8,[u8;16],i32,u8,usize), budget:&mut Budget)
   -> Result<(),E>.
   The flat info fields are module,index,arity,uniq,CANONICAL old_uniq,tag,offset.
   Move ONLY current context.rs298..332: captured rows then per-decode bindings
   collision checks. Each row still spends33 before the same digest/name work;
   errors retain the same UnsupportedProfile tag/absolute offset. This helper
   has no allocation, mutation, canonical lookup or binding append. local_function
   retains reached table validation, atom extraction, captured+virtual canonical
   lookup, calls this collision check, then the existing fallible binding growth/
   append. It must NOT append before collision validation or move checks into
   the free-value walk. Full function binding remains canonicalized before env.

Flatten EXISTING local_function5's info to the SAME flat seven-tuple above,
with its fifth component incoming old_uniq; caller finish_local_fun3 supplies
exactly the former values. The collision helper receives resolved canonical
old_uniq instead. Same five-argument function, no new tuple model/alias/API.
For ALL five bool diagnostics, precompute BOTH equality operands (arity then
16-byte uniq), then combine with &&. Keep the existing prepaid33 and evaluation
order; simply short-circuiting away the second comparison is not this selection.
Borrow `&ParseFrame` in finish_identity3 and finish_export3 and their actual
accept callers; copy fixed scalar/Copy-summary fields as needed, no frame clone.
finish_local_fun keeps consuming its owned Range/frame. Add only the scalar
header backticks around FLOAT_EXT for the doc lint. No error/fee/test weakening.

This yields exactly61 selected functions with the SAME13 private models, public
six-model API and eleven BODY files (including the new identity child). No other
helper/file/type/alias/test is selected. Root's natural formatter and census
must demonstrate every production file<=400 and every function<=100; projected
splits are not a waiver if actual output still exceeds the bounds. Root remains
sole writer and must rebind hashes/run the ordinary same-oracle checks afterward.
No catalog/storage/encoder or new fixture authority follows from this delta.

### Complete semantic, ownership and resource inspection of current BODY

No additional substantive defect found within the explicitly selected common /
ordinary foreign DATA profile. One grammar drives census and build with one
monotonically spent Budget. Both share immutable context and per-decode canonical
function bindings. take checks length then admits bytes before cursor progress;
begin_node enforces encoded depth; slots/emit/write_edge admit/count before
mutation. Completed children always precede their parent node IDs. Census stack
drops before result reserve; Build requires matching finished/edge/max-depth
counts and exact EOF. All returned edges are checked before map publication.
The private document owns the unchanged moved input Vec and flat arenas; getters
check ranges/IDs, no borrowed context/read-buffer lifetime escapes, no recursive
owned tree/drop chain or source-runtime execution is created.

Zero-head LIST aliases return the tail node while charging EACH encoded wrapper
level; STRING expands integer heads/list tail into counted synthetic nodes without
pretending extra encoded depth. Tuple/list/map edges retain wire order. Map
validation visits postorder nodes bottom-up, then pairwise prior keys. same_term,
resume and list cursors compare one continuation/candidate at a time, including
segmented/improper lists and nested map sets independent of entry order. Integer
tag/padding/sign/zero aliases, Latin1/UTF8 atoms and STRING/LIST aliases compare
semantically; int/float remain distinct and float bits preserve distinct +/-zero.
No raw-byte map-key proxy, host float==, recursive comparator, hash/rank optimizer
or source map traversal promise is present.

Scalars preserve i64::MIN and wider trimmed magnitude ranges, nonzero sign
truthiness, finite Float70 bits, raw/escaped FSV2 branches and exact ETF EOF.
Float99 first finds NUL within31, accepts arbitrary tail bytes, copies bounded
scratch, checks the source dot/comma/complete-exponent grammar, then converts
finite f64; no-NUL rejects and last-byte NUL accepts. Current Context validates
only reached captures, distinguishes unavailable/unusable Some from closed known
absence, pays lookup/name/digest work and retains canonical function old_uniq.
Unsupported73/75/121, zero-reference/empty-bit profile edges and function-key
collision lanes remain explicit; no Unsupported value is silently called valid
or corrupt. Foreign PID/port/ref/export/localfun nodes are identity DATA only.
I additionally checked pinned external.c dec_pid/port/ref/export/newfun and
64-bit erl_term.h PID bit layout; u32 serial already spans its32-bit local serial
range. NEW_FUN Size is skipped in actual source, as the unchanged Size0 fixture
requires. This review does not add a live foreign-runtime dependency.

reserve_exact prospectively spends target*sizeof(T)+old_capacity*sizeof(T)+1,
then calls try_reserve_exact(target-LEN), not target-capacity. Scratch grows only
when needed in bounded checked batches; no all-future-child enqueue or allocation
to configured ceilings. One budget covers both scans, context, normalization,
comparisons and result/scratch growth; work exhaustion is operational Limit,
actual reservation errors retain TryReserveError. Input LEN AND moved CAPACITY
are bounded. No exact allocator capacity, exercised OOM, universal CPU/heap/RSS,
physical allocation, asymptotic-linear total work, speedup or full input-profile
proof follows. Pairwise semantic equality can be nonlinear and is work-bounded.

### Actual first target and technical quality boundary

I actually whole/hash-read target8786044+1161/
c9d4ce261250b06f7c61079496025d3c069df3993da2240a465a464b75f0bd32.
It is10PASS, no failed/ignored/measured/filtered. My independent static test-name
set and exact R0/GREEN status dictionaries contain the SAME ten names: preserve
only wrappers::flow_value_rejects_compression_trailing_truncation_and_bad_lengths
as its prior pass, and flip exactly the other nine FAILED->ok. R0 actual4563/
d17da2c9 remains saved before BODY. Previously masked numeric, graph, foreign,
context, limit/depth/capacity and owner-pointer assertions now finish, including
all five-family field checks and the independent184-byte compound.

This FIRST target result does not make the current source geometrically accepted
or full-quality GREEN. Root's subsequent quality18475 terminated101 on the exact
11 technical Clippy diagnostics above after fmt/check0, with1586/9f54b1d7 unchanged.
No later quality steps or post-delta source result is inferred. Geometry/source
refinement must retain every frozen oracle and be ordinarily re-read/re-executed
under Root custody. No original3 HistoryValues mapping, typed publication/catalog/
real I/O/hydration, full source setup/profile, Linux/macOS or old-native8 completion
is earned here. PERF actual written findings will receive ordinary reciprocal
read when supplied; no generic readiness/append-ACK gate is introduced.

## ARCH-HVAL-BODY1 work-tariff correction to the frozen A review

PERF's subsequent concrete findings are confirmed against the complete A source
I read. They SUPERSEDE my prior no-additional-defect conclusion for three reached
work omissions. The actual first10 semantic passes remain valid; they did not
prove complete internal budget accounting. Root has been told immediately, before
postterminal source changes. Select these THREE bounded existing-helper fixes:

- equality.rs validate_maps59..62: the candidate budget.spend(1) is followed by
  ids[row] AND ids[prior]. Selected tariff is one candidate +two edge visits;
  change that pre-read spend to3. Existing map compare_pair156 already uses3
  for the same candidate+two-edge cohort. No debit after either read.
- equality/scalar.rs foreign_equal199: BEFORE matching the two child node gets
  for foreign node atoms, spend2 for those two node visits. Top-level pair-node
  admission does not fund a second distinct child-node lookup. functions_equal
  already pays its corresponding two-child lookup before its match.
- equality.rs list_step285..288: cursor normalization paid each node it inspected,
  but final dispatch re-fetches BOTH final nodes. Spend2 BEFORE this final match.
  Do not claim prior visits fund repeated visits or add a new cursor/view model.

No new helper/type/field/API/test/file, result ownership, source equality rule,
error class or fee reduction is selected by these fixes. They monotonically add
rights-consumption at the actual reached site and can correctly make a tight
max_work_units profile return Limit earlier. Keep all frozen ten oracles unchanged
and rerun under Root custody after the already selected geometry/Clippy changes.
These are ordinary substantive budget corrections, not a new readiness gate or
permission to infer full physical allocation/CPU/RSS, catalog or public parity.

## ARCH-HVAL-BODY1 revised B prospective-order and parser-tariff correction

During COMPLETE revised B read (changed six bound to Root's current hashes),
equality.rs119..123 contains one additional concrete admission-order defect:
sequence/localfun-free comparison constructs its pair with TWO edge gets and
dereferences, THEN spends2. Select moving that EXISTING spend2 immediately before
let pair, retaining the same total debit, helper/API/model/test and branch order.
This was unchanged A code missed by my earlier review. Root was notified at once;
no read or mutation is attempted during Root's equality edit window. The three
previous distinct edge/node omissions ARE correctly paid before their B reads.

Root also correctly requested the original exact table, which says one per
continuation VISIT/PUSH/POP (BODY selection1360), rather than a retroactively
narrowed completeness claim. My direct suggestion that parser pop+semantic
processing could consume ONE unit was too weak and is explicitly SUPERSEDED.
Select these finite additional admissions in EXISTING scan.rs, no new artifact,
helper, model or public API:

1. accept_child's existing spend1 BEFORE frames.pop becomes spend2: one pop and
   one semantic processing visit through its dispatch/worker. Keep that debit
   before the pop; no post-processing funding or refund.
2. run's STRING branch existing pre-pop spend1 likewise becomes spend2: pop plus
   its string_step semantic continuation visit. Synthetic byte/node/edge work
   remains separately charged exactly as already selected.
3. run actually peeks the top continuation to choose STRING handling, then (on
   the ordinary-tag path) peeks it again for expects_atom. Admit EACH reached
   nonempty-frame peek with1 BEFORE last()/payload pattern inspection. The
   simplest concrete rewrite keeps BOTH existing inspections: test is_empty
   metadata first, otherwise spend1 then evaluate the existing last()/pattern
   into a bool; branch using that bool. Repeat this same paid pattern at the
   existing expects_atom site after begin_node. Do not debit an absent frame,
   merge two actual reached peeks for free, move begin_node/error order, add a
   frame cache/model, or change any grammar/context/source diagnosis. The run
   function remains naturally below100 lines; Root verifies its actual format.

These are distinct high-level continuation operations. They do not purport to
count each Rust scalar field read, is_empty metadata, pattern arm or compiler
instruction. One semantic continuation processing visit covers its worker's
fixed dispatch/field handling; it does not erase the separate pop, push or
actually repeated frame peek. Equality's existing pre-pop1 then resume1 already
fund that corresponding distinction. Parser/equality work remains continuous
across both scans and all comparisons; a tight profile may now correctly reach
Limit earlier. No full physical CPU/stack/heap theorem or native paidR follows.

Root has the exact bounded existing-source corrections and owns their beforeimage,
postterminal edit/freeze, actual hashes and unchanged-oracle rerun. This also means
scan.rs joins Root's explicit source-write/hold accounting; it is not still a
held byte-literal file AFTER these admissions are applied. No original workflow
mapping, catalog release or new readiness/ACK gate is introduced.

## ARCH-HVAL-BODY1 final canonical source and focused execution acceptance

This bounded receipt closes actual revised-source/focused-outcome review, not
full workspace or original HistoryValues parity. I wrote ONLY this existing
architecture note. No source/test/shared metadata/build/compiler/VM/Git/probe
was run or changed. Root's full11 session41164 is live and uncredited here.

I actually read COMPLETE changed six B files, then COMPLETE final Scan/Equality
C files after Root STOP. Final binding (under value/original_flow):
context382/11254/6cafc1a6cfd205f4079621519b3d080114a578681a4f4770819b6445ed31b72b;
scan/foreign320/9156/36b56f7f1383ef627b7095ee5a04959eb2629672bbbceda50acb43a58ab238a0;
scan/foreign/identity185/5636/3d018265f83417084f8fc34f92f6b826c1c67f4f891d6c607404e1a9098d30d9;
scan/scalar284/9157/166c4f96dc306a4367fab5355e58e7049c39bd092cfc1e9f02e043ac5f1d4127;
equality/scalar343/9989/adbc4824edd8ffe077688885ed4e0a196b4dcec05f0e2fa3ba1c6a74d552222a;
FINAL scan285/9885/d23d764627d559ca6b5f04ceabda2694f8d4e7115622eb93da457ebde4f96057;
FINAL equality333/10125/7b310daa6ab1eeb4f9301dbef411b1a718b717d8118878d63ece420078645450.
Other BODY source/public parent/model/frozen oracles remain as previously read;
Root's1587/a5a9a6f3 freeze and explicit held-file proofs bind the execution input.
Scan is correctly counted among final changed sources, not still called literal.

My independent byte reconstructions reproduce ENTIRE pre-refinement A files:
context353/10301/760758c0 by reversing the flat tuple/helper extraction/five
precomputed boolean pairs; foreign480/14303/4f7e9582 by rejoining its actual moved
workers and reversing only the selected extraction/borrow/call changes; scalar
9155/bee3c40e by removing only the two doc backticks; equality10103/eac64e51 and
eqscalar9967/7c7433f6 by reversing their selected three debit fixes. I also
independently invert FINAL equality's statement move to ENTIRE B10125/0a1deb64,
and FINAL Scan's four admitted continuation sites/ordinary control spelling to
ENTIRE A271/9328/6322fb93. These are in-memory reconstructions; no old-source
file, temporary artifact or Git read/write was created. The BODY-root/R0 inverse
and complete inventories remain explicitly Root/DOMAIN-attributed as before.

Independent final census: ELEVEN BODY files,61 functions, SAME13 private BODY
models. Maximum file is context382; maximum function is sequence::start94.
local_function88, finish_identity84 and run56 are natural physical spans.
The one selected identity child/three helpers are the only roster additions.
There is no hidden tuple type/alias, public API/model/test change, packing,
line waiver, suppression or visibility/policy relaxation. Context precomputes
both comparisons before && after the same33 debit. Captured then virtual
canonical lookup still precedes collision checks and any binding append; function
canonicalization still precedes free env decoding. Identity fixed field reads,
local-only normalization and final emit preserve actual A order/operands.
Borrowed identity/export frames introduce no clone or owner. NEW_FUN Size,
Float99 comma/NUL grammar, encoded alias depth and original ranges are unchanged.

All EIGHT distinct selected correction sites are now present and prospective:
map candidate1+edge2; foreign child-node2; list final-node2; sequence/local-fun
pair edge2 BEFORE the pair reads; both actual nonempty parser frame peeks1;
STRING pop+semantic visit2; ordinary accept_child pop+semantic visit2. Existing
push_frame pays its push, equality separately pays pop then resume. The literal
visit/push/pop table is preserved, not narrowed after execution. Fixed scalar
metadata/is_empty control and several pattern arms INSIDE one admitted semantic
processing visit do not assert a separate physical-CPU unit per Rust instruction.
No additional distinct unpaid continuation operation was found in the complete
source review. Every repeated actual node/edge/byte/peek retains its declared
admission; no future credit, refund or reset covers the previously found gaps.
Tight operational work caps can correctly refuse earlier after these repairs.

No remaining scoped source/API/semantic/resource defect was found. The previous
complete common/ordinary-foreign DATA, iterative equality, postorder ownership,
continuous work/scratch/count and captured-context review remains applicable.
The errors for invalid data, unavailable/unsupported context/profile, operational
limits and real reservations stay distinct. No blanket source-profile support,
foreign execution, original initialR/whole E-T, allocator/RSS/CPU/stack theorem,
measured speedup or exercised reserve-failure claim is earned.

I actually whole/hash-read FINAL8803947+9423/
8fff26260841829df4e6bf30431fbf7ce58031c4154e902056ffe76d758e92a1 and its
entire target8804145+1161/
bd86cd04d2cc5f262117d88b93250f8c75aff493b179103872d6241d47fde36c.
My footer parse finds ALL EIGHT EXIT0/UNCHANGEDTrue under the SAME1587/a5a9a6f3
input. Target10PASS/zero other statuses; fmt,domain all-target check,strict domain
Clippy,arch52,reference480,progress120/132/550 anddiff all pass. My independent
static-source/R0/final name/status dictionaries have exactly the same ten names:
only the nine saved R0 failures become ok; the original negative wrapper pass
stays ok. Every frozen numeric/context/foreign/depth/capacity/ownership-pointer
and semantic-key tail now executes. Earlier first A target success and B eight-
step success are separate honest predecessor evidence, not the final fee/order
proof. The final packet is the evidence for these final bytes.

I also actually whole/hash-read PERF50272+11233/
012953dd36eb86967816199e4fa6c607c344c77e418720533f83875b6062105c and its
61505+1325/7dfee82e4f98633031dac7fb437955f8c03d32227fab5152533f7654b9659220
supersession. The earlier combined parser-pop/process paragraph is explicitly
superseded there and in my3355/afe5 record. Direct PERF revised-source findings
agree on the actual prospective fixes and no further distinct unpaid frame
operation. Its revised written receipt may be ordinary-read when published;
no extra appendix or acknowledgment loop is required for Root progress.

This is accepted scoped CODEC1 decoder evidence. The actual original three
HistoryValues arrays remain EMPTY: real catalog/versioned publication, stable
ordered reads/routing, complete hydration/overlay, retained refs/expiry and
snapshot/restore are still required. Full workspace conservation belongs to the
separate actual41164 result. Original setup/profiles, Linux/macOS completion,
old native8/public opening/settlement and the overall goal remain unfinished.

## ARCH-HVAL-REF-BINARY1: exact staged prerequisite selection before creation

This selects a coherent NEXT pure prerequisite, not catalog/publication authority
or completion of HistoryValues. No artifact except this existing note was created
or mutated; no build/VM/Git/test/probe ran. Root first saves the completed CODEC1
increment, then owns the separate release/one-writer custody. All original three
HistoryValues arrays stay[] through this prerequisite and until the real route.
The complete required next path remains same-transaction versioned publication,
current and EVERY intermediate history refs, ordered genuine reads/routing,
retention/recovery/snapshots and separately versioned common typed input/encoding.
Existing command Vec/Base64 bytes remain LOGICAL BINARY, never implicit ETF.

### Actual source and policy basis

I read actual pinned Keys relevant complete families:1..115,687..710,710..937,
Support125..180 and ValueStore254..316, reusing the prior full HistoryValues /
read-routing evidence. Keys SHA remains bf9cbe38; Support67ea918f; ValueStore
0c4dcc83 at66586c. Required distinctions:
- value_key?/value_ref_kind recognizes p/r/e/s using LAST colon before version,
  nonempty opaque ID, canonical decimal0..9007199254740991; IDs may contain
  colons/NUL/nonUTF8. Named n has EXACTLY encoded-owner:encoded-name:version;
  both decoded components nonempty, canonical unpadded base64url.
- Accepted tags are f, fa:<canonical0..255>, or f:<canonical unpadded43-character
  base64url digest decoding to32 bytes>. No trim, UTF8 normalization or padding.
- Keys.value_key? itself has NO65535 ceiling. HistoryValues/Router applies that
  separate key limit. Recognize valid shape above65535, then gate actual output/
  I/O separately; do not silently change the source predicate.
- Omitted/nil value_key partition uses auto_partition_key(ID), CRC32(ID)%256.
  Named omitted/nil uses OWNER ID. This differs from explicit :global ->{f}.
  Explicit binary __flow_auto__:<canonical0..255> selects that auto bucket;
  malformed auto-looking bytes are SHA256-hashed WHOLE, not rejected/coerced.
- Source binary encoding is identity except a logical binary starting FSV2 is
  prefixed with FSV2+tag1 ONCE. Bytes after it remain untouched, even if they
  resemble type2 ETF/compressed/invalid data. Empty binary at stored max0 is valid.

Actual checked custody: ports/blob.rs3/120/3392a4d1; ports Cargo19/368/bf2b3422;
architecture/policy.toml128/5041/b1eb1ec2; source/purity.rs44/1665/6698c05c;
architecture-tests tests.rs595/19027/133a8041. All proposed children/test files
below are actually absent. Cargo already allows exact sha2=0.11.0/no defaults/
features; no new dependency or Cargo/TOML edit is selected. Actual module-level
checker has NO separate sha2-in-blob prohibition, but blob is NOT yet in the
pure ports-module prefixes. ADR006's old purpose was private log hashing; it does
not itself document this new source partition-tag purpose. The narrow missing
purity classification and source-purpose rationale are selected below, rather
than misuse a prepared-log digest or open a hash edge into domain/wire/client.

### Exact R0 artifact and public contract roster

SIX author paths ONLY for callable R0/models/frozen tests:
1. EXISTING crates/wf-ports/src/blob.rs (four public models plus private children
   and five explicit function reexports; no lib.rs edit, blob already public).
2. NEW crates/wf-ports/src/blob/reference.rs (THREE rejecting callable functions).
3. NEW crates/wf-ports/src/blob/encoding.rs (TWO rejecting callable functions).
4. NEW crates/wf-ports/tests/workflow_value_bytes/main.rs (imports, forbid unsafe,
   mod references/mod encoding only; automatic Cargo target, no Cargo seam).
5. NEW crates/wf-ports/tests/workflow_value_bytes/references.rs (five tests).
6. NEW crates/wf-ports/tests/workflow_value_bytes/encoding.rs (three tests).
No helper/constant/extra child shell in R0. Later BODY-only children are named
separately below; no catalog/application/storage/domain/test fixture changes.

EXACT four public models, no manual methods/constructors/Display/Default:

OriginalValueRefKind = Payload | Result | Error | Shared.
OriginalValuePartition<'a> = Automatic | Global | Explicit(&'a [u8]).
OriginalValueRef<'a> =
  Regular { tag_prefix:&'a [u8], kind:OriginalValueRefKind,
            id:&'a [u8], version:u64 }
  | Named { tag_prefix:&'a [u8], owner_component:&'a [u8],
            name_component:&'a [u8], version:u64 }.
OriginalValueBuildError = InvalidInput | Limit |
  ReserveFailed(std::collections::TryReserveError).

First three derive Clone,Copy,Debug,Eq,PartialEq; Error derives Debug ONLY.
tag_prefix is the COMPLETE borrowed f:{...} prefix including closing brace.
Named component views are the ORIGINAL ENCODED base64url spans, not alleged
borrowed decoded bytes. No decoded-component Vec is needed to recognize their
canonical grammar/nonemptiness. Version is the parsed mathematical bounded u64.
These ordinary public values can be fabricated and convey no source provenance,
namespace/placement/read/write/durable-reference or allocation authority.

EXACT five public APIs (numeric suffixes here are arity, not identifier text):
- parse_original_value_ref(input:&[u8])->Option<OriginalValueRef<'_>>.
- render_original_value_ref(id:&[u8], kind:OriginalValueRefKind, version:u64,
    partition:OriginalValuePartition<'_>, maximum:usize)
    ->Result<Vec<u8>,OriginalValueBuildError>.
- render_original_named_value_ref(owner:&[u8], name:&[u8], version:u64,
    partition:OriginalValuePartition<'_>, maximum:usize)
    ->Result<Vec<u8>,OriginalValueBuildError>.
- original_binary_stored_len(input:&[u8], maximum:usize)
    ->Result<usize,OriginalValueBuildError>.
- original_binary_stored_parts(input:&[u8], maximum:usize)
    ->Result<[&[u8];2],OriginalValueBuildError>.

parse is the source BINARY shape predicate + borrowed view, with no length cap
or allocation. Nonbinary source acquisition is outside this &[u8] API; do not
pretend a nonbinary source input was parsed and rejected. The constructors
render CANONICAL ELIGIBLE references: regular ID/nonempty owner+name and version
0..MAX inclusive. Source Keys' raw integer renderer can also print negative /
unbounded versions or empty components which FAIL value_key?; that wider invalid
renderer domain is explicitly outside these typed constructors, not silently
called ported. Source p/r/e/s/n canonical eligible rendering is covered fully.
Both constructors reject those invalid fields as InvalidInput, then checked
measurement/overflow/output>min(maximum,65535) as operational Limit. maximum0 is
valid as a cap (all real keys exceed it); no arbitrary InvalidLimits variant.

Binary len/parts have NO reference/Router65535 cap. Their maximum is caller's
explicit stored-byte bound; checked input.len()+5 only on FSV2 prefix, overflow
or excess ->Limit. Empty/max0 succeeds. parts returns [static FSV2+1 prefix OR
empty slice, the ENTIRE unchanged input slice]. Static prefix coerces to input
lifetime; payload borrows its backing, not a transient descriptor. No allocator,
callback, joined Vec, decoding, source-term coercion or input mutation occurs.
len and parts must agree, including unknown/incomplete/raw-looking FSV2 values.
Borrowing can retain a larger caller backing; it is not proof that backing is
small, released, already funded, or equivalent to native/initialR accounting.

R0 is deliberately honest: parse ALWAYS None; all four Result functions ALWAYS
Err(InvalidInput), using underscore inputs. Types/fields/functions are genuinely
callable and compile before behavioral tests. No parser, SHA, CRC, base64, escape
logic, private helper or body-only child is created yet. Save the actual compiled
same-eight-test RED before separately releasing BODY; technical import/lint/
classification failures are not behavioral RED and stubs need not fail every
negative case. No guessed six/eight/failure count is asserted before execution.

### Later BODY-only exact source cut and private roster

After genuine R0 SAVE, change only those three production homes and add TWO
private children:
crates/wf-ports/src/blob/reference/base64.rs and
crates/wf-ports/src/blob/reference/tag.rs.
reference.rs privately declares them; use explicit crate::blob paths. No public
module, extra model/alias, hash facade, encoder-for-ETF or callback. This gives
five natural production files, all<=400LF/functions<=100; estimates are not a
waiver. If actual ordinary formatter exceeds these bounds consult the exact
additional responsibility before creation, never pack/drop cases.

EXACT BODY functions: existing five APIs plus FOURTEEN private helpers =19:
- reference.rs split_tag1(&[u8])->Option<(&[u8],&[u8])>;
  parse_version1(&[u8])->Option<u64>;
  decimal_length1(u64)->usize;
  reserve_key1(usize)->Result<Vec<u8>,E>;
  put3(&mut[u8],&mut usize,&[u8])->Result<(),E>;
  put_version3(&mut[u8],&mut usize,u64)->Result<(),E>.
- base64.rs encoded_len1(usize)->Result<usize,E>;
  valid1(&[u8])->bool;
  encode_into2(&[u8],&mut[u8])->Result<usize,E>;
  digit1(u8)->Option<u8>.
- tag.rs valid_tag1(&[u8])->bool (INNER tag without outer f:{/});
  render_tag3(OriginalValuePartition<'_>,&[u8],&mut[u8;49])
    ->Result<usize,E>;
  crc_bucket1(&[u8])->u8;
  auto_bucket1(&[u8])->Option<u8>.
E is only an import alias for OriginalValueBuildError, not a new type alias.
No methods beyond specified derives. Named immutable BODY constants are only
MAX_KEY_BYTES65535, MAX_VALUE_VERSION9007199254740991, BASE64_URL_ALPHABET64 and
AUTO_PARTITION_PREFIX; FSV2 bytes/CRC IEEE polynomial may remain inline literals.

Use first source marker, exact tag validation, last regular colon and exact named
three-part split, all borrowed. Base64 validation checks URL alphabet, impossible
length remainder1 and canonical unused tail bits; digest additionally length43
(decodes32). No decode/reencode temporary Vec. Version parser uses checked bounded
ASCII accumulation, canonical zero/no leading sign/zeroes. Regular IDs are never
UTF8-decoded, trimmed or split at their first colon. Auto bucket parser recognizes
only canonical0..255; CRC32 is standard reflected IEEE initial/final xor, not a
random platform hash. render_tag writes its complete prefix into FIXED49-byte
stack scratch: Global5 bytes, auto<=10, SHA256 tag49. Named base64 writes directly
into the final admitted key, no encoded-owner/name temporary buffers. Decimal
uses fixed20-byte local scratch and no to_string/format allocation.

Render validates semantic fields and computes checked sizes before ONE real
try_reserve_exact on empty output, then initializes/fills only that bounded Vec
(with actual capacity>=requested, never capacity==size assertion). A zero-filled
exact-length Vec is acceptable for checked in-place writing; count that pass,
not a fictitious allocation reduction. put/encode_into check their target ranges;
no unsafe/set_len, implicit Vec growth, unchecked writer cursor or second full
escaped-value/frame buffer. SHA/CRC/input scans remain proportional to borrowed
input lengths; output cap alone is NOT a CPU/input-byte cap. Real caller admission
must own all input/backing/work before these helpers. This prerequisite does not
mint an opening budget/F/G/permit or total allocator/stack/CPU/RSS bound. Real
ReserveFailed is propagated; no deterministic fake-OOM/capacity-overflow harness
or claim to have exercised allocation failure is selected.

### Frozen eight-test roster and source-oracle custody

No test helper/fixture/new module beyond the grouped target and two children.
references.rs exact five test names:
1. original_value_refs_parse_all_five_families_and_borrow_original_spans
2. original_value_refs_reject_noncanonical_tags_versions_and_named_components
3. original_value_ref_rendering_matches_global_automatic_and_hashed_partitions
4. original_named_refs_encode_opaque_owner_and_name_once
5. original_value_ref_key_limit_does_not_change_shape_recognition
encoding.rs exact three test names:
6. logical_binary_parts_preserve_plain_and_escape_fsv2_prefix_once
7. logical_binary_stored_length_and_zero_cap_boundaries_are_exact
8. logical_binary_parts_borrow_original_payload_backing

Use independent literal source bytes/versions/kinds, assert Some/Ok and exact
length/kind before pointer/range checks. Cover p/r/e/s/n; global/auto0/255/hashed;
opaque colon/NUL/invalidUTF8 IDs, encoded named components; version0/MAX and
invalid negative/sign/leading-zero/MAX+1; invalid digest alphabet/tail bits,
padding/empty named/extra separator, malformed auto input HASH fallback. Parsing
well-shaped65535 AND65536 keys remains Some; construction exact65535 succeeds,
65536/caller bound refuses Limit. Explicit nil-vs-global choice is tested, never
conflated. Existing automatic and explicit-auto spelling round trips are source
observations, not actual routing/placement authority. Binary literals cover empty,
plain/incomplete FSV, exact FSV2, modes0/1/2/unknown, opaque bytes and nested-looking
escape: no decoder is needed for the independent exact stored-byte oracle.
Stored max0/size boundaries and returned lengths use literal expected counts.
Borrow witness keeps input backing outside a temporary slice-descriptor block,
then uses both spans and pointer/content afterward; no vacuous empty zip. All
functions<=100 and test files<=600 naturally, no packing/coverage removal.

Root after CODEC SAVE owns original inline oracle capture BEFORE this R0 writer
needs its final literal vectors, using actual Keys/Support and no new probe file:
regular/named omitted-vs-global, CRC ID123456789, explicit empty/binary partition,
canonical auto0/255, malformed auto00/256 and opaque partition; named opaque
owner a:FF/name00:; predicate regular/named canonical/invalid versions and both
65535/65536 sizes; binary prefixes above. Known standard CRC/SHA examples are
not called actual pinned VM results until Root captures them. Put resulting
literal constants directly in existing selected tests, not a SUT-generated oracle
or an extra fixture artifact. This is one concrete normal integration step, not
an open-ended research/ACK gate or reason to hold the saved codec checkpoint.

### Exact Root-only module-policy/source control footprint

NO Cargo.toml, architecture/policy.toml, dependency version/features, log hash
visibility, domain/wire or public lib.rs edit is selected. Root alone owns:
- EXISTING crates/wf-architecture-tests/src/source/purity.rs: add ONLY the
  blob prefix to pure ports modules, including descendants through current under.
- NEW crates/wf-architecture-tests/src/tests/blob_policy.rs: ONE named test
  workflow_value_bytes_stay_pure_and_server_side, no helper/fixture.
- EXISTING crates/wf-architecture-tests/src/tests.rs: ONLY mod blob_policy seam
  (current595 leaves room<=600; no unrelated assertion rewrite).
- EXISTING docs/adr/006-private-prepared-command-codec.md: append narrow rationale
  for source SHA256 partition tags in private blob reference implementation,
  unchanged exact dependency/server/purity boundary and no command authority.

The one guard uses existing source_check literals: positives for blob,
blob::reference, blob::reference::tag, blob::encoding with safe slice/Vec/scalar
operations and existing SHA import; negatives for std::fs/std::time/std::thread
at these same prefixes, plus client reexport of crate::blob and worker direct
wf_ports::blob access. Existing exact SHA package graph/feature/source drift and
forbidden domain/wire edges remain unchanged. Root runs actual guard BEFORE new
pure prefix to expose that technical policy gap, then adds ONLY prefix/rationale
and reruns the SAME guard positive; no pure-all-ports blanket. That M0 classifier
RED/PASS is separate from callable reference/encoding eight-test behavioral RED.
Root coordinates these shared edits while no author/source workload is live,
then captures the exact author hold and releases only six R0 paths. No two writers
or held-file rebase is inferred. Every later BODY source is separately released
after real RED SAVE, normal review and independent resource consultation.

PERF/DOMAIN received this finite direction directly; independent resource/API
consultation is normal next evidence before Root release, not a generic approval
or appendix-ACK loop. This note is the concrete selection; no source is written.
No original3 mapping, actual publication/read/expiry/routing/snapshot, typed
encoder/input path, Linux/macOS/profile, initialR or native8 completion is earned.

### REF-BINARY1 prospective tag-cap refinement (same staged roster)

Independent PERF actual whole/hash-read of16522/e280f005 agrees with the exact
four-model/five-API/six-path R0/eight-test/purity selection and eligible-renderer
boundary. Its source/resource consultation also confirms opaque inputs, canonical
B64 pad bits, IEEE CRC32 (not CRC32C), separate4096 options versus Keys bounds,
max0 empty binary and input-dependent hashing work. No source was created.

Select ONE precise private BODY signature refinement before creation:
reference/tag.rs render_tag FOUR arguments, replacing the prior render_tag3:

render_tag(partition:OriginalValuePartition<'_>, id:&[u8],
           maximum_tag_bytes:usize, output:&mut[u8;49])->Result<usize,E>.

The public renderer FIRST checks semantic fields, checked regular ID or named
base64 output lengths, decimal digits and separator sums. It subtracts those
fixed non-tag bytes from min(maximum,65535), refusing Limit on underflow, then
passes remaining capacity to render_tag4. Prefix lengths INCLUDE f:: Global5,
auto8..10 and hashed49. Fixed Global/SHA/explicit-canonical-auto lengths must fit
BEFORE hashing or copying their bytes. Automatic ID CRC is needed to know whether
the bucket has one/two/three decimal digits: reject below minimum8 first, compute
the source CRC bucket, then check its EXACT tag length before writing. Do not
reject a fitting one-digit auto tag using a fictitious ten-byte maximum. This
is no permission to claim an output cap bounds the borrowed partition's SHA work.

Every actual checked overflow/cap check which is independent of the digest runs
before SHA and final Vec reserve/copy. Hash only after the known49-byte tag fits;
no expensive hash is done merely to discover that an already-known size fails.
Retain fixed49-byte scratch, one admitted output Vec and direct in-place named
B64 writing. No extra helper/model/constant/API/test/file is added: total BODY
functions remain19, with this helper arity4. All other public signatures and R0
rejectors remain literal; the tag/base64 children are still BODY-only after real
RED SAVE. PERF directly agrees on the lengths/order and no further concrete
resource disagreement. Its bounded written receipt is ordinary next evidence,
not an additional source-creation/ACK cycle. Root alone records/releases work.

## ENV-OTP28-RESTORE61: exact absent accepted-header restoration batch

Root's reused nativeWhole preflight failed BEFORE VM launch; no new Keys/Support
original goldens exist from that attempt. This finite environment-only decision
adds no product model/API/class/profile, compiler input, native build or proof
scope. I performed ONLY read-only metadata/content integrity inspection and this
allowed note append; I did not fetch/download, restore/write a header, run a VM,
compiler/test/Git/probe, alter a receipt or rebuild the .so.

My actual audit reproduces exactly305 receipt compiler_inputs,61 truly absent
leaf .h files and ZERO mismatches among244 surviving files. For each absent
entry os.path.lexists is false (not a dangling symlink), its parent already exists,
and its path is beneath the existing OTP src root. All61 join UNIQUELY to saved
source-inputs.jsonl entries of mode100644 with exact Git blob SHA1 and size.
Capture tools26 and first-party sources4 all independently hash-match. Existing
policy_runtime_capture.so71880 bytes matches saved native SHA256
 a362f921af1ca58310eb4d748bf1ac3f611f2d239413a6ce3136a5136e1e820a.
No surviving content change, loader/library mismatch or rebuilt native artifact
is inferred. Why headers became absent is not established by this audit.

Immutable anchors actually read:
- /private/tmp/ferricstore2-policy-runtime-capture-otp-28.3.1/native/build-receipt.json
  8779270B/SHA256bcd81ab56ed7d3b5828594bfda60e226cb2fbc8eb2fed28ccd01d59f492683db.
- SAME BASE/source-inputs.jsonl1947 rows/240204B/SHA256
  44fe76a2157cc37b4573d752e801283de5b65612b4ee781f1ff14953859d042b.
- Pinned repository erlang/otp, OTP28.3.1 accepted commit
  aa5b1d3366762ff8f420f2aa6a52604b481530c9. Saved mapping path/blob identities
  below are not a moving branch/ref or regenerated header recipe.

APPROVE Root restoring ONLY the61 absent paths enumerated below. Each relative
path resolves under EXACT existing base:
/private/tmp/ferricstore2-policy-runtime-capture-otp-28.3.1/src/.
No new directory, mirror/archive, source manifest, fixture, patch or receipt is
selected. Root may retrieve those exact PUBLIC Git blobs to memory through its
available read-only connector/cache. Root reports a working fetch_blob route for
atom.h4631B (blob08a525..., receipt32a864...) and in-memory dual verification;
that availability observation is explicitly Root-attributed, not my download.
There is no present retrieval blocker reported; other61 contents are NOT claimed
already fetched/verified/restored by this reviewer.

For EVERY proposed file before any file write: bind pinned commit/path to the
saved blob identity; require exact decoded RAW byte count and SHA256 in this table
AND Git blob identity SHA1("blob " + decimal byte length + NUL + raw bytes).
Preserve raw bytes/LF/encoding; no text normalization, guessed generated variant,
platform header substitution or content assembled from a nearby release. A path,
size or either-hash disagreement is a concrete blocker for that file; preserve
all existing inputs and report it, never relax the receipt or fabricate a VM
result. Verify all available bytes in memory before the restoration batch.

Immediately before creation recheck true absence. Never overwrite, replace,
rename away, chmod or delete a PREEXISTING entry, including symlinks. Use exclusive
no-replace creation for these leaf files only, source mode100644/ordinary644
(subject to normal process policy); no executable or directory permission change.
A newly appeared matching file can be audited/skipped without mutation; a
nonmatching/racing entry stops that path. If a newly created write fails, only
Root's OWN newly created incomplete inode may be rolled back; no other file may
be removed. Read back and verify each restored byte stream against BOTH hashes.
Do not leave partial content and call the repair complete.

Afterward Root repeats the SAME complete receipt preflight: all305 compiler
inputs present/matching,26 tools/4 first-party inputs matching, identical receipt
and source-map bytes and identical71880B .so. Do NOT modify compiler_inputs,
compiler_inputs_sha256/header_digest/native_sha256, rebuild/relink a NIF, refresh
the receipt, widen a source exclusion, bypass the reused preflight or infer any
new native/runtime acceptance. This restores already accepted source inputs;
only a subsequent real permitted oracle run can produce its source observations.
Rust M0 blob controls can progress independently. The six-path R0 writer is not
released by this environment repair or the failed preflight.

Exact allowlist columns: relative path | saved receipt SHA256 | pinned Git blob
SHA1 | exact byte count. The61 sorted TAB-delimited rows (including final LF)
have SHA2561d88337b578c6610c616702a3b60e6b062c3599df08c4538841def57b7639c60:

```text
erts/emulator/beam/atom.h	32a8641941dc5ea6b08633a76d4a53177a36b66ad24cb548aa18b9493850da9e	08a5256177c879d81338ed123263b50ba854ac37	4631
erts/emulator/beam/beam_code.h	3464860854325cb8fd96001e869a56134164767daa2343f98ceda8dff4985fa3	10dd5fe7276128be245fc908b5d1e469c26d73f1	5283
erts/emulator/beam/big.h	de05a3d78bacfba4250c80efa1ac56bc45a5c9202bd241155eec879cdc6632e6	6ec877a37d4a27887e66ba984c35cafd1475c069	5859
erts/emulator/beam/code_ix.h	42495c711d3df910cea7474e4583e822f09933471c6679f8f39514fa5e717c0e	565c8b539675277020534d70c6e144bb50b428a4	11312
erts/emulator/beam/dtrace-wrapper.h	e9c39d791a88b87c70a118eb8289591bc8f0afc8b33b79f5e46f32fcafd74d4d	f862618ed319b446731a306f2bff06f44d92f7fb	5056
erts/emulator/beam/erl_async.h	ca7f61bdfcd38c2e9eeb3cc20366d74b1ec020a43f4e616376cca821ec776995	eb89943449b625b2769ab62b3f54a9d9cfbd4e0b	1334
erts/emulator/beam/erl_bif_unique.h	2cd8cb50c5fde120f4d7319afcc3258ba2f06df0a6b4e79ba30f38cf6155f66d	1a3eddb6030f384fe893e954b07c656ad1de1a3c	15630
erts/emulator/beam/erl_binary.h	31ba99c1b20ee2c2e67ebc1654b65c6638ffa6372b6b4e972396c36adbfc80a5	6a7115f22391e62faba65c98b7da7fbaa984a510	13869
erts/emulator/beam/erl_db.h	32bc1544579d3743a5e034fb34c655f5a20a484f4d7ed2370c534f0761be5ae2	1c2e54c68a0314fb6bba7df18be2e4b1a0197b8d	10317
erts/emulator/beam/erl_debug.h	f197310b7810b3870bbcc507308f3ad38c3922e558b0019c9909c6dbcbefbd4f	8153feed9a2f647eaf951de4faf4621512ac227a	3170
erts/emulator/beam/erl_drv_nif.h	52417b60e2cb8f685545d2424e66e137cda4b59478b897a8fef14547e81dd02f	e98b3c1ad90482c5db8dda79f5f733c77821285b	6756
erts/emulator/beam/erl_dyn_lock_check.h	5205a2dac4057ada813ebc766b757286f4772156d0c77e067f2580dd3c53a187	dfb94162803b4213959d5346053168455ef0ae3f	1717
erts/emulator/beam/erl_fun.h	f59affc7351d82ba847e56104c5b9732571451b6d91594db7c42e893780e66c4	0f8fdeba319de236fabed4ccb62908dfa1c59718	4991
erts/emulator/beam/erl_gc.h	2ab46a9d8857102a6558130be2a8bfd9e26b37818f6b832af3e40f571818147f	2518aab70db3ae3ad89600133f6547be3c1cb877	6942
erts/emulator/beam/erl_hl_timer.h	fc3f7c60c20eba29287757c06087c51e17396cd9ebe796ce640e42f525bd01d5	0261cb89a1b993f79a2b3e40188f87b666616cc7	2919
erts/emulator/beam/erl_io_queue.h	7e40e0778fa75d864ed18462a15219eaeb6e008dcbce94d286de803189e5b190	614a5f880e4339858925c113c010aeb8a764d9c9	6391
erts/emulator/beam/erl_lock_check.h	52dcbe17449c73b68be24fddfbbc1599f1a4d05cb58dc53285dcfca78a6cb2a5	5eefa0dba1194ecddc87b338dc283e9daf32739a	4782
erts/emulator/beam/erl_lock_flags.h	890574b8c9a31a7d49a873f37ed89b431bf85e7b6b70188063bda9d22ad8f5f8	9d947a93b56a6dfa9b85221312374601ac1d40f1	2953
erts/emulator/beam/erl_map.h	346349550ff1e86df887c930afd22ef6bb8cf50af67e588a0f1db1ce37d48fc0	80dd84ae118c591ff6f84c8c7621b56233b5969c	8414
erts/emulator/beam/erl_msacc.h	2d4c59cb6748d3859e00c262f0408ea53f92552d6327e26841a0ba5727ef2c6c	c5ee23cd34e6a7331bd5bc4f0547c469716c3656	15691
erts/emulator/beam/erl_nif.h	1b6e2551504807d43e8b727c527096c7add8a498500841d774d5b3c325264c82	72f97a3979fbe6c4f6ba6abcd9bf6dc6c4cf5e74	13783
erts/emulator/beam/erl_node_container_utils.h	a7f0c4c49a3043042f4cd4c644f3d1ae6b70d9e49daf3a76e82d4509f0b3692d	8ec903dbab51c145f2b7b6fcd9f214f5153746dc	13783
erts/emulator/beam/erl_node_tables.h	b67b0e26740014b49da842982c210c2c69ea4c33c575f3544d889fdaada57c70	e3d8944462a92b5bdd65862af7bd033c01017dd1	11816
erts/emulator/beam/erl_port_task.h	9fd9ea30c9f1d7786a120624be7400f118c5260ef635e8af8e27d33e19574e4f	212096fc395eac2d37429d1d6a336a8d39877f88	8467
erts/emulator/beam/erl_process_dict.h	a27ab41585833b5e54a52734c21402274ce0ddfa6b96472be1a6cb4e5121aca8	9ea27a27af1be702c9d3a2f03a5630eec63475a4	1718
erts/emulator/beam/erl_ptab.h	a90826c7e6f32c8c7785b6013cf3771b871e07af0aecc35a3b7cb278465a7a09	1a0cb77bb9d9b6cf48e55c5604dc4e90fb59003f	14257
erts/emulator/beam/erl_sched_spec_pre_alloc.h	3dd3802d55eedf151ca0efe9042e5b1b94ae7b9da56966014ff6d455cce73010	1a2e8be049cd2c24db17af76ea5b1a15bdf7287f	6783
erts/emulator/beam/erl_sys_driver.h	4b3e43204e2e694c5b462ac43f1be8538fb5df1b5f6620179bed70a0fa862fbb	6d84d77d0c95875ffd0022a1532c43f76a69aaa5	2316
erts/emulator/beam/erl_term_hashing.h	34a40ab29c639428cae541d4c76f33478c99942a84ee2201f48e1a2806d07e38	72d3c6be5699b840b598a6da7fb65c50698c632b	2406
erts/emulator/beam/erl_thr_progress.h	33eccacc1c05e83c3594e34d94ef6094f3a8c0b870021ac054c64f93f582469a	89d6c25aa4f04f6690831a09c970a3922bcef9eb	9731
erts/emulator/beam/erl_time.h	eeba16d9038f19e3bd7b824ec9b7bdf988eeec6988ae2d6521d9969c666acce2	b560fa8c64e45531ce15d04b315397f60e93d354	16059
erts/emulator/beam/erl_trace.h	f19bc63a6d377ca44f947d434f93f4209d5eb1e5f464931606c49034dd9263fb	38d19b3d55f46cf9ba9d57283c049e07ff92bce5	13824
erts/emulator/beam/erl_utils.h	dad589935f12dacf7004f68588417df30e041b34499a8dc1e6336a0a292e78c4	f34c5a67bad024339d19da7f125eb66a04efd7af	9580
erts/emulator/beam/erl_vm.h	c362c2c222f5342bc0edd3b666ba58ef571dff6d6f37542169316c208fab9816	b9477af50e5d92290ca89a3cf1a22b862d635860	11233
erts/emulator/beam/erl_zlib.h	6da4ca5a99cca30a95a61584fdf0b74724804e6da048f25f5947c4061d9cfb04	6167add102e32928e1d8653ecf91a9dfc2d021ec	2216
erts/emulator/beam/error.h	5770e4e7c290081cc5f5cef96810b497442b3ca46761c982b432c9da418e8e3d	c37b5089ea8bb6af0d793fa9b21db38be5208310	7991
erts/emulator/beam/export.h	9be17e5c506b4b352eb070d790097218d5bc6174dcb7bcf20d63ca02e7c9d9fa	7a218f27b785cd5be2ccb403ee3d0ed5d340bd37	6216
erts/emulator/beam/external.h	5c8115922b43b6b5a01814683b5e9cb979bd752fa5c621295b18aba631846b4f	cfe4b956b2f556a00827b1be5ce883a87af75b53	7960
erts/emulator/beam/hash.h	9d4523310ac9362320b463cc48cefe28e6f5a46acdacd80229352a8252afbe22	c9216176e2dfb9f34a7dafda867cc92e2771d870	4404
erts/emulator/beam/index.h	ba80a66a63aec55768d234353ae870d300074ad51283db84882e93737f22a48c	61f4e608eb58f2f24f73b19a0cfc1e9f27d13d0a	2571
erts/emulator/beam/lttng-wrapper.h	bae3fe696f1e0bfecef8263ad6c20cabd19f0bc5c63927f088e8fef81c43e141	19836b54003b07dc49937786f9f6bfb40ed3699a	4142
erts/emulator/beam/module.h	77bb9228410fb8218885326c26286945f2670247cb52904804e8e23e6ea1c418	3449837e5d930a39f776b909a881bfdf3cb1f871	3960
erts/emulator/beam/register.h	30a639922109102f94ce364c0743ecf451a18365926c4fe8e8b88c90361cb561	785591a3317016e333bd29791546212d516dfe95	1733
erts/emulator/openssl/include/erl_md5.h	b9b2c05218d902842f17021c26d4ad39dd357af83ec1d506216c518f70e1e9d5	e0749241b5a7727ed78254bbe2f35787e215b5fe	1158
erts/emulator/openssl/include/openssl_local/md5.h	0d17bae572aa65c588e4b8b967e176c9f9a32c92ab0ce6845d3288b84e0ff329	c9b42ecb115c49b03898cf8014e111d796d163c8	2358
erts/emulator/sys/common/erl_mmap.h	88065258178fa71cba581a96655020ece70500020da4ffb109ea0234ee224afc	3218f67974982ffd4161e307ff4256c7b9403f27	7854
erts/emulator/sys/common/erl_mseg.h	ea3d88bf653293d64e17ba449f451623bc34218e776da880dcd4a1f5f2a84e72	024563ee2668388b0e5274cfdf35e26fd56fa4c4	3369
erts/emulator/sys/common/erl_poll.h	e07752c8044ddec116af68cfe446ae525e8cbc193ded85ce0f58b36fd8adfe78	de89d231ce75169f083410776a8228d31cba4696	11386
erts/emulator/sys/common/erl_poll_api.h	216fd9a4429163062742c83ad1195266a0b2aaa57a0925361fab334efe66256d	b4bd99a285c4cea5ce33ee99b1913989e6a38e70	5242
erts/emulator/sys/unix/erl_unix_sys.h	1d536b01ca03d79bc77ec2b9a8a63f9ea6b9bef5a7c797c9c26796a60e166758	a63f0b43f9e90bca419447f6a84848028e13eb52	11799
erts/include/internal/erl_errno.h	18c685e488656796280f8dac1c1d270eb4f75eaa2accdc2f725eaafc00a8bad5	2a03e4c7381b4d8500f32e2539514a667b86a619	1311
erts/include/internal/erl_misc_utils.h	713cc91a76a8adfc2d80ed03891cabcb977179a672fb079ab9d19363eaccb823	b15ac5c191092e2b9fea11869e3150d3096e2188	2751
erts/include/internal/erl_printf.h	64b3f6cac58954654f8a140582dc48466d0e544146c0ca78ab2da14e17a022b1	bbc4af7a7b68d8ec11a1d577852321e84822661b	2307
erts/include/internal/ethr_optimized_fallbacks.h	5fb17ded59c0a07a8d1aed1880830089dec5306bcca5287896c540859b3129f2	21eba6cda4c07cf7c75e631a7959e6435656a9c7	5643
erts/include/internal/ethread_inline.h	c296da363c365e68488cc1b8ff7653ee424cb9e0c25d202b4f11f4fe33d2e7e6	962a46139d4922ae0c941f1d3e4e9310e5163186	2743
erts/include/internal/gcc/ethr_atomic.h	bbb5f997aba5c7fe9570a78bfbaebd7f3aa3c7a774e8aacf05fb3bfd475ebc24	655056676cf635b7f17b5ba9a5917cedc42022ec	15431
erts/include/internal/gcc/ethr_dw_atomic.h	36bac82a3b1c2fc35b6cda62a89c57948094640fe00ebf7cfa1cc66bbe9f4315	ab38f0f5923a7f7d747d8380901361eb266bc16b	7775
erts/include/internal/gcc/ethr_membar.h	2e8d5d7c7639681c9cc7005a12ed427bbe6fe57e0961ebe445332578151db94c	7dea71be137d9d98fa796f0342d965fdd718f68c	10485
erts/include/internal/gcc/ethread.h	86b61a35b04ff5ccb7e142562f22b72c73184e09058b3a012c37579c39f075e9	dad78c0bfd54069678d0cef14e0ed8a4d6bcba0b	13563
erts/include/internal/libatomic_ops/ethread.h	331c7060e5bdc957eac4170a700c1083fee29eee35688b5748e63666469d6227	dffb0d34814580dcfdbf0e4ca831c32665b7b375	1288
erts/include/internal/pthread/ethr_event.h	8ef357ac55d1f3c71d38dc3b3f127821b03d99646d5d223dc09ed8fd11f9e977	f5764536651ba275cf263358fdbf3dc7361bc5f8	4612
```

No user permission, new acknowledgment gate, full parity claim or source-code
creation authority follows. Root owns the exact restoration and subsequent
read-only audit/normal original-oracle execution under its existing custody.

## REF-BINARY1 M0 actual classification guard review

Finite independent read-only source/actual-log review, no source/workload/Git or
shared-file mutation. I reread the selected16522/e280f005 contract and2341/7a4a79fc
amendment, the complete new guard, complete final purity child, sole registration,
actual source_check setup and Inspector path/facade rules. This writes ONLY the
existing Hval architecture note. No R0 author or BODY is released by this review.

Actual Root-owned guard:
crates/wf-architecture-tests/src/tests/blob_policy.rs38LF/1344B/SHA256
12bba58af3bceb781b6329a7a2fcbe3dd7022958f81d2ad7d50b07039727d9f1.
Exactly ONE named test workflow_value_bytes_stay_pure_and_server_side, no private
helper/fixture/new model. Function6..38=33 physical lines. Its four cases are
blob, blob::reference, blob::reference::tag and blob::encoding. Each first accepts
safe SHA256 plus borrowed-slice/scalar/Vec operations, then separately refuses
std::fs, std::time and std::thread with the standard-library diagnosis. This is
parsed-source capability rejection, not execution of filesystem/clock/thread
operations and not a promise that pure Vec code has no allocation.

The two server controls refuse client pub use crate::blob and worker direct
wf_ports::blob. They assert a nonempty finding rather than a particular message;
I independently followed the actual valid-source/known-module paths: source_check
uses runtime classifications, test_file=false and root_file=false; these inputs
reach respectively the existing client-port-facade server-capability rule and
worker-only-wf_ports::client rule. No malformed syntax, missing classification,
root-file unsafe declaration or unrelated size failure accounts for their result.
There is no current false-positive fixture or newly opened client/server edge.
The existing exact dependency/source/features graph controls remain separate.

Actual sole tests.rs registration596LF/19044B/SHA256
4ac3645e75922c1c6868bdde3b7795ae9830e34bf9b8e7567d37f8fabab5d38f.
My exact17-byte removal of mod blob_policy plus LF restores the ENTIRE prior
595LF/19027B/SHA256
133a8041c0efd146a64c150b19c5794d09af9d9d2bba0f81467d027e49234f8c.
No prior test/assertion registration or helper changed. Both tooling files remain
within600 and the new function within100, without packed code or a size waiver.

Final purity.rs45LF/1685B/SHA256
a83dcbc6bbe8de210cc86658abd11b428f003d7ced97c66d42a6c155d9060db2.
My exact20-byte removal of the single indented blob prefix plus LF restores
ENTIRE44LF/1665B/SHA256
6698c05c37606294acaf15139e0a1017237f5bf6126fd521dc59262f854c5cfc.
The existing under predicate requires an empty or :: suffix: blob and all its
private descendants become pure, not blob-like sibling names or every ports
module. Existing test exemptions, domain/wire/storage classifications, effect
allowlist, forbidden edges, aliases and runtime inspection remain literal. This
ADDS restrictions at the approved feature boundary; it does not allow an effect.
SHA remains the existing exact sha2=0.11.0/no-default-features dependency. No Cargo,
TOML policy, domain/wire/client facade or source-check rule relaxation is selected.

I read the final appended ADR006 section Pure workflow value references and
binary storage bytes: source SHA256 partition rendering, source CRC32 convention,
private child purity inheritance, unchanged server-only access, and no placement,
transaction/admission/durability authority. It matches the selected narrow reason
for the existing hash edge, not a new hash facade or arbitrary I/O permission.
Actual ADR whole106410B/1593LF/SHA25607d566161946f60f219144e63686b683086f8617dc858c8bc7d5cf85df42fd68;
new final section starts105439, length971, SHA25688c1a5081723742fc44dc1c09459c18a276a122c4fbf068d3320fa60317267d9.
I do not claim an independently retained full ADR before-image; the exact whole
purity/registration inverses above ARE independently reproduced from actual bytes.

Actual pre-prefix Root63346 packet is /private/tmp/ferricstore2-m2-source-pin-validation.log
8940546+1022/SHA256
d7fd1ed1a8efa77c8c686a8e235755055a64362b126cd01e20ec223a7a7b1c1f,
whole-read/hash-verified. Runtime1 test:0PASS1FAIL,44 filtered, no ignored/measured.
The safe first blob positive executes, then assertion27 fails specifically because
std::fs in blob had no forbidden-standard-library finding. This is the intended
M0 classification gap on the old prefix, not reference/encoding behavioral RED,
a source oracle, a missing-symbol failure or a weakened expected result.

No concrete scoped rule weakening, server leak, false-positive or geometry defect
found. ACCEPT the final source footprint and the genuine pre-prefix classifier
failure. A post-prefix same-guard PASS/full-quality result is NOT yet claimed by
this reviewer and follows ordinary Root execution; no additional approval/ACK gate
is added. Environment restoration is separate and remains Root-only. The six-path
R0 still needs its original vectors, real callable behavioral RED/SAVE and separate
BODY release. Original three HistoryValues mappings remain[]; actual publication,
read/routing/retention/recovery/hydration, typed input/encoding, Linux, public budget
and native8/full-goal obligations remain open. Finite review STOP.
