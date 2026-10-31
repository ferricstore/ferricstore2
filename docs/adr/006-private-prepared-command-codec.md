# ADR-006: shared private prepared-command codec

Status: selected for pure M1 implementation after architecture review A-PC1.
The initial represented batch implementation has exact-byte/boundary controls.
The represented policy format also has initial exact-byte/boundary controls;
the bounded physical-claim format now has initial exact-byte/boundary controls.
Independent final review, complete operation coverage and persistence/runtime
validation remain required.

M2-STEP-CODEC1 selects a separate preparation-only intent10/item12 singleton for
the existing pure StepContinue operation. It derives scope-bound caller fields
and verifies complete bounded FWCP1 bytes before owned materialization. Caller
intent differs from the complete prepared-frame digest; stamp, execution time
and ignored nonbirth priority remain preparation fields. Exact grammar, identity
and prospective tariffs are recorded in the coordination selection. Six actual
runtime refusals saved in32f9045 precede all bodies. The complete borrowed scan,
canonical hash and admitted materializer now pass seven focused cases. One
saved field-cap expectation was corrected from ByteLimit to the existing
ValueLimit vocabulary after a separate encode/decode conformance control;
all original inputs and thresholds remain. This does not activate legacy profiles,
Fresh/replay handling, response vocabularies, storage/history formats, recovery
dispatch, submission or public transports. Those require their own selected
contracts and actual persistence/source/fault/profile evidence.

The Step facade and all private descendants are classified as pure values.
Three exact module effect probes failed before the missing prefix was added;
b21f87f preserves that genuine architecture failure. The stricter prefix adds
no permitted effect or dependency. The shared singleton header accepts only the
explicit7/10 selections; its7 specialization preserves the complete previous
atomic-start header. Generic preparation and normal/stored recovery retain
their older profile restrictions.

M2-STEP-RESPONSE1 adds a separate data-only intent10/response9 grammar after the
five compiled refusals saved in d7faf8f. FWST1 binds all three original u64 log
identity components and execution time, followed by the result. Accepted replies
represent the actual singleton Changed StepContinued postimage, logical state,
rotated lease, counters, ordering and all three optional value fields. Rejections
share the complete existing extended atomic-start writer, including the dedicated
DirectRunningFifoEntry reason; that helper's body and all older decoders remain
unchanged. No generic/independent/public response profile is widened.

Measurement validates complete bounded fields before output reservation.
Complete borrowed scanning binds context and EOF; actual-plan matching verifies
the same grammar without allocating a second response. Token verification uses
bounded fixed decimal scratch. Preserved zero attempts and an independent
creation time are valid; initial Claim and Step preserve the current attempts.
The first golden incorrectly assumed1 and was corrected to0 after source review
and a separate after-body control. The exact160-byte format, other fields,
offsets and resource thresholds stay unchanged. All six focused tails now pass.
This format does not implement retained inspection, Fresh authority, historical
snapshot representation, persistence, recovery or public transports. Complete
those successors before claiming durable Step or the original source scenario.

M2-STEP-FRESH1 joins the explicit intent10 command and response9 through the
existing point retained gate and opaque consumed Fresh owner. Six genuine
inspection refusals saved in e7a9291 precede implementation. Only Replay scans
original response/context/status; conflict, expiry and exact-log precedence are
unchanged and do not consult current workflow authority. Fresh checks actual
group and full log/prepared control, uses the touched indexed planner and binds
metered payload-presence provenance to its actual postimage. The same owner
proposes indexes, history, original outcome and processing control.

The response's existing measure/emit grammar is shared at the late ownership
boundary: release decoded command/key/filter buffers before prospective reply
admission and reservation. The shared singleton tariff remains E+D+header+512+
reply+additional-live+effect-descriptors. Existing decoder/staging/effect work
accounts are retained. The retained constructor admits only intent10/response9
Accepted/IndexedRejected status; other intent specializations retain their
previous profile/error order. Value-profile wrappers8/9 remain unsupported for
Step. All six focused cases now pass unchanged, including exact one-below limits.
This proposal supplies no actual writer, historical schema, recovery or public
service. Those remain mandatory successors; no source-case credit is inferred.

## Ownership

Neutral owned preparation values belong to `wf-ports::consensus`. One private
codec implementation lives below `wf-ports::log`, with narrow typed encode/decode
functions. Application, storage and consensus already depend on ports. This
preserves the reviewed first-party arrows and avoids an unavailable application
decoder or independent copies of the same format. Worker remains restricted to
ports::client; public wire and transport adapters cannot import this codec.
The format is a private Rust log contract, independent of public JSON/TCP and
legacy on-disk encoding. No successful submission, durability or authorization
is supplied by these pure functions.

Extend the existing exact `sha2 = 0.11.0`, default-features=false, no optional
features approval to ports alone. ADR-005's source/security/license and unsafe
dependency observations still apply. Types/domain and wire gain no hash edge.
Negative architecture controls must reject feature/version/source drift and
attempted leakage. All first-party codec code is safe and synchronous.

## Bytes and identity

Prepared bytes start with ASCII `FWCP`, big-endian u16 format version1 and an
operation discriminator. Request headers encode tenant/group/recovery/request
as distinct u32-length-prefixed opaque byte fields, followed by u16 client-intent
version and 32 digest bytes, then physical u64/logical u16/wall u64 trusted time.
This format carries no native Rust layout, platform-sized integers or Unicode
normalization. Every scalar uses explicit big-endian width. Every optional value
has exactly tag0 absent or tag1 present; a present empty byte field differs from
absence. Collections carry u32 counts and meaningful batch order. Policy maps
use strictly increasing raw byte keys and reject duplicates.

The SHA256 prepared digest covers exactly these bytes. Neither that digest nor
the separately assigned Raft term/node/index is encoded in its own payload.
The apply adapter joins the independently assigned lossless LogIdentity to the
recomputed PreparedDigest. Client intent excludes leader-generated inputs and
remains distinct. Installed current retry/mode policies come from coherent
authoritative apply state; they are never caller-supplied payload authority.

Initial request discriminator1 is a bounded batch. Each item contains opaque
partition and record IDs, u8 creation priority, u64 explicit execution time,
then its typed operation. Tags1..10 identify create, claim, reclaim, transition,
complete, fail, retry, retry-with-policy, extend-lease and cancel. Their fields
follow the actual typed domain structs in declaration order. Retry-time tag0
means explicit and tag1 means policy-resolved. Backoff kinds0..3 mean none,
fixed, linear and exponential. The policy installation format below has its
own exact vectors. Further request-control additions require their own vectors
before support; unknown versions/tags reject rather than dropping fields.

Actual isolation and state keys are derived from decoded typed fields, with
their own versioned length prefixes, and checked against the complete 8,192-byte
scope and 65,535-byte key bounds. Payload/result/error slices derive directly
from the decoded command fields, with at most two represented occurrences.
No claimed size, caller-provided key, digest or verified flag is accepted as
field-binding proof. The encoder and decoder enforce immutable aggregate,
count and individual byte bounds before allocating/copying variable input.
The initial format has no recursive values; policy extensions need explicit
finite shape/work limits. Operational allocation/corruption failures never
become retained deterministic business rejections.

Isolation bytes are `FWSC`, u16 version1, then the three u32-length-prefixed
tenant/group/recovery fields. A state key is `FWSK`, u16 version1, then the
u32-length-prefixed complete isolation bytes, partition and record ID. The sum
of the isolation bytes and every derived key must also fit the immutable
decision-byte limit. This bounds repeated namespace-prefix amplification even
when the original prepared input is small. Request ID belongs to the request
header, independently of a record's authoritative state key.

The encoder measures the complete grammar before reserving its exact output
buffer. The decoder completes a borrowed structural/count/length/key pass
before materializing request fields, items or keys. DecodedRequest borrows the
original complete input, owns one decoded typed request and derived keys, and
exposes value fields by borrowing that request. Input bytes, decoded values and
key bytes have separate logical caps; their simultaneous ownership is not one
global allocation or native-memory bound. Safe fallible Vec reservations do
not imply that every standard-library allocation is fallible.

## Policy installation format

Request discriminator2 carries the normalized InstallCommand, followed by one
u32-length-prefixed private policy value. The command preserves workflow type,
optional expected generation, mode0Patch/1Replace, then the patch fields in
declaration order. FieldPatch tags0/1/2 mean unchanged/set/clear. Customer
version kind0 is opaque bytes and kind1 is u64. RetryPatch carries optional
max-u16, optional partial backoff (optional kind/base-u64/max-u64/jitter-u8),
and optional opaque target. Retention carries optional ttl-u64/history-u32.
State changes are byte-sorted, u32-counted names with tag0Remove/1Patch,
followed by mode/retry/retention patch fields. Mode0 is parallel and mode1 FIFO.

FWPV is magic/version1/generation-u64/snapshot-length-u32/FWPS, with an 18-byte
prefix. FWPS is magic/version1/type/version/max-active/normalized retry/
retention/states. Each snapshot state preserves optional mode, normalized
retry and retention, including absent versus explicit zero/parallel values.
Installed generation is 1..MAX_POLICY_GENERATION, independent of customer
version. Private snapshot bytes are at most 256KiB and the envelope at most
263168 bytes. Those caps include actual encoded prefixes. Policy work/raw
limits are separate from the inline workflow payload cap; complete FWCP and
isolation caps still apply. The private immutable default policy selector
resolves to 16,384 states; replica-profile checks must compare resolved values.

The codec verifies arbitrary legacy PolicyValue byte bindings by walking the
canonical semantic envelope against borrowed expected bytes before output
reservation. Decoding scans complete patch and snapshot shape, semantics,
sorted uniqueness, lengths/counts and boundaries before owning any fields.
PolicyValue owns one normalized snapshot and one actual encoded envelope;
DecodedRequest borrows the complete original frame and adds no second candidate.
Fresh installation still recomputes against coherent stored generation and
migration high-water. After retained inspection, fresh dispatch first checks
stored generation, expected type, actual envelope size and selected snapshot
limits using borrowed fields. Corrupt stored authority returns no control or
operation plan; coherent stale CAS remains a retained business rejection.
The standalone installation API preserves its source CAS-before-stored-value
validation order. The header gate precedes changed guards on retained replay.
Source business errors remain preparation/fresh-decision errors;
malformed private normalized bytes are operational failures.

Present state mode Clear remains representable in private command bytes, while
source-compatible A-N21 admission rejects it as InvalidStateMode. Valid normalized
snapshot mode=None remains distinct from this invalid caller field. The unchanged
policy_clear byte fixture explicitly restores its expected normalized generation2
value; it does not claim that the invalid command can install that value.
An actual decoded coherent-view control verifies the fresh business rejection,
required outcome write and stamped advancement while preserving installed policy.
Replacement state Remove is also mathematically representable, while reviewed
A-N24 admission rejects Replace+Remove as InvalidStatePolicy. Its supplemental
actual decoded fresh-view control verifies the required outcome and stamped
advancement with unchanged empty authority; valid Patch deletion remains distinct.
Architecture and performance independently passed the current22 codec controls
with unchanged relevant inputs. Those runs establish this represented pure
boundary, with complete operation/profile and persistence/runtime gates open.

Private policy encoding does not promise source ETF/on-disk or source-facing
wire/hash compatibility. Those observations and complete operation/error
registries remain separate acceptance work.

## Physical claim phase format

Request discriminator3 represents one physical-state claim leaf, selected in
A-N42/A-N43. Owned PreparedClaimQuery/PreparedClaimPhase live in the private
consensus/claim child and are reexported by consensus. The log/claim child owns
the grammar; no current policy, index, selected record, state key or generated
lease token is serialized. The domain selects from coherent authoritative
indexes at ordered apply. Automatic parent reclaim quotas and timeout/governance
orchestration remain separate implementation contracts.

After the unchanged header, fields are workflow type; optional physical-state
list; excluded-state list; optional partition-component list; optional u8
priority; worker bytes; u64 lease duration; u32 selected/scan/head limits; u16
token version; and u32 first ordinal. Lists use u32 counts and raw byte fields.
Optional lists preserve Any versus Only(empty), order and duplicates. Every
usize/u32 conversion is checked. Structurally representable empty/zero/max or
business-invalid fields remain for fresh domain admission after original replay.
The private decoder does not impose the fresh64 filter-footprint rule.

Each of the three lists has an explicit100,000-element structural ceiling,
independent of touched-record limits. Count<=remaining/4, actual field bounds,
checked scalar widths, complete input/scope limits and final end-of-input are
checked in the borrowed pass before any scope/query/reference reservation.
Encoder measurement checks counts/fields/widths before output reservation.
Decoder materialization uses fallible reservations and one owned leaf copy.
DecodedRequest also owns three bounded reference spines into the immutable
external encoded input. Its short-borrow claim_envelope(log) joins the verified
header/digest, borrows those spines without cloning, and sets resume=None.
Current authority and both trusted apply profiles remain outer inputs.

