# Executable architecture enforcement

This M0 infrastructure gate checks the reviewed workspace boundaries. It does
not execute workflow behavior or certify Elixir parity. Run `cargo arch`, or
`cargo +1.99.0 test -p wf-architecture-tests --locked`, from the checkout root.
The same tests run through `scripts/check.sh` with warnings denied.

## Declared Cargo graph and complete coverage

[`architecture/policy.toml`](../architecture/policy.toml) classifies all twelve
production and three M0 tooling packages. The checker requests locked, offline
`cargo metadata --no-deps --format-version=1` without a host/target/feature
filter. It inspects each package's **declared** dependencies, including inactive
optional, target-specific, build and dev edges. The resolved active graph alone
would miss those declarations. First-party dependency renames retain their
original package identity and workspace path.

Normal production edges follow [the architecture](architecture.md). Build
edges currently have an empty allowlist. Only `wf-test-support` is approved as
a production dev dependency; dev permission does not permit imports in shipped
source. Runtime normal/build edges into any M0 tooling crate fail. Production
external dependency allowlists start empty, including dev/build declarations.
Tooling external dependencies have exact version, source, feature and default
feature classifications. New packages, crate directories, features, external
dependencies and unsupported target kinds fail until policy is reviewed.

The supported feature profile is `default`, with no declared workspace-package
features. A new valid profile requires an ADR, policy and CI checks; blindly
enabling every future feature is not the supported matrix. Missing classified
members, empty source selections, fixture compiler targets, custom-build and
proc-macro targets are errors. Parser or Cargo failures return failed checks,
never passing empty reports.

## Syntax, facades and safety

Every manifest inherits package metadata and workspace lints without local
overrides. Root lint requirements include rustc/rustdoc documentation rules,
`rust_2018_idioms`, forbidden unsafe, Clippy all/pedantic and the selected
unwrap/expect/panic/todo/unimplemented/dbg/await-holding-lock restrictions.
Every compiler target root must declare `#![forbid(unsafe_code)]`. The syntax
visitor independently rejects unsafe blocks/functions/impls/traits/FFI and
inspects every first-party tooling source as well as production code.

Production root modules and facade visibility are fully classified. Pure types,
domain and wire source may reference only a conservative set of value-oriented
standard-library modules; clock, environment, filesystem, network, thread,
atomic and synchronization paths are excluded. Pure source rejects async/await,
global statics and effect macros. Domain Cargo dependencies are std plus
`wf-types`, with no approved external helper yet.

`wf-wire` may reference only `wf_domain::value`. That value module may not
import/re-export other domain modules. Worker ports are restricted to
`wf_ports::client`; that facade may not import/re-export server port siblings or
non-value domain modules. Worker wire paths are restricted to the private
`client::http` and `client::tcp` modules. Their module ancestors remain private,
and public re-exports through `client` fail. Import groups, aliases, Cargo
renames, glob imports/re-exports and qualified references are inspected.

`vec!`, `format!`, `write!` and `matches!` are the currently approved production
expression macros. Their supported argument grammars are parsed and recursively
visited, including vector repeats and match guards; unparseable arguments fail.
The supported `write!` form requires a destination and a string literal followed
by comma-separated expressions or named assignment arguments. The destination
and every argument receive the same effect, unsafe and dependency checks as
ordinary expressions. Missing or nonliteral format arguments fail closed.
Other runtime macros, code includes, module path redirection and archunit ignore
directives require a new audit gate. The `archunit: ignore` directive is rejected
independently because the evaluated upstream release deliberately omits that
dependency from its own graph. Tooling can use owned fixture includes and I/O.

No lint allowances are needed by this implementation. A future test exception
must name one specific Clippy lint on a test function and provide a meaningful
`reason`; whole-group, module-wide, unreasoned and production suppressions fail.
Review still decides whether a syntactically accepted reason is appropriate.

## Source size

