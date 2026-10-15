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
increments: 70 source declarations, 77 counterpart records and 301 assertion links.
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
At that retention checkpoint, captured-limit and creation-neighbor controls added no StateMachine, generation,
ETS or inheritance assertion credit. Public/setup/profile/physical and independent
Linux acceptance remain required; every older counterpart stays unchanged.
Four whole-flow counterparts link the exact complete governance model,
first-seen string-list order, the600-leaf governance size rejection and queued
keyword max_active_ms1000 rejection. Their direct whole-ETF or reason/body
assertions executed in the100-test policy target; its macOS workspace epoch passed
554 Rust tests and four compile-fail docs. All67 previous records are preserved.
Helper-only enum/bounds controls and the original allowed snapshot map without
states remain unlinked. Original setup/profiles, public/physical operations and
independent Linux validation remain required. That snapshot epoch failed its
final RustSec refresh because GitHub DNS resolution was unavailable. The subsequent
original whole-flow parent checkpoint passed567 Rust tests/four compile-fail docs
and every default wrapper stage, including all four dependency categories.

Six postimplementation tests now expose direct lower-parent generation/value/error
observations for the pinned monotonic, stale-CAS, omission-reset, cross-policy,
target-high-water and captured-limit recipes. All87 policy-request tests and strict
target lint pass. The22 new links preserve every one of the71 earlier counterpart
records and every registration/runtime ID. Actual prior original ETF output feeds
the next current read; expected bytes precede each operation. These are existing
behavior supplements, with no retroactive tests-first RED. Stored omission is
supplemental to source329's linked generation2/empty-states components; public
max_active_ms:nil projection remains unlinked. Public get/all-shard/physical ETS
assertions, source889 and error pattern assignments remain unlinked. All original
setup/runtime profiles and independent Linux acceptance remain required. Six
narrow component counterparts cannot certify six complete original cases.
The complete current macOS wrapper passes573 Rust tests and four compile-fail
documentation tests, including formatting, strict lint, architecture/reference/
progress validation and all four default dependency-audit categories. The same
1027 check inputs remain unchanged throughout that run; Linux remains pending.
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
