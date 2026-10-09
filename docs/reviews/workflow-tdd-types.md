# Workflow TDD types implementation and review

Owner: types agent. Write scope is `crates/wf-types/**` and this note only.
Reference is the complete read-only Elixir checkout at
`66586c596e271408d9184af32eb31cd7218c126e`. No source, frozen reference,
mapping, shared configuration, or peer writes are authorized here.

## Published contract for the domain peer

The coordination signatures are accepted and will be implemented as follows:

```rust
wf_types::identity::RecordId::new(Vec<u8>) -> Result<RecordId, wf_types::error::ValueError>
RecordId::as_bytes(&self) -> &[u8]
wf_types::version::TimestampMillis::new(u64) -> TimestampMillis
TimestampMillis::get(self) -> u64
TimestampMillis::checked_add(self, u64) -> Option<TimestampMillis>
wf_types::version::Fence::new(u64) -> Fence
Fence::get(self) -> u64
Fence::checked_next(self) -> Option<Fence>
```

`RecordId` is cloneable, comparable, ordered, and hashable, with private bytes;
it accepts every nonempty binary through 65,535 bytes. This is the source
**storage identity guard**, not the composed API state-key admission limit.
`TimestampMillis` and `Fence` are copyable, comparable, ordered, and hashable;
both retain zero and `u64::MAX`. Checked arithmetic returns `None` on overflow
and never consults a clock. The arithmetic result type is explicitly published
now to avoid a dependent implementation guessing it.

Additional published signatures are:

```rust
wf_types::identity::PartitionKey::encode_components(&[&[u8]]) -> Result<PartitionKey, wf_types::error::ValueError>
PartitionKey::as_bytes(&self) -> &[u8]
wf_types::bounds::validate_api_key_size(&[u8]) -> Result<(), wf_types::error::ValueError>
wf_types::bounds::checked_api_key_size(&[usize]) -> Result<usize, wf_types::error::ValueError>
```

The key-size helpers validate **already encoded** API keys or the lengths of
all their encoded parts. They do not construct source CRC32/SHA-256 routing
tags, sealed scope bindings, or the state key. Both source storage and API
ceilings are 65,535 bytes, but their scope differs; adding a key prefix can
reject an otherwise valid `RecordId`. Source key-size validation permits zero
bytes; record identity does not.

The source `PartitionKey.encode_components/1` accepts a nonempty component list,
including empty and arbitrary binary components, and emits `fpk:` followed by
decimal byte length, `:`, and the unchanged bytes for each component. No UTF-8
validation or delimiter prohibition will be added. A checked Rust constructor
will reject an empty component list; exact byte vectors are acceptance inputs.

## Current evidence and next work

Read `AGENTS.md`, `README.md`, `docs/architecture.md`, the coordination note,
and `docs/workflow-test-parity.md`. Verified the source checkout HEAD and clean
status. Read the full partition-key implementation and its three source tests,
and the record-identity implementation/tests. At publication time no behavior
code or tests have been written; failing-test evidence will be recorded before
the implementation.

## Red evidence recorded before implementation

Added 19 integration tests first in `tests/identity_and_bounds.rs` (7),
`tests/partition_key.rs` (8), and `tests/version.rs` (4). Runtime modules are
still the original empty M0 scaffold at this point.

Executed:

```text
cargo +1.99.0 test --locked --offline -p wf-types --target-dir /private/tmp/ferricstore2-wf-types-tdd-target
```

First attempt exited 101 and also exposed missing slice coercions in tests
whose target function did not exist; those test-only coercions were corrected.
The second attempt exited **101**, with `E0432` for absent `RecordId`,
`PartitionKey`, `ValueError`, `TimestampMillis`, `Fence`, and key-size helpers.
Each of the three integration-test binaries failed to compile. The clean red
log is `/private/tmp/ferricstore2-wf-types-red-2.log`; initial diagnostics are
in `/private/tmp/ferricstore2-wf-types-red-1.log`. This is an observed
**compile-red** against absent APIs, not a claim that assertions ran and failed.
Production implementation starts only after this note has been saved.

## Source assertion links

