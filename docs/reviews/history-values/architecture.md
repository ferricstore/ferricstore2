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

## NEXT-VALUE-PROVENANCE1: concrete producer prerequisite and durable successor

Finite proposal/selection for the NEXT increment AFTER REF-BINARY1 BODY, not a
release of source/model/test/helper/layout or workload creation. The live six
REF R0 author paths were NOT read. Only this existing note changes. The prior
catalog proposal1435..1635 remains required direction, not already implemented.
I also actually whole/hash-read PERF37949+12323/77291c18: no substantive conflict
on operation provenance, intermediate histories, source-neutral stored bytes,
independent expiry, actual transaction/recovery or read/decode ownership.

### Stable source findings and exact missing relation

Actual stable record_batch344/686d964e, fresh207/ec3b93f6, phase168..254,
independent168/dd7e64c1 and public_mutation/apply164/5f1d635d were read at their
producer/drop sites; atomic_start160/8f0472c4 shares the same relevant sequence.
DecodedRequest.value_bytes201..219 is POSITIONAL. Cancel slot0 is error;
Complete slot1 result; Fail/Retry/RetryWithPolicy slot1 error; payload is slot0
for Create/Transition and supported atomic StartAndClaim. None is absent and
Some(empty) is supplied. fresh::stage_values flattens two Option slots, so its
ValueRow index is not a kind or presence witness. An unchanged-byte comparison
cannot identify a supplied field: equal replacement still means NEW version/ref.

Actual BatchPlan.items preserves original input order. IndependentIndexedPlan
results preserves every source index; enumerate BEFORE filtering Err/no-op or
history absence. history_plans().enumerate would renumber holes, and final_indices
would lose intermediate same-ID histories. A zero-field real Claim/Extend/etc
history still needs a row so later reference carrying reaches that observation.
The existing reduced core ALREADY durably stores inline payload/result/error;
this proposal does not claim those bytes are currently unpersisted.

The existing real consumed path was inspected: transaction/record_batch119..203
prepares once, takes one authentic exclusive guard, gates once, loads Fresh facts,
constructs GroupApplyChanges and consumes commit before returning acknowledgement.
commit233/5788bd4d checks actual predecessor/control; commit/independent79/20d99c22
checks original versions once, final records once and ALL successful histories.
publication296/1753832e serializes complete current fields and FWSH metadata;
Immediate commit and uncertain fencing already exist. These are the authority to
extend, not a separate preupload/seed database or fabricated storage callback.

### Choose a bounded producer increment, not premature catalog activation

NEXT-VALUE-PROVENANCE1 retains an OWNED NUMERIC effect description inside the
SAME selected Fresh plan while decoded command fields and each accepted step are
still available. It does not materialize references/encoded values, load a store,
resolve old refs, manufacture expiry or claim publication. The subsequent real
catalog/current+every-history/read/layout increment remains mandatory.

Exact FIVE R0 paths proposed for separate Root release after REF BODY/save:
- EXISTING crates/wf-ports/src/blob.rs: private publication child and explicit
  reexports of the following ordinary models/function only.
- NEW crates/wf-ports/src/blob/publication.rs: three models, five read accessors,
  genuine rejecting inspector. No BODY helper/child/constants yet.
- NEW crates/wf-ports/tests/workflow_value_effects/main.rs: imports/forbid unsafe
  and mod provenance/mod intermediate; automatic grouped Cargo target.
- NEW crates/wf-ports/tests/workflow_value_effects/provenance.rs.
- NEW crates/wf-ports/tests/workflow_value_effects/intermediate.rs.
No Cargo, dependency, lib, policy, ADR, source fixture, storage or public-wire
change is part of this R0. Existing blob purity covers the private descendant;
client/worker access remains refused. No overlap with the current sole writer:
these changes can begin only after that increment's custody/save/release.

Exact ordinary public models:
RecordBatchValueEffect with PRIVATE item_index:usize, version:u64,
supplied:[bool;3] in PAYLOAD/RESULT/ERROR order, terminal:bool.
It derives Clone,Copy,Debug,Eq,PartialEq. No payload/reference/namespace/owner
borrow or allocation occurs in a row. terminal means the accepted postimage is
terminal and later publication needs the proper retention witness; it is NOT a
deadline, TTL, captured-policy proof or permission to substitute outcome TTL.
RecordBatchValueEffects with PRIVATE steps:Vec<RecordBatchValueEffect>;
Debug,Eq,PartialEq only, no infallible Clone/Default/constructor.
RecordBatchValueEffectsError derives Debug only, variants InvalidChanges,
UnsupportedProfile, Limit and ReserveFailed(std::collections::TryReserveError).
This last lane describes the genuine private builder reserve cause; the borrowed
inspector itself reserves nothing. No deterministic OOM/fake capacity test or
claim of exercised reserve failure is selected.

Exactly five accessors: Effect::item_index(&self)->usize,
version(&self)->u64, supplied(&self)->[bool;3], terminal(&self)->bool;
Effects::steps(&self)->&[RecordBatchValueEffect]. Scalars may be const methods.
Private fields simplify valid views but supply no transaction/admission authority.

ONE R0 public callable API, with this FINAL borrowed-output signature superseding
my earlier detached-owned-output sketch:
inspect_record_batch_value_effects<'plan>(decoded:&DecodedRequest<'_>,
    plan:&'plan RecordBatchApplyPlan<'_>, maximum_effect_bytes:usize,
    maximum_work:usize)
 ->Result<Option<&'plan RecordBatchValueEffects>,RecordBatchValueEffectsError>.
The returned borrow depends ONLY on the plan, not the decoded verification owner.
R0 always returns Err(UnsupportedProfile), with all parameters genuinely callable
and no inspection/parser/allocator/body. Save actual compiled behavioral RED before
BODY. A valid real-history Fresh plan must ultimately return the SAME attached
owner; recomputing a detached Vec cannot make these tests pass while producers
omit the owner. No new mutable ID/range API or proof token is introduced.

### Same increment's exact BODY footprint and private responsibility cut

This is a prospective named BODY roster, not permission before the genuine R0
save. Existing publication.rs becomes the full inspector/model facade; add ONLY
NEW blob/publication/scan.rs and blob/publication/step.rs as private children.
Use explicit crate::blob / crate::log paths and explicit ancestor visibility,
not the facade checker's problematic super spelling. No generic callback, trait,
cache, encoder/catalog/store or additional models are proposed.

Existing BODY edit set besides blob.rs/publication.rs:
- log/record_batch.rs: PRIVATE value_effects:Option<RecordBatchValueEffects>
  field on RecordBatchApplyPlan, getter value_effects(&self)->Option<&...>,
  ordinary internal reexports and effects_apply_error1 conversion only.
- log/record_batch/phase.rs: Complete inspection yields None; atomic Fresh joins
  the built owner after accepted decision and BEFORE decoded field destruction.
- log/record_batch/independent.rs: identical joined lifecycle for V6.
- log/public_mutation/apply.rs: identical joined lifecycle for public V1.
- log/record_batch/atomic_start.rs: identical joined lifecycle for supported V7.
- log/record_batch/fresh.rs: one shared value_effects_budget3 helper for the
  existing producer coexistence calculation; it does not flatten/reclassify
  the ValueRow or change the reducer/business algorithm.
The four existing destructure/drop sites remain after the new numeric owner has
been built; do NOT retain all decoded payload Vecs through outcome construction
just to call an inspector later. No prepared command/intent/wire bytes change.

Exact proposed private BODY roster (numeric suffix denotes arity, not identifier):
scan.rs:
value_effects_bounds2(encoded_length:usize,item_count:usize)
 ->Result<(usize,usize),E>, returning planned descriptor bytes and work envelope;
build_atomic_value_effects5(decoded,control,&IndexedDecision,max_bytes,max_work)
 ->Result<Option<RecordBatchValueEffects>,E>;
build_independent_value_effects5(decoded,control,&IndependentIndexedDecision,
 max_bytes,max_work)->same;
walk_atomic_effects4(decoded,&IndexedDecision,&mut ValueEffectsCharge,
 Option<&mut Vec<RecordBatchValueEffect>>)->Result<usize,E>;
walk_independent_effects4 with the independent decision, same roles/result;
check_atomic_effects4(decoded,&IndexedDecision,&[RecordBatchValueEffect],
 &mut ValueEffectsCharge)->Result<(),E>;
check_independent_effects4 with independent decision, same roles/result;
validate_effect_entry3(decoded,ControlPlan,&mut ValueEffectsCharge)->Result<(),E>;
reserve_effects2(count,max_bytes)->Result<Vec<RecordBatchValueEffect>,E>.
PRIVATE ValueEffectsCharge {maximum_work:usize,work:usize} and ONLY
spend(&mut self,units:usize)->Result<(),E> support one monotonic work account.
step.rs: value_effect4(decoded,index,&ScopedPlan,&mut ValueEffectsCharge)
 ->Result<Option<RecordBatchValueEffect>,E>; supplied_value_fields1(&Operation)
 ->Result<[Option<&[u8]>;3],E>, with payload/result/error positions explicit.
E is an import alias of the selected error, not a new type alias.

Existing fresh.rs value_effects_budget3(decoded,workspace_limit,additional_live)
returns the real remaining descriptor allowance and checked derived work cap.
Existing record_batch.rs effects_apply_error1 maps actual builder InvalidChanges
into existing ApplyError::InvalidAuthority, unsupported into existing unsupported
encoding lane, Limit into the operational encoding/profile limit, reserve cause
into existing CodecError::Allocation. Preserve existing Copy/Eq apply-error ABI;
no retained business error or new silently ignored cause. Public inspector uses
its explicit E lanes; no Display/Error/manual trait/helper shell is selected.

Named work constants only EFFECTS_BASE_WORK=64, EFFECTS_ITEM_WORK=64 and
EFFECTS_ENCODED_WORK=4 in scan.rs define a conservative local profile
64+64*n+4*encoded_length, with checked arithmetic. This is a proposed logical
algorithm envelope, not caller credit or physical instruction/CPU/RSS proof.
Charge BEFORE each reached item/result/continuation, compared source byte pair
(partition/ID/supplied value), descriptor validation/push and existing digest
comparison; include failed/no-op results, do not reset between passes. Final
source/code census must check the envelope against the actual loops before BODY
acceptance; it is not an excuse for an unpaid extra scan/helper.

### Association, rows, refusal ordering and actual ownership

First bind plan.control().next().last_entry().map(|id|id.prepared()) to
Some(decoded.digest()), using current EntryIdentity::prepared48. Check actual
profile/cardinality/partition/ID and new record/history version agreement; do not
trust matching IDs/versions without the prepared digest or rehash the input.
Accepted atomic items are input aligned. Independent results are enumerated
before filtering, preserving gaps. Each valid real write/history gets ONE row in
input order, including every intermediate version and zero-mask carry step.
A supplied slot must agree with its actual accepted postimage field bytes;
omission does not mean compare equal and suppress. No unrelated whole record or
old payload scan is introduced. No-op/history absence emits no row. Mismatched
write/history structure/source association is InvalidChanges, not missing data.

Replay/LogReplay/IntentConflict/Expired, whole rejection and an all-no-history
result have NO attachment and inspector Ok(None) after applicable association
checks. A real history with missing attachment is UnsupportedProfile, never an
empty known binding. Mixed independent errors/no-ops do not suppress other real
rows. A row with all three false bits remains PRESENT for a genuine carry history.
The inspector checks attached count/order/scalars against the real source/plan
without allocation and returns the identical owner borrow, not a rebuilt result.

Builders do one complete immutable semantic/count prepass before output reserve,
then one checked exact result Vec reserve and a second same-predicate pass/push.
A single work account spans both passes and reached byte equality. Output count
is at most input count. Count*size_of::<Effect>() plus the new owner/Option fixed
metadata is checked prospectively; no temporary payload Vec, key, B64/hash/ref,
map/set, Box or owned list is needed. Fallible requested reserve may overprovide;
never assert actual capacity==requested or claim a total allocator bound.

Coexistence MUST be real: existing inputs uses decoder_bound(E,n)+192*n+256+B
plus caller additional_live. That old192*n does NOT silently pay this new Vec.
After the actual accepted decision/count checks and BEFORE the new reserve,
value_effects_budget uses that conservative still-live source/plan bound and
checked workspace subtraction. New owner bytes fit the remaining allowance.
Do not move a blanket new pre-admission ahead of an otherwise earlier business
rejection simply to simplify accounting. Before each existing late outcome/
response allocation, add the retained numeric-owner/Option term to that path's
actual existing late formula after decoded donors drop. No release/refund is
fabricated; input and output coexistence is explicit. Work derives from already
bounded actual encoded length/count, not forecast F or native/initialR credit.
This remains logical per-call ownership, not physical frames/pages/allocator/RSS.

### Genuine next tests, not a seeded or error-only I/O route

Eight proposed test identities, no helper/fixture module selected:
provenance.rs:
value_effects_bind_exact_prepared_entry_and_operation_kinds;
value_effects_distinguish_omitted_empty_and_equal_replacements;
value_effects_preserve_atomic_input_indices_and_kinds;
value_effects_emit_carry_rows_for_real_no_value_histories;
value_effects_refuse_crossed_profiles_and_do_not_invent_expiry.
intermediate.rs:
value_effects_preserve_independent_histories_and_source_index_holes;
value_effects_admit_continuous_work_and_descriptor_coexistence;
value_effects_borrow_only_the_plan_after_decoded_verification_drops.

They use actual prepared encoding, actual Fresh gate/reducer producers and real
accepted plans, all four selected profiles. Cover Cancel error-only slot0,
Complete result-only/Fail error-only slot1, Retry payload omission/error presence,
Some(empty), equal replacement, failed/no-op holes, same-ID intermediate versions,
full rejection/replay/no history, digest mismatch, limits and numerical owner
identity. Positive tests call the borrowed inspector on the actual producer plan:
missing joined owner cannot pass. Retain the plan outside a temporary verification
DecodedRequest scope and use the returned view afterward; no descriptor lifetime
is accidentally tied to that temporary. Natural files<=600/functions<=100 and
production<=400 are mandatory. If the complete draft needs an extra helper/split,
consult its literal responsibility before creation; no packing/coverage removal.

After real callable RED/save, BODY must make those SAME producer tests pass;
standalone inspection success is insufficient. Existing workspace/input/core
oracles remain. This prerequisite cannot claim filesystem or durable route tests.
The next durable stage reuses actual wf-storage/tests/state_store and its existing
FileBackend/DiskFaultBackend, prepared apply calls and consumed commit—not a new
seed/admin API or callback returning fabricated bytes.

### Concrete REQUIRED durable successor, not selected source authority here

The following minimal responsibility inventory remains a proposal; no fresh
family/version/module/type/trait/helper is created or activated now:
- ports/blob.rs plus proposed blob/catalog.rs and blob/read.rs for exact stored
  byte owner+expiry, explicit Known/LegacyUnavailable current/history bindings,
  aligned ValueReadBatch/slots/unique owners and actual WorkflowValueReader.
- state transaction/current-record representation and GroupApplyChanges validate
  the joined effects/bindings against the SAME original and accepted plan.
- proposed storage/state/key/value.rs and state/value/blob.rs; existing key.rs,
  key/physical.rs/value.rs and record/history/record-scan homes for canonical
  namespace-bound immutable catalog keys/frames and profiled current/history refs.
- proposed storage/state/redb/values.rs plus existing commit.rs,
  commit/independent.rs, commit/claim.rs, publication.rs, read and recovery rows:
  actual same-writer value insertion/current bindings/ALL history bindings,
  ordered genuine reads and full relation validation. storage/blob.rs reexports
  only the actual reviewed capability; no second database/bootstrap.
- existing snapshot format/capture/validate/receive/stage/install/restore and
  policy/origin/catalog dispatch must capture/check the added family/frames and
  retained cohorts together. Existing public application/query/projection homes
  get only their later separately selected real barrier/routing/hydration adapter.

Fresh source witnesses: physical106/fd179712 closes families0..7. Recovery rows182/
f3817172 rejects unknown families; recovery273/e3ae23fc and retained validation
know the current inventories/origin/control. Snapshot format324/dbe195f8 lines41..46
ALREADY maps FWSS6->FWSP3, FWSS5->FWSP2. Do not regress to an obsolete FWSS5 maximum.
Candidate family8/FWSV1/FWSR2/FWSH2/FWSS7/FWSP4 remain registry PROPOSALS, not adopted
numbers/range-only edits. The value layout needs one closed replay/recovery/
row-relation/inventory/snapshot-cohort transition. Preserve every existing legacy
reader/format and protected retained snapshot/log-purge boundary. Legacy inline
bytes cannot reconstruct discarded historical versions; no migration may label
unobserved old refs KnownEmpty. Real retained-log backfill/replay or explicit
unsupported upgrade is required. A value-enabled layout must refuse a Fresh or
claim plan that lacks the applicable complete publication capability.

Reuse existing ActiveValueRef/Refs where logical ownership is useful; do not
pretend those descriptors certify storage. TimeoutValueWrite carries authentic
expires_at only for its active timeout plan. Actual active/plan272..290 obtains
that from its traced original capture. It does not grant reduced Complete/Fail/
Cancel a TTL. The latter's command fields lack captured lifecycle retention.
Source flow_create611..638 uses max(now_ms,apply_now_ms()), captured positive
record.retention_ttl_ms and MAX_EXACT clamp; absence/nonpositive capture yields
no terminal expiry. This is NOT blindly requested_now+TTL, current wall clock,
request-outcome TTL or a freshly changed policy. Full terminal writes need the
complete captured lifecycle/version/intent/persisted-profile path first.

I read pinned flow_values311..345/414..501 and source payload test182..219:
new supplied p/r/e bytes get that record's expiry; terminal refresh414..425
EXPLICITLY no-ops old references. Helper427 is a definition, not evidence that
an unused generic refresh executes. The source test waits for terminal record
removal and STILL mgets the old generated payload. Never cascade-delete older
unexpired value rows on terminal/record/history trim. Same-ID many loops visit
all intermediate next/attrs, so every such new value needs its OWN independent
expiry. Authentic nonterminal no-expiry publication can be the first enabled
subprofile, with terminal/shared/named/external lifecycle gates explicit.

Existing command Vec/Base64 semantics remain LOGICAL BINARY: use approved one-level
FSV2 escape parts, not implicit trusted typed ETF or double encoding. The catalog
stores neutral encoded bytes and expiry; later versioned original stored/typed
publication must fit it without replacing a binary-only schema. Storage envelope
validation must not require a live safe atom/export table or call a valid stored
bad-ETF payload storage corruption; hydration may later report invalid-flow while
omission can skip decode. Common typed publication/encoding remains required.

Ordered read successor keeps one slot per requested key including duplicates,
real missing/expired/omitted/storage-failure origins, coherent actual control/
applied watermark, scope/placement and original routing. A local redb revision is
not a quorum barrier/cross-group authority. Unique encoded owners may back slot
indices, but the current consuming decoder needs a prospectively admitted copy
for each source occurrence; no implicit decoded cache/free duplicate move. Use
maximum on STORED bytes (including FSV2), not user size or a whole-I/O cap. Empty
at cap0 remains a value. Preserve original HistoryValues zip/error/sticky-field
semantics and keep all three original arrays[] until the complete route passes.

Portable format arithmetic/wire lengths are explicit checked integers and stable
byte order, with no path/host pointer/usize persisted as data. Linux/macOS native
filesystem/locking/cache/reopen/sync/fault/snapshot evidence must be recorded
independently; macOS source/tests do not prove Linux durability. Use existing real
state_store files/fault backend and production prepare/apply for atomic abort/
uncertain reopen, repeated same-ID histories, replacement/carry refs, exact
ordered reads/omission, restart/replay, snapshot receive/install/restore and
independent retention. No benchmark/engine-allocation/CPU/RSS/speed claim follows.

PERF direct current consultation independently agrees on provenance-first, source
index holes, same-entry digest binding, two-pass continuous work, one admitted
spine and new coexistence term; its previous12323/77291c18 written proposal was
actually read. This is the finite NEXT recommendation, not an interruption or
hold on current REF R0/codec. Full real catalog/hydration/source setup/profiles,
M2/M3 gates, Linux+macOS, public initialR/E-T/backing/same-ref settlement and old
native8 remain open. Root alone owns any later creation/release. Finite STOP.

## VALUE-DURABLE successor: closed change matrix and activation dependencies

Finite stable-source inventory ONLY. No new file/model/type/helper/API/constant,
layout/ADR/policy/source/test or workload was created/released. The five live REF
BODY files were not read. REF's real R0/save and its current writer are independent
of this preparation. Everything below describes a REQUIRED successor, not an
implemented catalog or a new completion target.

### Explicit correction of the prior producer proposal

The earlier NEXT22049/cdf324 text's Claim/Extend carry example is too broad for
its selected producers. Actual V5/V6 intent/operation profiles do NOT admit
Claim or Reclaim; generic DecodedRequest zero-slot branches do not override that.
StepContinue is unsupported there too. Keep UnsupportedProfile for these in
supplied_value_fields/value_effect and the eight proposed tests. ExtendLease is
the genuine zero-mask real-history example. StartAndClaim is supported only by
its selected atomic V7. This supersedes that example and any implication that a
new source helper broadens the admitted mutation profile.

Claim remains REQUIRED through its DIFFERENT real path: ports/log/claim/apply.rs
ClaimApplyPlan59 contains control/decision, decide_fresh_claim_phase_observed183
and retain238..304 drop decoded donors before returning the plan. state/claim.rs
GroupClaimChanges validates its complete response/guard; redb/commit/claim138/
3c199c28 validates scope/control/actual receipt and uses write_records plus one
Immediate consumed commit. Its claimed history must carry current references;
it is not implemented by inserting Claim into a V5/V6 test. No source change or
profile permission follows from this correction.

The direct PERF consultation otherwise agrees on same-plan borrowed observation,
index holes and admission. Its actual stable producer arithmetic gives the old
early bound 4E+2B+1216n+2304+A. New requested numeric-owner term M must be added
AFTER real accepted semantic/count validation and BEFORE its reserve, preserving
existing earlier refusal order. V5 late E+B+H+531+A and V6/V1/V7 late
E+B+H+512+S+A each add the SAME retained M. Define M from actual type sizes:
size_of::<Option<RecordBatchValueEffects>>() +
count*size_of::<RecordBatchValueEffect>(), checked. Option already contains its
Vec-owner representation: do not add another whole Vec metadata term and call
it an exact physical size. Allocator overprovision/native frames remain outside
this requested logical formula. The proposed work envelope covers reached
source partition/ID/supplied byte comparisons in two passes (sum<=E), not scans of
old omitted payloads or whole history IDs. Inspection has its own per-call work,
not free credit from the builder. Current GroupApplyChanges keeps the full plan
through consumed commit; into_parts drops effects only during later reply handoff.
These are proposal refinements, not a next-R0/body release or executed tests.

### Actual identities and why one layout transition is inseparable

The current source has FWPK version1 families0 control,1 record,2 outcome,
3 due,4 FIFO,5 history,6 policy,7 placement. key/physical106/fd179712 rejects
anything else; key::header/start validates version/namespace and canonical
component encoding. ordered::encode_history_key179 binds numeric ID position and
version, preserving the source event ID. New value keys cannot be an unscoped
concatenation or an unchecked extra table.

Current record/value code is FWSR1 and FWSH1, not a reference-enabled frame.
record::encode_fields82 serializes core inline bytes; record/scan::scan246 has
namespace/point validation, complete EOF and512+all copied-variable-owner charge.
History encode/decode preserves event/version/retry metadata, but no value refs.
value::start94 closes version1. These legacy entry points must remain exact,
including old bytes, errors and caps; adding reference tails to version1 would
silently break both readers and snapshot/recovery identities.

Current state manifests are FWSS1..6, with production creation still FWSS3.
recovery::check_manifest176 sends5/6 to core_policy/catalog::parse5, otherwise
accepts only1..4. Layout6 has ORIGINAL-policy PROFILE1 and its separate catalog
hash domain (snapshot/original.rs3..6). FWSS5 has the current-policy domain.
This is an existing profile registry, not unused space for arbitrary blob flags.
Snapshot format currently maps FWSS6 to FWSP3 and FWSS5 to FWSP2; receive closes
FWSP1..3 and explicitly maps3->6,2->5,1->3. Descriptor FWSD1/2 and dual-slot FWSD3
have different semantics. No proposed family8/FWSV1/FWSR2/FWSH2/FWSS7/FWSP4 is
adopted by this note: those prior candidate numbers require one exact registry
contract before implementation, never a range-only edit.

### Change matrix: paths/functions and invariants that must land together

All paths below are relative to crates/wf-storage/src unless prefixed ports.
They are a concrete dependency inventory, not a broad creation batch.

| Responsibility | Existing homes/functions actually inspected | Required joint change / acceptance invariant |
| --- | --- | --- |
| Semantic ownership/bindings | ports/state/transaction.rs StoredWorkflowRecord, GroupApplyChanges::new/business/independent_business; ports/state/claim.rs GroupClaimChanges; transaction/load.rs and observations.rs; claim load/observations | Carry explicit binding knowledge from the authentic SAME revision into current and every history step. None current record is not an observed empty ref set. Claim retains refs in its own route. No fabricated absence or detached provenance owner grants commit authority. |
| Physical identity | state/key.rs key_start/header/encode_components; key/point.rs; key/ordered.rs history keys; key/physical.rs decode_state_key | Add one closed scoped value-family grammar, checked component/width/owned bounds, exact source ref/key matching and rejection under older layouts. Never use a side table that inventory ignores. New generated keys derive real group/recovery/tenant and actual partition/ref; no reverse hash-tag-to-partition guess. |
| Stored value/binding frame | state/value.rs start/error/profile exports; value/record.rs encode_record/encode_record_fields; record/scan.rs scan/into_owned; value/history.rs encode_history/validate_history/decode_history | Explicit new profiled envelopes for stored bytes+independent expiry and current/history bindings. Preserve old FWSR1/FWSH1 APIs as LegacyUnavailable, not KnownEmpty. Borrowed scans validate before materialization; owner charge includes all new ref bytes/metadata. No live ETF atom/export table is required just to reopen a stored envelope. |
| Exact consumed publication | state/redb/commit.rs publish/write_records/validate_versions; commit/independent.rs apply; publication.rs write_record/write_history/put | Join supplied kinds with ALL successful source indices and resulting versions, preserving equal replacement, Some(empty) and intermediate same-ID writes. Insert immutable value bytes once per new key, update final current refs and EACH history ref snapshot in the SAME write transaction as index/outcome/control. No new value on replay/refusal/no-op; check collisions, do not silently overwrite a version key. |
| Claim and other writers | redb/commit/claim.rs publish/validate; ports/log/claim/apply.rs; storage/transaction/claim.rs and its loaded observations; active timeout/terminal routes when adopted | Carry reference sets for each real claim history. Any value-enabled writer lacking full binding/effect/retention capability must fail closed before writes, not keep using the old write_history encoder. Whole-family support cannot be inferred from the four mutation producers. |
| Authentic ordered read | redb/read/point.rs read_record pattern; state/redb/read.rs; state/redb/store.rs transaction; proposed real blob read capability | One stable scoped revision/control watermark, one slot per requested key, duplicates retained, correct omission/missing/expiry/storage-failure distinctions. Borrowed row envelope inspection before copies; omission on stored length without decoding. Actual cross-group placement/strong barrier remains service responsibility, never a caller tag or fake nil callback. |
| Live reopen/semantic relations | redb/recovery.rs validate/cohort/check_manifest/validate_rows/inventory; recovery/rows.rs validate/record/history; recovery/index.rs; store.rs open_database | Recognize selected value feature/profile; enforce frame-family-layout compatibility and validate local generated identities/namespace/length/expiry plus coherent current/history versions. Reject unsupported profile or missing mutation capability. Do not require every reference to remain locally resolvable forever; legitimate expiry/missing/external targets differ from corrupt row framing. |
| Birth/current metadata and policy changes | redb/bootstrap.rs manifest/hash_row; redb/core_policy/catalog.rs read/parse5/bind_origin/verify/census/publish/publish_original/publish_target; commit/core_policy.rs and commit/original_policy.rs | Preserve immutable birth origin and current value feature across later policy installs. Original-policy publish_original currently targets6; extending parsing without changing publication would risk downgrading/losing a new value profile. Keep domain/version/profile checks explicit; no header rewrite can invent missing history. |
| Snapshot emission/copy/cohort | redb/snapshot.rs copy/check_descriptor/select/install wrapper; snapshot/format.rs sizes/header/entries/manifest/facts; snapshot/capture.rs capture/advance | Include value rows and bindings in actual frozen rows/hash/counts. Preserve old immutable snapshot IDs. copy currently exempts manifests3/5/6 from normalization and converts others to3: this must become feature-aware, not accidentally normalize a new value layout away. Retained/pinned alternate slots and pending transitions remain protected. |
| Snapshot receive and fresh restore | snapshot/receive.rs header/row/receive; stage.rs receive_database/receive/observe/into_store | Version-to-manifest/profile mapping, metadata order/count, all rows/full EOF/checksum, postreceive semantic validation and target admission change together. receive currently returns exactly layout6/5/3 from wire3/2/1. No stage is served merely because a new wire version parsed. Real file/directory durability and failed unserved-stage custody remain. |
| Snapshot install/transition/upgrade | snapshot/install.rs install/advance; read.rs current/pin/by_id; transition.rs mode/supports_snapshot_transitions/prepare_received_snapshot; upgrade.rs upgrade; validate.rs complete/retained/compatible/values; catalog.rs retained/compatible/require; policy.rs origin/upgrade_catalog; catalog/format.rs | Every closed3/4/5/6 layout predicate and origin lookup must account for the new profile coherently. No downgrade to an old layout, no dropping value rows from either current/anchor cohort, no log-purge release before received install/finish proves actual position. Upgrade cannot backfill discarded refs by relabeling. |

Potential new responsibility homes remain CANDIDATES, not created artifacts:
ports/blob/catalog.rs and blob/read.rs; storage/state/key/value.rs;
storage/state/value/blob.rs; storage/state/redb/values.rs. Group related helpers
under those responsibilities only after the full API/frame/helper consultation.
No independent seed/admin API or second database is a permitted shortcut.

### Two subtle registry/hash obligations found in the actual source

1. CurrentPolicyManifest's catalog_hash counts/hashes ONLY policy and placement
families6/7. Both core_policy/catalog::census176..220 and recovery::validate_rows
225..273 prove that; new workflow data is NOT already covered by that digest.
Do not blindly include mutable value rows in the policy hash without selecting
and funding atomic maintenance on EVERY mutation. The smallest coherent design
preserves the existing policy/placement commitment and explicitly versions the
value capability/profile; all value rows still participate in full snapshot facts
and semantic recovery. If a separate mutable-value root/count is desired later,
its writer/update/replay algorithm must be concretely selected, not implied free.

2. inventory143 accepts ONLY known tables/masks (2/3,4/15,6/63) and no multimap.
Existing snapshot slots reuse the same complete rows schema. Keeping catalog data
inside the reviewed rows family avoids an unaccounted seventh table, but still
requires new semantic family dispatch in EVERY live/frozen row validation pass.
A separate catalog table would require a different complete inventory, atomic
copy/hash/receive/cohort contract; that larger alternative is not selected here.

Actual anchors: core_policy/catalog325/32f49fa0; recovery273/e3ae23fc;
snapshot.rs332/0ecf2189; format324/dbe195f8; capture91/0e7bdfe0;
receive308/5b8ca83e; stage134/13d6de3f; install176/f7c2e7f3;
validate248/2108242c; catalog229/ba9ef837; catalog/format222/ec99c596;
policy75/784de460; read166/f39552a5; transition202/fd4f49e3;
upgrade54/e4576c46. Hashes bind actual files, not a claim every unrelated line
was re-audited: this review read the matrix's named functions/dispatch ranges.

### Knowledge, identity, expiry and payload are different relations

Known bindings mean their actual byte values were captured under the appropriate
profile. They are not an availability certificate. Known with no refs is different
from LegacyUnavailable; absent data, an expired value, an unsupported legacy
binding profile and a corrupt envelope must not collapse into the same state.
Legacy FWSR1/FWSH1 cannot be silently read as KnownEmpty. Current inline bytes
cannot reconstruct dropped older history values. Real replay/backfill from retained
trusted entries or explicit unsupported upgrade is required; origin3 by itself
is not that replay evidence. Preserve any older read entry point without awarding
new hydrated observations. No HistoryValues mapping from a row-codec-only pass.

A local generated row must match its source kind/id/version, actual namespace and
prepared partition. But an old unexpired value may legitimately outlive record
or history deletion, and a binding may legitimately reference an expired/missing
value. Do not impose a universal live foreign-key/cascade-delete constraint.
Shared/named/external source references require their own ownership/link/routing
rules; do not reject or reinterpret them as same-record generated references just
to simplify recovery. Field bindings and catalog bytes need separately reviewable
provenance. Storage errors remain distinct from later invalid-flow payload errors.

Catalog content is SOURCE-NEUTRAL STORED ENCODED BYTES with its own expiry, not
only a Rust Binary node. Existing command Vecs remain logical binaries and use
one exact FSV2 escape. A later typed/stored-input command profile must explicitly
version its representation/intent/validation/encoding, never inspect a prefix
and silently change existing bytes. Malformed typed FSV2 in a valid stored row
can be a later hydration invalid-flow field; it is not necessarily database
corruption. Reopen cannot depend on a currently available BEAM atom/export table.
Omission on stored length can avoid decode entirely. No foreign execution engine
or implicit decoded-document cache is selected.

Actual partition provenance is now closed for the reduced public path: I read
pinned mutation_attrs1195..1201 (nil AND :global ->nil, empty binary rejected),
create normalization124..136 (nil resolved to Keys.auto_partition_key(id)),
many_item_partition_key896..899 and flow_create246..282 (stores actual binary
partition and uses it for generated ref). Rust public_mutation/codec82..125
computes the same IEEE CRC bucket for missing caller partition and writes the
RESOLVED binary partition. Therefore new durable generated refs render with
Explicit(actual prepared partition bytes). Do NOT choose Automatic or direct
Keys Global from the caller's now-normalized option, rehash an opaque partition
as an ID, or reverse the key hash tag into a partition. Direct raw Keys Global /
Explicit(empty) oracle semantics are not public create option semantics.
Placement/group/recovery/tenant proof still comes from the authentic transaction,
not the ability to render syntactically valid bytes or a matching CRC bucket.

Terminal retention remains a concrete PREDECESSOR, not guessed expiry metadata:
current reduced Terminal/Cancel lack captured retention; active TimeoutValueWrite
has it only for its existing active trace. Source terminal start is
min(max(now_ms,apply_now_ms()),MAX_EXACT), then captured positive TTL and clamp;
no capture/nonpositive means no terminal expiry. New terminal values use that
actual witness. Old references keep their original expiry (terminal refresh is
explicitly a no-op), and old generated payload survives terminal record removal
in FlowValuePayloadTest182..219. Shared/named TTL, history trimming, orphan cleanup
and retained snapshot ownership must respect independent value lifetimes. Do not
invent an applied-time/TTL witness from outcome TTL or a changed current policy.

### One cohesive activation order and genuine test seams

Recommended VALUE-DURABLE activation is one closed feature/profile transition,
not publication enabled first and recovery/snapshots patched afterward:
(1) Finish producer-provenance with exact selected mutation profiles; separately
join the real Claim carry path and captured lifecycle/terminal retention model.
Resolve the true typed/shared/external source input/routing prerequisites without
pretending the nonterminal binary slice completes them.
(2) Freeze canonical physical key/frame/profile registry and explicit Known versus
LegacyUnavailable contract. Author callable models/ops/tests under named custody,
execute genuine RED/save, then implement real writer+read and joint recovery /
manifest/origin/snapshot dispatch. No seeded side route or inspector-only endpoint.
(3) Enable the value profile only when accepted operations atomically publish new
rows plus final current and EVERY intermediate history binding, together with
unchanged index/outcome/control; rejection/replay/no-op create nothing. Legacy
plans/capabilities and unknown bindings fail closed BEFORE durable writes.
(4) Exercise real ordered reads, restart/replay and snapshot capture/current/by-id,
pin/alternate/pending/received install/finish/fresh restore. Then real application
barrier/routing and the completed common typed/shared/terminal lifecycle route
satisfy the original full-value/history setup assertions and only then map them.

A private earlier nonterminal binary subprofile can be a predecessor, NEVER the
full endpoint or a reason to drop terminal/shared/typed/routed success. The full
three HistoryValues arrays remain[] until complete hydration/calldata/store
behavior passes. Public mget/history service still needs M2/M3 applied-position /
strong-read and real placement/routing; a locally coherent redb revision is not
such a barrier. Do not manufacture success through a storage callback.

Reuse actual state_store fixture/files/FileBackend and delegated faults backend,
prepared production apply calls and consumed guard. Required meaningful tests:
- supplied equal/empty versus omitted values; Cancel/Complete/Fail/Retry kind
  provenance; same-ID successful/error/no-op holes and ALL history versions;
- complete current/history/canonical catalog envelope with independent size/ref
  checks, actual ordered duplicate/missing/expired/omitted/error slots;
- known abort unchanged predecessor; uncertain commit fence and reopen/replay;
  no partial value/core/history/outcome/control publication or early success;
- pinned/alternate/received snapshots, old formats and immutable IDs, source and
  target profile refusals, checksummed but semantically wrong new rows/bindings;
- new value profile followed by complete AND original-policy mutation, proving
  the value capability survives catalog::publish/publish_original without downgrade;
- legacy upgrade refusal or real proven replay, unavailable historical refs kept
  distinct, orphan old payload retained after terminal record removal, new terminal
  value's own captured deadline and typed/shared/routed successful hydration.

Actual regression seams include state_store/claim/replay.rs retained replay and
strict reopen; core_policy/snapshots.rs policy_layout5_origin_and_current_census_
match_independent_wires / policy_activation_preserves_frozen3_ids_and_transitions_
to_snapshot2; original_policy/snapshots.rs mixed_birth_current_and_frozen_formats_
capture_receive_install and its mixed lineage tests. wf-node snapshot_recovery /
recovery suites supply the later production log-purge/install/replay route. Keep
all old identities/byte goldens, not just a new test that changes version constants.

Formats use checked fixed-width integers/stable byte order, with no host usize /
pointer/path persisted. Real Linux and macOS file/locking/fsync/reopen/fault/snapshot
results are separate required evidence; source portability is not an execution
claim. No engine-allocation/latency/RSS/speed guarantee or new benchmark follows.

Still unresolved for the LATER exact creation batch: complete captured terminal
lifecycle representation and its immutable source/application-time witness;
which actual retained-log range can establish legacy history (otherwise refuse);
exact new frame/layout/profile and digest registry; and complete typed/shared /
external input + cross-group read authority. These are concrete interface facts,
not user questions, a broad approval or an ACK gate on the current writer. Root
can select their exact staged contracts while retaining the full required endpoint.
Finite read-only inventory STOP; only this architecture append changed.

### NEXT provenance always-resident field precision (no creation release)

PERF's direct final accounting correction is adopted prospectively, superseding
any wording in22049/cdf324 that conflated the public effect-output cap with the
new plan field or treated None as free metadata:
R = checked(count*size_of::<RecordBatchValueEffect>()).
F = size_of::<Option<RecordBatchValueEffects>>().
The inspector's maximum_effect_bytes caps R, not F. Actual None has R0 and no
Effect Vec allocation; max_effect_bytes0 can therefore observe Ok(None) with
sufficient separate work. Every returned plan physically includes F even when
None. Count F ONCE (Option already contains the Vec-owner representation), add
F+R to Fresh early and late coexistence, and add F to Complete/replay/conflict/
expired/LogReplay construction. No old512/531/192n allowance or assumed zero-size
None silently owns it. This is a requested logical ownership profile, not an
allocator capacity or whole-frame proof.

Actual stable phase.rs95..118 finish_inspection4 returns plain Inspection and
constructs Complete after dropping decoded. Exactly five current callers wrap
it in Ok: phase inspect_record_batch72 / inspect_record_batch_point92,
public_mutation/apply60, atomic_start58, independent68. Select the narrow future
BODY refinement: finish_inspection retains its name/arity but returns
Result<RecordBatchInspection,RecordBatchApplyError>; after the retained gate and
BEFORE Complete construction it prospectively admits checked decoder_bound+F,
then drops decoded and returns Ok(Complete). Fresh remains the existing boxed
handoff, with F+R paid at its later actual builder/plan creation. Propagate the
Result directly through those SAME five callers. No second gate, new public API,
helper/model/file or earlier gate-error reordering is selected. All four Fresh
producer formulas and Complete constructors must be source-reviewed together.

This corrects an actual proposed-accounting omission before implementation. It
does not execute tests, assert unchanged refusal boundaries or release the next
increment; Root must include the real new workspace term in its named BODY batch
and preserved tight-bound tests. Current moving REF BODY remains untouched.

## REF-BINARY1 BODY actual source and first executed target review

Finite independent read-only review of ALL five stopped implementation files,
ALL three frozen test files, exact selected19-function/4-constant/API/cap4 contract,
current pure-module controls and pinned Keys/Support clauses. No source, test,
shared document, Git/compiler/VM/workload/probe or artifact changed here; only
this allowed architecture note append. R0 save39f1b3d precedes BODY release per
Root custody. Real catalog/storage/hydration/expiry/routing remains outside this
pure prerequisite and all original three HistoryValues arrays remain[].

### Actual complete source/model/test binding

Current complete files actually read (LF/bytes/SHA256):
blob.rs74/2739/3e742999d88b4c15038b4f9ca9e5546d348d9bb4f85052f89817611c4be931f4;
encoding.rs46/1279/57df5621e61a92acc424857839152389bcf33f9d33377abca2921988fe7ea599;
reference/base64.rs76/2411/370e3db486b97f0edb87a3880ed04f7fd4df681aa619b229a0825f8b055662ef;
reference/tag.rs95/2864/ad068b278e53352fb16744a38eeb5d108b8e19c7aae85d3079f6453bb337f0b8.
I first whole-read reference.rs242/7827/
54f3cfb80118ea4a6cf60d5bb7c528c4ddc15694f512c96b70e12ab010464949,
then separately whole-read Root's final stopped marker revision245/7881/
4d6911a08d4c798ed871c80afa13690dc2157e75420e7be273cee45f5d98b608.
My exact WHOLE expression inverse from the final four-line bounded windows
expression to the old one-line rest.windows(4).position expression restores
ENTIRE242/7827/54f3cfb8, not merely a normalized AST or selected line.

The unchanged LAST2514 parent bytes independently hash to
a17016b17f7b7260714eeaec2fa85c9038b5234b180dc0660228173f8d2feb7a.
All four ordinary public models/derive sets, lifetime-bearing encoded named views,
private children and FIVE explicit reexports/signatures match the selected R0.
Only parent introductory docs changed outside that literal tail. No manual
method/new type/extra public module/API/callback was introduced. Root/author's
full three R0 beforeimages and1590/ba9c5bfa hold are attributed custody evidence;
I did NOT independently recreate those full historical source beforeimages or
repeat that whole held-input inventory. The marker expression/full-parent-tail
inverses above ARE independently reproduced from actual bytes.

Frozen target files actually whole-read/hash-checked:
main18/868/f3e071475f004d007a6f9ee6ffad0af15504005e59dadfb84c9c97a008ec5e54;
references304/9909/d22a95a250d2eeacc499e3c4490f82418406fae780b384a889ee1fab95428b70;
encoding111/3972/752e8203686e983b10463098c7dad896ad940c4953a33c5de62b3171a2479ee4.
Exactly eight named tests/no helper. Physical test spans91/36/89/44/33 and32/35/37;
maximum91. Complete production roster is exactly19 functions and four constants:
reference three APIs+split_tag/parse_version/decimal_length/reserve_key/put/
put_version; encoding two APIs; base64 encoded_len/valid/encode_into/digit;
tag valid_tag/render_tag4/crc_bucket/auto_bucket. Largest current function is
render_tag57LF; largest production file245. No size waiver/packed-code remedy.

### Source grammar, limits and safety

Pinned Keys937/32363/bf9cbe380b9a59ef275a7efe9f94b8ee86ff10d2a7c4162eeb3a6dc24c6d8349
actual1..115/713..937 and Support680/25028/
67ea918f9582cc190f24df6424e54158ced9b965ff54d294a1bf5d106efd48b6
125..180 were reread. Binary-only acquisition remains explicit; no coerced Raw
or nonbinary FunctionClauseError parity is invented. Eligible constructors are
NOT called parity for the raw Keys constructors' negative/oversized integer or
empty-ID renderings that fail the predicate.

Parser requires the first f:{ prefix and FIRST }:v: marker, validates the exact
inner tag and retains the COMPLETE original suffix. Regular p/r/e/s uses the
LAST colon, nonempty opaque ID and canonical decimal0..9007199254740991; embedded
colons/NUL/nonUTF8/marker-looking ID bytes remain data. Named n has exactly three
fields, both nonempty canonical unpadded URL-B64 spans and the same version.
The returned spans borrow original encoded components, not decoded owner/name.
There is NO parser65535 or public ValueStore4096 cap. Valid65536-ID keys remain
recognizable even though actual eligible construction refuses its output size.
No UTF8/trim/string allocation or recursive parse was introduced.

Base64 recognizes the exact alphabet, impossible remainder1 and canonical tail
bits (low4 for remainder2, low2 for remainder3). Empty validity is used safely:
named caller rejects empty components, digest caller additionally requires43.
Canonical digest consequently decodes32 without a temporary decoded Vec.
Version parse handles exact zero/leading-zero/sign/range rules with checked
ASCII accumulation, independent of fixed-width clamp guesses. Every candidate
slice/cursor/size uses checked arithmetic/get; the only direct computed digit
bytes are bounded0..9. No unsafe/set_len or unchecked output growth exists.

Both renderers reject semantic empty components/version>MAX BEFORE any cap,
including max0. Then checked fixed/encoded/version/separator lengths and
min(maximum,65535) produce the remaining tag allowance. Global tag5, auto8/9/10
and SHA tag49 are counted INCLUDING f:. Known SHA49 bound is checked BEFORE
hashing; Automatic checks minimum8 BEFORE CRC then its EXACT digits before
writing, so the literal25 success/24 refusal cannot be replaced by a fictitious
worstcase10-byte tag. Valid explicit automatic spelling yields its bucket;
malformed auto-looking bytes hash WHOLE. Explicit(empty) hashes the empty binary
and remains distinct from Global/Automatic; public option rejection is elsewhere.
CRC's reflected IEEE polynomial/pre/post complement and explicit low-byte result
are portable and match source crc32%256, not CRC32C or host-endian reinterpretation.
Named Automatic hashes/CRCs owner bytes, not the already Base64-encoded spelling.

Fixed formulas are exact: regular suffix adds ID+decimal+6; named adds encoded
owner+encoded name+decimal+7. One EMPTY Vec try_reserve_exact(final length) follows
all semantic/size/tag checks. resize zero-initializes that buffer (a real pass),
then checked put/decimal/B64 writers fill it directly; final cursor must equal
measured length. Name encoding never creates temporary encoded-owner/name Vecs;
SHA digest and tag49/decimal20 scratch are fixed local values. Original reserve
cause propagates as ReserveFailed. No claim capacity==requested or exercised OOM.
Explicit partition SHA work remains input-length-dependent even if output fits;
a65K output cap grants no ingress/backing/work/admission authority.

Binary framing exactly matches Support125/127: if and ONLY if starts FSV2,
length checked-adds5 and parts returns [static FSV2+1,ENTIRE unchanged input].
Otherwise [empty,input]. Unknown/mode2/compressed-looking bytes are still logical
binaries, never implicit typed ETF or double encoded. Empty/max0 succeeds; binary
stored length has NO Router65535 limit. Both public functions share the exact
measured bound; parts allocates/copies nothing. Borrowing may retain a much larger
caller backing; root/input ownership, native accounting and backing release are
not proven. The fixture concat is test-owned, not a production escaped buffer.

### Exact selected marker-search improvement, now source-bound

Both reviewers independently endorsed this same-function/no-new-artifact change.
Valid inner tags are f(length1), fa:canonical0..255(length4..6) or f:digest43
(length45). Hence the LAST possible valid first marker begins at rest offset45
and its four bytes end at49. Searching rest.get(..min(rest.len(),49)).windows(4)
contains EVERY valid marker (at most46 candidate windows). A first marker beyond
that limit already has an invalid inner tag under the old implementation; both
versions refuse it. Full input still supplies prefix/suffix extraction and later
opaque ID/version scanning. This is NOT an input/key-size cap, rescan-to-later-
marker behavior or a suffix truncation. Current hashed49 and valid65536-ID tests
are the relevant nonvacuous controls. Root applied it ONLY after the first target
terminal and formatter STOP; the final whole-source hash/inverse is recorded
above. This proves removal of a needless malformed-tag prefix scan, not elapsed
speed, whole parsing complexity, allocation gain or a benchmark result.

### Frozen oracles, actual first execution and bounded acceptance

All15 reference/20 predicate/nine binary source-derived literals, separate size
controls, CRC38/owner241, source Global/Automatic/SHA fallbacks, MAX versions,
25-vs24 auto cap, both65535/65536 shape positives and InvalidInput-at-both65535/0
priority checks remain. Tests assert Some/Ok and exact nonempty content/kind/length
BEFORE pointer checks. Parser dynamic Vec backings and binary owned128-byte array
subranges survive temporary descriptor scopes; returned spans identify exact
original ranges and untouched input regions. Test-only buffers/concat have no
production allocation claim. No test weakened or source suite mapping added.

I actually whole/hash-read published DOMAIN7943+1638/
f60f34030cdd50fee6c7ec7593c6f216d721b2341470921dbac66bc8f6e41dd0,
and PERF R0 source94092+7405/56f31bd3 plus101497+3334/5ed24a9e.
Those bind all exact final frozen hashes and genuine technical-vs-behavioral
separation. PERF's direct stopped BODY findings independently agree on grammar,
one reserve, cap-before-hash, direct B64, fixed scratch and marker proof; its
forthcoming written BODY receipt remains ordinary next read, no ACK gate.

I independently located/read the complete actual R0 target in the saved Root log
/private/tmp/ferricstore2-m2-source-pin-validation.log at8973812+4138/SHA256
46d8d0a06f46a8fe954b748aee01dd2f770458efbe5ab27cbfe77f373dbe3ce7.
All eight names execute: exactly1PASS7assertionFAIL,0ignored/measured/filtered;
no missing symbol/compile/classification diagnostic substitutes for that RED.
Only reject_noncanonical is alreadyOK; positive/numerical/borrow tails were masked.
I also whole/hash-read PRE-MARKER BODY target20195 at8978174+1090/SHA256
8a97cf534b0ffbcf2f1d9852298e8d8954fbbe39d2ce03dc5b3653b79254026f:
8PASS/0failed/ignored/measured/filtered. My independent exact(name,status) Counters
match all eight frozen source identities: ONLY the sevenFAILED statuses becomeOK,
and the same original negative control staysOK, with no added/lost names. This
first target executes all formerly masked golden/boundary/pointer tails, but it
belongs to reference54f3cfb8, NOT a fabricated final4d6911a0 validation result.

No scoped semantic/API/purity/ownership/safety/geometry defect found. ACCEPT the
actual complete final source and unchanged oracle contract. Pure controls are
still purity45/a83dcbc6 and guard38/12bba58a; policy128/b1eb1ec2 and Cargo19/bf2b3422
remain exact existing dependencies and server boundaries. Root's final6046 eight-
step run is LIVE at this receipt; no result/full-quality/full-workspace status is
inferred for the revised source. Its actual packet and peer's written findings
can close ordinary follow-up without another source-approval cycle. This review
ran no test/build/VM. Full catalog/current/every-history/read/recovery/snapshot,
captured terminal/shared/typed/routed values, original3 mappings/setup/profiles,
Linux/native8/publicinitialR and the complete goal remain required. Finite STOP.

### REF-BINARY1 final B64 technical spelling and written-peer closure

After the preceding source receipt, Root6046 reached strict Clippy and stopped
101 BEFORE its target/remaining checks, after fmt/check passed. I whole/hash-read
actual raw8980047+1265/30731894eacb33fad9f646667a0951c850c767721e7c6d97532758a36e14dfcb:
ONLY two verbose_bit_mask diagnoses at the B64 low4/low2-bit checks, with no
behavioral failure or lint suppression. This is technical feedback, not another
TDD baseline and not a final eight-command result.

Root applied ONLY the prescribed two expressions after terminal and stopped.
I whole-read FINAL base64.rs76LF/2436B/SHA256
caa419b3d457ccc8d7eedf1ab1e5e7b065429cf2f4c8889bbbeff17db09fe46b.
My exact in-memory replacements trailing_zeros()>=4 -> value&15==0 and
trailing_zeros()>=2 -> value&3==0 restore the ENTIRE prior76/2411/
370e3db486b97f0edb87a3880ed04f7fd4df681aa619b229a0825f8b055662ef.
For u8, at least k trailing zero bits is equivalent to zero low-k mask; zero
has eight trailing zeros, so it remains valid for both k4/k2. This holds beyond
the actual decoded alphabet range0..63 too. No grammar, tail-padding rule,
function/model/constant, reserve/copy/work behavior or fixture is changed.
Reference stays245/4d6911a0; all other source/model/test bindings above stay exact.
The final source roster remains19 functions/four constants/max245file/max57fn.
No scoped defect arises from this equivalent lint spelling.

I also ACTUALLY whole/hash-read PERF117915+9987/
314b89c0032cd581a8753ec36046f4e8b97397e842fb4d757be3b7b1b4bd5cc3.
Its complete source/model-tail/frozen-test/cap/reserve/marker/source-profile
findings agree. It explicitly attributes the R0 Counter at that time; my actual
whole4138 R0 packet and exact eight-name comparison above are my independent
additional evidence, not falsely attributed to that peer. No substantive dispute
or further source gate remains. Root's final78238 sequence is still reported live;
its actual outcome/full-quality/full-workspace evidence is unclaimed here and can
follow ordinarily. No reviewer source/workload ran. Finite review STOP.

## REF-BINARY1 actual final focused acceptance; NEXT checked-index choice

I whole/hash-read FINAL saved Root packet
/private/tmp/ferricstore2-m2-source-pin-validation.log8981337+9479/SHA256
e07e620e465420b6b3e565dff59bc8d0e8d7f240d8b75d960935de0c65a12d15
and its ENTIRE target8982511+1089/SHA256
bf07720fdad1a63c9eb718c8568d805a72e852c2169d0ebd139a9f1303165538.
This resolves the pending78238 qualification in my preceding receipts; it is
not the predelta1090 target or the technical6046 Clippy refusal.

My independent parse yields exactly8 command headers and8 terminal footers,
ALL EXIT0/UNCHANGEDTrue, ALL under the SAME1595/SHA256
84442280c6e77b8347518221348127ed6e4904d18eff4800ea262631158e656a input.
Actual order: fmt, ports all-target check, strict ports Clippy, target8, arch53,
reference480, progress120/132/550, diff. Architecture is45unit+4archunit+3visibility
+1workspace; the actual blob purity guard passes. The reference/progress output
explicitly retains incomplete runtime reconciliation and full parity; neither
is a certificate for the original three HistoryValues declarations.

I independently compared the exact STATIC eight test names with actual R0
8973812+4138/46d8d0a0 and this final1089 target, including name multiplicity.
No name added/removed: ONLY sevenFAILED statuses becomeOK. The same original
reject_noncanonical negative case remainsOK. FINAL8PASS/0failed/ignored/measured/
filtered. Therefore the previously masked golden/cap/priority/canonical-B64 and
nonempty lifetime/pointer/input-unchanged tails now finish on the FINAL source,
not merely its earlier implementation. No technical check is counted as runtime
RED, no ignored/filtered case supplies behavioral acceptance and no fake OOM /
allocator/performance assertion is inferred.

All FIVE actual production hashes and THREE frozen targets were freshly rebound:
reference245/4d6911a0, B6476/caa419b3, blob74/3e742999, encoding46/57df5621,
tag95/ad068b27 and mainf3e07147/referencesd22a95a2/encoding752e8203 are EXACT
those already whole-read/accepted. No new source delta or source issue exists.
Whole source/marker/B64/tail/19-function/four-constant/safety/resource analysis in
11322/09b6495f plus2122/b8cd468d therefore applies to this actual final result.

Ordinary reciprocal written reads are actually closed: PERF127902+1477/
17df9dcaeec9c9a200d2a06c311df9024ca6b1377eb2439d9203c1a7cfc84c6d and
129379+3452/5d7f66c4bc797cb9bb569b490f55f4d236854cce6a916a2d53fbecd95a066d4d,
end132831, were whole/hash-read. It independently binds finalB64/marker inverse,
actual R0/final exact eight Counter, all8zero and my earlier written source
receipts. We agree; no substantive dispute or further append/ACK loop is needed.
ACCEPT the final scoped REF source/API/behavior and focused quality. Rootfull11
18656 remains separately live/reported pending here; no full conservation/docs/
smoke/Linux/full-profile outcome is inferred. No source/workload/Git ran here.

### Exact future private Effect index refinement BEFORE creation

I also actually whole/hash-read PERF104831+13084/
673c3a6723ea067192787e08cacc537ff6c6ab42fd0c88cf4aaed1bb1e4cddd5.
Its profile correction and explicit F/R accounting agree with my adopted
amendments. Independently read current admission.rs12/46..63: MAX_BATCH_ITEMS
100000, default1000, constructor rejects larger. common::count144..153 reads
u32, checked-converts to usize and enforces the captured admission count;
encode.rs60..63 enforces the same count before checked u32 emission.

CHOOSE the bounded u32 refinement for the UNCREATED NEXT model: change ONLY
RecordBatchValueEffect's PRIVATE item_index field from usize to u32 and its
item_index(&self) getter result to u32. This supersedes those two earlier usize
spellings in the22049 proposal; version stays u64 and all other fields/APIs,
profile/index-hole semantics and proposed artifacts/helpers remain literal.
The actual source index is0..count-1<=99999; singleton V1/V7 is0. Do not compact
or renumber after filtering independent failures/no-ops. Perform ONE checked
u32::try_from(index) per reached effect row PER pass before construction; no
truncating as cast. Later use checked usize::try_from(row.item_index) before a
bounds-checked lookup; conversion exhaustion is an operational Limit, and a
missing/mismatched source row remains InvalidChanges. No new helper/type/alias,
constant, layout probe or code artifact is introduced by this choice.

R remains checked(count*size_of::<ACTUAL RecordBatchValueEffect>()) and F remains
size_of::<Option<ACTUAL RecordBatchValueEffects>>(), explicitly resident even on
None and included as previously selected in Fresh/Complete coexistence. No claim
that this change actually produces24->16 bytes, a free Option niche, allocator
capacity equality or measured speed follows. Ordinary compiled type sizes, not
assumed field ordering/repr(packed), drive the future arithmetic. The same work
census must count the checked conversion; the64n envelope is conditional on the
actual future BODY source, not proof by naming. This is ONLY an exact prospective
contract refinement; no NEXT R0, source creation or durable activation release.

Finite review STOP. Full real catalog/current+every-history/ordered routed read,
source-neutral typed values, captured terminal/shared retention/recovery/snapshots,
original3/setup/profiles, Linux+macOS and native8/initialR/E-T remain required.

## NEXT-VALUE-PROVENANCE1 exact test-fixture geometry decision BEFORE creation

Finite stable-source test preparation only. No source/test/helper/model/fixture,
policy/dependency/shared document or workload was created/mutated. Root's current
REF full validation/save remains independent; no NEXT R0 release follows here.
The selected production three models/five accessors/borrowed inspector, checked
u32 source index, F/R and finish_inspection Result refinement remain unchanged.

### Actual reuse audit

I read the relevant actual fixture/helper/producer sections of:
ports tests/record_batch/main.rs582/21514/d8188368b06e9029642b71667e2cc60f4345e1b5fd1c0829df5f75754e878458;
independent_batch/main.rs478/15007/1bbd53aebfff65775ee1b10d681a6478ab7920fb69f47d84a932f26ef2f96b9e;
public_mutation/main.rs212/6550/80a9d661bff310ac77e943ed5efc60bb24b08c6e5ca96dac5bb0b36b17c9903f;
record_batch/atomic_start.rs386/15332/860c785347d32f7134abf7e3f65433acb4a36f58be5ca9fef6b4e50f7312929c.
Also touched/phases helpers, public gate/wire controls, caller input/create field
allowlists and actual source checker path-sharing restrictions were inspected.
These hashes bind files, not a claim every unrelated test body was rereviewed.

Their helpers are PRIVATE to DIFFERENT Cargo integration target crates. There is
no existing public wf-test-support four-profile preparation/view helper to import.
Using #[path] to pull one root would also pull its unrelated tests/modules/goldens
and violate the actual exact-owner/path gate. source.rs289..312 approves only the
three existing storage/node redirections; source_pins.rs9..62 rejects wrong owner,
module/name/path/nonliteral forms. No wf-ports exception exists. Making the old
helpers public would not make a separate integration binary a library dependency.
Therefore DO NOT change those roots, Cargo, dependency graph, source checker or
path allowlist merely for this fixture sharing. Reuse their proven SETUP PATTERN
locally with the real production calls, not the entire files or expected results.

### Exact smallest additional artifact and seam

SELECT ONE NEW test-only sibling:
crates/wf-ports/tests/workflow_value_effects/fixtures.rs.
The existing proposed NEW main.rs adds exactly mod fixtures; beside its unchanged
mod provenance/mod intermediate. Main remains imports/forbid unsafe/registrations,
no hidden test functions or shared-path attribute. There is no public fixture
crate, new fixture directory, alias reexport file or production fixture API.
This expands the proposed five-path R0 write set to SIX by that exact test path;
all four target paths are actually absent at this consultation. Root records any
later release/custody only after its current save, as already required.

Everything below is PRIVATE test scope (pub(super) only where siblings need it).
NO new struct/enum/trait/model/method is selected. Exactly ONE alias:
TestResult<T=()> = Result<T,Box<dyn std::error::Error>>.
Exactly ONE constant WORKSPACE:usize =64*1024*1024. Outcome TTL1000 is a fixture
argument to existing producers, NEVER a value-expiry witness. Profile dispatch
uses the EXISTING u16 intent versions1/5/6/7, not a new fixture enum or a generic
fallback treating other operations as supported.

Exact SIXTEEN helper names/signatures and responsibilities:
1. limits()->CodecLimits: CodecLimits::new(AdmissionLimits::default()).
2. scope()->TestResult<RequestScope>: owned t/g/z namespace via real constructors.
3. clock(now:u64)->TestResult<CommittedTime>: validated HLC(now,0)/wall now.
4. log(index:u64)->LogIdentity: existing fixed group/term1/7 plus explicit position.
5. control()->TestResult<ControlState>: SAME mathematical predecessor pattern as
   existing atomic_start::with_atomic_view: initial clock0, plan_control at log0,
   PreparedDigest([0x31;32]), Unstamped, then next. It is a fixture observation,
   NOT proof of a persisted log or a durable read. Actual NEXT frame is log1.
6. item(id:&[u8],operation:Operation)->TestResult<PreparedBatchItem>: actual owned
   partition p/id, now2, supplied operation, create_priority0. No effect metadata.
7. request(items:Vec<PreparedBatchItem>)->TestResult<PreparedRequest>: moves items
   into Batch, scope t/g/z, request q, clock2 and the existing placeholder intent
   which the REAL selected encoder recomputes. No copied fabricated prepared bytes.
8. batch_frame(items:Vec<PreparedBatchItem>,independent:bool)
   ->TestResult<PreparedBytes>: false calls encode_record_batch(V5), true calls
   encode_independent_record_batch(V6), same actual limits/workspace.
9. public_frame<'a>(operation:&'a str,id:&'a[u8],
   fields:&'a[(&'a str,MutationFieldValue<'a>)])->TestResult<PreparedBytes>:
   actual normalize_core_mutation with empty common fields, explicit p partition,
   captured admission/workspace, then actual encode_public_mutation at clock2/
   q/scope. Return its owned PreparedBytes, discarding only the returned intent
   value if the test does not need it; no typed-item substitute for the V1 route.
10. atomic_frame(payload:Option<&[u8]>)->TestResult<PreparedBytes>: construct ONE
    actual StartAndClaim item i/type r/state q/worker w/positive lease1000 with
    optional payload, then actual encode_atomic_start(V7). No fake running plan.
11. claimed(id:&[u8],payload:Option<&[u8]>)->TestResult<WorkflowRecord>: actual
    domain reduce Create at time1 and Claim at time1, token k/fence1/worker w /
    lease1000. Return the owned resulting record (fixture-only clones allowed),
    never restore an invented accepted postimage or imply Claim is V5/V6-admitted.
12. with_view<'current,T>(records:&'current[WorkflowRecord],ids:&[&[u8]],
    check:impl FnOnce(TouchedIndexedGroupView<'_, 'current>)->TestResult<T>)
    ->TestResult<T>: constructs the SAME genuine mathematical touched facts as
    independent_batch::with_view. Full explicit current observations for all ids,
    Some only for matching supplied records; actual DueOrder/LaneKey derivations,
    known placement p->g, parallel membership from real rows, default r policy/
    retry observations and known empty q-lane head. Invoke check with that view.
    Transient query/maps drop; only externally supplied current record owners can
    remain borrowed by T. This closure is a typed-view lifetime scope, NOT a
    storage callback returning fabricated bytes/success or a fake I/O capability.
13. prepare<'input>(profile:u16,bytes:&'input[u8],workspace:usize)
    ->TestResult<Result<DecodedRequest<'input>,RecordBatchApplyError>>:
    dispatch ONLY1/5/6/7 to actual prepare_public_mutation / prepare_record_batch /
    prepare_independent_record_batch / prepare_atomic_start respectively.
14. inspection<'input,'retained>(profile:u16,bytes:&'input[u8],workspace:usize,
    control:ControlState,original:Option<&'retained RetainedOutcome>,index:u64)
    ->TestResult<Result<RecordBatchInspection<'input,'retained>,RecordBatchApplyError>>:
    use prepare above and actual matching PointGateView inspector with scope,
    exact passed control/original, retained_nonempty=original.is_some(), log(index).
    This retains genuine Complete/replay/error outcomes; it never forces Fresh.
15. decide<'current>(profile:u16,records:&'current[WorkflowRecord],ids:&[&[u8]],
    bytes:&[u8],workspace:usize,additional_live_bytes:usize)
    ->TestResult<Result<RecordBatchApplyPlan<'current>,RecordBatchApplyError>>:
    actual inspection(profile,...,control(),None,index1), REQUIRE actual Fresh,
    scope it through with_view and call the matching existing touched producer:
    decide_fresh_record_batch_touched / independent / public_mutation / atomic_start.
    Pass the requested additional_live_bytes and TTL1000 unchanged. Return exactly
    the production result; no attachment/effect construction/acceptance override.
16. outcome<'plan>(plan:&'plan RecordBatchApplyPlan<'_>)
    ->TestResult<&'plan RetainedOutcome>: borrow the REAL Fresh/IndependentFresh
    outcome box, otherwise a fixture setup error. No fabricated/altered response.

Nested Results in13..15 are INTENTIONAL: outer Box errors describe invalid
fixture/setup/unexpected profile or Complete where Fresh was requested; INNER
RecordBatchApplyError is the untouched SUT result. Tight-workspace tests can
match exact real Workspace/operational errors rather than mistake setup failure
for expected refusal. Unknown profile numbers are OUTER fixture errors, not
hand-constructed SUT UnsupportedProfile successes. Negative crossed-profile
controls use actual prepare calls on mismatched REAL frames. No helper swallows
an error, substitutes None, changes limits or weakens expected result matching.

Fixture ownership is separate from production claims: maps/Vecs, seed-record
clones, normalization donors and Box errors are test-owned. Returned plan borrows
only the caller-live records, not temporary touched maps, ids or scope. Encoded
PreparedBytes are owned by the calling test and outlive decoding. No self-
referential fixture owner or hidden static/leaked result is needed. All helpers
call existing real code; none computes expected effect masks/versions, calls the
new inspector, serializes expected refs, fakes storage, seeds a new catalog or
mints admission/placement authority. Assertions remain in the eight tests.

### Concrete case/geometry assignment, unchanged eight identities

provenance.rs retains these FIVE names:
- value_effects_bind_exact_prepared_entry_and_operation_kinds: explicitly drive
  profiles5/6/1/7 through their actual encoders, gates and producers. V1 Create
  uses real fields type=r,state=q,payload=v (actual create allowlist/default was
  read); V7 uses atomic_frame. Assert actual accepted plan/history BEFORE the
  new inspector and literal row0/version1/payload mask. No detached substitute.
- value_effects_distinguish_omitted_empty_and_equal_replacements: externally held
  claimed donor payload v; real transitions with None/Some(empty)/Some(v), literal
  source masks, new versions and unchanged donors. Never suppress equal bytes.
- value_effects_preserve_atomic_input_indices_and_kinds: actual atomic unique-id
  donor rows; Complete result-only, Fail error-only, Cancel slot0 error, Retry
  with omitted payload plus error. Literal input-index/kind/version expectations
  stay in the test; no shared helper reproduces the production slot algorithm.
- value_effects_emit_carry_rows_for_real_no_value_histories: actual supported
  ExtendLease real write/history with a zero mask; not Claim/Reclaim/StepContinue.
  Whole rejection/replay/no-history controls still need no effects and cap0.
- value_effects_refuse_crossed_profiles_and_do_not_invent_expiry: actual mismatched
  source digest/profile controls, real rejection/Complete classifications and
  terminal boolean only. No alleged TTL/deadline is read from outcome retention.

intermediate.rs retains the other THREE:
- value_effects_preserve_independent_histories_and_source_index_holes: actual V6
  same-id Create success, invalid transition failure, valid replacement transition,
  terminal Cancel and terminal duplicate/no-op. Enumerated original positions
  and every real version remain literal (for example0/2/3), never compressed by
  final_indices/history_plans enumeration. Check the actual ordered result/real
  histories first, then expected inspector rows; no predeclared accepted plan.
- value_effects_admit_continuous_work_and_descriptor_coexistence: request limits
  explicitly at calls. Use nested real errors to check new F/R in Fresh and
  Complete formulas, no-history cap0 with adequate work and refusal on insufficient
  verification work. Compute F/R from actual selected types, no guessed24/16,
  allocator-failure hook or fallback workspace. Preserve source refusal origins.
- value_effects_borrow_only_the_plan_after_decoded_verification_drops: build actual
  plan outside a temporary prepare/verification scope; return new inspector's
  plan-tied borrow and use it after that decoded descriptor drops. Call the
  inspector twice on the SAME plan and require identical nonempty owner/step
  addresses; a second simultaneously live independent plan must have its OWN
  owner. Assert Some/exact rows BEFORE any pointer checks.

R0 tests may use ONLY the new callable borrowed inspector and five selected
accessors plus existing public production APIs. They must NOT call the future
BODY-only RecordBatchApplyPlan::value_effects getter before that method exists:
that would be technical missing-symbol failure rather than behavioral RED.
Same-plan repeated calls plus cross-plan identity/lifetime controls test the
owned attachment contract without expanding R0 to edit all old producers.
BODY source review must also bind the private actual plan field/getter and all
four pre-drop joins, as already selected. There is no silent standalone-only
endpoint. Some/accepted premise checks are never hidden in a fixture success.

One fixtures file is sufficient by the actual source cut: scalar constructors
are3..16 lines; encoder wrappers about10..35; view lifetime scope about50..65;
prepare/inspection/decide each split by these responsibilities naturally below100.
Sixteen bounded helpers plus imports/comments fit one tooling file within600;
this is an implementable source-cut estimate, not a claim a file was authored or
a waiver. The source author must census its ordinary formatted draft before
baseline; any genuinely necessary extra helper/file/model still needs an exact
precreation decision, never code packing, unrelated copied suites or lost cases.
The fixture cut lets the actual test bodies focus on explicit scenarios/assertions
rather than repeat50-line typed view setup in every function. No assertion count,
eight case identity, supported profile or same-owner/real-producer duty is reduced.

Root may record this concrete ONE-file/16-helper/alias/constant amendment for the
future SIX-path R0 release after its current REF save; no release is made here.
Full validation64206 remains independently live/unclaimed in this task. Only this
allowed note changed; no Main/code/helper/workload/Git or policy mutation occurred.
Finite fixture consultation STOP, no extra acknowledgment gate.

Fixture error-lane precision: TestResult's Box<dyn Error> is for EXISTING actual
constructor/producer/setup errors. The selected NEW RecordBatchValueEffectsError
is intentionally Debug-only and must NOT acquire Display/std::error::Error just
to permit question-mark propagation in tests. None of the16 helpers calls the
new inspector. The eight test bodies assert/match its Ok(Some)/Ok(None)/precise
Err variant explicitly (format Debug only for a fixture failure message if
needed), then return borrowed views after successful pattern matching. In
particular the temporary-verification lifetime witness uses a match before
leaving its scope; no hidden error-conversion helper or trait method is added.
This preserves the existing three-model/five-accessor/R0 ABI and keeps a future
technical trait/import failure distinct from the required assertion RED.

### NEXT provenance BODY geometry decision: two private lifecycle helpers only

This is a finite precreation decision for the later producer-joined BODY, not
its release. DOMAIN's current six R0 paths were not read. No source, test,
shared metadata, compiler, VM or Git operation was performed. The selected
three public models, five accessors, borrowed same-plan inspector, checked u32
source index, eight tests and F/R ownership contract remain unchanged.

Actual held-source geometry, independently read in full:
- log/record_batch/phase.rs:254LF/9050B/b6c45c9729ed89dd799781207823e078748c63413c4a4516b3e7da198fcc3611;
  finish_fresh168..254=87LF; finish_inspection95..118=24LF.
- log/record_batch/independent.rs:168LF/6038B/dd7e64c131cdb75e991880ed478c1efc3380d683d4f972403df23e798a587f38;
  decide_fresh_independent_record_batch_touched75..168=94LF.
- log/public_mutation/apply.rs:164LF/5526B/5f1d635d6a4a5ecc7deae6d0ccf2a963045a373110a878d6e2b2c1bf7bf8dab2;
  decide_fresh_public_mutation_touched66..164=99LF.
- log/record_batch/atomic_start.rs:160LF/5403B/8f0472c451ffb25324fc470c78e36d18d591fe2b2cdc703a400ac00550d3765b;
  decide_fresh_atomic_start_touched65..157=93LF.
Paths above are relative to crates/wf-ports/src. The unchanged parent
record_batch.rs is344LF/11971B/686d964e25d185d8bc2fcf2351a481a7e43631a3af09da666238f55bbcad5029;
existing fresh.rs is207LF/6698B/ec3b93f690a2b0320d2d3f388eaab3b75d09d5573b410ce1347b9bef7421bc4b.
Inlining all required owner/budget additions into the public99LF function is
not a natural100LF plan. Two repeated ownership responsibilities can be moved
without splitting business algorithms, introducing another model or extending
any donor lifetime.

Select EXACTLY these TWO added private functions in EXISTING
crates/wf-ports/src/log/record_batch/phase.rs:

1. release_decoded_inputs(decoded:DecodedRequest<'_>)
   ->(RequestScope,RequestId,ClientIntent).
   Visibility pub(in crate::log), arity1. Consume the existing decoded owner;
   move out its request and the existing isolation/state_keys/claim_filters.
   Explicitly drop request.operation, isolation, state_keys, claim_filters IN
   THAT existing order. Return only the moved owned scope/request-id/intent.
   No returned reference or input lifetime, replacement operation, clone,
   retained encoded buffer, Vec, Box, callback or new header model. Callers
   copy their existing encoded_len/limits/time BEFORE consumption as today.
   They may destructure the returned flat tuple into scope/request/intent;
   their unchanged four header-length reads then use those same moved fields.

2. retain_record_batch_outcome(
     header:(RequestScope,RequestId,ClientIntent),
     log:LogIdentity,time:TimestampMillis,outcome_ttl_ms:u64,
     response:(u16,Vec<u8>,RetainedStatus),admission:AdmissionLimits
   )->Result<Box<RetainedOutcome>,RecordBatchApplyError>.
   Visibility pub(in crate::log), arity6; the two flat tuples are existing
   moved ownership cohorts, not named models or borrowed donors. Move ONLY
   the repeated checked time+outcome_ttl_ms, RetainedMetadata construction,
   RetainedOutcome::new and existing single Box::new here. Preserve the same
   RequestValueError::InvalidExpiry and existing error conversions. The helper
   consumes the already encoded response with its exact version/status; it
   does not measure, encode, gate, reduce, hash, retry or decide that status.
   Caller stores the returned Box directly, never boxes it again.

Root's later BODY batch may add only these names to the existing internal
phase reexport in record_batch.rs. Use explicit crate::log paths where needed;
no public export, file/module/model/trait/alias/constant or additional helper
is selected. Existing request types are imported from
wf_domain::workflow::request, AdmissionLimits from workflow::admission,
RequestId from wf_types::identity and timestamps/LogIdentity from version.
No change to the scalar response wire formats or the existing Box behavior.
Putting both helpers in phase.rs avoids consuming the parent's remaining56LF
with shared constructor/drop code plus its new field/accessor/error mapping.
Their ordinary projected spans are well below100LF and phase stays below400;
actual pinned formatting/census remains required after implementation. This
is no geometry waiver or permission to compress statements/delete blanks.

All FOUR joined paths keep their existing profile/group/authority validation
and reducer order. V7 additionally retains the existing last-entry/log check.
After a real reducer result, while decoded source operands are still live,
construct/admit the numeric effects owner on the SAME control/decision, then
call release_decoded_inputs. The builder result owns only numeric descriptors;
it must not borrow request fields or retain the old payload Vecs to call the
inspector later. Preserve EVERY accepted intermediate history/version, V6
source-index holes, Some(empty), Some(equal-to-old) and real zero-mask history.
Whole rejection and real no-history yield None. Replays never run the Fresh
builder. Unsupported wire/profile operations remain UnsupportedProfile; a
zero-slot command is not proof that an unsupported route is accepted.

The profile-specific response measure/cap/late admission/encode stays in its
original producer. ONLY AFTER encoding call retain_record_batch_outcome:
V5 and V6 use response version1, publicV1 version2, atomicV7 version4. Thus the
old order remains response encoding -> checked retained expiry -> retained
value validation -> one Box allocation. The helper does not move retained
expiry ahead of an existing byte-limit/response error. Its outcome TTL is the
existing request-outcome retention parameter, NEVER a captured workflow-value
expiry witness. An error drops the newly owned numeric spine and existing
moved owners normally; no partially built plan or grant escapes. The resulting
owner remains attached to the real RecordBatchApplyPlan through the existing
consumed transaction path; into_parts remains the later reply handoff/drop.

finish_inspection needs NO new helper or factoring. Its current24LF body has
ample room for the already selected Result refinement: same arity4, return
Result<RecordBatchInspection<'input,'retained>,RecordBatchApplyError>;
after the real retained gate, admit decoder_bound+F BEFORE constructing any
Complete plan, then drop decoded and return Ok(Complete). The Fresh branch
remains the same boxed decoded handoff. Directly propagate this Result through
ONLY its existing five callers: phase inspect_record_batch72 and
inspect_record_batch_point92, independent68, public_mutation/apply60,
atomic_start58. Keep each caller's prior profile/replay-response/log checks
before that handoff. No second gate or new request/retained authority.

Explicit prospective accounting, superseding any inference that old slack
pays for the new field/spine:
E=encoded stored request length; B=decision_bytes; n=source item count;
H=the existing four owned header byte lengths; S=existing measured response;
A=additional_live_bytes; k=actual emitting-history count after semantic census.
D=checked(4E+B+1024n+2048).
Old Fresh inputs boundary I=checked(D+192n+256+B+A)
 =checked(4E+2B+1216n+2304+A).
F=size_of::<Option<RecordBatchValueEffects>>(), always resident on each plan.
R=checked(k*size_of::<RecordBatchValueEffect>()), only real descriptor rows.
The public effect cap bounds R alone; k0 returns None/no Vec and may pass cap0.
The n-row product is a checked upper estimate, not an early cap or reserve.
After the unchanged old I admission and real semantic/count pass, prospectively
admit checked(I+F+R) BEFORE the single actual row-spine reserve. All semantic
association/field/profile checks must precede that reserve. Real reserve
failure remains operational; no capacity==requested or allocator-failure
coverage claim. F is counted once: Option already contains the Vec-owner
representation; no assumed zero-sized None or duplicated Vec metadata term.
At late construction use checked(E+B+H+531+A+F+R) for V5, and
checked(E+B+H+512+S+A+F+R) for V6/publicV1/V7, BEFORE response allocations.
Complete/replay/conflict/expired/LogReplay construction uses checked(D+F),
without a descriptor Vec. These are logical requested coexistence profiles;
no unnamed512/531/192n credit, exact allocator capacity, total Rust frame,
RSS, initial-R or full E/T proof is inferred. The selected new-work tariff is
separate; the two helpers merely relocate existing drops/retained construction,
adding no effect scan, byte comparison, gate or reducer traversal.

Preserve old refusal STAGES when updating actual tight-bound tests later.
For the existing minimal V5 frame, D=16780748 and I=33558412. The old assertions
in tests/record_batch/main.rs498..546 still truthfully reject 16780747 at D,
16780748 at I and33558411 at I: these fail BEFORE the new census/reserve stage.
Do not blindly add F+R to their attempted numbers. The old exact-success input
at I now requires checked(I+F+R), with k1 even for a created history whose
three supplied slots are absent. Preserve its accepted metadata/state/outcome
assertions, calculate its new exact cap with actual size_of, and add the new
one-short refusal at I+F+R-1 reporting I+F+R. Likewise retain the distinct
additional-live, response-cap, status/replay and lifetime scenarios in
record_batch/{phases,touched}, independent_batch/main,
public_mutation/gate and record_batch/atomic_start/response. Decoder-only
failures remain D; a successful Complete construction now needs D+F. Rebase
only the mathematically affected successes/new-stage refusals, never replace
all caps by WORKSPACE or remove old business/byte/pointer assertions. These
are named later Root-controlled formula integrations, not permission for the
current six-path R0 writer to edit old tests. Actual complete source/result
review must verify the new errors occur at the selected reached boundary.

Fixture premise correction is accepted within the unchanged eight-case roster:
stable lifecycle.rs184..228 checks terminal_state at212 BEFORE making a Cancel
plan, so Cancel after Cancel is an error, not LegacyNoop. Root additionally
source-checked pinned flow_transition1474..1480 -> terminal guard604..610 and
lmdb terminal-state set804; no VM/profile result is claimed here. Use that real
error to preserve the intended V6 indices0/2/3 and holes. Genuine no-history is
repeated Complete against a separately completed donor in the SAME existing
carry/control case (Root cites source fencing_retry_test33..50, one completed
history). No invented successful duplicate Cancel, helper/test/model/path or
core behavior change is needed. This supersedes any earlier scenario shorthand
that conflated terminal duplicate Cancel with repeated Complete.

This decision adds exactly TWO private lifecycle helpers to the prospective
BODY roster and keeps finish_inspection's already selected Result change.
It does not release BODY, alter current R0, grant detached descriptor/storage
or expiry authority, or complete catalog/current+every-history/ordered-read/
recovery/retention/typed hydration. Root must separately adopt the source/test
integration after genuine compiled R0 SAVE. PERF's finite event census is a
separate implementation condition; no peer acknowledgement holds this choice.

### NEXT producer error-context closure: real-R Workspace before reserve

Root identified an actual gap in the prospective private signatures: a bare
RecordBatchValueEffectsError::Limit loses both actual counted R and whether
work or row coexistence refused. The old effects_apply_error1 cannot recover
Workspace{attempted:I+F+R,maximum} without another scan or an incorrect n-row
substitute. The following EXACT PRIVATE refinement supersedes that part of
22049/cdf324 and11312/088721f2. No current R0 source was read or changed; the
public models/error/API/getters, tests and two lifecycle helpers are unchanged.

Keep both builder names and arity5. Their signatures become:
build_atomic_value_effects(
 decoded:&DecodedRequest<'_>,control:ControlPlan,decision:&IndexedDecision<'_>,
 workspace:(usize,usize),maximum_work:usize
)->Result<Option<RecordBatchValueEffects>,
 (RecordBatchValueEffectsError,Option<(usize,usize)>)>;
build_independent_value_effects has the SAME arguments/result except that its
third argument is &IndependentIndexedDecision<'_>.
The fourth argument is (old_live_I,workspace_max), NOT remaining row bytes and
NOT an already admitted I+F. Both returned scalars are ordinary private budget
context, not transaction/admission authority or a public proof token.

Keep value_effects_budget arity3, with the exact private signature:
value_effects_budget(decoded:&DecodedRequest<'_>,workspace_limit:usize,
 additional_live_bytes:usize)
 ->Result<(usize,usize,usize),RecordBatchApplyError>.
Return the flat scalar tuple (I,workspace_limit,maximum_work). Compute the
already selected checked old-live/work bounds, without introducing an early
new-F or row-cap refusal. Its n*size_of estimate remains an upper estimate,
never a reserve count or cap gate. The actual existing old I admission stays
at fresh::inputs before the reducer. No new helper, error/model/alias, callback,
file, public API, source scan or byte owner is selected by this tuple change.

A builder uses this order, shared by all four real producer joins:
1. Preserve reached work spending and the COMPLETE semantic/count prepass over
   immutable decoded operands and every actual result. This produces k, with
   no output reserve. Every reached InvalidChanges/UnsupportedProfile or
   work-refusal origin remains at that reached event. Do not cap n rows or
   pre-refuse a new F term before this pass.
2. Compute checked R=k*size_of::<RecordBatchValueEffect>(), then checked
   base=I+size_of::<Option<RecordBatchValueEffects>>() and attempted=base+R.
   Both new F and actual-R arithmetic occur AFTER the semantic/count pass;
   even base overflow cannot mask an earlier semantic error. No third pass,
   payload compare, count-cache owner or maximum-n fallback is needed.
3. If attempted is representable and attempted>workspace_max, return exactly
   Err((RecordBatchValueEffectsError::Limit,
        Some((attempted,workspace_max)))). This is the ONLY Some origin.
   No reserve, push, retained outcome construction or caller rescan occurs.
4. Otherwise the exact whole I+F+R coexistence is prospectively admitted.
   If k0, return Ok(None) only AFTER this F-only check, with no row Vec.
   If k>0, derive checked remaining_rows=workspace_max-base and call existing
   reserve_effects2(k,remaining_rows) once. That cap is derived AFTER admission,
   not used to suppress census or infer worst-case count. Continue the one
   original build pass using the SAME monotonically spent work budget and
   already selected immutable predicates; attach the result before dropping
   decoded donors. The selected algorithm uses at most its two passes.

Every other private builder error carries None in the context slot:
(error,None), including semantic/profile diagnoses, work exhaustion, checked
arithmetic overflow and the actual TryReserveError. Overflow has NO exact
representable attempted usize: preserve its existing E::Limit operational
encoding/limit mapping, never invent usize::MAX or a false Workspace value.
Move the real ReserveFailed cause through the tuple unchanged; no Copy/Clone,
Display/Error implementation, Box, allocation or fabricated reserve test.
Inline error wrapping is bookkeeping, not permission for another named helper.

Refine only existing private effects_apply_error1 to accept
(error:(RecordBatchValueEffectsError,Option<(usize,usize)>))
 ->RecordBatchApplyError.
Map (Limit,Some((attempted,maximum))) from the actual attempted>maximum branch
to the existing Workspace{attempted,maximum}. Other causes retain the previous
cause-specific mapping; None on Limit remains the existing operational
encoding-limit lane. Do not translate every bare Limit into Workspace. An
inconsistent private non-Limit/context pair must not be turned into a workspace
grant or panic; its original cause remains the fallback. The private tuple is
not exposed by the public inspector and adds no new publicly fabricable model.

Cap and priority consequences are concrete:
- Public inspector maximum_effect_bytes still caps real R alone, and returns
  its original E. With sufficient separate work, a genuine None accepts cap0;
  F is not silently charged to that R-only public cap. A positive owner above
  that explicit cap returns ordinary E::Limit, not a producer Workspace.
- Producer workspace includes F on EVERY returned plan. With k0, sufficient
  old I but insufficient I+F returns Workspace{attempted:I+F,maximum}; it does
  not return a free None. With k>0 and maximum=I+F+R-1, return the exact
  I+F+R bound, even when maximum<I+F. No detached count inspection is required.
- Old decoder D and fresh I refusals still happen at their unchanged earlier
  stages and retain their old attempted numbers. Only a request which passes
  those stages and reaches full semantic census can produce the new exact
  workspace refusal. If work exhausts before completing census, it remains a
  work/encoding Limit with None; the implementation must not invent a count
  or claim a later workspace diagnosis took precedence.
- Late producers retain the SAME already-owned F+R and selected profile late
  formula; Complete paths still admit D+F via finish_inspection Result. This
  bridge does not turn a retained outcome TTL into workflow-value expiry or
  change profile acceptance, history holes, version/kind provenance or drops.

The new fixed context/error/arithmetic steps must be included in the actual
BODY work-event recount. Avoiding a third scan or heap owner is not permission
to omit reached bookkeeping from the selected fixed budget. No exact native
frame, allocator capacity, initial-R, full E/T or physical-memory claim follows.
Natural formatting/function/file census and actual unchanged-oracle results
remain Root's later BODY checks after genuine R0 SAVE/release.

PERF independently proposed the tuple bridge and agreed with the refined
(I,max) arithmetic order. I actually whole/hash-read its written supplement
152179+4037/bc79709a4323e76a6c35970574d0266412e2bd63265c8479cd520a3d86806274,
end156216, and found no disagreement. It explicitly preserves cap0/None/F-only,
actual attempted context, existing other errors and future source recensus.
No acknowledgement or extra proposal cycle is required; this closes the
identified private signature/error-lane gap only. No source creation, BODY
release, catalog activation or original HistoryValues mapping is inferred.

Arithmetic wording precision for the preceding positive-row example: the
one-short maximum I+F+R-1 is at least I+F when R>0. Separately, ANY smaller
maximum<I+F which has passed the old I boundary must still report the same
post-census attempted=I+F+R for k>0, not just I+F. These are two distinct
maxima, not a claim that the one-short maximum is below its own base.

### NEXT provenance actual stopped R0: six paths, real producers, eight genuine failures

Root requested independent stopped-source review, then relayed the actual
canonical technical and callable results. I read ALL six delivered files,
including final intermediate one-short loop and final fixture lifetime spelling.
I did not run any workload, formatter, compiler, VM or Git command, or mutate
source/tests/shared metadata. Only this architecture note is appended. There
is no BODY, producer attachment, new plan field or implementation helper in R0.

Current complete identities (paths relative to crates/wf-ports):
- src/blob.rs79LF/2905B/4faf3c4b7f23552df93c0a9a6432a11630517f2fa1a71f18aaffce81d3709ee7.
- src/blob/publication.rs80LF/2566B/1e92c1dabd433caf5595bc4601dcb80f14bbf4277d3fdfe088b8866af0d681e6.
- tests/workflow_value_effects/main.rs7LF/151B/e7866225c9d6fe5c56b347a2cd4cc7ba2385e5ae0fae699eefcdeba0a981f5a1.
- fixtures.rs324LF/10950B/86acaa03bc0528895ee404d709213414aa6fcf4a1f1a543c4dc6fd2bd81920c5.
- provenance.rs432LF/16195B/b401cc558671f20c087d01fecf8012ec5a9c8f6b68324bf40a105b437850ce7a.
- intermediate.rs238LF/9741B/9e8c29f6975afabfe6177243e7663512fbf3d1798d82333b80f592e306774b12.

Own full-parent inverse removes ONLY the private publication module declaration
and its four-line public use block (166B/5LF), reproducing ENTIRE old
74LF/2739B/3e742999d88b4c15038b4f9ca9e5546d348d9bb4f85052f89817611c4be931f4.
The previous REF models/types/functions remain byte-identical in that inverse.
I also actually whole/hash-read DOMAIN9581+2587/fd4822352f527cdd7723a6af2d30e70dd06ca76da173ae808257f98d05372fe5,
end12168, including author STOP, exact selected paths and original prefix proof.
Root independently reports all other1594/f470425a held inputs unchanged; that
full held-map claim is Root's verification, not an own unperformed inventory.

Models/API exactly match selection. RecordBatchValueEffect is Copy with PRIVATE
u32 source index, u64 version, three supplied bools in p/r/e order and terminal
bool. Its four const immutable getters are item_index/version/supplied/terminal.
Effects owns PRIVATE Vec<Effect>, derives Debug/Eq/PartialEq without Clone and
exposes only steps(). Debug-only Error has exactly InvalidChanges,
UnsupportedProfile, Limit and ReserveFailed(actual TryReserveError). No trait,
constructor, writable vector/ID, mutable getter, fabricated permit or error
message protocol was added. The public borrowed inspector has the selected
four parameters and returns Option<&'plan Effects> tied to the PLAN borrow,
not the DecodedRequest verification borrow. It literally always returns
UnsupportedProfile. No count/reserve/work/body shell, NameKind, private scan
module, lifecycle helper, shared commit field or producer join was smuggled in.
This is an honest callable baseline, not a claim to inspect existing owners.

Fixture cut is exact: one TestResult<T=()> alias, WORKSPACE64MiB and SIXTEEN
functions limits/scope/clock/log/control/item/request/batch_frame/public_frame/
atomic_frame/claimed/with_view/prepare/inspection/decide/outcome. The main file
has only docs/forbid unsafe and its three normal child modules. Helpers build
real prepared bytes using existing selected encoders, prepare functions,
retained point gates and four Fresh producer functions; they do not construct
an Effects owner, call the inspector or calculate expected masks. Their
mathematical touched facts are ordinary inputs to the real reducers, not a
storage callback or fake persisted-success path. actual claimed fixtures run
real Create then Claim reduce calls; V5/V6 do not gain a Claim encoding route.
Nested Result preserves setup failure separately from SUT operational failure.
The inspector's Debug-only error is explicitly checked/matched, never forced
through the TestResult error trait object by adding a convenience Error impl.

Root's technical Clippy feedback removed ONLY the explicit single-input
prepare lifetime spelling: prepare(profile:u16,bytes:&[u8],workspace:usize)
 ->TestResult<Result<DecodedRequest<'_>,RecordBatchApplyError>>.
The only reference input is bytes, so this preserves the same returned payload
lifetime and all16 helpers/arguments. My exact19B inverse restores ENTIRE
324LF/10969B/75a6f0f53dea09c9d22c1626d17c5b146a819cb746df66f4031bf7bae1e71779.
No allow, lifetime widening, assertion deletion or test-data change was used.
That earlier technical diagnostic is not counted as a behavioral RED.

Own physical census:30 total functions=5 getters+1 inspector+16 fixtures+8 tests.
The six publication functions are17..19/23..25/29..31/35..37/49..51/73..80.
Fixture spans are38..40,42..48,50..52,54..56,58..65,67..77,79..87,89..99,
101..126,128..144,146..177,179..238,240..252,254..280,282..314,316..324;
maximum60LF. The eight EXACT selected test names/spans are:
- value_effects_bind_exact_prepared_entry_and_operation_kinds: provenance24..100/77LF.
- value_effects_distinguish_omitted_empty_and_equal_replacements:103..152/50.
- value_effects_preserve_atomic_input_indices_and_kinds:155..249/95.
- value_effects_emit_carry_rows_for_real_no_value_histories:252..343/92.
- value_effects_refuse_crossed_profiles_and_do_not_invent_expiry:346..432/87.
- value_effects_preserve_independent_histories_and_source_index_holes:intermediate21..99/79.
- value_effects_admit_continuous_work_and_descriptor_coexistence:102..177/76.
- value_effects_borrow_only_the_plan_after_decoded_verification_drops:180..238/59.
Production files79/80<=400, all test files<=600, max actual function95<=100.
There is no geometry waiver, statement packing or unselected helper.

Oracles are independent literal source-index/kind/version/presence facts, read
against the actual decoded operations and real accepted histories. They cover:
- All four selected producers' create/StartAndClaim frames, original prepared
  digest equality, exact version1/mask and unsupported V5/V6 Start encoding.
- Transition omission vs Some(empty) vs equal replacement, checking raw slot
  presence as well as postimage and unchanged original current record.
- Actual atomic Complete/Fail/Cancel/Retry, each version3, explicit original
  positional slots and correct result/error family; Retry retains old payload.
- ExtendLease history with a real zero-mask row. A whole rejected plan and a
  repeated Complete on a separately completed record have no owner at cap0.
- Terminal data with no invented value TTL: outcome expiry1002 remains only
  request-outcome retention. Crossed wire profiles, changed digest/bytes,
  IntentConflict and Replay preserve real gate decisions and no descriptor.
- Real independent same-ID Create/invalid Transition/Transition/Cancel/Cancel:
  results1=WrongState and4=Terminal; histories remain original source0/2/3,
  versions1/2/3, while only final source3 persists as current. Expected masks
  follow input field families, never flattened ValueRow or final_indices.
- Actual F/R size_of and R-only cap, work0/1 refusal, owned I+F+R exact and
  two refused maxima [I,owned-1], Complete D+F exact and one-short, LogReplay
  None plus separate work refusal. The added one-short loop preserves the
  exact selected post-semantic real-R Workspace origin; no worst-case nR.
- One plan survives temporary decoded verification; repeated same-plan views
  and nonempty row pointers are equal. A second simultaneously live equal
  plan's owner/row pointers differ. Each pointer comparison is preceded by
  positive exact length/content assertions, never vacuous empty dangling data.

The duplicate Cancel correction is maintained: it is actual Terminal error,
not LegacyNoop. The independent runtime reaches and checks those two real
error holes before inspector. Repeated Complete is the separate genuine
no-history source premise, with explicit accepted LegacyNoop/history-none
assertions in the carry/control case. Root's pinned original source witnesses
were previously recorded; this review invents no contradictory duplicate
Cancel success and no storage/expiry fact from these numerical descriptors.

I actually whole/hash-read the complete final seven-technical record at
/private/tmp/ferricstore2-m2-source-pin-validation.log9123749+8238/
7644fcf902521caabf83bba9cd3bdfc909d14ce6f683b4e032d0e2755038afdf,end9131987.
Own parsed footers are EXACTLY seven EXIT0/UNCHANGEDTrue, all headers name
1600/a5c429eb23e3db613454481a773081f75a694f9729c6b15649497474aa50ce7c.
Fmt, ports all-target compilation, strict Clippy, architecture53, reference480,
progress120/132/550 and diff checks pass. Compiler acceptance includes the
returned-view lifetime test; this is not runtime pointer evidence.

I then actually whole/hash-read target9132197+4279/
01efa4412d998e6c7639429ebc3afc8e7f28645fa9ecc9a3b4e693c5c27a8d63,
end9136476, plus its exact header and EXIT101/UNCHANGEDTrue footer. Own Counter
of all source #[test] names equals all EIGHT runtime names/statuses:0PASS8FAIL,
no ignored/measured/filtered cases. Every failure is the intended positive
is_ok assertion receiving actual UnsupportedProfile from the callable stub:
provenance76/138/224/288/377 and intermediate83/125/204. There is no missing
symbol, profile-preparation failure, mismatched history premise or technical
Clippy result being called RED. The same1600/a5c429eb input identity is bound.

Masking remains explicit. The all-profile loop stops at V5 after constructing
all four frames, so its publicV1/V7 Fresh producer iterations are NOT yet
runtime-covered. V6's independent history/error premises do execute elsewhere.
The omission loop stops on its first omitted case; empty/equal tails remain
masked. Atomic four-operation history/version/raw-slot premises execute before
its failure. ExtendLease premises execute, but later whole-rejection/repeated
Complete setup/assertions remain masked. Terminal Complete premises execute,
but crossed-profile/digest/replay/expiry tails remain masked. Workspace test
executes its actual F/R>0 setup before125; cap/work, one-short, exact coexistence
and Complete F assertions are later masked. The two real lifetime plans are
accepted before204, but returned-view/pointer assertions are runtime masked.
Do not infer that work0/1 would prove the full continuous two-pass tariff even
once reached; the actual future source event census remains mandatory.

Two prospective PRIVATE BODY spelling precisions from DOMAIN are accepted
without R0 change: the new plan value_effects field uses the same nonpublic
pub(super) crate::log scope as existing control/decision, so the real sibling
public_mutation/apply constructor remains usable. Its selected immutable plan
getter does not make the field publicly writable or grant detached authority.
Exact effects_apply_error fallbacks are InvalidChanges->Apply(InvalidAuthority),
UnsupportedProfile->Encoding(UnsupportedTag), (Limit,None)->Encoding(ByteLimit),
ReserveFailed->Encoding(Allocation). Only the actual representable
(Limit,Some((attempted,max))) coexistence refusal maps to Workspace. Bare Limit
cannot recover arithmetic-vs-work provenance, so do not fabricate LengthOverflow
from it. No new public variant/helper/trait or changed source profile is selected.
These spellings complete the previously named lane/visibility contract only;
Root's future BODY adoption/release remains separate.

Scoped conclusion: no R0 source/oracle/ownership/geometry defect found; all
EIGHT genuine compiled runtime failures are accepted as the NEW rejecting
baseline. No producer-joined descriptor implementation, exercised reserve
failure, allocation/CPU/RSS improvement, publication/catalog/current/history
binding, durable ordered read/recovery, typed hydration, native8 or Linux/full
product parity is claimed. Original HistoryValues three arrays remain unmapped.
Root owns genuine R0 SAVE before any separate BODY release. PERF's independent
written receipt will be read ordinarily, without a new acknowledgement gate.

Ordinary written reciprocity closed: I actually whole/hash-read PERF156216+10830/
799826b08678f7ea45e8255c17109d0fc539f838a6dcdb8c08478dc56d6617bd,end167046,
including its224B explicit errors1/4 vs history0/2/3 clarification. Its full
six-source identities/inverses/geometry, resource and real-producer oracles,
actual eight-name0PASS8FAIL Counter and masked tails agree with this receipt.
No disagreement or further source change is required. PERF attributes technical7
to Root; my preceding8238-byte independent read additionally binds those seven
actual records. This is ordinary evidence reciprocity, not a new approval gate;
no further appendix/acknowledgement loop or wait for BODY is needed. Review STOP.

### VALUE-DURABLE next boundary: consumed publication, retained knowledge and lifecycle

Finite next-increment preparation ONLY while DOMAIN owns the current numeric
provenance BODY. I did NOT read its moving blob/publication/scan/step or log
producer buffers, mutate source/tests/shared metadata, create an artifact, or
run a workload/Git. This note reuses the closed193829+21871/6d4fd518 inventory;
it does not reopen the native/foreign ABI or select format/family numbers. The
current BODY's later actual review/SAVE remains independent of this proposal.

The original design's actual duties remain architecture.md193..205: one ordered
apply owner, publish only after persistence, retained rejections/control and
atomic authoritative indexes; bounded inline bytes are acceptable initially,
but a reference must never be acknowledged before its bytes are available to
all apply/snapshot paths. Lines211..215 separately require read-barrier/applied
position and tenant/query scope. The catalog is therefore an extension of the
EXISTING consumed transaction, not a seed API, detached callback or a second
preupload database. Existing inline payload/result/error already persist;
the missing durable information is versioned bytes/refs and their historical
and lifecycle relations, not basic inline persistence.

Actual unchanged source witnesses read in this task:
- ports/state/transaction.rs358/f89b072d: GroupApplyChanges237..305 MOVES the full
  RecordBatchApplyPlan into its private owner; plan319/into_plan325 expose the
  same owner. GroupApplyTransaction::commit199 consumes the stable revision.
- storage/transaction/record_batch.rs261/26f5f73c: apply147 opens the real guard,
  scope/control/outcome gate precedes Fresh load;197..202 decides, constructs
  GroupApplyChanges, calls tx.commit(&changes), THEN consumes the plan for reply.
  Thus accepted postimage Vecs and attached numeric provenance can remain live
  through the actual commit; decoded donors need not survive to supply bytes.
- state/redb/commit.rs233/5788bd4d: publish32..75 checks readiness, group/recovery,
  exact expected control, versions/indexes, records/history, outcome/control,
  sets Immediate and consumes commit. publish errors fence through apply24..29;
  once consumed, engine failure is Uncertain, not an unchanged-predecessor proof.
- commit/independent.rs79/20d99c22:45..52 writes final cores once;53..63 writes
  EVERY successful history. Value publication must follow the latter stream,
  preserving original source indices/results and intermediate record versions.
- publication.rs296/1753832e:99..154 currently serializes core and history;
  put192..201 is generic insert/REPLACE. Immutable versioned bytes cannot blindly
  reuse replace semantics: validate exact namespace/key identity and collision
  before reservation/insertion; an ordinary retry is handled by the real gate.
- commit/claim.rs138/3c199c28:21..64 is a DISTINCT consumed Claim transaction,
  with its own validation/outcome and shared write_records. Claim carry/history
  integration is not supplied by admitting Claim into V5/V6 mutation profiles.
These are actual stable file identities, not reads of the current BODY drafts.

The smallest coherent REAL successor is a value-enabled local state profile
whose publication, read and reopen/snapshot paths work together. Recommend that
Root's next architecture batch name VALUE-DURABLE1 around these boundaries:

1. Capture actual binding knowledge in the SAME loaded transaction revision.
   New genuine absence can establish the new record's KnownEmpty birth; a
   present legacy record/history frame remains LegacyUnavailable/LegacyUnknown,
   never KnownEmpty inferred from payload=None. A known set with zero refs is
   a positive represented fact, unlike an unavailable profile or missing row.
   Existing current bytes cannot reconstruct discarded older history. Migration
   needs real retained replay/backfill evidence or explicit refusal; no manifest
   relabeling or fabricated historical refs. Legacy reads may remain available
   under their old capability, without being promoted to hydrated-value reads.

2. Join the numeric SAME-plan row with its actual accepted write postimage and
   authentic existing binding state. Supplied masks already distinguish equal
   replacement and Some(empty) from omission; stored bytes may borrow the
   accepted record's p/r/e owner while the full plan is live. For each history
   in source order, form the next full binding snapshot; supplied fields create
   NEW versioned identity/bytes, omitted fields carry actual prior refs. Emit
   all intermediate history snapshots/value versions, then final current refs
   once. Do not inspect only final_indices, clone omitted payloads, compact V6
   indices or infer field kind from a flattened slot. Real zero-mask histories
   carry refs; replay/refusal/no-op create no new versioned value.

3. Render generated refs from the ACTUAL prepared partition as Explicit(bytes),
   alongside actual group/recovery/tenant and owner/version/kind. Public absent
   or :global caller partition was already resolved before preparation; direct
   raw Keys Global goldens are not authority to choose a different namespace.
   Syntactic/hash-tag rendering does not prove placement or authorize tenant
   reads. Preserve opaque logical command Vec meaning and one FSV2 escape;
   NEVER reinterpret an existing prefix as trusted encoded ETF. The catalog
   frame stores source-neutral ENCODED bytes plus separately represented expiry,
   leaving the later explicitly versioned typed/shared/external input route
   possible. The row schema must not be hard-wired to a Binary-only node.

4. Validate all required capability/knowledge/association/retention before any
   durable effect under the same guard; then place bytes, every history binding,
   final current bindings and existing indexes/outcome/control in that ONE write
   transaction. Unknown legacy or missing writer capability fails closed, not
   a None attachment silently using old encoders. No externally successful ref
   or in-memory publication occurs before Immediate commit succeeds. Preserve
   existing KnownAborted/Uncertain/fencing distinctions and reconcile after
   reopen; never blindly repeat a Fresh write on uncertain commit.

5. Supply an actual ordered owned read from persisted rows after the original
   prepared buffers/plans are gone. One slot per requested ref, including
   duplicates/misses; a stable scope/control revision and actual placement are
   required. Inspect borrowed envelopes before copying; omission uses complete
   STORED length, including escape/header, without decoding. Empty bytes at
   max0 are a value. Storage corruption/backend failure differs from valid
   stored bytes which later produce invalid-flow/context diagnoses. A local
   read revision is NOT the application quorum barrier/cross-group router.
   Current consuming decoder still requires admitted per-occurrence owned
   bytes; fetch dedup does not grant free clones or an unselected decode cache.

6. Reopen and snapshot must understand the feature BEFORE it can be enabled.
   Actual physical families remain0..7. recovery.rs176..222 accepts manifests
   1..4 or dispatches5/6 through the existing policy registry;225..273 validates
   every row but hashes only policy/placement families6/7 for that catalog hash.
   recovery/rows14..48 rejects unknown families. Current format.rs41..46 maps
   FWSS5->FWSP2 and FWSS6->FWSP3. New bytes/bindings require joint closed key,
   frame, layout/origin/profile, row-relation/inventory and snapshot capture/
   receive/install/restore/cohort decisions. No family8/range-only widening,
   new header number, changed old immutable snapshot ID or accidental downgrade
   through policy publication is selected. Valid stored typed payload must not
   require live safe-atom/export context merely to reopen a storage envelope.

The concrete next artifact candidates needing a LATER exact API/type/helper
creation decision are ports/blob/catalog.rs (knowledge/row semantics),
ports/blob/read.rs (owned ordered read capability), storage/state/key/value.rs
(scoped physical grammar), storage/state/value/blob.rs (source-neutral envelope)
and storage/state/redb/values.rs (same-guard publication/read integration).
They are responsibility names only: no shell/model/helper is approved now.
Existing transaction.rs/transaction/{load,observations}, state/redb/commit.rs,
commit/{independent,claim}, publication.rs, read/point.rs, record/history frame
code and the exact recovery/snapshot matrix in21871 all need coordinated edits.
A possible real-test grouping is existing state_store/main.rs seam plus proposed
state_store/values.rs and values/{publication,read,recovery,retention}.rs; exact
fixtures/helpers/case cuts must be consulted before those files are created.
Reuse existing real FileBackend, delegated DiskFaultBackend and prepared apply
functions, not a catalog seeding constructor or successful mock storage callback.

### Retention predecessor and a material three-way source distinction

Reduced Terminal/Cancel still lack authentic captured workflow retention. The
existing active/values.rs TimeoutValueWrite and active/plan274..289 carry only
THAT timeout trace's actual capture; they are not authority for Complete/Fail/
Cancel or request-outcome TTL. Before terminal publication, freeze the genuine
creation capture and terminal apply-time witness/profile. Source deadline uses
captured retention with the original max(requested_now,apply_now) rule/clamp;
current policy, requested-now alone or outcome_ttl_ms cannot substitute. Known
no-expiry must also be distinguished from unavailable legacy capture. Keep this
as a concrete missing input, not a fabricated constructor/default or reason to
claim complete terminal support from a nonterminal predecessor.

I re-hash-bound the WHOLE pinned FlowValuePayloadTest31,595B/
9588e98cb82aebd1fdfe3379797604c8a330c7a94e3a42bd4ec7acceeda0c17d
against workflow-test-manifest.json commit66586c596e271408d9184af32eb31cd7218c126e,
then read the following actual assertion bodies. Root independently agrees:
- 0e1a95deda23f709,182..219: after ORDINARY terminal record removal, the old
  created payload ref remains mget-readable with its full original value.
- 6907b04baf38b9f7,383..436: EXPLICIT retention cleanup reports values>=2 and
  removes BOTH created.payload_ref and completed.result_ref, as well as record
  and history. An old value surviving passive record expiry does not make it
  immune to source-authorized explicit owned-value cleanup.
- 76630580ff263e03,608..670: rewind keeps the original payload ref, clears the
  terminal deadline and reads that payload past the previous deadline. This
  requires the actual rewind/retention relation; it is not permission to infer
  an undocumented mutable-expiry operation from the case name alone.
Therefore immutable stored BYTES/version identity is distinct from expiry and
explicit cleanup/rewind transitions. Neither universal cascade-on-record-delete
nor blanket no-cascade-ever/immutable-expiry is source-faithful. Determine exact
owned-ref enumeration/cleanup authorization and rewind behavior from its real
producer before selecting those APIs. Preserve values required by frozen/pinned/
received snapshot cohorts; row/binding expiry is not permission to break recovery.
No original VM/profile was run in this review.

Minimal meaningful predecessor acceptance, before any value-profile activation:
- Production prepared V5/V6/selected public/V7 commit publishes supplied empty/
  equal/omitted values correctly, preserves each intermediate same-ID version,
  real holes and final current bindings, then reads them after input owners drop.
- Genuine KnownEmpty birth versus actual legacy unavailable frames, with truthful
  migration refusal and no invented old history; real Claim carries refs through
  its separate consumed commit/history path.
- Same transaction known abort versus uncertain/fenced reopen/replay: exact
  predecessor or complete successor, never bytes without bindings/control or
  acknowledged refs without bytes. Enforce immutable key collision semantics.
- Ordered persisted old/new/duplicate/missing/expired/omitted/error reads and
  restart/reopen, checking source stored length and row-vs-payload error origins.
- Current/pinned/alternate/received snapshot capture/install/fresh restore and
  policy upgrades preserve the value capability, rows, bindings and expiry;
  old FWSS/FWSP byte identities and refusals remain exact.
- Genuine terminal capture/no-capture refusal, new terminal value deadline,
  passive expiry vs explicit cleanup vs rewind, and old payload availability.
These name REQUIRED executable seams, not already created tests or a full new
helper roster. A bounded nonterminal binary subprofile can precede them only
as explicit progress; it cannot replace terminal/typed/shared/routed success.
Original three HistoryValues arrays remain[] until the complete actual route
and hydration, including its real error/missing/flag semantics, pass. M2/M3
predecessor authority/barriers and Linux+macOS durability/fault/snapshot evidence
remain separate gates; portable fixed-width framing is not execution evidence.

Resource/ownership condition: charge keys/ref-spines/current+every-history
bindings/catalog metadata/encoded bytes and temporary scans before ownership;
do not reuse old512/192n allowances or treat max_bytes as an I/O/RSS bound.
Use source-neutral row scanning before payload copies, prospective cumulative
transaction counters, and genuine fallible reserves. The pinned redb direct-row
insert_reserve opportunity remains UNSELECTED; it replaces an existing key and
cannot prove immutable collision handling or reduced engine costs by itself.
I actually whole/hash-read PERF37949+12323/
77291c18be33e800bc564a85b4f8dd57e7c05fbccd845678bdd42a52c67087ad,end50272;
its positional kinds, intermediate histories, lifetime/copy/legacy/fault and
row-vs-payload distinctions agree. No gain/allocator/RSS/initial-R/full E/T or
new storage model/fee/dependency authority is inferred. Material seam/retention
findings were sent directly to Root/PERF; no acknowledgement gates current BODY.

### Ordinary existing-test integration after current BODY STOP

Root's concrete old-bound audit fits the already selected geometry/ownership
contract. Its eight existing wf-ports test homes and actual current bound spans:
record_batch/main.rs:582LF, owned-stage function85LF; phases.rs559, Complete
helper92; point.rs318/helper67; replay.rs488/helpers90/86; touched.rs349/bound
functions44/22; independent_batch/main.rs478/bound68; public_mutation/wire.rs142/
helper18; record_batch/atomic_start/response.rs451/bound86. Root may, AFTER the
source writer STOP, import actual Effect/Effects and size_of, rebase ONLY exact
success caps by actual F/R and add real I+F+R one-short checks in those SAME
Fresh bound functions where natural formatting stays<=100. Complete exact D
success needs D+F; None has no row R. Preserve EVERY earlier decoder D/Fresh I
attempted number, additional-live case, business/replay/response/pointer assertion;
no huge WORKSPACE replacement, new helper/type/constant or weakened scenario.
main582 has only18 file lines spare, so its final whole-file census matters.
Near92LF Complete helpers need no forced new one-short duplication when the
new intermediate test already covers that case. No packing/waiver is approved.

One further exact test-only field integration is accepted: existing
crates/wf-ports/src/log/record_batch/atomic_start/tests.rs188LF/7046B/
8976568c8f8b0f76949972dcdb58836e9bafc2fe75502e84281352d1534a5565.
I read113..188, actual forged-retained-postimage function76LF. AFTER author STOP,
Root may bind value_effects in the120 destructure and MOVE the SAME binding into
the175 reconstructed plan literal. Exactly two field lines preserve the real
numeric owner through ALL six original invalid postimage cases and unchanged
baseline/InvalidChanges assertions. No None substitution, clone, rest-pattern
drop, new helper/artifact/assertion or current writer-scope expansion. The donor
.decision partial move is not another plan literal. This ninth existing path
remains naturally below100/function and400/file after the two-field integration.

Finite proposal/integration consultation STOP. These are concrete later adoption
inputs only, not source release, family/profile activation, completed catalog,
terminal capture, storage/hydration coverage or a hold on the current writer.

### NEXT actual BODY A: complete review, concrete fee defects and exact minimal refinement

Root57a77a2 saved genuine R0 BEFORE author release. I independently read ALL
TEN stopped/formatted production files and DOMAIN12168+3902/
1061b1e5809640f0b49c89244586cb1433b9e956e5d4d1be31c41e5be8e47f75,
end16070. I also read the exact nine-test integration diff and reconstructed its
whole beforeimages in memory. No compiler/test/VM/Git/probe/source/shared-file
mutation ran here; only this note changes. The A code's interim eight tests pass,
but reached-work omissions and technical lints prevent final source acceptance.
The following is the EXACT prospective delta Root may adopt before changing A;
it is not a claim those changes have already been written or verified.

SELECT one PRIVATE alias inside EXISTING blob/publication/scan.rs:
type ValueEffectsBuildResult =
 Result<Option<RecordBatchValueEffects>,(E,Option<(usize,usize)>)>;
Use it ONLY as the return spelling of the existing atomic/independent builders.
No nominal model/variant/owner, public export, fee or error shape changes.
Keep effects_apply_error arity1, but take
 &(RecordBatchValueEffectsError,Option<(usize,usize)>).
Its existing case mapping is unchanged; copy only attempted/maximum usize from
the borrowed pair. Existing producer map_err closures borrow their OWN owned
local error tuple; value_effects_budget similarly borrows its temporary
(error,None). The real reserve cause stays alive through the mapping and drops
normally afterward. No Clone/Box/helper/trait or suppression is selected.

SELECT these literal prospective work fixes in the SAME existing functions:
- supplied_value_fields changes arity1->2 and uses the exact independent return
  lifetime: supplied_value_fields<'a>(operation:&'a Operation,
  charge:&mut ValueEffectsCharge)->Result<[Option<&'a[u8]>;3],E>.
  The returned operand slices borrow ONLY operation; charge's short mutable
  borrow never constrains them. The caller's existing prepay1 owns dispatch.
  Inside each existing arm, BEFORE as_deref, spend1 for Create/Transition/
  StartAndClaim/Cancel, spend2 for Complete/Fail/Retry/RetryWithPolicy, and0
  such visits for ExtendLease. Unsupported dispatch does not invent a visit.
- step.rs A90 preconstructor spend1 becomes5: descriptor construction1,
  THREE unconditionally reached fields.map(is_some) Option examinations3 and
  actual terminal-state classification1. These were not free descriptor
  metadata. Keep the same mask/state/value semantics and borrowed operands.
- scan.rs A173/A216 preceding count spend1 becomes2, BEFORE checked count and
  output.as_deref_mut. The latter Option is reached EVEN in the count pass
  when output is None. Capacity and push retain their separate existing fees.
- A89/A136 reserve call prepay1 becomes3 BEFORE entering reserve_effects2:
  repeated checked R multiplication1, byte-cap comparison1, reserve transition1.
  The helper is not an undeclared single opaque cohort. Its one actual
  try_reserve_exact and real ReserveFailed cause remain unchanged.
No new function, model, cache, frame, file, test, mutable public view or predicate
is selected. The only added declared name is the one private type alias.

SELECT default work constants64 BASE /96 ITEM /4 ENCODED; ONLY ITEM changes
from64 to96. PERF's complete actual-function census, checked against these
source paths, gives the corrected maximum:
Let a<=2 be source as_deref visits and s<=a actual supplied fields per real row.
value_effect fixed22+a+s<=26 plus2*Li compared operand bytes. Atomic count/
emission walk overhead5/7 yields<=64n fixed across two passes; independent
6/8 yields<=66n. Call-wide fixed work85 includes BOTH walk headers, actual
entry digest64, new context checks and reserve's three selected operations.
Thus builder<=85+66n+4L with L=sum partition+ID+supplied operand lengths<=E.
Its default64+96n+4E exceeds this by30n-21+4(E-L)>=9 for valid nonempty n>=1.
Old64 ITEM is insufficient under this literal tariff without unselected framing
credit;80 ITEM is also insufficient at n1.87 is the mathematical coefficient
minimum for this bound, while96 is the selected simple conservative constant.
No unnamed encoded-frame slack, future free fixed events or reset work account.
The attached independent inspector has the lower one-pass bound78+33n+2L
(atomic78+32n+2L); caller limits still apply prospectively, including0/1.
This is a logical event/byte profile, not one unit for every Rust field/native
instruction, allocator latency, full frame/E-T/RSS or a measured CPU bound.
After Root writes the selected delta, actual source/census/format/quality/runtime
must be rebound; this arithmetic alone is not acceptance of unseen B code.

### Actual A identities, geometry and semantic/ownership findings

All paths below relative to crates/wf-ports/src; full independently hashed A:
blob.rs82/3025/68d4c97e50c0d594e3c3c99770a4d584f04eb703ac79f204371cc7b695d35b6d;
blob/publication.rs139/4793/0f4b7ea196447a0c2e6a8a64c0f41b278c2d89685584a1bc8991afdb364ca408;
publication/scan.rs381/12501/9f01f56e6a2bf90b14d2eca0f68144514037dccb6b144a8ff7adb09ab69963f8;
publication/step.rs114/4733/00473ab2eb9adbe4ad391164318380e38770546271e9aedbc17bab30cc783610;
log/record_batch.rs366/13049/03da07bf4c080723f2fa94720adbfc62c81d0ee2060f297d8890695b3cc3fe63;
record_batch/phase.rs313/11130/4f5812ac7c9368251f942416bb2ef3a668d3890080b5929e23e5ce7aa25db2e8;
record_batch/fresh.rs232/7515/e8a15436ad0be819b8d540906f1f76922ffd095e34b6e23fc3a245f5ed03c588;
record_batch/independent.rs161/6222/75b0a90ce7782ef5d61ad0c7d7bae2766ce29b24070223d3f09736497df0be3b;
record_batch/atomic_start.rs154/5669/09e9fc5213ee05ac2f28a9265055a179277c8d5c9e8076eff54d7e1a29f204b0;
log/public_mutation/apply.rs157/5710/e483d3e114c92a70460804f1e96929ac5e7e27e6ac4577301496f2e249a8ec68.
The scan/step names above are children of blob/publication, not extra roots.

Actual A census is67 functions/methods, NOT author's66: facade6 +scan10
(including spend)+step2+record21+phase10+fresh9+independent4+atomic2+public3.
Every name is selected: old R0 count50 plus17 additions. This is receipt
arithmetic, not discovery of an unapproved helper. Maxfile381, maxfn88 at
value_effect10..97. The four Fresh functions are75/80/83/85LF; lifecycle helpers
16/29; Complete finish_inspection41. All naturally fit400/100 without packing.
The private alias adds no function; revised natural census still needs checking.

I actually whole/hash-read the exact production reverse diff in the existing
validation log9157966+31452/
05379f9cf71bc736c8c9039b6c9c392f535f40e0576b66554fb68529d07b2590.
My IN-MEMORY reverse reproduces ENTIRE eight pre-BODY files:
blob4faf3c4b/publication1e92c1da/record686d964e/phaseb6c45c97/
freshec3b93f6/independentdd7e64c1/atomic8f0472c4/public5f1d635d,
with the full256-bit SHA identities already recorded for each R0 beforeimage.
Both new children are separately complete-read sources. The public three-model/
five-getter block1842B/eab73b763e179c8e582e7291180b1fd35c4680981075226a29e1483fb8e48b8b
also independently reconstructs WHOLE R0 publication2566/1e92c1da with the saved
old import/docs/rejector tail. Model fields/traits/privacy are literal, not a
similarity claim. Blob's private export inverse alone removes120B->whole4faf.
All unselected wire/intent/staging/reducer functions are preserved by the actual
diff. No encoder/catalog/storage/expiry/routing feature was added.

Source checks found the selected semantic cut implemented correctly apart from
those explicit work omissions:
- validate_effect_entry binds actual next-entry prepared digest to decoded,
  supports exactly1/5/6/7, nonempty items and singleton public/V7, and the actual
  PublicMutation root. No digest recomputation or invented source authority.
- Atomic rows stay input-aligned; independent iterates results().enumerate(),
  paying Err visits and retaining holes, never final_indices/history enumeration
  indices. Correct source partition/ID, supported operation family, write+
  history+Changed shape, history kind/version and supplied postimage bytes are
  checked. LegacyNoop/NotEligible with no write/history produce no row; malformed
  shapes refuse. Claim/Reclaim/StepContinue stay unsupported; V7 alone accepts
  StartAndClaim. ExtendLease is a real zero-mask history. Cancel error is mapped
  from its actual error field; Complete/Fail/Retry retain correct p/r/e order.
- Some(empty) and equal supplied values remain supplied. Omitted old payload/
  result/error contents are never scanned. Byte comparisons prepay both actual
  sides after length equality, and usize->u32 is checked before each real row.
  Only numeric descriptors survive; no input/descriptor borrow in the owner.
- Complete semantic census finishes BEFORE actual R/F arithmetic and exact
  I+F+R check; only representable workspace refusal returns Some(attempted,max).
  Arithmetic/work/semantic/reserve errors use None. k0 still checks F before
  returning None, with no row reserve. Positive k gets ONE fallible exact
  reserve and pushes behind capacity checks, preserving allocator>=requested
  rather than capacity==requested. Same immutable predicate drives pass2;
  emitted==count is verified. Charge stays continuous across both passes.
- Public inspector checks the ATTACHED existing owner, does not allocate a new
  result, and returns its plan-only borrow after the decoded verifier drops.
  Missing owner on a nonempty real history returns UnsupportedProfile; empty
  attachment or attached owner on Complete is InvalidChanges. Public byte cap
  is actual R only, separate from work; no-history None permits cap0 with work.
- All four producers build before the same explicit drop group. Header3 and
  response3 cohorts move, never clone. V5 late E+B+H+531+A+F+R and other three
  E+B+H+512+S+A+F+R are checked before response allocation. Old D/I admission
  remains upstream in unchanged fresh::inputs. Response versions1/1/4/2 and
  response encode -> outcome-expiry check -> RetainedOutcome validation -> one
  Box remain; request-outcome TTL never grants workflow-value expiry.
- finish_inspection's five callers preserve their old profile/gate/replay checks.
  Complete admits D+F then drops decoded and constructs None; Fresh keeps the
  existing boxed handoff. The new field's nonpublic crate::log visibility permits
  sibling producer construction. GroupApplyChanges can retain the whole owner
  through existing consumed commit; into_parts remains the later reply drop.

### Exact nine existing-test integrations and actual interim result

I actually whole/hash-read log9137884+15773/
13ca80610e6bb184bdbb5b477032f3fdc27e689bad10e41b74802dc44bd78ad2.
My independent in-memory reverse of EVERY hunk reproduces ENTIRE saved nine
beforeimages: internal atomic tests8976568c, independent main1bbd53ae,
public wirecdb1ece2, V7 response86742bc1, record maind8188368,
phases23d50d6e, point2fe84e47, replay3947df83, touched05bedc31.
Current whole hashes (same order)681dd106/b3db8e5a/2e7d5979/76dd5c9b/
4a21e998/8c1c6980/ab60fc2f/48fcc46a/b65a1f5a are independently bound.
The changes are exactly actual size_of imports/F/R success caps, new exact
one-short cases, two moved value_effects fields and the ninth test's explicit
RetainedMetadata/RetainedOutcome imports. Every old D/earlierI refusal attempted
number and original business/version/state/response/replay/pointer/forgery
assertion remains. No None substitution or clone defeats the six forged retained
postimage cases. No helper/type/constant/path was added; record main588<600.
All four NEW target files remain whole-byte-exact e7866225/86acaa03/b401cc55/
9e8c29f6, so the same eight named oracles are exercised without weakening.

I actually whole/hash-read strict Clippy2034 bytes at9154492/
ef402e85cda1580c49ce1beaea5707ff6ea7e7c3e586b2b72d6d36459874823f:
two exact type_complexity diagnostics on builder return types and one
needless_pass_by_value on effects_apply_error. The private alias/borrow choice
above addresses these without changing data/error semantics or adding allows.
Earlier inherited test-import correction is technical integration, not RED.

I also actually whole/hash-read interim target9156790+1085/
ea5ae9751415deaa7a5f511ac343c16bd07f6385e9cdde4662496ed2c2775b11.
Own exact source/R0/current name Counters conserve EIGHT identities and flip ONLY
all eight FAILED->ok:8PASS/0failed/ignored/measured/filtered. Root binds its
EXIT0/unchanged1602/40f2cc6d inputs. The previously masked publicV1/V7 producer,
empty/equal replacement, real repeatedComplete/rejection/replay, source holes,
actual F/R one-short/Complete and nonempty pointer/lifetime tails now finish.
That result is real behavioral evidence for A, NOT final quality/work acceptance:
the literal work omissions and three Clippy diagnostics remain OPEN until the
selected correction is written, source-reviewed and actually validated.

PERF independently whole-read A and found the same Option/state/output/reserve
omissions, derived the literal85+66n+4L ceiling, and supplied the exact independent
operation lifetime for arity2. I agree; no cohort/metadata wording exemption is
used. Its forthcoming bounded written receipt can be read ordinarily. No general
ACK gate or second architecture cycle is needed: Root has an exact one-alias/
borrow/arity/charge/96 delta, with no extra helper/model/file/public API/test.
No final-source acceptance, full workspace/current catalog/hydration, original
three mappings, native8, Linux, allocation/speed/RSS or full-goal claim follows.
Finite A review and prospective refinement selection STOP.

Ordinary A written reciprocity closed: I actually whole/hash-read PERF172244+
13850/d68a3682f87e4321bb1ac73923770fca20ef7c92d2ad24dbb961b5580054097f,
end186094. It agrees on complete A identities/inverses, exact nine-test changes,
interim eight repaired statuses, ownership/order and the still-open Option/
state/output/reserve fees; its actual85+66n+4L census and K96/lifetime precision
match this exact selection. No disagreement remains and no additional appendix/
ACK loop is needed. Root's subsequently stopped B is a NEW actual source delta,
not retroactively accepted by the interim A result or this reciprocal read.

### NEXT actual corrected BODY B: exact delta accepted; one existing-test premise corrected

After reading the published A review/selection, Root recorded adoption BEFORE
creating the one private alias or applying the fee/borrow changes, formatted
23102 and returned stable B custody. I read the actual revised scan/step and
all changed mapper/caller sites. Own WHOLE-file inverse of EACH changed B path
reproduces its complete A hash, not just selected similar lines. Blob/facade
remain byte-literal. No code/workload/Git/shared mutation ran here.

Actual changed B identities relative to crates/wf-ports/src:
- blob/publication/scan.rs383LF/12513B/023bce5f95041ba09df99339e0a45c347f57bdf5b45ba6977be5678576348c5e;
- blob/publication/step.rs139/5220/68ffc12cedf2d50b063bd306c17c0c2e83e63d6560ed7e8b5293cd43bf35bbb9;
- log/record_batch.rs367/13082/f3378c2683bbbd4570b754516c1211184befd738f6675a68dfea4a9b9becbd67;
- record_batch/phase.rs313/11146/81251065a6b58df2fe8371a1c3efaf77a23dd6ae338065d0f436281cd7aa67c5;
- record_batch/fresh.rs232/7516/a16912474a5f8687db2f0fe1b901a638bb2c5f41f6b8dee6d4e385fd61809412;
- record_batch/independent.rs161/6238/d9aed639fffcf391543d1cf11f72e95af9293230cbcbc3dc66cac2688e8512d1;
- record_batch/atomic_start.rs154/5685/de444ddb2f940f9485fb3c8a29e7b525b3e2069871d6e57d38d33d02efea765b;
- log/public_mutation/apply.rs157/5726/5e5cd1835e4a79204e16e8feb472141324f1e0bebf6e4c08bab63f26dbddab66.
Their whole inverse identities are respectively A9f01f56e/00473ab2/03da07bf/
4f5812ac/e8a15436/75b0a90c/09e9fc52/e483d3e1, with full hashes bound in A.
The remaining blob68d4c97e/facade0f4b7ea1 are still literal. Thus prior complete
A source inspection plus the actual B delta covers all ten current sources.

The delta is EXACTLY the selected alias, borrow signatures/closures, operation-
only helper lifetime, four categories of missing fees and ITEM96. In detail:
scan18 has PRIVATE ValueEffectsBuildResult, returned by only the same two
builders; no model/error/owner change. Mapper borrows its tuple and copies only
the two numeric bounds. Four inline producer closures borrow their local owned
error; fresh budget borrows its temporary(error,None). Real cause lifetime/drop
and all existing error diagnoses remain intact, with no Clone/Box or allow.
step99..139 has explicit 'operation tied ONLY to &Operation; charge is a separate
short borrow. Its1/2 arm prepayments precede EVERY actual as_deref, including
None; ExtendLease has0. Caller dispatch1 remains before entry. The descriptor
site90 prepays5 before its three Option examinations/state classification and
construction. Both walk count sites175/218 prepay2 BEFORE checked count/output
Option, including the count pass's reached None. Both reserve calls91/138
prepay3 BEFORE multiplication/cap/reserve. One charge persists across both
semantic/build passes; none of the predicates, byte comparisons, row ownership,
real-R exact workspace checks or source error ordering changed.

Actual B census remains67 functions and now ONE selected private result alias;
maxfile383 and maxfn88. Step helper grows only to41LF. No synthetic helper,
public API/model/variant/file/test change, packing or geometry waiver occurred.
The corrected actual census85+66n+4L is covered by64+96n+4E with margin>=9 for
valid n>=1/L<=E, as derived in the preceding exact selection. Inspector remains
one-pass<=78+33n+2L. No remaining separately reached unpaid event was found in
this bounded component review. Logical-work completeness does not imply total
CPU/frame/allocator/initial-R/full E/T or physical-memory completeness.

I actually whole/hash-read the COMPLETE corrected technical7 record at
log9189419+8337/dfad5736c7507190320087ffdbd2b7e0cbc50588b6715b99502afe4ecc78430a,
end9197756. Own parsed footers are7xEXIT0/UNCHANGEDTrue; every header binds
1602/03b6caa145f8f758f644a88f2dce689cc7988092e7208739ea5bf0cec59f05d2.
Fmt/check/strictClippy/architecture53/reference480/progress120132550/diff all
pass on actual B. No former lint is relabeled behavior. I also actually read
corrected target9197985+1084/
832a4a38ee46a22098d0b67bf79f252a9138cf1dcba5aa2d04358771b2d93532,
end9199069, with header/footer:8PASS/0failed/ignored/measured/filtered,
EXIT0/UNCHANGEDTrue. Own Counter preserves all eight R0 names and flips only
the original eight FAILED->ok. Both fee-fixed source and all former masked
semantic/cap/pointer/profile tails are now actual scoped evidence, unlike A's
interim fee-defective pass. Broader all-ports/workspace acceptance is separate.

### Explicit supersession of one wrongly assumed test-integration row count

The subsequent actual all-ports run terminated101 at
public_mutation/wire.rs142, the newly added one-short assertion. I read its
actual diagnostic and the stable original setup: main caller84 constructs
COMPLETE, and decide_missing's touched-current157..163 is None. It produces
no accepted history, so actual k0/R0. Our earlier generic F+oneRow adjustment
in THIS single existing test was wrong; I accepted that inference too quickly
when reading the patch without following its original fixture all the way.
PERF independently confirms the same premise. This is a test integration error,
not a reason to change the correct B no-history result or manufacture a create.

Select the exact minimal correction in EXISTING
crates/wf-ports/tests/public_mutation/wire.rs ONLY:
owner_bytes=size_of::<Option<RecordBatchValueEffects>>() (F alone), remove the
now-unused RecordBatchValueEffect import, and keep the same one-short, exact
success and retained[2,0,0] assertions. Keep every old decoder D and earlier I
refusal attempted number. No setup/operation/current observation is changed;
no new helper/type/constant/file/test, huge cap or assertion removal. The actual
bound is I+F for this missing Complete, not I+F+R. Positive-row publicV1 is
already covered by the unchanged selected eight-case creation loop; it must
not be substituted for this genuine zero-row predecessor scenario.

This supersedes the preceding blanket statement that ALL nine derived cap
integrations were sound: the other eight exact diff/beforeimage checks remain,
while this one formula needs actual-k correction. The original current fields,
wire goldens, missing Complete response and control assertions stay intact.
Root alone applies it after authoritative all-ports termination and binds the
new whole test hash/results. The failed whole run is preserved, not called a
passing all-ports packet or hidden by filtering. No production fee/model/API/
wire or owner change is requested. Actual B production source has no remaining
scoped defect; wider test/runtime closure awaits that exact correction and
ordinary complete execution. No final full-suite/product parity is inferred.

Root/PERF were directly notified of the exact B identities, whole inverses and
this test-premise correction. Ordinary written B reciprocity can close when
its bounded receipt is available, without another approval/append-ACK loop.
Original HistoryValues3, real catalog/publication/read/retention/recovery,
Linux/native8 and whole-goal gates remain unearned. B review STOP on that scope.

Ordinary B written reciprocity closed: I actually whole/hash-read PERF186094+
7249/76e5e516eb1ebc3acca9e70c4d50bf9676af6529e93725d1008bba5e2304d251 and
193343+3419/354c3d334ce8fa5e72f992d9649de2a249e074238ff5a37fd45e90ae9ab0bf95,
end196762. Its full B delta/inverses/67-function census, exact charge/lifetime/
96 proof, actual technical7 and corrected8 Counters agree. Both reviews
explicitly retract the mistaken one-row public-wire cap premise and require
F-only for missing Complete, preserving all original assertions. No production
or resource-contract disagreement remains; that existing-test correction and
wider actual execution belong to Root. No further append/ACK cycle is needed.
Finite B source/result review STOP.

### VALUE-CAPTURE decision: resolve four policies, select the necessary full-donor R0 first

This is a NEW bounded precreation decision, not reopening accepted numeric BODY B.
No source/model/file/test/ADR/policy/workload/Git change was made; only this note.
I actually whole/hash-read DOMAIN16070+8220/
846a4a7a63b3501e1678b8c5818fd67f5978367685b3d613fbd9ce1236a8a5ec,end24290,
and re-read the stable capture, policy projection/loader, clock primitives and
transaction/recovery evidence. The full endpoint remains actual SAME consumed
transaction bytes+current+EVERY history refs, ordered owned persisted reads,
reopen/snapshots and genuine terminal lifecycle; no codec-only endpoint is chosen.

A PRECISE necessary predecessor is visible in current source, not an external
input shortage. LoadedRecordBatch::load_policies174..238 already retains the
FULL StoredPolicyValue under the exclusive transaction. But its projections
are intentionally ONLY mode/retry: complete/record_projection.rs177..193 sets
state/root retention to default and max_active=None; original.rs79..95 does
the same. Passing those snapshots to capture_lifecycle would silently capture
the wrong TTL/state/default policy. Therefore the next R0 must operate on the
retained full COMPLETE or ORIGINAL donor, not broaden/relabel the old mode
projection or construct a fake current-policy absence. This is concrete work
needed before real stored capture/publication, not another optional audit.

Actual reusable source: workflow/active/capture.rs221/47d4197e provides
LifecycleDefaults, LifecycleOverrides, CapturedLifecycle and capture_lifecycle;
policy/definition.rs preserves type/state retention; StoredPolicyValue is full
Complete(Box<...>) or Original(Vec). create/terminal command types carry no
capture. Existing TimestampMillis explicitly certifies no clock authority;
CommittedTime is committed data, not a live read clock. ports/clock.rs is still
a three-line reservation; ManualClock is test input, not an implemented port.
These distinctions prevent four shortcuts that would otherwise produce green
but source-incompatible storage tests.

#### Binding/capture input and policy-revision decision

Select creation capture from full borrowed policy data while the SAME transaction
revision is held. The storage driver will use the actually returned
StoredPolicyValue for each workflow type, not a caller substitute or a later
policy lookup. None means actual observed installed-policy absence; an operational
lookup/profile failure is never converted to None. No artificial generation is
invented for original backing: the exclusive revision/control and full retained
donor establish the read association. The capture result is ordinary mathematical
data until the real producer/consumed transaction persists it.

Reuse LifecycleDefaults/Overrides/CapturedLifecycle rather than invent another
TTL scalar or reuse ActiveRecord blindly. New accepted creations resolve
configured defaults -> type -> initial state -> explicit command in the existing
source order, retaining max-active/hot/complete-history fields as well as TTL.
The future committed VALUE command profile MUST preserve override PRESENCE and
trusted configuration/default identity plus requested time separately from its
trusted prepared/apply time. Old V1/V5/V6/V7 bytes cannot acquire new overrides by
an optional unlogged sidecar or by inspecting payload prefixes. Source-compatible
input normalization and that explicit command/profile registry must precede
admitting such public options. Existing command Vecs remain logical binaries.

CapturedLifecycle's validated new-creation TTL is positive. That does NOT license
rejecting all source restored records with absent/nonpositive capture: future
ValueRetentionKnowledge remains LegacyUnavailable versus Known(Option<ttl>),
where authenticated known absent/nonpositive source capture is Known(None),
not default TTL or unavailable. A legacy FWSR1 row with no capture field is
LegacyUnavailable, not proof of Known(None). Keep known no-expiry compatibility
outside the positive new-creation constructor. ActiveRecord's terminal-iff-
deadline guard must NOT be imported as a universal invariant. Source terminal
stamp math stays captured TTL + clamped max(requested,actual apply witness),
with no new value expiry for authenticated no-positive-capture; never outcome
TTL, present policy, caller time alone or a fabricated timeout capture.

#### Registered layout and migration decision

First value-capable layout is an EXPLICIT fresh capability, not an in-place
populated legacy upgrade. Preserve every old FWSS1..6/FWSP1..3 codec/ID/profile
and old readers exactly. Opened legacy data exposes LegacyUnavailable value
knowledge; it cannot service a new known-binding writer by treating None as
KnownEmpty. New genuine record absence under the guard can establish KnownEmpty
at birth. Populated legacy conversion is refused until a real retained replay/
backfill contract is selected; discarded history cannot be reconstructed from
current inline bytes. No range-only family acceptance or guessed family8/layout7
number is selected here. The later VALUE-DURABLE R0 must freeze one complete
key/frame/profile/origin/snapshot registry and preserve it through both policy
publication variants, capture/copy/receive/install/restore and pinned cohorts.
This is a concrete no-silent-upgrade policy, not an indefinitely open choice.

#### Authenticated read-clock decision

Value expiry uses ONE server-owned source-compatible HLC/read-time observation
per ordered batch/session, captured before row iteration under the trusted
application/storage composition. The public read request has NO read_now field.
Reuse TimestampMillis as the numerical result, not as a token proving origin;
old committed control time cannot replace current read time. Clock failure is
operational, never Missing. The future read session owns scope/control/revision
and this one time observation, so duplicate slots cannot straddle clock samples.
No clock is consulted by deterministic reducers or by recovery validation.
The future ports clock contract must be implemented by the actual server clock;
ManualClock can back an explicitly scoped test adapter but is not itself proof
of production authority. Do not expose a default method which accepts an arbitrary
caller timestamp and calls it authenticated. Strong/quorum barrier, applied
position and real routing remain separate M2/M3/application gates.

#### Expired/missing-row recovery decision: keep evidence, do not guess

Select retention of expired LOCAL generated catalog rows while any authoritative
current/history binding references them. Reads hide them using the authenticated
read clock; expiry by itself does NOT physically erase the bytes. Recovery can
therefore validate a known Versioned(local) ref against its actual row/envelope
without knowing current wall time. A missing required local generated row is
Corrupt, not silently 'probably expired'. Explicit Opaque/external refs preserve
their source availability semantics and may truthfully be missing; do not infer
local ownership merely from a parseable key spelling. LegacyUnavailable makes
no positive local-ref assertion. KnownEmpty is positively represented.

Physical explicit cleanup must remove referencing authoritative bindings/history
first (bounded phases/continuation), and delete a value only when no remaining
local binding in that cohort requires it. Old snapshots retain their OWN coherent
row cohort; live cleanup cannot destroy an anchor/received snapshot's payloads.
Do not introduce tombstones in the first profile: retaining the actual row until
reference-safe deletion supplies the needed evidence with fewer models. A later
physical TTL collector/tombstone optimization would need its own complete
receipt/evidence/retained-cohort design before source creation. This supersedes
my earlier unselected tombstone suggestion, not an implemented format change.
Unexpired or expired orphan data still requires bounded explicit lifecycle/GC;
there is no immortal-value or free disk-growth claim.

Preserve the already hash-bound source three-way distinction: passive terminal
record removal leaves old payload readable; explicit retention cleanup deletes
old payload plus new terminal value/history/state; rewind preserves refs and
rewrites SAME encoded bytes/active expiry where the real source path does so,
skipping missing/nonbinary values rather than resurrecting them. Source tests
and the actual retention helper paths must determine the precise cleanup phase/
continuation/owned-member rules. A simplistic cascade or immutable-expiry rule
is forbidden. These are acceptance obligations of the eventual lifecycle route,
not permission to replace its public/typed/rewind cases with a nonterminal test.

#### Exact next R0 creation batch: STORED-CREATION-LIFECYCLE1

Select ONLY the following FOUR paths for a later Root release after current
full validation/SAVE:
1. EXISTING crates/wf-domain/src/policy/complete.rs: private child declaration
   and reexports only (current320LF, no existing body/model edits).
2. NEW crates/wf-domain/src/policy/complete/stored_lifecycle.rs.
3. NEW crates/wf-domain/tests/stored_lifecycle/main.rs.
4. NEW crates/wf-domain/tests/stored_lifecycle/fixtures.rs.
No storage/log/command/version/frame/clock/registry/policy/ADR helper shell is
created in this R0. Existing complete-module purity applies; Root owns any actual
technical classification feedback, not a blanket policy allowance.

EXACT public models in the new child:
StoredLifecycleCaptureInput<'input> with public workflow_type:&'input[u8],
initial_state:&'input[u8], overrides:LifecycleOverrides, defaults:LifecycleDefaults.
Derive Clone/Copy/Eq/PartialEq only, matching the existing scalar inputs; no new
Debug implementation on existing types or constructor/authority token.
StoredLifecycleCaptureError, derive Debug ONLY, exactly
Policy(CompletePolicyEncodingError) and Capture(ActiveValueError).
Do not collapse operational profile/work/byte/allocation into a business capture
error, or add Display/Error/Clone just for test '?'. Existing backing types and
CapturedLifecycle are reused unchanged; no seventh value model/view is added.

ONE exact entry API (arity5):
capture_stored_creation_lifecycle(
 policy:Option<&StoredPolicyValue>, input:StoredLifecycleCaptureInput<'_>,
 schema:PolicySchemaLimits, charge:&mut PolicySchemaCharge,
 maximum_temporary_bytes:usize
)->Result<CapturedLifecycle,StoredLifecycleCaptureError>.
There is no borrowed output: after the policy/index drops, the returned Copy
capture remains valid. Complete and original full data must supply actual root/
selected-state lifecycle fields; no mode/retry projection or unrelated field
normalization may replace them. Original unsupported context/profile remains
operational Policy(...), never corrupt/absent. Caller scalar rejection remains
Capture(...). This is a creation-only resolver, not a record/retention authority.

R0 body is literally unconditional
Err(StoredLifecycleCaptureError::Policy(
 CompletePolicyEncodingError::UnsupportedPolicyProfile)).
NO reader, field scan, bounds/charge loop, source inspection, helper, default
resolution or partial positive implementation in R0. After genuine compiled
RED/SAVE, the exact private BODY read/field/capture helper roster is selected
before any additional name is created; this is not blanket parser permission.
Existing capture_lifecycle/old projection bodies remain frozen in R0.

Resource contract for later BODY: one continuous supplied PolicySchemaCharge,
matching schema profile, no refund/reset; full backing stays the caller's owner.
Before any temporary decode/index allocation, admit the checked conservative
65536+256*encoded_backing_len logical temporary envelope for Some policy, reusing
the existing original loader profile rather than allocating a second policy
snapshot/map silently. None needs no backing index. The explicit temporary cap
is distinct from source field validity; a valid original donor at cap0 refuses
before its decode. Copy only scalar lifecycle fields/return value; no retained
source borrow or output Vec/Box. Actual decoder/verifier allocation and work
must be audited against that chosen bound after BODY; it is not a native-frame,
allocator capacity/RSS/CPU or initial-R/full E/T proof. No OOM harness/probe.

EXACT five new #[test] names in main.rs, no extra cases:
stored_lifecycle_captures_complete_policy_precedence;
stored_lifecycle_captures_original_policy_without_mode_projection_loss;
stored_lifecycle_distinguishes_absence_type_and_profile;
stored_lifecycle_admits_work_and_temporary_owners_before_decode;
stored_lifecycle_capture_survives_backing_drop.
They begin with valid positive expected capture, so the callable rejector yields
genuine assertion RED; later refusal tails are honestly masked until BODY.
Goldens are literal default/type/state/command results, not expected values
computed by the new SUT or by invoking capture_lifecycle as an oracle.
Use defaults TTL7000/hot2/history10, max-hot100/max-history1000;
type max-active5000/TTL11000/history30; initial q-state TTL22000/history40;
command override6000/TTL33000/history50/hot4. Check absent-policy defaults,
type fallback for another state, command precedence, explicit active infinity,
hot0 and real clamping/source validation, wrong type/unsupported original lane,
work/cap refusal and scalar output after BOTH backing owners drop. Raw original
fixture must carry the actual full root/state fields, not a mode-only snapshot;
complete/original setup must be independently accepted by existing constructors/
readers before the first new-function assertion. No HistoryValues case mapping.

EXACT test-only fixtures: one TestResult<T=()> alias to
Result<T,Box<dyn std::error::Error>>, NO constants, and TWO functions:
input()->StoredLifecycleCaptureInput<'static>, returning the named borrowed
static request/default/override cohort; policy_pair()->TestResult<
(StoredPolicyValue,StoredPolicyValue)>, constructing real complete backing plus
its independently literal original ETF donor. Neither fixture invokes the new
SUT, computes expected outputs, returns storage success or creates an authority
model. New SUT Debug-only errors are matched explicitly. No shared fixture import,
#[path], dependency, extra type/model or fixture file is authorized. Natural
source<=400/test<=600/function<=100; if the complete draft needs another helper
or split, consult its exact responsibility BEFORE creation, never pack/drop cases.

This necessary predecessor does NOT count as the catalog endpoint. Its direct
following integration is under the existing real LoadedRecordBatch guard:
resolve each genuine birth from the retained FULL policy/default/command inputs,
carry immutable capture on the same accepted plan, and persist it with current+
EVERY intermediate history refs/catalog bytes at consumed commit. The explicit
new command/current/history/value registry, actual ordered read clock/session,
reference-safe cleanup/rewind and closed reopen/snapshot matrix must be selected
and implemented together before value capability activation. No further detached
inspection API or an invented always-positive TTL may substitute. Full typed/
shared/routed/public setup and original3 mapping wait for the actual endpoint.

#### Ordinary tenth existing-test correction while this design is prepared

Root's failed full-run report is preserved (922PASS/1FAIL,87 summaries; not
full GREEN). Actual-read current storage test public_mutation.rs535LF/18500B/
783ce6c566e5a65bfbf09d5824d42f72c5d141111408c988d2ab73a66567943d:
workspace_limits361..390=30LF; earlier missing_rejection348..349 explicitly
requires empty writes/history. Its old exact success201330580 now needs F ONLY,
consistent with the already corrected missing Complete port case. Accept Root's
same-file import size_of/Effects and local owned=201330580+size_of<Option<Effects>>;
retain all three old tuples and add (oldI,owned,oldI,true) and
(owned-1,owned,owned-1,true), exact success at owned. Preserve call-order last
'record', assert_no_publication, response[2,0,0] and committed checks; optionally
assert the already-established empty writes/history at success. No R, huge cap,
helper/type/constant/wire/fixture or new path; geometry has ample30/535 headroom.
This is ordinary Root-only test integration after terminal, not a production or
clock/capture-model change, and creates no gate on current validation.

Four blocker policies are now concrete and the first exact R0 is named. No
external user input is missing: the required full policy data exists; the gap
is the lifecycle bridge and later explicitly registered persistence integration.
No source/artifact release, family number activation, storage/expiry authority,
original3 credit, Linux/native8/full-workspace/full-goal or measured gain is
inferred. Root must adopt/release after the current checkpoint, then genuine
R0/save before BODY. Finite next-decision STOP; no acknowledgement loop.

### STORED-CREATION-LIFECYCLE1: exact type-error and variant-level temporary refinement

Finite two-point precreation clarification of322206+17067/6bc46ca5 ONLY. The
FOUR paths, TWO models, ONE literal rejecting API, FIVE test names, TWO fixtures
and one TestResult alias are unchanged. No helper/model/file/source/workload/
Git/clock/layout/profile activation was created or released.

1. SELECT the new bridge's exact reached WRONG-WORKFLOW-TYPE lane for BOTH full
   backing variants:
   StoredLifecycleCaptureError::Policy(
     CompletePolicyEncodingError::Schema(PolicySchemaError::InvalidAuthority)).
   Do NOT classify this stored-donor identity mismatch as
   Capture(ActiveValueError::WrongPolicyType). Actual complete/restoration.rs
   90..124 validates the full donor and returns InvalidAuthority at121..122;
   original projection/read111..126 returns the SAME schema diagnosis at122..123.
   The new tests supply valid other inputs and enough schema/temp allowance to
   reach this binding check. This selects its lane without reordering an earlier
   profile/work/byte refusal or freezing simultaneous bad-default precedence.
   Capture(...) remains for reached scalar/default/override capture failures.
   Existing direct capture_lifecycle's own WrongPolicyType behavior is unchanged;
   no error variant, public model or trait implementation is added.

2. SELECT ORIGINAL cap0 predecode refusal as the R0 resource oracle. Leave
   COMPLETE cap0 behavior UNASSERTED until its exact BODY temporary route is
   selected. This supersedes the earlier blanket Some-policy temporary-minimum
   wording: load.rs208..232's65536+256E profile is the ORIGINAL raw decode/index
   branch, not an existing Complete decode cost or a mandatory minimum for every
   Some backing. None requires no backing index. Complete.data() already borrows
   typed max-active/type/selected-state retention fields; full validation and
   lookup still consume schema work even if they create no temporary owner.

Source precision: capture_lifecycle126 ALREADY borrows Option<&PolicySnapshot>.
An existing correct snapshot needs no clone. CompletePolicyData is a DIFFERENT
borrowed type, so there is no direct cast; the old mode/retry projection still
erases required fields. Prefer a later explicitly selected shared scalar-core
responsibility INSIDE existing capture.rs, preserving its current public entry,
so Complete can supply borrowed identity/Copy layers and Original can pay ONE
index then supply the same layers. Do not construct duplicate snapshots/full
Complete clones merely to adapt the signature. That helper's name/visibility/
body roster is NOT authorized now; it follows genuine R0 SAVE and separate
exact BODY consultation. No new trait/model is implicitly required.

The R0 resource test must therefore exercise a VALID Original donor at cap0,
with enough work to reach the temp gate, and require the Policy byte-limit lane
before its decode. It may not demand Complete-cap0 refusal or treat zero temp
as zero validation work. Actual prospective owner/work proof remains the later
complete BODY review; returned refusal alone is no allocator instrumentation.
All other literal defaults/type/state/command/scalar-lifetime oracles remain.

I actually whole/hash-read PERF204553+3446/
e1f22c57adab751db9f6da827e156d220c5c6c048504bc2fb5551b9be3a9e0a6,
end207999, and re-read the exact restoration/original identity checks plus
capture126..144 and loader194..214. No disagreement remains. This is the exact
ordinary contract refinement Root requested, not a new design/ACK cycle or a
release of source. Current component's reported full1088/all11 result is a
separate Root checkpoint; no new feature credit follows. Finite STOP.

### STORED-CREATION-LIFECYCLE1 R0: independent stopped-source and canonical-oracle review

Actual source review under Root's release4693247+1863/442d31b0 and the exact
17067/6bc46ca5 plus3730/a288e9e8 decisions. I read ALL FOUR stopped author files,
then whole-read canonical main after Root's pinned formatter. No source/test,
shared file, Git, formatter, compiler, VM or probe was changed/run by me. Only
this allowed note is appended. No BODY helper or implementation is selected.

Actual canonical bindings (LF/bytes/SHA256):
- policy/complete.rs324/12407/
  b015a80409b2d6c9a44099637979301708398c9e98d40fe480656582ae3d8189.
- policy/complete/stored_lifecycle.rs49/1979/
  6b2c9cb3e2b4e131c21354bdb52d3f00ea27ca5ad58d78989c55a0a025c831d8.
- tests/stored_lifecycle/main.rs371/12369/
  8136ec9e807ad598d79500167e9f294179d50459a7b2cf46d459bae4a8561510.
- tests/stored_lifecycle/fixtures.rs92/3849/
  67a9ebd3e67e7a6251bc66d1b7e769dc4a55d724040133894630ea8248601246.
All paths above are under crates/wf-domain. My in-memory removal of EXACTLY the
private child declaration and three-line reexport removes150B and reproduces
ENTIRE old complete.rs320/12257/
7261aad56aea9b67f16f43e2847663fec8439d00a545b1a59a1e54ea27c98a47.
No existing parent body/model changes. Reversing ONLY main's two formatter wraps
reproduces ENTIRE author368/12339/
7d00230446573ef59144a55db50c1bd0bfaf2ab122ac0b8fc9013e3842ffe0a5.
The two wraps expand the InvalidCapturedActive pattern and other_schema call;
assertions, literals, statements and order stay exact. The other three files are
literal author bytes. Root's1601-input hold is reported separately, not a claim
that I ran its hold checker.

The child contains EXACTLY the two selected ordinary public models. Input has
borrowed workflow_type/initial_state and Copy overrides/defaults; derive is only
Clone/Copy/Eq/PartialEq. Error derives Debug only and has Policy(complete error)
and Capture(active error). The sole arity5 entry returns CapturedLifecycle by
value and is an11LF unconditional Policy(UnsupportedPolicyProfile) rejector.
There is no reader, validation/default resolution, charge mutation, hidden
partial positive, private helper, new constant, output owner or authority token.
The parent exports precisely these three names behind its private child.

All five selected case identities are exact:
stored_lifecycle_captures_complete_policy_precedence;
stored_lifecycle_captures_original_policy_without_mode_projection_loss;
stored_lifecycle_distinguishes_absence_type_and_profile;
stored_lifecycle_admits_work_and_temporary_owners_before_decode;
stored_lifecycle_capture_survives_backing_drop.
Canonical spans are15..84=70LF,87..176=90LF,179..244=66LF,247..325=79LF,
328..371=44LF. Fixtures input14..31=18LF and policy_pair33..92=60LF; API39..49=11LF.
Thus EXACTLY eight new functions, maximum90LF, source49/test371+92 within natural
limits, no packed code or waived geometry. One TestResult alias, two fixture
functions, no test helpers/constants/extra case or fixture artifact are present.

Every first intended API assertion follows policy_pair's independent premises:
Complete data is restored at generation7, genuinely encoded, then verified by
existing complete APIs; generation/type/max-active are checked. Original bytes
are independently literal, portable-decoded with an existing reader, then checked
for generation7/type r/exact retained encoding before returning the two owners.
Neither fixture calls the new SUT or computes expected captures. I separately
examined the184 literal bytes (SHA256
6a94f5971f3d7725f5afa90b6c9355cf9775268c6f6dcf21bc7423dd72567cfd)
as source data, without invoking Rust or a VM: exact ETF envelope
{flow_policy_v1,7,map}, root type r/max_active5000/type ttl11000/history30,
q-state ttl22000/history40, exact EOF and no duplicate map keys. This is an
independent literal audit, not a newly executed upstream golden or product test.

The cases use literal scalar goldens: q-state versus other-state type fallback,
command active6000/ttl33000/hot4/history50, Original infinity/hot0 and configured
clamp3/25, absent-policy defaults None/7000/2/10. These catch the older mode/retry
projection's erased lifecycle fields. Wrong type for BOTH full donors expects
Policy(Schema(InvalidAuthority)); unsupported Original is independently checked
with the old portable reader before the new API assertion. Reached bad active,
retention/default values use Capture origin. No simultaneous invalid-input
precedence is invented. The selected later refusal tails remain source oracles,
not runtime evidence from this source read.

Resource/lifetime source is also faithful: valid Original-only cap0 expects
Policy(ByteLimit), with sufficient work and an independently accepted donor;
no Complete-cap0 behavior is asserted. A funded Original call is bracketed by
one continuous account and followed by refused-cap/retry checks; Work1 and profile
mismatch expect unchanged prospective counters. These are refusal/continuity
oracles, not allocator instrumentation or a complete future work census. Output
captures cross a block that drops BOTH full backing owners and its charge; the
outside four-scalar checks are nonvacuous only after both earlier is_ok guards.
No source borrow is returned. OS-neutral scalar/standard collections and byte
literals add no filesystem, native or target-specific dependency; no Linux run
or universal resource theorem follows.

Scoped source acceptance: NO concrete model/API/oracle/ownership/geometry defect.
I communicated actual findings directly to Root, DOMAIN and PERF. First intended
positive assertions are canonical53/135/184/263/340. Root compilation/runtime
and PERF's written source receipt are still separate pending evidence at this
append; do NOT infer genuine five-assertion RED, reached later tails or BODY
permission from the source shape. Original HistoryValues mappings, catalog,
consumed publication/read/recovery/retention, Linux/native8/full-goal remain held.
Ordinary actual runtime and written-peer reads will close this bounded review;
no readiness acknowledgement or extra source-acceptance gate is requested.

### STORED-CREATION-LIFECYCLE1 R0: technical correction, actual five-assertion RED, reciprocal closure

This closes the source receipt343003+6138/845df849 with actual corrected source,
Root execution and the written peer read. I missed the new integration target's
required explicit forbid(unsafe_code) declaration during that first source pass.
Its broad no-defect conclusion is superseded for this exact technical omission;
the model/oracle/ownership findings remain sound. No check was suppressed or
weakened. Root added ONLY the required target attribute after the module doc.

I whole/hash-read the actual first architecture failure at validation-log
9482719+5968/
8bf703e96ce4c4d8c9ad5b0f8e0e5d18c100069efdb2966226840099fc316bab:
52PASS1FAIL, solely workspace's missing-target-attribute diagnostic. Root reports
fmt/domain all-target check/strict Clippy had passed and later technical commands
had not yet run. NO behavioral case ran in that attempt; it is not the TDD RED.
Current main is373LF/12394B/
da5ef001a9080cdd7a8793b62cd57af5a42950c18801a3552665453bf3cc2cbf.
My removal of ONLY its25B `#![forbid(unsafe_code)]` block reproduces ENTIRE
canonical371/12369/8136ec9e807ad598d79500167e9f294179d50459a7b2cf46d459bae4a8561510.
Together with my prior exact formatter inverse this also binds the original
368/7d002 author assertions. The other three production/fixture identities and
150B entire-parent inverse in the source receipt stay literal. Test spans merely
shift two lines; maximum90 and exact eight-function/five-test census unchanged.

The corrected seven-command technical sequence27360 is Root-reported terminal0,
all1605 inputs unchanged/8a6c079e: fmt, domain all-target check, strict Clippy,
arch53, reference480, progress120/132/550 and diff pass. I independently
whole/hash-read its corrected architecture output9489935+5593/
1d5cd34c7d74a60599e606c840a8541efb1d6b08db0d466c2e1e36deb4553d5f:
45unit+4archunit+3visibility+1workspace=53PASS, zero other statuses. Other six
command outcomes/hold are attributed to Root; I did not run a workload or pretend
the architecture slice contains the entire seven-command packet.

I then whole/hash-read ACTUAL compiled R0 target9497313+2163/
31ee2dcfd1e9e44d2449392f7eaa09f119d57c39e6b29b2b670013b3ee2421e7.
It compiles the stored_lifecycle target and reports EXACTLY0PASS5FAIL,
0ignored/measured/filtered. My independent Counter of all five source names
matches the runtime's five unique FAILED statuses exactly. Every panic is the
intended Some(Policy(UnsupportedPolicyProfile)) from the literal rejecting API:
Complete precedence main55; Original full-field precedence137; actual absence186;
work/temp265; backing-drop342. Full Complete restore/prepare/verify and literal
Original reader premises therefore complete in EACH case before its intended
first-positive failure; the work case's extra original-reader prerequisite also
finishes. There is no setup, missing symbol, compiler, doc, lint or technical
failure masquerading as these five runtime assertions. Root reports session19521
terminal101 with the same1605/8a6c inputs unchanged.

Precise limits of that execution: no positive scalar golden is yet compared;
remaining type/state/command iterations, error-origin/refusal/profile/retry tails
and outside-block scalar checks are MASKED by each first-positive assertion.
The backing-drop case fails at its first Complete capture, so the second Original
capture and actual drop/use tail do not execute; its lifetime shape is compiled,
not an executed successful ownership demonstration. The current rejecting body
does no capture/work/decode; these tests are future BODY obligations rather than
proof of allocator limits, a continuous work tariff or completed lifecycle.

Ordinary written reciprocity is ACTUAL: I whole/hash-read PERF212717+6477/
96d831cddac49c66f1bfcba9cba1cda76ad7d4e3b534fc3f679a9562b91a4501,end219194.
It independently binds all four sources/full parent inverse/literal184 donor,
exact oracles/resource qualifications and natural geometry. I agree; no source,
semantic, lifetime or resource disagreement remains. PERF actually read my6138
source receipt and directly agreed. Both acknowledged the target-attribute
review omission and exact technical correction; actual runtime findings were
communicated directly. No extra appendix or acknowledgement is needed for this
reciprocity. Any later peer runtime receipt may be read ordinarily without a
new source/SAVE gate.

Scoped acceptance: corrected callable R0 and genuine five assertion RED are
established; Root may record/save that checkpoint under its custody. NO BODY
helper/model/field-reader roster is selected or source released here. The later
full-donor bridge must still use actual full typed/Original fields with continuous
admitted work and the selected variant-level temporary contract. No original
HistoryValues counterpart, catalog/publication/read/recovery/retention/clock,
Linux/native8/full-goal or measured resource gain follows. Only this allowed note
was appended; no source/tests/shared artifact/Git/build/VM/probe mutation or
execution by me. Finite current review STOP.

### STORED-CREATION-LIFECYCLE1 BODY: exact three-existing-path selection after saved RED

Root saved the genuine five-assertion baseline in
c2d9ceb36f644953bac953536f49564b4ac245fd and verified clean. I actually read the
latest coordination tail4695110+4633/09fa75cd and the unchanged source, the
attributed author proposal DOMAIN27498+2858/
4e9527757e5ee2a039531cacf6b38f4d136fea28c2ad88456ca7f5b299c2b168,end30356,
and PERF224143+9068/
da0a4ac5868e6192eb3d339974cfb47787dbf32c8a43203faa1b5a86c6ce329b,end233211.
I also whole/hash-read final R0 PERF219194+4949/1e262a99; its actual five RED,
whole corrected technical7 and honest masking agree with my closing receipt.
This is an EXACT precreation decision for later Root adoption/release, not code
creation or permission for an agent to start independently.

SELECT ONLY THREE EXISTING production paths, all under crates/wf-domain/src:
1. workflow/active/capture.rs, current221LF/8111B/
   47d4197e0c3a0fc92e8d5b5c41c26d07efeb0e996d9ae82efef79c6ba67e3061.
2. workflow/active.rs, current232LF/8278B/
   b1c22cc872999d1e25f4306560115a71798ce9dc7f846040c8f57a3053181488.
3. policy/complete/stored_lifecycle.rs, current49LF/1979B/
   6b2c9cb3e2b4e131c21354bdb52d3f00ea27ca5ad58d78989c55a0a025c831d8.
NO new file, nominal model/type/trait, dependency, constant, test/helper/fixture,
public API, policy/ADR/clock/command/storage/layout/profile activation. Existing
complete.rs parent/reexports and original decoder/normalizer files stay literal.
The TWO test files are ENTIRE frozen main373/da5ef001a9080cdd7a8793b62cd57af5a42950c18801a3552665453bf3cc2cbf and fixtures92/
67a9ebd3e67e7a6251bc66d1b7e769dc4a55d724040133894630ea8248601246.
Existing two public models/traits/error variants and the arity5 entry signature
remain exact. Author owns only the three paths after explicit Root release.

#### Exact shared scalar core and old entry compatibility

ONE NEW crate-private function in capture.rs, arity7:
capture_lifecycle_layers(
 policy_type:Option<&[u8]>, workflow_type:&[u8], max_active_ms:Option<u64>,
 type_retention:RetentionOverride, state_retention:Option<RetentionOverride>,
 overrides:LifecycleOverrides, defaults:LifecycleDefaults
)->Result<CapturedLifecycle,ActiveValueError>.
Define it pub(crate); active.rs adds ONLY its pub(crate) reexport. It is ordinary
scalar resolution, not policy admission or transaction authority. No public
facade export or generic callback/trait is added.

The existing public capture_lifecycle/5 keeps its signature and returned-error
order. Its wrapper gathers one infallible borrowed type/state layer and delegates;
no new map, snapshot, clone or fallible reader is inserted there. The shared core
performs validate_defaults -> optional identity -> active -> command-retention
validation/hot crosscheck -> defaults/type/state/command merge -> existing clamps
and result construction. Its pure borrowed state gather can happen before these
checks without changing the returned diagnoses; that does not claim identical
CPU/lookup timing. The existing validate_defaults body/semantics stay exact.
Generalize ONLY the existing private resolve_active/2 to
(max_active_ms:Option<u64>,command:MaxActiveOverride), and
resolve_captured_retention/4 to
(type_retention:RetentionOverride,state_retention:Option<RetentionOverride>,
 command:LifecycleOverrides,defaults:LifecycleDefaults), retaining their result
and validation/merge/clamp math. No second scalar engine or additional named
helper is authorized. This is one shared core plus the existing two workers.

#### Exact stored BODY/helper roster

Replace ONLY the existing public capture_stored_creation_lifecycle/5 rejector
body/docs and needed imports. Add EXACTLY FOUR private functions in the SAME
stored_lifecycle.rs, no helper child or type alias:
- read_original/5(raw:&[u8],input:StoredLifecycleCaptureInput<'_>,
  schema:PolicySchemaLimits,charge:&mut PolicySchemaCharge,max_temp:usize)
  ->Result<(Option<u64>,RetentionOverride,Option<RetentionOverride>),
           CompletePolicyEncodingError>.
- field/5(index:&OriginalPolicyCurrentIndex<'_, '_>,map:TermPosition,
  name:&[u8],atom_key:bool,charge:&mut PolicySchemaCharge)
  ->Result<Option<TermPosition>,CompletePolicyEncodingError>.
- retention/3(index:&OriginalPolicyCurrentIndex<'_, '_>,
  position:Option<TermPosition>,charge:&mut PolicySchemaCharge)
  ->Result<RetentionOverride,CompletePolicyEncodingError>.
- number/3(index:&OriginalPolicyCurrentIndex<'_, '_>,position:TermPosition,
  charge:&mut PolicySchemaCharge)
  ->Result<Option<u64>,CompletePolicyEncodingError>.
Thus FIVE NEW helper functions TOTAL (one shared plus four private), not five
private stored helpers. Existing type import renames such as Index/Error/Cursor
may refer ONLY to the already-defined index/error/cursor; no new typedef/model.
Reuse existing decode_portable_retained, TermPosition/current cursors,
charge_schema, flow_input::value::unsigned and existing retention constants.
No new read accessor or visibility widening of original modules is necessary.

The public bridge checks charge.profile()==schema FIRST; mismatch returns
Policy(Schema(ProfileMismatch)) without advancing counters. It then prepays the
fixed scalar/wrapper cohort below before touching policy branch/scalar inputs.
None uses no backing index and resolves actual defaults. Complete calls EXISTING
validate_stored_complete_policy on the SAME account, preserving full current
content/type/generation/profile validation. It then borrows data() and Copy
max-active/type retention, with a prospectively paid linear exact-binary state
lookup. Every reached row and both byte operands are admitted; old validation's
name charge does not pay this second lookup. No unpriced BTreeMap.get shortcut.
The found state's optional retention is copied, preserving empty/absent fallback.
No Complete snapshot, map, backing clone or second validation projection.

Original read_original performs ONE actual retained portable decode and ONE
catalog, with type identity checked while the index is live. Ok(None) from the
strict reader becomes Policy(Malformed), never policy absence/default. Existing
unsupported context/profile, allocation, work and checked-arithmetic diagnoses
propagate through Policy. Build the catalog before using normalized cursors:
CurrentCursor::new resolves valid zero-head LIST aliases through that catalog.
The earlier no-catalog idea is explicitly rejected. Then read only atom-root
max_active_ms/retention/states, binary-selected state, atom config retention and
atom retention scalar labels. Return only the Copy scalar triple; index/catalog
drop naturally before the shared core. No second decode, projection, snapshot,
map construction or retained output borrow occurs. After genuine Original type
equality, the caller may use Some(input.workflow_type) for the core's repeated
identity check; it is another reached comparison and its two byte sides are paid.
Complete can pass its actual borrowed type. None passes no policy type.

field/5 is a narrowly scoped PAID LINEAR map lookup using retained positions.
For atom_key=true, compare ONLY atom keys; for false, compare ONLY binary keys.
Do not use key_bytes() to merge classes, reinterpret :q as <<q>>, or rely on a
mixed-key ordering shortcut. Root/state/retention unknown keys stay unselected.
The existing strict reader already proves map identity; no new dedup/canonical
map owner is created. Missing/nonmap containers follow the source fallback below.

#### Source-exact fallback versus explicit supported-profile refusal

I actually read baseline retry_policy.ex995LF/32083B/
f6392af3042c6a13f6e1b4af363eba801df88362bbf47479205d6e235ce2e1a0,
lines210..240 and746..769. This corrects the earlier generic retention-normalizer
suggestion: public borrow_retention accepts keyword/binary aliases and forbids
hot; it is NOT the stored resolver's atom-only map merge. Do NOT substitute it.
Source max-active is integer-only: absent OR any noninteger portable term falls
toNone, including nil/binary infinity. number/3 checks Integer kind first;
noninteger ->Ok(None), without numeric interpretation or scanning its subtree.
For an Integer, call existing paid unsigned(Current,u64::MAX,same charge), keeping
padded arbitrary integer encodings representing u64. An unrepresentable/negative
nonzero integer is UnsupportedPolicyProfile, not silently absent. The caller
requires selected max-active integer1..MAX_RETENTION_MILLIS; outside this positive
captured profile is explicit Policy(UnsupportedPolicyProfile). This does not
pretend every arbitrary raw source integer is representable by CapturedLifecycle.

Absent/nonmap root retention, states or state config/retention preserves source
fallback; a nonmap layer is NOT malformed merely for being nonmap. An actual
retention MAP selects ONLY atom ttl_ms/history_max_events. Their PRESENT selected
values must yield Some(number) and satisfy positive TTL/source history bounds;
otherwise Policy(UnsupportedPolicyProfile), not Capture or absent fallback.
Unknown/binary-name labels do not become atom fields. Source also merges internal
atom history_hot_max_events, which RetentionOverride cannot carry: SELECT an
EXPLICIT UnsupportedPolicyProfile lane whenever that atom key is present in a
reached root/selected-state retention map. Binary-only hot is ignored by source.
Do not silently discard atom hot, fabricate a new scalar model or advertise full
arbitrary-Original compatibility. An unknown atom/context that the existing
portable decoder cannot admit retains its existing operational refusal. This
source-compatible common-profile bridge plus visible unsupported lanes earns no
original workflow mapping; broader retained raw/internal-hot support stays an
explicit later compatibility obligation if required by the full endpoint.

Stored-donor type mismatch is exactly Policy(Schema(InvalidAuthority)) for BOTH
variants, after actual donor validation. Invalid reached defaults/command scalars
from the shared engine stay Capture(existing ActiveValueError); do not recast
unrepresentable stored scalars as user Capture errors. Operational stored
validation precedes the core on this NEW entry; simultaneous invalid stored and
caller inputs have not acquired an invented cross-source priority. Old direct
capture_lifecycle retains its own defaults-before-identity return order.

#### Exact prospective resource tariff and owner boundary

Use ONE supplied continuous PolicySchemaCharge; no new/reset/refund account.
All following are actual prospective logical G/P debits, not native CPU or free
permission from a forecast. Select literal fixed allowances, NO new constants:
- Public fixed scalar+wrapper G=16_384 AFTER profile match and BEFORE policy
  branching/argument extraction. It reserves128 events at128: current core table
  defaults/validation40,identity/control2,active8,command retention12,hot crosscheck8,
  result defaults4,optional merges18,history clamp4,hot/conversion12,result fields4
  totals112; at most16 additional bounded wrapper selection/copy/control events.
  No data-dependent row, byte or magnitude loop is included. This supersedes the
  author's earlier tentative32_768, not a change to a frozen runtime oracle.
- read_original prepays fixed4096 (32 nonloop metadata/cap/layer/dispatch events)
  BEFORE its body; retention prepays2048 (16 nonloop Option/class/validate/result
  events) BEFORE its body. These bounded cohorts exclude helper/row/byte loops.
- field entry prepays128 before map-kind/length inspection. Each reached map row
  or typed state row prepays384 for the iteration/key/value visit BEFORE row/key/
  value access. Before each actual equality, additionally pay checked
  128+left.len()+right.len() for BOTH operands. Failed class matches do not compare
  unrelated bytes. Loop termination/presence/control stays in the named bounded
  helper/header cohort; there is no unbounded uncharged iteration.
- number entry prepays128 before cursor/kind inspection. ONLY a reached Integer
  additionally pays its WHOLE selected encoded scalar length BEFORE unsigned;
  existing unsigned magnitude-scan fees are additional, same account. This pays
  initial bounded integer decoding as well as padded fallback, without turning
  a noninteger ignored subtree into a numeric parse.
- The shared core's repeated identity also pays both actual byte operands plus
  its128 comparison visit BEFORE invocation when policy identity is present.
  Original's first genuine donor identity comparison is separately paid likewise.
All usize sums/products/u64 conversions are checked; overflow is
Policy(Schema(LengthOverflow)). Failed charge preserves its admitted prefix;
work exhaustion stays Policy(Schema(WorkBytes)), no error is funded afterwards.
These are explicit upper cohorts, NOT one unit for every machine instruction.
Actual stopped code must be recounted against112+16/32/16 and every dynamic site;
a new loop/extra path cannot borrow unnamed slack or rename an unpaid visit.

Original temporary permission: E=FULL borrowed encoded backing length. Compute
checked65536+256E and compare max_temp BEFORE decoder allocation; insufficient
cap is Policy(ByteLimit), including valid Original at cap0. That ceiling alone
issues no G credit and is separate from the work account. Reuse the existing
strict decoder's prospective actual reservation/G/P debits. For its ONE catalog,
pass conservative live=192E+128 (checked) so reserved n/k/f capacity, including
portable reserved f with virtual_funs.len()==0, is not lost by a length-only
owner_bound. Existing catalog admission adds at most8n+128, so with n,k,f<=E the
logical requested coexistence is <=200E+256 <65536+256E. No extra external P
forecast is imposed as an invented issued credit; build_catalog itself debits
its prospective peak before reserve. No scalar retention scratch owner is built.
Peak P remains charged after temporary drop; genuine allocator failures from the
existing decoder/catalog remain Policy(AllocationFailure), not Missing/absence.

Complete creates NO new temporary collection/index on its selected canonical=false
validation and borrowed-scalar lookup route. Its max_temp0 therefore need not
fail; sufficient work/current validation still apply. That is a real route
property, not a new Complete-cap0 test assertion or a claim its retained caller
Box/Vec backing is free. Original's cap covers the selected logical requested
owners, not Vec overprovision, allocator metadata, raw backing capacity, recursive
parser/key-comparison call stack, RSS/CPU or native initial-R/fullE-T. No Box/shrink,
allocation-failure harness, benchmark/speed claim or OS-specific API is selected.

#### Geometry, peer closure and exact release boundary

Author's natural estimates are stored<350/capture<280/active233 and functions<100;
these are planning estimates, not proof or a size waiver. Root formatter/actual
whole-source review must establish source<=400/function<=100. If the five-helper
cut cannot naturally fit, consult the exact additional responsibility BEFORE
creating it; do not pack/delete comments/drop cases. Imports/docs/body changes
are confined to the three paths and named workers. All other model/API/source,
the entire two frozen tests, old direct capture scenarios and mandatory original
acceptance obligations remain. Later validation must run unchanged-five GREEN
and old regression/full workspace; macOS evidence does not satisfy Linux.

Direct author and PERF consultation resolved catalog necessity, key classes,
noninteger fallback, explicit hot/range profile lane, shared-core identity debt
and the requested-owner bound. My actual written9068 PERF/2858 DOMAIN reads
agree; PERF directly checked the final concrete fixed/dynamic table subject to
actual stopped-code census. It will read this recorded decision ordinarily;
no additional appendix/readiness acknowledgement or design gate is requested.
No source/workload/Git/shared mutation by me. Root must whole-read/adopt/release
this EXACT three-path batch before sole authorship; no one may infer permission
from this note alone. The real catalog/current+EVERYhistory/consumed transaction,
authentic retention/ordered read/recovery/profile migration/full typed hydration,
original3/Linux/native8 and ordered full-goal endpoint remain required. Finite
BODY precreation decision STOP.

### STORED-CREATION-LIFECYCLE1 actual BODY A/B review and retention-cohort correction

Actual independent written-source review AFTER all three author buffers STOPPED.
I read the adopted release4699743+2905/cb6142fa, actually whole-read all three
author files, then whole-read the canonical stored file and its one-fee B revision.
Only this allowed architecture note is written by me; no source/tests/shared
artifact/Git/compiler/formatter/VM/probe workload was changed or run by me.
This reviews the implemented exact three-path batch after saved REDc2d9ceb,
not another catalog/clock/layout or public authority decision.

Canonical CURRENT source B (all paths under crates/wf-domain/src):
- workflow/active/capture.rs243LF/8909B/
  a18bf68a32065cbf7a769c3115e743993cf00f2f6028e5803e1ca52a7287637f.
- workflow/active.rs233LF/8328B/
  f3143f2b21ed9b450ea7d4582aa1f2ae23460cf52cf3a46a0ccd35b9c086aad2.
- policy/complete/stored_lifecycle.rs291LF/11044B/
  f70f427c0a4cca7be44ca7832d0907ea90e8e4604b5051e940a34f707a718b2d.
Root's formatter changed ONLY stored imports/order and rows.next error wrapping
from author289/10983/b4c2c32a2fbe5a7adbf196cb7af4d39dd5ae320afac0ab4a293a93e071643491
to source A291/11044/477c239ddec1362cf261fa4292b575140337523460c648af83070be808a692e4.
My exact two-part in-memory inverse recovers the ENTIRE author file, not just a
whitespace-insensitive comparison. Capture/active stay literal author bytes.

My independent whole scoped inverses reconstruct ALL THREE old sources exactly:
remove only the shared core and restore the selected old public/scalar worker
bodies -> capture221/8111/47d4197e0c3a0fc92e8d5b5c41c26d07efeb0e996d9ae82efef79c6ba67e3061;
remove only the50B crate reexport -> active232/8278/
b1c22cc872999d1e25f4306560115a71798ce9dc7f846040c8f57a3053181488;
retain the byte-identical public model block and reverse the allowed imports/
public docs/body/private additions -> literal R0 child49/1979/
6b2c9cb3e2b4e131c21354bdb52d3f00ea27ca5ad58d78989c55a0a025c831d8.
The B-to-A inverse below composes with that last proof. No hidden old helper,
model/trait/error variant/public signature or unselected body edit remains.

Entire frozen tests STILL hash to main373/12394/
da5ef001a9080cdd7a8793b62cd57af5a42950c18801a3552665453bf3cc2cbf and fixtures92/3849/
67a9ebd3e67e7a6251bc66d1b7e769dc4a55d724040133894630ea8248601246.
Complete parent324/b015a804 is also byte-exact. Root's full held1602/b89c proof
is attributed; I independently bound these full test/model/source inverses.

Exactly the selected FIVE new helpers exist: crate-only layers7 and private
read_original5/field5/retention3/number3. Twenty-four functions TOTAL across the
three files, including the unchanged generic expire_active_batch. New core
147..169=23LF; stored public49..135=87,read137..198=62,field200..234=35,
retention236..270=35,number272..291=20. All files<=400, all functions<=100,
maximum file291/function87; ordinary formatting, no packing or waiver. Existing
old capture wrapper126..142 and generalized active189..203/retention205..243
are the selected existing workers. No new file/nominal type/alias/trait/constant,
view/accessor/extra helper/test or broader public reexport is present.

#### Actual semantics and ownership

The old public capture wrapper gathers one pure borrowed layer and delegates;
inside the shared core defaults fail first, then identity, active and retention/
hot crosschecks, followed by the SAME defaults/type/state/command merges and
clamps. My old-source inverse proves validate_defaults and all remaining old
math unchanged. Borrowed state gathering can precede these checks but cannot
allocate/fail/change a returned diagnosis. I also read the stable unchanged
active_timeout/capture.rs283/8605/c797f204bc0054f34fb7699effcf74e585efe38271b0988050d6e84163ef7769:
existing infinity/live-policy, layering/caps/numeric-extreme and restore controls
remain. Their actual regression execution still requires Root's full run.

The stored entry FIRST checks schema/profile, then once prepays16_384 before
policy branching. None makes no backing index. Complete calls the existing FULL
current validator on the same charge, borrows data(), then prepays384 BEFORE
explicit iterator.next (stored72 versus75), and both name/request byte lengths
plus128 BEFORE equality89. It copies only actual max-active/type/selected-state
retention. There is no snapshot/Complete clone/map builder or new collection on
this canonical=false path; a zero temporary cap is not an invented failure.
The retained caller Box/Vec still belongs to the outer owner and is not free.

Original prepays4096, checks full E's65536+256E cap BEFORE the ONE strict decoder
call153, genuinely compares stored type with paid BOTH bytes before163, then
builds ONE catalog at171 with checked live192E+128. Valid zero-head LIST aliases
therefore remain supported. Catalog/index never escape: read returns only three
Copy scalars and drops them before the shared core. The repeated core identity
is separately charged122 before invocation125; Original's Some(input.workflow_type)
is used ONLY after actual full-donor equality succeeded. No fresh charge, second
decode/projection/snapshot/map or borrowed output is hidden in the route.

field's128 precedes kind/length inspection,384 precedes row/key/value access214,
and128+both actual byte operands precedes equality228. Atom labels are selected
ONLY by atom_bytes, binary state ONLY by binary_bytes: neither :q nor binary
retention aliases are accidentally treated as an atom field. Nonmap/missing
layers follow source fallback; noninteger max-active returnsNone before numeric
interpretation. Reached Integers prepay their full resolved encoded span before
unsigned; padded integer encodings retain its existing paid magnitude path.
Known unrepresentable/negative/range-invalid integer and present invalid retention
values preserve the selected operational UnsupportedPolicyProfile. Reached
internal atom hot is explicit unsupported, binary-only hot unselected. Root/
selected-state unknown fields are ignored without erasing operational decoder
context failures. These are the documented common profile and visible unsupported
lanes, not complete arbitrary-Original or internal-hot parity.

Stored type errors are Policy(Schema(InvalidAuthority)); malformed/unsupported,
work/length/allocation/byte-cap errors retain Policy origin. Reached invalid
caller/default scalars retain Capture. Matching-profile checks leave refused
accounts unchanged and no error refunds an admitted prefix. New stored validation
precedes the scalar engine as selected; old direct capture returned-error order
is preserved. No field/capture is a persistence/namespace/clock/admission token.
Scalar output has no input lifetime or collection owner; both backing drops and
post-drop uses are tested later by the frozen case, not justified by Copy alone.

#### Concrete resource defect found and exact existing-literal repair

Our PRECREATION16-event retention allowance was too small: result.validate calls
the real RetentionOverride validator, including its own Option/range/control
visits. Those visits cannot be paid by the DIFFERENT later shared core's112
allowance. Root flagged this, I explicitly agreed the existing-literal increase,
and PERF independently confirmed a conservative30-event bound:
header/class/default-return4 + TTL option/number-result/capture/propagation4 +
hot presence/refusal2 + history option/number-result/u32/capture/propagation5 +
result fields/return3 + FULL validator/propagation12 =30. The validator component
includes2Options,4zero/range comparisons,min bound,2OR controls,2if controls and
result propagation. Field/number helper internals and dynamic bytes stay paid
additionally. This is a logical semantic-visit/control table, not per-machine
instruction pricing or permission to regroup an unpaid path away.

SELECT/accept ONLY retention's EXISTING entry literal2048->4096 (32*128), as
already recorded/applied by Root after A's focused run terminal. No new helper,
model/constant/test/byte ceiling or ownership/branch change. I whole-read actual
Bf70f, and reversing ONLY that statement INSIDE fn retention reproduces ENTIRE
A477c. read_original's separate4096 stays unchanged; a global4096 inverse would
be the wrong reconstruction. A's passing tests did NOT close this undercharge.
The B change preserves full geometry, old-source inverses/models/oracles and
all branch outcomes; corrected B execution still needs its own actual packet.

Other actual closed fixed bounds, reviewed with PERF against the source:
- Shared core112 plus wrapper<=16 fits once16_384; the wrapper slots are
  dispatch/data/local/iterator+len5,typed type/active/retention3,policy-presence1,
  identity metadata/dispatch3,core input copies3,origin conversion1. The matched
  retention read is in its separately paid row value visit. No variable scan is
  hidden in those slots.
- read_original<=32:cap/arith/refusal5,decoded presence/entry3,identity metadata4,
  live calculation4,root cursor2,active Option/range4,type-retention binding1,
  states/selected dispatch3,triple3,bounded call/propagation3. Decoder/catalog,
  helpers and comparison/magnitude bytes pay their separate prospectives.
- Retention NOW30<=32. The actual row/byte/number ordering sites above have no
  remaining distinct reached unpaid event under this selected logical table.
All account debits remain continuous; an attempted debit advances nothing on
refusal. Requested owner n/k/f<=E gives decoder64(n+k+f)<=192E; catalog adds
<=8E+128 to live192E+128, so<=200E+256 fits65536+256E. Reserved function capacity
is included even if final virtual-fun length is0. The permission bound is NOT
issued G credit; actual decoder/catalog admission does the work/peak debit before
reserve. No allocator capacity/RSS, raw backing capacity, recursion/native-frame,
CPU/initial-R/fullE-T or total-allocation theorem follows. No shrink/Box/probe.

#### Actual A runtime and current acceptance boundary

I whole/hash-read saved R0 target9497313+2163/31ee2dcf and actual A target
9500855+730/a5104a65d1faa94eecb180e1451b76ba9f870ab49b03941d591f3c3961ff2e05.
My source/R0/GREEN Counters preserve EXACT five names and change only five
FAILED to ok, same stored_lifecycle-a438ac334992b1ef executable identity. Actual
A result5PASS/zero failed,ignored,measured,filtered means ALL previously masked
literal/precedence/error/refusal/retry and both-owner drop/use tails finish.
It does not execute additional untested raw-profile permutations or prove the
resource tariff on its own. No oracle or test bytes changed after genuine RED.

Root reports initial A focused27849 all eight commands0/unchanged1605/ba7a:
fmt/check/strictClippy/target5/arch53/reference480/progress120132550/diff.
I independently whole/hash-read actual A architecture9501766+5593/
f177a9dd850187bc13ffa23d0f9fdd2d327400ef9ac83d1c8e1f91713bd798f2,
53PASS/zero others and exact same architecture name/status Counter as corrected
R0. Other six command/whole hold outcomes are Root-attributed here. This SOURCE A
GREEN remains separate from the now-corrected B fee and its pending full evidence.

I actually read Root's attributed source/correction DOMAIN32353+2323/
028f7ec2e017f4228f671f183866b850c22942ad1937b37a795558c875151035,end34676.
It agrees on all source identities/inverses/geometry, A correctness and the
independently necessary tariff correction. Prior actual PERF3909 precreation
closure was read at this review's start; current actual-source census findings
were exchanged directly and resolved above. PERF's written BODY receipt and
corrected B result will be read ordinarily for final reciprocal/runtime closure,
without an extra readiness/SAVE gate. No remaining scoped B semantic/ownership/
resource/geometry defect is identified, subject to those actual checks. This is
NOT full workspace/old public regression, Linux, original3 mapping, genuine
catalog/current+EVERYhistory/consumed transaction/retention/read/recovery/typed
profile or full-goal completion. Only this role note changed by me.

### STORED-CREATION-LIFECYCLE1 final B focused-result and written-peer closure

Closes the actual source/fee receipt370628+12087/01456482. I independently read
and parsed the COMPLETE corrected B focused packet in the existing append-only
validation log:9508870+9377/
3861bdcb1b98973ad61f92703448f95a53fae1525812facce9fb3ade414be6b5,
end9518247 (starts at ROOT header, includes the final blank LF). Exact numbered
headers1..8, eight EXIT0/UNCHANGEDTrue footers, and all eight INPUTS headers are
1605/a6b31162bddc1dd9ea8e6f96cbf43c7ab70be7df602c4c26597972b399fe0223.
Thus the eight actual commands on corrected bytes pass: fmt, domain all-target
check, strict Clippy, target5, arch53, reference480, progress120/132/550, diff.
This is my own full-packet read, superseding pending/Root-only attribution for
CURRENT B in the prior receipt. No workload was executed by me.

I separately whole/hash-read final target9510184+730/
ff5f20451242870aad44918eac16496a83a6d807cda9b2afee663efb75d0efac:
5PASS,0failed/ignored/measured/filtered. My comparison with saved R02163/31ee2dcf
and all five frozen source identities preserves EXACT names and changes only
all five FAILED->ok, same a438ac334992b1ef artifact. I separately read B arch
9511107+5593/add4e86d753c2e92568da72384b68954ec6abd209e22061fafee2f8ea22065fc;
its53PASS name/status Counter equals A and corrected R0. No compilation/technical
failure is substituted for behavioral evidence; both entire frozen tests are
unchanged after the genuine RED/save. Every former fixed-test tail now finishes,
including both full-owner drops and outside-block scalar use. This does not
exercise unasserted raw-policy permutations or instrument allocator behavior.

Final actual source rebindings remain capture243/8909/a18bf68a,active233/8328/
f3143f2b,stored291/11044/f70f427c; the whole hashes/inverses are in the primary
receipt and were rechecked. Models/public signatures/frozen tests stay literal.
Retention's32-event4096 fee includes its FULL nested validator:30<=32. The old
16-event design underestimate is explicitly corrected, not redefined as a free
callee or charged to the later shared core. Other fixed112+16/32 and dynamic
row/byte/number sites remain as actually reviewed. No remaining scoped source,
semantic, ownership, resource or natural-geometry defect is identified.

Written reciprocal review is ACTUAL. I whole/hash-read PERF237120+11017/
ede4c0ef5daf9560d2684cb7ef1d9411643a4c11aefc869bed1fea57291b0148,
end248137. It independently binds all three full sources, old/formatter/fee
inverses,24functions/max87,frozen models/tests, initial16underbound and corrected
30<=32 table, full A/B execution and exact five-status conservation. I agree;
no substantive disagreement remains. PERF actually read my12087/01456482 before
publishing that receipt. This closes ordinary mutual written-source/resource
review; its later actual read of this result closure requires no extra appendix,
acknowledgement or source/SAVE gate.

Root's full11 session61737 remains separate at this finite close; no full workspace,
old direct-capture runtime conservation, Linux or later checkpoint result is
inferred from focused8. Original HistoryValues mappings, genuine persisted
catalog/current+EVERYhistory/consumed transaction/retention/ordered-read/recovery/
full typed profiles, native old-eight and full M1-M8 endpoint remain required.
Only own allowed note was appended; no source/test/shared file/Git/compiler/VM/
benchmark/probe action. Scoped corrected BODY B acceptance complete; finite STOP.

## VALUE-CONSUMED-BINARY1: exact connected R0 contract, with required successors retained

Decision after saved5255116536588f7e4a9294ae98b5bc656a9eff16. This is the next necessary real-storage vertical slice, not a detached inspector, seeded catalog or hydration completion claim. The first successful path is an explicitly value-capable FRESH database, a self-contained logged logical-binary batch, the SAME loaded revision/accepted plan/consumed Immediate commit, immutable stored byte rows plus current and EVERY intermediate history binding, ordered owned persisted reads, reopen and the actual snapshot capture/receive/install route. Root alone releases its named R0 after reading this decision and the exact test-fixture amendment. No production artifact has been created by this review. Private BODY helpers/models/codecs/registry implementations remain a separate exact consultation after genuine compiled R0 RED is saved; this decision is not permission for unnamed helper shells.

The entire original endpoint remains required: real source cleanup/rewind/retirement, same-ID recreation with its coherent retirement proof, named/shared/external references and routing/fallback, typed/preencoded input publication, common typed FSV2 hydration with the actual application overlay, original three HistoryValues cases/four observations, source terminal-retention cases, strong routing/read boundaries, Linux/macOS and predecessor M2/M3/native gates. The three original HistoryValues arrays remain empty. Binary publication/read/reopen does not earn any of that unexecuted coverage. A finite first profile may expose an explicit UnsupportedProfile at a real missing operation/profile boundary; it may not silently return Missing, corrupt-data errors or incorrect business results as a shortcut.

### Source witnesses and the five closed integration choices

I actually read the stable transaction/record_batch Fresh seam, observations21..88, load24..64/174..238, commit32..150 and independent45..63. Fresh loads, decides, moves its actual plan into GroupApplyChanges, consumes the SAME tx.commit, then calls fresh_reply/into_parts. The independent commit retains EVERY successful intermediate history plan and its write.record postimage; final_indices is current-only. Existing value_effects already verifies supplied p/r/e bytes against the corresponding postimage. Therefore publication borrows immutable postimage bytes while that plan lives and occurs before into_parts, without retaining/redecoding raw commands or making a self-referential owner. The new logged presence/guard/default inputs are copied into admitted numeric/Copy plan metadata before decoded donors drop. No detached metadata container grants transaction authority.

The actual whole-Loaded borrow would block post-decision policy reads. Select a disjoint observations borrow for BODY: only the actual current/query/index/mode/retry owners can be tied to the returned plan; full StoredPolicyValue owners and the SAME cumulative PolicySchemaCharge stay separately accessible. A short mutable staging-budget borrow must not be unified with the long current-record lifetime. No full policy/snapshot clone is selected. load_policies currently drops its locally created charge; retaining that SAME account is an explicit required change, including admission of its resident fixed owner BEFORE Loaded construction on rejected/absent-policy paths. Capture lifecycle ONLY for genuinely accepted Created birth postimages, including intermediate births, not all attempted Creates and not final records alone. Policy-reference resolution/guards still occur in their selected pre-operation order; expensive birth capture must not replace a genuine rejected/no-op result with a new capture error. Actual future BODY signatures/type/helper names for this borrow cut must be selected before their creation.

1. Select EXPLICIT value-capable genesis, not a first-workflow last_entry=None promotion. The new constructor consumes a genuinely empty redb Database, the real existing StateStoreGenesis placements/full Complete donors, and trusted WorkflowValueDefaults. It writes one registered coherent value layout, origin and membership atomically. Real policy installation/bootstrap may then advance control before the first value birth. No populated legacy promotion, no dummy workflow, no seeding helper and no missing-policy fixture substituted for a policy-backed capture. The old constructors remain legacy; opening them does not fabricate known bindings. Healthy legacy binding knowledge is LegacyUnavailable, never Known(None).

2. Select a separately logged defaults/policy/target contract. Defaults are fixed at this fresh capability's genesis and committed in its manifest. Each new batch must carry the identical full defaults and content identity; later live configuration changes require an explicit logged reconfiguration follower. Defaults digest is SHA256 over b"wf-workflow-value-defaults-v1" followed by ttl_ms:u64BE, history_hot_max_events:u32BE, history_max_events:u32BE, max_history_hot_max_events:u64BE, max_history_max_events:u32BE (28 scalar bytes). Validate all scalar semantics before treating the digest as matching. Constructor data is ordinary server-composition input, not a grant. The batch's policy witnesses bind actual type, representation, generation and SHA256 of the EXACT current stored donor bytes; absent policy uses the existing canonical generation-zero type-map fallback. The SAME loaded transaction verifies them. Lower local generation means not applied, higher means stale, equal generation/different content means conflict. Full donor capture alone is not policyRef authority.

I actually read pinned flow_command_time.exs320..490 and cross_shard_dispatch185..383/411..440. Its mandatory same-generation conflicting body, stale type/incarnation and unstamped creation refusals are preserved inputs/tests, not waived by the prior ordinary capture helper. Actual target guard runs before policy-reference resolution. Explicit absent guard differs from unspecified guard; present target must match both witness type and incarnation. A required captured marker is logged; None is missing and Some(false) is invalid. Representation changes are explicit: witness profile0 is the canonical absent generation-zero original fallback, profile1 is exact full Complete-V2 bytes, profile2 is exact Original ETF bytes. No digest across different representation profiles is silently equated. Current full-policy operations and the source-reference format remain distinct; unsupported Original safety/profile is an operational refusal, never a fabricated default.

3. Select source-faithful lifecycle actions. The pinned sections were actually read/hash-bound this round: flow_create57564/b684c85247545ea2e240daf8c050a23cf433599bd2fdd13d61190eb5f93b19a7; flow_claim_native_plan34746/6abe1477401341a1af4970840d9248dc0b34213cf11ee610f0ab389191627ec0; flow_transition58503/cbe11833bbe97d0c015e62866773a4a2997218dcf84a487de4a5e233177e4a5a; flow_retention_values24654/b5ebcfd24c7fb7279386767b492f19751cbdfc57dd6d9cccd8532250065fe0a7; flow_history_reads44319/f23e697a5567599f6b2aa661753683cdff64dbe6d48765eaaa40cece4abdc288; flow_history_writes37225/6fc26b1548806fa2d01a0d058eec42e1f0a88e63631bdfa8218c2943916bb8a3; flow_retention_state40353/d2825a9cfb210db0de11d3e71e79b8ac92f6c16fc50566763b034c8bdad6f949. These are whole-file byte/hash bindings plus the cited actual scopes, not a claim to have reread every line of each file.

Source flow_value_ref652..663 has a nil default fallback. Complete carries omitted payload but clears omitted result; Fail/Retry carry omitted payload but clear omitted error; Cancel uses reason_ref fallback rather than old error. Unmentioned dimensions retain their copied refs. Equal supplied bytes STILL replace at the new version, empty supplied binary remains supplied. The first cut has no explicit external/reason_ref/named/shared input, so omitted Cancel error clears; those explicit-reference inputs are required later. Terminal effective TTL is selected from Complete/Fail/Cancel override presence versus CURRENT effective captured TTL; Retry inherits current effective TTL and may terminalize without a new override. Preserve immutable birth capture separately. A selected nonpositive signed terminal override means known no-expiry; it must not collapse to omission/inheritance. Positive terminal deadline is min(max(requested-now, logged apply-HLC physical)+effective TTL, 9007199254740991); nonpositive/absent effective TTL has no deadline. No wall clock is sampled at apply/recovery. The signed i64 override is an explicit bounded internal profile; larger source integers/types are UnsupportedProfile, not silently defaulted.

The accepted birth's incarnation is its ALREADY validated OrderingMetadata.state_enter_seq, frozen thereafter. Both actual indexed/decision167..188 and source flow_create307..324 use (u128(log.index)<<32)+the state-entry allocation ordinal. It is NOT source_index: errors/no-ops allocate none and earlier accepted state entries may consume ordinals. First ordinal0 is valid, and a genuinely admitted index0/ordinal0 can produce incarnation0. Tests use independent literal log/ordinal math, never read the new binding to manufacture expected incarnation.

4. Select explicit private immutable identity plus public alias, not globally unique public refs. Source birth resets record version1 and public Keys.value_key81..94 omits incarnation. The source visibility/put paths permit a fresh birth after old state expiry and do not implement an NX immutable public-key grant. Thus the durable immutable byte identity includes public reference AND birth incarnation; a separate explicit public alias names the current target and expiry. Historical bare public refs resolve the CURRENT alias in the SAME read snapshot, exactly as HistoryValues raw_mget does; do not silently pin old history hydration to old private bytes. Alias, target, current/EVERY-history bindings and control publish in one consumed transaction. Same immutable identity with different bytes is a collision/corruption refusal. A future accepted reincarnation may rebind only with its real state/history retirement plan; stale cleanup must compare incarnation/alias ownership before deleting a reused alias or current/history row. Immutable bytes and mutable expiry/alias ownership are distinct.

The first binary creation cut does NOT invent that missing retirement operation. Fresh application touching an already expired current/cohort or a reused prior birth that needs retirement returns UnsupportedProfile BEFORE writing, preserving old alias/bytes; it does not misreport AlreadyExists/NotFound or overwrite them. The genuine recreation/retirement/cleanup follower must remove this explicit gap and test old/new public alias behavior. The format below leaves room for source-correct alias reuse without changing public spelling. This is an honest internal profile boundary, not a claim of complete source reincarnation parity or a permanent nonterminal substitute.

5. Select one authenticated SERVER HLC/wall read observation per eligible batch and one redb committed snapshot. Actual ExpiryContext3066/7f0dad269873621d6e0181049a8dc49f0bc86f24250f3a9cfc8282f09f3918ed and HLC311..323 were read: expiry0 live; expiry>HLC-now live; an otherwise expired positive expiry is UnsafeDrift when HLC-now-wall>=1000 AND expiry>wall, otherwise missing. Do not replace this pair with a bare caller timestamp or collapse unsafe to missing. The clock trait is a server-composition dependency, never a public DTO/read_now grant. Production distributed HLC/strong-read/routing remains M2/M3 integration, not an inferred capability of this local cut. Controlled test clocks are deterministic inputs to REAL redb reads, not fabricated storage callbacks.

Pinned redb4.4.0/7061f7e60d4573311c0636e161d417b93e9fd2b1 owns ReadOnlyTable through Arc transaction guards. The implementation may own one snapshot's fixed tables without unsafe/Pin/self-reference; returned batches own copied bytes and keep no AccessGuard. All table/guard owners release before claiming snapshot pins dropped. Open fixed tables once per batch where supported, charging metadata/opens/pins. Returned stable fetch dedup does not imply free per-occurrence consuming decode later. Invalid/empty/>65535/non-value refs occupy None slots, preserving cardinality; the raw predicate itself remains uncapped. All-invalid/empty input performs no clock/storage read. A parsed partition tag is never placement authority; validate actual stored namespace/partition and current placement. No reverse-hash inference or guessed Automatic/global partition: publication renders Explicit(actual prepared partition bytes).

### Exact R0 public models, signatures and limited source write split

R0 has THIRTEEN new ordinary public data/error types and ONE clock trait, no public catalog writer, publication plan, typed encoder, owned read-session shell or proof token. Public fields remain fabricable mathematical values; consumers cannot infer admission/current state/storage authority from construction. No blanket Clone/Debug/Default/serde/accessor roster is authorized. Copy/Clone/Eq/PartialEq may derive ONLY for WorkflowValueDefaults, WorkflowValueTargetGuard, WorkflowValueItemInput, WorkflowValueLifecycle and WorkflowValueReadFailure, all of which contain only existing Copy scalars. WorkflowValueReadFailure and WorkflowValueTransactionError derive Debug for actual failure diagnostics; no custom Debug helper is needed. WorkflowValueTransactionError alone has the ordinary Display::fmt and Error impl, with no custom source method. Other tests inspect explicit fields rather than requiring new Debug/getter/clone methods.

Under NEW crates/wf-ports/src/consensus/value.rs, existing consensus.rs has ONLY its module/reexport seam:
- PreparedWorkflowValueBatch { pub scope:RequestScope, pub request:RequestId, pub stamp:CommittedTime, pub profile:u8, pub policy_references_captured:Option<bool>, pub defaults:WorkflowValueDefaults, pub policies:Vec<WorkflowValuePolicyWitness>, pub operation:PreparedOperation, pub inputs:Vec<WorkflowValueItemInput> }.
- WorkflowValueDefaults { pub configuration_digest:PreparedDigest, pub lifecycle:LifecycleDefaults }.
- WorkflowValuePolicyWitness { pub workflow_type:Vec<u8>, pub profile:u8, pub generation:u64, pub digest:PreparedDigest }.
- WorkflowValueTargetGuard::{Unspecified,Absent,Present{incarnation:u128}}. Present expected type is the item's witness type, not a separately driftable duplicate type owner.
- WorkflowValueItemInput { pub source_index:u32, pub policy_index:u32, pub target:WorkflowValueTargetGuard, pub birth:Option<LifecycleOverrides>, pub terminal_ttl_ms:Option<i64>, pub requested_at:Option<TimestampMillis> }.

profile1 uses existing PreparedOperation::PublicMutation with its EXACT PublicMutationEvidence/Box ownership; profiles5/6 use Batch, profile7 singleton Batch StartAndClaim. No new PreparedOperation variant/duplicate command owner is created. Other operation/profile shapes refuse. inputs is exactly input-aligned, source_index==checked enumeration, witness indices bounds-checked. Policies are a canonical strictly type-byte-sorted unique witness list. Mode1 requested_at agrees in presence/value with existing evidence.requested_now_ms; mode5/6/7 requested_at is None in this cut, preserving their existing committed-time contract. Broader per-item private requested-time inputs require a separately selected follower. Birth presence only for birth commands; terminal_ttl only Complete/Fail/Cancel; default inheritance is explicit and validated. The encoder derives intent8 over caller fields/presence/profile and a SEPARATE digest over ALL prepared frame bytes. No caller-supplied old intent is reused and no old frame is nested/masqueraded as the new profile. Trusted generated stamp/defaults/policy witness inputs are excluded from canonical caller intent as preparation metadata but remain bound by the full prepared digest and actual loaded verification.

Under NEW wf-ports/src/log/value_batch.rs, existing log.rs gets ONLY module/reexport:
encode_workflow_value_batch(batch:&PreparedWorkflowValueBatch, limits:CodecLimits)->Result<PreparedBytes,CodecError> (arity2). Its R0 body is literal Err(CodecError::UnsupportedTag), no parser/hasher/scanner/helper/constants. The private decoder/body helpers are later selected after saved genuine RED.

Populate EXISTING wf-ports/src/clock.rs with exactly trait WorkflowValueReadClock { fn read_snapshot(&mut self)->Result<(HlcTimestamp,TimestampMillis),StateTransactionError>; }. No concrete production clock/default factory, SystemTime read, callback alias, extra model or trait method is created by R0.

Under NEW wf-ports/src/blob/catalog.rs, blob.rs gets ONLY module/reexports:
- WorkflowValueLifecycle { pub incarnation:u128, pub birth:CapturedLifecycle, pub effective_ttl_ms:Option<u64>, pub terminal_deadline:Option<TimestampMillis>, pub configuration_digest:PreparedDigest, pub policy_profile:u8, pub policy_generation:u64, pub policy_digest:PreparedDigest }.
- WorkflowValueBinding { pub refs:ActiveValueRefs, pub lifecycle:WorkflowValueLifecycle }.
- WorkflowValueBindingKnowledge::{LegacyUnavailable,Known(Option<WorkflowValueBinding>)}. New-capability absent core/history is Known(None); a real present core/history missing its required companion is Corrupt. A healthy old layout is LegacyUnavailable, not a fabricated empty set.
- WorkflowValueReadFailure::{ReadFailed,Unavailable,UnsafeDrift,UnsupportedProfile}. Actual corrupt byte-envelope/read failures stay storage-origin, not InvalidFlowValue. Recovery can still reject corrupt inventory globally.
- WorkflowValueReadBatch { pub values:Vec<Vec<u8>>, pub slots:Vec<Result<Option<usize>,WorkflowValueReadFailure>> }. values follows stable first eligible unique reference order; slots restores all input order/duplicates/missing/errors. A Some index names its unique owned stored FSV2 bytes. These public collections are ordinary data; the later hydrator must bounds-check fabricated instances, no proof inference.
- WorkflowValueReadLimits { pub maximum_references:usize, pub maximum_stored_bytes:usize, pub maximum_work:usize, pub maximum_temporary_bytes:usize }. Zero payload budget may admit an empty binary; empty input may finish without read/clock. Do not invent positive-only encoding caps from these fields.

Root's disjoint storage R0 slice is exactly existing wf-storage/src/state/redb.rs (mod values seam), NEW state/redb/values.rs, existing transaction.rs (module/reexports) and NEW transaction/value_batch.rs. No old commit/load/core/recovery/snapshot BODY changes in R0. The new values.rs supplies ONLY these three literal-rejecting inherent methods on the existing RedbStateStore:
create_value_database(database:redb::Database, group:GroupId, recovery:RecoveryId, genesis:StateStoreGenesis<'_>, defaults:WorkflowValueDefaults, limits:StateStoreLimits)->Result<Self,StateTransactionError> (arity6, Err(UnsupportedLayout));
read_workflow_value_batch<C:WorkflowValueReadClock>(&self, clock:&mut C, scope:&RequestScope, refs:&[&[u8]], limits:WorkflowValueReadLimits)->Result<WorkflowValueReadBatch,StateTransactionError> (self+4, Err(UnsupportedLayout));
workflow_value_binding(&self, scope:&RequestScope, partition:&PartitionId, id:&RecordId, history:Option<&str>, limits:WorkflowValueReadLimits)->Result<WorkflowValueBindingKnowledge,StateTransactionError> (self+5, Err(UnsupportedLayout)).

New transaction/value_batch.rs contains the final two public types:
WorkflowValueTransactionLimits { pub record_batch:RecordBatchTransactionLimits, pub maximum_value_rows:usize, pub maximum_value_bytes:usize, pub maximum_value_work:usize, pub maximum_value_temporary_bytes:usize };
WorkflowValueTransactionError::{Record(RecordBatchTransactionError),InvalidInput,PolicyReferenceRequired,PolicyGenerationNotApplied,StalePolicyGeneration,ConflictingPolicyGeneration,StalePolicyTarget,UnsupportedProfile}.
Its ONLY function is apply_workflow_value_batch_transaction(store:&mut RedbStateStore, scope:&RequestScope, encoded:&[u8], log:LogIdentity, limits:WorkflowValueTransactionLimits)->Result<RecordBatchTransactionOutcome,WorkflowValueTransactionError> (arity5), R0 literal Err(UnsupportedProfile). Distinct representable resource/workspace/corruption/uncertain errors remain in Record/its original causes, not relabeled as business policy rejection. No new From/helper conversion roster is selected.

This is FIVE genuine rejecting public APIs, not successful fake I/O. R0 tests must explicitly assert NEW constructor/encoder positives rather than use ? through a rejecting setup helper and then falsely call setup failures intended route RED. Exact test/fixture/clock type roster is being bound by the author to these signatures and will be recorded in the next bounded same-note amendment BEFORE Root releases any R0 artifact. It may reuse the EXISTING state_store target and its real filesystem fixtures, with one module seam and grouped children; no cross-target #[path] or dependency/fixture-file fabrication. Test case names and all later success/control/assertion tails are frozen before first execution. Genuine R0 must compile and pass all technical gates, then fail actual positive assertions. BODY remains held until that exact R0 is saved.

DOMAIN's proposed disjoint R0 slice is the seven port paths above plus the exact selected test paths/main seam from that upcoming concrete fixture decision. Root owns its FOUR storage paths and the M0 classification/shared decisions below. Two authors may work ONLY their recorded disjoint path sets; Root runs no formatter/build/VM/Git/shared mutation while either writer has a live buffer. Both STOP return custody, canonical formatter/review follows, then Root alone runs compiled RED. This is a concrete cooperation cut, not permission to edit each other's or shared files.

### Closed registry contract for the following BODY; no R0 byte emission

Reserve new normal ClientIntent/profile8 for the self-contained value-batch payload under existing FWCP outer version1/opTag1. Modes1/5/6/7 are a contained exact reducer selector, not old intent versions on the outer frame. Old profiles1..7 remain legacy and cannot silently skip bindings when applied to a value-capable store. Explicit registered policy/membership/bootstrap commits may run on the value layout and must preserve its manifest/features/defaults; old mutation/claim routes refuse until their actual value-aware carry/current/EVERY-history integration is selected. No old log is reinterpreted as value-aware, and no discarded historical data is backfilled from the current inline record.

Reserve FWSS7 with policy-profile1 AND value-profile1, extending the complete FWSS6 current-manifest facts with value profile and the five defaults fields/content digest before its full checksum. Retain the actual FWSS3 origin/hash and the existing original-policy catalog definition for ONLY placement/policy families6/7; ordinary mutable value rows must not accidentally join a hash that is not maintained per commit. Registry fields and full encoded offsets/canonical length/header emitters are BODY-selected code, not R0 placeholders. New fresh initialization writes this exact coherent capability before policy setup, with no user records/outcomes permitted as genesis seeds. A genesis policy donor is real full Complete authority, but test5 must additionally exercise actual logged full policy installation BEFORE birth and a later update, not only a constructor-supplied None/default donor.

Reserve FWSP4 for FWSS7. Keep old FWSP1..3 meanings and the existing six physical table names. New value layout needs primary, snapshot and alternate complete cohorts, manifest/membership/origin inventory, strict namespace/order/checksum/feature dispatch and no unknown entries. Snapshot wire/hash covers ALL value/binding rows, not only the policy catalog. Capture, size/header emission, receive, independent validation, install/restore, same-genesis comparison, transition/advance and descriptor/marker guards must dispatch this profile together. Existing layout upgrades never silently promote populated legacy workflow state. No range-only widening from6 to7 is sufficient. Node normal recovery header/replay dispatch and normal-stub/golden registration must recognize8 explicitly before service activation; until the actual route is integrated, no M2/M3/full service capability is claimed.

Reserve FOUR closed physical families under existing FWPK1, valid ONLY with registered FWSS7/value1: family8 immutable bytes keyed by group/recovery/tenant/public-ref plus birth incarnation; family9 public alias keyed by group/recovery/tenant/public-ref; family10 current companion keyed by the SAME five components as core record; family11 history companion keyed by the SAME logical record/event position as family5 history. Use existing canonical zero-escaped component encoding, explicit fixed-width incarnation and canonical history millis/event ID; no raw concatenation or trusted hash inversion. Exact scalar placement and value codec functions/private helper roster will be selected before BODY creation. This reservation chooses semantic identities, not permission for an enum catch-all or family8..11 range acceptance. Unrecognized family/profile stays unsupported/corrupt according to the existing origin distinction.

Family8 content is source-neutral stored FSV2 bytes with immutable identity/content and validated storage envelope; family9 carries actual partition/record/kind/version, current immutable target/incarnation, independently mutable expiry and acknowledged ownership relation. Separate expiry/alias metadata avoids needing to assert an immutable whole envelope while later retention changes; it does NOT prove engine zero-copy. Families10/11 persist refs plus lifecycle/birth/config/policy identity and enough ownership to validate local references. Private physical ownership can retain superseded bytes for safe cleanup/snapshot evidence; public hydration still follows current alias. New profile generation proves all required current/EVERY-history companion rows and local alias targets. Expired local rows are retained evidence while bindings still require them: reader returns missing by captured expiry context, but reopen/recovery does not sample a clock to excuse a missing acknowledged target. Unaliased old targets require a represented supersession/ownership relation, not unexplained junk. Legitimate external/missing references need a distinct later represented origin; no fabricated local target is required for them.

A valid stored envelope is not equivalent to valid typed FSV2. Malformed typed payload can be a later HistoryValues invalid-flow flag without making the storage envelope corrupt, and unsafe/failed reads remain redacted storage errors. The first writer handles ONLY original command logical binaries via exactly [optional b"FSV2\x01", original payload] when original begins b"FSV2". It never interprets magic-looking current Vec bytes as trusted encoded ETF. Stored size for omission is actual FSV2 byte length, excluding private catalog envelope; a checked header may avoid an adapter copy/decode but does not prove redb avoided mapping/materializing/traversing payload bytes/checksum. Typed/preencoded profile later needs a separately logged discriminator and validation/encoding decision, not a prefix guess.

### Mandatory ownership/admission and actual test boundaries

All published data joins the SAME consumed transaction before control/outcome/Immediate commit. No seed API/callback/detached writer is added. Backend immutable collision check, alias/capability/guard checks and all byte/key length bounds precede reserving/emitting/replacing any row; any precommit failure aborts the whole transaction, while actual commit uncertainty fences/reopens instead of blindly retrying as KnownAborted. Each accepted item is visited, including nonfinal histories; failed/no-op holes retain original source indices. Keep/Clear/Replace is history-kind/real-presence driven, not a postimage byte diff. Empty/equal supplied data is not discarded. Birth capture uses every actual Created postimage and actual ordering metadata, never a current final record after policy changed.

Use one continuous value work account and the SAME continuous policy schema account. Prospectively price every reached loop/probe/miss/end, Option/dispatch/scalar/control cohort and every byte on BOTH sides of comparison, key rendering, SHA, CRC/frame/checksum/emission/copy; no resets per item/key/dedup/retry. A conservative initial BODY tariff may use128 per bounded reached scalar/control/probe transition and one per actually processed/copied/hashed byte, but its exact functions/cohorts and cumulative envelope must be selected and actual-code recounted; naming a budget is not proof. Dynamic original parser/index work remains additional. No unnamed512/192n slack funds added owners.

Before any new owner allocation, checked live formulas include: full encoded request/decoded command donors, new logged witness/default/target metadata, always-resident added plan/Loaded/schema owner shapes even None/replay/rejection, actual accepted numeric/capture rows, existing records/full policies/mode projections, every current/history/alias write and its private envelope, temporary rendered keys/row buffers, backend guard limits, read tables/pins and output unique byte owners plus full ordered slot vector. Use actual size_of for fixed/row owners and checked lengths for variable fields, not assumed layouts or capacity==requested. Admit actual surviving row count after semantic/count validation before one final reserve; count/growth scans themselves must be funded. No Clone of large policy/input/current buffers, no infallible Vec clone into duplicates, no Box/shrink for style, no fake reserve/OOM test or claimed allocator/RSS/stack/initial-R theorem. A per-row final contiguous storage buffer is allowed where redb needs it; [prefix,payload] does not make backend copies disappear.

Real tests must route through full accepted prepared inputs, real policy installation, SAME GroupApplyChanges/consumed redb transaction and actual ordered reader. Reopen and real snapshot capture/receive/install, stale/no-publication, corruption by deleting a REQUIRED local row, stable duplicate ordering, opaque/empty/FSV2-escape bytes, per-event omission/clearing, terminal override presence, fixed-clock unsafe/expired visibility and post-donor drop ownership are the first cut's evidence. Exact expected birth incarnation is literal log/ordinal math. Preserve actual lower-level gate/replay/refusal controls. Source key predicate65536 remains true while this routed reader filters it by ValueStore's65535 ceiling; do not change the uncapped parser or make parsed partition text a capability. No benchmark/gain claim is selected.

The endpoint's real passive terminal removal, explicit cleanup of created payload AND result, rewind same refs/same bytes refreshed with deadline cleared, and true recreation need their actual later operation/guard/continuation models. Current Operation lacks rewind/cleanup. Source rewind additionally restores priority/created_at/attempts/scheduling/refs and checks lease/lineage/target history; reduced FWSH1 alone cannot reconstruct these. Source sm_store_batch_get uses ONE ExpiryContext, committed expired rows become missing/unsafe and same-apply pending rows have distinct visibility. The later refresh planner must preserve those branches, not rewrite every physically retained row. Those obligations and Original helper/typed/application/routed integration remain the named required followers, never silently omitted or credited by ten binary storage tests.

M0 classification is a precise Root-owned prerequisite, based on actual source/purity.rs: blob is already recursively pure; NEW consensus::value, log::value_batch and populated clock are not. Extend only the existing tests/blob_policy.rs module loop with those THREE names, retaining its original SHA/Vec positives, fs/time/thread negatives and client/worker controls. Run that guard before prefixes (technical failure expected), then add ONLY these three recursive pure prefixes in source/purity.rs and append the narrow rationale to EXISTING ADR006. No new test function/file/dependency/blanket purity exemption. Root may make these three existing-file changes before R0 writer release, then freeze them during authorship. The guard is classification evidence only, never business RED or storage success.

### Actual peer consultation and finite pending test binding

I actually whole/hash-read PERF253210+15791/b391987fb52b86fb1f820b04c4905c694c7dc360f84634b5c11f0039fca7d81d through269001. It agrees on disjoint observations/SAME schema account, accepted intermediates, postimage borrowing, same consumed commit, real full-policy setup, explicit activation/reincarnation choices, owned redb tables, HLC/wall unsafe/pending distinctions and the missing real hydration endpoint. Its subsequent direct alias qualification agrees: historical refs follow current alias, rebind is atomic, stale cleanup checks ownership and no speed/copy theorem follows. I sent the full exact public ABI above directly to DOMAIN so the existing-target fixture roster is tied to actual signatures, not invented imports. DOMAIN identified the real ordering-ordinal incarnation premise; the source-backed math above closes it. Its concrete fixture/test roster is the remaining named-artifact amendment for this R0, NOT a new research or readiness gate and NOT permission to create tests before Root reads that amendment. No source/model/test/shared doc/ADR/raw log/workload/VM/Git mutation occurred here; only this allowed architecture note append.

## VALUE-CONSUMED-BINARY1 final R0 fixture/decoder/intent amendment and disjoint release roster

This amendment closes the concrete pending creation items in386254+33935/e5dfa264. It supersedes that span's five-rejector count and preliminary DOMAIN-port/Root-storage split ONLY: SIX literal-rejecting APIs, the SAME thirteen production public types/one clock trait, Root ALL production and existing test-main seams, DOMAIN EIGHT new test files. No other public model, path, callback, shell, constant, alias, trait method or private BODY helper is implicitly authorized. Root's whole/read/adoption and explicit release still control creation; this is the finite exact decision, not a request for an acknowledgement loop.

### Sixth necessary API, exact canonical intent and owned wire view

Select ONE added public function in the already selected NEW wf-ports/src/log/value_batch.rs, reexported by its existing log.rs seam:
decode_workflow_value_batch(encoded:&[u8], limits:CodecLimits)->Result<PreparedWorkflowValueBatch,CodecError> (arity2). R0 body is literal Err(CodecError::UnsupportedTag). It returns the SAME ordinary OWNED input model, with no proof/admission/current-store authority and no new decoder-view type/accessors. Its future BODY freshly checks whole framing/checksum/digests/counts/widths and prospectively admits all copied owned fields before decoding/emission. It cannot validate current policy/default/target state by itself. Existing generic DecodedRequest/PreparedOperation do not currently retain the new metadata; do not pretend decode_prepared exposes those fields. The exact ports-to-existing-plan BODY handoff/visibility/helper signatures remain the saved-RED consultation, not an unreviewed callback, old-frame authority laundering or permission to reconstruct caller metadata after donors drop.

There are now TWO literal producer refusals (encode/decode UnsupportedTag), THREE store-method refusals (UnsupportedLayout) and ONE transaction refusal (UnsupportedProfile). The owned decoder adds no path/type. The two producer tests first assert encoder-positive, then explicitly assert decoder-positive and compare independent literal semantic fields. A later encoded donor can drop while decoded owners remain valid. Initial rejecting decoder tails are honestly masked when encoder fails first; no decoder/runtime-success claim is inferred from compilation.

Freeze ClientIntent8's exact top-level roster. INCLUDE the server-bound RequestScope tenant/group/recovery bytes, contained reducer profile, ordered mode-specific existing canonical CALLER operation/evidence stream (logical partition/id, command fields/presence/explicit caller guards/priority as represented in that existing stream), plus the source-supplied birth LifecycleOverrides, terminal TTL override Option presence/signed value and requested-time presence/value. A duplicated requested-time field must be coherent and charged but emitted into caller intent ONCE. MaxActiveOverride's Inherit/Infinite/Millis and all retention/hot override Option discriminators remain caller semantics. Existing explicit fence/version/state/lease guards in Operation remain included. Existing mode-specific exclusions for generated command time/resolved retry choices remain; a requested time becomes caller-semantic through its actual presence/evidence, not because a prepared scalar happens to contain a timestamp. New V8 domain separation and exact byte stream/helper implementation are BODY-selected before creation, with these semantics frozen.

EXCLUDE request identity (it is the separate retained key), committed stamp, policy_references_captured, the policy witness list/indices/profiles/generations/digests, source_index, captured target guard/type/incarnation, configured defaults and configuration digest. Those are SERVER preparation context, not new caller input. Input order is already encoded by the ordered operation stream; source_index is a checked alignment witness. All excluded fields ARE included in the FULL PreparedDigest/whole frame and freshly checked at actual apply. Re-preparing the SAME caller request under changed server policy/target context preserves ClientIntent8 while changing PreparedDigest; the existing wire case freezes this exact control. Changing explicit caller guards or terminal/birth override presence changes intent. No captured target is promoted into an explicit caller-semantic guard.

Marker Option is REPRESENTABLE data in encoder/decoder: None and Some(false) must be able to reach actual apply's selected PolicyReferenceRequired versus InvalidInput lanes rather than being rejected as an unknown encoding discriminator. Structural shape/bounds/profile safety checks remain codec responsibilities; store semantic/current authority validation remains in the transaction. Target semantic guards run before policy-reference semantic resolution, consistent with the pinned source. Old retained/control gates remain explicit and unchanged rather than a detached freshly fabricated grant.

The absent policy witness is type-dependent, NEVER SHA(empty) or zero. profile0/generation0 hashes the exact existing original fallback ETF {flow_policy_v1,0,#{type=>Binary(requested_type)}}. Select the fixture type b"r", matching the existing full rich donor:38B hex836803770e666c6f775f706f6c6963795f7631610074000000017704747970656d0000000172; SHA2566e9978636a6017b6e2abf81647a5aab1b71f003b16e23ad931a1beda2d62b9a9. I independently constructed this static literal from the ACTUALLY read existing original_entry/callsite fallback, not by running a new SUT/workload. The wire fixture first verifies the OLD raw original-reference API accepts/agrees on that literal before a NEW encoder assertion. Other absence queries may use the old exact fallback preparation as INPUT construction, never as an expected capture/output oracle. No new fixture constant is required.

### Exact twelve cases and eight new test files

Use the EXISTING wf-storage state_store compiler target, already forbid(unsafe_code), with ONLY `mod workflow_values;` added by Root in tests/state_store/main.rs. No new Cargo target/dependency or cross-target #[path]. DOMAIN creates exactly these EIGHT test files:
1. tests/state_store/workflow_values.rs: imports plus six child module seams, no new type alias/constant/helper/test body.
2. tests/state_store/workflow_values/fixtures.rs.
3. tests/state_store/workflow_values/fixtures/policy.rs.
4. tests/state_store/workflow_values/wire.rs.
5. tests/state_store/workflow_values/publication.rs.
6. tests/state_store/workflow_values/guards.rs.
7. tests/state_store/workflow_values/reads.rs.
8. tests/state_store/workflow_values/recovery.rs.

Exactly TWELVE #[test] names, no ignored/filter/setup-generated replacement:
wire.rs: value_binary_encoder_preserves_presence_and_configured_defaults; value_binary_encoder_preserves_reducer_modes_and_policy_witnesses.
publication.rs: value_binary_commit_publishes_catalog_current_and_intermediate_history; value_binary_binding_actions_preserve_equal_empty_and_omitted_values; value_binary_birth_and_terminal_ttl_overrides_survive_replay.
guards.rs: value_binary_policy_witnesses_guard_births_and_targets; value_binary_fresh_capability_refuses_legacy_and_unimplemented_lifecycle; value_binary_limits_preserve_no_partial_publication.
reads.rs: value_binary_ordered_reads_preserve_duplicates_missing_and_expiry; value_binary_read_clock_is_sampled_once_and_failure_stays_operational.
recovery.rs: value_binary_reopen_preserves_capture_after_policy_change; value_binary_snapshot_roundtrip_preserves_alias_and_immutable_bytes.

The first two are genuine producer positives after valid old typed/scalar premises. Each TEN real-store cases uses valid OLD scope/placement/clock/limit/real filesystem/new empty redb premises, receives the NEW constructor Result unconverted, explicitly asserts that intended positive at the test site, then destructures it. Do not use ? through a rejecting constructor and call the hidden setup error semantic RED. Later encoder/apply/read positives likewise assert before conversion. Initial expected constructor UnsupportedLayout failures mask later real I/O tails honestly; only future GREEN plus actual result review unmasks them. No new encoder in storage setup silently fabricates a passing frame. All four existing reducer modes1/5/6/7 and genuine independent holes/no-ops preserve the prior supported-operation boundaries; the first mode loop may mask later iterations in R0 and must be reported accurately.

The literal tails cover: EVERY same-ID intermediate postimage/history rather than final-only; None/equal/empty supplied behavior and exact Keep/Clear/Replace; newrefs at positive record version and true core ordering; defaults/type/state/command lifecycle values; terminal override omission/positive/zero or negative known-no-expiry and requested/apply max; exact replay/no duplicate publication; target/type/incarnation/missing marker/equal-generation conflicting body/not-applied/stale policy; positive empty/opaque/magic FSV2 bytes, including single escape and stored size; stable fetch order/duplicates/filter cardinality/missing/expired/unsafe/error lanes; all-invalid/empty bypass and one clock sample for an eligible batch; returned owners after request/policy/plan/transaction/session donors drop; actual installed FULL policy BEFORE birth, actual later policy update/reopen without recapture; real snapshot copy/receive/install and complete byte/alias/binding preservation; legacy/populated/expired-retirement/external-profile refusal with no partial publication; actual prospective resource/one-short refusal controls. No declared tail is a claim that it has executed in R0.

The existing root fixtures reusable unchanged are directory, scope, clock, log, placements, driver_limits and raw_rows. DO NOT reuse root raw_manifest or replace: I read their actual bodies; root raw_manifest asserts metadata len3 and root replace drops all rows other than manifest/membership/snapshot, which would destroy the new origin row. That was a concrete reuse defect caught before creation, not permission to weaken the new metadata inventory. New primary value metadata has exactly manifest/membership/origin/snapshot; snapshot wire omits the local marker and carries the three authoritative metadata rows. The feature's own helper below preserves/verifies all four.

Freeze the missing-required-local control in the SAME existing snapshot test tail. After real capture/receive/install/owned reads and dropping active handles, find an actual immutable row by independently specified prefix b"FWPK\0\x01\x08", remove ONLY that existing family8 row through a real redb write transaction, preserve alias/current/history/all metadata, then existing RedbStateStore open/open_database must return Corrupt. This is corruption injection into the actual backend, not a production seeding API or a fabricated missing success. No new helper/case is needed; if final natural formatting cannot fit100, consult a concrete responsibility cut BEFORE creating another helper. The private value codec/golden implementation still gets BODY review and independent checks; no present corruption runtime claim is made.

### Exact seventeen fixture functions and one local clock type

Reuse the existing root TestResult alias; no new alias/constants/production fixture models. Ordinary use imports may name existing modules; no new type alias is selected. To avoid conflicting fixtures module names, feature code can refer directly to crate::fixtures for the root donor helpers. In fixtures.rs, private-to-feature functions have pub(super) visibility; in fixtures/policy.rs the six helpers use pub(in crate::workflow_values) and fixtures.rs may reexport ONLY their six names to the same feature scope. No broader test visibility or glob helper permission.

fixtures.rs ELEVEN functions (the eleventh is the actual origin-preserving correction):
- defaults()->WorkflowValueDefaults.
- limits()->WorkflowValueTransactionLimits.
- read_limits()->WorkflowValueReadLimits.
- fresh(path:&Path)->TestResult<Result<RedbStateStore,StateTransactionError>>. OLD filesystem/database/scope/default scalar setup may fail in the OUTER TestResult; the NEW six-argument constructor result is the INNER Result and must stay unconverted until the test's explicit positive assertion. It uses a real empty redb handle, actual placements and fixed defaults, not a seed or caller read_now.
- batch(profile:u8, request:&[u8], now:u64, operation:PreparedOperation, inputs:Vec<WorkflowValueItemInput>, policies:Vec<WorkflowValuePolicyWitness>)->TestResult<PreparedWorkflowValueBatch> (arity6). Builds ordinary prepared input only; does not invoke a new encoder/capture/producer to calculate expected outputs.
- item(id:&[u8], now:u64, operation:Operation)->TestResult<PreparedBatchItem> (arity3).
- input(source_index:u32, policy_index:u32)->WorkflowValueItemInput (arity2), with explicit default presence fields adjusted at callsites for each literal scenario.
- apply_value(store:&mut RedbStateStore, scope:&RequestScope, batch:&PreparedWorkflowValueBatch, at:u64)->TestResult<RecordBatchTransactionOutcome> (arity4). Calls real encoder and actual new transaction, explicitly asserts each positive before converting; it does not fabricate returned success.
- assert_binding(knowledge:&WorkflowValueBindingKnowledge, owner:(&[u8],&[u8]), incarnation:u128, versions:[Option<u64>;3], lifecycle:(Option<u64>,u64,u32,u32,Option<u64>,Option<u64>), policy:&WorkflowValuePolicyWitness)->TestResult (arity6). Matches every ref class/exhaustive unexpected branch, literal partition/id/kind/versions and independently expected incarnation. lifecycle tuple is birth max-active, birth positive TTL, hot cap, history cap, effective TTL and terminal deadline. It also checks configuration against independent defaults input and policy profile/generation/digest against the INPUT witness. No reading an output to manufacture expected capture/identity.
- assert_read(batch:&WorkflowValueReadBatch, values:&[&[u8]], slots:&[Option<usize>])->TestResult (arity3), exact bytes/count/order and missing/duplicate slots; failure-lane tests inspect their explicit Err variants separately.
- raw_manifest(path:&Path)->TestResult<Vec<u8>> (arity1), real primary table read, asserts EXACT four metadata keys including origin and returns actual manifest bytes. Never calls root replace/raw_manifest.

fixtures/policy.rs SIX functions:
- complete_policy(generation:u64, type_ttl:u64, state_ttl:u64)->TestResult<CompletePolicyValue> (arity3), using existing full typed restoration/preparation/verification as an independent accepted INPUT donor, not expected capture via the new SUT.
- original_policy(generation:u8)->TestResult<StoredPolicyValue> (arity1), independent literal Original ETF with OLD portable acceptance before a new-call positive; not mode-projection reconstruction.
- install_complete(store:&mut RedbStateStore, scope:&RequestScope, patch:PolicyPatch, expected:Option<u64>, at:u64, now:u64)->TestResult (arity6), actual existing logged full policy transaction on this SAME fresh value-capable store, with real control advancement, no sidecar policy.
- install_original(store:&mut RedbStateStore, scope:&RequestScope, raw:&[u8], expected:Option<u64>, at:u64, now:u64)->TestResult (arity6), actual original policy transaction/profile, not raw table seeding.
- load_policy(store:&mut RedbStateStore, scope:&RequestScope)->TestResult<Option<StoredPolicyValue>> (arity2), authentic existing same-store guard/read; drops the guard before returning its admitted owned donor.
- witness(workflow_type:&[u8], policy:Option<&StoredPolicyValue>)->TestResult<WorkflowValuePolicyWitness> (arity2; descriptive parameter name corrects the proposal's ambiguous `kind`). Hashes the actual retained encoded() backing and generation/profile; no new digest reader trait, raw owner, re-encode or Clone. Absence uses the exact type-dependent fallback above. This is prepared INPUT identity, not a SUT-generated expected capture oracle.

ONE new test-only type in fixtures.rs:
ValueReadClock { hlc:HlcTimestamp, wall:TimestampMillis, calls:usize, failure:Option<StateTransactionError> }, type/fields private to the feature (pub(super) as needed by its children), no inherent constructor/method/derive/default. Its ONLY impl is WorkflowValueReadClock::read_snapshot(&mut self)->Result<(HlcTimestamp,TimestampMillis),StateTransactionError>: checked call count then the selected pair/error. A foreign ManualClock cannot receive the foreign port trait impl under Rust's orphan rule, hence this explicit local adapter. It never grants production HLC/quorum authority and never substitutes a fake storage callback. No lifetime ties returned owned read bytes to this clock.

Count:17 fixture/helper functions, ONE trait implementation method and12 tests, with zero new test aliases/constants. Natural estimates fixtures<500LF, policy<360LF, other children<400LF, each function<100; these estimates do NOT waive actual formatted checks. The original index0/ordinal0 allowance remains; literal accepted state-entry ordinal, NOT input source index, determines expected incarnation.

### Final disjoint authorship and finite peer closure

Supersede the preliminary port-writer split: Root owns ALL ELEVEN production R0 paths, plus the one existing test-main seam. Production list exactly: wf-ports/src/consensus.rs; consensus/value.rs; log.rs; log/value_batch.rs; clock.rs; blob.rs; blob/catalog.rs; wf-storage/src/state/redb.rs; state/redb/values.rs; transaction.rs; transaction/value_batch.rs. Existing seams change only module/reexports (plus the reserved clock trait population); new files have exactly the selected13 public types, Error formatting and SIX rejectors, no substantive BODY. Domain owns ONLY the EIGHT new test files listed above. Root's three existing-file M0 purity/control/ADR changes are separately adopted/completed before this R0 release and then held. Every other source/shared file remains literal during BOTH authors' live buffers; Root formatter/compiler/VM/Git and shared coordination writes wait both STOP. Root journals the release/hold beforehand, then performs full stopped-source/read/inverse/census/formatter/technical checks and intended compiled12-case RED/save. No third writer or moving-source-as-final review is implied.

I actually whole/hash-read PERF269001+3321/4a3bcd4e0decd515ff16bb81875b4ff390f5dca13343d65f59cd48aab52c7d98, which qualifies public alias history semantics, exact pinned policyRef tests, fixed retained schema owner, uncapped predicate vs routed max and header-copy vs backend backing. Direct discussion agrees with the core33935/e5dfa264. PERF independently points out StoredPolicyValue::encoded() already borrows exact full backing, so no new trait/re-encode/owner is needed for the witness. Its now-published272322+4418/aafde131760dd642fc7d1705c70576da75ac0ca55781c9d275678187e61754cf is an ordinary final written read, not another release/append acknowledgement gate. I have actually received and resolved the author's exact fixture proposal above, including raw_manifest origin preservation and necessary owned decoder, without granting source authority. All current design work remains read-only except these architecture appends; no new artifact/model/source/test/ADR/policy/raw log/build/VM/Git was created or changed by me. The finite next-R0 decision is now concrete for Root's ordinary adoption; actual R0/BODY execution/reviews and the full required endpoint remain unclaimed.

## VALUE-CONSUMED-BINARY1 actual Root-stopped R0 production review; test writer still live

I read the actual journaled two-author release in docs/reviews/workflow-completion-coordination.md, then waited for Root's explicit twelve-path STOP. Only THEN I actually whole-read all eleven production files and the existing target-main seam, split into output ranges where necessary. DOMAIN's eight test files are still moving and were NOT read/treated as stopped evidence. No compiler/formatter/VM/test/Git action occurred. Root's held1598/a22b341752f1faad3b8db640adef84fb72c8f7ec5805f163489d630fc83febb0 proof is attributed to Root; I did not independently scan moving test buffers as a frozen tree.

Independent whole file LF/bytes/SHA256 bindings:
- wf-ports/src/consensus.rs100/4090/11f5fe417facb8f43d6b41c02abc7c866cb9407b6dedca3d6675b474c74a6932;
- consensus/value.rs83/3281/001b923ea3bdc4dd5a15fd4141a979bff708aa9855c6048f7f25cb47af273771;
- log.rs396/16216/844df32946963243b0eebb798da5d48629891981d2712e408c9d95d3a0f1d972;
- log/value_batch.rs27/820/ee72e8f130f3d81656790bc0193a79005a074e144400b69f004c1ee86486c8e2;
- clock.rs14/572/a42a1545b5d8cd94212781b552f6af6ffd0a7d661fb01471b6b7521d2cf24590;
- blob.rs87/3220/fb0121bb0d07ce332bbbc38d42684789bb50472842537a1aa91cc171ebc5783e;
- blob/catalog.rs75/3031/7d40d0fda883b12859d98b1d6ca4ebdb76affdda58afc27e78f29b00f62e9998;
- wf-storage/src/state/redb.rs228/7943/7154f51e849791dfd435f11a2b7be5ef41f1db6760ce76fdee197600bb52d767;
- state/redb/values.rs56/2046/4c88cab181697b378ade762d6008876a562914c71b5aef9bed905fcbce9c2bdb;
- transaction.rs120/4749/25afbffa95151c3967a23b8f3ca74962d26f62c76ffd957e04790066ffcf5946;
- transaction/value_batch.rs82/3310/1516be2657ff2848224ad9d95a5a28c1a78f3f13e72745cf32ed36c4435f922e;
- tests/state_store/main.rs543/17244/3c978b0ee9929d2570de7b700fe0ca377854b6a1ade28bcb795c1b320429c166.

The ACTUAL selected ABI matches all thirteen new data/error types, one clock trait and six rejecting signatures in core33935/e5dfa264 plus final19569/1eb322b5. Field optionality, signed terminal override, scope, profile/marker/witness/target/birth/source index, existing PreparedOperation owner and full defaults are exact. No duplicate operation owner/request-authority wrapper, new derive/helper/alias/constant/encoder BODY/private parser/model appears. The only new Copy derives are the explicitly selected scalar/default/target/input/lifecycle/failure data; old PreparedOperation/ActiveValueRef Clone/Box availability is not a newly executed Clone. Read output collections and public fields remain ordinary mathematical data, not proof of admission or current storage.

I independently removed ONLY the exact six selected parent/main seams in memory, checking each occurs once. ENTIRE reconstructed beforeimages match Root's saved pre-release metadata: consensus removes168B ->3922/390e9f76ace535c58948b2e583cdb2a95072484ce3670556619bc10f067ad8db; log removes98B ->16118/2e75f4d68ae0d4ab14dc45f7dba842b6964ca970c81524a90d672768a9a05ce5; blob removes195B ->3025/68d4c97e50c0d594e3c3c99770a4d584f04eb703ac79f204371cc7b695d35b6d; redb removes12B ->7931/d8c5fdffc67261340a31fee543e9b8b44a6f87c5f1263082e3347bd652cf1ea3; transaction removes155B ->4594/6627e1de96c40ca4863a747b08919c1b0852c09549b5d7cc5fb9a03ad4735ec5; target main removes21B ->17223/bb8bcf84042a1d399eec9dc63b75b198bb1204b1aa0a232c4d6e875d700d8f56. I also reconstructed the exact authorized reserved clock3LF/121B beforeimage edd13843523e203ac9e740971f8566d2b4699d7efaa412ff4d1a22e1006c5c2b from the saved literal. This is full beforeimage binding, not a changed-line-only claim. No Git command was used. Existing unrelated parent function bodies, old test scenarios/assertions and single public operation owner remain literal.

Actual six bodies are ONLY Err(CodecError::UnsupportedTag) for encode/decode, Err(StateTransactionError::UnsupportedLayout) for create/read/binding, and Err(WorkflowValueTransactionError::UnsupportedProfile) for apply. Apart from normal error fmt/Error, no new function body does explicit decode/hash/copy/reserve/clock sampling/transaction/table publication or emits a registry byte. The clock method is a declaration, not a concrete HLC. Constructor ownership is qualified: its real redb::Database is CONSUMED and naturally dropped/closed on the refusal, so I do not claim zero global filesystem I/O/destructor effects. No real value capability, successful owner-copy, alias, profile8 or persisted byte route follows from a signature or compilation.

Natural current geometry is within limits: largest existing production parent396LF, largest test target543LF, all six new/populated files14..83LF, actual six rejecting bodies6..10LF and selected error formatter16LF by a conservative static brace census. No geometry waiver/packing/body helper or new compiler target is hidden. The existing target retains forbid(unsafe_code). Source is still pre-format/pre-compile; no quality or business RED credit is asserted.

Two actual documentation corrections were communicated to Root for an authorized post-STOP patch window, not written by me: blob/catalog.rs values must say owned STORED FSV2 bytes, not decoded/logical binary bytes; terminal_deadline belongs to the RECORD lifecycle, not a deadline automatically inherited by every earlier value reference. Existing value expiries remain independent. These are semantic documentation corrections, not an API/type/body change. Actual later hashes/inverse will need ordinary rebinding.

Root requested a static large-enum assessment. Known(Option<WorkflowValueBinding>) currently embeds three optional ActiveValueRef owners and the entire lifecycle; Versioned refs own both PartitionId and RecordId backing, while lifecycle carries u128/two32B digests/captured/options. The inline unit-vs-large shape may trigger Clippy large_enum_variant, but no layout probe/compiler diagnostic/size has executed. I explicitly select RETAIN THIS EXACT INLINE R0 shape until actual post-BOTH-STOP compiler/Clippy. A speculative Box would add a real per-binding owner/allocation; Box::new is not a fallible reserve route and is not an ordinary spelling change. No Box/helper/waiver/model alteration is authorized by a predicted warning. If a real diagnostic appears, select a concrete representation and actual ownership/admission refinement before applying/rerunning. PERF independently agrees on this measured-evidence boundary.

### Concrete install_original6 route, without fabricated full-replacement authority

Root and DOMAIN found that the existing apply_original_policy_set_transaction consumes public SET point frames, not the pure prepare_original_flow_policy_replace_frame plan. I actually read transaction/original_policy's preparation/point gate/consumed writer, the public_set frame/request model/API, existing original_policy/fixtures/frames public_frame, and the actual interop authority/replay examples. GroupOriginalPolicyChanges requires OriginalPolicySetRequestPlan; no full-replace plan cast or direct encoded-policy seed is acceptable.

Select the author's SAME NAMED install_original6 limited-fixture route before its creation. Validate raw with the OLD portable original reader, parse generation and require WHOLE raw bytes equal the selected original_policy(parsed-generation) r fixture. Construct the same known literal caller fields (type r, max_active5000, type retention TTL11000/history30, state q retention TTL22000/history40) through the REAL public SET request with replace:true and expected_generation. Use prepare_original_policy_set_request -> exact captured-order OriginalPolicySetFrameInput -> prepare_original_policy_set_frame -> actual apply_original_policy_set_transaction on the SAME store. Keyword controls and known fixed small-map orders/patch identity are mathematical fixture inputs validated by existing APIs, not fabricated current storage or an unreviewed generic raw-map converter. No new helper/type/model/path is selected by this route.

This test-only helper is NOT an arbitrary ETF upload API. Public SET allocation/normalization may emit a different legitimate installed generation/encoding than the donor; do not claim byte-for-byte direct raw replacement. Later load_policy reads ACTUAL installed owned backing/generation, and witness hashes that INPUT. Expected NEW catalog birth capture remains the independent literal5000/11000/30/22000/40, never obtained by calling the new SUT. Unsupported raw donors fail this fixture's stated precondition rather than being silently mapped to unrelated fields. All ordinary full-donor/current guard/replay/CAS rules remain in the actual existing transaction. If real formatting exceeds100LF, another responsibility helper requires exact consultation BEFORE creation; no packing or implicit generic helper permission. DOMAIN's other test work continues normally, without an acknowledgement/source gate.

### Actual written reciprocal source read and limits of this receipt

I actually WHOLE/hash-read PERF281467+6934/495461eaf003db41635f80d97bd408295cd054d053f437256ed1145c533fbafe,end288401. Its independent twelve hashes, exact ABI/stubs, Database Drop qualification, static inline ownership and pending two-doc corrections agree with this review; no production-contract discrepancy was found. DOMAIN's unfinished tests, technical compilation and intended R0 runtime are still unreviewed/unexecuted here. No current case is declared PASS/FAIL, no stored success or original HistoryValues mapping is inferred, and no BODY is selected/released. Only this allowed architecture note was appended; all source/test/shared artifacts/receipts/workloads/Git remain untouched by me. Actual stopped test/peer/runtime review follows normally when supplied, without another readiness/append acknowledgement loop.

### R0 test-only original_frame4 geometry amendment before helper creation

DOMAIN reports the selected install_original6 fixed public-SET route would naturally exceed100LF when it also owns all Raw arrays, five map-order records and frame constructors. Its old donor/whole-fixture/generation check and actual durable driver are a separate responsibility from constructing the accepted old public input frame. Select ONE additional private helper in the ALREADY approved tests/state_store/workflow_values/fixtures/policy.rs:
original_frame(scope:&RequestScope, expected:Option<u64>, at:u64, now:u64)->TestResult<PreparedBytes> (arity4), same feature visibility pub(in crate::workflow_values).

This helper owns ONLY the known literal r max-active/type/state retention Raw caller tree, proper replace:true/CAS keyword controls, exactly FIVE literal identity map-order rows for that fixed tree, the OLD prepare_original_policy_set_frame_bytes patch range/SHA, and OLD prepare_original_policy_set_frame. The five maps are root policy patch, type retention, states, q-state configuration and state retention; any unexpected normalized patch/order mismatch must fail the existing preparer's validation, not be accepted by a fake capture token. Request identity/time are ordinary input setup from the supplied at/now. No generic raw-policy-to-attrs converter, new model/file/alias/constant/trait, new SUT use or expected capture computation is selected.

install_original6 retains the OLD portable raw/gen/whole canonical donor precondition, calls this fixed OLD frame helper, then invokes the REAL apply_original_policy_set_transaction and checks actual accepted receipt. Load/read of actual installed backing remains separate, so no claim that public SET uploads arbitrary encoded raw bytes or preserves the donor generation/encoding verbatim. Independent literal capture oracles remain unchanged.

Fixture count changes17->18; twelve cases, all signatures other than this added arity4, thirteen production types/clock/six rejectors and eight test paths are unchanged. Estimated original_frame90LF/install30LF is a responsibility estimate, not a geometry waiver; actual ordinary formatting still must satisfy100LF/function and600LF/test file. Helper has NOT been created at this decision. Root's concrete message adoption is the sole release and can serve as amendment journal while its shared coordination file is held during test authorship. Other already approved test work may continue; no extra readiness acknowledgement is required.

### R0 authorship-only amendment: reads.rs transferred after safe relinquishment

Root reports DOMAIN's actual buffer status: wire.rs already has a live in-memory draft and MUST remain DOMAIN-owned; reads.rs is entirely unstarted and safe to transfer. Select ONLY tests/state_store/workflow_values/reads.rs for Root authorship, retaining its EXACT two approved tests value_binary_ordered_reads_preserve_duplicates_missing_and_expiry and value_binary_read_clock_is_sampled_once_and_failure_stays_operational. No model/helper/API/clock/fixture/path/test/assertion or semantic change accompanies ownership transfer.

Final ownership is Root THIRTEEN paths (the prior eleven production plus existing state_store/main seam plus this one reads.rs), DOMAIN SEVEN (the other seven new files). DOMAIN explicitly relinquishes reads.rs to Root BEFORE Root creates any buffer/file; Root's attributed safe-status report is not a claim that I inspected a moving draft. No shared ownership or overwrite is allowed. wire.rs stays wholly DOMAIN-owned and is not transferred merely because Root had earlier offered two paths.

The twelve existing Root production/seam buffers remain STOPPED/readable at the reviewed hashes. Root's new reads.rs scenario buffer becomes LIVE until its own reported STOP; DOMAIN's remaining seven buffers remain LIVE until its STOP. All formatter/compiler/tests/VM/Git/shared-document work stays held until BOTH authors are stopped. Root journals this narrow message adoption while coordination is otherwise held, and records the final custody normally after STOP. This changes who authors one already selected artifact, not the review/RED/SAVE/BODY gates. No source/test artifact was created or changed by me.

### R0 exact custody extension and actual stopped read-scenario review

Root reports DOMAIN explicitly offered BOTH guards.rs and recovery.rs as absent/unstarted/no in-memory drafts. Select ONLY those TWO already approved paths and their exact FIVE approved cases for Root authorship, after DOMAIN's explicit relinquishment and before Root creates them. Prior reads-only transfer remains. Final split is Root FIFTEEN paths (eleven production, existing target main, reads, guards, recovery) and DOMAIN FIVE (feature module, fixtures, fixtures/policy, wire, publication). No new helper/type/model/field/API/clock/constant/alias/case/assertion or semantic permission follows. DOMAIN retains every live wire/publication/policy draft; no moving draft is overwritten. Existing Root production STOP remains; Root scenario buffers are LIVE until reported STOP and all formatting/build/VM/Git/shared-doc actions still wait BOTH authors. This is an owner-only message-journal amendment, not a new release gate.

I actually WHOLE-read Root-stopped reads.rs162LF/7043B/a30647a22d480c7bc86824822644463d9bae9caa74e1f4a250587413e461c352. Its real post-constructor tails create payload b"FSV2\x02opaque", Cancel at200 with supplied empty error and terminalTTL50, and expect exact stored payload b"FSV2\x01FSV2\x02opaque". The old nonterminal payload retains independent expiry0; the new error expires250. At HLC/wall201 both are live;250 equality makes only error missing; HLC1500/wall249 makes error's250 expiry UnsafeDrift while payload remains live. Stable duplicate owners/missing/empty slots, exact clock count, failed clock/cap-before-clock and post-store/reference drop byte owners are meaningful literal controls. Every NEW positive result is explicitly asserted before conversion; initial R0 constructor refusal honestly masks all later writer/reader/pointer tails. No real I/O success was claimed.

I found ONE concrete integration-oracle defect before baseline: oversized=vec![b'x';65536] was already invalid key SHAPE, so a parser-only implementation lacking the routed65535 ceiling could pass the all-invalid zero-clock control. PERF independently found the same. Selected ONLY a same-case literal correction: build b"f:{f}:v:p:" +resized x ID+ b":1" to EXACT65536, and assert OLD parse_original_value_ref accepts that shape BEFORE the first NEW constructor assertion. This does not change the raw uncapped predicate; it makes the reader's separate source routing ceiling observable. No new helper/type/test/model or workload was introduced by review.

Root reopened only its reads buffer, then explicitly reSTOPPED. I ACTUALLY whole/hash-read the complete revised166LF/7255B/ec50989c11dc1013625ba41134b8d08f5f28183f22bf667638403e67ed03ae67. The length and old-parser premises precede the first positive NEW constructor assertion, the invalid/all-skipped list retains the shaped oversized owner, and the other expiry/duplicate/clock/error/budget/ownership observations are unchanged. Both functions are75LF before canonical formatting, no helper/packing permission; actual formatted geometry still needs checking. All other Root production/model hashes remain as above by Root's STOP report; DOMAIN's live files were not read. This is stopped-source acceptance with intended R0 tails, not observed PASS/RED or resource-proof/runtime credit. No remaining scoped semantic defect found in these two scenarios; production's two doc corrections and actual technical enum diagnostic remain pending the correct window.

### R0 wire geometry: two exact private helpers before creation

Select DOMAIN's concrete responsibility split in ALREADY approved tests/state_store/workflow_values/wire.rs, whose first case otherwise exceeds100 natural LF:
- presence_request()->TestResult<PreparedWorkflowValueBatch> (arity0): ONLY the OLD absent-reference literal/SHA premise and independent literal two-item birth/terminal input; no NEW encoder/decoder/apply/capture call.
- assert_presence_fields(decoded:&PreparedWorkflowValueBatch)->TestResult (arity1): ONLY independent literal defaults/policy/target/birth/terminal/items/payload assertions; no codec call or expected value derived from SUT output. Establish exact lengths/classes before indexing; no input lifetime escapes.

The unchanged first test owns explicit positive NEW encode/decode calls, whole-header/intent checks, server-context-vs-caller-intent controls, nonempty encoded data and donor-drop lifetime tail. Second reducer-mode case stays natural<=100 without a helper. Same12 test names, eight new test files, production13types/clock/six rejectors and all assertions remain. Total selected test helper functions18->20 (18 fixture +2 wire), plus the one clock method; no new model/path/alias/constant/generic helper. Final ordinary geometry still mandatory. Helpers are NOT created at this decision; Root's concrete message adoption releases this exact delta while other approved drafts continue. Only this architecture note was appended.

### R0 Root-scenario geometry: two exact private I/O helpers before creation

Select TWO helpers in existing approved test paths, for actual repeated responsibilities that otherwise exceed100LF:
- guards.rs: refused_value_batch(path:&Path,scope:&RequestScope,batch:&PreparedWorkflowValueBatch,at:u64,limits:WorkflowValueTransactionLimits)->TestResult<WorkflowValueTransactionError> (private arity5). Read actual primary rows/full manifest through the value-aware four-key fixture; open the REAL existing store; explicitly assert NEW encoder-positive; run actual apply; require Err; drop store; compare actual primary rows/full manifest unchanged; return the UNMAPPED actual error for each test's independent literal match. Callers release any existing database/store handle first. No desired-error parameter, seed/callback/copy donor or new model. Legacy three-metadata refusal remains outside this value-only helper.
- recovery.rs: remove_required_immutable(path:&Path)->TestResult (private arity1). ONLY after successful REAL snapshot capture/receive/install/read and handle drop, observe actual primary rows/full manifest; select an existing family8 key using independent literal b"FWPK\0\x01\x08"; a real Immediate redb transaction removes ONLY it; after dropping handles compare rows to exactly before-minus-that-key and manifest unchanged. The TEST owns the later actual reopen==Corrupt assertion. Missing target is a fixture failure, never fabricated deletion success. No alias/companion/origin/member writes or seed API.

Successful snapshot/owned-read assertions stay in the existing test; policy/limit input cases and explicit initial constructor positives remain at their test sites. Same12 case names, eight test files and production ABI; total selected test helpers20->22 (18 fixture+2 wire+2 scenario), plus one clock method. No new file/type/alias/constant/body API. Actual ordinary600/100 geometry remains mandatory. Both helpers are NOT created at selection; Root's concrete adoption releases only these responsibilities. This is precreation scope, not I/O execution or runtime acceptance.

### R0 publication geometry: scoped live guard and four literal actions

Select TWO private helpers in existing approved workflow_values/publication.rs before creation:
running_guard(store:&mut RedbStateStore,scope:&RequestScope,id:&RecordId)->TestResult<LeaseGuard> (arity3): ONLY real same-store GroupApplyStore begin_apply/record for the fixed fixture partition p and exact scoped id, assert running/fence1/nonempty lease, return the owned ordinary caller guard from its actual persisted token. This is authentic input acquisition, not a seed, authority grant or expected-output oracle.
assert_action(store:&mut RedbStateStore,scope:&RequestScope,case:u8)->TestResult (arity3): ONLY one finite literal Start7-to-action scenario via selected f::apply_value/running_guard, and independent current/history ref/lifecycle expectations. Cases0 equal payload+empty Complete result;1 omitted Complete payload/result;2 Fail with error E;3 Retry with error None. Unexpected case errors; no generic generated oracle. Testsites retain explicit initial constructor positives; birth/terminal case may reuse running_guard. Repeated Complete is the real no-op control, never duplicate Cancel.

Coverage limit is explicit: a fresh Start begins with absent result/error, so the omitted cases alone do NOT prove clearing a PREVIOUSLY nonempty result/error. Existing retry_transition requires a real running lease; no fake restored preimage/seed is permitted to manufacture stronger coverage. Genuine reclaim/rewind/clear carry coverage remains a required follower. Same three test names, source ABI/files and assertions; total selected test helpers22->24 plus clock method. Natural100/600 limits remain; no packing/new model/alias/constant/path. Root message adoption releases this exact delta; helpers were not created at this decision.

### R0 exact one-Create value-row and limit oracle

Select Root's independent literal in the existing guards limit case: ONE accepted Create with ONE supplied binary emits exactly FOUR value-related rows: immutable8, public alias9, current companion10, history companion11. Existing record/index/outcome/control are charged by the existing record_batch profile, not counted again as value rows. Thus maximum_value_rows3 is a genuine one-short atomic refusal;4 is positive followed by exact one-per-family8..11 assertions. Real manifest/default initialization precedes this batch and is not hidden fifth publication row. This fixes the one-item oracle only; future BODY must count actual coalesced current and EVERY-history publication work/rows honestly across larger batches, not infer a universal4n or free repeated writes.

Select max-value bytes/work/temporary1 refusal controls, independently resetting the other caps to sufficient values each time. All value-specific cap refusals use existing WorkflowValueTransactionError::Record(RecordBatchTransactionError::Transaction(StateTransactionError::Limit)); no new variant/type/helper or fake capacity failure. Real before/after refusal helper verifies no partial publication. These are logical operational caps, not exact capacity/allocator/RSS/initial-R measurements. Same case/helper names and every original assertion remain; actual source/runtime still pending.

### R0 exact legacy refusal helper/origins; stopped recovery review

Select ONE private guards.rs helper before creation: check_legacy_refusals(path:&Path,scope:&RequestScope,batch:&PreparedWorkflowValueBatch)->TestResult (arity3). REAL old genesis/placements/empty policies +old encode_record_batch/actual apply populate legacy including inline payload. Explicit positive new binding read must return LegacyUnavailable (NOT Known(None)); old inline bytes do not prove catalog provenance. New value encoder-positive/actual apply returns WorkflowValueTransactionError::UnsupportedProfile with legacy rows and OLD three-key root raw_manifest unchanged. Drop handles; attempting new create_value_database on that populated real database returns StateTransactionError::NamespaceConflict, preserving rows/manifest. Existing bootstrap::ensure_empty180..195 actually returns that exact origin when tables exist, which I read. No upgrade/seed/clone/callback/new type/path. The helper is LEGACY-only; never pass it to the four-key value refusal helper. Same case retains its explicit initial new fresh-constructor positive.

Also freeze Root's other two origins: old V5 mutation against value-capable store returns RecordBatchTransactionError::Transaction(UnsupportedLayout), wrapped naturally where the selected test calls it; an expired-terminal/cohort requiring unimplemented retirement returns NEW WorkflowValueTransactionError::UnsupportedProfile with no change. Unregistered contained mode128 is CodecError::UnsupportedTag. These are visible profile refusals, not fabricated NotFound/AlreadyExists/empty bindings. Same three case names; helpers24->25 plus clock method, no extra ABI or error variant. Root message adoption releases this exact helper/oracle.

I ACTUALLY whole/hash-read stopped recovery.rs201LF/9069B/2640bc28303e4c1b0e97fd9ff2f4930ce8294b44ac84af4fca6d904f2fa487fd. It installs FULL policy atlog0 BEFORE birth1, expects incarnation1<<32 and literal5000/22000/hot2/history40, updates policy2, preserves the SAME request replay3, then reopens/reads without recapture. That replay-before-current-policy check intentionally preserves the existing Rust retained-gate contract; it is not a new claim of full direct Elixir wrapper-order parity. Snapshot test performs actual capture/receive/install, independently expects2immutable/2alias/1current/2history rows, exact source/target primary rows+manifest, stored magic escape once/empty bytes and owned read after handle drops; approved helper deletes exactly one existing8row and actual reopen mustCorrupt. No semantic defect found. Current82/76/18LF estimates are PRE-format, not a geometry waiver; canonical100/600 remains mandatory and any further helper requires exact consultation. All tails are still factory-masked in R0; no I/O success or runtime credit. DOMAIN five live buffers and Root live guards were not inspected.

### Measured post-format R0 recovery factoring and old-intent correction

Root canonical formatting measured publication::assert_action101LF, recovery reopen107LF and snapshot115LF; no waiver. Select TWO private helpers in existing recovery.rs BEFORE creation:
recovery_policy(updated:bool)->PolicyPatch (arity1): ONLY literal input-donor construction. Initial max-active5000/typeTTL11000-history30/qTTL22000-history40; updated max-activeClear/qTTL88000-history80, with type retention unchanged. Actual policy installation/assertions remain at testsites and expected capture stays independent literals.
publish_snapshot_values(store:&mut RedbStateStore,scope:&RequestScope)->TestResult (arity2): ONLY fixed actual Create100(payload FSV2snapshot) then Transition100(q->waiting, supplied empty payload), via selected f::batch/apply_value. No seed or expected-result computation. Factorypositive, capture/receive/install/familycounts/read/drop/physical equality/corrupt controls stay in the test. Helpers25->27 plus clock method; no new path/type/case/API.

For assert_action's101LF literal case table, allow ONE meaningful local expected-ref binding (e.g. complete_replacements=[Some(2),Some(2),None]) reused by its Complete case. It names the logical independent expectation; each statement remains naturally formatted, no collapsed branches/deleted assertions/blanks/packing or waiver. Root must remeasure<=100. A new semantic worker would need separate approval.

Actual stopped guards had TWO old setup defects: ClientIntent::new(5,[0;32]) always refuses because current new() supports only1. Select existing ClientIntent::record_batch([0;32]) at exactly both old-frame construction sites; encode_record_batch freshly derives its real5 digest. This changes no new API/guard/case/assertion and prevents masked old-premise failures after factory GREEN. Root owns the narrow correction/reSTOP. Future resource/body/RED remains unclaimed.

### Measured R0 enum refinement after actual technical Clippy failure

I actually whole/hash-read raw9665090+1532/e3da0f17f22f865b5484ea5c678542fee4a6719fcbb9aca3c736f989c9bbecd2: actual large_enum_variant at catalog37 reports Known at least304B versus unit legacy. This is technical, not business RED; it supersedes only the earlier pending-lint shape decision. I also actually read existing architecture.md1412..1413 and ADR00682..83: ordinary Rust Box behavior is explicitly qualified, not an all-allocations-fallible promise.

Select EXACT Known(Option<Box<WorkflowValueBinding>>) in existing blob/catalog.rs. Not Box<Option<Binding>>: legacy and known-missing allocate no binding box. Same thirteen types/variant meanings/fields/API, no new helper/derive/dependency/file/waiver. Existing borrowed test matches auto-deref; no fabricated successful binding construction is added to R0. Six stubs stay literal errors, so no successful Box allocation executes now.

Later BODY must admit actual outer enum metadata on all paths, plus size_of<Binding> and ALL actual reference backing only for Known(Some). Construct the owned binding once and move it into the ordinary Box. This is explicit host-allocation behavior: no stable fallible Box::try_new/OOM guarantee, fake reserve failure, exact16B result/layout, allocator/RSS/speed or allocation-free conversion claim. Fallible Vec errors retain their actual lanes; do not pretend Box::new returns one. Root owns narrow source edit/reSTOP/hash inverse before quality rerun.

### Two concrete R0 oracle closures before baseline

Select ONE private publication.rs helper BEFORE creation: read_intermediate_values(store:&RedbStateStore,scope:&RequestScope)->TestResult<WorkflowValueReadBatch> (arity2). It builds only OLD renderer input refs i/payload1,i/payload2,j/payload1,i/payload1 under Explicit(p), samples the selected deterministic100/100 clock through the ACTUAL new read API, explicitly asserts that positive, returns its OWNED result. No expected value generation/seed/callback; renderer Debug-only errors get the existing test String adaptation. Parent V6 test calls before dropping prepared/outcome/store, then independently asserts literal A/B/C and slots0/1/2/0 AFTER those donors drop. Existing versions/incarnations/familycounts remain. Helpers27->28 plus clock method; no model/path/API/test change, actual100/600 remeasurement required. This closes a real gap: ref/count checks alone did not prove V6 intermediate BYTE contents, while snapshot already tested V5 bytes.

Also select same existing recovery case's stronger cross-log replay assertion after actual policy update: match actual f::apply_value atNEWlog3 as Retained, require Accepted, original_position==log1, original_time==20 and response[0]. The prior discarded generic is_ok could accept IntentConflict/Expired/LogReplay without detecting it. Same helper/case, no new type. Root applies ONLY after live technical4020 terminal; this is an oracle correction before intended RED, not runtime/BODY success. No source mutation by reviewer.

## VALUE-CONSUMED-BINARY1 finite final R0 source/runtime acceptance

All TWENTY final stopped files were actually whole-read/rebound; prior9800/15b6ed95 supplies unchanged source/beforeimage detail. Final changed semantic bindings: catalog75/3049/fecb51f64185c67f0fc212a0f3eb9220c886b99044e2a14eec8ccc8c87ac2c32; publication401/14619/b28506f9e3481266c74c81a8c4487c9dc6f8d8613bc1a2e2c1eb7c4f8fd090bf; recovery287/10508/cfb080d4d99f634147b6e43a144b5ccb9a3094a9652e36ac619b8122bffec670; fixtures311/10305/dc02d8cbc6effb141d9b24adce773657cb6a416177f39ce39333e2977d30b2c7; wire278/10673/e9cc5c59f145101d2f03bc51596fe489ce65fdecb18b4710bbadcd34a1efcecd. Policy305/dffd2418,guards416/59942ac4,reads196/41a05bcf,feature10/711e6ad7 rehashed/read unchanged. Root's held1598/a22b and full1618/e096 freeze is attributed to Root, not a second whole-tree scan by reviewer.

My corrected static census includes the tuple-array signature: EXACT41 functions=12 unchanged tests+28 approved helpers+clock method; max98 (legacy refusal), production max396/main543, no waiver/packing. Thirteen production types/one clock/six literal rejectors remain; original_frame is correctly private to its sole caller module. Optional Box exists only inside Known(Some); no successful binding construction executes. My scoped Box reverse restores ENTIRE3044/18cfbe68; reversing both selected docs as well restores ENTIRE3031/7d40d0fd. Canonical log396/16216/29d274ec6afba3f240218a5e0a93b993703ae65ed9a881c22ea18a2b787c8f9d exact seam removal restores ENTIRE16118/2e75f4d6. No unrelated parent body changed.

Actual oracle closures are present: shaped-valid65536 old-parser premise; real V6 read helper35LF followed by literal A/B/C and slots0/1/2/0 AFTER request/outcome/store drop; exact Retained/Accepted/original log1/time20/response[0] at newlog3 after real policy update. Scope/marker/generation/incarnation, mode1/5/6/7 fields, origin-preserving snapshots, actual family8-only corruption, independent expiry and one-clock/error/limits tails remain. Known-r Original SET uses the actual old public transaction, never raw seeding. Omitted fresh result/error still does not prove clearing prior NONEMPTY values. Real cleanup/rewind/recreation/typed/external/app hydration/routing/dual-OS/native/full endpoint remains required. No passing-storage or measured resource claim follows from compiled tails.

I WHOLE/hash-read actual9672914+5918/9387b67306b3136b1dd49f8a3c54074fad0fb14e087e97fbc349de5d441dedfe and its derived9673154+5653/1aa9f5a33c867b94ef19ff2938cec001228cf2900e386abe409a29e6190b003f. Own source/runtime Counter is EXACT12 names,0PASS12FAIL,0ignored/measured,105old filtered; EXIT101/UNCHANGEDTrue. Wire24/136 first encoder-positive receives UnsupportedTag; guards31/122/219,publication27/120/147,reads18/114,recovery29/124 first fresh-positive receives UnsupportedLayout. Old scalar/fallbackSHA/Original portable/empty-real-database premises before these assertions actually passed; shaped65536 premise also ran. Mode loop reaches ONLY profile1 before rejection. Decoder/later modes/new stored bytes/boxed outputs/guards/clock/limits/replay/snapshot/legacy/install tails are MASKED, not runtime proof. Earlier type/style/304B-enum failures were technical only and are not counted as this genuine callable RED.

I also actually read boundary9671800+1114/a8382297db116de676cb5b381541883445963e5ee8fa064d6cdf4e94cb75438e: after the previous run's terminal tail, all THREE TECHNICAL4 headers fmt/workspace-alltargets-check/strictClippy have their own EXIT0/UNCHANGEDTrue under1618/e096. Current old1093/docs/architecture/reference/progress/deny verification is a separate Root run, not established by this filtered target or three checks.

Written reciprocity is ACTUAL: DOMAIN47190+3797/0ff3eb1d and50987+4815/2b5744f0, PERF295123+3461/fa020dec and298584+2302/5462c8a4 were whole/hash-read. They agree on source/ownership/technical-vs-business/masking; older pending source counts are explicitly superseded by final41. No remaining scoped source/oracle disagreement found. Ordinary Root SAVE/next BODY selection is separate; no extra appendix/readiness acknowledgement gate. Original HistoryValues arrays remain empty; no BODY, successful publication, Linux/native8 or full-goal completion claimed. Only this allowed architecture note was appended, no source/workload/shared/Git action. Finite R0 review and own note writes STOP.


### STORED-LIFECYCLE-RESTORE1: exact scalar predecessor selected

Root's proposed micro-batch is selected before creation, separately from the still-unreleased VALUE-CONSUMED-BINARY1 codec BODY. Source witnesses actually read: `workflow/active/capture.rs` (private scalar fields, `validate_defaults`, `resolve_active`, and existing capture construction), and the existing `tests/stored_lifecycle/main.rs` target. The saved value R0 remains 7e344609; this predecessor grants no storage publication or recovery authority.

Exact three-path write batch, Root-only: `crates/wf-domain/src/workflow/active/capture.rs`, existing `crates/wf-domain/src/workflow/active.rs` reexport, and existing `crates/wf-domain/tests/stored_lifecycle/main.rs`. One new public ordinary function, no new file/type/helper/constant:

`restore_captured_lifecycle(max_active_ms: Option<u64>, retention_ttl_ms: u64, history_hot_max_events: u32, history_max_events: u32, defaults: LifecycleDefaults) -> Result<CapturedLifecycle, ActiveValueError>`.

Its selected final semantics are validation, then exact construction: (1) invoke the existing defaults validation unchanged; invalid configuration returns `InvalidCapturedRetention`; (2) validate the supplied optional active duration, preserving `None`, rejecting zero or greater than `MAX_RETENTION_MILLIS` with `InvalidCapturedActive`; (3) validate supplied TTL as positive and bounded, supplied history as positive and at most `defaults.max_history_max_events` (therefore also the source hard ceiling), and supplied hot as at most both that history and `defaults.max_history_hot_max_events`; these failures return `InvalidCapturedRetention`; (4) retain all four supplied scalars unchanged. Existing `resolve_active(..., MaxActiveOverride::Inherit)` and `RetentionOverride::validate` may be reused inside this one function. No clamping, policy read, current-policy recapture, default substitution, new byte owner, or I/O. The defaults argument is the recorded creation configuration/caps, not a license to substitute later configuration. The returned value is ordinary data; ingress/storage must independently establish the correct configuration and cohort relation.

Exact four appended test functions, with the old five functions and fixtures preserved: `stored_lifecycle_restore_preserves_exact_birth_scalars`, `stored_lifecycle_restore_accepts_infinite_active_and_zero_hot`, `stored_lifecycle_restore_refuses_corrupt_scalars_without_clamping`, and `stored_lifecycle_restore_revalidates_configured_defaults`. Each begins with an explicit expected-positive call to the new function before destructuring its Result. Independent literal getters cover all supplied scalars, finite/infinite active and zero hot. Rejections cover active zero/max+1; TTL zero/max+1; history zero/above configured cap; hot above total/above configured cap; the existing invalid-default categories, including defaults-first precedence when a captured active value is also invalid. No helper or output-derived expected capture is permitted. Four functions must each fit 100 physical lines and the existing 373-line target must remain at most 600; the cases fit direct scalar/array loops without new fixtures.

R0 function body is exactly `Err(ActiveValueError::InvalidCapturedRetention)`, with the selected signature/reexport and four real tests compiled together. Root must execute and SAVE genuine four intended positive-assertion failures before replacing that body with validation. Existing tests remain intact. Scalar BODY is a later Root release after that SAVE; codec files remain held through this micro-R0 workload. Logical scalar admission is not a claim about native stack size, allocator behavior, Linux execution, or any of the still-required catalog/hydration endpoints.


### STORED-LIFECYCLE-RESTORE1: actual R0 source and RED4

Actual stopped-source review binds capture259/9467/9103ba1fd61804ad69e437ca6c3ecce879df3793023bc86d4ef1a27c9e3cf487; active233/8356/6051b05e52b6f085c63a7de2a7869e13882cf147a6ea99aaf78a15052d958ae3; canonical main498/16746/d2d775a4b44061cbdd4ee542fa91919bfd265811a2944244ba29d79b154d9c66. I read both production files, every appended test and changed imports. Exact whole-file inverses remove only the new documented rejector, reexport and four test/import additions: capture243/8909/a18bf68a32065cbf7a769c3115e743993cf00f2f6028e5803e1ca52a7287637f; active233/8328/f3143f2b21ed9b450ea7d4582aa1f2ae23460cf52cf3a46a0ccd35b9c086aad2; main373/12394/da5ef001a9080cdd7a8793b62cd57af5a42950c18801a3552665453bf3cc2cbf. Thus all five old tests remain literal. The new four natural spans17/19/50/29 fit, and no helper/type/constant/validator BODY was added. Independent finite values differ from defaults; None/hot0, exact maxima, no-clamping refusals and defaults-before-active-before-retention are meaningful oracles.

I whole/hash-read PERF308947+2524/e4e3667539285760859c10329caa050ebe9d38b4c97fcf0bae2ed972b3e14b69; source/owner/error findings agree. Root's technical3 all-zero and held1615/50a5 proof are attributed, not an independently rerun workload.

I actually whole/hash-read raw9865757+1832/12ca2a05571b0114a712a84b0128d4801307dc3aac772b73bb05e3bf9e73b102 and full packet9865503+2111/d8df56470ba9f62901771af8da13bd77ba60e0afc8e2815bf25c2ebb0f294d19. The source-derived four-name Counter exactly equals runtime:0PASS/4FAIL,0ignored/measured,5oldfiltered, EXIT101/UNCHANGED under1618/cddcd40f. Every failure is the selected first `restored.is_ok()` with InvalidCapturedRetention at380/401/421/472; no setup/compiler failure is counted as RED. All later getter, corrupt-boundary and precedence tails remain masked until BODY. No scoped source defect remains. This finite micro-R0 review stops; Root alone owns SAVE and later exact validation BODY. Codec consultation continues separately, with no codec source release or endpoint credit.


## VALUE-CONSUMED-BINARY1-CODEC1: final finite precreation decision

This selects the next executable batch after saved value R0 7e344609 and the separate scalar-restoration checkpoint. Root adoption/release still controls authorship. It implements the two real owned codec endpoints and an actual generic DecodedRequest handoff; it does not replace the four remaining storage/read endpoints or the complete workflow goal. Existing twelve test bodies and their eight files/main stay literal except the explicitly selected one-line supplemental-module seam below. No catalog, key family, store capability or snapshot profile activates in this batch.

Source/consultation basis actually read: existing Reader/Writer, common header/item/count/key grammar, generic encode/decode/scan, owned and public operation/canonical workers, all four Fresh paths, claim admission, private CapturedLifecycle/default validation, the frozen wire/guard premises and R0 source. Existing log.rs is396LF; common164, encode216, decode221, scan130, record_batch367, fresh232, phase313, atomic-start codec277, claim apply336, public parent278/codec357/intent326. I whole/hash-read PERF300886+8061/4237f79562a054d3c4299414e8225678084ac7d39f71309e9720e7dc5d2c2ab4 and311471+2257/211f68c6727bfbcd6f646e5276e6581dc370e9dd308a813fc3699f736b920653. Its owner/linear-work/length-first constraints are incorporated; the earlier additive public-item term is expressly replaced by the Batch-OR-public rule below. DOMAIN's direct concrete worker/geometry proposals were consulted. This is the final decision, not a further request for a general design round.

### A0: supplemental callable RED before codec BODY

Root owns these FIVE paths for the small R0 extension: existing `crates/wf-ports/src/log.rs`, `src/log/common.rs`, `src/log/value_batch.rs`; existing `crates/wf-storage/tests/state_store/workflow_values.rs`; NEW `tests/state_store/workflow_values/codec.rs`. There is no new Cargo target, fixture module/type/alias/constant, policy classification or dependency. Append ONLY `mod codec;` to the existing feature module; all original twelve function bodies and other bytes remain unchanged.

Move the EXISTING CodecError enum and its Display/Error implementations WHOLE from log.rs into existing common.rs, preserving the public `wf_ports::log::CodecError` reexport and all old variants/messages/traits. Add exactly `WorkLimit` ("private prepared work budget exceeded") and `OwnedLimit` ("private prepared owned-byte budget exceeded") plus their normal Display arms; no suppression. This responsibility cut fits common naturally and frees parent space rather than packing log.rs. common's import must stop importing its own CodecError; log's now-unused fmt import is removed. No new error model/module is created.

Two new callable signatures in existing value_batch.rs, implemented as R0 refusals ONLY:

- `pub fn prepare_workflow_value_batch(encoded: &[u8], limits: CodecLimits, maximum_owned: usize, maximum_work: usize) -> Result<(DecodedRequest<'_>, usize), CodecError>`: literal `Err(CodecError::UnsupportedTag)`; normal log reexport.
- `impl DecodedRequest<'_> { pub fn workflow_value_metadata(&self) -> Option<(u8, Option<bool>, WorkflowValueDefaults, &[WorkflowValuePolicyWitness], &[WorkflowValueItemInput])> }`: literal `None`. This checked borrowing surface is ordinary data, not a storage grant. No sidecar/model/decoder BODY is created in A0.

The NEW codec.rs has exactly FIVE `#[test] fn ...() -> TestResult` cases:

1. `value_binary_codec_rejects_truncation_and_trailing_bytes`;
2. `value_binary_codec_rejects_footer_and_caller_intent_tampering`;
3. `value_binary_codec_checks_metadata_shapes_and_bounds`;
4. `value_binary_codec_owned_and_generic_decoders_preserve_distinct_ownership`;
5. `value_binary_public_retry_preserves_prepared_jitter_outside_caller_intent`.

Exactly TWO private helpers in that file: `codec_request() -> TestResult<PreparedWorkflowValueBatch>` and `reseal(encoded: &mut [u8]) -> TestResult`. The first uses only existing old scope/time/item/witness/default fixtures and literal mode5 Create(i,r,q,payload[0,255]), partition p, scope t/g/z, request wire, time100, one absent r witness, input0/policy0/Absent, and all birth/terminal/requested presences None. It calls no new codec/default/constructor API. The second independently hashes the prefix into the final32 bytes, checks length first, and never calls the codec. Each test's FIRST new call is explicitly asserted expected-positive encode, before Result conversion; new decoder/prepare positives are also explicitly asserted before conversion. R0 therefore has five genuine intended UnsupportedTag failures, with all corruption/ownership/bounds tails compiled but masked. Root executes and SAVES that baseline before A1 helpers/BODY. No hidden `?` through a new rejector.

Freeze independent duties: verify the complete286B minimal frame layout below, literal header fields/caller stream, SHA footer and whole PreparedDigest; reject EVERY strict prefix, trailing bytes, bad footer, re-sealed wrong caller intent and re-sealed caller-field mutation. Re-sealed generated stamp, valid witness digest and marker None/false remain ordinary server-data changes with the same caller intent. Test known bad body/mode/option/default/config-digest/count/index/alignment/target discriminators, duplicate/unsorted witness types and explicit byte/count limits. Footer mismatches/intent mismatches are Malformed; true trailing bytes are TrailingBytes; unknown body version is UnsupportedVersion; unknown contained mode/tag is UnsupportedTag; invalid Option discriminators are InvalidOptionality. Empty/excess counts are ItemLimit; misalignment/out-of-range witness index is Malformed. Each shape mutation is independently resealed where needed, so it reaches its intended parser check.

The ownership case uses both public owned and bounded generic decoders. Owned output retains nonempty fields after encoded/prepared owners drop; generic output retains its actual encoded borrow (do not drop that donor prematurely), ONE operation, correct metadata and derived keys. Compute requested ownership from actual public size_of terms below, test one-short/exact OwnedLimit and work one-short/exact WorkLimit. For this286B fixture, isolation21 +state-key41 gives K62, N=P=1 and selected generic W111232. This is a logical cohort oracle, not CPU timing. The fifth case builds a literal public RetryWithPolicy via OLD scalar/lease constructors: jitter11 survives owned decoding; changing only it to22 preserves caller intent while changing the full prepared identity and decoded scalar. No extra fixture helper is permitted. Natural file<=600/function<=100 remains mandatory; actual formatting is checked before compilation.

### Exact wire and caller identity

All integers below are unsigned BE unless explicitly signed. `raw` is u32 length +unmodified bytes; ordinary Option is byte0/byte1+body. No FSV2 parsing/escaping occurs here: outer value-format1 means logical-binary input. Later storage performs the selected exact one-level escape; a magic prefix is never an ETF discriminator.

Header is existing `FWCP`4 +u16(1)+operation-tag u8(1), raw tenant/group/recovery/request, u16(8), caller-intent SHA32, HLC physical u64/logical u16/wall u64. Preserve actual ClientIntent8 throughout; do not nest/relabel an old5/6/7 frame.

Body, exactly in order:

- u8 value-format1; u8 contained mode1/5/6/7; marker byte0=None or byte1 followed by bool0/1.
- Defaults60: ttl u64, default hot u32, default history u32, configured maximum hot u64, configured maximum history u32, then configuration digest32.
- P:u32; P witnesses in strictly increasing unique opaque workflow-type byte order: raw nonempty type, profile u8(0/1/2), generation u64, exact-backing digest32. P is positive and bounded by admission batch count and remaining minimum46B/row before looping. Do not infer current policy, source authority or correctness of an absent witness from this representation.
- For contained mode1 ONLY, evidence flags u8 with bit0 auto-partition, bit1 requested-time present, bit2 retry-override present; reject other bits; requested u64 follows iff bit1. Then, for ALL modes, N:u32 and N existing raw item headers: raw partition, raw id, priority u8, prepared command now u64, operation tag/body. N is positive/bounded and impossible counts are refused before looping. Mode1 is PublicMutation with exactly one item; modes5/6 are Batch; mode7 is exactly one StartAndClaim.
- Operation bodies reuse the EXISTING OWNED operation grammar for modes1/5/6, including tag8's jitter:u32 even in mode1. This is a NEW outer8 body; old root4/public1 bytes remain unchanged. Contained public1 permits only tags1/4/5/6/8/9/10; modes5/6 only1/4/5/6/7/8/9/10; mode7 only11. Claim/Reclaim/StepContinue do not become supported. Start11 is raw type/state/worker, lease u64, optional payload raw. Old owned tag widths/order remain exact.
- I:u32 must equal N; each Copy input is source_index:u32 equal to its ordinal, policy_index:u32<P, target0=Unspecified/1=Absent/2+u128 incarnation; optional birth; optional terminal i64; optional requested u64. Birth is active tag0=Inherit/1=Infinite/2+u64Millis, optional retention TTL u64, optional history u32, optional hot u64. Do not resolve/clamp birth fields or proactively call capture on attempted commands. Accepted-birth capture occurs later after real reducer success.
- Footer32 is SHA256 of ALL preceding frame bytes, including caller digest. Parse body, take exactly32 footer bytes, then require EOF; this preserves TrailingBytes. PreparedDigest is SHA256 of the ENTIRE frame including footer. Both hashes are separately paid.

Structural checks include exact mode/operation agreement, input alignment/index bounds, requested-time coherence, flags/option classes, canonical witness order and fixed defaults validity. They do NOT check loaded policy generation/content, captured target versus current type/incarnation or marker truth: None/Some(false) must reach the guarded store's existing selected errors. Policy generation/absence/content are checked under the same actual transaction, not by the codec. Recognized birth scalars remain ordinary caller fields until accepted-birth capture; no fake-policy/default recapture is an encoder validator.

Mode1 uses the actual prepared partition for length-only measurement, then verifies auto partition by the existing IEEE CRC helper after complete length admission. It checks evidence requested time equals input.requested_at presence/value and prepared now equals requested-or-stamp physical. Other modes require requested_at None. It does not reject a fitting partition from an imagined worst-case17B automatic spelling.

Minimal286B mode5 fixture offsets (zero-based): header0..81; format82; mode83; Some(true) marker84..85; defaults86..145; P146..149; witness150..195; N196..199; item200..237; I238..241; input242..253; footer254..285. Request wire makes caller SHA32..63, matching the frozen original control. Existing frozen twelve tests remain valid.

Canonical caller intent is SHA256 of EXACT stream: `b"WF-WORKFLOW-VALUE\0V8\0"`; raw tenant/group/recovery; mode u8; N:u32; ordered caller operations; N:u32; ordered caller extensions. Public1 emits the existing public canonical(tenant,normalized caller,emit) stream ONCE (including its own old domain/registry), not old measure/prepare wrappers. It has no extra raw item wrapper. Modes5/6 emit each raw partition/id then existing hash_operation4, with identical old caller-field presence/priority/explicit fence/state/version/lease/retry exclusions. Mode7 emits each raw partition/id, tag11, priority, raw type/state/worker, lease u64, optional payload raw. Each extension emits the birth stream and terminal Option exactly as above, then requested Option for modes5/6/7; public1 requested is already in its canonical stream and is NOT emitted twice. Checked streamed-length arithmetic applies; no canonical buffer is allocated.

Request identity, stamp/generated now, marker, defaults/configuration digest, witness list/content/profile/generation/indices, source_index and captured target/type/incarnation are EXCLUDED from caller intent and INCLUDED in full framing/digest. Existing mode-specific generated retry choices remain excluded from caller intent; mode1 jitter is nevertheless retained in this new full body. Explicit caller guards and override presence remain included. Re-preparing a retained user request under changed server context preserves caller identity; no target guard is silently promoted into caller semantics.

### Shared scalar grammar and A1 artifacts

Exactly TWO private types, no new public model/trait/alias: `ValueBatchMetadata` in existing log/value_batch.rs with ONLY profile:u8, marker:Option<bool>, defaults:WorkflowValueDefaults, policies:Vec<Witness>, inputs:Vec<Input>; and `ValueBatchCharge` in new budget.rs with maximum:usize and spent:usize (spent visible to its parent). The DecodedRequest field is private `value_metadata: Option<value_batch::ValueBatchMetadata>`, inline, no sidecar Box/Clone. Legacy constructors set None; metadata getter returns the checked borrow above. The same PreparedOperation/scope/request move exactly once from materialization into the generic request; no duplicate PreparedWorkflowValueBatch plus operation owner survives.

Root alone adds `LifecycleDefaults::validate(self)->Result<(),ActiveValueError>` in existing active/capture.rs, delegating the existing private validate_defaults unchanged. Existing capture and restored-scalar error order/semantics remain literal. No policy recapture/default clamp is introduced. Existing consensus/value.rs gains only TWO public inherent methods on WorkflowValueDefaults: `to_bytes(self)->Result<[u8;60],CodecError>` and `from_bytes([u8;60])->Result<Self,CodecError>`. Validate scalar defaults first; hash literal `b"wf-workflow-value-defaults-v1"` plus28 BE scalar bytes; require supplied digest exactly, never auto-repair it. Both bad defaults and config mismatch are Malformed. from_bytes reconstructs ordinary fields then uses the SAME check. No Reader/Writer dependency or new byte owner. Future FWSS7 manifest must use this same60B grammar.

Existing workflow/request.rs adds only public const `ClientIntent::workflow_value_batch([u8;32])->Self`, version8; existing ClientIntent::new continues accepting only1. This is ordinary identity data.

Exactly FIVE NEW private production files under `crates/wf-ports/src/log/value_batch/`; all functions are private to that feature except the explicitly crate-log-visible seams:

- `frame.rs`: `write(batch:&PreparedWorkflowValueBatch,intent:Option<ClientIntent>,writer:&mut Writer<'_>,limits:CodecLimits)->Result<(),CodecError>`; `preflight(encoded:&[u8],limits:CodecLimits,charge:&mut ValueBatchCharge)->Result<(usize,usize,usize,usize,ClientIntent),CodecError>` returning N/P/K/E/header-intent; `materialize(encoded:&[u8],limits:CodecLimits)->Result<PreparedWorkflowValueBatch,CodecError>` only after successful census/admission.
- `metadata.rs`: `write_policies(&[Witness],&mut Writer)->Result<(),CodecError>`; `scan_policies(&mut Reader,&mut ValueBatchCharge)->Result<usize,CodecError>` borrowed/no allocation; `read_policies(&mut Reader)->Result<Vec<Witness>,CodecError>`; `write_inputs(&[Input],&mut Writer)->Result<(),CodecError>`; `read_input(&mut Reader)->Result<Input,CodecError>` Copy only; `scan_inputs(&mut Reader,N:usize,P:usize)->Result<(),CodecError>`; `read_inputs(&mut Reader,N:usize)->Result<Vec<Input>,CodecError>`; `check(&PreparedWorkflowValueBatch,CodecLimits)->Result<(),CodecError>`. Adjacent ordering checks and checked policy_index are linear/O(1); no N*P lookup/sort/map rebuild.
- `operation.rs`: `write(&PreparedOperation,mode:u8,&mut Writer,CodecLimits)->Result<(),CodecError>`; `scan(&mut Reader,mode:u8,scope_length:usize,&mut ValueBatchCharge)->Result<(usize,usize),CodecError>` returning N/K and debiting item cohort BEFORE its loop; `read(&mut Reader,mode:u8,CodecLimits)->Result<PreparedOperation,CodecError>`; `read_item(&mut Reader,mode:u8)->Result<PreparedBatchItem,CodecError>`. Public1 reads directly into its ONE Box item, without a temporary allocated item Vec plus Box. Batch modes reserve only their item Vec. Start11 reading/writing is confined here.
- `intent.rs`: `derive(&PreparedWorkflowValueBatch,CodecLimits)->Result<ClientIntent,CodecError>`; `raw(&mut Sha256,&mut usize,&[u8])->Result<(),CodecError>`; `start(&PreparedBatchItem,&mut Sha256,&mut usize)->Result<(),CodecError>`. Public canonical streams directly once; byte counter includes all emitted chunks and checked length conversion. No old frame/intent derivation wrapper is called.
- `budget.rs`: ValueBatchCharge methods `new(maximum:usize)->Self` and `debit(&mut self,amount:usize)->Result<(),CodecError>`; `owned(E:usize,N:usize,P:usize,K:usize,generic:bool)->Result<usize,CodecError>`; `limits(CodecLimits,generic:bool)->Result<(usize,usize),CodecError>`; `reserve<T>(count:usize,maximum_count:usize)->Result<Vec<T>,CodecError>`; `admit(attempted:usize,maximum:usize)->Result<(),CodecError>`. Exactly five private constants BASE_WORK65536, ITEM_WORK4096, POLICY_WORK1024, BYTE_WORK128, KEY_WORK64. No Reader field/method, Bounds/Header model, budget alias or new Writer method.

Existing value_batch.rs implements the three public functions (two saved codec APIs plus bounded prepare) and metadata getter. Exactly THREE additional private functions there: `read_owned(encoded:&[u8],limits:CodecLimits,maximum_owned:usize,maximum_work:usize)->Result<(PreparedWorkflowValueBatch,ClientIntent,usize),CodecError>`; `read_decoded(encoded:&[u8],limits:CodecLimits)->Result<DecodedRequest<'_>,CodecError>` using derived closed caps and bounded prepare; `validate(encoded:&[u8],limits:CodecLimits)->Result<(),CodecError>` borrowed census only. No extra helper is implicitly permitted.

### Existing worker and generic handoff edits

Root moves only the error definitions described in A0. Narrow visibility/reuse changes in A1: encode::operation3 becomes pub(super), preserving its old signature/body; decode::operation3 and scan::operation3 are reused unchanged (new8 uses owned public=false). In public_mutation.rs, generalize ONLY with_caller3 to crate-log-visible `with_caller<T>(item:&PreparedBatchItem,evidence:&PublicMutationEvidence,limits:CodecLimits,inspect:impl FnOnce(&NormalizedMutation<'_>)->Result<T,CodecError>)->Result<T,CodecError>`. Its two old callers extract their existing PublicMutation parts and pass them; old root4 encoding/canonical bytes remain unchanged. Existing public codec::auto_partition2 and public intent::canonical3 become pub(in crate::log) with narrow parent reexports. Existing record_batch/intent::operation4 becomes pub(in crate::log), reexported as hash_operation. No old public operation writer/prepare wrapper is exposed; no old operation/policy_retry arity changes.

common::header accepts EXACT tag1/intent8 via the new primitive, and normal_recovery_header reports8; do not widen a version range or treat a header as full-body proof. decode_prepared retains byte-cap-first behavior and routes header8 directly to read_decoded, avoiding its old scan+decode census chain. All other supported roots retain the old chain. scan::preflight's explicit8 branch calls validate for old borrowed inspectors; legacy physical/profile preparations still refuse8. Old decode::read does not accidentally parse an8 body as an old count; explicit UnsupportedVersion on that private old path is acceptable/selected. New generic construction happens in bounded prepare after ONE preflight/materialize, moves the actual operation/header/metadata and builds actual isolation/state keys. Legacy decode and atomic-start constructors initialize metadata None. claim/phase destructuring/drop groups explicitly account for the added field; no source owner is retained accidentally. value_bytes recognizes StartAndClaim under actual8+contained7 as well as old7. No Fresh/reducer/retained validator rewrites actual intent to an old version.

### Prospective work, owner and error contract

One checked continuous ValueBatchCharge covers the complete new operation. Logical unit is one bounded visit/control event and each actually copied/hashed/compared byte; conservative cohort prepayment is intentional, not native CPU/RSS or exact visit-count measurement. Decoder cohort is `W=65536+4096N+1024P+128E+64K` (K0 for public owned decode; actual derived isolation+state-key lengths for generic). Debit fixed+128E BEFORE scanning; P cohort BEFORE witness loop, N cohort BEFORE item loop, and64K BEFORE any generic key allocation. No reset/refund between census/footer/materialization/default/caller/full hashes/key construction. Constant-header peeks are included; no second whole census/decoder is allowed. Direct public canonical once is mandatory: existing114 dispatches and24 backoff-body visits/normalizer bounded scans are covered, not assumed free. Actual post-STOP code must be recounted against this table.

Public encode prepays the same closed cohort using D=admission.decision_bytes as the byte UPPER bound, actual N/P and K0. It is deliberately not actual-E spend. First frame::write(None) performs complete length-only measurement using represented partition bytes, zero32 intent/zero60 defaults, counts/lengths/discriminants ONLY: no source-byte comparison, policy ordering, CRC, normalization or SHA. Bound the WHOLE frame including32 footer before those operations. Only then run metadata checks/default SHA/auto CRC/direct caller hash. This avoids the actual old public writer's CRC-before-cap hazard and N*D aggregate hazard. Some(intent) emits the checked body into ONE fallibly reserved final Vec(body+32), verifies actual length/capacity, appends footer within admitted capacity, then hashes entire frame. Incoming prepared owners are not cloned or called new acquisition R.

Decoder requested ownership, with E as an explicitly conservative leaf upper rather than a new Reader counter: H +N*sizeof(PreparedBatchItem)+N*sizeof(Input)+P*sizeof(Witness)+2E; generic additionally N*sizeof(Vec<u8>)+K. H is sizeof(PreparedWorkflowValueBatch) for owned decoding, and that header PLUS sizeof(DecodedRequest) for generic transient move assembly. The item term is EITHER Batch spine OR one public boxed-item pointee (public N1), never both; no temporary public Vec. Generic DecodedRequest includes the inline sidecar header ONCE, not thirteen fictional resident models. 2E covers copied leaf occurrences plus potential identity Vec-to-Box coexistence; borrowed encoded capacity is incoming-owned. Before materialization admit the complete bound; then one fallible exact reservation per admitted Vec. Allocator overprovision, Vec-to-Box shrink/reallocation and ordinary public-item Box retain existing qualification: no stable fallible Box/OOM/RSS theorem or capacity==request claim. Limits derives checked closed wrapper caps from validated D/batch counts and K<=D; public wrappers use those caps, bounded prepare uses caller's explicit caps and returns its SAME account's successful spent.

WorkLimit is distinct from malformed/frame byte caps; OwnedLimit is distinct from frame ByteLimit. Arithmetic overflow stays LengthOverflow, reserve failure Allocation. Later storage maps ONLY the new value-owned/work refusals to its existing StateTransactionError::Limit origin and subtracts the actual returned spend before continuing its value work; it cannot restart a fresh full allowance. No per-policy/account reset is authorized.

Adding the metadata Option changes legacy fixed shape even for None/replay/reject. Select conservative NEW additional header allowance X=sizeof(DecodedRequest<'static>), explicitly NOT a second actual allocation or exact layout delta. Preserve the old D/I admissions FIRST and their attempted values/error order; then admit D+X/I+X before construction/coexistence. No unnamed2048/512 slack pays it. One exact new private function in EXISTING record_batch.rs: `admit_decoded_request(encoded:usize,count:usize,limits:CodecLimits,workspace:usize)->Result<(),RecordBatchApplyError>` retains old decoder_bound admission then checked D+X. Existing prepare/check sites reuse it. fresh::inputs retains old I then I+X before staging; value_effects_budget includes X in its actual live base. Complete retains old D+F check then D+F+X. claim's existing decoder checks similarly retain old bound then +X. Drop X after decoded owners actually drop; no blanket late-response increment. This conservative operational profile refinement is not a performance claim.

Root-only existing-test integrations, no new helper/case/type or weakened original assertion: ports `tests/record_batch/{main,phases,point,replay,touched}.rs`, `tests/independent_batch/main.rs`, `tests/public_mutation/wire.rs`, `tests/record_batch/atomic_start/response.rs`, `tests/claim_phase_codec/apply.rs`; storage `tests/record_batch_transaction/public_mutation.rs` and `tests/state_store/claim/limits.rs`. Rebase only actually affected exact successes with public size_of<DecodedRequest>, preserve all old early D/I negative controls and expected attempts, add new-threshold one-short/positive where applicable, retain business/replay/response/pointer/call-order/no-publication assertions. Do not add X to a genuinely post-drop phase. These paths are the exact boundary-integration scope, not broad test-edit permission. No HistoryValues SourceCase array/progress coverage changes are granted; same original identities remain.

### Disjoint A1 ownership and following integration gates

After A0 SAVE and scalar BODY completion, DOMAIN owns exactly EIGHT production paths: existing domain `workflow/request.rs`; ports `consensus/value.rs`, `log/value_batch.rs`; and the FIVE named new private children. Root owns existing domain `workflow/active/capture.rs` (both scalar predecessor and later validate seam; never concurrent DOMAIN ownership); ports `log.rs`, `log/common.rs`, `log/encode.rs`, `log/decode.rs`, `log/scan.rs`, `log/public_mutation.rs`, its `codec.rs`/`intent.rs`, `log/record_batch.rs`, its `intent.rs`/`fresh.rs`/`phase.rs`/`atomic_start/codec.rs`, `log/claim/apply.rs`, and `log/public_mutation/apply.rs`; plus the ELEVEN exact old-test paths and the two supplemental-test paths. Root A0 value_batch ownership transfers only at its stopped, journaled A1 release. Neither writer touches the other's live paths. All formatter/compiler/workloads/shared edits wait BOTH STOP. Production400/test600/function100 are actual mandatory checks; an unselected helper/type/file requires concrete consultation, not packing. The moved-error cut and new children provide natural room; estimates are not waivers.

Expected scoped sequence is new5 callable RED SAVE, then codec bodies/generic handoff and actual seven pure cases (original2+new5) GREEN. Ten genuine factory-first cases remain honestly unsupported/masked until the connected implementation below; that is no endpoint or original3 credit. Root runs real conservation/quality for each stopped phase.

Following REQUIRED connected batch remains named VALUE-CONSUMED-BINARY1 storage integration, not a substitute inspector: actual six-argument fresh genesis in existing redb/bootstrap+core_policy/catalog, atomically writing origin/current-policy catalog/FWSS7 defaults/value capability; no separately acknowledged old3-key bootstrap then upgrade. Extend closed key/value/inventory/cohort/recovery dispatch for EXACT families8/9/10/11, profile7/policy1/value1/FWSP4, retaining FWSS3 origin and family6/7-only policy census. Fresh capability permits real policy bootstrap before first birth; populated legacy is never promoted. Existing core current/history codecs remain legitimate inline durability, not reconstructed missing historical refs.

The real transaction/value_batch route must use this bounded decode, SAME loaded guard/schema charge and accepted plan/consumed commit. Split observations' borrow by actual fields; retain the SAME policy account after load and admit its real fixed owner. Capture ONLY successful Created intermediate postimages; verify target-before-policy, marker, generation/content/profile against exact borrowed retained donor bytes, and defaults against genesis. Move Copy presence/default/guard inputs before existing decoded-drop helpers. Outer8/contained1/5/6/7 must be coupled through Fresh checks, value spine, observations and retained-summary/recovery validation while retained/header identity stays8. The persistent value-aware outcome registry must preserve contained-mode interpretation, not guess it from a numeric version range. Birth restore uses the real validated scalar constructor and stored creation defaults, not later policy recapture.

Existing commit/independent writes EVERY intermediate history and final current; add immutable bytes +public alias+current/EVERY-history companions inside that SAME transaction before control/Immediate commit and reply/into_parts. Keep/Clear/Replace is event-specific, including Complete result omission/Fail-Retry error omission, Cancel reason lane and carried other refs. Use actual prepared partition, birth state-entry ordinal/incarnation and separately mutable effective terminal TTL/override presence/requested-vs-apply time. Catalog bytes are source-neutral stored logical binaries with exact one-level escape and independent expiry. No duplicate retained decoded operations/redecode or detached seed/writer grant. Missing required acknowledged local rows are corrupt, even if physically retained expired evidence; bytes/identity immutability does not forbid explicit source retention refresh/cleanup.

Actual ordered persisted reads use one backend snapshot plus one authenticated HLC/wall clock pair, strict routed-ref filter, stable unique owners/ordered slots, missing/unsafe/operational origins and byte/work/temporary admission. Returned bytes own their backing; header-only omission does not claim all backend payload I/O disappears. Reopen/snapshot capture/receive/install/restore and wf-node closed recovery must agree FWSS7/FWSP4 before capability acknowledgement. All ten frozen real-store cases must actually reach their I/O tails. Genuine cleanup/rewind/retirement/recreation, typed/encoded/named/shared inputs, routed HistoryValues projection/omission/error semantics/original3 observations, stronger read routing, Linux/native8 and full-goal conservation remain REQUIRED followers. Codec GREEN grants none of those claims. This finite named decision ends consultation; Root can adopt/release the exact batches without another general acknowledgement gate.

### Scalar BODY actual closure accompanying this decision

I read the stopped restore body and hash-bound277/10115/5ce4f1bc7924a018db36d74c4cc120605f2f48d819a71ef0e8497007df4460bb. Exact restoration-function reversal recovers ENTIRE micro-R0 capture259/9467/9103ba1f. It calls unchanged defaults validation, resolve_active(Inherit), configured-cap RetentionOverride validation, hot<=configured cap/history, then constructs all four supplied scalars unchanged;27LF, no new helper/owner/recapture/clamp. Reexport/tests/fixture remain frozen by Root's report. I whole/hash-read actual9868832+1018/316cbff36f51878db4f385ce675dd22737680e0ca9b734e5f21df12496d69e66 and independently matched old5+new4 source/runtime names:9PASS0other. All former four RED tails now execute. Root reports focused10/all quality0 under unchanged1618/afddcf62; those additional commands are Root-attributed. I also actually read PERF313728+1608/dd02227612de565fe24ac4f40a5332a34fea96e106b0a50747cf8db0fa5b9d0b, closing micro-R0 written reciprocity; PERF directly reports independently reviewed BODY/inverse and same9/10 result, with its final written BODY receipt ordinary later work. No remaining scalar source defect or new codec authority. Only this allowed note was appended; no source/test/Git/workload was changed/run by me. Finite STOP.


### CODEC1-A0 measured tuple-lint refinement

Select Root's exact terminal-Clippy refinement before creation: ONE new public ordinary borrowed `WorkflowValueMetadata<'a>` in EXISTING `crates/wf-ports/src/log/value_batch.rs`, reexported through existing log.rs, with exactly public fields `profile:u8`, `policy_references_captured:Option<bool>`, `defaults:WorkflowValueDefaults`, `policies:&'a [WorkflowValuePolicyWitness]`, `inputs:&'a [WorkflowValueItemInput]`; derive ONLY Clone,Copy. It has no constructor, owned byte/vector, validation/proof authority or allocation. Shared references and scalar defaults copy without cloning their donors; fields remain fabricable ordinary data. Existing `workflow_value_metadata(&self)` returns `Option<WorkflowValueMetadata<'_>>`, lifetime tied to self, still literal None in A0; later A1 owned private ValueBatchMetadata stays separate and moves the same owners. Replace only the supplemental ownership test's tuple use with those named fields, preserving every assertion. This resolves the actual type_complexity diagnostic without suppression or another file/helper/type alias; no layout saving is claimed and decoder ownership still uses actual full shapes. Also select the SAME-case full `generic.encoded()==encoded.as_bytes()` assertion IN ADDITION to pointer identity (a prefix can share the pointer), and the first case's independent literal defaults28/configuration SHA plus absent-witness generation8/digest32 assertions. All five test names/two helpers/original twelve cases remain; these strengthen already selected duties before first behavioral baseline. Initial technical failure is not business RED. Root owns these stopped-source refinements after terminal59921 and must reSTOP/rebind; no A1 parser/sidecar/BODY release follows. This paragraph is the exact narrow amendment, not another general approval round.


### CODEC1-A1 unchanged old hash worker: counter precision

Select the author's concrete correction before BODY: retain existing record_batch::intent::operation4 BODY/hash stream and its VARIABLE-leaf-byte counter unchanged. New8 intent::derive's private checked size counter is a CONSERVATIVE upper, not an exact emitted-stream length: before each supported mode5/6 worker call, checked-add a literal256 fixed-byte allowance; use a fresh local variable counter for that old worker and checked-add its resulting leaf count. Current supported old arms have fewer than64 fixed tag/prefix/option/scalar bytes (PolicyRetry at most59), so256 safely bounds them; no extra helper/type/file/alias/constant. New raw/start/public emit and caller-extension chunks count their own bytes normally. Exact canonical HASH bytes, new286B/78B caller golden, all signatures and W prepayment remain unchanged. The earlier phrase that the reused counter itself counts all emitted chunks is superseded; no old caller/admission semantics or checksum/intent stream is rewritten. Actual post-STOP census must check the fixed upper as usual. This resolves the narrow source-backed issue without holding A0 RED/save or reopening the general design.

Counter-bound precision: the conservative canonical counter is used ONLY for checked overflow and the already prepaid work accounting; it is NEVER compared with raw frame ceiling D. Full framing independently enforces D. Thus the required exact-D286 frame with its78B actual caller stream is not falsely rejected by the256B fixed-operation allowance. No separate canonical ByteLimit or new bound is introduced.


### CODEC1-A0: actual final source and genuine RED5

I whole-read all FIVE stopped paths, then read/rebound each changed view/oracle revision only after Root STOP. Final LF/B/SHA256: log345/13863/afd6ad95e4b17d81cd8fd7b9eee0f82f8a5b8b7e761685443d10d33e747d8ae5; common224/8508/f49b572ff9f0c7ab811e7ea198f83657af40017d327fb17ba5ff8d280129eb81; value_batch66/2199/8a1c2d9619e1fce89c2598abf5c444d5b39d4cbdd0365996f3058639229c4546; feature11/194/d04d1bd31d1f6e214bbfd7d79430e434840e48d4a9c426492c89836c15bf13cf; codec460/18075/377ba476524d365b5ddb17ab9ab3f02583336090b05e602d5989d940bb924aa6. Own whole-file inverses restore original log396/16216/29d274ec6afba3f240218a5e0a93b993703ae65ed9a881c22ea18a2b787c8f9d, common5765/2cb6d1e46323db52bb75060a2db186ddde3faa5deef937f42b3b294132ee6361, value27/820/ee72e8f130f3d81656790bc0193a79005a074e144400b69f004c1ee86486c8e2 and feature183/711e6ad7139aca5d368f0d0822f92a934d81f19e46056ed2008f3a768d6edb05. Original twelve bodies/other eight files are frozen by Root's held-input proof; no shared/source mutation was performed by me.

The implementation is EXACT A0: existing two codec refusals plus new bounded UnsupportedTag refusal, metadata getterNone, selected borrowed five-field Clone/Copy view, whole old error relocation and two limit variants. No A1 sidecar, parser/budget/default/canonical BODY, duplicate operation, value storage or authority token exists. Natural test/helper spans90/60/87/98/64/21/6 match my static census. Literal286B header/body/defaults/witness/caller/footer identity is independent; caller78B SHA394e6045, default hashdd630bd1, K62/W111232 agree with the selected contract. Nonempty coexisting payloads/full encoded slice+pointer/post-drop fields make ownership checks nonvacuous. Actual full equality closes PERF's prefix-pointer gap. Re-sealed second-witness type200 r/q closes my decoder-ordering gap without changing caller identity; exact9LF reverse recovers ENTIRE18dc261b. Final equal-byte spelling reverse restores ENTIREdf78f1d0. No old assertion/helper/case was removed.

I actual-read technical1037/23c13cfb: tuple type_complexity was a technical failure, resolved by the selected ordinary borrowed view. Root's later byte_char_slices failure is also technical; changing only the equal-byte source spelling resolves it without suppression. Neither is counted as RED.

I whole/hash-read actual9892145+3200/894b7da7503a8ac379676ad1e9824d5e420d8385cc84a6f63e21142d424d2d33, plus the adjacent actual step6 header1619/6bfab00e and25B EXIT101/UNCHANGEDTrue footer (SHA28d4369885d777ff4f92697b072b5d5368138d1bd82450506a4406852f3fba59). Own source/runtime Counters match EXACT five selected names:0PASS5FAIL,0ignored/measured,117filtered. Each failure is the intended FIRST encoded.is_ok assertion29/121/183/272/395 with UnsupportedTag, after old fixture premises; no import/setup/compiler failure is substituted. All malformed/generic/default/ownership/limits/jitter tails remain masked. Root reports preceding five final technical commands0/unchanged, honestly attributed rather than rerun here.

I actually whole/hash-read PERF322540+4947/e016fb17c2c065d7e2e44e795b9ce383184827d31e1006b84bd893e68afb481d; full source/inverse/oracle/owner findings agree. It accurately left the final runtime pending at append; my actual runtime read above closes that later evidence without claiming the peer had already read it. No remaining scoped source/oracle issue. Root owns SAVE and later exact A1 release; current scalar checkpoint is07994f2, and no codec BODY/storage/hydration/Linux/native/original3 credit follows. Only this role note appended. Finite review/note STOP.


### CODEC1-A1 routing: prospective fixed control and one charged core

Select the concrete existing-entry refinement before source creation. Parent `decode_prepared(encoded,limits)` delegates unconditionally to EXISTING selected `value_batch::read_decoded2`; it performs no header peek. read_decoded owns routing: first the existing cheap complete-encoded byte-cap check; then create ValueBatchCharge with the CLOSED bootstrap maximum BASE_WORK and debit BASE_WORK BEFORE inspecting magic/length prefixes/digest/stamp/version or other header bytes. A block-scoped common::header probe yields only the8/non8 branch fact. That header performs bounded controls/fixed-byte reads and borrows variable slices; it performs no variable byte walk/hash/allocation. Non8 executes the SAME old scan::preflight→decode::read chain, preserving its byte-cap/header error order and leaving old roots/formats unchanged. Closed value-profile cap arithmetic occurs ONLY after recognizing8, so a huge caller D cannot introduce an overflow refusal into an otherwise unchanged legacy route.

For8, derive the selected closed owned/work caps and extend ONLY this SAME charge.maximum after checking its already-spent BASE_WORK is within the new cap; preserve spent exactly (no reset/refund). Narrow the existing budget field visibility to `pub(super) maximum` so only its feature parent can perform this explicit bootstrap-to-closed-cap transition; spent remains feature-parent visible as already selected. No public field, method, model or authority token is added. Then reuse ONE genuinely necessary new PRIVATE helper in EXISTING log/value_batch.rs: `prepare_charged<'input>(encoded: &'input [u8], limits: CodecLimits, maximum_owned: usize, charge: &mut ValueBatchCharge) -> Result<DecodedRequest<'input>, CodecError>` (arity4). The output lifetime is tied ONLY to encoded, not the charge. It owns the already-selected single preflight→owner admission→materialize→canonical check→metadata/key assembly sequence. It introduces no additional source pass, owner, callback, trait or frame and prevents duplicating generic assembly in two wrappers.

Public bounded prepare4 performs byte-cap-first, creates its caller-capped charge, debits BASE_WORK before any header read, invokes this SAME helper and returns that charge's spent. read_decoded2 invokes it with the retained bootstrap charge and returns the decoded owner. All OTHER preflight callers (read_owned4 and validate2) likewise perform byte-cap-first and debit BASE_WORK at entry. Consequently frame::preflight3 receives an already BASE-funded account and MUST NOT debit BASE_WORK again; its existing128E/P/N stage fees remain prospective and continuous, with64K before generic keys. No caller invokes a second census or a new independent account mid-operation. Existing signatures/arities remain unchanged except the ONE named added private helper; no file/type/constant or decoder/model/source authority expansion. W remains65536+4096N+1024P+128E+64K; the exact fixture111232 is unchanged. Root owns the parent routing edit; DOMAIN owns the feature/budget/preflight changes within its existing paths. They coordinate that narrow interface without reading or overwriting moving peer buffers. This is the code-ready resolution of the actual routing question, not a general approval/ACK gate or a change to A0 saved oracles.


### CODEC1-A1 measured intent responsibility cut

Select the author's concrete prospective115LF geometry correction BEFORE creation: ONE additional PRIVATE `extensions(inputs:&[WorkflowValueItemInput], mode:u8, hash:&mut Sha256, size:&mut usize)->Result<(),CodecError>` (arity4) in the ALREADY selected `crates/wf-ports/src/log/value_batch/intent.rs`. Move ONLY the existing ordered caller-extension emission: N:u32, each birth active/retention/hot presence and values, terminal signed Option, and requested Option for nonpublic modes (public1 already emits requested time once in its caller stream). Its checked borrowed emit closure moves with that responsibility. derive2 retains fullscope/mode/caller dispatch/public callback and unchanged old-worker256 counter handling; raw3/start3 remain selected. No allocation, additional traversal, hash byte/order change, defaults/policy/current-state validation, new field/model/file/alias/constant or counter/raw-D comparison. Existing N/work cohort already covers this SAME loop; actual post-STOP source census remains required. Both natural functions must fit100LF and the existing child400LF; no packing/waiver. Root's ordinary amendment adoption releases this one helper within DOMAIN's existing path; other authorized source work may continue without another readiness cycle.


### CODEC1-A1 actual stopped scan entry: fund every routing peek

Actual Root-stopped scan::preflight still reads common::header before its8→validate branch. PERF and I identified that this direct old-preparer entry's routing peek is not prospectively covered by validate's later BASE debit. Select this exact correction BEFORE helper creation: ONE new crate-log-private `legacy_preflight(encoded:&[u8],limits:CodecLimits)->Result<(),CodecError>` (arity2) in EXISTING Root-owned `crates/wf-ports/src/log/scan.rs`, containing the SAME old non8 structural grammar. Existing preflight2 becomes an unconditional delegation to value_batch::validate2 and performs no header read. The legacy worker explicitly refuses unexpected8 before treating its body as an old count; other roots/order/field/EOF checks remain unchanged.

Refine the ALREADY selected DOMAIN-owned validate2: byte-cap first, closed BASE bootstrap/debit BEFORE magic/prefix/version/header inspection; non8 calls legacy_preflight;8 extends the SAME account's maximum without changing spent and invokes its single charged new-frame census. Existing read_decoded2's non8 branch calls legacy_preflight DIRECTLY under its already prepaid BASE, rather than reentering the routing preflight/validate pair. Thus there is no recursion, additional routing account/reset, double BASE debit or second new-frame census. BASE covers the added bounded routing controls; the old non8 data grammar retains its original admission profile and is not falsely claimed newly repriced by that fixed fee. All new8 dynamic stages remain under the selected continuous W. No new file/type/model/alias/constant/public API or other arity; helper visibility only reaches crate::log and descendants. Root owns scan edits, DOMAIN owns validate/read_decoded edits within existing live paths; actual stopped-source recheck follows normally. This resolves the concrete unpaid peek with an explicit entry change, not a retroactive comment or an additional general consultation gate.


### CODEC1-A1 Root-only stopped-source review

I actually read all SIXTEEN Root production seams, every changed ten-test hunk and the unchanged storage Claim peak. DOMAIN's eight live files were not read; this is a Root-half review before combined canonical formatting, not parser/encoder or compiled acceptance. Root bindings actually hashed: capture287/97120a83; log348/89b9d442; common226/fede0ef9; encode216/07ecaa30; decode225/d4c2d999; public parent277/bd16e021, codec365/0bcbb810, intent326/428a2969; record parent375/3de9ce29, intent218/3f706249, fresh235/7e824e7d, phase314/a05ec4cc, atomic codec278/59b372a0; public apply157/70e8864f. After Root's narrow reSTOP, actual scan137/4192/8a591544bfa31826c28b3ae29c9836257a05700c3b83de4f0cf48202fd5e4202 and claim343/12121/77519ac2066fbf2997f9ae1b64b9bc8527a3d8488d7135876731f8c1694f03c4 are bound. The latter adds only the explicit size_of import to the whole body already read; its exact inverse is341/520b82e6. All counts here are PRE-format, with no geometry waiver.

Root's actual parent decode_prepared has no header peek and delegates to selected read_decoded2. common registers EXACT tag1/intent8 while retaining all old profiles. Old owned decode refuses8 before allocating isolation/command owners, old Fresh checks remain1/5/6/7, and Start value_bytes recognizes actual8+metadata7 without relabelling intent. Existing encode operation/canonical worker bodies preserve old wire/hash semantics; visibility and the four-argument borrowed caller adapter introduce no operation clone or normalizer owner. Capture's new LifecycleDefaults::validate merely delegates unchanged private validation; its exact whole inverse restores277/10115/5ce4f1bc. Common's inverse restores8508/f49b572f, and public canonical visibility inverse restores10228/9292661f. Existing lossless scalar restore/error order remains intact.

The actual unpaid scan peek was a substantive finding, now CLOSED on Root's half under the recorded2012/60d871 selection: scan::preflight only delegates; legacy_preflight preserves old non8 grammar and refuses8 before old count interpretation. Matching validate/read_decoded BASE-before-header and single-census behavior belongs to DOMAIN and remains unverified until its canonical STOP. This distinction is retained rather than treating a later fee as prospective admission.

Legacy owner integration is source-correct: old D admissions first, then conservative D+X before materialization/check; old I first, then I+X before staging; effect-builder live base includes X; Complete retains D+F then D+F+X. Claim also preserves its old returned base while admitting X at each real coexistence. Actual operation/key/filter/value-metadata drops precede late response bounds, and no post-drop X is charged. The unchanged storage Claim test's tight equation describes that post-drop peak; source retain() confirms the release point. No source behavior/reducer/public response/current-policy authority is fabricated by this fixed shape allowance. The added borrowed/default interfaces are ordinary values, not grants or exact native allocation/RSS claims.

I whole/hash-read text-only9895370+16938/6abaa2cc7aec6bba71ea36da56cac92849d33b6a38e2ece775d3d5cd38ba900c, then independently reversed EVERY hunk against the actual stopped files IN MEMORY. All ten complete current hashes matched the packet and all ten ENTIRE reconstructed A0 files matched its before hashes; each file's old/new #[test] name Counter also matched. The original D/I refusal attempts and all business/replay/response/call-order/no-publication assertions survive. Exact decode-positive ceilings are shifted by X to reach their original later stage, with separate new-threshold one-short controls; the storage public case truthfully retains F-only rather than a fictitious row. No source was written by the reverse audit. The intentionally unchanged storage Claim limits535 hashes fd42fe500c3d552ba1464a0efed46c97d6d838c54901a20a64be50ecd2735e99. No SourceCase role/progress credit or testcase normalization is inferred.

I actually whole/hash-read PERF329793+3412/7a05a6749a2b9d5815348b3a25a454af5bf8e96ad35aaaabff978aa713729c62; independent Root-half/ten-whole-inverse/drop/routing findings agree. No remaining scoped Root-side issue. Actual combined source/fee/geometry and all runtime/conservation checks remain AFTER both writers' STOP; DOMAIN has now reported STOP but Root's canonical handoff is still pending, so its new8 files remain unread here. No formatter/compiler/VM/test/Git/probe/source/shared-doc action occurred; only this allowed note appended. Finite Root-only review and own note STOP.


### CODEC1-A1 real semantic regression: reducer priority and unresolved presence

I verified actual `workflow/indexed/decision.rs237..247` and existing independent_batch assert_index_refusal396..404: absent Create/Start priority3 becomes reducer `IndexRejection::InvalidPriority`; V6 retains its error hole and later successful histories. The old typed5/6/7 codecs carry raw u8. New metadata::check and operation::scan instead reject >2/nonbirth nonzero as frameMalformed; the author confirms this was an unselected interpretation. Their birth/terminal operation-context rejection likewise exceeds the selected ordinary-field contract. This is a real semantic finding, not acceptance merely because the current seven pure cases omit it.

Select BEFORE test creation exactly TWO appended `#[test] fn ...()->TestResult` in EXISTING `crates/wf-storage/tests/state_store/workflow_values/codec.rs`: `value_binary_codec_preserves_reducer_priority_inputs` and `value_binary_codec_preserves_unresolved_extension_presence`. No helper/type/alias/constant/file/target; original five test functions and two helpers stay byte-literal (only necessary imports plus these functions). First case uses existing codec_request/old scalar constructors, modes5/6/7 with Create/Start priorities3 AND255, then ignored nonbirth priority255 in5/6, requiring explicit expected-positive encoder/owned-decoder calls and literal exact priority/operation fields. Second uses a literal nonbirth ExtendLease with existing valid lease guard, birth Some{Millis(0),ttlSome0,historySome0,hotSome(u64::MAX)} plus terminalSome(-1); require explicit encoder/owned-decoder positives and literal exact unresolved fields. No capture, clamping/default substitution, kernel, storage or business-success claim. Each new test's first intended encoder positive currently returns Malformed. Root adds/runs these only AFTER the active technical runner terminates, records genuine two intended failures and SAVES them BEFORE semantic fixes. Whole file<=600/functions<=100 naturally; if actual layout cannot fit, consult a concrete cut before creating any helper, never pack/drop cases.

Then select ONLY this source correction in existing metadata.rs/operation.rs: remove global priority>2/nonbirth-zero constraints and the global birth/terminal-operation-context rejection. Preserve raw priority for5/6/7 to the actual reducer; keep ONLY public1's EXISTING normalization priority coherence (Create0..2/nonCreate0), with no global restriction. Unresolved extension scalar widths/tags/optionality remain ordinary logged caller data for later guarded interpretation and accepted-birth-only capture; their presence itself is not an encoder capture operation. All closed modes/tags, defaults/configuration validation, witness order, counts, indices/alignment, requested-time/evidence/auto-partition coherence and checksums remain exact. Existing public caller normalization otherwise stays unchanged. No helper/API/owner/budget or hash-stream change: removing extra validation does not lower selected W or award a speed/resource gain. Root alone owns the two test additions, baseline SAVE and later narrow source correction; no semantic source edit is released before that RED. The original full workflow/rejection/history contract remains required.


### CODEC1 A1 — three Node recovery budget integrations and one exact geometry cut

Selection before creation: extend ONLY `crates/wf-node/tests/recovery/{limits,atomic_start,claim}.rs`. In the three existing named audit-bound tests, let X be `size_of::<DecodedRequest<'static>>()` and funded be checked old exact outer workspace plus X. X is the selected conservative additional full-header allowance, not a second DecodedRequest owner or an exact layout delta. Keep every old D-minus-one refusal and its original attempted/maximum values. Add the new-header one-short refusal with actual attempted D+X and maximum D+X-1, then succeed at funded. Rebase only the other point, wire, log-count and ValueLimit controls whose purpose requires this decoder stage to succeed. Preserve their original expected errors. Leave external scope peaks, outer driver envelopes, the 512 wrapper, post-drop peaks, business results, consumed-prefix custody and no-change observations unchanged. No Node production edit or intent8 acceptance follows.

Actual source witnesses: `join.rs:193..210` transparently subtracts base+512 and passes the remainder to the existing profile preparers. Current whole-file hashes are limits 17f68115ee7f12b6f196974e83f658d6e51a15395599d5fd049193341b971738, atomic_start f8f6003e66d6477c78b25e200e3032562114862fb84993dc75892ffe088327cb, claim f78f94c26558e2da2e876275183dd07fb85715d99085a623c0faacae1caeeccc. Whole target functions were read: limits 44LF, claim 77LF, atomic exactly100LF. The old compatibility failure packet 9932561+63411/6b6f2c7c2835be12733ac04b464d92bfd19750cb999f5ca91ea37fd2dbbf1da2 and its 581PASS/3FAIL are Root-reported here, not independently claimed runtime evidence.

Approve exactly ONE new private helper in existing `atomic_start.rs`: `fn check_atomic_driver_peaks(frames: &mut Vec<(u16, PreparedBytes)>, reference: PathBuf) -> TestResult`. Move only the existing outer-envelope and inner-prefix tail (currently lines318..361) into it. It reconstructs the same deterministic scope, e and wire from the same two frames and starts with the same old default driver limits; it then performs the original peak-1/peak real recovery, blank/failing-entry append, tiny inner-driver refusal, state/log equality and all directory cleanup. Consume reference so its cleanup remains owned. Parent retains history setup, exact old/new decoder and original log/ValueLimit controls, removes its first audit directory, then tailcalls the helper. No frame/record clone, seed change, callback, new model/file/test identity, assertion removal or altered outer formula. Normal local declarations and imports are allowed; canonical 100/600 geometry remains required. This cut is necessary because the existing atomic function is already exactly100LF.

These are test-only integrations of the already selected X contract. Priority/context predicates remain a known codec defect pending genuine two-RED SAVE; this decision neither fixes them nor grants storage capability. Root alone adopts, edits and executes; own source/workloads remain untouched.


### CODEC1 A1 — actual combined source review; semantic two-RED remains open

Actually whole-read the eight DOMAIN production files after combined canonical STOP, their sixteen Root integration seams, and the ten complete old-test inverse/diff packet recorded at528229+4644/e900a65e. Current sorted path+NUL+hexSHA+LF roster for the eight hashes to 80effe1c593c43732ee9423161db3b75411053b254f7319f557dd61b201c0b00. Bindings: request306/81ec48d4, consensus151/2e5c69c1, feature298/8e9cd8ee, frame175/323c3c8e, metadata278/7a5c1cda, operation204/2a849bf2, intent163/3b2bb9ac, budget116/e045224e. The exact scoped count is36: primitive1, defaults2, feature8, frame3, metadata8, operation4, intent4, budget6. This corrects the author's37 count without inventing an artifact. Two private structs and five work constants match the selected roster. Formatted source fits the physical limits; Root's existing Atomic live-expression factoring retains its assertions.

The actual routing counterpart closes the earlier unpaid peek: parent delegates without parsing; read_decoded/validate debit BASE before their common header, non8 enters only raw legacy_preflight, and8 extends the same maximum/spent before one census. Bounded prepare also uses that one account. No reset, second operation, nested old frame or relabelled old intent exists. Prepared scope/request/operation move once into the generic request; sidecar owns only metadata, and the public view borrows that owner. The public owned decoder has no donor lifetime; generic retains the explicit full encoded borrow. Existing Start handling is restricted to contained7 while the actual identity remains8. Old entrypoints still refuse8; Node support is not inferred.

Encoder first measures complete frame length using placeholders and lengths, then admits E<=D before comparisons, CRC, default hashes and caller derivation. It allocates one final body+footer Vec. Decode validates framing/footer before admitted materialization. The defaults60 grammar validates its supplied digest rather than repairing it; caller intent excludes the selected server preparation context and includes ordinary birth/terminal presence. The unchanged old operation counter counts variable leaves; the checked256/item addition is a conservative work/overflow upper only, never a raw-D comparison. The literal286B/frame78B caller/K62/W111232 fixture completes in the actual original pure cases.

Resource review agrees with actual PERF's closed census: at most29 writer calls/31 reader takes for worst item+PolicyRetry,17 input calls, two passes and bounded controls below3072/item versus4096; witness walks below512 versus1024. Adjacent order checks plus checked witness indices avoid N*P. CRC<=64E and remaining variable passes<=16E fit128E, with generic64K separate. BASE covers one public canonical invocation and bounded normalizer, not free reused work. H plus actual spines, batch OR public Box pointee,2E and generic keys matches code; X is the conservative added full-header allowance on actual old coexistence paths. No second physical header allocation, exact allocator capacity, OOM coverage, CPU/RSS or measured gain is asserted.

One real semantic defect is NOT accepted: metadata::check and operation::scan globally reject raw typed priority3/255 and unresolved birth/terminal operation context. Actual indexed/decision and the preserved independent-batch oracle route priority3 to item rejection; typed nonbirth priority is ignored. The selected correction at532873+3297/bffbb2d6 removes only these extra global predicates after SAVE, preserving public1's old normalization and every structural/default/index/order/requested check. Author explicitly acknowledged the interpretation error. No fix has yet run or been accepted here.

Actual final regression source is codec572/22711/a2bcb80d4e63c1b8de3e2cc66f5fd980d07810f5da3e35b95ef3b42955590812. Independently removing ONLY its two appended cases and necessary import changes IN MEMORY restores ENTIRE460/18075/377ba476524d365b5ddb17ab9ab3f02583336090b05e602d5989d940bb924aa6, including original five functions/two helpers. Whole/hash-read raw9921076+6731/d97dd545 and compared exact source/runtime name Counters: original17=7 pure PASS+10 first-factory UnsupportedLayout FAIL,0ignored/measured,105filtered. All actual storage/clock/replay/snapshot tails remain masked. Whole/hash-read raw9930794+1518/67b60484 and complete9929511+2826/ba6da4b7: new2=0PASS2FAIL,122filtered, intended first encoder Malformed at485(mode5/priority3) and557(unresolved ExtendLease presence). Three preceding technical commands are actually0, all four unchanged1624/6c3226b5. Later loop modes/255/decoder/drop tails remain masked. Technical inference, missing-Debug and style diagnostics are not counted as RED.

Actually whole/hash-read DOMAIN55802+4684/8469f38b and60486+4342/60b5ba8c, and PERF333205+8614/89f0b95d331dab52036ef762d2505c28cd02bdf287da09769ac295406d1e747f. Direct combined ownership/work findings agree; semantic acceptance and complete legacy conservation remain withheld. Root's oldcompat581PASS/3 Node failures are reported, not falsely claimed passing; exact three-test X/geometry integration is selected separately at536170+3076/85015b1d. No additional source defect was found in this snapshot.

Only this note appended; no source, workload, shared-doc or Git action. This is a qualified codec predecessor review, not storage publication/capability completion. Genuine same consumed transaction, every history binding, owned persisted reads/clock, closed registry/reopen/snapshot/node, typed/shared/routed hydration, real cleanup/rewind, original HistoryValues3, native8 and Linux/macOS end-to-end duties remain required. Finite combined review STOP; Root owns RED SAVE, the exact correction and subsequent validation.


Atomic cross-entry clarification, selected before edit: approve the SAME-test direct `prepare_atomic_start(frames[1].1.as_bytes(), normal.codec, d-1)` negative requiring exactly `Workspace { attempted: d, maximum: d-1 }`, then retain the Node refusal at its unchanged old outer cap with the truthful FIRST-entry attempted value `4*frames[0].1.as_bytes().len()+e+3072+X` and maximum `d-1`. Existing funded-minus-one and funded success continue to reach the last entry's D+X; preserve full no-change assertions and every outer/scope/post-drop control. No production, frame, seed, helper, case or fee change. Actual current atomic386/76c412daf7fac519b71cb5b8e00fe497a33bce9d63d0cf456fab11f7d001337f and whole raw10005705+3568/2f0ae618d9f0c4da950ef9645439f144d44dc707757fb68845d82a12341285e8 were read:21PASS/1FAIL, observed4397 versus old expected3717 at maximum3716. Earlier-entry D_first+X legitimately preempts last-entry D; keeping both direct old-D and real Node boundaries preserves the intended obligations without claiming the unreachable stage. Exact prepare error may be extracted with `.err()` to avoid requiring decoded-output Debug. Semantic codec two-RED remains unfixed pending SAVE. Only own note appended; finite clarification STOP.


Header-registry test integration, selected before edit: extend ONLY existing `crates/wf-ports/tests/recovery_header.rs`, whole-read187LF/7118B/21d4b71d2ab705b280e7f9f285512593a2359a7f107ed1813bf92552c181bf97. In existing `normal_recovery_header_borrows_atomic_start_without_body_authority`, change only the stale unsupported `(tag1, intent8)` negative to `(tag1, u16::MAX)` with the same UnsupportedVersion expectation. Add a separate literal header-only `(tag1, intent8)` positive requiring profile8, unchanged borrowed scope bytes and pointer into the original header plus the exact stamp17/3/11. Keep all original atomic7, malformed/truncated/cap/legacy/request-bound controls and all three test identities; no helper/type/file/target or production change. This follows the already selected closed intent8 registry, not body verification, storage capability or a widened version range. Natural100/600 limits still apply. Root's872PASS/1 stale-negative failure packet10012647+91552/71050f85 is reported here; complete old conservation and genuine semantic two-RED SAVE remain pending. No semantic priority/context fix follows from this registry assertion correction. Only own note appended; finite selection STOP.


### Header patch review and exact immutable-prefix compatibility resumption

Actual final header source196LF/7510B/4295a5e1020656257b20626c6db9011695a4c0a475a7ec88fe93068777e0d403 whole-read. Independently reverse only its6-line value-header positive and the expanded u16::MAX negative to recover ENTIRE187/7118/21d4b71d2ab705b280e7f9f285512593a2359a7f107ed1813bf92552c181bf97. Exact three test names unchanged; all old controls remain, third function96LF. Header-only8 returns profile8, original scope/pointer and stamp17/3/11; u16::MAX remains unsupported. This grants no body verification or storage authority.

Independently hash/parse the complete saved126211B baseline9738292/7d7ea7ba first two runtime steps and current91552B raw10012647/71050f85. Strict prefix BEFORE FIRST wf-ports Running header is10012647+72086, ending10084733, SHA25618ff1d07aa208f94a49a24e2408bb4fd94060b5195e0669722e166dd9d5b0876. Every target summary agrees with its parsed tests. Exact logical(package,source-target,test-name,status) Counter contains688PASS: application26, architecture53, domain477, node76, parity56. Only four extra tuples versus prior1093 are the exact scalar restore tests in stored_lifecycle; no test-name/status normalization. All185 partial wf-ports tuples, including its stale failure, are discarded. Required disjoint complement is409: ports206, storage168, test-support13, types22. Projection/wire/worker have zero cases but their complete all-target invocations remain required. Two captured domain smoke sections also compare byte-equal.

Thus preserving this immutable prefix is sound PROVIDED the actual remainder matches that exact complement and the source-equivalence premise holds. Root reports its complete two-path inventory inverse (header+coordination only) to original1624/f402; own entire header inverse above independently verifies its code portion. No production input changed. Rerun all six remaining packages plus complete old storage skipping workflow_values, then docs/build/arch/reference/progress/deny/diff. Required missing-tuple multiset hashes3b9b50402ca453a84c39455117c622653214f3f65486ebb97d3058ded1ed2545 (sorted NUL-separated package,target,name,status,multiplicity). Union must equal1097 exact tuples, not merely that count. This proves the admissible resumption partition, not a future successful remainder: Root now reports205PASS/1 stale value-effects budget failure in ports; storage and final closure remain pending.

### Value-effects existing-test X integration, selected before edit

Approve ONLY existing `crates/wf-ports/tests/workflow_value_effects/intermediate.rs`, actual238LF/9741B/9e8c29f6975afabfe6177243e7663512fbf3d1798d82333b80f592e306774b12 whole-read with fixtures, fresh::inputs/value_effects_budget and phase::finish_inspection. Keep all three names and the other two cases literally. In the existing continuous-work/coexistence case, keep old D=decoder_bound, I=early_bound, O=I+F+R and C=D+F formulas unchanged; X is actual sizeof DecodedRequest. Preserve every original cap observation, with truthful earlier X refusal. A single natural Fresh table may require (maximum,attempted): (D-1,D), (I,I+X), (O-1,I+X), (O,I+X), (I+X-1,I+X), (I+X,O+X), (O+X-1,O+X); then succeed at O+X. A Complete table requires (C-1,D+X), (C,D+X), (C+X-1,C+X), then actual Complete/LogReplay at C+X. The existing fixture's prepare is the first old-D check, so its D-1 row needs no new helper. Explicit actual cohort relations X>F+R and D+X<=I may be asserted if used to identify the earlier stages; no exact size/layout is assumed.

Old C cannot be observed as the refusal at C-1 because both preparation and public inspection check D+X first; no private bypass is selected. Funded C+X reaches the genuine F-only Complete owner. Keep direct borrowed-inspector row-minus-one/work0/1 refusals and replay None/work checks unchanged; they do not acquire X. No production, fixture, helper/type/file/case or broader cap change. Separate table loops and short named funded totals fit the existing76LF case under100 naturally; if actual formatting cannot fit, consult before adding an artifact. Real-R, F-only and continuous-work obligations remain intact. Semantic priority/context two-RED stays unfixed pending SAVE. Only own note appended, no source/workload/Git action; finite review/selection STOP.


### A1 final pre-SAVE custody: exact old1097 closes; semantic two-RED preserved

Actually whole-read final intermediate260/10578/3ed79da1c25d361b84df49b46eba3f7453554a8f3f758e28aaac4c78edf8bfad. Its one changed case is96LF; all three exact names remain. Independently reverse ONLY the selected import and bound-table/success-cap edits IN MEMORY: ENTIRE238/9741/9e8c29f6975afabfe6177243e7663512fbf3d1798d82333b80f592e306774b12 is recovered. Thus both other cases and all original row/work/owner/replay checks remain literal, while old caps truthfully report their earlier X stages and funded one-shorts/exact positives cover real F/R coexistence. No source/fee change or extra helper was needed. Actual focused raw10161484+477/829bb155 is precisely that one named PASS/7filtered.

Independently hash/parse every runtime tuple and target summary in baseline9738292+126211/7d7ea7ba, strict current prefix10012647+72086/18ff1d07, ports10104699+22399/f96aed30, later10127397+4443/029382342bc221c41ffc705ae3b8a7d112638a20133feac69587e3c1a4af8d91, storage10132095+16579/d0361d2c and the focused477B. Exact(package,source-target,test-name,status) Counter equals ALL1093 originals plus ONLY the four scalar restore names:1097PASS/0FAIL. Replace only the matching old failed effects tuple with its actual focused PASS; never add a duplicate or normalize a name/status. Sorted NUL-separated tuple+multiplicity SHA256916d94a5ad3f8764e60d966862fbe949d7908a1622375887bd7eb6d51981f4f0. Full-input equivalence across the test-only change remains Root's reported two-path inverse; own entire test inverse is independent.

Actual docs2706/24a0f879 gives6PASS/15 summaries. Read the complete following9902B quality suffix (SHA31307dfd980ff61ce3a59bd3ef0bf435b8f6fede7425d7ca4b49975b14cf8049): docs build, arch53, reference480, progress120/132/550, deny and diff each EXIT0/UNCHANGED; final fmt/strictClippy also0 under b9867. Whole/hash-read fresh semantic1421/f1ad6238: exact two names still0PASS/2MalformedFAIL at intended485/557 with122filtered. Actual metadata7a5c1cda and operation2a849bf2 remain the unchanged defective production bytes; later modes/decoder tails remain masked. No semantic repair is hidden in old compatibility closure.

Actually read PERF344441+3275/a06b3b4c and DOMAIN64828+4042/50a69dbd plus final68870+3701/e740d263. Independent source/stage and exact-union findings agree; no remaining checkpoint discrepancy found. Select SAVE NOW of the released A1 production/tests and reviewed closed role notes/CONTINUE/coordination, preserving the genuine two-RED checkout. Only AFTER that commit may Root release the already recorded narrow metadata::check/operation::scan predicate correction at532873+3297/bffbb2d6. Preserve public1 normalization, structural/default/order/index/requested checks, wire/hash/owners/work and frozen test oracles; no new artifact or broader contract follows. Four real storage/read endpoints and all connected durable/hydration/native/Linux/full-goal duties remain open. Only this note appended; finite pre-SAVE review STOP, no source/Git/workload action or extra ACK gate.


Codec A2 actual closure: current metadata269/9684/cb2386752e69244281bd6fa6d0c4e854d7a80dc1254d8e286ad24dd37859f924 and operation204/6770/b1754174936565960e4e16e32f20be6a0c4db28b5e8161555cde949f10863b73 match the selected correction. Root explicitly authorized read-only Git for this audit; I read both622db0f beforeimages/diffs and independently reversed EVERY exact hunk IN MEMORY to recover ENTIRE metadata278/7a5c1cda and operation204/2a849bf2. Only the global typed priority/context predicates disappear, the now-unused item zip becomes aligned input iteration, and public1's priority rules remain explicit in both check/scan. Structural/default/index/order/requested/CRC/wire/hash and budget owners remain unchanged; no new helper/type/file. Whole/hash-read raw10167666+6820/fbe3116b and compared exact19 source/runtime names against prior17 plus new2: ONLY those two intended Malformed failures become PASS; all other17 statuses unchanged. Actual9PASS/10 first-factory UnsupportedLayout FAIL/105filtered, so all pure modes/negative/ownership/jitter and newly unmasked regression tails now finish, while real ten storage tails remain masked. Root reports technical3all0/unchanged1624/ba8d5e19; the earlier unused-binding compiler diagnostic is technical only. PERF's actual direct source/runtime findings agree. Scoped codec acceptance has no remaining defect; full old79971 remains live and is not claimed complete. Storage batch remains consultation-only, with no capability/publication/recovery/hydration completion credit. Only own note appended; no Git mutation/source/workload. Finite A2 review STOP; next concrete storage roster is separate.


Necessary frozen-fixture correction selected BEFORE edit/storage BODY: actual `workflow/batch.rs:244..267` rejects repeated IDs in contained atomic5, while existing independent6 genuinely preserves ordered same-ID intermediate results. Three currently factory-masked fixtures incorrectly request5 for two same-ID writes while requiring both histories. Change ONLY these existing literal profile arguments5→6: `wf-storage/tests/state_store/workflow_values/reads.rs` first read case's `f::batch(...b"read"...)`; `guards.rs` capability case's `f::batch(...b"terminal"...)`; `recovery.rs::publish_snapshot_values` `f::batch(...b"snapshot"...)`. Paths are under crates/. Actual read files41a05bcf/59942ac4/cfb080d4 and their item vectors establish identical IDs read/expired/snap respectively. DOMAIN independently found all three. Select independent6 rather than two commits so each original SAME consumed transaction/history/expiry/snapshot scenario remains intact. Preserve every test name, helper, assertion, constructor-first positive and all other bytes; no reducer/codec semantics weakening. This supersedes only these erroneous input literals, not the frozen behavioral oracles.

Root must wait for the current workload to terminate before applying, then compile/run and SAVE the corrected RED checkpoint before releasing storage BODY. Expected existing19 result remains9 pure PASS/10 genuine first-constructor UnsupportedLayout FAIL; later fixed6 producer/storage tails are still MASKED until real capability implementation and cannot be credited now. This supplies the concrete source defect plus newly rebound saved-RED prerequisite for the frozen-test exception. No source/test/workload/Git mutation by this reviewer; finite clarification, full storage artifact decision still pending actual roster.
