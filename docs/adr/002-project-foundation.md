# ADR-002: executable M0 workspace and isolated quality tooling

Status: accepted, implemented and locally validated, 2026-10-09. All three infrastructure reviewers accepted both peers and the root/handoff; findings and dispositions are recorded in the infrastructure review record.

## Authorization and scope

The user authorized project infrastructure based on the reviewed architecture: Rust scaffolding, test support, lint/dependency policy, architecture tests and CI. This supersedes the design-phase prohibition on Cargo/code/CI for this increment. Workflow business behavior, runtime Raft/storage/cache adapters, client listeners and UI remain later work.

## Decision

Create the twelve production crate boundaries from ADR-001 as library/module scaffolds. Do not add fake service behavior, placeholder successful mutations or premature Raft/storage library adoption. The dependency allowlist governs declared edges even while implementation modules are empty. Application implementation and worker transport modules remain private; compiler-failure doctests exercise raw-port and worker-private boundaries.

Pin Rust 1.99.0 with edition 2024, rustfmt and Clippy. Every crate inherits workspace lint policy; forbid first-party unsafe and require documentation and standard lint checks. Commit Cargo.lock, use locked builds and declare supported feature profiles. The initial profile has no optional runtime features. The repository remains private/unpublished, with no new public licensing decision.

Keep wf-architecture-tests as isolated tooling. Select archunit 0.0.2 dev-only after executable negative-fixture evaluation, supplemented by Cargo dependency classification, item/module checks and compiler visibility. The library's documented file/syntax limits do not establish transitive semantic purity or macro-expanded capability safety. New or generated code needs the supported check/profile policy and review.

Add wf-test-support for isolated test resources, controlled clocks and acknowledged bounded fault hooks. Add wf-parity-tests for immutable reference preservation and explicit incomplete-parity reporting. These are tooling/test boundaries; production cannot depend on them. Dev-dependencies do not create runtime permission to import test helpers. No runtime source collection or workflow counterpart is inferred from textual declaration IDs or a successful artifact-integrity check.

## Validation

CI and local checks must run the same formatting, compiler/Clippy/doc warnings, architecture rules and negative fixtures, scaffold/helper tests, reference-integrity checks and dependency audit. The full parity gate fails until original runtime cases, assertion mappings and executed Rust counterparts exist. No ignored/deferred case can count as passing parity.

The complete local check wrapper passes: 53 infrastructure tests, three compiler-failure doctests, formatting/compiler/Clippy/rustdoc checks, complete-workspace architecture/reference checks and all four pinned dependency audits. See [the infrastructure review record](../reviews/infrastructure-review-record.md) for peer findings, versions, dispositions and limits. M0's later runtime ExUnit collection/scope decisions remain unfinished; this increment is the infrastructure foundation, not all M0 exit evidence or any workflow runtime milestone. The supported Linux/macOS remote host matrix is configured separately from local execution evidence.

## Parsed standard formatting audit

The October 15 architecture review extends the existing parsed expression-macro
policy to standard `write!` for direct formatter output. This removes explicit
temporary strings in existing error formatting. The checker requires a
destination, literal format string and parseable comma-separated arguments,
then recursively inspects the destination and all arguments. Tests first expose
the missing allowance and inspection; positive formatting, hidden I/O, forbidden
layer access, unsafe expressions and malformed syntax remain executable controls.
Other macro and generated-code audit requirements remain in force. This changes
neither crate dependencies nor domain capabilities and establishes no measured
native performance gain.

## Captured option module classification

The October 23 option-helper increment adds public `wf-domain::options` and its
private `options::put` child. Register `options = true` in the complete root
module classification; the flag declares public visibility. Package-wide domain
purity continues to cover both modules, with the same dependency allowlist.

The module validates captured proper atom-key options and borrows their values
and diagnostic keys. Row updates operate on typed keyword or unique-map views
with explicit source-strict, deterministic equality and fallible row allocation.
This supplies no generic VM parser, opaque term equality or public admission.
The checker regression accepts both actual files and pure borrowed-value code,
rejects environment/file/I/O/async/global-state examples, and exercises forbidden
ports/runtime dependencies through mutated metadata. No checker rule, effect
permission, dependency edge or profile is relaxed. Its preclassification failure
is architecture evidence; the eight option behavior tests require their own
executed failing baseline and passing implementation.

## Captured metadata module classification

Register public `wf-domain::metadata` with `metadata = true` in the complete
root module classification. Its private `metadata::attributes` and
`metadata::state_meta` children retain the package-wide domain purity and
dependency rules. The flag declares visibility; it grants no effects, dependency
edges, public admission or durable state authority.

This module owns the complete original canonical metadata predicates and both
normalizers. A metadata-specific input view preserves floats, arbitrary
integers, atom aliases, captured map order and proper/improper lists. Shared
policy input, its closed ETF profiles and outer-frame accounting stay unchanged.
Separate descriptor and leaf lifetimes permit owned output spines to outlive
temporary descriptors while borrowing payloads. Public results remain ordinary
values. Source-returned, source-raised and real reservation errors stay distinct.
Logical byte limits are not heap or admission bills.

The named classification regression checks all three actual source files and
pure UTF-8/trim/float controls, rejects environment/file/I/O/async/global-state
examples, and rejects declared ports/runtime dependencies through graph checks.
Its actual preclassification assertion reports an unclassified production
module. Registering this one module does not weaken checker logic or permissions.
That architecture failure is separate from the required compiled behavioral
baseline, source assertion preservation and passing metadata implementation.
