# Dedicated TCP workflow API

Status: proposed design, revised after actual HTTP/system cross-review and parent relay on 2026-10-09. No implementation, executable tests or measurements. The [shared transport contract](../transport-design.md) owns normalized operations, outcomes and dependencies; this file owns the TCP profile. Numeric defaults below are proposed policy, subject to the joint resource gate. Bounded current primary-documentation evidence and the actual exchanges are recorded below and in the [TCP review](../reviews/transport-tcp-review.md).

## Scope and ownership

`wf-api-tcp` is an outer adapter depending only on types, approved domain values, wire and application. Node gives HTTP and TCP the same application authorization/admission service. TCP cannot name raw ports, storage transactions or Raft handles, mint a second tenant quota, choose claim candidates, determine lease validity or return success before durable committed apply. `wf-wire` holds pure versioned DTOs, not socket types or persisted commands.

Use a dedicated client listener. Raft replication, membership administration, general KV/Redis, pubsub and legacy command execution do not exist on this listener. TCP is suitable for bounded persistent worker connections; it does not provide different workflow semantics from HTTP.

## Compatibility decision

Propose a new workflow-only protocol, `FWTP/1`, deliberately incompatible with legacy `FSNP` bytes, native opcodes and native SDK negotiation. Use a different magic and listener; never sniff and fall back to FSNP/RESP/HTTP. There is no legacy native SDK binary-compatibility claim. The existing workflow semantics and [source-test acceptance obligation](../workflow-test-parity.md) remain mandatory. Native parser/framing cases receive explicit public-byte exclusions only where the parity decision selects the new protocol; preserve their workflow authorization, size, ordering and fault guarantees through corresponding new adapter cases. Mixed native suites are reviewed case by case.

The pinned legacy native document describes 24-byte FSNP framing, lane-local ordering, chunk/compression flags and a connection-local request number. It is evidence for requirements, not the proposed format. New `call_id` replaces that correlation number; new durable `request_id` has a separate application contract. Preserve legacy lease/fence retry tests without manufacturing reused request IDs: a distinct compatibility/application surface must exercise their original no-ID inputs and history/guard assertions. New public mutations require explicit request IDs.

Source evidence: `ferricstore/docs/native-protocol.md` at `66586c596e271408d9184af32eb31cd7218c126e`, read using the pinned git object; preserved source tests under `tests/reference/elixir-baseline/` are immutable. Bounded inspection of native [codec cases](../../tests/reference/elixir-baseline/apps/ferricstore_server/test/ferricstore_server/native/codec_test.exs), [workflow partition authorization](../../tests/reference/elixir-baseline/apps/ferricstore_server/test/ferricstore_server/native/commands_test.exs) and [query result negotiation/corruption](../../tests/reference/elixir-baseline/apps/ferricstore_server/test/ferricstore_server/native/flow_query_result_codec_test.exs) found requirements for correlated replies, claim state/attributes/tokens, effective per-item partition permission and fail-closed malformed prepared replies. Preserve these guarantees in FWTP cases even when exact FSNP tag bytes are explicitly out of compatibility scope. No source test ran and no exclusion was marked passing.

## One concrete initial frame and encoding

All fixed-width integers use unsigned big-endian encoding. Header size is exactly 24 bytes. Version 1 has no alternate header size, checksum, compression, chunking, fire-and-forget flag or generic command opcode. TLS protects transit integrity; JSON schema validation does not establish durable correctness.

| Offset | Width | Field | Version 1 rule |
|---:|---:|---|---|
| 0 | 4 | magic | ASCII `FWTP` (`46 57 54 50` hex) |
| 4 | 1 | frame_version | `1` |
| 5 | 1 | kind | table below |
| 6 | 2 | flags | zero; any set bit rejects |
| 8 | 4 | lane_id | control `0`; data lanes `1..16` under the proposed profile |
| 12 | 8 | call_id | client calls nonzero, strictly increasing per connection; never durable identity |
| 20 | 4 | body_len | UTF-8 JSON body byte count, excluding the header |
| 24 | body_len | body | exactly one JSON object; no trailing bytes or extra JSON value |

