# ADR-009: explicit stored workflow value ingress

Status: private codec and explicit physical capability implemented and locally
verified. Stored intent9 publication and recovery registration remain required.

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

The selected physical capability retains FWSS7 and FWSP4. The existing first
u32 schema field now explicitly selects alias profile1 or2; the binding schema
stays1. Existing create_value_database creates profile1. The additive fresh-only
create_stored_value_database creates profile2, with unchanged configured
defaults, origin and table families. There is no implicit upgrade. Catalog
updates retain the selected profile, and snapshot install requires equal alias
profiles and defaults before changing the target.

FWVA1 records a logical binary; its existing escape-aware comparison remains.
FWVA2 records complete stored source bytes and compares them directly with the
current core field. A private bool corresponds exactly to the alias wire
version. Profile1 refuses FWVA2 even with a valid checksum. Identity, canonical
reference, incarnation, complete length/digest and immutable existence checks
remain mandatory before expiry classification. Content decoding is separate;
malformed ETF may have a valid storage envelope. FWVB1 is unchanged, so carrying
a value retains its actual alias and encoding. No prefix inference or additional
payload owner is introduced.

Normal recovery and durable drivers remain closed to intent9 until actual
atomic publication and outcome recovery are implemented. Next publish immutable/
alias/current/every-history/core/control/outcome changes in the same consumed
transaction. Bind stored9 outcomes to the actual profile2 manifest even when
there are no value rows. Fresh opt-in, mixed8/9, retained replay, corruption,
reopen, full snapshot receive/install and Node audit/replay require executed
tests before durable registration. The existing original-policy operation tag9
is independent of the stored-value intent version9.

Full common/foreign typed ingress, original HistoryValues observations, actual
history acquisition, shared/named/external availability, authorization, routing,
read barriers and Linux/macOS acceptance remain required. This ADR establishes
neither a public query service nor complete workflow parity.

Root performs the current review solo under the user override. Concrete paths,
TDD stages and validation are recorded in
[the coordination journal](../reviews/workflow-completion-coordination.md).

The six genuine compiled rejecting cases were saved in `46e53c6` before the
codec body. They now pass, including all four producer modes, independent
ownership/work bounds, exact typed/raw/escaped bytes, corruption, cross-profile
retagging, preparation identity and explicit durable-header refusal. The complete
local run preserves all earlier1,165 cases and adds only these six;1,171 Rust
tests, six documentation tests, both correctness smokes and strict quality gates
pass. These results do not add original scenario/parity or current Linux credit.

The physical predecessor's two alias-codec and four real-store cases were saved
rejecting in b09e499 before the body. All complete codec/corruption/fresh/policy/
binary-carry/reopen/snapshot/cross-profile tails now pass. One previously masked
fixture needed its witness taken from the actually installed policy rather than
the pre-install donor; its complete inverse preserves every assertion. Current
validation passes1,177 Rust cases across104 targets, six documentation tests and
both correctness smokes, retaining all earlier1,171 identities. Formatting,
compilation, strict all-target Clippy, architecture, reference/progress integrity,
documentation and offline dependency checks pass. This establishes physical
capability and binary publication under it, not stored9 publication or full parity.
