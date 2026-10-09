# ADR-001: separate HTTP and TCP adapters over shared workflow services

Status: accepted design decision after three-agent reciprocal review; implementation remains unstarted. Date: 2026-10-09. Design only.

## Problem

The initial `wf-api` proposal covered HTTP only. The user now requires a dedicated TCP workflow API as well. Both must preserve workflow test parity, durable outcomes, tenancy and bounded resource use without duplicating Raft/business logic or giving one tenant two independent admission budgets.

## Decision

Replace `wf-api` with `wf-wire` (pure public contracts), `wf-api-http` and `wf-api-tcp` (outer adapters). Node constructs one shared application authorization/admission/submission service and passes it to both. Worker clients implement the existing WorkflowClientPort; server adapter crates are not SDK dependencies. Keep client listeners, internal Raft traffic and cluster control capabilities separate. New TCP is a workflow protocol; general Redis/KV compatibility is outside scope. Any legacy wire compatibility needs a specific test/adapter decision rather than implicit support.

The shared contract and allowlist are in [transport design](../transport-design.md); actual peer reads, responses and final integration are in the [review record](../reviews/transport-review-record.md). HTTP/TCP normalize to the same client intent and durable request identity. Mutations bind original group/recovery request_scope before first submission and retain it on retry; placement/recovery is checked again before handoff after waits/forwarding. Call correlation and credentials/deadlines do not become dedup keys. Success requires committed durable apply; transport cancellation does not undo submission. Shared budgets include work cost and bytes through their actual lifetimes.

## Consequences and validation

This adds two distinct effect policies plus a pure wire policy, justified by separate protocol/TLS/connection behavior and independent conformance checks. No empty compatibility crate retains the old name. Ports/domain remain transport-free; no adapter imports storage, consensus or raw transaction capabilities. FWTP/1 framing, HTTPS endpoint shape and shared identity/outcome contracts have passed textual design review. Numeric limits, exact operation field/error registries, safe codecs, selected library releases and real conformance/resource/fault evidence still require implementation gates. Update the canonical architecture/quality allowlist and negative fixtures with this ADR.

No implementation, executable tests, Cargo manifest or measured p99 guarantee is introduced.