Input, caller preparation, decoded leaves, references and isolation can coexist.
At three100k empty lists,1.2MB of wire length words accompanies300k owned Vec
descriptors and300k slice references. A conditional, unmeasured64-bit model
gives12MB decoded descriptor/reference bytes and another7.2MB if original
descriptors coexist. These figures are not portable layout promises, replicated
charges, measured RSS or a whole-process allocation/work guarantee. The three
count limits bound empty-field amplification separately from encoded byte caps.

Twenty-one independent Python vectors cover binary values, list order/presence,
priority, empty type/worker and zero/max scalar inputs. Existing15 batch/7 policy
vector bytes remain identical. Six controls cover those bytes/SHA/typed values,
every truncation/trailing byte, malformed tags/counts, exact entry/inline bounds,
each list100k/+1 on both paths, simultaneous maxima, malformed final input after
two complete maximum lists and actual envelope/filter pointer binding. Real
missing-API and six-control unsupported-stub failures preceded implementation.
Five supplemental controls now run actual decoded envelopes against coherent
parallel authority: accepted mutation with complete lease/history/index assertions,
complete empty claim, fresh invalid-priority rejection, unsupported-token failure
without an exposed control/outcome plan, and retained accepted/rejected replay
before changed server phase hints. They derive original retained status from
actual first processing, preserve the caller query/intent, and check the original
borrowed response, time/log/expiry and no second outcome write. Local authority
owners remain alive through a bounded HRTB callback; borrowed plans never escape.
These are postimplementation integration checks, not retrospective behavior RED.

Current owner and independent architecture/performance executions each pass11
claim+10 batch+12 policy controls and the exact workspace policy target. Their
separate280/276 before/after input bindings remain unchanged. The full owner
wrapper passed322 all-target tests plus four doctests and strict/architecture/
reference checks, then failed normal RustSec GitHub fetching on DNS resolution.
That saved-tree evidence does not establish a completed audit, current CI or
public parent orchestration. Exact phase/hash provenance is in the coordination
and reviewer records; every full workflow/source/profile/adapter gate remains.

## Acceptance

Independent Python struct/hashlib byte and digest vectors precede implementation.
Actual compile and behavior REDs precede represented codec GREEN. Test every
typed operation, arbitrary binary values, absent/present-empty/zero/max scalars,
meaningful batch order, full truncation, trailing bytes, unsupported version/tag,
malformed optionality, excessive count/length and exact complete limits. Hash
and field binding must agree with those vectors, not only a Rust round trip.
Architecture/performance reviewers independently read and execute the controls.
Strict checks and all dependency audits remain required. This codec does not
certify atomic state/index/history/outcome/checkpoint persistence, membership,
snapshots or recovery; those remain the ordered M2/M3 gates.

## Complete normalized policy VALUE codec (A-V2VAL1)

The canonical FWPS/FWPV version-2 policy VALUE grammar belongs to the pure private implementation in wf-domain::policy::complete. The three complete-policy prepare, verify and decode operations use actual typed content or borrowed bytes and the same caller-owned PolicySchemaCharge. They do not install a policy, allocate its generation or compute an intent or digest. Existing version-1 policy bytes and FWCP framing, dispatch, namespace binding and hashing remain owned by wf-ports.

This exception changes responsibility for normalized VALUE bytes only; it adds no crate dependency, runtime, I/O, clock, hash or OS API to domain. Private measurement stays within one operation's immutable content borrow. Preparation moves the candidate, verification streams against existing bytes, and decoding completes borrowed structural preflight before materializing variable data or envelope owners. Options retain supplied empty groups, false and zero; maps use raw-byte order and lists retain their normalized order. New complete values accept generations 1 through the existing maximum, without converting legacy generation0 or arbitrary source ETF/bignum values.

Each operation checks equality with the account’s immutable profile first and prospectively pays its selected structural, byte, comparison and copy visits before the corresponding work. Matching profiles do not establish accounting identity; the enclosing parent must own and thread the same charge through all of its operations. Failed debit commits neither failing work nor scratch and preserves previous successful work. Existing G1 peak P still describes canonical scratch, not the sum of live Data, VALUE, request, FWCP or parent owners. Native capacity, allocator failure behavior, CPU, RSS, caller construction and rejected-owner destruction require separate evidence. This codec supplies no whole-request, live-owner, source parity, storage or performance certificate.

Creation follows independent literal byte and error oracles, actual API compile-RED, implementation, and reciprocal architecture/performance review. Production files/functions stay within400/100 physical lines, tests within600; dependency and purity checks remain unchanged. Linux and macOS require independent executions. Compound framing/hash/accounting, authoritative install/Patch/intent, original Elixir scenarios and later ordered runtime milestones remain separate acceptance gates.

## Complete replacement caller and intent scheduling (A-M1-OUTER2-I1)

Status: exact Stage I names and responsibility have complete reciprocal approval;
creation follows six independent test bodies, a real missing-API RED, separately
selected rejecting-scaffold behavior RED, implementation and actual peer review.
The selected contract alone is no authored-body, native or product acceptance.

The private `wf-domain::policy::complete::outer` implementation may check the
actual typed caller header and measure the generation-free normalized intent
grammar. It remains pure, synchronous, std/types-only and has no SHA, effect,
clock, OS or backwards dependency. Ports owns the actual private field-to-SHA
stream. CompletePolicyReplaceInput moves scope/request/time/guard/Data owners;
Caller retains that input and immutable copies of the supplied schema and
AdmissionLimits. These copies are mathematical content caps, not AdmissionCharge,
account identity, digest verification or install/parent authority.

Each closed entry checks equality with the actual schema account's immutable
profile first at zero logical debit, then pays root128 before finite header,
stamp and scope inspections. The inclusive FWSC aggregate is18+tenant+group+
recovery<=8192. The actual measured intent is at most1MiB. Typed counts check
their cap/u32 representation before container128 and width4; RAW pays128 before
actual cap/u32 and4+length; an option pays its flag1, then Some128 and the
distinct represented payload group/container when present. Existing structural
field/list/map caps and all Option/enum widths remain exact; this is not full
installed-policy semantic admission or raw Elixir compatibility.

The operation-local measured view borrows actual tenant/Data and copies the
guard before that same paid field walk. Consuming IntentMeasure::finish releases
its mutable charge borrow and fills only private checked bytes/nodes. Two
distinct prospective work events then prepay the actual stream and SHA. The view
drops before Input moves into Caller. No extra sizing walk, unused operand,
public counter, paid-result encoder, callback, allowance, reset or second
account is introduced. Only the raw owned-input ports entry closes the actual
preparation/hash schedule on its one received mutable charge; mathematical
domain calls and public immutable getters cannot prove composition identity.

Independent minimal B52/N5 costs692 measurement+692 stream+180 SHA=1564/P0;
Some(0) costs1844 and attrs[a]2615. Maximum1563 refuses the last180 while
retaining1384; two minimal calls on the same object cost3128, and maximum3127
retains2948 after the final refusal. A failing prospective debit commits neither
failing scalar and preserves prior successful work and positive scratch. The
caller constructs no uniqueness/sorting scratch. P remains max canonical
scratch, excluding simultaneous Data/VALUE/frame/parent owners, native capacity,
tree comparisons, caller construction, rejected-owner Drop, CPU and RSS.
Represented-length work coefficients do not certify physical byte reads.

Existing V1 branches and VALUE responsibility are preserved. Stage F request
models, frame/restore APIs, private ClientIntent2 constructor, Reader seams,
tag4 framing, FWSC emission and their tests/constants remain separately held.
Two encoded owners during future VALUE-plus-frame preparation need explicit
ownership accounting; no free in-place, Box or Arc conversion is selected.
Operational schema/work/length/cap errors stay structured and never become a
retained business rejection. ONE charged retained gate, Fresh-only current
authority, coherent atomic namespace/high-water/outcome/checkpoint installation,
raw Patch/ETF/reference/source parity and later M2-M8 remain required.
Linux and macOS require independent current execution and matched compiler,
release/overflow/features/allocator/workload resource and timing measurements.

## Original core-Patch caller and intent scheduling (A-NEXT1-CONSULT)

The selected branch preserves the original normalized CompleteCorePolicyPatchInput;
it does not encode a merged CompletePolicyValue as Patch identity. Private domain
outer/caller/core_patch measures the owned caller command; private ports
log/complete_policy/core_patch streams the same borrowed fields through the
existing IntentHasher. Existing Complete-Replace/IntentMeasure/Hasher bodies and
VALUE/FWCP/V1 responsibilities remain exact, apart from the reviewed header
operand factor and private hasher visibility seams. Public getters expose
immutable content/caps, with no account identity or install/replay authority.

The exact registry is WF-INTENT\0COMPLETE-CORE-PATCH\0V2\0 (33 bytes), followed
by tenant RAW, guard OPTION-u64, workflow-type RAW, version FieldPatch,
max-active FieldPatch, retry FieldPatch, retention FieldPatch and states MAP.
RAW is big-endian u32 length plus opaque bytes; MAP is big-endian u32 count and
existing raw-byte BTreeMap order. OPTION is 0 absent / 1 present. FieldPatch is
0 Unchanged / 1 Set / 2 Clear, with payload only for Set. Version is 0 Bytes+RAW
or 1 Number-u64. Maximum Set writes u64. Retry Set writes max OPTION-u16,
backoff OPTION-partialBackoff and target OPTION-RAW. Partial Backoff writes
kind OPTION-u8 (0 None / 1 Fixed / 2 Linear / 3 Exponential), base OPTION-u64,
max OPTION-u64 and jitter OPTION-u8. Retention Set writes TTL OPTION-u64 and
history OPTION-u32. Each state writes key RAW then 0 Remove / 1 Patch; Patch
writes mode FieldPatch (Set 0 Parallel / 1 Fifo), retry and retention FieldPatch.
No padding, default expansion, empty collapse or invented governance/indexed
Patch field is selected. Full u64 guards and business-invalid typed values retain
identity until later Fresh semantic validation.

The closed entry first checks equality with the actual account's immutable
profile at zero debit, then pays request-root128 before checked FWSC aggregate
18+tenant+group+recovery<=8192. One paid borrowed measure walk uses the unchanged
IntentMeasure. It pays partial-root128 before workflow RAW; RAW descriptor128
before cap/u32/4+length; count cap/u32 before container128/width4; each state
row128 before key RAW and action tag1; only Patch adds payload128 before its
three fields. Every FieldPatch pays tag1 and only Set pays payload128. Retry,
Backoff and Retention add distinct group128; scalar/version visitors add no
group. Every Option pays flag1 and Some payload128. Incoming state count uses
usize::MAX followed by checked u32 representation, never final max_states.
Type/version/target/key RAW cap is262144; scope is8192 and complete intent1MiB.
No BTree sorting, reference spine or native comparison fee is invented.

WR1 checks represented byte addition, then global byte cap, then conversion and
prospective same-account byte debit, and commits the byte count only on success.
LengthOverflow and structured Encoding(ByteLimit)/schema/work errors retain
existing mappings. Consuming measure.finish releases its mutable charge borrow;
private checked B/N metadata prepays stream128*N+B and SHA128+B as TWO separate
prospective events on that same object. The view ends before original input
moves into Caller. Ports then performs one actual immutable SHA traversal.
There is no second account, reset, refund, callback or reusable paid recipe.
Failed debit commits neither g nor p and preserves earlier work/maximum scratch.

Independent grammar/event rows are (B,N,G): minimal(52,5,1564), guards0/MAX
(60,6,1844), emptyRetrySet(55,7,2085), Some-emptyBackoff(59,9,2609), state-a
Remove(58,7,2094), state-a defaultPatch(61,8,2359), SOURCE8 original partial
(92,16,4500). G=256*N+3*B+128 and new scratch is0. Eight full independent
literal/digest rows freeze this core; all remaining branch/cap/exclusion controls
and all six complete readable test bodies must freeze BEFORE test writes.
Maximum1563 refuses final180 and retains1384; two minimal operations under3127
retain2948 at the second final180 refusal. Guarded1843 retains1656. Legitimate
held indexed1928/P64 plus minimal1564 gives3492G/P64,total3556; maximum3555
refuses180 and retains3312G/P64,total3376. Profiles are immutable from each
scenario's start; all real setup/operations remain on the same received account.

P remains maximum canonical scratch, excluding simultaneous input/current/
Data/VALUE/intent/parent owners, native capacity/tree/SHA/allocator/Drop/stack/
CPU/RSS/latency. Pointer/source moves do not prove allocation or lifetime gains.
Public current-policy comparisons in tests must call the real paid pure Patch
preparation on legitimate independent values, not unused variables or producer-
derived expected data. That model supplies no Fresh/storage authority.

Original partial-command Stage F, exact full headers, ONE charged retained gate
before consuming Fresh materialization/current/CAS/context/high-water/remerge,
coherent durable atomic parent and source-public/raw/ETF parity are separate
successors. ClientIntent::new stays V1-only here. Missing-API RED, separately
selected S0/behavior RED, substantive source/GREEN, strict/fmt/docs/architecture
checks and independent Linux AND macOS/full profiles remain actual later gates.

## Borrowed original core-Patch frame verifier (A-NEXT-F1-CREATE1)

