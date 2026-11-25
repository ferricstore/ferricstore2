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
- **A member that lost its data does not vote until it caught up:** Raft's
  safety rests on a voter keeping its log and its vote, so a member that
  starts without a log gives no vote until every other member has answered
  it once (`Status` answers the last log index and the term; the answers
  need not come together, since a term only grows). If none holds a log
  past a forming group's first entry, the group is new. Otherwise it waits
  to apply a commit point that a leader at or above every term it heard
  set in its own term (the first such point it heard, from the newest such
  leader, kept from its start): that leader holds every committed entry,
  so this member then holds every entry its lost log had taken. Without this, its vote could elect a member lacking entries that
  only it and a stopped member held (25 Nov). It may stand for election
  meanwhile, since each voter checks a candidate's log against its own. A
  group with one member down and another that lost its data has no
  majority until one of them is back.
- **A leader takes back a follower that lost its data:** the leader had
  recorded how far that follower's log matched, and OpenRaft stops it when
  the follower's log goes back (a panic in debug builds; in release builds
  it stops replicating to that follower). Members allow log reversion, so
  the leader starts that follower's progress over. OpenRaft warns that such
  a member may vote unsafely; the rule above keeps it from voting, and the
  leader never lowers its commit point.
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