Pinned source path prefix below is
`/Users/yoavgea/repos/ferricstore2-elixir-baseline/`.

| Source identity / assertion | Rust counterpart | Preservation |
|---|---|---|
| `Ferricstore.Flow.PartitionKeyTest`, `test encodes components with byte lengths`; declaration `f05f226483bfbc13`, [source line 6](/Users/yoavgea/repos/ferricstore2-elixir-baseline/apps/ferricstore/test/ferricstore/flow/partition_key_test.exs:6) | `partition_key::encodes_components_with_byte_lengths` | Exact `fpk:8:tenant-a9:invoice-1` bytes |
| `Ferricstore.Flow.PartitionKeyTest`, `test delimiter bytes cannot make different component lists collide`; declaration `3221412a3842b3c5`, [source line 11](/Users/yoavgea/repos/ferricstore2-elixir-baseline/apps/ferricstore/test/ferricstore/flow/partition_key_test.exs:11) | `partition_key::delimiter_bytes_cannot_make_different_component_lists_collide` | Original two component lists remain unequal |
| `Ferricstore.Flow.PartitionKeyTest`, `test uses byte sizes for arbitrary binary components`; declaration `4ba84b7b957de543`, [source line 16](/Users/yoavgea/repos/ferricstore2-elixir-baseline/apps/ferricstore/test/ferricstore/flow/partition_key_test.exs:16) | `partition_key::uses_byte_sizes_for_arbitrary_binary_components` | Exact prefix and `[0, 255]` bytes |

These are all three literal cases in the partition-key source suite, whose
implementation is [partition_key.ex](/Users/yoavgea/repos/ferricstore2-elixir-baseline/apps/ferricstore/lib/ferricstore/flow/partition_key.ex:6).
New properties supplement them: all single-byte values, empty components,
multibyte text lengths, decimal boundaries through 65,536 (encoding has no
storage limit), and injectivity of all 30,783 lists of 1–3 components drawn
from the 31 binary strings of length 0–2 over `[0, 255, ':', '0', '1']`.

The storage identity bound comes from
[record_identity.ex](/Users/yoavgea/repos/ferricstore2-elixir-baseline/apps/ferricstore/lib/ferricstore/flow/record_identity.ex:6),
which returns a boolean rather than an oversized-ID error. Empty-ID error text
comes from [Flow.validate_id](/Users/yoavgea/repos/ferricstore2-elixir-baseline/apps/ferricstore/lib/ferricstore/flow.ex:1764).
Composed-key admission and error text come from
[Flow.validate_key_size](/Users/yoavgea/repos/ferricstore2-elixir-baseline/apps/ferricstore/lib/ferricstore/flow.ex:1767),
[get_bound_record](/Users/yoavgea/repos/ferricstore2-elixir-baseline/apps/ferricstore/lib/ferricstore/flow.ex:541),
and [Router.max_key_size](/Users/yoavgea/repos/ferricstore2-elixir-baseline/apps/ferricstore/lib/ferricstore/store/router/part_05.ex:223).
The Rust storage-ID, empty-component-list, length-overflow, and allocation
errors are typed local adaptations, not invented legacy error messages.

Explicit time and initial fence values come from
[flow_create_record_with_lifecycle](/Users/yoavgea/repos/ferricstore2-elixir-baseline/apps/ferricstore/lib/ferricstore/raft/state_machine/sections/flow_create.ex:259):
committed `now_ms` populates created/updated fields, initial fencing is zero,
and an absent lease has deadline zero. Claim writes explicit time/deadline and
its new fencing value in
[flow_claim_next_record](/Users/yoavgea/repos/ferricstore2-elixir-baseline/apps/ferricstore/lib/ferricstore/raft/state_machine/sections/flow_retention_values.ex:545).
Public timestamp/duration/deadline admission is capped at 9,007,199,254,740,991
by [ClaimDueAPI](/Users/yoavgea/repos/ferricstore2-elixir-baseline/apps/ferricstore/lib/ferricstore/flow/claim_due_api.ex:764)
and [Options](/Users/yoavgea/repos/ferricstore2-elixir-baseline/apps/ferricstore/lib/ferricstore/flow/options.ex:47);
that per-command cap is distinct from the agreed full-`u64` primitives.
The zero tests are source-derived; full-`u64` and overflow tests are additional
Rust representation/safety coverage, not a claim that Elixir API admission
accepts `u64::MAX`.

