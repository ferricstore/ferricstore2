# Workflow progress accounting

`workflow-progress.json` accounts for every frozen declaration ID, separately
from the immutable M0 `workflow-mapping.json`. It records source-derived Rust
assertion links and optionally references actual coordinator-owned ExUnit
registration. It has no pass/status or execution-proof fields. The full
`require-parity` gate remains fail-closed.

Run the progress validator with the pinned compiler and an isolated target:

```sh
cargo +1.99.0 run --locked --offline \
  --target-dir /private/tmp/ferricstore2-wf-parity-agent \
  -p wf-parity-tests -- check-progress
```

Success means structurally valid accounting and verified assertion links.
Missing collection prints `runtime reconciliation incomplete: collection
absent`; partial collection retains every unresolved ID. Neither result means
executed behavioral parity. Missing/malformed artifacts, changed source files,
dropped/duplicate/fabricated IDs, unknown statuses, broken hashes, unregistered
cases, cross-source case IDs and nonexistent/ignored/disabled/wrong-scope Rust
tests fail validation.

The [schema](workflow-progress.schema.json) describes the ledger shape. Rust
validation additionally checks bidirectional sets, portable nonsymlink paths,
content hashes, scoped identities and exact assertion anchors. `rust_module`
is the inline module path **within `rust_file`**, empty for a root integration
test function. The conservative Rust navigator strips comments/literals,
excludes nested named helper assertions and opaque macro token trees, accepts `#[test]` with optional
`cfg(test)`, rejects ignored/unsupported conditional tests, and bounds inline
module recursion to 64. It does not expand macros or compile/execute tests.
Source anchors must begin with `assert`/`refute` inside the frozen textual
declaration region. Human review still checks fixtures and semantic equivalence;
anchors alone cannot establish completeness or actual assertion execution.

The offline generator records reviewed partition, fencing, borrowed retry-policy
and authoritative claim-view observations, plus subsequent reviewed domain
increments: 60 source declarations, 67 counterpart records and 258 assertion links.
Both reviewers accept the supplemental source603 shape-error anchor; it supplies
one error component with its remaining setup/profile/physical obligations explicit.
The separate original ETF generation-boundary counterpart retains MAX, the returned
policy-map bytes and MAX+1 refusal. It preserves every old scenario ID and limitation;
original encoder, migration and full setup/profile/physical acceptance remain separate.
Four new raw-retry counterparts retain the original cap/timing/generated inputs
and caller old-name rejection, including all 1800 per-attempt normalizations.
Their direct assertion links preserve every older counterpart and explicitly
retain jitter, descriptor, setup, profile, public/physical and Linux limitations.
Two new retention counterparts link the original type/state scalar fields and
the actual TTL5000/hot3 rejection cause and body after all86 policy controls pass.
Captured-limit and creation-neighbor controls add no StateMachine, generation,
ETS or inheritance assertion credit. Public/setup/profile/physical and independent
Linux acceptance remain required; every older counterpart stays unchanged.
Two separately scoped counterparts
belong to one FIFO lifecycle declaration; a single state/partition test retains
selected observations from two declarations. It preserves every other
declaration with empty assertion mappings.
After reviewing any counterpart edit, regenerate hashes/line anchors:

```sh
python3 tests/parity/generate-progress.py \
  --collection tests/parity/collection/reconciled-registration.json
```

The generator creates neither collection nor execution proof. Command results
and independent reviews remain in
[the M1 parity review](../../docs/reviews/workflow-m1-parity.md), separate from
the ledger. The [runtime collection guide](../../docs/runtime-case-collection.md)
describes original schema-1 registration and new schema-2 raw/origin accounting:
pinned commit/manifest, `collection_kind=exunit-registration-only`, explicit
blockers, exact unresolved and unmatched complements, and unique cases with
frozen source lineage, module/name, retained tags and scoped SHA-256 identities.
Hash consistency cannot authenticate an edited artifact's origin; only real
collection with its recorded command/inputs can provide registration evidence.

The current reconciled capture accounts for 5,903 raw registered cases, 4,222
exact linked cases, 1,681 unmatched runtime identities and 1,655 unresolved
frozen declarations. These include one documentation example and 1,654
unproved macro environments; zero macro origins are admitted. Earlier literal
recovery was withdrawn after compiled alias injection counterexamples. Source
identity recovery and pending mixed-case scope
do not certify setup, fixtures or required profiles. The original capture is
preserved unchanged.

Borrowed retry-policy and bounded authoritative claim/FIFO-view selection now
exist at the represented pure-domain layer. Durable history, policy installation,
index maintenance, automatic reclaim scheduling, response-loss transport,
storage, consensus, worker/API, query/recovery and wider workflow scope remain
incomplete. Fencing counterparts count emitted reducer events in a deterministic
harness. Each new counterpart states its omitted original observations; partial
assertion links cannot make the complete source scenario pass.
