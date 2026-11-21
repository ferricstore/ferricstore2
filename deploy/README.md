# Running ferricstore2

A node runs alone or as one member of a replicated group (three members
survive the loss of one). Every setting is a `FERRICSTORE_*` environment
variable, and each has a flag that overrides it (the table at the top of
`crates/wf-node/src/main.rs` lists them).

## One node

```sh
ferricstore2 --data-dir /var/lib/ferricstore2 --host 0.0.0.0
```

## A group: how members find each other

`FERRICSTORE_DISCOVERY` picks how a node learns its members:

| Value | Members | This node |
|---|---|---|
| `none` (default) | itself only | — |
| `static` | `FERRICSTORE_CLUSTER_NODES=host[:port[:peer_port]],...`, the same list on every member; ids are positions from 1 | `FERRICSTORE_NODE_ID`, or the entry whose host is `FERRICSTORE_ADVERTISE_HOST` (default: `FERRICSTORE_NODE_NAME`, then `HOSTNAME`) |
| `dns` | the `FERRICSTORE_CLUSTER_SIZE` pods of a Kubernetes StatefulSet, `<name>-<ordinal>.<FERRICSTORE_DNS_NAME>` | its own pod name (`HOSTNAME`) |

Members reach each other on the peer port (`FERRICSTORE_PEER_PORT`, 7388)
and resolve names on every connection, so a pod that restarts with a new IP
is found again. Clients connect to any member on the native port
(`FERRICSTORE_NATIVE_PORT`, 6388); `SHARDS` sends them to the leader by the
name above, so clients must resolve those names.

The group forms by itself: its lowest-numbered member creates it once every
other member answers that it holds no data. A member that lost its data
rejoins the group instead. Members must keep their data directory: a member
that loses it may also have forgotten a vote it cast.

Health: with `FERRICSTORE_HEALTH_PORT` (6380 in the image), `GET
/health/live` is 200 while the process runs and `GET /health/ready` is 200
once the member knows its leader (followers send clients there).

A member whose leadership ended with a change the group never took stops and
exits with status 75 (fail-stop); restart it (Kubernetes and the compose
file do) and it rebuilds from the group.

## Kubernetes

```sh
docker build -t <registry>/ferricstore2 .
# set the image in deploy/kubernetes/ferricstore2.yaml, then
kubectl apply -f deploy/kubernetes/ferricstore2.yaml
```

The manifest runs a StatefulSet of 3 with `dns` discovery behind a headless
service (`publishNotReadyAddresses: true`, so members resolve each other
before they are ready), starts the pods in parallel, probes the health
endpoints, keeps each member's data on its own volume and allows one pod
down at a time. Clients connect to the `ferricstore2` service.

Keep `FERRICSTORE_CLUSTER_SIZE` equal to `replicas`: changing the size of a
running group is not supported yet.

## Docker Compose

```sh
docker compose -f deploy/docker-compose.cluster.yml up --build
```

Three members with `static` discovery by service name. Clients run on the
compose network (or use `network_mode: host` on Linux), since `SHARDS`
names the members `fs1`, `fs2`, `fs3`.

## Processes on one machine

```sh
for id in 1 2 3; do
  FERRICSTORE_DISCOVERY=static \
  FERRICSTORE_CLUSTER_NODES=127.0.0.1:6491:7491,127.0.0.1:6492:7492,127.0.0.1:6493:7493 \
  FERRICSTORE_NODE_ID=$id FERRICSTORE_NATIVE_PORT=649$id FERRICSTORE_PEER_PORT=749$id \
  FERRICSTORE_HEALTH_PORT=638$id \
  ferricstore2 --data-dir data/n$id &
done
```