| kind | Name | Direction / lane |
|---:|---|---|
| `0x01` | HELLO | client to server, lane 0, first frame |
| `0x02` | HELLO_ACK | server to client, lane 0, HELLO call_id |
| `0x03` | REQUEST | client to server, positive lane |
| `0x04` | RESPONSE | server to client, matching REQUEST lane/call_id |
| `0x05` | CANCEL | client to server, lane 0, fresh call_id; body identifies target call |
| `0x06` | CANCEL_ACK | server to client, lane 0, CANCEL call_id |
| `0x07` | GOAWAY | server to client, lane 0, call_id 0 |
| `0x08` | PING | client to server, lane 0, fresh call_id |
| `0x09` | PONG | server to client, lane 0, PING call_id |
| `0x0a` | PROTOCOL_ERROR | server to client, lane 0; bounded error, then close |

`PROTOCOL_ERROR` echoes a safely decoded offending call_id if possible, otherwise uses 0. No other unsolicited frame is defined. Unknown kind/version/flag, illegal direction/lane, reused or decreasing call_id, and oversized header length terminate the connection before dispatch. No resynchronization scan through attacker bytes. Call numbers do not wrap; open a new connection before exhausting u64. Responses across lanes may arrive out of call-number order.

JSON is the only initial body encoding, selected to reuse the pure DTO contract and HTTP fixtures. This is an interoperability/simplicity policy, not a claim of optimal wire size or throughput. Request schema version is independent of frame version; handshake fixes both. Unknown fields, duplicate fields, nonfinite numbers and invalid UTF-8 reject. No unbounded generic JSON tree is accepted. Opaque bytes use the common explicit base64 tag, such as `{"encoding":"base64","data":"AAH/"}`; accept the canonical standard alphabet with required padding, zero pad bits and no whitespace. Public u64 revisions, fences, positions and counts use decimal strings with grammar `0|[1-9][0-9]*`, range-checked to u64; header call/lane/length are binary integers. Meaningful absent/null/zero distinctions and any source float semantics remain schema-specific parity gates.

An illustrative exact frame vector has lane 2, call 17 and this 177-byte ASCII body (no newline):

```json
{"schema_version":"1","operation":"workflow.get","tenant_id":"t1","args":{"workflow_id":"w1","partition_key":"p1"},"attempt":{"timeout_ms":5000},"consistency":{"mode":"strong"}}
```

Its 24-byte header, then the exact body above, is:

```text
46 57 54 50 01 03 00 00 00 00 00 02 00 00 00 00 00 00 00 11 00 00 00 b1
```

This vector documents encoding/length only; it is not an executed round-trip test. Whitespace changes JSON byte length but not normalized semantic intent.

## TLS, handshake and authorization

Proposed production profile: TLS 1.3, required ALPN `wf-tcp/1`, verified server name and configured trust roots, optional authenticated client certificates. Disable TLS early data for all workflow calls. Isolate ticket/session-resumption stores whenever listener audience, server identity, client verifier or authentication policy differs; a public client session cannot resume as a Raft member or privileged administrator. There is no STARTTLS or plaintext credential fallback. A separate explicitly configured loopback development listener can use plaintext; clients cannot downgrade from a failed TLS connection to it. Neither client routing hints nor a client-provided principal become authenticated identity.

States are `accepted -> TLS -> awaiting HELLO -> authenticated ready -> draining -> closed`. Handshake/header limits apply before authentication. HELLO offers only protocol 1 and `json`, declares client receive bounds and a bounded credential. HELLO_ACK selects one common schema/encoding and effective limits after shared authentication; rejected credentials close. Negotiated schema label `wf.v1` maps exactly to REQUEST/RESPONSE `schema_version: "1"`; there is no request `schema` alias. The base initial body is:

```json
{"protocol_versions":[1],"schemas":["wf.v1"],"encodings":["json"],"client_name":"worker-a","receive_max_body_bytes":1048576,"auth":{"method":"bearer","token":"<redacted>"}}
```

```json
{"protocol_version":1,"schema":"wf.v1","encoding":"json","session_id":"ephemeral-session","limits":{"max_body_bytes":1048576,"max_lanes":16,"max_active_calls":16,"max_pending_calls":24,"max_queued_calls":8,"max_parked_waiters":8,"default_attempt_timeout_ms":5000,"max_attempt_timeout_ms":30000},"capabilities":{"operations":["workflow.create","workflow.claim_due","workflow.complete","workflow.get","workflow.outcome"],"long_poll":false},"authorized_request_scopes":[{"tenant_id":"tenant-a","request_scope":{"group_id":"g1","recovery_id":"r1"}}],"auth_expires_at_ms":"1791561600000"}
```