The verifier-only branch uses FWCP format 1, outer tag 5 and intent version 2.
Its length-prefixed payload is the complete original COMPLETE-CORE-PATCH V2
intent region. Header tenant and region tenant must match; state keys are
strictly increasing unsigned bytes. Original partial presence, empty leaves,
zero/MAX scalars and business-invalid values remain data until Fresh remerge.
Legacy tags 1–3, public V1 ClientIntent construction and held Replace tag 4 stay
within their existing contracts.

One paid domain structural scan returns ScannedCorePatchFrame with private
same-frame borrows and fixed parsed scalars. It exposes only original_intent,
with no constructor, derives, account, paid count, token, callback or credit.
Restoration consumes that mathematical view; no unrelated frame/header/region
can be supplied. Only the raw ports verifier recomputes SHA and privately mints
VerifiedCompleteCorePolicyPatchFrame on its continuous received charge. Domain
view/header data cannot attest actual SHA, account continuity or install rights.

Profile equality and actual frame cap precede the structural scan. Header RAW
caps are tenant 8192 and group/recovery/request 256, followed immediately by
nonempty checks. The checked isolation bound follows the third ID before the
request and intent fields. Complete outer bounds precede nested inspection.
Intent SHA is prepaid after structural validation; digest comparison is paid
before all header copies and later scope planner/measure/reserve/emission and
frame SHA. An early structural ScopeLimit therefore precedes IntentMismatch.
Every reached read, comparison operand, copy and tail event remains separately
paid; refusal preserves prior work and scratch without refund. No second
structural scan, original Patch/Data materialization or default expansion occurs.

The existing fallible FWSC codec and exact private ports SHA dependency are
reused. Canonical scratch P excludes borrowed input, owned header/isolation and
held simultaneous owners; source moves and fewer parses prove no native gain.
Full independent literals, six readable tests and actual physical fit precede
creation. Missing-API RED, separately selected rejecting S0, behavior RED,
substantive SOURCE/GREEN and independent Linux AND macOS remain required.
Frame preparation, consuming decoder, ONE retained gate, Fresh-only original
partial remerge and coherent atomic parent remain separately selected successors.

## Original core-Patch frame producer (A-NEXT-F2-CREATE1)

The selected producer extends existing caller/core_patch and ports core_patch/
frame modules. It introduces no production file, dependency, retained model or
account capability. Public ports preparation accepts raw owned request input,
schema/admission limits and its actual mutable PolicySchemaCharge, and returns
only existing PreparedBytes. Domain returns the unchanged original Caller and
mathematical intent/frame lengths; its tuple grants no reusable paid credit.

Private CompletePolicyIntentSink has exactly two implementations for existing IntentHasher
and Writer. One canonical original-field visitor serves hashing and emission;
all six previous field-visitor bodies retain their grammar. The immutable Caller
supplies header tenant, original tenant, metadata and original partial command.
No isolated intent Vec, merged VALUE substitution or public accepted-Caller
encoder is added. Existing verifier, public V1, legacy tags1–3 and Replace tag4
remain within their current contracts.

Let A be the four ID byte lengths, B the original intent bytes, N its measured
nodes, H=79+A the full header including payload-u32, F=H+B, M=128*N+B and
MH=768+H. Profile equality precedes payment. Paid request/scope and original
measurement precede header measurement and the checked full F cap. The same
account then admits distinct stream M, original SHA128+B, reserve128+F, header
emission MH, original emission M and full-frame SHA128+F events. Every admission
finishes before actual ports hashing/allocation/emission. Refusal retains prior
work and maximum scratch; fallible output failure retains admitted work.
G=384*N+6*B+4*H+1920, new scratch0. Minimal B52/N5/H83/F135 costs4484;
F134 refuses at1543 before stream. Global +1 fills exactly1MiB before its
Remove tag refuses, retaining1050240 rather than the different payload refusal.

Prepared output uses the existing fallible Vec writer with complete-frame caps
and exact final-length comparison before finish. Vec reserve can exceed F;
boxing may relocate/shrink unless length equals capacity. Original boxed IDs,
command Vec/tree storage, output capacity and SHA/stack/held owners coexist
outside canonical scratch. Source structure proves no native performance gain.

Three approved private test children freeze sixteen original frames and six
header variants, full SHA/bytes, partial presence, scalar/RAW/global/scope caps,
independent generation1/2 VALUEs and same-account refusals/held outputs/priorP.
Actual missing-API RED precedes separate rejecting S0 and substantive body
orders. Current Linux/macOS execution, matched performance, consuming decoder,
ONE retained gate, Fresh-only remerge, atomic parent and full parity remain gates.

## Consuming original core-Patch decoder (A-NEXT-F3-CREATE1)

The domain's private decoder consumes the sole immutable, bound frame header.
It reparses its original intent through the existing Reader, preserves all
Unchanged/Clear/Set and optional distinctions, and constructs the original raw
partial command. Header scope/request/stamp owners move into that input; no
second ID copy or default/business admission occurs. Input::into_parts moves
these owners back out without granting payment or apply authority.

The raw ports decoder runs the complete unchanged verifier on its received
schema account before its private consume_verified_frame reconstructs input on
that same account. DecodedCompleteCorePolicyPatchFrame keeps private fields,
immutable getters and a consuming input accessor. Frame and original-region
bytes remain borrowed; verified digest and isolation move. No public accepted-
Verified bridge, constructor, account token or reusable credit is introduced.
The domain header binds immutable mathematical bytes, not actual SHA, Fresh or
storage authority. Existing producer, verifier and legacy/Replace contracts
retain their bodies. Reader optional and copy_leaf visibility widens only
within policy::complete, with existing copy behavior and other Reader contracts.

For original bytes B, reached structural nodes N, adjacent-key comparison cost K,
owning objects O and copied-leaf cost C, fresh reconstruction pays
D=128*N+B+K+128*O+C. Each copy pays128+length before its fallible reserve/copy;
each adjacent comparison pays128+previous_length+current_length. Refusal retains
prior work and peak scratch. No refund, replacement account or second hash is
used. Minimal B52/N5/O2/C129 costs1077 after verifier4423, total5500 and new
scratch0. Tight total5499 retains5372. Incoming original state-change count
remains wire/work-bounded rather than limited by the merged final-state cap.

Canonical scratch0 excludes native Vec/tree capacity, moved IDs/isolation,
hash/stack state and simultaneous retained owners. Exact Vec reserve can exceed
the requested size; boxing can move or shrink spare capacity. BTreeMap insertion
has no recoverable allocation Result. These source/accounting facts establish
no measured allocation, RSS, throughput or latency gain.

Six tests preserve independent frozen frames/header variants, original partial
presence, business-invalid values, same-account fees/refusals/prior peaks,
ownership moves and equal-merged-value/distinct-original identity. Real missing-
API RED and separately selected uniform-refusal behavior RED precede the body.
The raw codec is independent of the ordered apply successor: verification then
exactly ONE retained gate must precede Fresh-only reconstruction, coherent
current/CAS/context/highwater, paid remerge and atomic proposed writes. Full
public/original profiles, both-host execution and durable adapters remain gates.


## Charged header-only original core-Patch gate

A/P-NEXT-F4-GATE1 and the separate TEST1/S01 decisions preserve one shared
legacy/charged gate. Header and view lifetimes are independent; the only returned
reference borrows retained authority from the view. Legacy None performs no
debit and no checked fee arithmetic. It may still evaluate length getters and
helper branches; source factoring certifies no native cost equivalence.

The charged wrapper first checks the same account's immutable profile at0.
Reached events then charge root128; scope128 plus both scope-ID lengths;
control128; one retained get128 plus request length; retained scope128 plus both
scope lengths; retained request128 plus both lengths; metadata128; intent196;
expiry128. Each event admits atomically before its comparison or lookup, with
scratch0 preserving any prior peak. Exact LogReplay stops before lookup, and
intent conflict precedes expiry. Work refusal preserves the preceding scalars.

One-byte IDs give Fresh519, LogReplay390, Conflict1107, Replay/Expired1235.
After the independent raw verifier4423, the nine prefixes are4551/4685/4813/
4942/5076/5206/5334/5530/5658. Two Fresh raw calls total9884. Real held indexed[a]
restore1928/P64 plus one Fresh totals6934; two total11876. Wider2/3/4/5-byte IDs
independently verify actual operands with V4476/Fresh535/Replay1271. The tests
freeze exact and tight fees and hold/reobserve the original owners throughout.

These are logical events, not native tree-comparison/CPU/allocation bounds.
Native equality may short-circuit; BTreeMap lookup compares internal keys.
Frame/SHA/ID/FWSC/retained/policy owners, capacities, stack/ABI/error formatting,
Drop and simultaneous live-owner sums remain outside scratchP. No automatic
Vec-to-Box conversion, constant-time or measured resource gain is certified.

The gate itself verifies no tag5/V2/SHA binding and reconstructs no Patch. Only
the separately selected raw parent may freshly verify, gate exactly once and
consume that owner/reconstruct/current/remerge on Fresh. Existing opaque retained
fixtures are admitted mathematical results, not installed policy or durable
replies. Current/CAS/context/highwater/retention/response/atomic parent contracts,
full public/original profiles and independent Linux and macOS remain successors.

## Fresh-only original core-Patch request parent

A/P-NEXT-F4-PARENT2-BODY1 selects domain complete/request and ports
core_patch/frame/apply. The raw entry threads one mutable PolicySchemaCharge
through one verifier, exactly one retained gate and Fresh-only consumption.
Non-Fresh returns before original reconstruction, current/context/migration/
window reads, response copying or renewal; replay borrows only external retained
authority. The Plan's private fields preserve required revision, base control,
proposed control and the complete decision, without a public constructor.

Fresh pays current lookup and validates the present selected-profile/content/
type/V2 authority before guard/CAS. Restoration precedes captured positive
history context and merge; active high-water is checked before completed
high-water and exhaustion. The old standalone guard-first algorithm remains
unchanged through three private paid stages. Only the eighteen selected incoming
guard/absent-type/merge/generation business origins become retained rejections;
current/read/restoration/context/schema/work/output failures remain operational.

Eligible results check the original positive window and expiry, then represented
R=scope/request bytes+installed envelope+private response before fallible response
copy and retained-owner construction. Accepted response version1 is the whole
assigned canonical FWPV2 envelope, preserving governance/indexed content. Accepted
Fresh proposes policy/predecessor+outcome+control; rejection outcome+control;
Replay/Conflict/Expired control only; exact LogReplay no publication. The minimal
parent costs11178 after verifier4423, gate519 and reconstruction1077 on one
account. Ninety-four fixed prospective boundaries, current restoration, repeated
and held-owner peaks, and late cap767 refusal/cap768 acceptance are executed controls.

This pure View/Plan supplies no physical reader/configuration, strict stored
codec acquisition or atomic namespace/revision/base-control/current-predecessor
compare-and-publish adapter. Sequential supplied-view writers are not runtime
concurrency evidence. Public optional-state/default presentation and all original
source/setup/profile scenarios remain distinct requirements on Linux AND macOS.
G/P/R excludes coexisting frame/ID/FWSC/partial/current/Data/VALUE/reply owners,
native capacities/tree/ABI/Drop/live sums; retain Vec and claim no allocation,
copy, CPU, RSS or latency gain without matched native measurements on both hosts.

## Original Complete Replace frame contract

A-NEXT-M1-COMPLETE-REPLACE-FRAME1-CREATE1 selects the producer and borrowed
verifier contract. The codec now follows genuine missing-API RED and passes six
new controls within the 48-test target. macOS workspace source checks pass;
online RustSec refresh and independent Linux execution remain open.

Private domain outer/replace_frame and its scan child own mathematical frame
structure, paid header restoration and borrowed list uniqueness. Private ports
log/complete_policy/replace owns actual hashing, FWSC and fallible frame output.
FWCP format1/tag6/intent2 carries the original header and generation-free full
Replace intent. Its exact registry is WF-INTENT\0COMPLETE-REPLACE\0V2\0.
Original field presence, list order and business-invalid scalars survive;
assigned generation, current policy and defaults are outside original identity.

The shared private CompletePolicyIntentSink moves the existing CorePatchSink
grammar into intent.rs with its two existing sinks and error adapter. Replace
hashing and frame emission use one visitor. Seven redundant inherent Hasher
convenience methods are removed; new/write/finish, existing Core Patch visitors
and all old caller payments retain their contracts. The V2 scanner exposes only
its unchanged complete-private Data field tail, after its existing VALUE prefix.

New producer list checks follow original/header measurement and the complete
frame cap, before all stream/hash/output prepayments. Each reached container
and list pays its specified occurrence; each leaf pays128+length before borrowed
set insertion, after admitting scratch64*count. Duplicates within a list refuse
as operational Malformed; distinct unsorted lists and equal names across lists
remain valid. This performs no normalization or business admission. Minimal
production costs4612; minimal verification remains4423. The same caller account
retains previous work and maximum scratch after every refusal.

