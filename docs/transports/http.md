# HTTP workflow API design

Status: proposed design only, authored by HTTP agent `01a12041-6519-7c42-8bb8-462619dea045` on 2026-10-09. No Rust, Cargo, executable CI, test implementation or commit is authorized. Initial draft preceded bounded primary-documentation research; actual reciprocal reviews are recorded in [HTTP review](../reviews/transport-http-review.md). No performance, production readiness or executed parity is claimed.

## Scope and ownership

The [workflow parity contract](../workflow-test-parity.md), [architecture](../architecture.md), [workflow analysis](../notes/workflow-analysis.md) and [implementation plan](../implementation-plan.md) remain controlling requirements. Full scoped workflow parity is wider than create/claim/complete. Deferred governance, approvals, budgets, effects and durable steps remain incomplete parity, never silently green or excluded.

Parent owns [shared transport design](../transport-design.md) and the architecture ADR. This file proposes the HTTP mapping of one normalized contract. `wf-wire` owns pure validated DTO contracts using approved types/domain values; `wf-api-http` owns HTTP decoding and presentation and depends only on types/domain/wire/application. `wf-api-tcp` is its sibling. `wf-node` gives both ONE application service for tenant authorization, admission, routing, deduplication, deadlines and workflow rules. Neither adapter gets raw ports, storage or consensus capabilities. No transport-specific Raft or quota authority is introduced. Framework, TLS and client types remain outside the functional core.

## Proposed routes and operation mapping

Proposed prefix is `/v1/tenants/{tenant_id}`. Tenant IDs are bounded ASCII identifiers whose URL encoding is canonical; workflow IDs and opaque partition components are carried in JSON rather than embedded in paths. This avoids treating workflow IDs containing `/` as route structure. Required body `tenant_id` must equal the URL tenant after validated decoding; mismatch rejects before proposal. Equality supplies asserted scope, not authority: shared authorization binds the principal to that tenant. This is a new API proposal, not compatibility with `/dashboard` or Redis/native command framing.

| Endpoint | Contract |
|---|---|
| `GET /v1/capabilities` | Authenticated schema/operation/limit discovery; no tenant existence enumeration |
| `POST /v1/tenants/{tenant_id}/commands` | Exactly one typed normalized mutation, including explicitly bounded batch commands |
| `POST /v1/tenants/{tenant_id}/queries` | Typed read with scope, consistency, pagination and projection watermark; no mutation despite POST |
| `GET /v1/tenants/{tenant_id}/requests/{request_id}?group_id=...&recovery_id=...` | Maps to `workflow.outcome`: authorized lookup requiring BOTH group and recovery identities; no silent current-recovery substitution |

The two POST routes return discriminated typed replies rather than accepting a general KV language. A command operation tag is a closed `workflow.*` enum with snake_case names. Query tags use the same vocabulary. Outcome lookup is also available as `workflow.outcome` on `/queries`, with args request_id/group_id/recovery_id; its request_id is a lookup argument, not a new mutation ID. At M4, authenticated single-group discovery supplies the authorized stable group/recovery scope for a tenant; it does not expose Raft membership or dial authority. A future placement resolver is an authorized query with a frozen typed registry entry, not client-selected cluster control. The table describes coverage obligations; final enum names and source case identities freeze at M0/M4. An absent advertised capability is explicit `unsupported_operation` before submission, never silent success. Routes do not establish a feature disposition.