Examples illustrate schemas, not a promise that the displayed operation subset exhausts the parity scope. Advertise only implemented operations, modes and consistency options; HTTP and TCP expose the same supported long-poll mode at a rollout, with explicit unsupported-mode rejection while it is disabled. HELLO_ACK values are effective ceilings, not entitlements to work or preallocated bytes. Bearer and verified-mTLS identity resolve through the common application auth service; the node's configuration selects allowed credential methods. No anonymous mutation profile. Public server discovery excludes privileged topology or tenant data before auth.

For the initial single-group placement profile, HELLO_ACK may return the bounded authorized tenant/group/recovery scope metadata shown above. It never enumerates unauthorized tenants or creates membership authority. A principal's scopes that cannot fit this bounded reply use an authenticated bounded discovery query before mutation; future multi-group placement resolution is a read capability, with authoritative validation still required at submission. No successful mutation reply is the sole source of its namespace: the worker obtains and journals scope before first send.

Check expiry/revocation/tenant permission at each operation, including retained outcome lookup and retries, not only HELLO. Recheck after queue/poll waits immediately before submission; parked credentials confer no continuing permission. Credential refresh in place is initially unsupported: reconnect with a fresh credential; drain old work normally. Do not reassign a session's principal mid-flight. Token, lease secret, payload and private route details never appear in errors or logs. Exact credential format, trust roots, authorization-cache freshness and deployment revocation SLA are shared auth gates, not properties granted by rustls.

## Message schema and identity

A REQUEST body is one common normalized operation envelope. Endpoint/op arguments and errors come from the same `wf-wire` registry as HTTP. Examples below are proposal vectors to freeze jointly, not executable fixtures:

```json
{"schema_version":"1","operation":"workflow.create","tenant_id":"tenant-a","request_id":"create-7b4f","request_scope":{"group_id":"g1","recovery_id":"r1"},"args":{"workflow_id":"order-7","workflow_type":"order","initial_state":"ready","partition_key":"orders-a","correlation_id":"invoice-7","payload":{"encoding":"base64","data":"AAH/"}},"attempt":{"timeout_ms":5000}}
```

```json
{"schema_version":"1","operation":"workflow.complete","tenant_id":"tenant-a","request_id":"complete-8e2d","request_scope":{"group_id":"g1","recovery_id":"r1"},"args":{"workflow_id":"order-7","partition_key":"orders-a","lease_token":"<opaque-secret>","fence":"42","expected_revision":"8","result":{"encoding":"base64","data":"b2s="}},"attempt":{"timeout_ms":5000}}
```

```json
{"schema_version":"1","operation":"workflow.get","tenant_id":"tenant-a","args":{"workflow_id":"order-7","partition_key":"orders-a"},"consistency":{"mode":"strong","after":{"group_id":"g1","recovery_id":"r1","index":"128"}},"attempt":{"timeout_ms":5000}}
```

`tenant_id` is a required asserted scope validated against authenticated authorization; HTTP retries also require exact equality with the path tenant. No `tenant`, `command`, `args.type/state` or flat-timeout aliases are accepted. Group and route hints are advisory; placement/related workflow atomicity is server-verified. Never use arbitrary advertised endpoints as unverified redirect targets.

Each individual mutation and whole atomic-group mutation requires `request_scope: {group_id, recovery_id}` obtained through authorized discovery before first send. The application verifies exact placement/recovery compatibility before proposal, including recheck after queue/poll/forwarding waits through the supervised handoff owner; it never fills missing scope from the latest namespace, trusts a mismatched asserted group, or silently changes scope during forwarding. SDK journals preserve tenant/scope/request ID/normalized intent across TCP/HTTP retry. An old scope after disaster restore rejects before proposal and does not prove the old attempt never committed; no automatic new scope/ID is safe reconciliation. Initial live group migration is unsupported. A deliberate new intent after unresolved restore/retention uncertainty needs the application's explicit reconciliation/operator policy, not a socket reconnect decision. Scope checks supplement the separate restore/member/worker fencing contract; they do not prove disaster recovery safe by themselves.

