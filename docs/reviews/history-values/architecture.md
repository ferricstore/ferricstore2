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