| Scoped workflow operation | Proposed tag / endpoint | Required behavior and evidence family |
|---|---|---|
| Create / bulk create | `workflow.create`, `workflow.create_many` / commands | Fixed type/defaults, caller item order, per-item partition placement, supplied identity and payload metadata; SDK and flow create tests |
| Get / multi-get / list / locate | `workflow.get`, `workflow.get_many`, `workflow.list`, `workflow.locate` / queries | Exact partition scope, absent vs omitted fields, ordering and bounded cursor; read/locator/shared-scope suites |
| Claim due / blocking claim / priority and multi-state claim | `workflow.claim_due` / commands | Authoritative lane-head selection, earliest-due/multi-state and priority semantics; fencing, FIFO, claim and waiter suites |
| Transition / advance / bulk transition | `workflow.transition`, `workflow.transition_many` / commands | Expected logical state, revision and lease/fence guards; running and run_state remain distinct |
| Complete / fail / cancel and corresponding many operations | `workflow.complete`, `workflow.fail`, `workflow.cancel`, `workflow.complete_many`, `workflow.fail_many`, `workflow.cancel_many` / commands | Central terminal hooks, lineage accounting, history and batch rollback; terminal/child tests |
| Retry / retry-many / retry exhaustion | `workflow.retry`, `workflow.retry_many` / commands | Attempt/backoff policy, cleared lease, preserved run_state, exact active/terminal exhaustion target |
| Extend / release / reclaim lease | `workflow.extend_lease`, `workflow.release_lease`, `workflow.reclaim` / commands | Current ownership/fence, expiry boundary and authoritative group time; source-name mapping must confirm release capability |
| Set/delete/install/patch/read policies and state metadata | `workflow.policy_set`, `workflow.policy_delete`, `workflow.policy_get`, `workflow.policy_list`, `workflow.state_meta` / commands or queries by effect | Replace vs patch, generations, state/type defaults and leased-job policy behavior |
| Schedule creation/update/pause/resume/delete and get/list/summary | `workflow.schedule_*` / commands or queries by effect | Target IDs, deadlines, dispatch count, cancel/timeout races and recovery; schedule suites |
| Signal publish/wait/consume and signal reads | `workflow.signal`, `workflow.wait_signal`, `workflow.signal_*` / commands or queries by effect | Durable signal-versus-timeout ordering, scope and ownership; signal tests |
| Spawn children / wait group / child-group reads / lineage | `workflow.spawn_children`, `workflow.children`, `workflow.lineage` / commands or queries by effect | Same-group atomic parent/child changes, wait:any/all, sibling policy, idempotent resolution; cross-group reject |
| Rewind | `workflow.rewind` / commands | Selected history event, original payload/state and explicit guards; destructive action authorization |
| History / history values / payload and result value reads | `workflow.history`, `workflow.history_values`, `workflow.value_get`, `workflow.value_get_many` / queries | Range/reverse/event/worker filters, lazy hydration, metadata preservation, missing vs unavailable bytes |
| Value put/delete, if selected by compatibility gate | `workflow.value_put`, `workflow.value_delete` / commands | Workflow-scoped references only; initial replicated inline limit, no unproven large-blob availability |
| State/type/worker/due/statistics/retention reads | `workflow.states`, `workflow.types`, `workflow.workers`, `workflow.due`, `workflow.stats`, `workflow.retention_get` / queries | Authorized scopes and freshness; no general KV access |
| Retention policy and bounded cleanup | `workflow.retention_set`, `workflow.retention_run` / commands | Reachability, retained outcomes/history/snapshots, exact counts and safe cleanup assertions |
| Governance / approvals / budgets / durable effects / durable steps | `workflow.governance_*`, `workflow.approval_*`, `workflow.budget_*`, `workflow.effect_*`, `workflow.step_*` / commands or queries by effect | Explicit supported/deferred/dropped decision per source case at M0/M5; no wildcard execution or invented green parity |
| Worker convenience / handler outcome | Claim + extend + transition/retry/terminal operations above | Handler outside Raft; uncertain reporting never becomes an automatic second fail/retry command |

Wildcard rows are inventory families, not accepted arbitrary tags. The M0 source mapping must enumerate each actual operation before capability advertising. Group membership, snapshots and cluster administration use a separately authorized operational plane and do not masquerade as workflow commands.

### Bounded pinned-source observations

Pinned source is `66586c596e271408d9184af32eb31cd7218c126e`; the preserved files below were read as immutable references. These are observations about required workflow semantics, not a claim of exhaustive source audit or ported tests. The broad operation table is derived from [workflow analysis](../notes/workflow-analysis.md) and [the mandatory parity contract](../workflow-test-parity.md); its unenumerated scope families remain explicit gates.

- [flow_paths_test.exs](../../tests/reference/elixir-baseline/apps/ferricstore_server/test/ferricstore_server/health/endpoint/flow_paths_test.exs) establishes IDs containing slash, explicit partition scope and rejection of percent-decoded invalid UTF-8. Proposed JSON IDs preserve the logical ID; obsolete dashboard/form syntax needs a case-level compatibility disposition.
- [route_requirements_test.exs](../../tests/reference/elixir-baseline/apps/ferricstore_server/test/ferricstore_server/health/endpoint/route_requirements_test.exs), scoped detail/value and destructive-form cases, binds partition/action permissions for reads, reclaim, retention, policy and rewind. Mapping a route without shared operation/scope authorization is insufficient.
- [http_flow_routes.exs](../../tests/reference/elixir-baseline/apps/ferricstore_server/test/ferricstore_server/health/dashboard_test/sections/http_flow_routes.exs), lazy-value case, asserts detail/refresh performs no value fetch before an explicitly scoped value request. Its reclaim case preserves supplied type, partition list, worker, limit and lease_ms. Its rewind case restores the selected history event/state using expected state/revision. New routes must preserve those included workflow assertions; 302 HTML redirects and UI text receive separate scope decisions.
- [claim_waiters_test.exs](../../tests/reference/elixir-baseline/apps/ferricstore/test/ferricstore/flow/claim_waiters_test.exs) was boundedly navigated by test declarations: row/key capacity, atomic broad registration, no unrelated scope wakeup and delayed-timer cleanup are explicit future conformance obligations. No claim that its full implementation or suite was executed is made.

## Encoding, version and identity