The identities are independent: header `call_id` identifies this socket attempt; durable `request_id` identifies normalized tenant/group/recovery mutation intent; workflow `args.correlation_id` identifies business lineage. Reconnect or HTTP retry uses a new call/attempt identity and the same durable request identity and scope. The shared [intent digest proposal](intent-digest.md) defines bounded normalized typed TLV/SHA-256 v1, after decode/default normalization; it includes opaque bytes, semantic guards, requested times and meaningful batch order, but excludes credentials, transport fields, traces, routing hints and `attempt` context. `request_scope` is required identity-key context, excluded from the digest; semantic partition args still enter it. An altered guard/payload under a retained request ID returns identity conflict. Raw JSON hashing is forbidden; v1 has no client-supplied `intent_digest` request field. Replies may expose `intent_digest: {normalization_version, algorithm, value}` and `outcome_retained_until_ms` with actual retained evidence. DTO JSON is never the persisted Raft command format.

Response bodies retain these exact shared semantic pairs:

| `outcome` | `submission` | Meaning |
|---|---|---|
| `applied` | `durable_outcome` | Known committed, applied and persisted mutation success |
| `rejected` | `durable_outcome` | Persisted deterministic rejection and applied checkpoint; no successful workflow change |
| `not_submitted` | `not_submitted` | This attempt is proven not to have crossed submission |
| `unknown` | `may_have_been_submitted` | Reconcile the same durable identity/intent |
| `read` | `not_applicable` | Query result with explicit consistency/freshness |

Known durable success and rejection include position/retention and request identity. Edge rejection uses a shared `error.code`, never a transport-private business enum. A timeout, auth denial, overload or CANCEL acknowledgement proving this attempt unsubmitted does not cancel or disprove an earlier attempt with the same request ID. Current principal denial prevents outcome disclosure, without rewriting history. HTTP's problem wrapper preserves the same extensions. Error/retry fields are one common typed contract.

```json
{"schema_version":"1","request_id":"complete-8e2d","request_scope":{"group_id":"g1","recovery_id":"r1"},"outcome":"unknown","submission":"may_have_been_submitted","error":{"code":"outcome_unknown"},"reconcile":{"operation":"workflow.outcome","request_id":"complete-8e2d","group_id":"g1","recovery_id":"r1"},"retry":{"action":"lookup_or_retry_same_request","after_ms":100}}
```

Queries return freshness/coverage/watermark metadata and bounded cursors. Stable client write positions include `group_id/recovery_id/index`; compare no indexes across groups or recovery identities. Outcome lookup is an authorized strong query whose args require `request_id`, `group_id` and `recovery_id`; the lookup target ID is not a new mutation ID. The outer query is `read/not_applicable`; `data.state` distinguishes `applied`, `rejected`, `not_observed`, `expired`, `incompatible_recovery` and `unavailable`, and a found row nests the original retained mutation outcome/position. `expired` requires bounded retained expiry metadata proving prior retention; if unavailable, return `not_observed` with `retention_guarantee: "unknown"`, never infer historical expiry from absence. Strong absence does not prove a previously submitted command cannot commit later. Exact expiry-marker retention and the automatic reconciliation horizon remain shared gates. No TCP error or lookup state invents a commit guarantee beyond the shared application contract. Mutation replies echo request_scope when safely known, including not-submitted/unknown; omission due to invalid framing is not scope substitution or outcome proof.

An individual mutation or whole `atomic_group` batch requires its outer durable request ID and request_scope. An `independent` batch envelope has no durable outer ID/scope; each item has a mandatory stable request ID/scope and preserved input position, and receives its own outcome/submission pair. Transport call correlation is not a durable orchestrator. Atomic-group intent is the ordered whole command in one server-verified group; cross-group atomic submission rejects before any proposal. Bounds apply to the complete batch and its complete reply, not just each item. No segmentation-based coalescing changes transaction semantics.

## Pipelining, ordering and flow control

The client pipelines independent calls on positive lanes subject to advertised count and byte bounds. Proposed explicit ordered-lane policy: each lane runs one application call at a time; the next call cannot cross the application's submission boundary before the earlier one yields a definitive known result. This guarantee comes from the selected server scheduling rule, not call_id numeric order or TCP arrival alone. Emit replies in lane order. Other lanes execute concurrently. This serial policy deliberately exposes head-of-line cost; use different lanes for independent work. A lane is not a partition, worker lease or FIFO lane, and lane IDs have no routing authority.

An uncertain submitted mutation retires its lane: return its unknown outcome if writable, reject queued later calls as `lane_stopped/not_submitted`, and never reuse that lane ID on this connection. There is no RESUME frame; even successful reconciliation does not resurrect the retired lane. Other lanes remain usable. The client reconciles the uncertain mutation before issuing dependent work on a new lane/connection. Transport serial order cannot provide a global order against concurrent HTTP/TCP writers; use explicit revision/fence guards, an acknowledged position followed by a suitable read barrier, or a supported same-group atomic batch. Do not pipeline completion that requires an unreceived claim token.

