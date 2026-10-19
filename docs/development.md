# Development checks

The workspace contains M0 infrastructure, a partial pure M1 domain core and
scoped synchronous durable-log/state adapters with source-derived tests.
Protected local purge and pinned-snapshot startup are implemented. Safe repeated
snapshot advancement, received snapshots beyond the local log tail, consensus,
caches and live transports remain incomplete. Reference
integrity, registration and architecture checks do not establish full workflow
parity or milestone completion. See the handoff for current results.

Use Rust **1.99.0**, edition **2024**, on Linux or macOS. The CI host matrix is
`ubuntu-24.04` and `macos-15`; local checks use the current native host. Windows
and cross compilation are not part of the supported M0 matrix.

Compiler-lineage process controls also require installed **Elixir 1.19.5** and
**Erlang/OTP release 28** on PATH. CI pins OTP **28.3.1**; the local wrapper checks
the release major, while source-capture verification binds the actual build
separately. Install these versions using the
[official Elixir installation guidance](https://elixir-lang.org/install/)
before running the wrapper. This tooling runs synthetic controls; it does not
load or execute the original FerricStore tests during infrastructure checks.

Install the pinned compiler components and audit tool from the repository root:

```bash
rustup toolchain install 1.99.0 --profile minimal --component rustfmt --component clippy
cargo +1.99.0 install cargo-deny --version 0.20.2 --locked --root .tools
bash scripts/check.sh
```

The audit tool belongs in `.tools`, never a global installation for these checks.
The advisory database and its lock also live under the ignored repository-local
`.tools/advisory-dbs`, as configured by `deny.toml`; run individual audits from
the repository root. Normal RustSec fetching remains enabled. This location
keeps audit writes within the workspace and changes no audit threshold.
The wrapper requires exactly `cargo-deny 0.20.2`; missing or different versions
fail with a setup message. It does not install tools or silently skip audits.
The wrapper queries installed Elixir/OTP versions with the same removed
startup/code-path environment variables as the trusted verifier. Missing or
different versions fail before Rust checks; no original collector runs here.
Dependency downloads and the live RustSec advisory database need network access;
an unavailable prerequisite is a failed check. Keep `Cargo.lock` in version
control with the workspace; checks use `--locked` and cannot resolve a new
dependency graph.

CI calls the same wrapper on both hosts. It runs formatting; warnings-denied,
locked workspace check and Clippy on all targets; workspace tests; doctests;
documentation with warnings denied; `cargo arch`; `cargo reference`; and all four
`cargo-deny` checks (advisories, bans, licenses and sources). `CARGO_BUILD_WARNINGS`
is set to `deny`, and rustdoc receives `-D warnings`. A failure stops the wrapper.

The wrapper also runs `cargo progress`, which validates source/runtime identity
accounting and actual assertion links without certifying execution or full
workflow parity. Architecture checks enforce 400 physical lines per production
Rust `src` file and 600 per tooling/integration-test file, as recorded in
[ADR-003](adr/003-source-size-limits.md).

Group related code by responsibility. A module with private children uses
`feature.rs` and `feature/child.rs`; a multi-file Cargo integration target uses
`tests/<target>/main.rs` with child modules beside it. Small cohesive modules
can stay in one file; Rust does not require one file per type. This follows the
[Rust Book's module layout](https://doc.rust-lang.org/book/ch07-05-separating-modules-into-different-files.html).
Size limits include comments and blank lines. Before creating or moving files,
types or modules, record the concrete architecture review and implementation
assignment required by [AGENTS.md](../AGENTS.md); review the resulting code and
ownership after implementation as well.

The supported feature profile is the default profile: there are currently no
first-party Cargo features. New feature combinations require an explicit reviewed
matrix and matching architecture/audit coverage. Do not substitute a blind
`--all-features` run when features may be mutually exclusive.

First-party packages are private (`publish = false`) and inherit workspace
package/lint policy. Crate roots forbid unsafe code. Clippy `all` and `pedantic`
are denied along with the selected unwrap, expect, panic, todo, unimplemented,
dbg and lock-across-await restrictions. Test exceptions must be narrow, explain
the reason, and undergo review; broad allowances are rejected by architecture
checks. There is no first-party open-source license grant.

`deny.toml` permits MIT, Apache-2.0 and verified Unicode-3.0 third-party licenses, rejects unknown
registries and unapproved Git sources, duplicate versions and wildcard requirements, and
denies yanked, unmaintained and unsound dependencies. Unpublished workspace
packages are exempt only from license declarations. Third-party dev/build
dependencies remain audited. There are no advisory ignores. Three exact
duplicate-version exceptions cover `syn@2.0.119`, `thiserror@1.0.69` and
`thiserror-impl@1.0.69`: the reviewed architecture/Serde tooling requires
incompatible upstream majors. Every exception has its chain and review owner in
the policy; it must be re-reviewed on dependency updates. An unused exception
fails through `--deny unmatched-skip`. The exact reviewed redb Git source is
selected in [ADR-007](adr/007-durable-log-storage.md); other Git sources remain
closed. See [the evidence](reviews/infrastructure-ci-review.md).
New license requirements or exceptions need evidence and review.

Useful individual commands are:

```bash
cargo arch       # Locked architecture policy tests, including negative fixtures.
cargo reference  # Preserved input integrity; no workflow implementation claim.
cargo progress   # Checked identity/assertion accounting; no execution certification.
cargo parity     # require-parity: must fail while included cases remain unported.
```

`cargo parity` is intentionally outside the green infrastructure gate. Future
workflow acceptance requires the source case/assertion mappings, runtime
collection and actual execution described in [the parity contract](workflow-test-parity.md).

The pinned audit release was reviewed against its [upstream manifest](https://github.com/EmbarkStudios/cargo-deny/blob/0.20.2/Cargo.toml)
(MSRV 1.88.0, edition 2024, MIT OR Apache-2.0), [release](https://github.com/EmbarkStudios/cargo-deny/releases/tag/0.20.2)
and [check configuration](https://embarkstudios.github.io/cargo-deny/checks/cfg.html).
Its own install resolution is frozen by `cargo install --locked`; it is not a
workspace/runtime dependency. Checkout is pinned to
`3d3c42e5aac5ba805825da76410c181273ba90b1`, verified with upstream
`refs/tags/v7.0.1`. CI grants only `contents: read`, does not persist checkout
credentials and does not publish source or request secrets.

CI installs compiler-control tooling with
[erlef/setup-beam v1.24.0](https://github.com/erlef/setup-beam/releases/tag/v1.24.0)
at verified release commit `fc68ffb90438ef2936bbb3251622353b3dcb2f93`.
Its [pinned action inputs](https://raw.githubusercontent.com/erlef/setup-beam/fc68ffb90438ef2936bbb3251622353b3dcb2f93/action.yml)
select strict quoted Elixir1.19.5/OTP28.3.1 with Hex/Rebar installation disabled.
The [upstream compatibility guide](https://github.com/erlef/setup-beam/blob/fc68ffb90438ef2936bbb3251622353b3dcb2f93/README.md)
supports the existing Linux/macOS runner matrix. Authoring this prerequisite
does not establish successful installation or checks on either CI runner.

The original M0 wrapper passed locally on native macOS: 53 infrastructure tests,
three compile-fail doctests, documentation/architecture/reference checks and all
four audits. `cargo parity` separately fails as required. The authored Linux and
macOS GitHub runner matrix subsequently passed for the foundation commit recorded
in [the handoff](CONTINUE.md). Current continuation changes need their own final
checks and CI result. Actual negative
checks and reciprocal acceptance are recorded in [the CI review](reviews/infrastructure-ci-review.md).
