# M0 parity lineage inventory

`workflow-mapping.json` lists all 5,858 preserved textual declaration IDs as
unported. It is separate from the immutable `tests/reference` manifest and
snapshots. Its [schema](workflow-mapping.schema.json) deliberately supports
only honest M0 absence of collection and execution evidence. The Rust validator
also checks bidirectional equality against the preserved source IDs.

`cargo reference` checks artifact integrity only. `cargo parity` always fails
in M0, even with a valid unported mapping. Manual passing statuses, fake
counterparts, attached evidence, missing lineage and unknown fields fail.

A standalone test snapshot cannot register the pinned application's actual
ExUnit cases. A future schema and evidence verifier must collect source module,
describe context, runtime name, parameter identity, location, effective tags,
setup/fixtures, baseline failures and mandatory profiles, then map assertions
and execute actual Rust counterparts. Source-to-runtime lineage can be one to
many. Declaration count must never be reported as an executed runtime count.
See [testing infrastructure](../../docs/testing-infrastructure.md) and the
[parity contract](../../docs/workflow-test-parity.md).