There is no application WINDOW_UPDATE frame initially. The application provides bounded per-call admission; TCP socket flow control plus per-session byte/call limits enforce the connection profile. A client sending beyond advertised limits receives definitive overload for requests that provably did not submit, or is closed when even bounded error delivery is impossible. Closing does not classify unresolved mutations. Independent or atomic batches are explicit common operations, not implicit coalescing based on TCP segmentation; unsupported cross-group atomic batches reject before submission.

One supervised reader validates/fragments input, one writer owns socket serialization, and bounded per-lane scheduling submits through the application. Do not create one unbounded task per frame or lock an entire connection through a slow await. Control is parsed independently of lane execution, with at most two unresolved client control calls, an 8 KiB control body cap and 16 KiB reserved control/error capacity. Bound control rate under the node/source admission policy; control cannot create an unlimited error queue. It cannot bypass a partial body ahead of it or guarantee delivery behind a blocked socket write. Partial-frame timeout closes because the stream cannot safely resume. Large frames cause head-of-line delay on one TCP connection; use a small bounded pool and pagination rather than claim multiplexing removes TCP head-of-line blocking.

## Bounds and reservations before allocation

The following are proposed initial effective limits, not measured safe capacities. Parent operations/conformance owns final deployment/global budgets; maximum effective values are the minima of server, authenticated tenant and compatible client receive limits. A lower client receive limit cannot admit a command whose worst-case required outcome cannot fit.

| Resource | Proposed initial limit |
|---|---:|
| Unauthenticated HELLO body | 8 KiB |
| Normal frame body / individual response body | 1 MiB |
| Individual decoded opaque value / total opaque bytes per call | 256 KiB / 512 KiB |
| JSON nesting / aggregate fields and collection items | 16 / 4,096 |
| Session lanes / executing data calls / aggregate pending data calls | 16 / 16 / 24 |
| Admission-queued data calls / queued calls per lane | 8 per connection / 8, also constrained by the connection aggregate |
| Parked claim waiters | Shared caps: 128/node, 64/group, 16/tenant, 8/connection; at most 16 registered scope keys/waiter |
| Resident input bytes / output bytes per session | 4 MiB / 4 MiB |
| Reserved control/error bytes per session | 16 KiB |
| TLS+HELLO wall budget / absolute frame assembly budget | 5 s / 5 s |
| Writer progress / absolute response write budget | 5 s or remaining attempt budget / remaining attempt budget |
| Shared attempt timeout cap / default | 30 s / 5 s |

Pending≤24 includes executing, admission-queued, parked and completed calls whose terminal response still awaits the writer; these categories do not each receive 24 slots. Retain the connection call permit until terminal delivery or safe reply abandonment, so many tiny results cannot create an unbounded reply-object queue behind a slow writer. Submitted work outliving that reply retains its separate application permit. Counts are upper bounds subject to atomic byte/cost admission. Ordinary admission queue wait is at most 10 ms or remaining deadline; queue owns only bounded input/state/queue resources, and fair dispatch atomically acquires execution/cost/output. Transfer allocation ownership once; concurrent physical copies remain charged separately. The independent parked-waiter ledger permits longer bounded claim waiting without execution/output reservations. Wake reauthorizes and reacquires full submission/response resources.

The final [operations profile](operations-and-conformance.md) selects an 8 MiB parked metadata/wake slice inside the 128 MiB request-state partition, at most 8 KiB metadata/waiter, scope keys≤256 bytes, at most 8 wake dispatch tasks/node, 32 ready waiters/batch and one coalesced token/waiter; worker waiter cap is 8. These are accepted finite proposed bounds, not proven memory/latency guarantees. These values require source scenario reconciliation and target workload measurements before adoption; a smaller reply cap must page/reject supported query shapes without omitting asserted data. Source acceptance may require an explicitly reviewed larger profile, never silently weakened assertions. Config changes that alter outcome/digest retention never silently change semantic retry guarantees.

Required acquisition order: accept-slot and fixed handshake allowance before TLS; fixed 24-byte header scratch before socket read; validate checked header arithmetic and state; acquire body-byte/decode-envelope/queued-call reservation from connection and node frontend pools before allocating or growing the body; bounded structural preflight before typed field/base64 allocations; shared execution/cost and worst-case outcome capacity before submission. No body_len-driven `reserve` occurs before permits. Backpressure never queues an unbounded number of blocked reservation waiters.