`RecordIdentityTest` source identities `9589f7a84bac8e16` (accepts unscoped
and sealed shared state ownership) and `05cd1ba469691b9f` (rejects foreign
sealed scope and malformed identity without raising) were read. They remain
**unported**: primitive bounds alone do not implement state-key ownership or
sealed storage scope. Neither is claimed as passing by this increment.

## Green evidence and available APIs

After the red note was saved, implemented only the five owned runtime modules.
The same pinned Cargo command exited **0**: **19 passed, 0 failed, 0 ignored**
(7 identity/bounds, 8 partition encoding, 4 explicit value tests). Output is
`/private/tmp/ferricstore2-wf-types-green-1.log`. No doctests are counted.
The source three-case `Ferricstore.Flow.PartitionKeyTest` suite was also
executed directly with in-memory `Code.require_file` and `ExUnit.start` from
the pinned full checkout: **3 tests, 0 failures**, seed 454383. It needs no
application/dependency startup and wrote no source files.

`cargo +1.99.0 fmt -p wf-types` and pinned `clippy --locked --offline -p
wf-types --all-targets -- -D warnings` passed, with the separate target directory
above. Clippy output is `/private/tmp/ferricstore2-wf-types-clippy-1.log`.
Initial workspace integration checks waited for shared-code availability.
The three source counterpart names/paths above are now available
for the parity peer's actual assertion-link ledger. Each lives at the **root**
of `crates/wf-types/tests/partition_key.rs`, so the ledger's `rust_module` is
`""`; the table's `partition_key::` prefix names the integration-test binary,
not an inline Rust module. Warnings-denied docs also passed with
`RUSTDOCFLAGS='-D warnings' cargo +1.99.0 doc --locked --offline -p wf-types
--no-deps` and the separate target dir; output is
`/private/tmp/ferricstore2-wf-types-doc-1.log`.

## Peer review progress

Read the domain and parity notes in full on their first publication, and read
the domain's published structures, rejecting reducer skeleton, and first two
source-derived tests. Domain correctly distinguishes storage from reducer
effects, last-event legacy guards from explicit request-ID replay, and
`Explicit` from `PolicyResolved` retry times. No completed domain behavior was
available at this review point, so this is API/source review, not acceptance.

Read the parity schema/TDD notes, including its actual ten-test behavioral-red
report. Registration and assertion mappings remain distinct from execution;
the full gate remains closed. Production progress-validator code is not yet
available for completed behavior review. Both implementations and updated
notes will be read again before the final report; concrete findings will be
recorded here for their owners, without peer edits.

### Concrete findings for owner resolution / coordinator relay

**T-P1 — progress parser counts a disabled crate test (resolved by owner).** Read the actual
`progress.rs`, `registration.rs`, `rust_source.rs`, `rust_tokens.rs`, CLI and
adversarial tests after the initial green report. In `rust_source::scan_items`,
only outer `#[...]` attributes are read; inner `#![cfg(any())]` is skipped and
the test is still considered active. Built the actual parity binary with
`cargo +1.99.0 build --locked --offline -p wf-parity-tests --target-dir
/private/tmp/ferricstore2-wf-types-review-target` and exercised a temporary
fixture root, never a shared reference/mapping/peer file. Both the active
control and this disabled source exited **0** and counted **1 source-derived
declaration, 1 Rust counterpart, 1 assertion**:

```rust
#![cfg(any())]
#[test]
fn encoded_bytes() {
    assert_eq!(1, 1);
}
```

