# Project foundation

The user has authorized the Rust project foundation: workspace/module scaffolding, test support, lint/dependency policy, architecture enforcement and CI. Implement those M0 infrastructure pieces. Workflow business behavior, storage/consensus adapters, live HTTP/TCP listeners and a UI remain outside this increment. Do not add fake service behavior or mark workflow parity passing.

Read README.md and docs/architecture.md before proposing changes. Keep the workflow-only scope. Source observations, proposed decisions and unresolved validation gates must be distinguished. Cite source paths and upstream primary documentation. Record disagreements and resolutions in docs/reviews; do not claim independent review unless it happened.

Implementation must follow the dependency allowlist, functional core and safety rules. Architecture changes require an ADR and updates to architecture checks. Do not suppress checks just to pass CI. Preserve backward compatibility only where explicitly included in the parity matrix. All first-party code inherits workspace lint policy and forbids unsafe. Tooling/test-support crates must never enter production dependencies.

Use subagents for this foundation work, communicate through actual shared-file reads and coordinator relays where direct tools are unavailable, and record reciprocal findings/resolutions in docs/reviews. Disjoint write sets avoid overwriting another agent's work.

The existing Elixir workflow tests are mandatory acceptance inputs: read docs/workflow-test-parity.md. Preserve source identities, scenarios and assertions in Rust counterparts; do not silently drop cases, weaken assertions or count ignored/deferred tests as parity. tests/reference contains preserved upstream test sources and fixture/manifest data, not a runnable Rust implementation.