Count physical buffer capacity and retained backing allocations, not only `Bytes` slice length. Include TLS buffers, codec read-ahead, UTF-8/token processing, decoded bytes, DTO collections, response construction and partial-write backing storage. Per-session 4 MiB ceilings are acquired lazily from shared pools, not preallocated entitlements: 1,024 sessions cannot allocate 4 MiB each under a 64 MiB ingress partition. Baseline control/TLS/session allowances also consume their declared global budget. Likewise 256 work slots do not admit 256 concurrent 1 MiB outputs under a 64 MiB response pool. Typed schema limits and a conservative reviewed decode expansion allowance form the reservation; a generic JSON tree cannot bypass it. Allocation/decode failure before submission yields definite rejection when writable. Safe code does not turn out-of-memory into an ordinary error reliably; reserve and bound proactively, and validate peak RSS in the future fault/load harness.

Separate lifetimes: frontend input permits follow retained bytes; output permits follow buffers until written/dropped; submitted execution permits follow the supervised application operation until durable resolution/failed-state ownership transfer. Dropping a socket future cannot release a still-running submitted operation's permit. Reply construction must have capacity before proposal: committed mutation result may never be replaced by an unrecorded business rejection because the reader became slow. The retained authoritative outcome remains lookupable after delivery fails.

Shared application admission accounts for HTTP and TCP together on one node by tenant/group, operation work, storage/snapshot pressure and bounded response cost. Per-connection limits are an additional layer; using several nodes can multiply local allowances and is not a cluster-wide quota guarantee. Use bounded fairly scheduled decode/submission work; reserve capacity for lease extension/reconciliation as a slice of total active/response budgets, separately from parked claim waiters and heavy projections, with finite tenant limits. Priority never reorders already committed log entries or authorizes a stale fence.

## Cancellation, close and drain

CANCEL uses a fresh control call_id and names `target_call_id` as a decimal string. Cancellation races through the same supervised submission-state owner as dispatch. If the target is queued/parked and removed before proposal can start, return CANCEL_ACK `not_submitted` and the target RESPONSE with definitive cancellation. If proposal may have started, CANCEL_ACK says `too_late`; it never reports a successful durable rollback. Target completion/cancellation/timeout compete for exactly one terminal RESPONSE; the separate CANCEL_ACK is not a second target result. A cancelled queued target's terminal RESPONSE waits in its bounded lane-order writer queue where required. A racing known result remains that terminal response, or remains lookupable if delivery was already abandoned. Distinguish unknown target from already-finished only when bounded recent-call metadata proves completion; no unbounded retired-call registry. Non-submission of this target attempt says nothing about another attempt with the same durable ID.

An attempt clock starts when the bounded header is accepted, using server monotonic time; it includes body receipt, decode, authorization, queue/parking, forwarding, consensus/application/persistence wait, encoding and delivery. Separate absolute partial-header/assembly watchdogs bound bytes before `attempt.timeout_ms` can be decoded. When decoded, apply the requested budget to that original start, never to decode completion; already-expired requests do not submit. Writer timeout cannot reset or extend it. It is not a domain lease or absolute client wall-clock deadline. Expiry before the coordinated submission boundary is not-submitted; expiry after it yields unknown unless the retained outcome is known. On peer close, release unsubmitted queued calls/waiters, supervise submitted work and retain its outcome. Socket cancellation never performs workflow.cancel, releases a durable lease, changes fencing or interrupts committed apply.

Node drain first stops accepts and ordinary/claim admission, removes unsubmitted claim waiters, then sends GOAWAY with reason and drain duration. Bounded authorized maintenance/reporting/reconciliation on already established connections may use the shared maintenance slice until the common service enters full stop; the TCP adapter cannot invent a separate drain exception. Reject provably unsubmitted ordinary queued work and wait a bounded period for submitted work and reply writes. A reported last decoded call_id is diagnostic, not an acknowledgement of all lower calls or proof later calls cannot commit. On final close every unresolved mutation is unknown unless a definitive outcome was observed. Application operations outliving the connection keep their permits and reconciliation owner; node shutdown must use the shared Raft/storage drain contract, not kill tasks on a TCP-only timer. No automatic QUIT lease release.

## Worker client behavior

