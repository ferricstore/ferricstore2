# ADR-009: explicit stored workflow value ingress

Status: selected private codec predecessor. Durable publication and recovery are
required following gates; they are not implemented by declaring this format.

The binary command's intent8 interprets supplied payload/result/error bytes as
logical binaries. Publication escapes a leading `FSV2` once. Inferring a typed
value from those bytes would silently change its meaning. The reviewed source
also distinguishes a corrupt storage envelope from malformed typed content,
which history hydration may report as `ERR invalid flow value`.

Intent9 explicitly means that every supplied value contains complete stored
source bytes. It preserves raw binaries, escaped binaries and typed ETF bytes
without decoding, escaping, canonicalizing or constructing a second value owner.
Malformed typed bytes can therefore survive a valid command envelope; this
codec does not grant a successful typed decode or execution capability.
Logical typed APIs require their real preparation adapter before selecting9.

The existing `PreparedWorkflowValueBatch` already owns the operation and source
presence/guard metadata. The new codec reuses that shape and contained modes
1/5/6/7. Its own canonical domain separator and `ClientIntent` version9 bind the
stored bytes and caller presence. Request ID and server preparation context stay
separate from caller identity. Complete frame/footer/EOF and intent validation
remain mandatory. Retagging an8 frame and resealing its footer cannot create9.

Binary8 APIs, frames, identity and resource/error behavior remain unchanged.
Direct decoders reject the other profile. Generic prepared decoding recognizes9
through the same bounded one-operation/metadata ownership path. No additional
payload sidecar, Document clone, Box conversion, dependency or unbounded decoder
is introduced. Stored-byte validation is not a default legacy fallback.

Normal recovery, durable drivers and capability acknowledgement remain closed
to9 until actual atomic publication and recovery are implemented. Following
physical work must select a versioned alias representation, compare current
core values according to each alias's recorded encoding, retain old aliases for
carried values, and publish immutable/alias/current/every-history/core/control/
outcome changes in the same consumed transaction. Fresh opt-in, mixed8/9,
retained replay, corruption, reopen, full snapshot receive/install and Node audit/
replay require executed tests before durable registration. Exact persistent
format numbers are not assigned by this codec decision.

Full common/foreign typed ingress, original HistoryValues observations, actual
history acquisition, shared/named/external availability, authorization, routing,
read barriers and Linux/macOS acceptance remain required. This ADR establishes
neither a public query service nor complete workflow parity.

Root performs the current review solo under the user override. Concrete paths,
TDD stages and validation are recorded in
[the coordination journal](../reviews/workflow-completion-coordination.md).
