# Preserved workflow acceptance tests

Elixir source snapshots for the Rust workflow port. Read [parity contract](../../docs/workflow-test-parity.md) and [inventory](../../docs/workflow-test-inventory.md).

`elixir-baseline/` preserves pinned source bytes and `elixir-working-tree/` preserves separate uncommitted review inputs. `workflow-test-manifest.json` records provenance, hashes and initial unported source-declaration identities. These references include mixed suites and legacy UI/setup code for explicit case-level scope review. They are not a self-contained runnable Elixir project and do not run under cargo test. Do not edit baseline snapshots; accepted source updates require new provenance and mapping.