Verification checks original grammar, tenant and intent digest before copying
header identities, then pays FWSC and whole-frame hashing. Scanned/Header/
Verified values provide immutable mathematical content rather than reusable
payment, Fresh authority or installation rights. No Data reconstruction or gate
occurs in this codec. Independent bytes, same-account boundaries, real prior
scratch and malformed-input tests precede implementation. Consuming decoding,
one retained gate, captured-history/source-exact Fresh rejection, coherent
physical reads and atomic publication require subsequent contracts. Full
original profiles/parity and independent Linux/macOS execution remain required;
the source ownership choices establish no measured allocation or speed gain.

## Consuming original Complete Replace decoding

A-NEXT-M1-COMPLETE-REPLACE-DECODE-CREATE1 selects the two private decode children
within domain outer/replace_frame and ports complete_policy/replace. Raw ports
decoding verifies once, then privately consumes that result on the same actual
schema account. Domain consumes the header, freshly reconstructs the original
guard and complete Data, and moves scope/request/stamp into the original input.
The private-field decoded model moves FWSC and borrows exact original/frame
bytes; its consuming input/decomposition methods move existing owners.

The shared owning Data tail is unchanged. Existing V2 snapshot decoding still
pays its structural and owning roots before FWPS/version. Original Replace
reconstruction pays its root, registry, tenant, optional guard, Data structural
and owning roots, and every reached shared field/read/copy. There is no Input
wrapper fee, synthetic snapshot, assigned generation, second hash/verification,
normalization, business admission, current/context lookup or request gate.

Source-derived reconstruction is 1077 for minimal original content and 20635 for
the full fixture. Combined raw decode targets are 5500G/P0 and 43062G/P128.
Every admitted debit and earlier scratch peak persists through refusal and
repetition. Six tests now execute these source-derived expectations after
genuine tests-only missing-API RED; the target passes 54/54. All macOS workspace
source checks pass, with 472 Rust tests and four compile-fail doc tests. The full
wrapper fails only its final online advisory refresh; the separate cached
four-category dependency audit passes against the October 9 database. Actual
source reciprocal review is closed without blockers. Canonical G/P
does not bound allocator capacity, live owner totals, tree allocation/comparison,
Drop, CPU, RSS or latency. Existing
Vec/Box representation stays intact. Independent Linux execution and original
public/setup/profile parity remain required. Fresh rejection/history
and the ordered atomic parent are separate successors.


## Source-ordered normalized Complete Replace content

A-NEXT-M1-COMPLETE-REPLACE-CONTENT1-CREATE1 and the reciprocal performance
creation decision select private domain replace_content with core, governance,
indexed and rejection children. Its mathematical checker borrows Data, checks
the immutable account profile and captured history context, runs unchanged
operational complete_metrics, pays semantic stream and peak scratch, then
visits representable business fields once in pinned Elixir source order.
Operational caps, correspondence and noncanonical duplicates return outer
errors; no schema-error conversion can manufacture the private rejection.

The immutable rejection contains only its restricted reason and, for history,
the admitted original limit. Exact source response construction freshly pays
the actual received account and uses static literals or a seven-byte local
decimal buffer, then one fallibly reserved output Vec. No Data cloning, default
expansion, VALUE installation, generation, retained status or Fresh authority
is selected here. Multiple invalid typed map entries follow raw-byte BTreeMap
order; general Elixir map first-error equivalence remains unproved. Existing
standalone/core-Patch business order and payments remain unchanged.

Six controls now execute after genuine tests-only missing-API RED; the policy
target passes 51/51. Independent supplemental response-fee assertions were
added after implementation and pass in the same 51-test target. The macOS
workspace passes 478 Rust tests and four compile-fail docs, strict source lint,
architecture, documentation and reference checks. Its wrapper stops only at
online RustSec DNS; a separate configured October 9 cached audit passes all
four categories. Final actual-source reciprocal review is closed without blockers.
Logical scratch does not bound allocator/tree/live-owner/Drop/CPU/RSS costs;
there is no measured resource gain. Original raw/public/ETF/setup/profile
parity, physical atomic publication and independent Linux validation remain
required alongside macOS.

## Fresh-only original Complete Replace request parent

A-NEXT-M1-COMPLETE-REPLACE-PARENT1-CREATE1 selects domain
complete/replace_request and its retention child, ports complete_policy/replace/apply,
four private tests under the existing Replace target, and narrow existing
encoding, response-length, retained-status, export and architecture-test seams.
The parent is implemented after genuine missing-API RED; all eight new and
54 existing controls pass within the 62-test target. Strict library and target
Clippy pass. Actual written-source reciprocal review is closed without blockers.
ROOT-PARENT1-CHECK1 records the complete macOS wrapper PASS with 487 Rust tests,
four compile-fail docs and all source/dependency stages; focused counts overlap.
Independent Linux acceptance remains pending. No dependency or source-rule
waiver is added.

Raw verification and the existing charged header gate each run once on the same
account. Non-Fresh returns before original reconstruction and current, context,
migration or window reads. Fresh checks same-current V2 bytes before CAS and
requested-type equality, then captured context, one source-ordered incoming
content check, active-before-done migration high-water and assigned generation.
Current correspondence omits old business admission; the accepted Data is
measured and emitted privately after the actual successful content walk. It is
then moved into the existing VALUE owner without a second business pass.

PRIVATE-RESULT1 refines the prepared representation to a private Result alias:
accepted predecessor/VALUE or a bounded two-arm generation/content rejection.
The outer operational Result remains separate. This preserves the three
original branches without an additional heap owner or resource event; it
supplies no measured native size or speed improvement. The added
retained content status holds the immutable original descriptor/history limit.
Eligible results pay window128, aggregate128 and metadata128 before response
construction, then retained-owner128. Aggregate R counts four identity lengths,
installed envelope and exact response. Accepted retains the entire private
VALUE through one paid copy; content responses use the existing freshly paid
encoder and move its one Vec into retention. Body length is determined by a
restricted fixed-prefix/decimal helper after aggregate payment, before output
allocation. No operational error produces a retained business result.

Independent source/contract event oracles give minimal verifier4423, Fresh
gate519 and reconstruction1077, reaching6019. The selected missing-current
parent reaches9640; present current adds1144 correspondence and130 for both
type operands, reaching10914. A supplied guard adds288. Tight9639 retains9512;
two same-view mathematical Fresh calls reach19280, with tight19279 retaining19152.
A real held1928G/P64 predecessor reaches11568G/P64, total11632; tight11631
retains11440G/P64. Non-Fresh minimum totals are4813 LogReplay,5530 conflict and
5658 replay/expiry. Full382 VALUE plus its382 response and four IDs yields R768,
while its477-byte raw frame fits cap767. The focused tests now execute these
independent event oracles. Original history18/C17 uses verifier4825 and
reconstruction1595, then Fresh6939 and retained9182; its replay/expiry6060
differs from the minimum frame because its original input is wider. One late
measurement expectation was corrected from unchanged Writer source:
Sink::Measure overflow yields Schema(SnapshotBytes). The preceding failure is
retained; no production behavior or fixed counter was changed. These logical
results do not establish native performance measurements.

The supplied View/private Plan are mathematical compare inputs and owners.
Original generation0/raw/ETF/public setup and normalization, indexed counts,
migration queues, physical coherence, whole-bundle atomic publication and all
required profiles remain successors. Native capacities, tree comparisons,
stack/ABI/Drop and simultaneous frame/ID/FWSC/current/Data/VALUE/reply owner
sums stay outside logical G/P/R. Preserve Vec and fallible reserves; independent
Linux AND macOS results are required before full acceptance.

## Original indexed-policy input normalization

A-NEXT-M1-POLICY-INDEXED-INGRESS1-CREATE1 selects two private domain production
children and two children of the existing policy integration target. The
implementation follows genuine tests-only missing-API RED; its six new controls
pass within the 58-test policy target with strict lint. The full macOS wrapper
passes 495 Rust tests and four compile-fail docs. Written-source architecture
and performance review and their reciprocal review are closed without blockers.
Borrowed RawPolicyIndexedValue preserves
Nil/Binary/Atom/List/Other classes; Atom("nil") follows the source Nil branch in
each context. PolicyIndexedInputRejection keeps thirteen exact static source
messages. The entry returns outer operational errors or inner source rejection
versus the existing optional Vec fields. An optional private IndexedOwners alias
changes only spelling; there is no accepted/paid result or retained-status bridge.

After profile admission and entry128, both-tree preflight pays each attempted
node128 before depth/occurrence checks, then its actual canonical text length
after the raw byte cap. Root depth0 is limited to64, total occurrences to4096
and total raw text to262144 bytes. These operational limits are distinct from
source semantic limits. Scratch is one prospective A+64D event, with A192 for a
nonempty attribute root list and D the maximum nonempty metadata-list ancestry;
prior peak survives. Source semantics then pay reached roots, borrowed key
UTF-8/trim/bounds visits and equality operands in order. Stable unique3/1 checks
retain all later key and local nested-list error priorities.

Only after both fields succeed does present attribute output pay owner128 and
reserve128+128N, then each final key copy128+length. Present metadata pays its
single copy128+length. Fixed source-derived expectations are absent384/P0,
present attribute nil/empty768/P0 and the original four-name example4784/P192;
repeat totals9568/P192. A real prior1928G/P64 seed plus absence gives2312G/P64.
These independent oracles were fixed before implementation and now execute,
alongside depth/node/raw-byte, nested-priority and held-owner refusal controls.
Bare Binary version/owner and Nil source-field observations were supplemental
post-implementation tests; they are not attributed to the earlier RED. No native gain,
capacity or total-live-owner guarantee. Old typed APIs and encoding stay exact.
Original whole-policy options, ETF/current readers, public/setup/profile/physical
operations and both-host acceptance remain required.

## Original stored retry-key inspection

A-NEXT-M1-ORIGINAL-RETRY-KEYS1-CREATE1 selects two private production children,
two integration children, OriginalPolicyMapKey/OriginalPolicyMapValue and the
sole paid boolean predicate. The implementation follows genuine tests-only
missing-API RED; all64 policy controls and strict target lint pass. The full
macOS wrapper passes502 Rust tests and four compile-fail docs with all source
and dependency checks. Written-source architecture and performance reviews and
their reciprocal review are closed without blockers. No raw-reader acceptance is claimed. Existing APIs,
positive normalized constructors, encodings and retained outcomes stay exact.

Profile mismatch precedes entry128. Visible map/key/Other-value occurrences
pay128 before depth64/node4096 checks; actual represented key bytes are capped
at262144 before their length debit. Named-key classification compares retry,
states and max_attempts in that order, paying128+both operand lengths before
each reached equality. A six-bit per-map mask rejects repeated queried
atom/binary identities before visiting the duplicate's value. The operation
then admits one G0/P128 scratch event and executes paid source-ordered root,
key, comparison, matched-value and state-row visits. Root atom presence differs
from state guarded-map preference; membership skips its value. No dynamic
output owner, default, accepted credit or generic term-identity proof is added.

Independent pre-implementation expectations now execute: emptyG512/P128/T640,
root-old-keyG2313/P128/T2441 and queried duplicateG926/P0. Complete visible
graphs have odd descriptor count1+2E:4095 succeeds atG1048576/P128, while
attempted4097 refuses atG524544/P0. The corrected refusal counts the root once.
Real held1928/P64 and4784/P192 seeds preserve continuous charges and maxP.
Native stack/masks/input construction/comparisons/Drop/live-owner sums remain
outside these logical bounds. No measured native gain is inferred.

The original raw reader below uses bounded paid backing and the shared
borrowed-span selector for ETF safe atoms/functions, exact consumption, encoded
cap, generation0..MAX, arbitrary maps and callsite type/error distinctions.
Recursive slice input does not itself prove allocation-free construction.
Original assertion603 now has an executed generationless shape control and reviewed
direct supplemental ledger anchor. Caller-normalization584 stays
distinct and unlinked. All original setup/profiles/physical and Linux/macOS evidence
remain acceptance work.


## Implemented original raw entry reader

A-NEXT-M1-ORIGINAL-ENTRY1-CREATE1 selects the raw-entry feature, eight private
domain children, one private ports child and six test children. Reciprocal
consultation and actual written-source reciprocal review are closed. All six test
children preceded production with genuine missing-API RED. Domain72/72 and ports65/65
pass, with strict target lint and the extended architecture control. No public general term tree, accepted-entry
encoder, authority/payment flag, dependency or rule exception is selected.
A-NEXT-M1-ORIGINAL-ENTRY1-REFERENCE-BYTES1 adds one purpose-specific domain
raw-read byte-preparation API because the work account has no ports debit API.
It freshly reads once, copies or emits the exact source bytes, and prepays SHA
and reference metadata before returning only generation/Vec. Ports hashes its
output; metadata refusal may precede native SHA execution. This conservative
schedule preserves all selected logical fees and introduces no payment bridge.

An owner-free syntax/safety pass counts N spans, K map keys and F local functions.
After its success the received account admits scratch64*(N+K+F) as a SUM, then
prospectively pays the three separate reservations128+32N,128+4K and128+64F,
including the fixed zero-count events. Reserved pass2 repeats full byte/node/
safety work and charges actual captured/virtual lookups and insertions.
Bottom-up map uniqueness, strict original envelope/generation, fresh shared
retry-key traversal, atom-only type selection and entry metadata follow in order.
All temporary owners drop before original bytes escape as borrowed entry fields.
Every failure preserves successful G and maxP, with no partial entry or refund.