Proposed media type is `application/json`; URL major `v1` and envelope `schema_version: "1"` must agree. Common request fields are schema_version, operation, tenant_id, args, mutation request_id AND request_scope, optional consistency and attempt. `request_scope:{group_id,recovery_id}` is REQUIRED for each individual mutation and the outer atomic_group batch; an independent-batch envelope has neither durable outer ID nor scope, and each item has both. Queries retain their explicit consistency/lookup scopes rather than inheriting a mutation scope. `command`, `type`/`state` create aliases and flat attempt_timeout_ms are rejected in v1. TCP's negotiated `wf.v1` label maps explicitly to this same schema_version 1. Unsupported major/schema or requested required capability is rejected before proposal. A capability response lists enabled tags and bounded effective limits; every voting replica must understand the chosen committed command version. HTTP protocol negotiation (HTTP/1.1 or HTTP/2 over TLS) is separate from workflow schema negotiation. HTTP/3 is not selected by this design.

Opaque workflow payload/result bytes are tagged base64, never assumed UTF-8 or parsed as user JSON: `{"encoding":"base64","data":"AAH/"}`. Use RFC4648 standard alphabet, required padding when needed, no whitespace and zero pad bits; our rejecting noncanonical decoder policy is stricter than permissive decoding. Decoded bytes, not textual spelling, enter normalized intent. Reject invalid/noncanonical base64, duplicate JSON fields, trailing data, unknown command fields and ambiguous numeric values. Unsigned 64-bit revisions, fences, attempts, indexes, epoch counters and unsigned millisecond values use decimal strings with canonical grammar `0|[1-9][0-9]*`, checked to `u64`; clients must not convert them to JavaScript Number. Signed timestamp fields, if selected, require a separately declared signed decimal type. Small structural lengths/limits can be bounded JSON integers. Null, absent, empty bytes and unavailable payload are distinct tagged states. No silent Unicode or partition-key normalization is permitted. Standards basis: [RFC4648 §§3.5/4](https://www.rfc-editor.org/rfc/rfc4648.html#section-3.5) and [RFC8259 §§4/6](https://www.rfc-editor.org/rfc/rfc8259.html#section-6); string-u64 and strict field policies are our proposed contract.

Every individual new public mutation and whole atomic-group batch requires stable request_id AND request_scope; an independent-batch envelope has no durable outer ID/scope and every item requires both. TCP call_id is attempt correlation only and workflow correlation_id remains business lineage. HTTP has no connection-local call_id requirement. The application checks tenant + server-verified request_scope.group_id + request_scope.recovery_id + request_id with a versioned canonical decoded-intent digest. Client obtains an authorized stable scope before first submission and stores it with ID/original intent, preserving all three on every HTTP/TCP retry. Authoritative placement verifies the asserted scope at admission and immediately before consensus handoff after any queue/poll/forwarding; old/incompatible recovery rejects before proposal. The server never fills in the latest recovery, and pending work cannot transfer to a newly restored namespace.

Scope is identity KEY context, not membership authority, a route hint or a digest field. Semantic partition/workflow values remain hashed in args. Operation, command version, explicit guards, payload bytes, meaningful atomic item order and normalized semantic defaults are included. Credentials, trace/call IDs, raw HTTP bytes, advisory routing hints and per-attempt wait/deadline are excluded. Explicit scheduling time is semantic and included; server prepared time/IDs have separate committed-entry identity. V1 accepts NO client-supplied intent_digest field: the application computes the digest and compares it with retained version/intent, rejecting reuse for different input. Replies echo request_scope with durable results and not_submitted/unknown when known; they expose intent_digest normalization_version/algorithm/value and outcome_retained_until_ms where applicable. An incompatible-scope denial is this-attempt non-submission and does not settle the original outcome in its old namespace. SDK never replaces scope or creates a fresh ID automatically after unknown; starting new intent requires reconciliation/operator decision. Parent selected SHA-256 over [typed TLV normalized intent v1](intent-digest.md); that common document owns the prefix/tags/default rules, schema-registry dependency, retained-version support and future golden vectors. No adapter independently hashes raw JSON or defines a competing canonical algorithm. Float/partition type and lossless wire semantics remain case-level parity gates; a string-u64 profile does not authorize converting source float fields or treating all numeric-looking strings as numbers.

Deduplication precedes re-evaluating changed workflow guards and persists original accepted/rejected outcomes with application state. Current tenant authorization precedes outcome disclosure. Retention bounds the original-outcome promise and advertises `outcome_retained_until_ms`; after expiry clients must not assume replay is safe. Legacy no-ID lease/fence tests remain a separate compatibility surface with original guard/history no-op rules. The harness must not manufacture common request IDs to pass them.

Illustrative typed create (values and limits are proposed, not measurements):

```json
{
  "schema_version": "1",
  "tenant_id": "tenant-a",
  "request_id": "req-create-01",
  "request_scope": {"group_id": "g1", "recovery_id": "r1"},
  "operation": "workflow.create",
  "args": {
    "workflow_id": "mail/2026-10-09/17",
    "partition_key": "tenant-a",
    "workflow_type": "send_mail",
    "initial_state": "queued",
    "run_at_ms": "1791540000000",
    "payload": {"encoding": "base64", "data": "AAH/"},
    "correlation_id": "campaign-17"
  },
  "attempt": {"timeout_ms": 5000}
}
```

Illustrative committed reply:

```json
{
  "schema_version": "1",
  "request_id": "req-create-01",
  "request_scope": {"group_id": "g1", "recovery_id": "r1"},
  "outcome": "applied",
  "submission": "durable_outcome",
  "intent_digest": {"normalization_version": "1", "algorithm": "sha256", "value": "<64 lowercase hex characters>"},
  "data": {"workflow_id": "mail/2026-10-09/17", "revision": "1", "state": "queued"},
  "position": {"group_id": "g1", "recovery_id": "r1", "index": "42"},
  "outcome_retained_until_ms": "1791626400000"
}
```

The parent owns the common field names and canonical digest. Placeholder digest and timestamp are shape examples, not an actual computed vector or selected retention duration. A complete call requires Authorization and a URL tenant equal to tenant_id; the HTTP body above is identical to the TCP REQUEST JSON for the same intent.

Illustrative leased completion request:

```json
{
  "schema_version": "1",
  "tenant_id": "tenant-a",
  "operation": "workflow.complete",
  "request_id": "req-complete-17",
  "request_scope": {"group_id": "g1", "recovery_id": "r1"},
  "args": {
    "workflow_id": "mail/2026-10-09/17",
    "partition_key": "tenant-a",
    "expected_state": "running",
    "expected_revision": "8",
    "lease_token": "<opaque-secret>",
    "fence": "18446744073709551614",
    "result": {"encoding": "base64", "data": "b2s="}
  },
  "attempt": {"timeout_ms": 5000}
}
```

## Authentication and tenant scope

Proposed production profile is TLS1.3 with verified server name/trust roots; ALPN negotiates h2 or http/1.1 separately from the TCP wf-tcp/1 listener. Disable TLS early data for all public workflow requests. Authenticate bearer credentials in `Authorization: Bearer ...` or deployment-selected mTLS identity mapped by one application auth service; never put credentials in query parameters. Header bearer usage is grounded in [RFC6750 §2.1](https://www.rfc-editor.org/rfc/rfc6750.html#section-2.1); the selected credential format, audience and revocation policy are deployment gates. Never treat routing/tenant headers or a client-supplied worker ID as authority. Identity resolution supplies principal and authorized tenant/operation/scope; both adapters call the same policy. Outcome lookup and replay require current permissions even when original submitter permissions changed. Requests cannot select another principal by body field. Workflow claims additionally bind authorized worker identity; lease tokens are secrets and fence counters do not replace authorization. A parked wait rechecks expiry, revocation and scope on wake before proposal.

Proposed v1 has no cookie authentication. A future browser session boundary needs explicit origin/CSRF policy. CORS is deny by default; allow configured origins only, never wildcard with credentials. Validate HTTP authority/Host through the deployment boundary; forwarded identity and routing fields are trusted only from configured proxies. TLS to a proxy does not authenticate its arbitrary forwarded headers. Cross-tenant missing/denied records must not leak existence. Queries with broad/auto-partition scopes must still authorize every selected scope. Redact credentials, tokens, payloads and private IDs from logs as the operational policy requires.

## Replies and errors

Success follows committed application and required durable persistence, never socket receipt or leader acceptance. Proposed command success is `200` with a typed durable outcome; empty claims also have an explicit result, not `204` with missing ownership information. Query success is `200` with read metadata. No `202` asynchronous command handle is selected without an explicit durable submission protocol.

Use a common typed error enum with HTTP presentation `application/problem+json`: standard problem fields plus shared `error.code`, `request_id`, validated `request_scope` when authorized/known, `outcome`, `submission`, `retry`, structured field errors, optional position/retention/reconciliation metadata. Never infer/fill current scope after failed validation or reveal unauthorized topology. [RFC9457 §3.2](https://www.rfc-editor.org/rfc/rfc9457.html#section-3.2) allows problem-specific extensions; these particular fields and semantics are our application contract. Human detail is redacted and never parsed by clients. A deterministic rejection may carry an applied position/outcome record; transport validation/admission failures do not invent one. Use exactly the parent outcome/submission pairs: applied/durable_outcome, rejected/durable_outcome, not_submitted/not_submitted, unknown/may_have_been_submitted and read/not_applicable. Rejected/durable_outcome means a persisted deterministic rejection, not successful workflow mutation. Non-submission is evidence about THIS attempt only; it does not undo or disprove an earlier attempt with the same request ID.

| HTTP status | Typed categories / meaning |
|---|---|
| 400 | Malformed envelope, ambiguous/duplicate input, invalid encoding or bound/field shape; not submitted |
| 401 / 403 | Authentication / tenant-operation denial; not submitted by this attempt |
| 404 | Unknown route or authorized resource missing; outcome lookup uses its explicit state contract rather than conflating absence with rejection |
| 406 / 415 | Unsupported accepted representation / request media type; not submitted |
| 408 | Header/body receipt watchdog expired before proposal; not submitted by this attempt |
| 409 | Request ID reused for different intent, stale fence/revision/state, unsupported group atomicity; typed reason distinguishes cases |
| 413 | Encoded or decoded payload/body bound exceeded; not submitted |
| 422 | Recognized semantic input violates a pure domain rule; recorded rejection if submitted |
| 429 | Shared tenant/worker admission exhausted; not submitted when the server can prove that; bounded Retry-After hint |
| 501 | Known operation deferred/unsupported at this rollout; not submitted and visible in capabilities |
| 503 | No safe leader/quorum/read path or draining; not submitted only with proof, otherwise unknown |
| 504 | Attempt deadline elapsed; submission state explicit, generally unknown after submission |
| 500 | Internal/storage failure; never pretend a business rejection or empty read; may be unknown |

Unknown-outcome example:

```json
{
  "type": "urn:ferricstore:problem:outcome-unknown",
  "title": "Command outcome is unknown",
  "status": 504,
  "schema_version": "1",
  "error": {"code": "outcome_unknown"},
  "request_id": "req-complete-17",
  "request_scope": {"group_id": "g1", "recovery_id": "r1"},
  "outcome": "unknown",
  "submission": "may_have_been_submitted",
  "retry": {"action": "lookup_or_retry_same_request", "after_ms": 100},
  "reconcile": {"operation": "workflow.outcome", "request_id": "req-complete-17", "group_id": "g1", "recovery_id": "r1"}
}
```

A timeout, stream reset, partial response or disconnected socket is client-local unknown unless a validated response proves this attempt not-submitted or gives a durable outcome. An HTTP gateway's own status/body is not a trusted application outcome. A lookup itself returns read/not_applicable, with a `data.state` of applied, rejected, not_observed, expired, incompatible_recovery or unavailable; a found record nests the original retained mutation outcome/position. `not_observed` is not proof that an in-flight original cannot commit. `expired` requires retained expiry metadata proving the original guarantee expired: a missing row alone is insufficient. Without that evidence use not_observed with retention_guarantee unknown. A bounded tombstone/expiry-frontier policy and its limits must be selected at M7; after that evidence is itself removed, do not invent historical existence. Missing/expired records do not permit an automatic fresh-ID mutation. Replays beyond the advertised original-outcome window have no once-only promise. An uncommitted in-memory pending hint, if exposed, has no durability promise. Same-ID replay uses the original intent, never a new complete/fail/retry mutation to mask uncertainty. Exactly-once external effects are not promised. Query/lookup failures may carry error/status with read/not_applicable because the query has no mutation proposal; they do not settle the original mutation. The status table's submission rules apply to mutation attempts, while read freshness/unavailability remains explicit.

## Reads and watermarks

Each query requests `consistency:{"mode":"strong"|"projected","after":position}` with optional per-group minimum stable applied position. Strong workflow/ownership/outcome reads require the consensus-approved barrier and authoritative state. Projected reads return checkpoint/freshness and either wait to the supplied minimum within the attempt deadline, use an explicitly supported authoritative fallback, or fail `watermark_unavailable`; no silent downgrade. The requested consistency and actual path are returned. Projected mode is rejected for correctness-critical ownership/outcome queries rather than silently used for reconciliation.

Positions identify group and recovery namespace plus durable log index. Local projection generations and cursor versions are separate and cannot invalidate valid durable read-your-write positions after normal restart/snapshot. Different groups/recovery identities are incomparable. Cross-group queries return a vector and `snapshot_scope: "per_group"`; no global snapshot claim. Pagination cursor binds tenant, query shape/order, consistency, group/checkpoint/retention scope; tampering and incompatible/expired cursors reject, never restart silently.

Workflow detail defaults to payload metadata, preserving lazy hydration. Explicit include/value queries return tagged `present`, `omitted`, `absent` or `unavailable` plus metadata and decoded-byte budget. A missing durable payload is not an empty base64 string. History ranges/reverse/filters, exact cardinality/order and hidden internal fields remain parity obligations.

Illustrative read after a write:

```json
{
  "schema_version": "1",
  "tenant_id": "tenant-a",
  "operation": "workflow.get",
  "args": {"workflow_id": "mail/2026-10-09/17", "partition_key": "tenant-a", "include_values": false},
  "consistency": {"mode": "strong", "after": {"group_id": "g1", "recovery_id": "r1", "index": "42"}},
  "attempt": {"timeout_ms": 5000}
}
```

```json
{
  "schema_version": "1",
  "outcome": "read",
  "submission": "not_applicable",
  "data": {"workflow_id": "mail/2026-10-09/17", "revision": "1", "state": "queued", "payload": {"state": "omitted", "size_bytes": "3"}},
  "consistency": {"mode": "strong", "position": {"group_id": "g1", "recovery_id": "r1", "index": "42"}, "snapshot_scope": "per_group"}
}
```

## Batches and atomicity

Require explicit mode: `independent` or `atomic_group`. Independent batches return caller-ordered per-item applied/rejected/unknown/not_submitted results using the shared submission states. Each item has its own stable request ID/scope/digest; NO durable outer request ID or request_scope is accepted. Optional batch attempt correlation is transport context only. There is no durable batch orchestrator or global outcome lookup. Already processed items are not rolled back if another item fails. Retries reconcile individual outcomes under each original scope. A lost whole response makes each unobserved item potentially unknown; an explicit not_submitted item applies only to that item's current attempt. A pre-dispatch invalid whole envelope submits no items. Authorization/admission changes during independent dispatch can yield mixed per-item outcomes without retroactively revoking committed members.

Atomic-group batches use one outer durable request ID and required request_scope for the ordered whole intent and one Raft-group transaction. Server-authoritative placement verifies every touched workflow/partition and parent/child record belongs to that requested group/recovery; reject cross-group or incompatible recovery before proposing. Validate all items before any mutation, including terminal hooks/history/outcomes. Domain rejection records whole-group rejection; timeout means whole-group outcome unknown, never a fabricated per-item committed prefix. Reply identifies scope and one applied position. Atomicity does not span an HTTP connection, separate calls or multiple groups. Bounds include item count, decoded bytes, touched records and application work, not just encoded request size.

## Claim waiting, deadlines, cancellation and drain

`workflow.claim_due` may include an attempt-only bounded `attempt.wait_ms`, capped by remaining attempt budget and advertised wait capability. HTTP/TCP advertise the same supported modes at a rollout; a disabled wait capability rejects before proposal. Notifications/timers are eligibility hints; each actual claim is committed and selected from authoritative lane-head indexes. Before parking a replay, consult retained outcome/deduplication through the application so an already-completed ID cannot wait for another lease. Register/check/wake/recheck must close missed-wakeup races. Waiter budgets are shared by TCP/HTTP and bounded by tenant, worker, node and key footprint; cap broad scopes atomically. Do not scan every waiter or every FIFO follower on the hot path. Hints may coalesce but may not strand ready work or cross tenant/scope.

Only one actual claim proposal completes one request ID. If it commits empty after a contention race, that empty outcome is final for that ID; continued polling uses a fresh claim identity. While waiting with no proposal, disconnect/cancel removes local waiter/timers. A waiting deadline can return a provably not_submitted/not_submitted empty wait result; it is not a durable claim outcome and says nothing about another concurrent attempt. If submission races cancellation/timeout, return unknown and reconcile SAME identity. Lease expiry is a committed domain-time rule, not the HTTP timeout or local wait clock. Claim reply includes lease token, fencing/recovery identity, durable deadline, running and run_state. Worker capacity includes submitted-but-unknown claims. A replayed retained claim is historical evidence, not current ownership: before resuming it, require a strong check of current worker/lease/fence/recovery identity and expiration or equivalent current-authority evidence. A check cannot prevent a later cancellation race; state guards remain authoritative for subsequent commands and external receivers still need effect idempotency/fencing. Never handle an already-expired/superseded replayed claim merely because its original applied reply exists.

Adapter parsing/header/body watchdogs bound slow clients. The local monotonic attempt deadline starts on bounded header acceptance and includes body/authentication/admission/routing/apply/persistence/delivery; each watchdog is further bounded by remaining time. A timeout decoded from the body applies to that original start, not decode completion; already-expired requests cannot propose. Partial headers have an assembly watchdog before header acceptance; handshake has a separate watchdog. Dropping a handler future, HTTP/2 reset or connection close cannot revoke a committed/potentially committed command. Workflow cancellation is an explicit authorized workflow.cancel command with its own ID/guards. Shutdown refuses new ordinary/claim admission and cancels unsubmitted claim waiters, but bounded authorized maintenance/reconciliation for existing ownership may use the shared reserve until full stop. Let submitted commands/replies drain within configured bounds and preserve unknown outcomes for reconciliation. Worker shutdown stops polling first, drains/heartbeats owned work, and applies a selected domain release/retry policy only with valid ownership; disconnect itself does not release. Exact lifecycle phases are owned by the shared conformance design.

## Routing and retries

Prefer one trusted HTTPS cluster origin with server-side application routing. Redirects are optional, never a correctness assumption. A provably pre-submission leader referral can use 307 with a validated HTTPS Location and typed not_submitted/not_submitted routing body; 308 is unsuitable for changing leadership and 301/302/303 must not rewrite command POST intent. Method preservation for 307 is specified in [RFC9110 §15.4.8](https://www.rfc-editor.org/rfc/rfc9110.html#section-15.4.8); trusted-target and submission requirements are our policy. A referral after possible submission is unknown, not proof the target should re-execute with a new ID. Automatic blind retry of POST is not established by HTTP method semantics; request-ID replay is the explicit application protocol.

Clients disable automatic redirects and permit only configured trusted cluster origins/certificate identities, bounded hops, no TLS downgrade, no credential forwarding to arbitrary hosts and no body replay until redirect policy validates the target. Preserve request ID AND request_scope, normalized digest and exact semantic command across a TCP-to-HTTP or HTTP-to-TCP retry. Route hints carry no authority; placement/group/recovery compatibility is rechecked without rewriting scope. Reads and explicitly not-submitted attempts can retry with bounded jitter; potentially submitted commands reconcile/replay SAME identity within retention. Retry-After is a scheduling hint, not quorum/commit evidence. Gateway timeout configuration must permit the advertised long-poll duration plus request/reply overhead.

## Effective HTTP resource profile

All numbers are unvalidated proposed initial policy. The [shared operations design](operations-and-conformance.md) owns node/tenant/group resources; the [parent common contract](../transport-design.md) owns the public profile. Effective limits are the minimum of shared/node, authenticated tenant, operation, HTTP edge and selected client receive limits. HTTP and TCP consume the same backend allowance on a node. Local caps do not establish a cluster-wide tenant entitlement.

| HTTP edge or shared resource | Proposed limit / interpretation |
|---|---|
| Header block / field count / request-target | 16 KiB / 64 / 8 KiB; native HTTP/TLS parser and HTTP2 decompression/read-ahead buffers require separate accounting |
| Encoded JSON request and ordinary serialized reply | 1 MiB each, including envelope/base64; decoded inline command also <=1 MiB and all temporary capacity is charged |
| Individual / aggregate decoded opaque values per call | 256 KiB / 512 KiB, matching TCP; smaller serialized envelope/receive limits bind first |
| JSON nesting / total fields and collection elements | 16 / 4,096; bounded structural preflight before allocating typed strings/collections/base64 |
| Active / ordinary queued / unresolved data requests per connection | 16 / 8 / 24; unresolved includes active, queued and parked requests, never 24 additional queued calls |
| Resident ingress / egress ceiling per connection | 4 MiB / 4 MiB, lazily acquired within the shared pools; no preallocation entitlement |
| Fixed error/control allowance | 16 KiB per connection, lazily charged plus separately bounded node control/TLS/header overhead; no promise to write through a blocked socket |
| Parked claim waiters | 128/node, 64/group, 16/tenant, 8/worker, 8/connection, <=16 registered scope keys/waiter; aggregate pending<=24 still applies |
| Parked metadata / wake work | System's 8 MiB slice inside shared request-state pool, <=8 KiB/waiter and <=256 bytes/scope key; <=8 wake tasks/node, <=32 ready waiters/batch, one coalesced token/waiter |
| TLS/authentication / header receipt / absolute body assembly watchdog | 5 s / 5 s / 5 s; body/auth work also bounded by remaining monotonic attempt time |
| Attempt default / maximum | 5 s / 30 s, including receipt through delivery; a caller timeout can reduce this |
| Response progress watchdog / absolute write deadline | min(5 s, remaining attempt); no new 30-second delivery allowance after apply |
| Graceful drain | 30 s proposed node budget; submitted ownership follows the shared durable shutdown contract |

Counts are upper bounds, not promised capacity. For example, a 64 MiB shared response partition cannot reserve 256 simultaneous 1 MiB replies; each phase uses the tighter atomic count+byte+work admission. Ordinary queued work (<=10 ms) owns bounded input/state/queue resources only; fair dispatch atomically acquires active execution/cost/worst-case output before proposal. Parked waiters use their finite independent ledger, also without active/output permits; wake reauthorizes and reacquires full submission admission. Transferred buffers keep one accounting owner, while physically overlapping copies count separately. No unbounded tasks may wait for any phase's permit. The maintenance allowance is a slice of total resources, not another unaccounted pool. Do not pin 1 MiB output for each 30-second waiter.

Reject oversized declared Content-Length before body acquisition; for unknown/chunked lengths acquire byte permits incrementally before buffer growth and stop at the encoded cap. Early rejection closes HTTP1 if unread framing cannot be safely bounded/drained; HTTP2 resets the affected stream under bounded connection resources. Use the maintained HTTP stack's parser and explicit limits, not a custom request-smuggling parser. Malformed/duplicate Content-Length, conflicting transfer framing and invalid authority are reject/close conformance cases. Request Content-Encoding other than identity is initially unsupported (415): enabling decompression later requires post-expansion reservations and a separate bounded profile. No body/extractor limit by itself proves these parser/TLS/aggregate bounds.

Reserve worst-case COMPLETE serialized mutation outcome under minimum(node/per-call/client-receive cap) before proposal, including JSON/base64/history metadata overhead. Indivisible incompatible mutation rejects before submission; no commit-then-truncate result. Claims choose a supported bounded count/hydration mode, retaining exact included source cardinality and metadata. If a source case needs a larger shape, select a reviewed case-specific profile or report incomplete parity; never rewrite its assertion to fit a smaller cap. Queries paginate under an explicit cursor contract and never silently omit asserted data.

HTTP2 streams and HTTP1 request/pipeline order grant no application commit order. HTTP reply framing order is distinct from mutation order. Client causal sequences wait for an outcome and carry guards/positions; ordered atomic batches are explicit. TLS session/ticket-resumption stores and verifier/client-auth policies are isolated between client HTTP, client TCP and Raft/admin identities; a reused store must not bypass a different audience or auth policy. This is an accepted proposed deployment requirement, not a measured TLS integration guarantee.

## Current library evidence and remaining gates

Primary documentation was inspected on 2026-10-09 in a bounded pass. Observed current pages identified Axum0.8.9, Tower0.5.3, rustls0.23.45 and reqwest0.13.5. These are evidence-page versions, not selected dependencies, security clearances or MSRV recommendations. Pin reviewed releases/features/transitives only at the future adapter gate.

| Candidate / primary evidence | Observed documented API behavior | Proposed use and required validation |
|---|---|---|
| [Axum DefaultBodyLimit](https://docs.rs/axum/0.8.9/axum/extract/struct.DefaultBodyLimit.html), [Serve](https://docs.rs/axum/0.8.9/axum/serve/struct.Serve.html) | DefaultBodyLimit is extractor-local; direct body consumption can bypass it. Json/Bytes use the documented 2 MB default. Serve offers with_graceful_shutdown and HTTP1/HTTP2 feature support. | Set an explicit smaller common cap and globally limited body path; map extractor rejections to the typed problem envelope. Verify HTTP/TLS accept integration, header/stream/read-ahead limits and graceful signal behavior. Graceful shutdown alone does not prove submitted commands or buffers drained. |
| [tower-http RequestBodyLimitLayer](https://docs.rs/tower-http/latest/tower_http/limit/struct.RequestBodyLimitLayer.html), linked from Axum documentation | Layer configures a request-body cap across body consumers; this complements extractor-local limits. | Candidate effect-only dependency; validate feature/release and errors. It does not reserve decoded/base64/aggregate memory or limit native header/TLS buffers. |
| [Tower ServiceBuilder](https://docs.rs/tower/0.5.3/tower/struct.ServiceBuilder.html) | Layer order changes buffer/concurrency behavior; buffer-before-limit can admit queued work in addition to inner concurrency. Concurrency counts response-future lifetime; load_shed rejects lack of readiness and timeout bounds a future's response wait. | Explicit reviewed middleware order and bounded/no buffer for mutations. A Tower permit cannot stand in for shared submission-lifetime ownership after timeout/disconnect; supervised application operation owns that permit. Convert layer errors with known submission state, never generic timeout => rejected. |
| [rustls overview](https://docs.rs/rustls/0.23.45/rustls/), [protocol features](https://docs.rs/rustls/0.23.45/rustls/manual/_04_features/index.html) | TLS1.2/TLS1.3 and ALPN are documented; crypto provider is explicit/customizable, with aws-lc-rs default and ring alternative features. | Select TLS1.3 production config, verified identity, no early data and separate auth/resumption policies. Audit selected provider/target/transitive code; a safe public API does not imply all dependencies contain no unsafe. HTTP TLS listener wiring is a separate spike. |
| [reqwest ClientBuilder](https://docs.rs/reqwest/0.13.5/reqwest/struct.ClientBuilder.html) | Default redirect policy follows up to ten; total timeout defaults to none and includes body completion; read_timeout resets on each successful read. Builder exposes redirect, retry, https_only, tls_version_min and tls_backend_rustls. | Reuse a bounded client/pool; disable automatic redirects, configure total/connect/read and response-byte budgets, verify cert/name, select rustls explicitly and review built-in retry policy before mutation use. Application replay must preserve original ID/digest within retention. No client timeout proves non-submission. |

These facts support using existing safe APIs; they do not prove bounded RSS, p99, durable Raft semantics or safe automatic workflow retries. No dependency/tool was installed. No custom HTTP/TLS/serialization parser or first-party unsafe optimization is justified by this design.

## Required future HTTP conformance

Preserve every included original workflow assertion and source identity through the selected compatibility surface. Adapter additions supplement them: split/trickled headers/bodies, base64/u64 extremes and absent-vs-zero, duplicate/unknown fields, malformed framing/authority, encoded/decoded/structural boundaries, shared HTTP+TCP quotas, slow HTTP2 streams/readers, partial response/stream reset at acknowledged submission/apply hooks, parked-wait expiry/wakeup/cancellation, current auth on replay, routing credential leakage/downgrade, cursor lag/restore incompatibility, and drain maintenance. Verify one final response per target operation; a transport cancellation receipt is not a durable outcome. Byte permits must follow retained capacity and submitted task ownership until completion/explicit failed-state transfer. Cross-transport digest/replay/conflict, replayed-claim stale ownership and required request_scope cases are mandatory: missing/mismatched scope rejects, cross-group atomic rejects without prefix, old-scope retry after disaster restore cannot enter the new namespace, forwarding/SDK journal preserves original scope, queue/wake revalidation prevents scope transfer, and no-ID legacy cases remain untouched.

All conformance, architecture, source collection/porting and load/fault tests are future gates. No tests were implemented or run. The small-healthy-workload latency target in the system design is an unvalidated goal, not an HTTP guarantee; report offered/rejected/unknown traffic and client scheduling delay alongside successful p99. Raft/disk stalls, failover and external handler time have separate measured envelopes.

## Review coordination

Direct `send_input` is not exposed in the inspected tool inventory. Communication uses [the owned review file](../reviews/transport-http-review.md) and parent relay. Both peer designs, both peer review files, current parent transport contract, architecture and ADR were actually read. The review file records individual responses/resolutions and remaining gates; direct messaging or peer endorsement is never inferred.
