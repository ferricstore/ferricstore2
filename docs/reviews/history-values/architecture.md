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