The independently fixed minimum is G3736/P256/T3992; type-t K1 is G6778/P448.
Raw read adds258, then present reference copy/hash/metadata adds460 for
G7496/P448/T7944. Absent reference has B=37+query bytes and G2048+4B/P0;
type-t costs2200 and checkout2228. These independently fixed expectations execute
against the reader. F1 costs G9664/P832/T10496; captured F1 costs9862/P832/T10694.
Every positive minimum event, separate K/F reserves, actual insertion/reuse,
comparison/copy/hash/metadata, repeated accounts and retained scratch peaks have
fixed source-derived refusal controls, without learning fees from returned totals.

Semantic comparison charges both normalized cell widths and freshly reached
subtree/rescan work. The selected runtime context distinguishes active exports,
captured local functions, first virtual entries and static reference branches;
it grants no runtime presence, liveness or cross-call coherence. Original
source603 remains a two-tuple shape rejection, separate from incoming584.
LOCAL, atom-index tags73/75, empty BIT len0/bits0 and zero-word NEW/NEWER
references return operational UnsupportedTag in the selected portable profile.
The zero-word native result is unknown; no VM replay or source-invalidity claim
supports that exclusion. Nonzero empty BIT is source-invalid; ordinary empty BINARY
and valid1..5 references retain their selected semantics and fees.
Original public physical/profile/both-host acceptance remains required.
Vec capacities, stack, caller context, original bytes, Drop, simultaneous native
owners and latency are outside logical scratch; retaining Vec adds no native
performance or recoverable-OOM guarantee.

## Implemented original-current replacement composition

The reciprocal A/P-NEXT-M1-ORIGINAL-CURRENT-REPLACE1-CREATE1 decision selects
one private domain child, two borrowed-input/error models and two six-operand
APIs, using the existing ports parent and Plan/Fresh/retained-status registry.
The View separates gate, raw-map and runtime-context lifetimes. Confirmed miss
alone is Absent; a nil/nonbinary HIT is corrupt. These are mathematical inputs,
with later coherent-read and atomic-compare duties unchanged. Eight new tests
preceded production with independently reproduced missing-API RED; all 73
controls and strict domain/ports lint now pass.

The existing verifier and request gate each execute once. Every non-Fresh
decision bypasses reconstruction and current/context/migration/window reads.
Fresh pays the common root and current lookup, including class inspection,
then performs one complete unbound original decode before CAS. Its existing
type projection/metadata fees stay before CAS; semantic atom:type binding and
its both-operand comparison follow CAS. Malformed current beats stale CAS,
while a valid foreign type with stale CAS retains the stale result first.
No stored-map content admission, Data reconstruction or second parse is added.

Both typed and original-current paths share only the private post-type tail:
captured history context, one incoming content walk, active and completed
migration reads, allocation, and same-Data late encoding. Existing outcome
retention is reused. Common parent/gate/tail errors wrap the existing error;
reached reader/type-debit failures preserve Current(Encoding), and current
corruption displays `ERR corrupt flow policy high-water`. Public Policy
corruption spelling and all existing API/event priorities stay unchanged.

Independent fixed inputs select absent G9640 and TYPE_R38 present
G16548/P448/T16996. Final tight16995 retains G16420/P448; two calls reach
G33096/P448/T33544, with tight33543 retaining32968/P448. A real held1928/P64
prefix gives G18476/P448/T18924. Reader scratch/reserve and every reached
phase remain prospectively paid; Drop does not refund or reset the account.
These independently fixed source-derived expectations now pass in the executed
tests, with no native resource or speed claim. The exact private architecture
control also passes. The private two-map ReplaceMigrations alias was explicitly
approved before creation after strict lint diagnosed tuple type complexity;
private view helpers borrow without changing the public API or fee stream.

Private V2 proposed values/replies are an explicit semantic encoding and must
be refused by the original ETF input path. Original ETF emission/sizing,
raw option/retry/governance normalization, migration codecs, public responses,
physical policy/outcome/control publication, complete runtime profiles and
independent Linux/macOS execution remain required in the full ordered plan.

## Implemented original retry input normalization

The mutually reviewed A/P-NEXT-M1-POLICY-RETRY-INGRESS1-CREATE1 contract selects
borrowed raw retry descriptors and caller diagnoses, returning existing full
or optional override models. Two private production children and two test
children preserve source nil/presence/defaults, atom masking, last keyword
duplicate and ten exact field errors. Eight new tests preceded production with
independently reproduced missing-API RED; all 80 policy controls and strict
target lint pass. Two ordinary test-helper borrowing lint failures were repaired
without changing an input, assertion, output or resource oracle.
Actual source and assertion-mapping review is mutually closed without blockers.
Existing normalized, stored-reader and retained-status contracts stay unchanged.

The one visible tree is preflighted before semantics: depth64, count4096 and
represented text262144. Every attempted descriptor pays128 before its count/depth
refusal; text caps precede text debits. A key's classification and queried-key
duplicate check precede its value. Other hides unrepresented term identity/bytes;
no arbitrary map uniqueness, raw ETF or native memory proof is inferred.

Semantic root work precedes nil/shape/collection; collections declare logical
P128. Each reached field lookup scans and pays independently, then validators
run in original order. Atom nil is canonicalized after its visible three-byte
preflight and before kind spelling. Final metadata precedes the single fallible
target reservation/copy after every validation. No allocating default, temporary
Map.new owner, normalization credit or blanket Vec-to-Box conversion is selected.

Independent targets include nil override512/full646 atP0, atom-nil515/649,
empty override1024/P128 and empty full2566/P128. Actual one-row backoff-nil
input is4124/P128/T4252; the proposal's2176 subtotal was corrected before any
test/source creation. A nonempty target `done` gives3459/P128/T3587, with
tight3586 retaining3327/P128 before allocation. Existing held-owner seeds and
all 1800 original generated normalizations now pass. These are source/input-derived
logical expectations, without measured native gains.

Original whole-flow ingress order, other raw groups, ETF output/sizing, public
and atomic physical publication, complete profiles and both OS remain required.


## Implemented original retention input normalization

A-NEXT-M1-POLICY-RETENTION-INGRESS1-CREATE1 selects one private production child,
two existing-target test children, one caller diagnostic and two APIs. It reuses
`RawPolicyRetryValue`, `OriginalPolicyMapKey` and Copy `RetentionOverride`; the
retry-oriented descriptor name is retained without claiming a generic term
parser. Only existing selector visibility/reexport widens within complete; the
actual body, retry classifier and all old public fees remain exact.

Nil returns no override; empty collections preserve a supplied empty override.
Atom presence masks binary keys and keywords select their last duplicate.
Source order is positive TTL up to31536000000, forbidden hot-key presence, then
positive history up to original captured C. Missing fields are not defaulted.
The diagnostic owns C; the encoder validates1..1000000 operationally, uses static
bodies or seven local decimal bytes, then one fallibly reserved final Vec.

Matching profile precedes entry128 and captured-C admission. Visible input
preflight pays descriptors before depth64/count4096 refusal and applies the
represented-text262144 cap before text debit. Queried map-key duplicates reject
before their value; shadowed/unknown values and alias occurrences remain bounded.
Collections declare logical P64, freshly pay each reached selector and present
numeric validator, then successful metadata. Nil is512/P0, empty896/P64,
TTL1 single row2404/P64 and history50 with C100 gives2832/P64; C10 rejects at2704.
Fixed diagnostic fees are311/315/309; C1/17/1000000 history bodies cost318/320/330.
These expectations were frozen before production and are not native measurements.

Six meaningful tests preceded substantive implementation with genuine missing-API
RED independently reproduced by Root. All86 policy controls and strict target
lint pass without postproduction oracle changes. Actual source/mapping review
is mutually closed without blockers. No retained status, authority token,
full-default/resolved API or dependency is selected. Original whole-flow/public/ETF/physical/setup/profile
and independent both-OS acceptance remain required.


## Implemented original whole-flow snapshot producer

The explicit whole-ingress creation decision approves three borrowed/public
diagnostic models, two snapshot/body APIs and eight private cursor/staging models
in fourteen cohesive children, with seven existing-target test children. Root owns
ETF/map ordering and shared boundaries; Writer owns the source semantic walkers.
Both authored substantive tests first; independently reproduced missing-API RED
preceded production. All100 policy controls and strict workspace lint pass.
Source-derived post-RED corrections and supplemental wrong-class refusal checks
remain distinct from that original RED buffer. The full macOS run passes554 Rust
tests and four compile-fail docs, then stops at the default RustSec DNS failure.

The complete original order and nil/presence/error behavior apply to every top,
state and governance group. Customer version magnitudes may exceed u64; output
is deterministic original ETF, not normalized V2. Exactly the reachable finite
normalized subset has a source-proven equal external-size/emitted-length metric.
Governance size is checked after its seven groups and before states; whole size
follows states. Synthesized binary maps use final unique key count, ascending
binary order through32 keys and descending swizzled pinned binary hash above32.

MASK1 freezes the46-name preflight registry for actual represented Map keys only;
it must not classify generic List/Tuple descriptors as keywords. Old primitive
preflight tables/fees remain unchanged. New visible bounds are65536 occurrences,
depth64 and64MiB represented text; the mandatory600 unique~2k governance leaves
must reach their exact caller size error. Concurrent cells/edges/work and bounded
staging contribute summed logical scratch before all three fallible reservations.
Every later selector, comparison, placement, size visit and writer visit pays
freshly. The independently fixed empty-map stream is G9374/P4096/T13470.

The returned snapshot Vec has no generation, origin, paid or retained authority.
Original frame/ports/Fresh replacement integration must consume the same private
layout, preserving one admission/gate and source ordering; those successor artifacts
are not approved in this batch. Physical/public behavior, all pinned source test
assertions/setup/profiles and both native OS remain required in ordered M1–M8.

The source-only CONTIGUOUS-EDGES1 correction stages each completed dynamic-map
child/name in the existing work slot with one paid128 update, then appends all
enclosing edges after every child. Nested child ranges remain contiguous. The
two-state stream is27396/P9664/T37060; its older frozen RED vector remains
historical. This adds no owner/model or source business pass, and changes no old
primitive, MIN or Gov-list stream. No implementation result supplied the revision.

## Selected original public Policy.set preparation

ARCH-NEXT-M1-ORIGINAL-PUBLIC-SET1-CREATE1 and its
GROUP-AND-COMPACT1 amendment select a pure domain predecessor to public dispatch.
The five private children below `policy/complete/public_set` own controls,
immutable patch views, conversion, synthesized-row compaction and raw-size
measurement. Three public models and two functions are reexported through the
existing facade; no wire, client, runtime or authority contract is added.
The existing architecture test checks all five descendants and the five exports.

The request owns one prefixed policy key, borrows response-state and raw payload
bytes, and retains a bounded cell/edge arena. FIRST control occurrences determine
state, expected generation and replace; both replace modes survive. Ordinary
keyword values collapse LAST before recursive conversion. State-pair callbacks
all run in source order before duplicate compaction. Tuple interiors and map keys
remain conversion barriers; the reached struct predicate is checked at the
Map.new/2 boundary, before value callbacks. Converted raw size precedes
current-policy/CAS/business
validation. The seven fixed caller diagnoses remain distinct from operational
profile, work, shape, allocation and unsupported-term errors.

This new represented-input profile admits131072 occurrences, depth64 and64MiB
of visible bytes; every old profile and fee remains unchanged. One continuous
account pays entry, preflight, logical SUM scratch104N+128D+256, three fallible
reservations, reached selectors/conversion, measurement and late routing-key
ownership. Alias occurrences are charged separately. Temporary slots drop before
return; the retained view supplies no captured-order, origin or payment authority.

The same temporary slot Vec stages eligible rows, performs a paid heapsort by
exact key bytes and source ordinal, then compacts adjacent groups to their LAST
entry. This replaces the initially proposed all-prior hash scan: cached reads
plus required scratch already exceed the default128MiB work allowance at4096
distinct keys despite a49158-byte valid patch. Cached work134184960 plus
scratch1278672 is135463632, above134217728 before other reached work.
Fresh operand reads, comparisons, swaps and real compact
moves remain individually charged. Hash equality never substitutes for exact
key equality. This is a bounded-work improvement, without native latency or
allocation measurements and without a claim that every small input is faster.

Four reviewed test children contain eight substantive tests with independently
derived fee streams, seven literal caller bodies, raw-size boundaries, held
owners and an actual default-budget4096-key control. Root independently reproduced
the final missing-API RED before releasing the five production files. All112
policy tests and strict lint now pass, and Root independently reproduces the
complete macOS582-test/four-doc-test wrapper with all source/dependency stages.
The pre-SUT predicate-order supplement, the source-review improper-terminal
correction/regression, both RED epochs and ordinary compiler/lint failures remain
separately recorded.

Default-false arbitrary-current merge, coherent source/VM order and opaque-term
identity, public replies and committed identity, physical atomic/all-shard
installation, every required original setup/runtime/assertion profile and native
Linux and macOS execution remain required successors. Preparation does not grant
those capabilities or full original public-call parity.

