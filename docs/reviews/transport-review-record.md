# HTTP/TCP design round and reciprocal review

Status: completed design review and coordinator integration, 2026-10-09. No implementation or measured performance. Exactly three subagents were explicitly spawned with `model: gpt-6.1-sol` and `reasoning_effort: max`. A server interruption before any transport document was written required resuming the same three agent IDs; no replacement reviewer was added.

| Agent | Owned design | Owned review |
|---|---|---|
| Heisenberg / TCP / 01a12041-64ae-7ca3-ba21-ac06c0104a83 | [TCP](../transports/tcp.md) | [TCP findings and replies](transport-tcp-review.md) |
| Huygens / HTTP / 01a12041-6519-7c42-8bb8-462619dea045 | [HTTP](../transports/http.md) | [HTTP findings and replies](transport-http-review.md) |
| Dirac / operations / 01a12041-6585-74c1-af88-33ccf87c7a3b | [Operations/conformance](../transports/operations-and-conformance.md) | [System findings and replies](transport-system-review.md) |

Coordinator owns the [shared contract](../transport-design.md), [typed intent grammar](../transports/intent-digest.md), [ADR-001](../adr/001-dual-client-transports.md) and integration into architecture, libraries, quality allowlist and implementation plan. Agent write sets were disjoint. Each agent actually read both peer drafts and review files, answered received findings, revised its own design, and reread revised peer text. Their final records report no unresolved protocol disagreement in this round.

Direct agent messaging tools were unavailable inside the agent sessions. Actual communication used shared repository files and coordinator relays through send_input. The records distinguish a written answer, an actual later read, and explicit acceptance; they do not invent direct messages or endorsement of paragraphs written after the last read. All three agents finished and were closed after delivering their files.

## Reciprocal exchanges actually recorded

| Review direction | Finding IDs and actual resolution evidence |
|---|---|
| HTTP → TCP | H-T1..H-T6: TCP's answer table and final H-T6 resolution; HTTP's final section accepts revised schema, limits, lanes, claim replay and lookup spelling |
| HTTP → operations | H-S1..H-S5: system's answer table; HTTP's final section accepts count/byte minima, finite waiter/wake ledger, queue ownership and drain/retention |
| TCP → HTTP | TCP-H1..H5: HTTP's answer table; TCP's final table accepts schema, recovery lookup parameters, expiry evidence, attempt-scoped non-submission and claim finality |
| TCP → operations | TCP-S1..S4: system's answer table; TCP's final table accepts effective limits, independent waiter ledger, lazy memory ownership and maintenance drain |
| Operations → HTTP | S1..S9: HTTP's answer table; system's final reread table accepts revised schema, outcomes, scope, limits, TLS, source parity and no ordering overclaim |
| Operations → TCP | S1..S9: TCP's answer table; system's final reread table accepts corresponding revisions and exactly one target reply with separate cancel ACK |

These are textual design reviews. Library API inspection, bounded pinned-source observations and document checks are separately attributed in each owned design; no runtime verification is implied.

## Selected decisions and integration checks

| Finding / decision | Selected design resolution |
|---|---|
| Architecture / H-P1 / TCP-P1..P3 | Pure wf-wire plus sibling wf-api-http/wf-api-tcp, one shared application auth/admission/submission service. No raw ports/storage/consensus capability in adapters. Worker imports wire only in private client modules; architecture and quality allowlist updated. |
| Schema / H-T1 / TCP-H1 / S1..S3 | Exact schema_version/operation/tenant_id/args/attempt/consistency vocabulary, required URL/body tenant equality, lossless public u64s and opaque bytes, one outcome/submission taxonomy. No unsupported aliases or client-supplied digest. |
| Recovery gap / TCP-D1 / H-N1 / S-D4 | Original request_scope:{group_id,recovery_id} discovered and journaled before first mutation, required for individual/atomic scope and each independent item. Check again before handoff after waits/forwarding. Reject stale namespace without auto-rebinding; submitted work stays in its original namespace. All three final records explicitly accept this resolution. |
| Parent integration / S-A1 | Coordinator corrected architecture's durable dedup key to tenant + group + recovery identity + request identity. System's targeted final reread explicitly confirmed closure. |
| Identity / batching / polls | Application-owned bounded SHA-256 typed intent v1; operation-specific defaults/float semantics still gates. Independent batch has per-item IDs/scopes and no durable outer orchestrator; atomic_group has one whole-intent ID/scope. One logical retained claim outcome includes empty results; duplicate Raft entries replay it rather than selecting new leases/history. No at-most-once-log-entry promise. |
| Lookup / H-T6 / TCP-H2..H4 | read/not_applicable with explicit incompatible_recovery, expired only with bounded evidence, missing not a no-commit certificate. unknown uses outcome_unknown and original reconciliation scope. Non-submission proves only this attempt's state. |
| Resource ownership / S4..S5 | Count, bytes and cost all bind; session limits are lazy ceilings. Ordinary queue owns input/state only; dispatch acquires execution/output atomically. Parked waiters have finite count/key/byte/wake ledgers. Complete serialized reply fits receiver before proposal; no commit-then-truncate. |
| Cancellation / ordering / drain / S6 | Submitted work remains supervised after timeout/disconnect. TCP ordered lanes are local; unknown retires the lane for that connection, with no RESUME. One target response and separate cancel ACK. Shared drain stops ordinary/claim ingress while bounded current-owner maintenance/reconcile remains until full stop. |
| TLS / S7..S8 | HTTPS h2/http1.1 and FWTP/1 over separate TLS1.3 listeners, no early data/downgrade, distinct Raft/admin audiences/verifiers/resumption. Shared limits protect one node; distributed quotas need separate design. |
| Source parity / S9 | Original reference files and parity contract preserved. Separate legacy no-ID guards/history, exact public values, deep FIFO head selection and acknowledged real faults remain mandatory. New protocol byte compatibility needs case-level disposition; no suite is silently excluded by filename. |
| Latency | Bound admitted work, queues, parsing, writes and background competition; benchmark open arrival schedules across both transports and record rejected/unknown traffic and durable-resolution latency. Healthy small-workload p99≤50ms is an unvalidated planning target, not a successful-write guarantee during stalls. |

ADR-001 is accepted as a design decision. The coordinator read the final three review records and checked shared contract, digest, architecture, library table, quality allowlist and plan for these resolutions. Parent status/integration wording does not imply agents read this newly completed coordinator record.

## Verification and work still required

Document validation: local Markdown links resolve; 13 JSON examples parse; the illustrative TCP header is 24 bytes and its declared/actual body is 177 bytes; whitespace checks pass. Changes are Markdown only. Preserved source tests/manifest/parity files have no changes, and no Rust source, Cargo workspace or executable CI was introduced. These checks are not workflow test execution.

Before implementation/release, complete exact operation/source assertion mappings, closed field/error registries, numeric/partition/default semantics, independent canonical byte/hash vectors, retention/expiry proof/version rollout, safe bounded codecs/TLS/physical memory accounting, original-scope recovery fencing and selected library/provider/MSRV audits. Port and run the Elixir acceptance scenarios, real cluster/fault tests and workload/SLO measurements. Cargo graph, visibility/negative architecture fixtures, Clippy/rustfmt/dependency checks and first-party unsafe prohibition remain required future enforcement.

The basic protocol/architecture disagreements identified here are resolved; these validation gates remain open. No claim of passing Rust parity, installed linters, benchmark superiority or a guaranteed p99 follows from this review.
