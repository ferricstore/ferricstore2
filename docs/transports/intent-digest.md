# Version 1 normalized mutation intent

Status: selected comparison grammar after three-agent review. The bounded pure application encoder and policy-set/claim-due entries pass twenty scoped tests against eighteen generic, seven policy-set and twelve claim-due independent byte/hash vectors under [ADR-005](../adr/005-application-intent-hash-helper.md), with independent code/resource acceptance for those represented mappings. Full operation/client/transport gates remain. This is an internal comparison grammar for retained request identity, not another network or persistence format. HTTP and TCP use the [same request contract](../transport-design.md).

## Authority and input

The application owns a pure, bounded `intent` module. It constructs a typed client intent from validated command values and authorized tenant scope, before leader preparation injects time, randomness or generated IDs. V1 requests do not accept a client-supplied digest. Adapters cannot establish authoritative identity by supplying one. The application recomputes it, compares version and digest with the retained request record, and rejects reuse with different intent. The wire crate carries version/digest values and DTO normalization rules; it does not access the outcome store. SDKs need not compute a digest to retry a request correctly.

Version 1 uses SHA-256 over the exact prefix bytes below followed by one canonical typed value. The implementation uses checksum-pinned RustCrypto `sha2` 0.11.0 through its incremental safe API ([primary documentation](https://docs.rs/sha2/0.11.0/sha2/)); its selected features and transitive unsafe review are recorded in ADR-005. A digest is neither an authorization credential nor proof that an outcome was committed; authorization and durable scope checks still apply.

The root is a map with exactly `normalization_version` = string `"1"`, `schema_version` = string `"1"`, `tenant_id` = opaque bytes, `operation` = string, and typed `args`. Each operation's registry defines its complete argument mapping, types, optionality and semantic defaults. Do not derive this mapping automatically from Rust struct names or serialize a generic domain object. A registry version change that affects meaning requires a new normalization version and retained-version retry support.

The first implemented entry is `workflow.policy_set`, with its exact typed
InstallCommand mapping in ADR-005. It preserves business-invalid caller values
for comparison; source admission runs separately. In particular, mode Clear
hashes as present null while source-compatible state-policy admission rejects
that null. Replace+Remove also retains its supplied identity while source
admission rejects it; Patch deletion remains valid. This entry does not establish raw HTTP/TCP normalization or permit
dropping unsupported governance/indexed fields from a supplied command.

A-N46 selects the implemented internal comparison entry, `workflow.claim_due`,
with the exact mapping in ADR-005. A-N49 architecture and performance acceptance
each passed8 caller,20 application and1 workspace control on unchanged inputs. Its
eleven mandatory args are workflow_type/worker Bytes, states{any:Bool,
names:List<Bytes>}, partitions{mode:String,keys:List<Bytes>}, lease_ms/limit/
reclaim_ratio Unsigned, reclaim_expired Bool, reply_mode String, payload Bool
and values{mode:String,names:List<Bytes>}. Optional priority/payload_max_bytes/
governance_shard_id/now_ms are Unsigned; governance_limit_scope is Bytes.
Absence remains omitted and present zero distinct. Partition modes are
auto/any/global/only, values modes omitted/all/only, and replies records/jobs/
jobs_compact/jobs_compact_attrs/jobs_compact_state/jobs_compact_state_attrs.

Fixed claim defaults are lease30000, limit1, reclaim=true, ratio25, absent
priority and records reply. Payload override wins over full, whose default is
reply==records. State names keep last occurrence/order; partition/value names
keep first. Scalar ASCII partition aliases normalize; list aliases remain
literal bytes. Mixed ANY+names and empty/zero/MAX business-invalid inputs remain
comparable; source key/footprint/scope/admission runs separately. Both Any and
Global require the source shared-tenant rejection, never authority from a hash.

Optional source requested_now_ms is caller now_ms identity only and never
committed-clock authority; future wire permission remains a review gate.
Policy-dependent payload cap absence differs from every explicit cap. Block
wait and all server stamp/log/budget/token/selection preparation remain excluded.
No current policy, clock or generated routing/selection result changes identity.
The borrowed normalizer and bounded flat encoder confer no public parent,
transport, source-profile or durable execution guarantee.

Include workflow identity, exact semantic partition values, explicit expected revisions/fences/lease guards, caller-supplied schedule times, policy/definition versions, opaque payload/result bytes and meaningful item order. For an atomic-group batch, the root covers the entire ordered set of item operations/arguments. An independent batch hashes each item separately; it has no durable aggregate identity.

Exclude `request_id` itself (it belongs to the lookup key), TCP call/lane IDs, HTTP headers/routes, credentials, traces, attempt timeout/wait, transport receive limits and advisory routing hints. Required mutation request_scope binds the original group/recovery namespace in the retained identity key, not a mutable routing hint in the digest. Clients discover it before first submission and retain it with the same request ID across retries; the application checks again immediately before handoff after waits/forwarding and rejects incompatible placement/recovery rather than silently entering the latest namespace. Already-submitted supervision remains in its original namespace. Independent items bind their own scopes; an atomic-group batch binds one whole-group scope. Semantic partition placement remains in the arguments. Include explicit state preconditions in arguments; a transport freshness/read-watermark request is not a replacement for them. Server-prepared timestamps, IDs and jitter are committed inputs separate from client intent.

## Canonical typed encoding

The 13-byte prefix is ASCII `WF-INTENT`, NUL, ASCII `V1`, NUL:

```text
57 46 2d 49 4e 54 45 4e 54 00 56 31 00
```

Each value is `tag:u8 || payload_length:u64_be || payload`. Length is the exact following payload byte count, not a character count. All lengths/counts use checked conversion/arithmetic; excessive depth, items or bytes reject before submission. Encoders can stream into the hash after a bounded length pass instead of assembling an additional whole-request buffer.

| Tag | Type | Payload |
|---|---|---|
| `00` | Null | Empty, length 0 |
| `01` | False | Empty, length 0 |
| `02` | True | Empty, length 0 |
| `03` | Unsigned integer | Exactly 8 bytes, unsigned big-endian |
| `04` | Signed integer | Exactly 8 bytes, two's-complement big-endian |
| `05` | String | Validated UTF-8 bytes, unchanged |
| `06` | Opaque bytes | Original decoded bytes, unchanged |
| `07` | List | Item count `u64_be`, then each encoded value in caller order |
| `08` | Map | Entry count `u64_be`, then encoded string key and encoded value for each entry |
| `09` | Finite binary64 | Exactly 8 IEEE-754 bits in big-endian order; preserve the sign of zero |

Map keys are unique validated strings, sorted lexicographically by their UTF-8 bytes, with a shorter equal prefix first. No locale sort, Unicode normalization, case folding or randomized-map iteration enters the encoding. Empty strings/bytes/lists/maps have distinct tags. Absent optional values are omitted map entries; explicit null is a present null value unless that operation explicitly forbids null. This distinction preserves zero versus absent and omitted versus empty data.

Numeric categories come from the typed operation schema, never a guess from JSON spelling or a JavaScript Number. Decimal-string u64 fields decode to tag `03`; a business string containing digits remains tag `05`. Binary64 is available only where the operation schema explicitly defines that type and its lossless wire representation. Reject NaN/infinity and lossy/out-of-range conversions. Arbitrary JSON-number, scalar-partition and legacy float/string semantics must be settled against the preserved source cases before publishing those fields; this grammar does not authorize changing their equality rules. Decimal or wider numeric support, if required by parity, needs an explicit new type/version decision rather than silent conversion to binary64.

Tagged base64 payloads decode before hashing. JSON key order/whitespace or equivalent allowed escape spelling cannot change typed values. Reject duplicate/unknown fields and prohibited noncanonical scalar encodings; do not repair invalid input while computing identity. Never serialize the raw request body and hash its bytes.

## Defaults and retained versions

Normalize only defaults declared equivalent by the versioned operation schema. Omitted versus explicit policy-dependent options remain different representations unless the schema establishes equivalence. Resolving the current tenant policy, current clock or current workflow state cannot alter the same retained client intent on retry. Preserve an explicit absence marker where meaning depends on committed policy resolution; the original prepared command/outcome owns that resolution.

Retain normalization version, digest and original outcome with the atomic applied checkpoint. Store enough versioned command/intent evidence to compare a retry using an older supported normalization version. A software upgrade cannot recompute retained identities using today's defaults and then report a conflict. Recovery/group mismatch and expired outcome guarantees are separate from different-intent conflict. The documented retention window must cover the supported retry/reconciliation duration; no duration has been selected here.

Expose digest as lowercase 64-character hexadecimal with `algorithm:"sha256"` and `normalization_version:"1"`. Compare decoded fixed-length values and versions. Security-sensitive tokens remain redacted and are not exposed through diagnostic intent dumps. This design uses SHA-256's collision resistance as an engineering assumption; it does not make digest equality a cryptographic authorization mechanism.

## Required vectors and gates

Before SDK mutations, produce independently reviewed byte and hash vectors across the Rust application and browser/worker clients. Required equivalent cases: reordered JSON keys, allowed whitespace/escape differences, HTTP versus TCP attempts, changed timeout/call IDs, identical decoded payloads and versioned equivalent defaults. Required different cases: changed tenant/operation/partition, payload bit, fence/revision, absent/null/zero, list order, policy version and requested schedule time. Maximum u64, non-ASCII identifiers, negative zero where supported, atomic/independent batch boundaries and invalid duplicate/ambiguous numbers need explicit fixtures.

Fault/replay cases verify that injected leader time/IDs never replace client-intent identity, concurrent same-ID attempts produce one retained outcome, current authorization applies to lookup/replay, old-version retries retain their original meaning after upgrade, and a retained client journal from before disaster restore cannot mutate a new recovery namespace through either transport. The scoped pure encoder checks above do not establish client/transport equivalence, retained-version service behavior or durable retention; those implementation gates remain.