Implement TCP behind `WorkflowClientPort`, in an outer/private worker client module; it may depend on types, approved domain values, wire and approved client port contracts, never server adapters/storage/consensus. Own bounded endpoint pools, verified TLS/auth, negotiation, call numbers, lane correlation, send buffers, remaining deadlines, jitter and durable request journals needed for reconciliation. One socket reader maps responses; no consumer steals another handler's reply. Do not infer success from a completed socket write.

Claim only up to actual handler capacity; submitted-but-unknown claims consume reserved worker capacity until replay/outcome reveals whether leases were acquired. Retrying claim with a new request ID can acquire extra work and is not automatic reconciliation. Original-outcome replay does not renew an old lease: before resuming a replayed claim, obtain current strong authorized ownership evidence for its group/recovery identity, worker, lease token, fence, state and applicable revision/deadline. Never execute an expired/reclaimed/cancelled/restored stale claim from its retained reply alone. Such a check is an as-of-state observation, not protection from a subsequent revoke; external fencing/idempotency still matters. An uncertain heartbeat does not establish a renewed lease; uncertain complete/fail/retry replays the same request identity and original guard values. External handler effects remain at-least-once unless the external system enforces its own idempotency.

Initial long-poll claim capability can be off for both transports. If enabled, it uses the shared bounded application waiter facility outside Raft and a dedicated client lane; it must not park holding a mutation execution slot, worst-case response reservation or block lease maintenance. Reconcile/check retained outcomes before parking a replay. Notifications may cause bounded authoritative eligibility rechecks, but each durable ID has one logical finalized claim outcome, including an empty committed claim after contention; later polling uses a fresh independent ID after that known outcome. Physical retransmission/failover may propose duplicate Raft entries: apply deduplicates to the retained outcome before selection and never obtains another lease set. This is not an at-most-once-log-entry guarantee. An unsubmitted cancelled/expired wait returns `not_submitted`, not a durable empty claim. Worker shutdown stops claiming, resolves unknown claims as bounded policy permits, keeps lease maintenance during handler drain and reports/reconciles results; final disconnect is not durable release. Pools have small per-endpoint bounds; extra connections on one node cannot evade its shared tenant limits.

Follower routing policy: application owns trusted forwarding or definitive redirect with not-submitted evidence; TCP does not blindly retry or decide leader authority. After an uncertain forward/response loss, replay or lookup the same durable identity. Retry redirects/definitive overload with bounded attempts, remaining time and jitter; do not automatically retry auth, schema or identity-conflict errors. No retry loop gets a fresh deadline at every hop.

## Safe implementation candidates and validation gates

Candidates are Tokio for sockets/runtime, bytes for bounded immutable/owned buffer views, tokio-util for safe codec plumbing, rustls with tokio-rustls for TLS, and serde/serde_json for the common JSON DTOs. No first-party unsafe, custom TLS, unchecked pointer casts, packed struct reinterpretation or homegrown general serialization framework. Bounded primary documentation was checked on 2026-10-09; selected-release/MSRV/features/dependency audit and safe parser/admission spike remain gates.

