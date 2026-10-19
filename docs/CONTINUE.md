# ferricstore2: implementation handoff

Updated: 2026-10-19. The active full-completion goal continues beyond this reviewed
increment. GPT-6.1-sol/max owners communicate with dedicated architecture and
performance review and disjoint writes. See
[the active coordination record](reviews/workflow-completion-coordination.md).

The latest increment implements bounded neutral Blank/Membership bodies and
atomic last-applied membership with the SAME workflow ControlState. Pure ports
preserve complete ordered joint configurations, learners, original owners and
explicit stamped/unstamped metadata. The actual state guard supplies its captured
R/C limits, typed membership reads and consumed Immediate control publication.
Apply/recovery stay borrowed; original Normal FWCP bytes and workflow publication
remain unchanged. Persisted applied membership remains distinct from Raft's
possibly uncommitted effective configuration. See
[ADR-007](adr/007-durable-log-storage.md#applied-membership-and-neutral-control-entries).

FWSS2 atomically initializes required None membership with manifest/control.
Strict noncreating recovery binds the exact metadata inventory, original body SHA,
full identity/stamp and HLC/wall lower bounds. Intact legacy FWSS1 is explicitly
UnsupportedLayout; there is no silent upgrade. A genuine behavioral RED exposed
an impossible FWMV declared length reported as Limit; constant-work actual framing
validation now returns corruption while healthy smaller-profile refusal stays Limit.

All twelve new test identities pass after preimplementation missing-contract RED:
three pure-port, seven storage and two real consensus-function tests. They cover
independent complete wire/value goldens, stamped/unstamped/replay/clamped behavior,
exact/tight actual guard budgets, complete ROWS+META cohorts, corruption, actual
uncached read failure and same-live-guard fencing, delegated sync uncertainty,
reconciliation and four new process kills. The before-sync cases target the first
non-growth commit-path sync, which may precede a later flush; after-return cases
require the complete new cohort. A real stack showed the initial fixture instead
hit a growth sync, correctly Unavailable; that setup correction is not a product
bug. Existing fault modes and every old test identity/oracle remain covered.
These are process-boundary tests, not power-loss, network ACK or Linux evidence.

Architecture and performance reviewed actual tests/source and each other's work.
The initial donor count check now precedes linear scans. A local buffer rename,
a concretely approved borrowed-tuple alias and typed empty-slice assertions resolve
strict diagnostics without waivers or changed owner/parser/oracle behavior; peers
executed complete-file inverses for the style changes. Related modules remain in
private feature directories and all source-size policies pass.

The final native macOS snapshot passes **732 Rust tests across 87 workspace
targets**, plus six compile-fail examples across 15 documentation targets, with
zero failures or ignored tests. Formatting, all-target compilation, strict Clippy,
warnings-denied rustdoc, architecture and reference/progress checks pass. The full
cached dependency audit passes advisories, bans, licenses and sources without
waivers/exclusions; no fresh online advisory refresh is claimed. All 708 selected
source/manifest/policy/style inputs stayed unchanged throughout final verification.

The separate pinned native acceptance executable passes formatting, compilation,
strict Clippy and its complete persistence/fault/process-kill run. All 725 combined
inputs stayed unchanged. Its 17 files, including 15 Rust sources, remain exactly
proof 55bcbd85; no native source changed. Canonical redb 4.4.0/no-default/std-only
and all dependency manifests/lockfiles remain unchanged. No runtime framework,
concrete backend edge in consensus or registry package is introduced.

The executed full parity gate still fails. Ledger counts remain 82 source-derived
declarations, 92 mappings and 365 assertion links; 1,681 runtime cases remain
unmatched and 1,655 declarations unresolved. These component tests add no full
source-case parity credit. Complete pinned Elixir setup, fixtures, mandatory
profiles and runtime scenarios remain required, as does native Linux validation.

Next: verify historical applied membership against the actual durable log and
replay only the ordered committed suffix through real Blank/Membership/Normal
apply paths before returning usable owners. Preserve the existing checkpoint
verifier's separate contract and no-snapshot qualification; it does not silently
perform this successor. Architecture/performance are reviewing a finite proposal
in the existing notes; no next source is released. Then finish snapshot/install/
purge, consensus/read barriers, remaining domain and Claim/policy routes, services
and full M1–M8 acceptance. The full goal remains active; new artifacts still require
concrete architecture consultation and substantive tests before implementation.

Earlier startup checkpoint (`0e80526`):

That checkpoint adds bounded startup verification between the separate durable
log and workflow state stores. The required log point read admits the caller's
actual body owner before copying, preserving old page/wire behavior. The consuming
verifier returns both owners only after full term/node/index and SHA-256 of the
actual body match the saved applied checkpoint. Every failure drops both handles.
Its read-only state guard ends before log activity; checked workspace reserves the
maximum of those sequential phases. See
[ADR-007](adr/007-durable-log-storage.md#bounded-checkpoint-startup-join).

All five new product test identities pass after genuine missing-contract RED,
including real separate-file reopen, identity/body/scope mismatch, exact/tight
ownership caps, unchanged observable authority and three actual process kills:
after durable append, after durable committed persistence before state apply, and
after successful state-driver return. These are call-boundary process tests, not
power-loss, network acknowledgment or Linux evidence. The applied-ahead rejection
checks its exact control/log; its two-record business cohort is not independently
reasserted. Architecture and performance reviewed the actual tests/source and
each other's findings. Two lint-only repairs preserve the complete frozen oracles
and checked arithmetic; the new exact dependency/module guard failed before the
selected policy change and now passes.

The final native macOS snapshot passes **720 Rust tests across 86 workspace
targets**, plus six compile-fail examples across 15 documentation targets, with
zero failures or ignored tests. Formatting, all-target compilation, strict Clippy,
warnings-denied rustdoc, architecture and reference/progress checks pass. The full
cached dependency audit passes advisories, bans, licenses and sources without
waivers or exclusions; no fresh online advisory refresh is claimed. All 691 selected
source/manifest/policy/style inputs stayed unchanged throughout final verification.

The separate pinned native acceptance executable passes formatting, compilation,
strict Clippy and its full persistence/fault/process-kill cases. All 708 combined
inputs stayed unchanged. Its 17 files, including 15 Rust sources, remain exactly
proof 55bcbd85; no native source changed. Both stores retain the canonical redb
4.4.0 pin and no-default/std-only profile. The consensus verifier adds only the
existing exact no-default sha2 dependency and uses neutral ports; no concrete
storage edge, runtime framework or new registry package is introduced.

The executed full parity gate still fails. Ledger counts remain 82 source-derived
declarations, 92 mappings and 365 assertion links; 1,681 runtime cases remain
unmatched and 1,655 declarations unresolved. These component checks add no full
source-case parity credit. Complete pinned Elixir setup, fixtures, mandatory
profiles and runtime scenarios remain required, as does native Linux validation.

Next: bounded neutral Blank/Membership bodies, atomic last-applied membership with
the SAME ControlState, and ordered recovery/apply through the actual ports. Persisted
last-applied membership must remain distinct from Raft's possibly uncommitted
effective configuration. Preserve existing Normal FWCP bytes and SHA-256(body).
Then finish snapshot/install/purge, consensus/read barriers, remaining domain and
Claim/policy routes, services and full M1–M8 acceptance. Startup verification alone
performs no suffix replay, membership recovery or service admission. The full goal
remains active; next artifacts still require concrete architecture consultation
and substantive tests before implementation.

Earlier state-store checkpoint (d664574):

The scoped synchronous durable log now has a product state-store counterpart.
RedbStateStore implements the existing GroupApplyStore through explicit bounded
genesis, strict noncreating recovery and one exclusive consumed Immediate writer.
It binds namespace/bootstrap data, validates local placement and complete
bidirectional due/FIFO authority, and atomically publishes records, indexes,
history, retained outcomes and control for the existing V5/V6/public1 drivers.
Genesis donor admission and recovery/read/output accounts are prospective and
cumulative. Fatal uncertainty/corruption/unavailability fences the same store.
See [architecture](architecture.md#scoped-synchronous-state-authority),
[ADR-004](adr/004-committed-time-and-apply-admission.md#scoped-state-store-admission)
and [ADR-007](adr/007-durable-log-storage.md).

All six substantive state-store test identities pass, including full independent
physical cohorts, maximum binary/value profiles, strict indexes, delegated-sync
failure/reconciliation and eight real boundary kills. Four kills occur before
sync with no business acknowledgment; four occur after successful return/ACK.
These are process-termination checks, not power-loss or Linux evidence. Three
separate regression controls failed before narrow repairs: applied clocks below
immutable genesis, a valid policy exceeding a smaller state-count profile, and
count2 retained output reaching a real batch1 guard. Preimplementation missing-API
RED and pre-freeze fixture corrections are recorded separately. Architecture and
performance reviewed the actual source and each other's findings; style repairs
preserve complete frozen oracles through executed whole-file inverses.

The final macOS snapshot passes **714 Rust tests across 85 workspace targets**
and six compile-fail examples across 15 documentation targets, with zero failures
or ignored tests. Formatting, all-target compilation, strict Clippy,
warnings-denied rustdoc and reference/progress checks pass. The full-workspace
cached dependency audit passes advisories, bans, licenses and sources without
exclusions or waivers; this is not a fresh online advisory refresh. All 685 selected
Rust/manifest/policy/style inputs remain unchanged throughout validation.

The existing isolated native acceptance executable also passes pinned formatting,
compilation, strict Clippy and its complete persistence/reopen/process-kill cases.
All 702 combined product/native inputs remain unchanged. Its exact 17 files,
including all 15 Rust sources, are identical to proof 55bcbd85 on
codex/workflow-record-batch-native-proof; no new native proof source was needed.
Both use canonical redb 4.4.0 commit 7061f7e60d4573311c0636e161d417b93e9fd2b1,
no-default/std only. Offline validation uses
CARGO_HOME=/private/tmp/ferricstore2-m2-durable-log-cargo-home, seeded from verified
upstream objects; product manifests retain the upstream pin.

The full workflow-parity gate was executed and still fails as required: original
scenarios, mandatory profiles, setup, fixtures and adapters are incomplete.
Ledger counts remain 82 source-derived declarations, 92 counterpart mappings and
365 assertion links, with 1,681 unmatched runtime cases and 1,655 unresolved
declarations. Component/disk tests confer no new full-source parity credit.

Continue with the earliest actual M2 predecessor: neutral Blank/Membership and
applied-state recovery, exact full log-ID AND actual-body-digest joins between
separate database owners, then snapshot installation and safe purge before full
consensus/service wiring. Committed Claim/policy routes, complete M1–M8, original
Elixir setup/runtime parity and native Linux remain required. No successor
artifact/model or implementation is released merely by this next-step statement;
use the concrete architecture consultation and tests-first boundary in AGENTS.md.
This checkpoint establishes no milestone exit or completed product.

The entries below record earlier increments; their counts and next-step statements
belong to those historical checkpoints.

The public-ingress continuation now has its domain predecessor: bounded borrowed
scalar caller normalization and source-compatible retry jitter derived inside
authoritative apply. All 354 domain tests across 40 targets pass. The final
focused controls and strict Clippy across all domain targets also pass after
reviewed style repairs. Nineteen independent tuple samples were executed on
OTP 28 and 29 before implementation. Review found reserved-type and unsupported
field diagnoses; added controls reproduced each finding before its repair.
The shared reducer preserves the private explicit-sample path and skips source
hash work for None backoff or a zero jitter percentage.

The reviewed public ingress now also supports independent CreateMany through the
existing normalization, retained outcome and consumed-transaction path. Every
item is normalized and measured before effects; duplicates retain ordered,
independent results. The native fixture exercises the original explicit-partition
mixed result and automatic-partition 34-slot/33-created scenarios, physical
history/index/outcome cohorts, original-request replay and database reopen.
Existing terminal families retain their operation restrictions. See
ARCH-PUBLIC-CREATE-MANY1-CREATE1 and the reciprocal review records.

Review also reproduced a pre-existing restoration bug: Create accepts terminal
state names with a due time, but domain restoration and stored-record scanning
rejected the resulting valid records after acknowledgment. Two narrow validation
changes admit that combination only with an explicit Created witness. Regression
tests failed twice before the repair and now cover all three terminal names with
default and explicit-zero due times, non-Created rejection and native reopen.
See ARCH-TERMINAL-CREATE-ROUNDTRIP1-CREATE1 and its precision.

The final macOS run passes all 700 Rust tests across 83 workspace targets and six
compile-fail examples across 15 documentation targets, with no failures or ignored
tests. Formatting, compilation, strict Clippy, warnings-denied rustdoc and
reference/progress checks pass. The complete isolated native executable also
passes formatting, strict Clippy and all eight real process-kill boundaries.
All 648 combined source/manifest inputs remained unchanged during verification.
The exact 17-file native proof is saved at f8ec8443 on
codex/workflow-record-batch-native-proof. Registry dependencies are unchanged;
the earlier online advisory-refresh DNS failure is not a successful fresh audit.

Ledger counts remain 82 source-derived declarations, 92 counterpart mappings and
365 assertion links. Runtime reconciliation still has 1,681 unmatched cases and
1,655 unresolved declarations; these component checks confer no new full-source
parity credit. Original setup/runtime profiles, production backend/consensus
integration, Linux execution and full M1–M8 parity remain required. Architecture
and performance selected the scoped durable-log predecessor described above,
using the existing OpenRaft/redb spike as evidence. Its backend tests and source
remain the next implementation work. Rich individual Create/Transition/Extend replies,
current authorization and supervised runtime integration also remain required;
this increment establishes no milestone exit or full-source parity.

The current increment implements the private V6 independent batch bridge through
the same preparation, one retained gate, Fresh decision and consumed transaction
commit. Counted scalar responses bind every ordered result; exact V5 bytes and
strict profile boundaries remain. Publication compares original versions once,
writes final records/indexes once and preserves every intermediate history.
Captured admission limits govern input, output and retained ownership.

The full macOS wrapper passes 687 Rust tests across 80 targets plus six
compile-fail examples across 15 documentation targets, with zero warnings,
failures or ignored tests. Formatting, compilation, strict Clippy, rustdoc,
architecture and reference/progress checks pass on 1,190 unchanged non-review
inputs. Its sole failure is GitHub DNS during the default advisory refresh;
the separate cached October 14 audit passes all four categories. The isolated
native redb executable also passes formatting, strict Clippy and every old/new
case, including six actual process-kill boundaries and sync uncertainty.

All nine new product identities pass their first assertion execution after
missing-contract RED twice. Review additionally finds a typed Replay status/body
mismatch; its added control fails twice before the narrow fix, then passes with
exact-log/conflict/expiry bypass controls. Later lint repairs preserve complete
frozen oracles by whole-file inverses. Ledger counts remain 82/92/365, with no new
full source-case parity credit. See ARCH-M2-INDEPENDENT-V6-PUBLICATION1-CREATE1.

Next are actual public normalization/submission and no-pre-read hook behavior,
backend adoption and remaining ordered persistence/recovery gates. The native
consumer is an isolated acceptance experiment. Original setup/runtime profiles,
Linux execution, complete pure-domain acceptance and full M1–M8/workflow parity
remain required; this increment establishes no milestone exit.

The preceding increment adds a touched independent indexed plan for all eight
mutation operations. Later commands for the same scoped ID use the latest
successful write; item errors and no-ops preserve that predecessor and ordering
cursor. Results retain every successful history event. Separate descriptors
borrow original canonical records and select final records/indexes once per ID,
without changing the intermediate expected versions or cloning earlier plans.
This is the required domain representation for durable publication, not the
publisher itself. Both original pure entry points retain their old restrictions
and admission bounds.

The latest macOS wrapper passes 678 Rust tests across 79 targets plus six
compile-fail examples across 15 documentation targets, with zero warnings,
failures or ignored tests. Formatting, compilation, strict Clippy, rustdoc,
architecture and reference/progress checks pass on 1,181 unchanged non-review
inputs. The wrapper exits 1 solely because GitHub DNS blocks its default advisory
refresh; the separate cached October 14 audit passes all four categories.
Architecture and performance accept the actual source and reciprocal reviews.
Four new tests passed on their first assertion run after missing-API RED twice;
subsequent lint repairs preserve the complete test oracle by whole-file inverse.
Ledger counts remain 82/92/365, with no new source-case parity credit.

That indexed prerequisite supplies the original/final/history representation
used by the V6 bridge above. Public hook/no-pre-read behavior, engine adoption,
original setup/runtime profiles, native Linux and full M1–M8/workflow parity
remain required. See ARCH-M2-INDEPENDENT-INDEXED1-CREATE1 and its
step-representation refinement; it establishes no milestone exit.

Pure independent terminal batches now use the same ordered planner for Complete,
Fail, caller-resolved Retry and Cancel. Per-item errors preserve other successes;
an identical Complete pair emits one write/event followed by a legacy no-op.
The old transition entry point retains its operation restriction through public
compatibility aliases. Cumulative admission charges the actual evolving record
before every reducer call, including an eventual no-op; results own their bytes.

That macOS wrapper passes 674 Rust tests across 78 targets plus six
compile-fail examples across 15 documentation targets, with zero warnings,
failures or ignored tests. Formatting, compilation, strict Clippy, rustdoc,
architecture and reference/progress checks pass on 1,176 unchanged non-review
inputs. The wrapper exits 1 solely because GitHub DNS blocks its default advisory
refresh; the separate cached October 14 audit passes all four categories.
Architecture and performance reviews accept the actual source and 30 new direct
component assertion links across three source declarations. Ledger counts are
82/92/365; all 5,855 other declarations and every runtime identity are unchanged.
Four new tests passed on first assertion execution after missing-API RED twice;
a later test-only Clippy style repair preserves the complete frozen oracle by
whole-file inverse. Public/durable independent publication, hook/no-I/O cases,
physical history/index effects, original setup/runtime profiles, native Linux
and full M1–M8/workflow parity remain required. This increment establishes no
milestone exit; see ARCH-M1-INDEPENDENT-TERMINALS1-CREATE1 and the review records.

Pure independent transitions now preserve ordered per-item failures and successes,
including repeated IDs consuming the latest successful proposed record. Errors and
legacy no-ops retain that predecessor; the existing atomic batch stays unchanged.
Outputs own every proposed partition/core/history value after input donors drop.
Cumulative evolving-owner admission precedes each copy, with a separate borrowed
lookup descriptor allowance and no whole-group clone. The four selected tests
passed on their first assertion execution after precise missing-API RED twice.

The final macOS wrapper passes 670 Rust tests across 77 targets plus six
compile-fail docs, with zero warnings/failures/ignored tests. Formatting,
compilation, strict Clippy, rustdoc, architecture and reference/progress checks
pass on 1,175 unchanged non-review inputs. The wrapper exits 1 solely because
GitHub DNS blocks its default advisory refresh; the separate cached October 14
audit passes all four categories. Actual source and reciprocal architecture/
performance reviews accept this increment. Nine direct component assertion links
add two previously unmapped source declarations: ledger counts are 79/89/335.
All 5,856 other declaration records and every runtime identity remain unchanged;
the two new runtime IDs remain unresolved. Public/indexed/durable independent
publication, original setup/runtime profiles, native Linux and full M1–M8/workflow
parity remain open. See ARCH-M1-INDEPENDENT-TRANSITIONS1-CREATE1 and the active
review records; this component does not establish a milestone exit.

The separate Elixir bugfix checkout now has commit 7c351783, fixing the Elixir 1.20
TermCodec pin warning after a genuine fresh-target diagnostic failure. Strict
target checks and the same 89 existing test cases pass on both 1.19.5/OTP 28.3.1
and 1.20.4/OTP 29.0.5 (178 executions). Cached support-module scope is explicit;
native/service/full-source profiles, public worker/governance and Linux remain
unexecuted. Earlier dirty changes are preserved. This adds no Rust parity credit;
see [the Elixir review](reviews/workflow-completion-elixir-bugs.md).

The preceding joined record-batch driver preserves complete policy backing,
independent query owners, cumulative R/S/C accounting, actual operational prefetch
precedence and owned responses after consumed commit. Frozen product/native
oracles failed twice for the precise missing APIs/files before source release.
All seven new product behaviors passed on first execution within 29 focused
tests. The complete isolated redb executable passes, including real delegated
sync-error recovery and four accepted/rejected process-kill boundaries. A further
healthy-policy work-limit control and a smaller-state-profile control each first
failed with Corrupt, then passed after narrow mapper repairs. Count refusal can
precede deeper content validation; it does not certify stored bytes as healthy.
Borrowed local indexes remove repeated linear outer
lookups with separately charged descriptors; no measured native speedup is claimed.

The final macOS wrapper passes 666 Rust tests across 76 targets plus six
compile-fail docs, with zero warnings/failures/ignored tests. Formatting,
compilation, strict Clippy, rustdoc, architecture and reference/progress checks
pass on 1,173 unchanged non-review inputs. Lint-only repairs preserve the complete
frozen oracle files by whole-file inverses. The wrapper exits 1 solely because
GitHub DNS blocks its default advisory refresh; the separate cached October 14
product audit passes all four categories. Native formatting, strict Clippy and
the complete executable separately exit 0. Architecture and performance reviewed
actual source; product and backend owners reviewed each other. See
ARCH-M2-RECORD-BATCH-TRANSACTION1-CREATE1 and its borrowed-encoder/lookup precisions
in the coordination/review records. Ledger counts remain 77/87/326 with no new
Elixir parity credit. Engine adoption and production backend wiring, original
assertion/setup/runtime profiles, native Linux and full M1–M8 completion remain
open. This increment does not establish M2 exit.

The preceding M2 increment implements bounded touched-record/indexed inputs.
Explicit observations distinguish unknown authority from actual absence, with
shared batch/index algorithms preserving the complete-view path. Ordinary FIFO
followers need their exact member without an unrelated head lookup; named
claims retain true-head checks. Reached retry observations override item hints,
including observed default policy. The consuming Fresh bridge reuses decoded
owners and adds caller-live logical bytes before metadata and late response
allocation. These supplied facts establish no physical read provenance.

The final macOS wrapper passes 659 Rust tests across 74 targets plus six
compile-fail docs, with zero warnings/failures/ignored tests. Formatting,
compilation, strict Clippy, rustdoc, architecture and reference/progress checks
pass on 1,164 unchanged non-review inputs. All six new behavior tests passed on
their first assertion run. Later lifetime-name and test-helper lint repairs
preserve every assertion operand. The wrapper exits 1 solely because GitHub DNS
blocks its default advisory refresh; a separate cached October 14 audit passes
all four categories. Ledger counts remain 77/87/326, with no new Elixir parity
credit. Next: coherent bounded placement/policy/current/index loading and the
real atomic adapter, preserving complete policy backing and accounting for
temporary read-owner overlap. Engine adoption, crash/uncertainty recovery,
original assertion/setup/runtime profiles, native Linux and full M1–M8
completion remain open.

The preceding M2 increment adds a bounded retained point/cohort gate. A missing
requested outcome cannot hide other outcomes in its tenant/group/recovery scope.
The same classifier preserves full-map and charged API order; independent query
and outcome lifetimes let replay borrow its original owner. The ports phase
shares existing decoded checks and Complete/Fresh handling without a temporary
map. A required transaction prefix probe and bounded physical outcome-prefix
encoder support the future adapter; no engine is implemented by this contract.

The final macOS wrapper passes 653 Rust tests across 73 targets plus six
compile-fail docs, with zero warnings/failures/ignored tests. Formatting,
compilation, strict Clippy, rustdoc, architecture and reference/progress checks
pass on 1,157 unchanged non-review inputs. One pointer-style lint was repaired
without changing its compared addresses; all six new behavior tests passed on
their first assertion run. The wrapper exits 1 solely because DNS blocks the
default advisory refresh; a separate cached October 14 audit passes all four
categories. Ledger counts remain 77/87/326 with no new Elixir parity credit.
Next: explicit bounded touched-record/index and current policy observations,
then the real atomic storage adapter. Dependency adoption, original runtime/setup
profiles, native Linux and full M1–M8 completion remain open.

The preceding M2 increment separates bounded raw batch preparation, one retained
inspection and Fresh-only indexed decision. Preparation admits decoded owners
before allocation and captures the actual profile and derived intent result.
Complete drops decoded scratch and borrows the original retained result;
Fresh moves the same buffers without retaining the outcome-map/query lifetime.
The legacy API preserves its error order and the existing reducer and bounds.
Full coherent retained and indexed authority remain required.

The final macOS wrapper passes 647 Rust tests across 73 targets plus six
compile-fail docs, with zero warnings/failures/ignored tests. Formatting,
compilation, strict Clippy, rustdoc, architecture and reference/progress checks
pass on 1,156 unchanged non-review inputs. One initial test-only Clippy finding
was repaired without changing operands. The wrapper exits 1 solely because
DNS blocks the default advisory refresh; a separate cached October 14 audit
passes all four categories. Ledger counts remain 77/87/326 with no additional
Elixir parity credit. Its point-retained successor is described above;
touched-authority inputs and the real atomic storage adapter still follow.
Dependency adoption, original
runtime/setup profiles, native Linux and full M1–M8 completion remain open.

The preceding M2 increment implements borrowed record/outcome scans with complete
expected point binding and prospective ownership admission. One shared parser
feeds the new consuming views and the existing owning decoder APIs. Exact
logical tariffs include 512 plus every returned variable field; wire/domain
caps remain separate. Five new behavior tests pass their first assertion run
after twice-reproduced missing-API RED. Two additive peer-review controls are
explicitly post-RED refinements; no original operand was weakened.

The final macOS wrapper passes 642 Rust tests across 73 targets plus six
compile-fail docs, with zero warnings/failures/ignored tests. Formatting,
compilation, strict Clippy, rustdoc, architecture and reference/progress checks
pass on 1,154 unchanged non-review inputs. The wrapper exits 1 solely because
DNS blocks the default advisory refresh; a separate cached October 14 audit
passes all four categories. Ledger counts remain 77/87/326 with no additional
Elixir parity credit. Its single-gate successor is described above; the real
atomic storage adapter still follows. Dependency adoption, original runtime/setup
profiles, native Linux and full M1–M8 completion remain open.

The preceding M2 increment adds pure physical point/ordered keys and full history
values under the named architecture selection. All seven new behavior tests and
six older value tests pass their first assertion execution after independently
reproduced missing-API RED. Independent byte vectors preserve full FIFO u128,
binary component order, original history IDs and distinct committed event time.
The final macOS wrapper passes 637 Rust tests across 73 targets plus six
compile-fail docs, with zero warnings/failures/ignored tests. Formatting,
compilation, strict Clippy, rustdoc, architecture and reference/progress checks
pass on 1,152 unchanged inputs. Four earlier lint findings remain recorded; their
repairs preserve all frozen operands. The wrapper exits 1 solely because DNS
blocks the default advisory refresh; a separate cached October 14 audit passes
all four categories. Ledger counts remain 77/87/326, with no new parity credit.
Engine adoption, actual atomicity/recovery, native Linux and full M1–M8 completion
remain open. The exact redb 4.4 Git-source experiment now passes offline compilation,
strict lint and real guard/range/reseek, two-table abort/Immediate commit,
writer-lock and clean reopen assertions on macOS 26.6.2 arm64. All 131 upstream
files and 34 Git tree objects remain pristine. The first consumer import and
four lint failures are recorded with unchanged operands. This temporary profile
is not a registry archive/checksum, dependency adoption, GroupApplyStore adapter,
crash/uncertainty or native Linux result. A separate reviewed warm-cache cursor
experiment now passes its complete untimed oracles and all 48 paired samples:
same-writer reseek medians are 2.05–2.14 times the local retained-range baseline
across four fixtures. Twelve Immediate commits are measured separately. This is
native macOS fixture evidence, not a deployable retained cursor, product p99 or
Linux result. The isolated redb graph also has zero cached-audit errors, with
four unused product-policy warnings recorded. The separate joint redb/OpenRaft
proof now passes the real storage-v2 Suite, singleton write/read-barrier/snapshot
transfer/reopen, cancellation drain, delegated-sync fencing/reconciliation and
four acknowledged process-kill recovery cases. Its three bounded snapshot tests,
strict Clippy and formatting pass; earlier compile/lint/format failures remain
recorded. These are temporary native macOS proof results, not additional product
tests or workflow parity. Its full-graph audit stops before checks on uncached
Windows/Android crates, and registry adoption remains open. The product scanner
successor is described above; a single retained gate before Fresh authority
reads and the real atomic storage adapter still follow. Full runtime, native
Linux and M1–M8 remain open.

The preceding reviewed M2 prerequisite implements neutral group transaction traits,
validated change projections and versioned control/record/outcome codecs in five
production files. Nine frozen behavior tests pass on their first assertion run
after missing-API RED independently reproduced by Root. Independent literal
vectors preserve exact original stamps, record witnesses and retained responses;
two new architecture tests have their own RED/GREEN. Reciprocal source review
is closed without a component blocker. Strict lint repairs preserve every
assertion, byte grammar and existing admission/response encoding body.

The final macOS wrapper passes 630 Rust tests across 72 targets plus six
compile-fail docs, with zero warnings/failures/ignored tests. Formatting,
compilation, strict Clippy, rustdoc, architecture and reference/progress checks
pass on 1,144 unchanged inputs. The wrapper exits 1 solely because DNS blocks the
default advisory refresh; a separate cached October 14 audit passes all four
categories. Earlier lint/format failures remain recorded. Ledger counts remain
77/87/326 with no new parity credit. Engine adoption, physical atomicity/crash
recovery, snapshots, native Linux execution and full M1–M8/source/profile
completion remain open. The next M2 recommendation is read-only readiness work.

The latest restoration repair preserves valid generated retry deadlines
above the caller timestamp ceiling. Its whole-record regression first failed
with RunAtTooLarge against unchanged production; removing only that incorrect
stored due-time check then passes13 retry and12 active-timeout tests. Lease,
terminal, clock and witness checks remain intact. Ordinary source review is
closed without a blocker. The final macOS run passes619 Rust tests across70
targets plus four compile-fail docs, with no warnings/failures/ignored tests.
Formatting, strict lint, documentation, architecture and reference/progress
checks pass on1132 unchanged inputs. The wrapper exits1 solely at the default
RustSec DNS failure; all four cached audit categories pass separately.
Ledger counts stay
77/87/326 with only the existing test fingerprints/line anchors refreshed.

The latest reviewed commit `b530513` implements private indexed mutation-batch
encoding/apply and retained replay. Six new tests pass after independently
reproduced missing-API RED; source review's non-batch error correction has its
own regression RED/GREEN. Fresh bounds temporary owners, uses one request gate
and preserves actual retry authority, input order, priority/FIFO and rollback.
Fixed retained metadata is boxed after admission and validation, without a
measured native performance claim. An independent exhaustion model adds64
retry attempts across16 limit/target combinations to the existing target.
Sixteen existing error displays also avoid temporary formatting Strings; the
same352 Rust tests and one doc test pass before and after. The parsed `write!`
audit has genuine tests-first RED/GREEN and preserves nested effect inspection.

The final native macOS joint run passes618 Rust tests across70 targets and four
compile-fail docs, with zero warnings, failures or ignored tests. Formatting,
all-target compilation, strict Clippy, rustdoc, all40 architecture tests,
reference integrity and progress checks pass on1132 unchanged inputs. The wrapper
exits1 only because GitHub DNS blocks its default RustSec refresh; the separate
cached October14 audit passes all four categories. Earlier full-wrapper macro
and missing-target-safety-attribute failures remain recorded. The ledger now has
77 declarations,87 counterparts and326 links, adding only twelve direct bulk
ID/state/version observations. Physical refs/info/fetched histories, original
setup/runtime profiles, independent Linux and full M1–M8 completion remain open.
The architecture exit table now identifies concrete evidence for all five
literal pure M1 model subjects. It selects no further pure policy/model layer.
The next phase is read-only M2 atomic publication/recovery readiness and an
exact artifact proposal; no backend or persistence implementation is approved.

The preceding local commit `dbfdf0b` implements original public Policy.get validation and reader
composition in three production files. Domain preparation preserves complete
keyword validation, FIRST state and source error order; the ports state boundary
performs one allocator-zero capability call and releases its temporary key before
reply projection. All seven new behavior tests pass, and the full policy target
passes 129 tests. The final native macOS workspace run passes 610 Rust tests
across 69 targets and four compile-fail docs, with zero warnings, failures or
ignored tests. Formatting, compilation, strict lint, rustdoc, architecture and
reference/progress checks pass on 1,122 unchanged inputs. The wrapper exits 1
only at its default RustSec refresh because GitHub DNS is unavailable; the
separate cached October 14 audit passes all four categories. Strict all-feature
Clippy also passes. Source and reciprocal reviews found no component blocker.
The ledger has 73 declarations, 83 counterparts and 314 assertion links, adding
only direct GET generation/max-active and corrupt-type/body observations while
preserving all older records. Full M1–M8, physical/cache/barrier adapters, original
setup/runtime profiles and independent Linux validation remain incomplete.

The preceding committed checkpoint implements original public policy reply views in eight
private production files. Root independently reproduced missing-API RED before
production; the seven new domain tests and the real Fresh/Replay caller test
pass. The final native macOS workspace run passes 603 Rust tests across 68 targets
and four compile-fail docs, with zero warnings, failures or ignored tests.
Formatting, strict all-target lint, rustdoc, architecture and reference/progress
checks pass on 1,116 unchanged bound inputs. The wrapper exits 1 only because GitHub
DNS blocks its default RustSec refresh; the separate cached audit passes all four
categories against the October 14 database. Strict all-feature Clippy also passes.
Two library and three test lint repairs preserve expectations and paid order;
their failed epochs and the post-SUT States lookup regressions remain recorded.
At that checkpoint the ledger has 71 declarations, 79 counterparts and 306 assertion links,
adding only the three executed hot-history-key absence observations. Full M1–M8,
public/physical operations, original runtime profiles and native Linux remain
incomplete. Written-source and reciprocal reviews are closed without a component
blocker. See the selected reply contract in ADR006 and the coordination note.

The preceding checkpoint contains the selected original public Policy.set frame and
both-mode Fresh merge successor. Its twelve new test identities have genuine
tests-only missing-API RED independently reproduced by Root before production.
All eighteen selected production files have substantive bodies. The native macOS
workspace run passes595 Rust tests across68 targets and four compile-fail docs;
formatting, all-target compilation, strict Clippy, rustdoc, architecture and
reference/progress checks pass with zero warnings, failures or ignored tests.
The full wrapper exits1 only at its default RustSec refresh because GitHub DNS
resolution is unavailable. A separate cached audit passes all four categories
against the October14 database; it does not certify a fresh advisory refresh.
The same1102 bound source/tooling/fixture/documentation inputs remain unchanged
through the complete run. Written-source architecture/performance and reciprocal
reviews are closed without a remaining component blocker.
The new source-backed 1,000-rule retained-current regression is explicitly a
post-initial-RED refinement. No new parity credit or native Linux result is
claimed. See ROOT-NEXT-M1-ORIGINAL-PUBLIC-MERGE-RED1-RELEASE1 in the coordination
record and the selected contract in ADR006.
The last complete wrapper success belongs to the preceding582-test preparation
checkpoint. Earlier retention is `a0a8858`, following retry `a969309`,
original-current Replace `2759262`, reader/source603 `bbeba0d` and MAX-generation
supplement `7f3e21a`.
That earlier wrapper passes strict lint, architecture, documentation,
reference/progress and all four default dependency-audit categories, exiting0
on unchanged1036 check inputs. Its source binding and prior failed epochs are
in the coordination record. The configured RustSec database is at
`edb58f5974b83eb8302332821cb6d389777aa351`
(October14). Earlier DNS-failed wrappers remain historical failures. Current
Git SSH/HTTPS still fail host resolution, so no fresh Linux CI run is claimed.
The whole-flow normalization/original ETF batch is implemented after explicit
architecture/performance approval and independently reproduced tests-only
missing-API RED. All100 policy controls pass, including the old86; source-based
post-RED corrections, supplemental refusal tests and the first failed workspace
architecture epoch are recorded without rewriting their provenance.
Actual source and mapping reviews, including reciprocal reads, are closed
without blockers for this snapshot component.
Detailed local results are below;
saved CI belongs to its earlier commit. M1,
later runtime milestones and full workflow parity remain incomplete.

The original whole-flow lower replacement parent is now implemented after
independently reproduced tests-only RED. Its104 domain policy tests and81 policy
request tests passed, including twelve new tests. One borrowed structural verifier
and one retained-request gate precede Fresh. Fresh preserves current/CAS/type/
captured-limit/history order and emits an original ETF envelope directly from
the same normalized layout. Its owned key, installation and distinct response
survive the frame/current/layout owners. The parent alone accepts263168 encoded
current spans, closing read-after-own-output while the old public4096 profile
stays unchanged. Eleven production files fit400 lines, seven test files fit600,
and functions fit100. Actual source reviews cover the final paid-read repairs;
lint-only test edits invert exactly to the frozen preproduction buffers.

This component implements the lower replace=true command. Faithful VM/source
order capture is still an ingress obligation; the sidecar proves relative
correspondence only. Raw Other remains a temporary REQUIRED opaque-term identity
gap, not a product exclusion. Public Policy.set preprocessing/replies,
physical atomic/all-shard installation, original setup/runtime profiles and
independent Linux execution remain required. The checked ledger now has71
source declarations,78 counterparts and303 assertion links.
Logical ownership improvements supply no measured allocation or speed claim.

Six supplemental direct source tests now pass within the87-test policy-request
target, with strict lint. Their315-line private child preserves generation1→2,
stale CAS, FIFO omission reset, cross-policy encoded-patch rejection, generation3→4
and captured C100/C10 history50 recipes. Twenty-two new links cover value/error
components only; all71 older counterpart records and every runtime ID are exact.
These tests were added after implementation and supply no new tests-first RED.
Public get/all-shard/ETS observations and the public nil projection remain
unlinked; source error pattern assignments are not direct assertion anchors.
The preceding source-anchor macOS wrapper passes573 Rust tests/four compile-fail docs and every
source/dependency stage with the same1027 check inputs before and after execution.

Public Policy.set input preparation now passes all112 policy tests, including
all104 old controls and eight new tests. The five private production files and
four test children preserve FIRST controls, LAST ordinary policy values, both
replace modes, callback/barrier semantics, exact raw sizing and borrowed payload
owners. Root independently reproduced missing-API RED before production and the
final complete macOS wrapper passes582 Rust tests/four compile-fail docs, every
source stage and all four default dependency-audit categories on unchanged1036
implementation/tooling inputs. Strict lint,400/600-line files and100-line
functions remain enforced without suppression.

Actual source review caught an improper state-pair tail that must fail during
its predicate before generic conversion. The source-derived exact7777/tight7776
regression passes; it remains a postimplementation supplement. Two initial
borrow-lifetime errors and ordinary library/test lint failures were corrected
without changing the frozen fees, expected values or eight test identities.
The same compact slot Vec stages, sorts and compacts adjacent duplicates, and
all4096 distinct keys now pass under the actual default work budget. This is
bounded-work evidence, with no measured native allocation or speed claim.

The ledger links only the pinned invalid response-state diagnosis and exact
caller-body components, advancing to71 declarations/78 counterparts/303 links
while preserving all77 older counterparts. Actual written-code architecture,
performance and reciprocal reviews are closed. Full default-false arbitrary-current
merge, source/VM capture,
opaque identity/Enumerable, public identity/replies, physical atomic/all-shard
publication, every original setup/runtime profile and independent Linux remain
required. See ROOT-NEXT-M1-ORIGINAL-PUBLIC-SET1-GREEN1 in the
[coordination record](reviews/workflow-completion-coordination.md).

Original whole-flow input now preserves top/state/governance error order,
arbitrary nonnegative customer versions, exact caller bodies and deterministic
original ETF snapshots. Borrowed retry/indexed/retention walkers share semantics
while retaining their old public fees. One bounded incoming walk stages cells;
late governance and complete-snapshot size gates precede one fallible final Vec.
Synthesized state maps preserve the source32/33-key ordering transition using
explicit fixed-width hash arithmetic. The standalone snapshot API grants no frame, Fresh,
current-policy, installation or payment authority. All fourteen new production
files fit400 lines and seven test files fit600; functions fit100. The checked
ledger has71 reviewed declarations,78 counterparts and303 assertion links,
preserving every older record. Full public/physical/runtime/profile acceptance
and independent Linux execution remain required, with no measured native
allocation or speed claim.

Claim/reclaim now constructs the replacement record directly, avoiding copies of
old state, saved run state and lease owners that would immediately be replaced.
The 24-scenario ownership matrix and six existing unit controls pass against both
the original constructor and the candidate. The test helper now accepts borrowed
slices; strict Clippy remains enabled. This is a source-level copy reduction,
with no measured speedup or allocation claim.

The new lease lifecycle target executes seven pinned source scenarios in four
tests, covering reclaim boundaries, extension ownership, completed-record
rejections, stale failure guards and unleased cancellation. The existing default
claimed fixture moves unchanged into `tests/claimed_support/mod.rs`, loaded only
by its three existing consumers. Every old test byte and assertion offset is
preserved before their new module declaration. The new file has 426 lines;
the largest affected file has 590, and all functions fit 100. The ledger now has
51 reviewed source declarations, 52 counterparts and 211 assertion links.
Public return-OK/no-materialization, error references, queries/selection,
original runtime setup/profiles, durable storage and Linux validation remain
required acceptance work. ROOT-NEXT-M1-LEASE-LIFECYCLE1-GREEN1 retains the passing
full wrapper and the preceding lint failures. Written-source architecture and
performance review is mutually closed without blockers.

The original Complete Replace frame producer/verifier is now implemented after
genuine tests-only missing-API RED. Six new tests in four approved private files
preserve frozen bytes, headers, field presence, caps, canonical lists and continuous
work accounts. The target passes 48/48. The producer checks uniqueness after
frame measurement and its byte cap, before hashing/output prepayments. Existing
caller and V2 behavior remain covered. A missing import and ordinary test lint
issues were corrected; source arithmetic also corrected a scratch-refusal
expectation to retain the already-paid list root. CHECK1/CHECK2 preserve all
failed and passing evidence. Actual source architecture/performance reciprocal
review is closed without blockers.

The consuming decoder is now implemented after genuine tests-only missing-API
RED. Its six new controls pass within the 54-test target, preserving original
fields, header owners, business-invalid values and continuous reconstruction
work. The two private production children and two test children fit the ordinary
file/function limits. CHECK1 passes all macOS source checks and retains the
separate final online dependency-refresh failure. Actual decoder source review
and reciprocal architecture/performance review are closed without blockers.
The downstream audit records
captured history limits, source error order and exact governance/indexed responses.
The Fresh retained-request parent is implemented below; physical publication
and the full original runtime contract remain required.

The normalized Replace content checker and exact response encoder are now
implemented after genuine tests-only missing-API RED. Six new controls pass in
the 51-test complete-policy target. Operational metrics precede one borrowed
business walk in source field order; indexed correspondence stays operational.
Immutable history diagnoses keep the original admitted limit, and responses
use static literals or seven local decimal bytes with one reserved output Vec.
Supplemental post-implementation assertions independently verify fresh fees for
every response fixture, including history limits 1, 17 and 1,000,000; the updated
51-test target and strict target Clippy pass. These reruns overlap the 478 total.
CHECK1 preserves seven test-style failures and CHECK2 one ordinary formatting
wrap; CHECK3 passes every macOS source stage and stops only at online RustSec
DNS. The current offline four-category audit passes against the configured
October 9 database. Final actual-source reciprocal review is closed without blockers.
The five production files fit 400 lines and the three test files fit 600;
all functions fit 100. This supplies no measured native gain or full original
public/raw/ETF/setup/profile parity. Independent Linux and the closed Fresh
physical/atomic publication remain required.

The original Replace request parent is implemented after genuine missing-API
RED. Its eight new tests pass with all 54 existing controls in the 62-test target;
strict library and target Clippy pass. One expected late-measurement diagnosis
was corrected from unchanged Writer source; every prior failure is preserved.
The three production files contain 244/161/131 lines, and all four test children
fit 600 lines and 100 per function. All old source bodies and the existing test
prefix are preserved. Actual written-source reciprocal review is closed without
blockers. ROOT-PARENT1-CHECK1 records the complete macOS workspace wrapper PASS:
487 Rust tests, four compile-fail docs and all source/dependency stages. The
62-test target overlaps that total. Physical atomic publication, original
public/raw/ETF/setup/profiles and independent Linux acceptance remain required;
the full-completion goal remains active.

The exact pinned policy-generation boundary now has a passing supplemental
regression with the original `generation-boundary` type and `public-v1` version.
It preserves maximum-generation decode, above-maximum refusal, migration
maximum-minus-one allocation and the exact exhaustion error, using an explicit
normalized FWPV/FWPS replacement for ETF. All 52 complete-policy controls and
strict target Clippy pass. Reciprocal review is closed; the full macOS wrapper
passes 488 Rust tests and four compile-fail docs with all source/dependency stages.
The ledger records 52 reviewed declarations, 53 counterparts and 217 assertion
links. This tests existing code and supplies no tests-first RED, legacy ETF,
original runtime/profile or independent Linux acceptance.

The original indexed-policy input normalizer is implemented after genuine
missing-API RED. Its six new controls pass with all existing tests in the
58-test policy target, including exact source whitespace, nested-list error
order, all thirteen rejection bodies and continuous bounded work accounts.
Both raw trees are checked before final owners are allocated; only accepted
unique keys are copied. A review caught and corrected one binary/atom fixture
mismatch. Three bare metadata observations were added after implementation
and are recorded as supplemental coverage. Strict target lint and the full
macOS wrapper pass: 495 Rust tests, four compile-fail docs and every source and
dependency stage. The ledger has 57 reviewed source declarations, 59
counterparts and 227 assertion links. Written-source architecture review
and performance review and their reciprocal review are closed without blockers.
The saved private-branch CI remains readable, but current SSH/HTTPS Git access
fails host resolution and generic GitHub API access fails to connect. A fresh
both-OS run has not started; the reviewed local checkpoint and implementation
work can proceed while current Linux acceptance stays pending.
Public replies, whole raw options, ETF/generation-zero readers, physical
installation/index/migration operations, original setup/profiles and current
independent Linux execution remain required. The full goal stays active.

The original-reader component follows an explicit architecture decision and
closed reciprocal consultation: a borrowed deprecated-retry-key predicate in
two production children and two children of the existing policy test target.
Its six real tests preceded production with clean missing-API RED. The implementation
preserves root atom-key shadowing, guarded state fallback and value-independent
old-key membership, with descriptor-only bounds and one continuous work account.
All 64 policy controls and strict target lint pass. The full macOS wrapper passes
502 Rust tests, four compile-fail docs and every source/dependency stage.
The production files have 84/178 lines and tests 236/323; all functions fit100.
Written-source architecture and performance reviews and their reciprocal
review are closed without blockers. The original generationless decoder fixture
remained unlinked at that checkpoint, and the ledger stayed57/59/227. The full raw
ETF/generation-zero reader is now implemented under
A-NEXT-M1-ORIGINAL-ENTRY1-CREATE1, with closed reciprocal consultation.
All six approved test files preceded production with genuine missing-API RED,
independently reproduced by the coordinator. The domain target passes72/72 and
ports65/65, including eight new reader and three reference controls. Actual source
architecture/performance reviews and reciprocal reads are closed without blockers.
The raw helper reads once; ports hashes once and moves its output Vec into the
existing prepared owner. Temporary reader indexes drop before borrowed bytes escape.
Strict target lint and the extended architecture control pass. Runtime repairs
preserve local/remote distinctions and captured/first-created function identity.
LOCAL, native atom indexes, dangerous empty-bitstring headers and zero-word
references are explicit operational profile exclusions, not invented native replies.
The existing source603 fixture has a supplemental direct assertion for its original
generationless shape refusal; both reviewers accept this post-implementation ledger
anchor. The checked ledger now has58 declarations,61 counterparts and232 assertion
links, with full runtime reconciliation and parity still incomplete.
The original ETF generation-boundary fixture now has a separately reviewed
counterpart for MAX generation, the complete returned policy bytes and MAX+1
refusal. The two-line direct refusal anchor supplements existing behavior;
all72 policy controls, strict lint and progress validation pass. Existing normalized
counterpart IDs and limitations stay unchanged. No original encoder, migration
allocator, public/setup/profile/physical or Linux result is inferred from this link.
Original ETF output, complete raw ingress, public/setup/profile/physical and
independent Linux acceptance remain required, with no measured native performance gain.

Original stored-policy Replace composition is implemented after genuine missing-API
RED independently reproduced by the coordinator. Eight new controls cover strict
envelope/generation read, CAS, then atom:type, reusing incoming-only validation,
migration allocation, late encoding and retention. The 73-test target, strict
domain/ports lint and private architecture control pass. Reciprocal architecture
and performance source review is closed without blockers. Existing V2, CorePatch
and standalone priorities and fee streams remain covered. The source-backed
first-install fixture now supplies an actual missing map entry; its independent
expected value is built before apply. This final test-only refinement passes
73/73, lint and formatting after the 523-test whole-workspace epoch.
The private V2 output remains an explicit semantic encoding, with original ETF
output, physical/public/runtime profiles and both-OS acceptance still required.
Original incoming retry/override normalization is implemented after independently
reproduced missing-API RED. Two production and two test children preserve full
defaults versus absent overrides, atom masking, keyword last values, source error
order and one fallible final target copy. All 80 policy controls, including eight
new tests and 1800 per-attempt normalizations, and strict target lint pass. Two
test-helper borrowing lint failures were corrected without changing any oracle.
The regrouped private checker preserves its old body and the 599-line parent;
the new purity/server-boundary control and strict tooling lint pass. Four new
counterpart records validate 59 source declarations/65 counterparts/252
assertion links, preserving every old record and leaving full parity incomplete.
The latest full wrapper passes all source stages and stops only at the default
RustSec GitHub DNS refresh. Reciprocal source and mapping review is closed
without blockers.
Remaining raw ingress groups, ETF output, public/physical/profile and both-host
obligations remain required. Original retention override normalization and
captured-limit diagnostics are now implemented after independently reproduced
missing-API RED. Six new tests pass with all80 previous policy controls; strict
target lint passes with no postproduction oracle changes. The pure normalizer
returns existing Copy fields without allocation, while the separate encoder
owns one fallibly reserved final Vec. Two narrow counterpart records validate
60 source declarations/67 counterparts/258 assertion links and preserve all65
older records. Captured-limit StateMachine/ETS and creation/inheritance source
assertions remain unlinked. That retention checkpoint's macOS wrapper passes539 Rust tests,
four compile-fail docs and all source stages, then stops at the default RustSec
GitHub DNS refresh. Actual source/mapping review is mutually closed without
blockers; no full-default resolver or generic term parser is added.

The separate Elixir bugfix worktree now has reviewed checkpoint106993c6 for the
existing TermCodec helper and its tests. A header walk rejects the reproduced
OTP28.3.1 empty-bitstring underflow before native decoding, and protectively
excludes zero-word references whose native outcome is unknown. Opaque matching
bytes and valid reference neighbors stay covered. All73 codec/governance tests,
selected formatting and strict compilation pass on macOS. Broader project,
physical-store and independent Linux validation remain open; the four unrelated
dirty files were preserved.

The exact pinned lease-takeover regression now passes in the existing lease
test target (10/10): lease 50 at 1000, reclaim 500 at 1050, stale-owner completion
and extension at 1060, and all three history kind/worker observations. This is
coverage of existing behavior, with no production change. Its original 588-line
file and 100-line test met the size limits. That increment recorded 44 reviewed source
declarations, 45 counterparts and 170 assertion links; public history-query
success, claim/reclaim selection, original setup/profiles and Linux execution
remain open. ROOT-NEXT-M1-LEASE-TAKEOVER-GREEN1 records the passing full script.
Written-source architecture/performance reciprocal review is closed without blockers.

The pure complete policy presentation now preserves type/state defaults, whole
raw governance presence, customer versions, indexed borrows and captured public
retention without a context history clamp. It verifies present V2 content once
and charges reserve/each state before producing borrowed descriptors. Seven new
controls follow actual missing-API RED; that increment's whole script passed 45 policy
tests and 454 total Rust tests after restoring the original All opaque-version
assertion and replacing nine forbidden fixture panics with same-message test
errors. Production and grouped integration files contain 296 and 532/185 lines,
within the 400/600 file and 100 function limits. That ledger had 43 reviewed
declarations, 44 counterparts and 158 assertion
links. The fifteen new scalar links preserve the pinned response case;
public set/get replies and hot-map refutes remain explicitly unlinked required
acceptance. Actual final receipts are in ROOT-NEXT-M1-PRESENTATION-GREEN1 in the
coordination note. Full public/setup/durable/profile parity and Linux execution
remain open; full written-source architecture/performance reciprocal review is
closed without blockers.

Earlier, the reviewed FIFO activation-drain counterpart was written and
passes with all19 tests in its claim target. Focused strict lint, formatting,
architecture27 and progress validation pass. The ledger has42 reviewed source
declarations,43 counterparts and143 assertion links; original setup/public/
durable profiles and Linux execution remain open. The charged replay gate's
shared implementation now passes all36 tests in its target after genuine
missing-API and old32PASS/new4FAIL stages. The full check script also passes on
this tree after a single semicolon correction required by strict Clippy.
Independent written-source architecture/performance review and mutual review
are closed without blockers. The next Fresh-only parent direction has separate
architecture/performance acceptance and completed mutual review. Complete code
now has exact architecture/performance contract selection and closed full
reciprocal proposal review. Its six controls and grouped private implementation
are now written after real missing-API RED and compiled old36PASS/new6FAIL S0.
The focused target passes42/42. That increment's full script passed447 Rust tests,
four compile-fail doctests, strict lint, architecture28, reference/progress and
all four dependency checks. Actual written-source architecture/performance and
full reciprocal review are closed without blockers, including reconciled design
notes and all narrowly recorded test/lint corrections.
Pure supplied views leave physical readers, atomic publication, public/original
profiles, full Elixir parity and Linux execution open.

The user explicitly reconfirmed Linux AND macOS support. Production changes
and acceptance must cover both. The authored CI matrix is ubuntu-24.04 and
macos-15; saved CI belongs to its older commit, and current Linux execution
remains open. Local Darwin/arm64 runtime, layout and timing evidence is scoped
to that host and cannot replace Linux tests or performance validation.

The user also requested a dedicated Elixir bug-fix owner. GPT-6.1-sol/max
`elixir_bugfix` completed the selected pure guard correction in the separately reviewed,
initially clean `/Users/yoavgea/repos/ferricstore2-elixir-bugfix` worktree at
the pinned66586c5 commit, on `codex/workflow-elixir-bugfixes`. Its findings and
future reproduction/fix review live in [the owned bug-fix note](reviews/workflow-completion-elixir-bugs.md).
The dirty original and frozen Rust reference baseline remain unchanged; no bug
is claimed reproduced or fixed by checkout preparation.

The Elixir owner constructed the reviewed EB2 fifteen-comparison probe and
four regression declarations in its isolated checkout. Independent inverse
checks recover every prior test byte. The first supervisor candidate refused
before a VM because Homebrew's reported Python alias differed from its pinned
literal identity; that historical refusal remains retained. A separately
reviewed origin correction, fresh preparation, syntax/archive and single
candidate order subsequently produced a real pure-projection RED: thirteen
of fifteen comparisons fail, while the two owned-running controls pass.
Root read the complete stdout/stderr and validated both complete records,
all1856 unchanged inputs and the gone process group. This does not execute
Mix, native storage, project setup or the four ExUnit declarations. Root has
separately assigned the reviewed LMDB-only nonempty-binary owner guard. The
owner wrote it; root's whole-source inverse recovers every original byte.
The reviewed GREEN validation source is now written in its existing carrier;
root read all540 actual lines and independently reversed all eight hunks to
every old source and carrier byte. Six current pins match; GREEN was absent
at preparation.
Its four-stage execution consultation and reciprocal source review are now
complete. Root separately recorded/read and executed the four distinct stages.
The sole GREEN candidate passes15/15 pure comparisons with outer/child exit0,
all1856 input rows unchanged and its process group gone. Root read all stdout
and788 stderr lines, both complete canonical records and freshly rehashed all
inputs. The thirteen original failures are corrected; all preservation controls
pass. This accepts only the pure guard on Darwin/arm64: the four ExUnit tests,
native/public execution, legacy rows and both-platform full profiles remain open.

The separately consulted EB-GOV-R1 mixed-reclaim regression is now authored
in that isolated checkout's existing governance correctness target at line409.
ARCH and PERF accepted the exact 69-line construction after complete actual
reciprocal review; ROOT separately issued the source-owner order before writing.
The owner and ROOT independently verified the entire 55516-byte resulting file
and the exact inverse restoring every prior 52959 byte. The test is constructed,
not executed. ARCH and PERF subsequently reviewed the entire actual authored
source, independently restored every old byte, read each other's complete
findings and closed reciprocal source acceptance without discrepancies.
Reservation-ID reuse across reclaim/normal/fill remains a source prediction. No production
repair or native startup follows from construction. Genuine unchanged-source
RED and later separately selected GREEN, with accepted real public/native
profiles, remain required independently on Linux and macOS.
The independent Elixir readiness review is saved in the owned bug-fix note at
line2281. ROOT read it completely and verified unchanged causal sources and the
test insertion inverse. Native isolated project startup remains required; the
completion/replay tail has a composed-assertion limit, while uniqueness455 is
independently sensitive. No native run or source amendment followed this review.
The captured-lifecycle/timeout-closure Rust batch has explicit architecture
creation decisions and separately actual-read coordinator assignments. Its
twelve controls produced real missing-interface compile RED before the new
APIs. The grouped modules, approved test extractions and A-ACT12 guards are now
implemented; root, architecture and performance completed the stable source
and reciprocal reviews. Performance independently passed200 Rust tests and
one doc test on its853 unchanged pre-rename inputs. A subsequent two-file
local-binding rename repairs the purity checker's collision with local `core`;
exact whole-file inverses preserve every expression and real `.core()` call.
Current formatting, workspace policy and strict domain Clippy pass.
The unchanged restored-FIFO control then produced an actual 0/1 behavior RED
before the optional-due guard correction; the first twelve-control run passed
12/12 with all22 scoped inputs unchanged during each run. Root read both complete
frames. Those local results belong to the recorded draft: FIFO authority/order
and full work/copy accounting still need correction and stable peer acceptance.
Shared-file communication and reciprocal review continue through the owned notes.
The independent unresolved-running-parent control subsequently produced a real
0/1 ordinal RED, then the same oracle passed1/1 after the logical-state comparison
fix; root independently verified all821 bindings, with only parent.rs changing
across that pair. A later body passed178 domain Rust tests and one compile-fail
doc with821 unchanged inputs during the run. Parent-cycle and oversized consumed
history-key controls subsequently produced an actual10/12 behavior RED before
the two guards. First GREEN passed12/12; the final expanded batch passes200
domain/types Rust tests and one compile-fail doctest, with821 unchanged input
bindings during each run. Owner strict Clippy, documentation and formatting
pass. Exact visit/link/step/history-delete, deep cancellation, FIFO refetch and
sequence/ordinal controls are implemented. The stable accounting/body handoff
has independent acceptance. Four approved test files also
changed between that RED and first GREEN; root and performance independently
verified exact inverses preserving the approved alias/unit/lint assertions.
The exact five-path generic indexed None-due FIFO batch has architecture
selection, performance consultation and root's separate owner assignment.
All five controls preceded API changes: actual compilation reports only two
missing Option-method errors; after the Option-only seam, the twelve-title
target passes11 and fails1 because Cancel omits its real FIFO before row.
Later controls are not reached by that failure. Root read both full frames
and both854-row unchanged maps. First GREEN passes12/12 and the affected
four-target regression run passes20/20. Three oversized control functions led
to two reviewed private helper extractions with an exact515-line donor inverse;
a separately reviewed scalar assertion refinement preserves every expectation.
Actual formatted functions now fit100 lines and the child fits515/600 lines.
The final owner run passes200 Rust tests plus one doc test, with strict Clippy,
documentation, formatting and workspace policy checks all passing on the same
854 unchanged inputs. Architecture and performance completed actual reciprocal
body/inverse/resource reviews. Performance separately passed200 Rust tests and
one compile-fail doctest on the same current bindings; root read all444 non-map
raw lines, both complete maps and freshly rehashed every bound input. These are
local Darwin/arm64 results. FullM1, Linux execution and complete acceptance on
each platform remain open. The implementation owner has published the concrete
scheduler/running-credit planning predecessor proposal. Complete architecture
creation selection, performance source review and actual reciprocal reads now
precede a separately recorded ROOT tests-first creation/check order for the
exact grouped ten-path batch. All twelve controls preceded production APIs;
the retained tests-only RED contains four missing-import errors. A provisional
12/12 GREEN and a real singleton-coalesced block11/1 behavior RED are published.
The reviewed resolver split meets file limits. ROOT read the final212 domain/
types tests plus one compile-fail doctest, strict checks and both complete864-row
unchanged maps, and freshly rehashed every bound input. The final stable handoff
and complete inverse/environment receipts are published. Independent source/
resource reviews are complete. Architecture separately passes12 coordination
controls and one workspace check; performance passes212 domain/types Rust
tests plus one compile-fail doctest on the same unchanged864 inputs. These
overlap the owner checks and add no original-case coverage or measured gains.
The first architecture receipt-wrapper failure is retained; its reviewed
readback correction permits only the separately successful second command.
Unavailable historical initial-child header bytes remain explicitly unresolved,
separate from acceptance of the fully reviewed current body. ROOT's current
whole-workspace check passed360 Rust tests and four compile-fail doctests,
then stopped at the normal RustSec audit because github.com cannot resolve.
These local Darwin results grant no Linux acceptance.
Full authoritative governance
decoding, identified credit owner protocol and the atomic public claim parent
remain required downstream.


The current whole local ordinary wrapper, after Complete Replace and its
equivalent test assertion correction, passed **392 all-target Rust tests and
four compile-fail doctests**, zero failed/ignored/filtered, plus formatting,
locked compilation, strict workspace Clippy, warnings-denied documentation,
architecture, reference integrity and progress. The additional 26 architecture
executions overlap and are not added to 392. The real wrapper exit remains 1:
normal RustSec fetching cannot resolve github.com. No audit skip was added.
ROOT read all 987 raw output lines and independently reacquired the five
affected whole source/test bindings unchanged across the run. This ordinary
check provides no whole-workspace unchanged-input map or bounded resource
certificate. Native evidence is Darwin/arm64, Rust 1.99.0/default features;
Elixir 1.19.5/OTP 28 is the tooling profile. Current Linux execution, full original
Elixir 1.20/NIF/public/profile acceptance and complete dependency audits remain
open. The preceding 386+4 run belongs to the pre-Replace source tree.

The latest whole local ordinary wrapper, after the reviewed normalized V2
codec and class-limit correction and before the Complete Replace increment, passed **386 all-target Rust tests and four
compile-fail doctests**, zero failed/ignored/filtered, plus formatting, locked
compilation, strict workspace Clippy, warnings-denied documentation,
architecture, reference integrity and progress. The extra26 architecture
executions overlap and are not added to386. The real
wrapper exit remains 1: normal RustSec fetching failed because github.com could
not resolve. No audit skip was added. ROOT retained the entire native output,
command, clock observations and real exit in the coordination record; this
ordinary check has no877-input unchanged-map or bounded resource certificate.
Its Elixir 1.19.5/OTP 28 tooling profile does not execute the required original
Elixir1.20/native cases. Fresh GitHub connector discovery returned repository
not found for the private Rust branch; no current Linux CI job was executed.
Linux and full product/original/public/profile acceptance remain open.

The preceding saved whole local run, after the coordination batch, passed **360 all-target Rust tests and
four compile-fail doctests**, plus formatting, locked compilation, strict
Clippy, warnings-denied documentation, architecture, reference and progress.
All864 scoped input hashes, including frozen references and policy/ADR inputs,
stayed unchanged. Its retained complete977-line non-map frame exits1. It stopped
at normal RustSec fetching because this environment cannot resolve github.com.
The reviewed cache repair uses ignored `.tools/advisory-dbs` and preserves the
audit policy. No audit skip was added; the full wrapper remains incomplete.
The preceding whole-wrapper purity failure remains retained; the reviewed
two-file local rename resolved it. The earlier348+4/853-input run and
336/820,322/808,296+4 and242+4 passes
and saved CI below belong to their recorded trees. This current local run is
Darwin/arm64 evidence; Linux current execution and both complete audits remain open.

Architecture/performance selected A-OPT1's two existing refund regions. ROOT
separately assigned the existing R3/R1000 value/order/borrowed-pointer controls
on unchanged production, then one fallible upfront reservation and focused
before/after plus strict checks. The two existing regions are implemented;
the strengthened unchanged-production baseline and final focused26 tests pass,
with strict Clippy, documentation, formatting and workspace policy. ROOT read
the complete current/original bodies, raw receipts, four864-row maps, full
inverses and fresh current inputs. Architecture's complete reciprocal source/
receipt review accepts both existing regions; ROOT actually read its closure.
The preceding360+4 whole-tree result does not certify the changed refund tree.
One source reservation invocation instead of R is not a measured allocation,
capacity or latency gain. Both platform results remain independently required.

The stable claim/domain regression set passes83 controls, and the current private
codec set passes33, including five decoded coherent-claim controls; the existing
application set now passes20 (old14 plus six caller-identity controls); domain/types
pass188 plus one compile-fail doctest, including eight new caller-normalization
controls. That V1 caller/intent batch passed strict Clippy, documentation and formatting.
Their independent source/resource acceptance is complete: architecture and
performance each passed8 caller+20 application+1 workspace checks, with828 and826
unchanged input bindings respectively and actual reciprocal publication reads.
Original caller arrays may coexist with the normalized spines because their
array/leaf lifetime is shared; no native allocation or speed claim follows.
Architecture and performance each independently reran
all33 codec controls and the exact workspace policy target, with280 and276
unchanged input bindings respectively. Earlier independent56 parity-tooling,
12 installation and22 codec runs retain their original narrower scopes. The grouped installation target
preserves all ten original bodies; mode-null and replacement-null source
corrections are reviewed. Static compiler controls preserve original capture/
accounting and do not execute original scenarios. Existing-final publication
refusal previously passed12 artifact controls in owner and independent runs.
The current static wrapper's shared-budget JSON intake and private final
publication observer pass syntax checks. The current artifact target passes21
synthetic controls in owner and independent domain runs, including exact diagnostic
byte limits and cleanup-result source corrections, with unchanged14 scoped inputs
and independent architecture/performance source review. These controls do not
execute the collector or original setup/tests. Successful original run/observer coverage
remains open. The isolated
reference-valid missing-capture correction passes56 parity controls in owner and
independent runs, with a separate performance source review. Architecture's
earlier directory/visibility audit found229 unique Rust files within400/600-line limits;
the current new caller feature modules and grouped integration targets also pass
the enforced limits.
The separately supervised direct cached-runtime preflight now passes after
source-reviewed API and package-metadata corrections:39 specifications,40 code
paths, six fresh reviewed modules and the exact test/host configuration are
bound to one accepted observation. Its generated data path stayed absent,
no ExUnit modules or product applications started, and all1912 input/provenance
rows stayed unchanged. Its process group is gone. Earlier failed attempts are
retained, including the normal Mix.PubSub loopback-socket denial. This is a
cached-load/config preflight under Elixir 1.19.5/OTP 28, below the required1.20
source profile; it executes no original tests/helpers/setup/collector or
publication and does not advance workflow coverage.
Fresh read-only discovery also found concrete installed Elixir1.20.4/OTP29.0.5
roots and bound selected arm64 native/module bytes. The exact A-SUP2 core-only attempt passed Python syntax,
then failed because its initially-unloaded-crypto assertion found the installed
crypto module already loaded. Its process group is gone; all1848 bound inputs
stayed unchanged, and no validation frame or core acceptance was produced.
The retained failure remains distinct. After the reviewed one-line amendment,
exact fixed-launcher syntax/archive and a separately assigned single A-SUP5
attempt, the installed Elixir1.20.4/OTP29 core observation now passes: one valid
frame, exact four module disk/loaded proofs, crypto/JSON controls, unchanged1848
inputs and owned process group gone. The complete retained raw result was
actually read by root. This certifies installed core only; no project/source/
config/NIF/helper/setup/original scenario or source profile is accepted by it.
Public claim
caller normalization and typed intent comparison are implemented and pass the
owner checks and independent acceptance above. Private claim-phase
apply and public comparison do not establish public parent orchestration.
The parity owner has published the complete R-MIX1 supported-project readiness
proposal; architecture/performance completed reciprocal SOURCE review and
ROOT separately assigned only the complete inert existing-note source draft.
After the concrete three-part fit consultation, A-MIX1c/P-MIX5 and a separate
actual-read ROOT order select complete Inputs/Execution/Entry inert note drafts
sharing one namespace and budget, replacing the uncreated two-part shape.
Each must fit32768 bytes/600 physical lines, aggregate98304/1800; complete
publication is now bound: Inputs22491/375, Execution32757/579 and Entry31973/552,
sum87221/1506. ROOT actually read all three complete bodies and verified exact
carrier/prefix/object/V2 custody as DATA. Architecture/performance completed
the actual-body review and actually read each other's complete findings.
The three selected amendments cover prepaid source ownership once, the
complete protected0700 directory chain, and separate frame graph, encoding-owner
and actual wire limits. ROOT read PARITY's complete response and the architecture/
performance selection of its exact changes and two existing responsibility moves.
The complete amended inert bodies are published: Inputs29321/489,
Execution27864/498 and Entry31937/552, sum89122/1539. ROOT and both peers actually
read all three complete R1 bodies. Their reciprocal review found five error
sentinels initialized after descriptor acquisition. A-R-MIX1-R1-SENT1 explicitly
approved moving those existing initializations before acquisition in place.
ROOT applied only those five adjacent line reversals, four current SHA fields
and one3086-byte amendment; every359345-byte predecessor byte and all source
lengths/LFs remain unchanged. Whole-note forward/inverse and protected full
readback pass, and ROOT actually read every corrected body. The carrier462653
bytes and all source caps fit. Architecture and performance independently read
the complete corrected sources and accepted the full reciprocal SOURCE review.
Loader, seed and runtime remain unassigned. This inert
Darwin/arm64 readiness draft supplies no Linux acceptance. Separate Linux
origins and independent execution on both platforms remain required.
The G1a normalized complete-policy contract has final reciprocal source approval
and a separate tests-first assignment: all twelve test bodies exist before the
production APIs. Architecture/performance approved nine private test-helper
extractions, preserving the complete original donor before the separate22-slot
and full-value strengthening. ROOT independently read all final source bodies
and verified both complete donor inverses, final files within600 lines and
all25 functions within100. Complete reciprocal source/inverse reviews now
accept the final test bodies and both preservation stages as SOURCE. Read-only
implementation preparation subsequently found five same-value assertions whose
assert_eq! macro requires Debug on existing LimitEnforcement. Their narrow
equality-assertion correction is implemented and formatted in the two existing
test children. ROOT and both reciprocal reviewers accept the complete actual
source and whole-file inverses. Existing production traits remain unchanged.
The old check wrapper's complete bounded source was
not retained, so its receipts cannot authorize reuse. ROOT has now published
the complete inert replacement24958/478. Complete reciprocal actual-body review
found two allocations outside descriptor protection. The separately selected
correction is now published as24964/478 in a unique V1-R1 fence; its full inverse
recovers every original byte. Complete corrected-body reciprocal review accepts
the source. After the corrected tests and actual-source reviews completed,
ROOT invoked the exact retained body through the separately approved isolated
heredoc. The focused Cargo command returned the expected four E0432 imports of
the missing complete-policy module. Complete raw diagnostics, source owners,
868 frozen before/after rows and append/close/readback establish genuine
pre-interface RED on Darwin. This is no executed assertion or passing test.
The finite validation account remains monotone: actual failed reads and every
later assigned component remain paid without reset. The reviewed nine-file
complete-policy implementation and sole module seam are implemented; production
files fit400 lines and functions100. Original donor inverses, four reviewed
private helper extractions, metadata scratch and retry descriptor corrections
preserve the design. Owner, ROOT, architecture and performance completed their
actual source and reciprocal reviews. No extra production path was created.
The bounded R1 receipt remains FAILED on missing test-crate documentation.
Separate DOC1/CLIP1 corrections and fifteen test-only lint corrections retain
their complete source proofs and historical failed ordinary attempts. Three
E0282 diagnostics then led to eight explicitly typed borrowed-empty expectations;
EMPTYTYPE1 completed owner/ROOT whole-source inverses and reciprocal approval.
The fourth ordinary Darwin attempt passed12 complete-policy controls and224
unique domain/types Rust tests plus one compile-fail doctest, strict Clippy and
warnings-denied documentation, on resolution17736/e71de238. Its formatting
failure and architecture NOT_ATTEMPTED remain retained. The reviewed FMT1
correction changes only one assertion's whitespace; ROOT proved the complete
inverse and token identity. Fresh formatting and workspace architecture checks
then passed on resolution17758/509/4d7db848. Current test children fit539/509/537
lines and functions91/94/87. This accepts a source-preserving staged local
validation; the six families did not all execute in one attempt or on one hash.
No R1 retry, new grant/reset, bounded resource certificate, measured gain,
Linux execution or full Elixir/native/public/profile parity is claimed.
A-V2VAL1 now approves the standalone normalized VALUE codec in the pure
complete-policy domain implementation: three move/borrow APIs, nine named
private source files and three private test children with twelve independent
controls. The exact API/grammar/ownership contract preserves current G1 and V1;
compound request framing, intent, hash and install authority remain separate.
The reviewed narrow responsibility amendment is saved in ADR006 and architecture.
DOMAIN authored all twelve independent literal/malformed/accounting controls.
ROOT recorded actual API compile-RED (three missing-import errors), then the
approved guard-only scaffold produced genuine behavior RED: twelve existing
policy tests pass and twelve new codec tests fail, with no ignored cases.
The nine approved codec bodies are now authored and independently reviewed.
Architecture, performance and ROOT found three reversed circuit error-class
limits. ROOT's independently specified count257/1000 and byte256/257 regressions
produced genuine behavior RED:24 tests pass and the two regressions fail.
DOMAIN corrected only the three reviewed argument pairs to count1000/name256;
the unchanged focused target then passed26/26, including the2006/P0 pre-payload
refusal. ROOT and architecture verified complete source inverses. Performance's
maximum-size late-refusal and ordered-key supplements preserve earlier oracles;
these post-authoring regressions do not retroactively add preimplementation TDD.
A one-expression test byte-literal spelling correction then passed strict
Clippy without changing either tested byte. The whole current local wrapper
passes386 Rust tests and four doctests with strict checks, but still exits1 at
normal RustSec fetching because github.com cannot resolve. Linux execution,
full original Elixir profiles and compound/public authority remain open.


A-M1-CREPLACE1 adds normalized complete replacement preparation and a borrowed
installation decision. Architecture/performance selected the exact APIs and
two private helpers before authoring; the six independent controls preceded
production behavior. The grouped test extraction preserves the entire donor
and all assertions within600/100 lines. The initial missing-import compile RED
is separate from the scaffold's real26-pass/six-fail behavior RED.
The unchanged focused target now passes32/32 on Darwin/arm64, including all six
replacement controls. Stored/CAS/current/new/late-high-water priority, omission
reset and present-empty values, generation correspondence and the same
nonrollback schema account are exercised. ROOT read the entire190-line final
source, proved the complete scaffold inverse and exact1139-byte unchanged
wrapper/getter region; every function fits49 lines or fewer. The owner's focused run used the unchanged facade and three test inputs.
ROOT's first broader check then found one manual_assert_eq diagnostic; an
equivalent assertion macro correction preserves both operands and all expected
values. The corrected child fits 258 lines; its whole inverse recovers every
prior byte. The replacement-wrapper run passed 392 Rust tests and four doctests
with strict checks, then stopped at normal RustSec DNS fetching. A-M1-FINAL1 and
P-M1-FINAL1 independently accepted all five current source bodies, complete
inverses, resource fees and physical limits, and read each other's full results.
They reviewed the focused raw receipts; the broader current wrapper result is
attributed to ROOT's complete capture. This pure replacement plan supplies no
atomic storage/public authority or measured native gain.
A-M1-OUTER2-I1 and complete PERF reciprocity approve the exact caller/intent
Stage I batch. Eight tests occupy four grouped integration files; all eighteen
functions fit 100 physical lines and all files fit 600 (largest 435).
ROOT and both reviewers read every final source, checked five fixed golden
fixtures and complete helper/donor/call/format inverses, and preserved the six
baseline controls plus 21 scalar cases and 40 local presence outcomes.
Selected FIT2 extractions closed FMT1's four function-size failures; FMT2
changed no bytes. Native ownership, Drop and allocation gains are unmeasured.

The fresh eight-test missing-API compile RED ran on Darwin at 01:55:37–38 UTC,
with six E0432 errors (exit 101) and zero executed test bodies. The separately
selected A-M1-OUTER2-I1-S0 scaffold is now written: three new source files and
two facade seams, with both preparation functions rejecting InvalidAuthority.
ROOT independently read all five production sources and verified whole facade
inverses and unchanged test sources. Fifty production functions fit 100 lines
(largest 38), and all five files fit 400 (largest 308).
At 02:06:39–49 UTC the scaffold compiled and all eight focused tests executed
and failed (exit 101). This is behavioral RED; later successful scalar/presence
outcomes were not reached. A/P scaffold review and complete reciprocal closure
are now finished. The separately selected I-M1-OUTER2-I1 owner order assigns
five domain paths to DOMAIN and three ports paths to ROOT. All eight sources
are implemented and frozen. The eight focused tests passed on Darwin; the private
infallible SHA finalizer was explicitly selected in FIN2 after a real Clippy
failure. Four later test style corrections preserve all scenarios and values.
Before the core Patch increment, format, strict Clippy, all 27 architecture tests,
401 workspace Rust tests, four doctests and strict docs passed on the accepted
Stage I body. Those results predate the current Patch work. Architecture tests
overlap the workspace count. ARCH and PERF accepted the complete 14-file final review and the
accounting dependency; their full direct reciprocal review closed normally.
PRIVATE1 explicitly accepts the actual private visitor refinements after body
review. The current native receipts are Darwin only.
On that accepted Stage I body, all 112 production functions fit 100 lines
(largest 43); files fit 400 (largest 308), and tests fit 600 (largest 435).
Linux native acceptance, full public
workflow parity and measured performance remain open.

ROOT's additive notes and dependency test have complete reciprocal source
acceptance and two focused Darwin passes. Stage F FWCP/frame/restore/models remain
held. Raw Patch/ETF/full-source identity, coherent parent installation, all original
profiles, current Linux and macOS execution and later ordered milestones remain open.

The pinned generation-test review retains all 14 source cases as broader
acceptance work. The normalized core Patch increment has a fully reciprocal
architecture/performance contract and six tests in grouped private children.
ROOT read all six actual authored sources; the largest test file is 585 lines
and the largest test function is 99. The actual Darwin missing-API compile
RED had four E0432 errors (exit 101). After the exact rejecting scaffold and
facade seams, the focused target compiled: 32 existing tests passed and all
six new tests failed (exit 101). These tests-first failures preceded BODY1.

DOMAIN implemented and froze patch.rs, its private patch/merge.rs child and
the candidate-only decoder factor after the named CPATCH1/FIT1 decisions.
ROOT's separately recorded RET1 ownership transfer applied only four borrowed
retention-selector ampersands. Two later existing-test style corrections
rename a local variable and make a Vec clone explicit; all scenarios,
assertions, expected values and prior whole-body inverses remain intact.
The earlier strict Clippy production and test failures remain recorded as
failed; no lint was suppressed. The public decoder's charged encoded-copy
suffix is unchanged, proved against every byte of the original decoder.

On 2026-10-11 the corrected current body passed Darwin formatting, strict
workspace/all-target Clippy, all 27 architecture tests, all 407 workspace
tests across 67 target summaries, four doctests and strict documentation.
The workspace count includes the 38 focused policy tests (32 original plus
six new) and architecture tests; overlapping runs are not added. No test was
failed, ignored, measured or filtered. Reference integrity passed for 480
pinned files and 5858 textual declarations. The validated progress ledger
remains INCOMPLETE: 5903 registered runtime cases, 4222 linked, 1681 unmatched
and 1655 unresolved declarations. These checks do not certify execution of
the original cases or full behavior. The full check.sh wrapper, exact Elixir
tooling profile and RustSec/network acceptance remain open.

ARCH and PERF completed full independent corrected-source review and actual
reciprocal review of their complete findings, then returned normally. Both
accepted the typed CPATCH1 production and six-test batch; no source blocker
remains within that scope. ARCH and PERF completed full actual reciprocal
review of DOMAIN's next original partial-Patch caller/intent contract and returned
normally. A-NEXT1-CONSULT selects its exact three paths, immutable owners,
helpers, registry, same-account schedule and eight independent byte/hash oracles,
plus exact additive ADR005/006/architecture text and an existing checker block.
One documentation phrase was corrected because the typed core has no Boolean
payload. ROOT retained every selected addition and verified exact existing-byte
inverses. The following tests-first batch preceded the now-written reviewed production
artifacts and exact selected documentation/checker additions.
DOMAIN's complete original six-test draft has 763 physical lines and functions
of 90/66/146/119/169/120 lines; its unchanged original source remains retained.
ROOT, ARCH and PERF read all six bodies. A-NEXT1-TEST-LAYOUT selects exactly two
new test files, ten private helpers and the existing main's 16-byte module seam.
Both reviewers completed actual full reciprocal review without discrepancy.
All 53 independent literal/hash records, full immutable caller observations,
eight same-account rows and original duties are preserved. The authored paid
models explicitly vary the captured history limit and compare distinct partials
with independently checked equal complete VALUE results; stamp observations
are explicit. Original 101-line function and 608-line file/106-line helper
formatter failures remain recorded. Meaningful checked-byte and readonly locals
resolve them: actual ordinary parent/child formatting gives 583/594 physical
lines, maximum function 99, with no limit waiver or additional helper.
I-NEXT1-TESTS1 released the exact tests-only owner order. DOMAIN wrote the two
files and main seam; ROOT's full authored readback matches every reviewed raw
byte and its owned ordinary formatter output. The genuine locked/offline target
compile on Darwin failed with four E0432 missing-API errors (exit 101), before
any production implementation. Zero test bodies/assertions executed. Post-run
source bindings and git diff --check passed. Complete A/P-NEXT1-S0-CANDIDATE1
reciprocity then selected the exact five-file rejecting scaffold; ROOT's
I-NEXT1-S0-1 owner order preceded its writing. Whole authored-source inverses,
actual owned formatting and all six frozen test files were independently checked.
The next unfiltered locked/offline Darwin run compiled and executed all 14 tests:
**eight existing controls PASS, six new controls FAIL** with InvalidAuthority,
exit 101 and zero ignored/measured/filtered. All eleven source/test bindings
remained unchanged across that run; git diff --check passed. These six failures
stop at initial positive preparation; no full 53-oracle/model/account reach or
GREEN is inferred. I-NEXT1-BODY1 then wrote only the already consulted two production children,
private measured view, nine domain/seven ports helpers, header factor,
visibility-only hasher sharing and real entry bodies. ROOT independently read
all seven full bodies, checked ordinary formatting and ran genuine Darwin GREEN:
all 14 controls PASS. The selected three additive documents and existing checker
block are written with whole prior-byte inverses. Strict Clippy found 13 style
sites only in the two new test modules; exact fixes preserve all original
literals, assertion operands, controls and complete pre-fix body inverses.
Their ordinary formatted sizes remain 583/594 lines, with no lint allowance.
Clippy, workspace formatting, check, strict docs and four compile-fail doctests
pass. The BODY1 corrected-source workspace run passes 413 named tests across
67 targets, including 27 architecture controls; the separate architecture run
also passes 27, which overlap rather than add to the workspace count.
All 19 scoped source/test/doc/checker bindings remain unchanged after execution.
ARCH and PERF completed independent body/test/document reviews and actual
reciprocal review, including the exact 13 test style edits; no blocker remains
within that scope. Full original raw/public/profile parity and current Linux
execution remain open.
The existing CI matrix covers Ubuntu24.04 and macOS15; its configuration does
not demonstrate a current Linux run. Linux executor access remains unresolved.
Progress reconciliation remains explicitly incomplete: 5903 runtime cases,
4222 linked, 1681 unmatched and 1655 unresolved declarations. These typed core
controls do not certify full Elixir behavior or complete M1/product parity.
Separately selected Stage F binding and the ONE retained gate/Fresh-only remerge
remain mandatory successors.
Raw empty-list resets, aliases, ETF, generation zero/bignums, full source
identity, public replies, routing, concurrent atomic installation and every
required profile remain separate. No complete original source case or Linux
acceptance is closed by these typed-core tests.

This ordinary implementation scope neither retries or repairs the permanently
failed bounded R1 nor resets or extends its finite G1 validation account. Each
admitted complete-policy operation requires the caller's same mutable schema
charge, whose P continues to describe canonical scratch. Full simultaneous Data/value/frame/
scratch/parent accounting needs a separately approved resource boundary; ports'
FWCP/header/copy/hash work is not paid or certified by standalone VALUE acceptance.
Unknown ETF/hash, legacy generation, customer version and original source parity
remain gates. No public debit/count/attestation, free copy, hidden owner field,
second account or backward dependency is authorized.
Linux and macOS are both supported targets and independently require execution.
Portable core behavior, dependencies and optimizations must preserve both;
platform-specific work belongs at the existing adapter boundary. Linux execution
and full macOS product/native/original/public/profile acceptance remain open.
Original project callback origins, configuration, live PubSub, ordinary locks,
shared bounded ownership and failure/cleanup bodies require complete actual
draft review before any separately assigned stage or execution.
No project callback/build/collector ran in this proposal phase. Fresh native
builds, original setup/tests and Linux AND macOS acceptance remain required.
CI tooling setup is authored; current Linux/macOS execution, successful original
publication, bounded original collection/setup, supported source runtime/profile
execution, complete M1 and later milestones remain open. Exact scopes, real RED/GREEN,
precreation decisions and source/profile gaps are in the active coordination
record. These results do not replace the saved commit/CI provenance below.

The separately reviewed A/P-PARITY-DUE1 increment adds the original future-due
boundary case (invariant_test.exs:1291–1321) to the existing claim_apply target.
Actual written-source review confirms all four mappings and their explicit
public/durable/profile limitations. All 15 claim_apply tests pass on Darwin.
The preceding workspace run passed 414 named tests across 67 targets, with zero
failed or ignored; strict Clippy, formatting, 27 architecture controls and the
progress validator also pass. Only the selected progress declaration changed;
all other 5,857 declarations and registration/manifest metadata are preserved.
This covers existing typed behavior and supplies neither a TDD RED nor complete
original-case/public/profile parity. A-NEXT-F1-CREATE1 now approves the four
named frame-verifier paths and one-scan contract after full ARCH/PERF reciprocity.
The exact additive ADR006/architecture sections and existing checker amendment
are written; all 27 architecture controls, strict Clippy and formatting pass.
A/P-NEXT-F1-TESTLAYOUT1 and separate TAG1 now close the full six-test layout,
one additional private test child and six named helpers after actual reciprocal
review. ROOT wrote only the two tests and their private parent module seam;
ordinary sizes are 488/324 lines with all functions at most 95. The first real
compile failed with three missing-API and ten test hash-rendering errors. A
byte-equivalent fixed-digest comparison correction preserves every expected
value and observation. The corrected target failed solely with three E0432
missing-API errors, before production implementation; zero assertions executed.
Separate A/P-NEXT-F1-S0-1 reciprocal approval and I-NEXT-F1-S0-1 owner order
then selected the two rejecting sources and five exact facade seams. ROOT wrote
them and ran the unfiltered target: all 20 tests executed, old14 PASS/new6 FAIL
(074caa, exit101); ordinary formatting passes. No later assertions after each
test's failure are credited. The earlier 414-test passing workspace precedes
these new tests and is not a current workspace GREEN result. Full FIT1 source
reciprocity and I-NEXT-F1-BODY1 preceded the substantive sources: DOMAIN wrote
D1/D2 and the three Reader/module/constructor seams; ROOT wrote P1. The same
unfiltered focused target now passes20/20 on Darwin (d9a807, exit0). Ordinary
formatting wraps only three Reader signatures; final format/diff checks pass.
The private scanner's unchanged tenant comparison is in public scan before
SHA admission; the domain pair fits245/245 lines with largest functions37/94.
Strict Clippy first failed on private restore_header's by-value argument
(f276c2, exit101). Reviewed CONSUME1 moves that same private helper into the
existing Scanned impl as consuming self; every paid event remains unchanged.
A second strict attempt found four test-fixture implicit-clone expressions;
the exact explicit-clone corrections preserve every input and assertion.
Final strict Clippy passes (54e2be, exit0). At the completed frame increment,
the macOS workspace passed420 tests across67 targets (88917d, exit0), including all20 focused tests
and27 architecture controls; four compile-fail doc tests, warnings-denied docs,
formatting, diff, reference integrity and progress checks pass. No tests are
ignored. ARCH and PERF accepted all final written production/test sources and
actually read each other's complete findings; full reciprocal source review is
closed without discrepancies. At that checkpoint the progress ledger had
38 reviewed declarations/39 counterparts/128 links, with1681 unmatched runtime
cases and1655 unresolved declarations.

The subsequent A/P-PARITY-LEASED1 counterpart covers the original leased/terminal
claim case (invariant_test.exs:1246–1289) in the existing claim_apply target.
ARCH/PERF actually read each other's complete findings and closed reciprocal
review without discrepancy; ROOT verified written bytes match their exact
accepted candidate. The test preserves real creation, the selected claim write,
leased-empty selection, completion with the observed token/fence, and terminal-
empty selection at the original times/limits. Actual unfiltered claim_apply
passes16/16 on macOS (141527, exit0). At that checkpoint the full workspace passed421 tests
across67 targets (0b491e, exit0), with zero failures, ignored or filtered tests;
strict Clippy (8ab4b6), formatting (936a23), diff (174acd), reference (6267d3) and
progress (0a80ea) checks pass. The ledger now has39 reviewed declarations,
40 counterparts and133 links. The other5856 declaration rows and all registration/
manifest metadata are preserved; the prior future-due counterpart changes only
its shared-file digest and four Rust assertion line offsets by2. Runtime gaps
remain1681 unmatched cases/1655 unresolved declarations. This is executed typed
existing-behavior coverage, without a retroactive TDD RED or full original/public/
durable/profile acceptance. The complete LIMITS entry preserves those gaps.
The subsequent original core-Patch frame producer has explicit A-NEXT-F2-CREATE1
selection and complete reciprocal proposal review. Three private test children
precede production: actual missing-API RED reports three E0432 import sites for
two absent APIs (a740e9, exit101, zero assertions). Separately selected rejecting
S0 then passes the existing20 and fails the new6 solely with UnsupportedTag
(da8411, exit101). I-NEXT-F2-BODY1 writes the exact eight approved existing
production candidates; no new production file/model/dependency is introduced.
One private CorePatchSink visits the same original Caller for intent SHA and
frame emission, preserving partial identity without an isolated intent buffer.
Each prospective payment stays distinct on the actual received schema account.
The unchanged unfiltered focused target passes26/26 (134658, exit0).

Strict Clippy first reports three test-style diagnostics only. An inclusive
single-element range and two semicolons fix them; the exact inverse preserves
every prior test byte and all fixed expectations. Final cases.rs remains595
physical lines/max function91, SHAadceec30531b084fcf92767613384e10b5d33db9cb2fe4b38f021675888d898c.
Final strict Clippy passes (fd7f83, exit0). At that producer checkpoint the macOS workspace
passes427 tests across67 targets (275d50, exit0), zero failed/ignored/filtered,
including the final26 focused and27 architecture controls without adding them
again. Four compile-fail doctests (3faf94), warnings-denied docs (fca084),
formatting (569353), diff (4eab2f), reference (a759d5) and progress (e6f9bf)
pass. The ledger remains39/40/133 with1681 unmatched cases/1655 unresolved
declarations; these six internal producer tests add no original-case mapping.
ARCH and PERF accepted all eight complete written production sources, all three
complete current test children, the three final style expressions and both
documentation additions. Each actually read the other's entire findings;
written-source reciprocal review is closed without discrepancy. Their source
review and ROOT native evidence remain distinct; neither claims a native gain.

The consuming original-command decoder now has explicit A-NEXT-F3-CREATE1 and
complete A/P proposal reciprocity. Seven test-only paths precede production:
missing-API RED (6d2d98, exit101) executes no assertions; separately approved
uniform-refusal S0 passes old26 and fails new6 (43f7fa, exit101). The substantive
decoder preserves original partial presence and moves immutable header owners
while freshly paying reads/comparisons/copies on the received schema account.
First compilation catches private combinator lifetime inference; a closure
passes32 tests but strict Clippy rejects it. A-NEXT-F3-LIFETIME1 explicitly binds
the callback to the received Reader's byte/charge lifetimes and restores the
method item without suppression or changed expectations. Final strict9c4ada
passes. ARCH and PERF read every selected written source/test and both final
documentation sections, accepted the exact batch, and actually read both entire
parts of each other's findings. Written-source reciprocity is closed without a
discrepancy; native evidence and unmeasured resource limits remain separate.

The two separately accepted PARITY-PARALLEL-ACTIVATE2 counterparts preserve
default-parallel two-ID selection and FIFO activation after real fixture
creation, with actual writes/control carried into later views. The unfiltered
claim_apply target passes18/18 (bd92e8). Normal generation707c42 adds exactly
two mappings and refreshes only two existing shared-file digests; all other
declarations and registration metadata remain unchanged. The ledger now has
41 reviewed declarations,42 counterparts and138 assertion links. Complete
LIMITS retain public/durable/setup/profile gaps; these are existing-behavior
tests with no retroactive TDD RED or full original-scenario acceptance.

The full macOS bash scripts/check.sh exits0 (9f3920):435 tests across67 targets,
zero failed/ignored/filtered, including final decoder32/32 and architecture27
without counting repeated checks twice. Four compile-fail docs, formatting,
warnings-denied compile/Clippy/docs, reference/progress and advisories/bans/
licenses/sources checks all pass. Diff316320 passes. Runtime accounting remains
5903/4222/1681/1655; current Linux execution remains open.
The explicit full-parity command exits1 (8282de): all5858 frozen declarations
still lack full required behavioral execution, including setup/fixtures,
mandatory profiles and adapters. Checked partial links do not certify parity.

The next GPT-6.1-sol/max writer proposes the smallest charged retained-gate
predecessor and ordered apply parent in the existing domain review note.
Every new artifact/API requires explicit ARCH consultation before creation.
Exactly ONE retained gate must precede Fresh-only original reconstruction,
coherent current/CAS/context/highwater, paid remerge and atomic proposed writes.
Full original/public/profile acceptance and M1–M8 remain open. These macOS
results and older saved CI do not certify Linux. The full goal stays active.

Final peer findings C-P1/C-P2 are closed for this conservative increment:
unknown macro environments remain unresolved, and the retry assertion pairing
now matches the observed times/results. Regenerated artifacts, independent
compiled diagnostics and the post-fix full wrapper passed. No macro scaffold
proof or full behavioral parity is claimed.

## Location and saved state

- Project: Rust, workflow-only, private
  [ferricstore/ferricstore2](https://github.com/ferricstore/ferricstore2).
- Checkout: `/Users/yoavgea/repos/ferricstore2-rust-design`.
- Branch: `codex/workflow-tdd-core`.
- Reviewed implementation commit `25701439accaeb51d41ffc3fcdcddc5b503f2a8e`
  is pushed to this branch after reciprocal review and the final green local
  gate. Its [Linux/macOS CI run](https://github.com/ferricstore/ferricstore2/actions/runs/37973550388)
  **passed on ubuntu-24.04 and macos-15**. That run certifies the saved commit.
  The later documentation checkpoint `85f82b616521283078d5093726292dfd99d1173f`
  also [passed both runners](https://github.com/ferricstore/ferricstore2/actions/runs/37974338754),
  verified through GitHub CLI on October 12. These saved runs remain distinct
  from the current implementation.
- Latest reviewed local increment: indexed mutation-batch composition and the
  formatter/macro/model changes described above, with618 passing macOS Rust
  tests, four compile-fail docs and all strict source checks. Its predecessor
  is `dbfdf0b`. The
  default dependency refresh fails on DNS; the separate October 14 cached audit
  passes all four categories. It is saved locally with reciprocal review closed.
  SSH/HTTPS host resolution and generic GitHub API connection currently fail;
  no push or fresh CI run happened.
  Current Linux/macOS CI remains required, and subsequent persistence work needs
  its own concrete architecture consultation and predecessor evidence.
- Main's last handoff commit was `b0151c5`. The foundation Linux/macOS CI passed
  `f2ff13b2da4e47e1fd52f74520fb219efe2de613`
  ([run](https://github.com/ferricstore/ferricstore2/actions/runs/37929778754)).

The original dirty Elixir checkout `/Users/yoavgea/repos/ferricstore` was
preserved. The isolated detached baseline
`/Users/yoavgea/repos/ferricstore2-elixir-baseline` remains source-clean at
`66586c596e271408d9184af32eb31cd7218c126e`; its own locked dependencies/build/data
were used for original registration and selected source executions. Elixir is
the compatibility reference, not the new runtime.

## Implemented pure behavior

`wf-types` has checked opaque binary record IDs, separate composed-key bounds,
exact source `fpk:` partition-component encoding, committed timestamps and
nonwrapping fences. Binary/empty component handling, 65,535-byte limits, zero
values and overflow are tested. Partition encoding retains the exhaustive
collision check over 30,783 small component lists.

`wf-domain` implements deterministic single-record create, explicit claim,
transition, complete/fail, retry, lease extension, cancellation and explicit
expired-lease reclaim. Commands carry explicit committed time and ownership.
Results propose one record write and optional history; they do not establish
durability. Borrowed inputs stay intact. Legacy matching duplicate effects emit
no second event; stale guards, intervening operations and different terminal
effects reject. These legacy rules are separate from unimplemented explicit
request-ID replay.

New retry decisions resolve defaults, installed type, logical-state and command
overrides in source order. None/fixed/linear/exponential backoff, caps, explicit
committed jitter, inclusive attempt limits, scheduled retries, active/terminal
exhaustion and nine typed audit observations have source-derived tests. The
installed retry-only view is supplied authority, not policy-installation
persistence. Source hash preparation and public audit bytes remain adapter gaps.
The old caller-resolved retry-time API remains compatible.

New claim selection borrows coherent authoritative canonical records, parallel
due indexes and complete FIFO lane maps. Exact u128 member ordering, running
heads, descending priority, due/binary-ID ties, state/partition filters and
80-follower deep-head lookup are tested. It reads all represented lane heads
before merging, never skips a blocked head to its follower, and distinguishes
claim-limit/scan-budget exhaustion from complete empty work. Limits are 1,000
selected records, 16,384 scanned candidates and 4,096 direct lane-head reads.
Continuations use group/revision/request values; every consumed row/head must
match its unique canonical scoped record. Equivalent reconstructed snapshots
work; conflicting later-page identities reject. Index completeness, revision/
recovery uniqueness, policy-mode placement and atomic batch application remain
adapter obligations. Selection does not mint leases or persist changes.

Production dependency/module policy is unchanged: types/domain are synchronous
pure values with no runtime, clock, random, storage or cache effects. Storage,
consensus, application, wire, transport and worker crates remain scaffolds.
M2+ predecessor gates remain closed.

## Quality policy and accepted optimizations

Workspace compiler/rustdoc warnings and Clippy `all`/`pedantic` remain denied;
unsafe and blanket suppression remain forbidden. [ADR-003](adr/003-source-size-limits.md)
adds checked limits of 400 physical lines per production Rust file and 600 per
tooling/integration-test/benchmark file, including comments and blank lines.
A runtime module named `tests.rs` still gets 400. Oversized reducer/tests were
split by responsibility; no allowlist exemption was added. `cargo progress` is
now in the shared local/CI wrapper.

The separately requested optimizer reviewed actual runtime code and measured
safe temporary probes before proposing changes. Two accepted changes preserve
public constructors and slice accessors: private `RecordId`/`LeaseToken` use
`Box<[u8]>`, and partition encoding emits decimal lengths through a stack buffer
without temporary decimal Strings. Growable buffers remain Vecs. Independent
post-change code/trait reviews and all 99 type/domain tests passed.

Local aarch64 measurements showed ID/token handles 24 -> 16 bytes, workflow
record 304 -> 288 bytes, and partition encoding medians for 1/8/64 components
42/254/1856 -> 19/52/297 ns. These are focused local observations under concurrent
development, not service/RSS/tail-latency guarantees. Box conversion may shrink/
reallocate spare capacity; no clone-speed gain was established. Broad record or
PartitionKey boxing, retry-target overlays and canonical-ID index redesign are
unaccepted future candidates. Full canonical record equality remains intact.

`.codex/config.toml` requests six spawned subagents for future sessions. The
current session's tool limit remains four active agents including the
coordinator; that live limit was not resized. Agents exchange actual review
files and direct notifications, with disjoint write sets and no interruptions.
Only the coordinator commits/pushes.

## Actual verification and review

The final native macOS `bash scripts/check.sh` exited 0:

- **181 Rust tests**, zero failed/ignored: types 19, domain 80, parity 45,
  architecture 24, test support 13.
- **Three compile-fail doctests** passed.
- Formatting, locked all-target compilation, strict Clippy, warnings-denied
  documentation, architecture/reference/progress checks and all four dependency
  audits passed.

Actual post-fix output: `/private/tmp/ferricstore2-m1-v2-final-check.log`.
The shared wrapper also passed on both GitHub runner platforms for `2570143`;
the run linked above checks the same implementation and hash-bound artifacts.
`cargo parity` separately exited 1 with accurate full-scope execution gaps:
`/private/tmp/ferricstore2-m1-v2-final-parity.log`. Fifteen standalone Elixir AST
regressions also passed independently (seed 120422); all four unchanged compiled
alias-injection diagnostics now show actual `:second` and recovered unresolved.
The wrapper validates parser/artifact hashes but does not execute that Elixir
suite or original application tests. Earlier green logs describe their saved
pre-fix trees, not final provenance acceptance.

This continuation executed **78 selected original source cases**, all passed:
12 FIFO primitive/retry-policy cases (seed 729398), 56 priority/multi-state/retry
section cases (seed 734816; 209 location-excluded), and 10 FIFO invariant cases
(seed 654031; 20 location-excluded). Commands/logs/copied JUnit are in the
coordination record. The previous saved increment separately executed its three
partition and six fencing cases successfully (seed 885123). These are source
subsets, not execution of all registered cases or mandatory nondefault profiles.

All four subagents used GPT-6.1-sol/max. Real preimplementation compile-red and
behavior-red output is distinguished from supplemental first-pass checks and
optimization measurements. Reciprocal/coordinator review caught and closed
lexical alias wrong-origin links (R-P1), process-address continuation identity
(C-C1), conflicting later-page scoped identities (C-C2), weak final claim
observation (P-R1), unchecked supplemental-root review hashes, macro-generated
aliases in consumer/source definition environments (C-P1), and one incorrect
source/Rust assertion pairing (C-P2). Owners fixed and peers independently
re-reviewed them without weakening policy.

Current evidence and contracts:

- [Coordination/integration](reviews/workflow-m1-resume-coordination.md).
- [Retry policy](reviews/workflow-m1-retry.md).
- [Claim selection](reviews/workflow-m1-claim.md).
- [Runtime reconciliation/assertion review](reviews/workflow-m1-parity.md).
- [Optimization measurements and decisions](reviews/workflow-m1-optimizations.md).

Previous saved-increment notes `workflow-tdd-{coordination,types,domain,parity}.md`
remain historical evidence; they are not the current verification totals.

## Reference accounting and remaining gates

The immutable reference remains **480 files and 5,858 textual declarations**;
`tests/reference/**` and frozen `workflow-mapping.json` were preserved. The
[progress ledger](../tests/parity/workflow-progress.json) now has **77 reviewed
source declarations, 87 counterpart records and 326 assertion links**. Links
retain exact source observations and explicit omitted scenario assertions; they
do not certify complete original scenarios or executed full parity.

The original 5,843-case/355-module capture is retained unchanged. Supplemental
registration adds the omitted pinned blob consumer and copied hash-verified
source evidence. Reconciliation accounts for **5,903 raw cases in 356 modules,
4,222 exact linked cases, zero admitted macro origins, 4,203 linked frozen
declarations, 1,681 unmatched runtime identities and 1,655 unresolved frozen
declarations**. All ninety source section reviews carry unproved macro
environments. Earlier literal recovery was withdrawn after compiled
counterexamples; no ExUnit/loader name-only exception or scaffold proof is
admitted. The unresolved set includes one documentation example.
Every raw identity and frozen ID retains its separate complement. Mixed case
scope, fixtures/setup and mandatory profiles remain pending; no cases were
invented, dropped, excluded wholesale or called passing by registration.
See [the reproducible collection guide](runtime-case-collection.md).

Continue in this order:

1. Inspect this handoff, actual working tree, review notes and current branch/CI.
   Run the complete wrapper after relevant new changes; full parity remains an
   expected failure until required execution and mappings actually complete.
2. Obtain reviewed compiler lineage or precisely scoped macro-environment proof
   before reinstating any macro origins. Review unmatched runtime cases, the
   documentation non-case, mixed workflow scope, setup/fixtures and mandatory
   profiles without editing frozen inputs or treating registration as execution.
   Port remaining original observations.
3. Use the bounded five-subject pure M1 exit assessment in the architecture
   review. Address concrete remaining source obligations without inventing
   another policy/model layer. Review M2 neutral publication/recovery contracts,
   selected dependency versions and real adapter proof requirements, then obtain
   concrete architecture selection before any new artifact. The represented
   pure subset is not a completed workflow engine.
4. Pass M2 persistence/recovery gates before M3 consensus and M4 application,
   shared wire, HTTPS/TLS-TCP and worker work. Preserve atomic record/index/
   history/outcome/applied-position writes and uncertainty semantics.
5. Follow later workflow/query/retention milestones and explicitly review any
   deferred optimization before implementation. Commit/push only reviewed,
   verified changes; never reset/clean the original dirty Elixir checkout.

Read [AGENTS.md](../AGENTS.md), [architecture](architecture.md),
[the parity contract](workflow-test-parity.md) and
[the ordered implementation plan](implementation-plan.md). No production-ready
Raft, live Rust listener, complete workflow parity or unconditional p99 claim
exists yet.
