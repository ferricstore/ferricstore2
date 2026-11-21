# ADR-019: Cluster setup and discovery

Status: accepted (2026-11-21). Solo decision at the user's request ("make it
easy to setup cluster of nodes with discovery support like kubernetes need
or other tools"); no independent review is claimed.

## Context

ADR-018 phase 2 started a member with `--node-id` and one `--member
ID=HOST:PORT/PEER_HOST:PEER_PORT` per member: every node needed every
address, and resolved them once at start. On Kubernetes, pods get stable
names from a StatefulSet but new IPs when they restart, and are configured
through environment variables and probed over HTTP. The Elixir server reads
`FERRICSTORE_*` variables, picks discovery with `FERRICSTORE_DISCOVERY`
(`gossip`, `dns`, `epmd`, `consul`, `etcd`, `none`; `config/runtime.exs`)
and answers `GET /health/live` and `GET /health/ready` on
`FERRICSTORE_HEALTH_PORT`.

## Decision

- **Settings:** every setting is a `FERRICSTORE_*` variable (Elixir's names
  where it has one: `FERRICSTORE_DATA_DIR`, `FERRICSTORE_NATIVE_PORT`,
  `FERRICSTORE_HEALTH_PORT`, `FERRICSTORE_NODE_NAME`, `FERRICSTORE_DISCOVERY`,
  `FERRICSTORE_CLUSTER_NODES`, `FERRICSTORE_DNS_NAME`), each with a flag
  that overrides it. The `--member` flags are gone.
- **Discovery** builds the member table (`wf_node::configuration`):
  `static` lists hosts with optional ports, ids by position; `dns` is a
  Kubernetes StatefulSet behind a headless service, members
  `<name>-<ordinal>.<FERRICSTORE_DNS_NAME>` with ids ordinal + 1. Raft needs
  stable member identities, so `dns` uses pod names, not the A records of
  whatever pods answer now (Elixir's DNS polling) and not IPs. Gossip,
  Consul and etcd wait for changing the size of a running group.
- **Names resolve per connection:** peers are `host:port` strings resolved
  on each connection.
- **Formation:** the lowest member forms the group once every other member
  answers a status request (`Status`, peer kind 6) that it holds no log; if
  one holds a log, the group exists and this member waits to be replicated
  to. This keeps a member that lost its data, with a member list that
  changed meanwhile, from writing a different membership under the first
  entry's log id.
- **Health:** `GET /health/live` (200) and `GET /health/ready` (200 once
  the member knows its leader and no lost round stopped it, else 503) on
  `FERRICSTORE_HEALTH_PORT`.
- **Packaging:** a Dockerfile, a Kubernetes manifest (headless service with
  `publishNotReadyAddresses`, client service, StatefulSet with parallel pod
  start, probes and volumes, PodDisruptionBudget) and a compose file, with
  `deploy/README.md`.

## Consequences

A group is three pods and one manifest; members find each other again after
restarts. Not decided here: changing a running group's size (adding and
removing members through Raft membership changes), and members reachable
by clients outside the Kubernetes network.