Surviving Raw Other terms currently produce operational UnsupportedTag during
measurement; reached struct Enumerable dispatch is likewise an explicit
temporary unsupported profile. Neither gap is a product exclusion.

## Selected original public Policy.set frame and both-mode Fresh

ARCH-NEXT-M1-ORIGINAL-PUBLIC-MERGE-CREATE1 selects eighteen privately grouped
production files and twelve test children within the existing targets. Root
independently reproduced both final tests-only missing-API RED results before
releasing production. All eighteen bodies now exist. Integrated tests, strict
lint and every source-check stage pass in the595-test/four-doc native macOS run.
The wrapper subsequently exits1 at its default RustSec refresh because GitHub
DNS resolution fails. A separate cached October14 audit passes all four
categories; that is not a default-refresh success. Written-source architecture,
performance and reciprocal reviews are closed without a component blocker.
Earlier complete582-test wrapper results belong to
the preceding preparation checkpoint, and native Linux remains pending.

The new frame has discriminator9 and request registry4. Mathematical intent
begins with `WF-INTENT\0ORIGINAL-POLICY-SET\0V1\0`, then binds raw tenant/key,
replace, optional reply-state bytes, optional expected generation and canonical
patch. The whole frame also carries scope/request/stamp and the captured patch
source-order sidecar. Producer hashing is patch, intent, digest overwrite and
whole frame, followed by the final supplied-context comparison. Verification
hashes intent and patch, restores and binds represented capture correspondence,
then hashes the whole frame. Hashing remains exclusively in ports.

Canonical patch bounds are131072 nodes, depth64 and262144 scalar payload bytes;
the independent reply selector admits64MiB and the key admits65535 bytes.
Received raw public values keep their64MiB physical payload admission, including
large padded integer encodings; canonical measurement and emission freshly trim
integer padding. One bounded ordinal Vec supplies canonical staging. The producer
returns exact intent/patch ranges for the ports hashes, without a domain digest
or runtime authority.

One structural verifier and one existing request gate precede Fresh. Fresh reads
strict current bytes, checks the raw expected generation and binds atom:type.
True replacement drops current decoding owners; false replacement binds the
captured exact current bytes, builds its child catalog from retained spans of
that same decode and uses
four bounded merge owners. Exact keys take precedence over source aliases;
resolved atom-state nil values delete, while proper empty lists remain distinct.
Struct Enumerable guards run only when the corresponding source operation is
actually reached. Surviving known values enter one source-ordered business
layout; overwritten current values cannot fail an earlier independent business
admission. Late migration high-water checks and generation allocation precede
direct original ETF emission from that same layout.

The new-only current scanner admits263168 encoded spans and threads a measured
span bound into deprecated-map traversal. Old public wrappers retain4096 limits
and exact work streams. The explicit source-backed1,000-rule current/true-reset
regression is a post-initial-RED refinement, not part of the frozen RED manifest.
No source fixture, initial expected fee or previous failure is rewritten.

Logical resource accounting reserves every simultaneously live index, capture,
merge arena, joined layout, installation ETF and key before allocation. After
transient owners drop, a separate zero-work peak admission covers retained
metadata, final key, installation and distinct response. Historical peak values
cannot substitute for this active-owner sum. Native allocation or speed is not
inferred from these logical reservations. The proposed installation has no
physical publication authority, and its original-envelope retained response is
not yet the final public reply-state projection. Coherent VM capture, opaque
ingress identity, complete original scenarios/setup/profiles, physical atomic
all-shard publication and independent Linux/macOS acceptance remain required.

## Selected original public policy reply views

ARCH-NEXT-M1-ORIGINAL-PUBLIC-REPLY1-CREATE1 selects eight private production
files and six test files. The pure domain projects original SET and GET replies
from borrowed ETF donors and explicit caller defaults. SET decodes its returned
value without a type-binding check; GET binds the stored type. Missing GET uses
generation zero and caller defaults. Read failures precede unused context.

One retained structural index supports generic values and exact-key backoff
overlays. A cached type-default record is shared across state records. Four
bounded indexed names use fixed inline storage; records and their ordering keep
fallibly reserved Vec capacity after LAST compaction. Public map iteration and
atom lookup pay before observation and preserve iterator state on refusal.
Resident reservations include explicit additional caller owners; historical
peak charge is never an ownership credit. These are logical work and ownership
bounds, without a measured native speed or allocation claim.

Source Enumerable dispatch, eager callbacks, false/nil fallback and independent
indexed-helper failures retain source order. Captured native state-map order is
an explicit correspondence input; synthetic reply maps use specified logical
order. Complete opaque ingress identity and custom Enumerable runtime support
remain required; reached unsupported profiles return operational `UnsupportedTag`.
The Fresh/Replay caller test consumes real existing plans and keeps
stored intent/value immutable while applying current caller defaults. It does
not publish a physical result. Full wire output, coherent ingress capture,
physical atomic/all-shard publication, original setup/runtime profiles and
independent Linux execution remain required.

## Original public Policy.get ingress and reader composition

ARCH-NEXT-M1-ORIGINAL-PUBLIC-GET1-CREATE1 selects two private domain files and
one private ports file, with seven behavior tests in three files. Domain
preparation validates the complete keyword shape, then nonempty type, FIRST
state and key size in source order. One advancing list cursor preserves FIRST
nil and ignores unrelated values without SET patch conversion or recursive
payload traversal. Reached atom keys retain their source character bound.

The policy-specific reader and caller live under `wf-ports::state`, separate
from prepared log commands. The caller always prepares fresh raw input and
invokes one read using allocator shard zero and the derived policy key. The
returned bytes borrow the reader independently of that temporary key. An
explicit key-owner drop precedes the unchanged generic GET projector, which
retains read/decode/type origins and the supplied caller context.

Preparation prospectively pays the immediately following read invocation.
Its local peak covers this request's key and scratch only; raw inputs, reader
storage and earlier standalone requests remain external to that stage. Held
request tests verify preservation and cumulative work, without claiming their
summed live memory. The caller passes additive context owners unchanged and
does not infer ownership credit from the account's historical peak.

This source compatibility capability does not select blocking production I/O,
strong-read barriers, tenant authorization, cache suppression or a concrete
storage adapter. Tests record actual capability invocation and exact replies;
they do not execute physical reads or original setup/runtime profiles. Native
allocation/latency, opaque ingress provenance, full wire output, all required
source scenarios and independent Linux/macOS acceptance remain required.

## Selected original indexed mutation batch composition

