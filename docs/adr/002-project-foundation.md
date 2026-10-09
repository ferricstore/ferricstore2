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
