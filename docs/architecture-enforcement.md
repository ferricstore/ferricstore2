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

`vec!`, `format!` and `matches!` are the only currently approved production
expression macros. Their supported argument grammars are parsed and recursively
visited, including vector repeats and match guards; unparseable arguments fail.
Other runtime macros, code includes, module path redirection and archunit ignore
directives require a new audit gate. The `archunit: ignore` directive is rejected
independently because the evaluated upstream release deliberately omits that
dependency from its own graph. Tooling can use owned fixture includes and I/O.

No lint allowances are needed by this implementation. A future test exception
must name one specific Clippy lint on a test function and provide a meaningful
`reason`; whole-group, module-wide, unreasoned and production suppressions fail.
Review still decides whether a syntactically accepted reason is appropriate.

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