Evidence: `/private/tmp/ferricstore2-wf-types-parity-cfg-review.log`, fixture
`/private/tmp/wf-types-parity-cfg-review-2cx8zlsb`. This does not bypass the full
parity gate (which correctly remains incomplete), but contradicts the progress
contract that ignored/unsupported conditional tests cannot count. Owner:
handle crate/module inner conditional attributes conservatively and add an
active-control/disabled-source regression before the fix. Other checked
properties are sound in the inspected paths: exact all-source ID accounting,
content hashes, duplicate scope detection, bidirectional runtime accounting,
unknown status rejection, and unconditional incomplete full-parity reporting.

Resolution: read the owner's actual acceptance and new pre-fix failing
regression in the parity note (`workflow-parity-inner-cfg-red.log`). Read its
updated inner-attribute handling for crate and inline module scopes. Independent
pinned `test -p wf-parity-tests --lib` passed **20 tests**, including the inner
cfg regression; log is `/private/tmp/ferricstore2-wf-types-parity-parser-review.log`.
Rebuilt the CLI and reran the exact original disabled temporary fixture:
it now exits **1** with `scoped Rust test absent, ignored, conditionally
unsupported or not #[test]: ::encoded_bytes`. Evidence:
`/private/tmp/ferricstore2-wf-types-parity-cfg-resolved.log`. This finding is
closed with actual independent code/read/run evidence.

**T-D1 — represented fencing guards omit the source numeric limit (open).**
Read the implemented create/claim/transition/terminal/retry paths, their tests,
and pinned source helpers. Rust `require_fence` compares the full `u64` without
admitting the caller's guard first; `DomainError` currently has no oversized
fence rejection. A running record with a valid lease token and a guard of
`Fence::new(u64::MAX)` therefore gets `ERR stale flow lease`. Source
[MutationAttrs.transition_attrs](/Users/yoavgea/repos/ferricstore2-elixir-baseline/apps/ferricstore/lib/ferricstore/flow/mutation_attrs.ex:362)
and terminal/retry parsers use
[Options.required_non_neg_integer](/Users/yoavgea/repos/ferricstore2-elixir-baseline/apps/ferricstore/lib/ferricstore/flow/options.ex:47),
which rejects above 9,007,199,254,740,991 with exact text
`ERR flow fencing_token exceeds maximum 9007199254740991` before storage access.
The broad primitive `Fence` must still preserve `u64`, as agreed; validation
belongs to the represented command guard. Owner: add source-derived boundary
tests and preserve this rejection for transition/terminal/retry/extension/
cancellation guards, including duplicate calls. Cancellation's represented
optional expected-version guard should receive the same source-cap review.
This finding is static source/code review; no peer test was edited.

Verified positive domain properties against the actual source: create starts
version 1/fence 0/attempts 0; claim does not increment attempts and preserves
logical run state; transition/terminal duplicate checks intentionally ignore
different payload/result bytes and the old token when same-fence effect guards
match; retry has both schedule and latest-event shortcuts. The source history
event-ID helper uses `max(now, previous.updated_at)` independently of its
event's original `now`, matching the reducer. Broader monotonic history or
deadline-token revocation must not be invented from a test title.

Both peer owners have now independently reviewed the types code. Parity read
all modules/tests and found no defect; domain read all modules/tests, compared
the source bounds, and independently ran all **19 tests passing**. Actual
updated peer notes were read, including domain's accepted primitive findings
and its independent `stringify!` assertion-parser finding for parity. There is
no outstanding incoming finding against types.

### Integration checks while peers are still implementing

Warnings-denied **workspace docs passed** (all 15 crates), exit 0, saved to
`/private/tmp/ferricstore2-wf-types-workspace-doc-1.log`. First workspace Clippy
run exited 101 on domain `nonminimal_bool` and four `assigning_clones` findings
in active implementation; no types failure. Log:
`/private/tmp/ferricstore2-wf-types-workspace-clippy-1.log`. Owner resolution is
pending; these are integration diagnostics, not a claim that peer code is final.

The architecture suite ran 19 passing infrastructure cases and then its
workspace policy case failed (exit 101). Log:
`/private/tmp/ferricstore2-wf-types-architecture-1.log`. Own failures were
unapproved `write!` expressions in `ValueError::Display` and missing explicit
`#![forbid(unsafe_code)]` in the three integration-test targets. Added the
explicit target guards; next, use the already approved pure `format!` macro
instead of requiring a policy exemption for `write!`. No lint suppression or
root-policy edit is needed.