ARCH-NEXT-M1-RECORD-BATCH1-CREATE1 selects three private production files under
`wf-ports::log::record_batch`, three behavior test files and one architecture
test child. The six semantic tests pass after independently reproduced
missing-API RED and implementation. The exact
field/code tables and named shared seams are bound in the
[architecture decision](../reviews/workflow-completion-architecture.md#arch-next-m1-record-batch1-create1).

The producer consumes a PreparedRequest, independently computes its operation
intent and replaces the supplied digest without cloning the command tree. The
consumer structurally scans once, prospectively admits decoder ownership, then
decodes actual FWCP bytes. Both header directions accept intent1 for every
existing root, or intent5 only for Batch; other combinations reject. ClientIntent's new
record_batch constructor is mathematical and grants no validation authority.
Existing registry1 construction and the other dedicated registries stay intact.

Registry5 streams `WF-RECORD-BATCH`, NUL, `V5`, NUL, tenant and the meaningful
ordered mutation list through SHA256. Fixed-width big-endian fields, explicit
operation/backoff tags and absent/present byte fields define identity. Caller
partitions, IDs, guards, values, create priority and explicit deadlines remain.
Committed clocks, request/group/recovery identities, generated policy jitter,
policy-resolved retry time and unused non-create priority are excluded. A
RetryWithPolicy override retains one optional complete Backoff tuple. This is
a prepared-value contract; earlier caller/default provenance remains separate.
The eight supported mutations are Create, Transition, Complete, Fail, Retry,
RetryWithPolicy, ExtendLease and Cancel. Named Claim/Reclaim remain available
through the old codec and their appropriate data-reply paths.

The [selected pure StepContinue core](004-committed-time-and-apply-admission.md#selected-pure-committed-stepcontinue-core)
is not a ninth Registry5 mutation. Its command and intent encoders return
UnsupportedTag; the new DirectRunningFifoEntry response reason also remains
unsupported. Existing decoder/tag grammar and independent/public allowlists stay
closed. No operation, response or history tag/version is added by that core stage.

The [selected pure StartAndClaim core](004-committed-time-and-apply-admission.md#selected-pure-committed-startandclaim-core)
also remains outside this eight-mutation profile. Both shared command and intent
encoders explicitly return `UnsupportedTag`; value inspection within those
profiles returns no supported value shape. Decoder grammar and independent/public allowlists stay
closed. Storage's existing Create-only policy discovery does not discover
StartAndClaim policy; that durable integration remains required separately.
The passing pure body supplies no public or durable operation support.

### Selected private atomic-start codec

Six server-side functions retain their existing signatures:
`encode_atomic_start`, `prepare_atomic_start`, `inspect_atomic_start_point`,
`decide_fresh_atomic_start_touched`, `atomic_start_retained_summary` and
`atomic_start_response_matches`. Six compiled regressions reached a valid first
encode and failed with `UnsupportedTag`; they were frozen in `37afcf6` before
this body release. All six now pass within the 22-test focused target and full
981-test macOS checkpoint. The feature root and three private
codec/response/response-scan children introduce no new
type, public API, dependency or legacy decoder allowance.

The isolated prepared grammar is FWCP format1, top-level Batch1, intent7,
exactly one item and item tag11. Caller intent hashes `WF-ATOMIC-START`, NUL,
`V7`, NUL, then fixed-width big-endian length-prefixed raw tenant, partition,
ID, type, logical initial state and worker, followed by lease, priority and
explicit payload absence/presence/content. It has no V5 batch count or opcode.
Request/group/recovery identity and coherent prepared command time/stamp are
excluded from caller intent but must survive exact frame encoding/decoding.
Borrowed count, field, key-length, optionality and EOF checks precede owned
materialization and use the existing checked decoder admission. The primitive
intent constructor authenticates no digest or origin. Shared command/intent
codecs and independent/public operation profiles stay closed.

The new retained response is FWAS format1, externally paired as response4 with
intent7. One accepted/rejected kind follows its six-byte prefix. Accepted data
represents the actual one-record Changed/expectedNone Created plan, full leased
postimage, raw worker/token, v1/fence1/attempt1, original times, ordering and
payload presence; fresh result/error remain absent. Rejected data retains the
exact indexed business reason, including local DirectRunningFifoEntry, without
an accepted plan or count. Accepted summary is Some(1), rejection is None.
The old rejection grammar stays unchanged. Writer measurement, allocation and
verification share one grammar; a matcher scans the complete actual bytes
before comparing the actual plan. Valid different content yields false;
malformed/profile/cap errors remain errors.

Only the primitive outcome constructor gains response4/intent7 with Accepted or
IndexedRejected. Its broad existing response1 branch is unchanged and does not
independently reject every mathematical intent7 value. A generic point gate
does not parse response bytes. The atomic-start Replay branch must therefore
scan the original response, using original metadata and the decoded request's
current captured limits, and require exact status/count before returning the
same borrowed owner. Syntax/profile/cap failures remain Encoding errors; status
inconsistency is InvalidAuthority. Earlier generic gate precedence is preserved.
Original log/time/expiry are not renewed by later processing control. Standalone
syntax/index checks do not authenticate term/node identity or persisted origin;
Fresh binds full original metadata to its actual control before splitting.

Encoding retains checked input/output admission. Decoder and Fresh/late logical
accounts are specified in ADR-004. R is measured before output allocation and
must fit B and each represented field bound. Accepted responses also require
the implicit seven-byte physical `running` label to fit the inline limit.
A retained `ValueTooLarge { bytes: 7, maximum: 6 }` rejection remains scannable
with an inline limit of six. The None response is 114 bytes, present-empty 118
and one-byte payload 119. Real Some(v)/Some(x) responses of the same 119-byte
length must not match each other's plans. Fresh workspace6531/6532
and standalone SAME-response caps114/113 and token8/7 are distinct tests; no
late-only or tiny-response-budget Fresh witness is claimed.

The stored integration below adds GroupApplyChanges, FWSO encoding/scanning,
policy discovery and transactional adapters under separately saved regressions.
Common recovery dispatch and a recovery-header
helper are deferred until an actual selected caller needs them. No private
postimage alone establishes durable/public/profile or Linux/macOS acceptance.

### Selected stored atomic-start integration

The seven compiled regressions saved in `be13b91` precede this selected body.
The existing dedicated `apply_atomic_start_transaction` reuses the common
exclusive transaction driver with isolated preparation and point inspection.
Fresh discovers StartAndClaim's actual type through the existing charged query
and policy path, including absent current records. Accepted publication includes
record, ordering, indexes, Created history, outcome and control in one consumed
Immediate commit; rejection publishes outcome/control. Replay returns the
original response after committing only processing control. Failed or uncertain
commit exposes no retained acknowledgement.

The private ports `state::transaction::atomic_start::validate` compares original
scope/full log/time/status and singleton count against independent actual Fresh
control, then matches the actual plan/postimage/history/response. Its existing
hard ceilings (512 MiB inline and 100,000 items, with actual response length)
check structural coherence. They supply no captured-cap or persistence
certificate. Existing generic predecessor and older-profile checks stay intact.

The private storage `state::value::outcome::atomic_start` functions `wire_status`
and `scan` share D1 response validation through unchanged FWSO1 framing. Only
intent7/response4 gains this route. Healthy narrowed response limits reach
`ProfileLimit`, including the same donor at decision114/113 and inline8/7.
The backend independently repeats scanning under actual transaction admission
before output-key/frame allocation; point and reopen readers use the same scan
before admitting/copying a returned owner. Existing older error origins remain.

Retained ownership stays `O = 512 + H + R` and wire ownership
`F = 102 + H + R`. These and the key debit use their existing cumulative banks;
the reserved read/staging/adapter sum L is charged once. Fresh moves the same
producer Box; Replay moves the original response Vec after erasing its borrow.
Reopen pays for a new independent owner. No layout/event version, cloned
response, tariff, common Normal recovery dispatcher or public-wire route is
introduced. All seven unchanged regressions now pass within the full 988-test
macOS checkpoint, including the complete state suite. Full original profiles,
parity and Linux acceptance remain required.

### Existing Registry5 inspection and scalar retention

One existing request inspection precedes all Fresh current-record, retry-policy
and index work. Fresh checks effective committed time, binds exact derived keys
and each present payload/result/error occurrence, then calls the existing
indexed reducer with first ordinal0 for this single batch frame. Two stable
reference/item buffers avoid per-item value vectors. The two existing indexed
signature lifetime splits keep returned predecessor references tied solely to
the authoritative view. Scratch, decoded operation, keys and isolation drop
before namespace/request owners move into retention.

The private retained response version1 is acceptance `[0]` or a business-rejection
code exhaustive for this selected wire profile, with scalar payload of at most19 bytes. Explicit matches
preserve every supported Domain/Admission/Batch/Index code; Rust enum layouts are
not serialized. Typed status and immutable original time/position/expiry remain
bound to the response. Operational decode, resource, authority, allocation and
expiry errors are never retained as business outcomes. Replay borrows its exact
original owner and bypasses current guards and new TTL configuration.

Fresh stores its retained metadata in `Box<RetainedOutcome>` after the late
workspace check and retained-value validation. This avoids the large inline
enum variant while moving response/namespace ownership. The fixed512 metadata
term covers the object and pointer. It adds one fixed allocation and indirection;
it supplies no measured native gain. Stable `Box::new` follows existing fixed
metadata precedents; [fallible Box allocation remains nightly](https://doc.rust-lang.org/std/boxed/struct.Box.html#method.try_new).
Variable Vec builders remain fallible. Host allocation failure for this fixed
box is fail-stop before durable publication, without a recoverable-OOM promise.

For frame bytes E, immutable decision cap B and verified item count n, decoder
ownership admits D=4E+B+1024n+2048; Fresh admits D+192n+256+B before either new
buffer reservation. After transients drop, retention admits E+B+H+531, where H
is moved namespace/request payload. Producer output admits the owned-input sum
plus1024n+512 and measured frame bytes. Arithmetic is checked. These are
operational logical coexistence bounds, separate from replicated business
admission and native allocator capacity. The corrected fixed-wire proof
E<=V+335+80n gives a default Fresh bound50,847,292 for core-admitted V under a
64MiB workspace allowance. It makes no universal all-input or native RSS claim.

The source write-contract suite observes scalar bulk acknowledgement and
separate record fetches. Original sorted bad-first rollback and supplemental
reversed late refusal remain distinct. Public raw/schema conversion, independent
bulk modes, physical value references, queries, original setup/profiles, atomic
durability and complete Linux/macOS acceptance remain required successors.

## Mutation batch preparation and retained inspection phases

ARCH-M2-RECORD-BATCH-PHASES1-CREATE1 factors the existing raw consumer into
`prepare_record_batch`, `inspect_record_batch` and `decide_fresh_record_batch`.
The sole decoder captures immutable CodecLimits and the actual V5 derivation
Result. Generic decoding still succeeds when only that private derivation is
unsupported; phase inspection preserves the original mathematical error.
Preparation admits D before materialization; inspection checks actual profile,
D and intent before the single existing retained gate. Complete drops decoded
owners and returns the inline plan/original borrow. Fresh alone boxes the same
owner and retains no query/outcome-map lifetime. Consuming Fresh releases the
box before indexed work and command/key scratch before the unchanged late bound.
The old raw API preserves its signature and intent/group/gate error order.

D=4E+B+1024n+2048, Fresh=D+192n+256+B and late=E+B+header_bytes+531 remain
unchanged. The existing full GateView and complete IndexedGroupView still apply;
no requested-only map, projection or candidate window becomes authority. Fixed
Box allocation uses existing allocator semantics. These logical per-call bounds
do not sum previously held returned owners or establish native latency/RSS.
Five new tests pass after real missing-API RED; actual source reviews preserve
the unchanged reducer, encoder and wire grammar. Physical loading, atomic
publication, point-cohort proof and independent Linux execution remain open.

## Point-only retained inspection

ARCH-M2-POINT-RETAINED-GATE1-CREATE1 adds inspect_record_batch_point using the
same decoded profile/D/actual intent checks and the same Complete/Fresh mapping.
It invokes one shared point gate without building an outcome map or loading
current records. PointGateView separates the expected-query and retained owner
lifetimes. Its requested outcome and exact tenant/group/recovery nonempty
observation must come from the same transaction revision; neither field certifies
that provenance. Some outcome implies nonempty; nonempty with unapplied control
refuses before exact log replay. Existing full-map/charged callers retain their
check/debit/lookup order and normal retained classification algorithm.

The physical existence bound is the existing FWPK1 family2 prefix over group,
recovery and tenant, without a request component. It reuses existing escaping,
checked measurement and one admitted output reserve; min16/binary21/all-NUL
maximum17421 remain separate from the prepared isolation cap. Transaction
retained_nonempty is required and accounts for open plus first candidate/end,
with no copied outcome. These inputs enable point-retained inspection;
coherent touched-index/policy loading and the real atomic adapter still follow.

## Selected touched-record indexed input

ARCH-M2-TOUCHED-INDEXED-INPUT1-CREATE1 selects an ordinary borrowed
TouchedIndexedGroupView and consuming decide_fresh_record_batch_touched entry.
Its query maps distinguish an unobserved key from observed None/false. Reached
unobserved authority is an operational failure; unused facts are not eagerly
validated. One canonical ClaimRecord supplies the core record and ordering.
Only current-record borrows survive the result; query keys, map descriptors,
placement and policy references may be released. The view certifies no physical
read or transaction provenance.

Closed private Full/Touched branches share the existing batch and index
algorithms. Full-map defaults and dual-map consistency checks retain their old
behavior. Touched retry observations override item retry hints at the existing
reached retry-policy admission/build point, including observed None for source
defaults. Parallel predecessors require exact due membership and an observed
empty FIFO prefix. Ordinary FIFO mutations require their exact member without
requiring a head lookup; named Claim/Reclaim still require the true head.
Unscheduled followers retain canonical record/order and no-parallel checks.

The consuming Fresh entry unboxes the same decoded owner, with no new decode,
hash, retained gate or projected map. Authority construction adds no core-record
clone. It retains the two
fallibly reserved value/item buffers. For caller-live logical bytes L, touched
admission checked-adds L to the old Fresh bound before either reserve and to the
old late bound after transient owners drop. Full callers pass zero and retain
their existing error order. L is a caller-supplied mathematical input, with no
refund or implicit credit for fact maps. The real driver must separately admit
D+R+S during loading and include all remaining read owners, staging and copied
bounds in L. These formulas establish neither native allocation sizes nor RSS.

Implementation and executed acceptance are tracked in the continuation and
review records. Real coherent loading, atomic publication/recovery, original
assertion/setup/profile coverage and native Linux/macOS acceptance remain
required.


## Selected joined record-batch transaction

ARCH-M2-RECORD-BATCH-TRANSACTION1-CREATE1 selects synchronous neutral
orchestration under wf-storage::transaction. Implementation and executed
acceptance remain tracked in the continuation and review records. The driver
uses an externally stable expected scope, prepares once, then holds one
exclusive group/recovery guard through control, exact retained cohort/outcome,
Fresh placement/current/policy/index observations and consumed commit. Complete
performs no Fresh reads. Reply values own their complete retained metadata and
bytes and escape only after successful commit; no local loaded-record borrow
escapes. Uncertain commit returns an operational failure and requires fencing
and durable reopen/reconciliation.

Actual pre-gate I/O can fail before an otherwise exact LogReplay. Bounded eager
Fresh prefetch can likewise fail on an unused later corrupt policy before an
earlier pure business rejection. These are explicit physical-driver operational
precedence rules; existing pure gates/reducers retain their old ordering. A
healthy empty-type Create performs its actual empty policy-key lookup and reaches
its existing deterministic EmptyValue rejection. No fabricated absence, second
gate or copied business reducer is selected.

The pure domain projection preserves the complete policy owner unchanged. Its
mode view carries the same generation and every state mode; its retry view
carries all type/state overrides. These temporary views are never persisted as a
replacement complete policy. Installed storage values use exact normalized
FWPV2; original public ETF policy installation/read parity remains separate.
FWPK1 family6 encodes group/recovery/tenant/type policy points. Family7 encodes
recovery/partition placement with the raw nonempty GroupId value, independent of
tenant and group. Query types admit empty and greater-than-65535 byte components
under the positive encoded bound. Stored policy content still requires its full
existing validation and exact expected type.

The borrowed record encoder accepts existing scope, partition, core and ordering
values directly. It shares the legacy encoder's single measured grammar without
cloning the scope or core through a StoredWorkflowRecord container. The borrowed
record scanner exposes its already validated version for consumed commit checks.
Neither API changes bytes or creates a custody token. The isolated redb backend
uses one table/real write transaction and full record/index/history/outcome/control
oracles; it introduces no product dependency or engine-adoption claim. Native
Linux, runtime consensus wiring and full original scenario/profile parity remain
required.

## Independent V6 private batch profile

ARCH-M2-INDEPENDENT-V6-PUBLICATION1-CREATE1 adds intent6 to the existing FWCP1
nonempty batch root1. Its eight caller-operation field streams use separator
WF-RECORD-BATCH\0V6\0; V5 bytes/digests remain exact. Generic decoding caches the
actual derived Result without newly propagating it. Public preparation,
inspection and Fresh entry points reject cross-use of verified5/6 profiles.
Claim/Reclaim remain outside this mutation profile; public empty-batch handling
remains a required pre-submission behavior.

Response version remains1. V6 evaluated responses are root2, a positive u32
count<=100000 and exactly that many scalar rows: success0 or a closed existing
domain/priority error. Whole-call refusal retains root1 and its existing table.
V5 success remains scalar root0. Per-item admission/duplicate/cross-group/claim
head errors are forbidden; incomplete/trailing bytes reject. Parsing and exact
plan comparison stream rows without a per-row owner. Changes validates status,
count and every row against the actual independent decision.

Only after the one retained gate returns Replay does the pure wrapper check
grammar/status agreement and compare Some(count) with incoming item count.
Whole-call rejection has None and remains replayable. Exact-log replay, conflict
and expiry bypass that response metadata check. Physical outcome scan retains
its explicit pre-gate fail-closed precedence. Its borrowed descriptor adds only
the parsed optional count; configured transaction admission remains separate.

The same consumed Changes/commit path publishes original-once/final-once/all
histories with outcome/control. Retained replies move their original owned
buffers only after commit. This private profile and the isolated native proof
do not implement public normalization/submission hooks, SDK/wire compatibility,
production dependency adoption or full original setup/runtime/Linux acceptance.


## Selected charged policy point prerequisite

ARCH-NEXT-M1M2-POLICY-POINT1-CREATE1 and its exact test-geometry precision select
one private production gate/point.rs child and the existing parent reexports.
inspect_complete_core_policy_patch_point_gate takes the ordinary same-revision
PointGateView, committed header, actual profiles and one continuous schema charge.
It checks profile equality before any debit, then shares actual header, coherent
cohort, control and retained classification with the old point path. No new
nominal model, alias, trait, dependency, command/policy/map owner or authority
certificate is introduced. The independently elided query lifetime does not
enter the returned retained borrow.

Only non-LogReplay reaches the conservative logical request observation tariff
128+actual request bytes, before Some/None classification. It claims no physical
map lookup or copied request owner. For tenant/group/recovery/request lengths
6/5/8/7, gate-only cumulative prefixes are128,294,422,557,723,865,993,1189,1317:
Fresh557, exact LogReplay422, conflict1189, Replay/Expired1317. Prior g and p stay
on the same account, failed debit preserves the last successful prefix, and no
scratch is newly requested. None retains the existing uncharged helper bypass
before checked fee arithmetic; all old order/results/fees remain exact.

The original386-line integration root moved byte-for-byte to request_gate/main.rs
plus one private registration; charged.rs contains exactly two additional
identities and four private assertion helpers. Actual reciprocal review covers
accepted/rejected original owners, pointer/metadata/lifetime, corrupt cohort and
retained facts, exact LogReplay bypass, unapplied/applied-zero, each exact/tight
event, profile mismatch and real prior-peak/repeated-call admission. Tests were
frozen in04c7f4d after an actual missing-function RED. The first strict diagnostic
was only an elidable lifetime name; its equivalent spelling has an executed
whole-file inverse without a waiver or changed oracle.

All875 workspace tests/90 targets and six compile-fail examples/15 targets pass
on macOS, alongside strict checks, frozen reference/progress and separate pinned
native compatibility. Actual source reviews include complete parent and style
inverses. Full durable policy publication still requires real closed Fresh
staging, current/context/migration authority, canonical discovery/reindexing,
atomic policy/outcome/control publication, explicit origin/current layout and
snapshot propagation. Those future artifacts are not selected by this gate.
Public policy/Claim/governance, accepted runtime, every original setup/profile,
Linux and full M1–M8 acceptance remain required; no CPU/RSS/speed claim follows.

## Pure workflow value references and binary storage bytes

ARCH-REF-BINARY1 selects pure borrowed reference parsing, checked canonical
reference construction and borrowed logical-binary storage parts under
wf-ports::blob. Its private children inherit the existing parsed-source purity
rules. Standard-library filesystem, clock and thread effects remain forbidden;
the client facade and worker still cannot expose or import this server module.

Explicit partition tags use SHA-256 through the existing exact sha2 dependency
without default features. This is deterministic source-compatible byte
construction, not storage access or a new dependency edge. Automatic partition
selection preserves the source CRC32 convention. Borrowed views and constructed
keys grant no placement, transaction, authorization, admission or durability
authority. Actual publication, routed reads and history hydration remain later
contracts; no domain, wire or client boundary changes here.


## Value integration port purity classification prerequisite

VALUE-CONSUMED-BINARY1-M0-PURITY1 extends the existing recursive pure-source
classification to exactly wf-ports::consensus::value,log::value_batch and clock.
They hold deterministic preparation data/codec work and clock PORT contracts;
real clock observation belongs to a separately selected effectful server adapter.
Filesystem, standard-library time and thread effects remain forbidden here.
The existing exact hash dependency/Vec positives and client/worker boundaries
remain; no dependency, visibility, unsafe, suppression or size exemption is added.

The existing value-byte architecture test loop first produces an actual compiled
missing-diagnostic assertion failure under old classification, saved in627eecc.
The fix adds only those three recursive prefixes, with all old module controls
retained. This technical M0 evidence does not establish value publication,
clock authority, business TDD, an encoding/layout version or workflow parity.
The connected value R0 still requires its exact public/registry/test creation
contract before any source/model/helper release. Linux and macOS evidence stay
separate; no runtime, allocation or performance result is inferred here.


## Consumed binary value command profile

VALUE-CONSUMED-BINARY1 keeps actual intent/Normal profile8 throughout preparation,
retained inspection and publication. Contained producer modes1/5/6/7 reuse their
existing headerless operation grammar; they are not nested old frames or
relabeled old intents. Their retained response versions are8/5/6/7 respectively.
The complete prepared digest binds server context separately from caller intent.

The bounded codec retains raw typed priorities and unresolved extension presence
until guarded interpretation; public mode1 keeps its existing normalization.
One decoded operation and metadata owner supplies actual codec work to the same
continuous value account used by the real driver. The independent planner's
actual ordering vector is moved into the new path with its header/slots admitted;
legacy wrappers discard it before their unchanged return shapes.

A real retained point gate precedes Fresh-only target, capture-marker, policy and
default checks. Reprepare therefore preserves the original outcome even after
server context changes. Fresh creation uses one external schema account through
policy loading, projection, witness verification and reached birth capture.
Unsupported profiles remain explicit operational refusals rather than fabricated
business outcomes. Canonical footer/intent/metadata/EOF validation remains
mandatory in direct storage application and explicitly enabled Node operation-8
startup audit/replay. Historical audit uses the bounded complete preparer without
recapturing current policy or defaults; replay invokes the original consumed driver.

The exact selected wire and owner decisions are recorded in the
[architecture review](../reviews/history-values/architecture.md); durable rows
and snapshot semantics are in
[ADR-007](007-durable-log-storage.md#binary-workflow-values-and-captured-lifecycle).
This private profile does not establish transport/public hydration capability,
full source/setup parity or accepted runtime readiness.


## Selected pure history-value overlay

HISTORY-VALUE-OVERLAY1 selects two callback-free domain operations: prepare stable
references from borrowed original entries, then consume a captured read batch
into ordered writes over the same original base. Generic original sources may
retain arbitrary typed fields and event identities; the transformer neither
clones nor reconstructs them. Decoded writes own complete original-value
documents and retain no borrow into dropped read buffers or compatibility tables.
Collection preserves event and payload/result/error order and the first equal
binary backing, without requiring a generated storage-key grammar.

Disabled or genuinely empty history skips reads and threshold computation.
Every enabled nonempty history computes a checked positive threshold, even when
no references can be fetched: signed maxima below1 yield1, otherwise max+1 must
fit usize. An empty fetch list requires batch=None but still processes oversized
reference errors. Captured direct lists preserve source zip truncation; mixed
eligibility masks with a mismatched reply length restore missing slots. Actual
backend adapters remain responsible for their ordered cardinality and failures.

Missing writes nil then its missing flag; stored-size omission writes its omitted
flag then the exact complete stored size. Success writes the full decoded value
then stored size. Storage failure and invalid stored-value corruption write only
their static sanitized errors; unrelated result classes write nothing. No branch
implicitly clears stale fields. Only InvalidFlowValue becomes a field error;
context/profile, resource and reservation failures remain operational errors.
Stored-size selection precedes per-occurrence copying and compatibility checks.
Each decode attempt, including repeated fields, gets an independent fallibly
reserved input owner.

The plan retains one checked work account and one conservative CUMULATIVE
requested-owner liability across preparation and application. Six positive
outer-limit checks form a bounded bootstrap before the account exists. After
that bootstrap, incoming read capacities, reference/result/write spines, copies,
relocation, fixed metadata, each reached event and both equality operands are
prospectively admitted. Every
decode attempt independently prepays its entire decoder work cap W on BOTH work
and requested backing liabilities, without refund after invalid-data continuation.
The old public decoder, private workers and meter remain unchanged.

Private fixed-byte helpers expose conservative checked decoder metadata H using
the actual target types. Caller coverage additionally names the helpers'14/6
size/count arrays and IntoIter phases, scalar/result metadata and bounded
computation work before calling them. These allowances are distinct from W and
the input/outer owners. They do not certify allocator capacity, native stack,
RSS, recoverable OOM or a measured speed improvement. Actual stopped-source
event/coexistence review and complete test execution remain acceptance gates.

This pure capture/delta contract adds no persistent representation, fetch,
physical fallback, accepted service or read barrier. Actual size-aware reads,
separately versioned typed/common publication preserving binary8 bytes,
shared/named/external availability, ordered history acquisition and public
hydration/routing remain required. The coordination record binds the selected
files/models and genuine saved RED before BODY; the handoff records current
execution status. The original source mappings, native/Linux/macOS evidence and
ordered milestone exits remain separate and unfinished.

## Stored-payload Step identity

Intent 11 selects the same singleton Step operation 12 and complete fields as
intent 10, with stored payload bytes instead of logical binary. Its caller hash
uses `WF-STEP-CONTINUE\0V11\0`; request identity, leader stamp and item time keep
the established caller/prepared identity separation. The same borrowed wire
scan validates all fields, derived keys and EOF before ownership. Independent
literal vectors verify both the full frame and derived digests. Old Step and
generic preparation do not select this profile.

The explicit Step response 9, retained inspection, Fresh touched decision and
provenance validation accept the two distinguished Step intents. Stored-byte
preparation supplies no successful typed decoding or public submission promise;
the existing typed decoder retains invalid/context/limit distinctions. There
is no new payload owner, operation, event, dependency or codec wrapper. This
selection enables the coupled durable/recovery batch in ADR-007; it is not a
claim of complete application or original runtime acceptance.

The GC4 stored mutation successor retains intent9 and the existing contained
response5/6/8 grammars. Common rejection kind8/code0 denotes whole-command release
backlog only for those associations. It is not an independent row error, and
older intent1/5/6/8 outcomes cannot acquire it. AtomicStart kind7 remains unchanged.
No new mutation opcode or successful reply format is allocated. Actual write,
point reads and canonical recovery require GC4 for this new receipt, including
receipt-only cohorts. Private retained metadata moves without cloning old response
bytes when the selected driver substitutes that deterministic rejection. This
codec value grants no queue-full or durable-commit authority by construction.

## Selected atomic Claim parent framing

PUBLIC-CLAIM-APP1 adds original-first application orchestration over an explicit
authorized inquiry/preparation/supervised-submission capability. Canonical public
V1 is selected as internal intent21 before lookup; no current policy, clock,
nonce or independent credit call precedes that lookup. An unresolved submission
owns the original scope/request and one shared immutable frame. Retry preserves
that frame, re-admits current profiles, inquires first and submits the same owner
only after an absent observation; absence is no external no-spend proof.
Denial, redirect, invalid reply and unresolved inquiry preserve the original
context. Current production authorization/read-barrier/supervision implementations
remain separate gates; the port interface alone does not implement them.

The complete returned receipt is bound to the original caller's worker, type,
lease duration, partition/state/priority filters, exact scheduled phases/quotas,
count and explicit/derived credit scope/shard. Source payment IDs encode one
canonical epoch/batch and original ordered1-based ordinals. A concurrent earlier
original may use another nonce and earlier time; do not compare it with this
attempt's generated inputs or the caller's identity-only requested now. This
works without an old log after retained snapshot custody. Native integration
tests execute actual log/state funding, refund, pending/lost delivery, original
lookup and historical recovery; they establish no production Raft/service.

PUBLIC-CLAIM-CALL1 additionally selects **op19/intent21/response18** for the
public funded parent. Intent21 stores the exact existing public V1 canonical
caller digest; the internal selector is not a new public normalization version.
The incremental encoder matches all12 preserved V1 vectors and excludes only
the already excluded block attempt. Trusted time and the original16-byte credit
nonce belong to the prepared body/digest, never caller comparison. Whole frame,
EOF, canonical caller and digest checks precede current state. Older preparers
continue to reject op19.

The FWFR1 response contains either the complete actual FWAR1 Claim result, or
a typed known credit rejection with separate FundingRejected/NoDemand branches.
Only actual physical-presence exhaustion after known failed funding permits
NoDemand. Data encoders provide no spend, emptiness or durability authority.
The real GC5 driver stages current-policy funding, all Claim phases and unused
original-ID release under one original retained gate and writer. It publishes
one public receipt/control. Cross-group funding, public ingress/authorization,
blocking/projection/pipeline completeness and source/profile parity remain open.

ATOMIC-CLAIM-PARENT1 reserves op18/intent20/response17 for the actual atomic
ordinary Claim parent required by ADR-004 A-CM1. Its immutable frame contains all
normalized original caller fields except block-attempt duration, plus separate
trusted time, operational token/scan/head inputs and optional original paid IDs.
Intent20 binds tenant and canonical original caller content; generated IDs/epoch,
time and operational budgets affect the prepared digest without changing caller
intent. The decoder normalizes and stream-verifies the complete actual caller
bytes before retaining reference spines; no supplied digest certifies arbitrary
unparsed caller content. Existing leaf and generic codecs stay closed to op18.

The real driver must inspect retained replay once and stage every phase under
one actual transaction, then publish one original response/control. Complete
current governance, original-ID monotonicity, explicit route authority and full
receipt validation remain required; mathematical codec/Fresh values prove no
durability or external ownership. Fresh unresolved Auto/Global scope refuses
until its real source route is implemented. Original optional partition capture,
pipeline identity, fault/recovery and both-host acceptance remain open. This
selection allocates a contract; implementation evidence must be recorded apart.

The implemented selected native parent now keeps one actual Fresh gate and one
writer across the original schedule. Exact borrowed nested response bodies
preserve accepted order, original time, one worker/type/deadline and unique
partition/record identities across phases. Original payment is retained whole;
used positions are monotone, may be non-prefix in the grammar and are checked
against the original payment. A rejected receipt contains no accepted prefix.
Only the actual writer proves rollback and publication; response grammar alone
provides neither. Current native canonical binding consumes one original cursor
across all accepted phases; source bind-before-skip capture is still required.

Policy-derived native payment uses the complete current stored policy under
that writer, exact source scope/shard and actual current credit configuration,
fingerprint, lease generation and original active IDs. Explicit pairs skip unused
governance. One PolicySchemaCharge spans initialization, phases and replacement
rejection guards. No scalar projection, client confirmation flag or reconstructed
leaf Fresh value supplies this authority. Public funding/NoDemand/Unknown,
compensation, original optional partition capture and pipeline identities remain
separate required integration work; this native entrypoint is not that service.