| Primary API evidence | Proposed use and limits of inference |
|---|---|
| [Tokio TcpStream](https://docs.rs/tokio/latest/tokio/net/struct.TcpStream.html) exposes caller-buffer `try_read` and readiness APIs | Header-first bounded socket reads; this API is not an aggregate byte reservation or scheduling guarantee |
| [Tokio write_all](https://docs.rs/tokio/latest/tokio/io/trait.AsyncWriteExt.html#method.write_all) documents partial-write cancellation risk | Persist writer offsets in the one writer owner; never cancel and restart a full frame on a live stream. If partial-state recovery is uncertain, close and classify unresolved mutations conservatively |
| [bytes Bytes](https://docs.rs/bytes/latest/bytes/struct.Bytes.html) supports shared backing allocations and distinct slice views | Charge the allocation until its last retained owner drops; small slice lengths do not establish low memory usage |
| [tokio-util length framing](https://docs.rs/tokio-util/latest/tokio_util/codec/length_delimited/index.html) supports length offset/adjustment and whole-frame buffers | FWTP's body length is at offset 20; a safe header-first wrapper must reserve before growth. Candidate framing helpers do not supply our admission contract |
| [rustls ServerConfig](https://docs.rs/rustls/latest/rustls/server/struct.ServerConfig.html) exposes ALPN/protocol/early-data configuration and warns about shared resumption stores across auth policies | Explicit TLS 1.3/ALPN selection, zero accepted early data and isolated resumption policy; these knobs do not establish tenant authorization or revocation freshness |

The default ALPN configuration is not our required protocol policy; configure/check it explicitly. The reservation wrapper around any TLS/framing integration is a proposed design obligation inferred from buffer ownership, not a documented library guarantee. Pure typed DTO decoding must enforce our duplicate/unknown/structure limits; dependency names alone do not do so.

Do not rely on a `LengthDelimitedCodec` maximum alone: an off-the-shelf framed reader may buffer/grow before the application's byte reservation. Use safe header-first acquisition around codec reads, constrain read-ahead, and prove total permit ownership. `Bytes` refcounting does not mean memory is bounded; bounded Tokio channels do not bound contained byte capacity. The future implementation gate must demonstrate safe checked partial-header/body parsing, no reservation bypass through framing/TLS/decode, and cancellation-safe partial read/write ownership.

## Required tests and performance evidence, specified only

These supplement every included original workflow scenario/assertion and its stable source identity; no executable tests are added in this design phase.

| Contract | Required scenarios / assertions |
|---|---|
| Frame and codec | Golden 24-byte header and HELLO/REQUEST/RESPONSE bytes; every header/body fragmentation boundary; multiple coalesced frames; truncated EOF; checked lengths; invalid magic/version/kind/flags/lane/call; no resynchronization or dispatch |
| Schema/bytes | Duplicate/unknown fields, invalid UTF-8, malformed base64 and integer strings, absent/null/zero distinctions, limits at/above boundaries, nesting/items, encoded+decoded byte charge, preserved opaque bytes and SDK digest vectors |
| Bounds | Huge claimed lengths rejected before body allocation; many connections/lanes/frames; read-ahead/TLS/retained `Bytes` capacity; tiny completed replies remain within pending count and writer byte/object bounds; no unbounded waiter/task/retired-call queue; failed reservation never submits; permit leak/early-release assertions |
| Auth/TLS | Failed version/ALPN/certificate/auth, no downgrade/early-data mutation, tenant denial, per-call expiry/revocation including replay, credential redaction, authorized mTLS identity, reconnect refresh |
| Ordering | Same-lane definitive order; independent cross-lane out-of-order replies; unknown stops lane with queued work provably unsubmitted; concurrent connections/HTTP guards; FIFO lane-head cases remain authoritative and select eligible heads despite 80 earlier-due followers |
| Cancellation/drain | Queued/waiter cancellation vs submit boundary, exactly one target terminal reply plus separate ACK, known/unknown/cancel races, partial read/write cancellation, close before/after commit/apply, GOAWAY diagnostics not outcomes, slow readers/control congestion, maintenance-only drain, execution permit stays owned through unresolved submission |
| Retry/failure | Leader loss at acknowledged precommit and committed-before-reply hooks; uncertain forwarding; same-ID/scope replay through HTTP/TCP; different-intent conflict; lookup absence while original later commits; scope acquired/journaled before first send; old/missing/mismatched scope rejects before proposal and never auto-rebinds after restore; retention/restore epoch; no extra leases from uncertain claim retry |
| Worker | Bounded capacity includes unknown claims; current strong ownership check before executing retained old claims; empty committed poll stays final for its ID; duplicate physical Raft entries replay the retained claim rather than select new leases; heartbeat/completion races; tokens/fences preserved; stop-claim then handler drain; effects not exactly-once; reconnect correlation cannot collide with previous-session calls |
| Shared service | Mixed HTTP/TCP exhaust one tenant/group/cost budget; client pool cannot multiply it; client listener cannot reach peer RPC/admin/consensus/storage; wire cannot import effect crates |

Protect p99 through explicit admission/queue/body budgets, fair shared scheduling, bounded result pages, capped batch work and delayed/bounded background maintenance. Measure TLS costs, parse/base64 overhead, head-of-line effects, disk/Raft stalls and mixed-transport load rather than assume TCP improves p99. Report offered and completed load, overload/unknown/availability rates and client-observed successful/rejected latency separately; include timeouts/retries in workload accounting. Neither multiplexing, a timeout nor a queue cap guarantees successful durable writes within a latency number. No throughput or p99 guarantee has been measured.

Before implementation: freeze the common schemas and cancellation/outcome taxonomy, effective resource profile, legacy byte exclusions, global pools, request-outcome retention/recovery contract, trusted forwarding, exact TLS/auth profile and current library releases. Cross-review and actual question resolutions belong in [TCP review](../reviews/transport-tcp-review.md).
