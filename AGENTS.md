# Design phase

This repository currently contains design only. Do not introduce implementation, Cargo manifests, executable CI or a UI until implementation is requested.

Read README.md and docs/architecture.md before proposing changes. Keep the workflow-only scope. Source observations, proposed decisions and unresolved validation gates must be distinguished. Cite source paths and upstream primary documentation. Record disagreements and resolutions in docs/reviews; do not claim independent review unless it happened.

Future implementation must follow the dependency allowlist, functional core and safety rules. Architecture changes require an ADR and updates to architecture checks. Do not suppress checks just to pass CI. Preserve backward compatibility only where explicitly included in the parity matrix.