[ADR-003](adr/003-source-size-limits.md) limits every production Rust `src`
file to **400 physical lines**, including conditional source and nested test
modules. Tooling and integration-test/benchmark files may contain **600**.
Blank lines, documentation and comments count; LF/CRLF and a final line without
a newline are handled consistently. Preserved upstream snapshots and explicit
parser fixture data remain outside the first-party Rust selection.

The architecture checker enforces both boundaries in `cargo arch` and the
local/CI wrapper. There are no file-specific exemptions. Divide large files by
responsibility while preserving the module allowlist, pure domain and existing
public contracts. Clippy's long-function/argument checks remain enforced.

## Archunit evaluation and compiler evidence

The [official archunit API](https://docs.rs/archunit/latest/archunit/) identifies
release 0.0.2 as syntax/file analysis without macro expansion. The exact release
is pinned **dev-only** after executable fixtures proved file/layer rejection of
forbidden imports, re-exports, aliases and qualified references, with positive
controls and strict empty file/layer selections. Workspace layer checks cover
all fifteen classified source scopes; Cargo/source checks supplement the
framework's ignored unclassified endpoints and item/visibility limitations.

Forbidden Rust lives under `crates/wf-architecture-tests/tests/fixtures`, as
parser/string data, outside workspace members and compiler targets. The tests
copy only explicit fixture inputs into isolated temporary projects. Real
consumer compilations first prove approved public facades compile, then require
the intended compiler errors for private worker/application modules (`E0603`),
HTTP/TCP consumers directly importing transitive ports (`E0432`), and forbidden
unsafe. The production API doctests additionally exercise their own dependency
contexts. These prove direct visibility/dependency boundaries; they do not
prove that every future public type is transitively capability-safe.

## Pins and audit boundaries

The current lockfile resolves the architecture tool's direct pins as follows:

| Dependency | Version | Scope / selected features |
|---|---|---|
| archunit | 0.0.2 | dev; default |
| serde_json | 1.0.151 | tooling; default |
| syn | 2.0.119 | tooling; default + full + visit |
| toml | 0.8.23 | tooling; defaults disabled, parse |
| tempfile | 3.27.0 | dev; default |

The pinned source manifests/API were inspected. TOML 0.8.23 meets the ordinary
manifest/policy grammar and unifies archunit's tool chain, avoiding four
duplicate dependency families from the initial TOML 0.9.8 draft. Remaining
versioned tooling duplicate/license decisions and actual cargo-deny results are
recorded in [the CI review](reviews/infrastructure-ci-review.md). Forbid applies
to first-party Rust; transitive dependency unsafe and advisory changes still
require the complete dependency audit.

## Limits and next gates

This is conservative syntax and declared graph enforcement, not name/type
resolution or an effect system. It unions conditional branches, does not expand
procedural/attribute macros, and cannot prove transitive semantic purity through
every future facade, method, trait, dependency or generated artifact. Approved
expression macro inspection closes their visible token escape without claiming
general expansion. Future derives/macros, generated source/build scripts,
external pure helpers and richer public contract types require explicit source
and expansion review, supported profile/compiler checks, capability-focused
negative fixtures and an ADR/policy update. Deterministic ordering and correct
workflow semantics need later behavioral/model tests. Source text and test data
are not passing runtime parity evidence.

See [the architecture review](reviews/infrastructure-architecture-review.md)
for actual checks, reciprocal findings and dispositions, and
[CONTINUE.md](CONTINUE.md) for the next authorized work.

## Pure storage value boundary

ARCH-NEXT-M2-STATE-CONTRACT1-CREATE1 applies the existing pure-source rules to
exact `wf-storage::state::value` and its descendants, plus exact
`wf-ports::log::record_batch::response`. Transaction and backend responsibilities
remain effectful. Tests exercise approved pure imports and refusal of filesystem,
environment aliases, async behavior, global state, unsafe and hidden macro
effects. Neighboring effectful modules remain positive controls. Declared graph
mutations reject wire-to-ports/storage and storage-to-consensus edges; client
facade fixtures reject the new server capability and response helper exports.
These checks add no dependency or size exemptions and supply no indirect-call
effect proof. Borrow safety additionally requires compiler-backed trait docs.

ARCH-M2-RECORD-BATCH-PHASES1-CREATE1 additionally applies direct purity checks to
exact `wf-ports::log::record_batch`, its exact intent child and its phase child
and phase descendants. The existing test now exercises filesystem/environment
aliases, async/await, globals, unsafe and macro-hidden effects at those paths;
neighboring response_backend/phase_backend remain effectful positive controls.
The added rule has an executed missing-diagnosis RED before repair and focused
GREEN afterward. The five new exports are also refused by client-facade checks.

ARCH-NEXT-M2-PHYSICAL-FORMATS1-CREATE1 extends the same pure rule to exact
`wf-storage::state::key` and its descendants. Existing tests cover point/ordered
paths, nested descendants, filesystem reexports and neighboring effectful key
modules. The new history child inherits the existing value rule. No broader
storage purity rule, file/function limit exemption or dependency edge is added.

ARCH-M2-STATE-POINT-SCANS1-CREATE1 adds the actual record scan child to the
existing value-module test matrix. The descendant rule already applies; its
filesystem/alias/async/global/unsafe/macro refusals need no new exception.
Behavioral tests separately cover complete point binding, distinct ownership
caps and query/donor lifetimes. Source review verifies pre-copy admission and
shared parsing; the purity matrix alone does not establish those properties.

ARCH-M2-POINT-RETAINED-GATE1-CREATE1 uses existing pure domain/phase/key paths,
without a new policy exemption. Existing client-facade and wire/domain-value
fixtures now reject the point view/inspection exports at their actual paths.
The transaction remains an effect boundary. Compiler-backed behavior tests
separately exercise independent query/outcome lifetimes; direct source checks
do not establish same-revision physical observations or backend durability.

ARCH-M2-TOUCHED-INDEXED-INPUT1-CREATE1 adds exact
`wf-ports::log::record_batch::fresh` and its descendants to the same pure rule.
Its compiled missing-diagnosis test failed twice before repair; the focused
test then passed. The neighboring fresh_backend remains an effectful positive
control. Domain batch/indexed children already inherit domain purity. Existing
client, wire and domain-value checks reject the new server-only exports.
No dependency or source-size exemption is introduced. Borrowed touched facts
remain ordinary supplied values; these checks prove no physical provenance.


ARCH-M2-RECORD-BATCH-TRANSACTION1-CREATE1 keeps the domain policy projection
under existing domain purity and the new storage transaction children effectful.
The state-value test matrix exercises the actual new projection child and nested
path, while client/wire/domain-value fixtures reject its actual server-only
exports. The key encoders, shared borrowed record encoder and scalar scanner
getter inherit existing pure key/value rules. No source policy expansion or
missing-rule RED is attributed to these already-covered descendants. Focused
fixtures pass independently before source implementation; behavioral RED remains
separate. The native main/cases/fixtures are test tooling within600 physical lines;
five adapter/fault implementation files remain within400, all functions within100.
The same strict workspace lint tables apply to the isolated native manifest.

ARCH-PUBLIC-MUTATION-INGRESS1-CREATE1 applies the same direct purity rules to
exact `wf-ports::log::public_mutation` and its descendants. Negative controls
for the root and response child failed twice before the narrow classifier
repair; all42 architecture tests then passed. The named neighboring
`public_mutation_backend` stays effectful. `consensus::mutation` remains an
effect contract, and the worker client facade remains value-only. This adds
no dependency, source-size or lint exemption. The shared pure V1 registry is
the limited delegation recorded in ADR-005; actual authorization, same-writer
observations, retained publication and Linux/macOS durability need separate
executed evidence.