**T-D2 / coordinator integration:** architecture also reports the new domain
`error` module is unclassified/root modules differ, and the three domain
integration-test targets need explicit unsafe forbids. Domain/coordinator must
resolve those within their ownership (either use existing classified modules
or a coordinator-reviewed policy change); this agent will not edit them.

Read the parity peer's completed independent review of all owned types modules
and tests: it found no correctness defect and explicitly confirmed the separate
storage/API bounds, raw binary vectors, and overflow coverage. The later domain
peer review likewise found no primitive blocker. T-P1 is closed above; T-D1
and domain/coordinator integration corrections remain with their owners.

After the owned architecture corrections, the 19 types integration tests and
all-targets warnings-denied types Clippy passed again; outputs are
`/private/tmp/ferricstore2-wf-types-green-2.log` and
`/private/tmp/ferricstore2-wf-types-clippy-2.log`. The repeated workspace policy
case reports **no types violation** and only the five existing domain
classification/target-guard issues; it still exits 101, rather than silently
passing an incomplete shared gate. Evidence:
`/private/tmp/ferricstore2-wf-types-architecture-2.log`.

Final domain behavior review: read the completed transition/terminal/retry/
extension/cancel/reclaim code, six fencing counterparts and their test-only
scenario helper, and lease/guard regression assertions. Independently ran
`cargo +1.99.0 test --locked --offline -p wf-domain --target-dir
/private/tmp/ferricstore2-wf-types-review-target`: **23 passed, 0 failed,
0 ignored** (3 checked-counter unit tests, 2 create/claim, 6 source fencing,
12 lease/guard tests); output is
`/private/tmp/ferricstore2-wf-types-domain-review.log`. Passing this subset does
not close T-D1's missing oversized-fence rejection or establish persistence/
policy/FIFO parity. The cancellation expected-version cap is now present;
represented fences still need the source-admission cap.

After the domain's implementation/lint changes and parity's parser hardening,
repeated the broad required checks. **Workspace Clippy on all targets passed**
with `--locked --offline -- -D warnings`, exit 0, and **warnings-denied
workspace docs passed again**, exit 0 (all 15 crates). Evidence:
`/private/tmp/ferricstore2-wf-types-workspace-clippy-2.log` and
`/private/tmp/ferricstore2-wf-types-workspace-doc-2.log`. The architecture
classification/unsafe-target findings above remain separately open; a passing
Clippy/docs run is not reported as a passing architecture gate.

## Owned deliverables and handoff

- Runtime: `crates/wf-types/src/{identity,bounds,version,error,lib}.rs`.
- Tests: `crates/wf-types/tests/{identity_and_bounds,partition_key,version}.rs`.
- Notes: this file, including signatures published before dependent behavior,
  preserved source identities, observed compile-red before implementation,
  19-test green, source execution, integration corrections and both reviews.

No dependencies, manifests, locks, shared policy/configuration, peer files,
frozen snapshots, or existing mapping were changed by this owner. No commits,
pushes, or subagents. Original dirty Elixir checkout was untouched; the pinned
source remains clean at the specified commit after direct source-test execution.
Coordinator should relay/close T-D1 and the shared domain architecture gate;
types implementation and incoming review obligations are complete.

## Explicit omissions

No state-key construction, CRC32 auto routing, SHA-256 partition tags, sealed
scope/ownership validation, admission adapter, persistent encoding/decoding,
clock progression policy, distributed fence namespace, or workflow operations.
The checked API-key helpers require every already encoded part from their
caller; they do not make `RecordId` alone sufficient for API admission.
Elixir arbitrary-precision integers beyond `u64` are not represented by these
agreed Rust primitives; overflow is rejected explicitly. Empty component-list
rejection adapts the source's absent function clause to a typed Rust error.
Encoding imposes no extra 65,535-byte partition-component restriction.
The Rust construction suite started compile-red, not behavioral-red; this
distinction remains explicit. Full workflow parity is incomplete.
