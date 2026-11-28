"""Voters and replicas through the Python SDK (ADR-020).

Starts three ferricstore2 members on this machine, the third as a replica
(`FERRICSTORE_CLUSTER_ROLE=replica`), and checks with the SDK's own calls:
`cluster_role()` on each member, a replica that reads every acknowledged
write and refuses writes, `cluster_promote()` asked of the replica itself,
`cluster_demote()` of the leader asked of another member (leadership
moves), the last voter kept, and a name that is no member refused.

Usage, with a Python that imports the `ferricstore` SDK:
    <sdk>/.venv/bin/python tests/cluster/roles.py [path/to/ferricstore2]
The binary defaults to target/release/ferricstore2. Prints ROLES-OK.
"""

from __future__ import annotations

import os
import shutil
import sys
import tempfile
import time
import uuid
from pathlib import Path
from typing import Any

from failover import QUERY, ROOT, Member, free_ports, topology_client, wait
from ferricstore import FerricStoreError, FlowClient, JsonCodec


def text(value: Any) -> str:
    return value.decode() if isinstance(value, bytes) else str(value)


def direct(member: Member) -> FlowClient:
    return FlowClient.from_url(member.url, codec=JsonCodec(), timeout=5.0)


def roles(clients: dict[int, FlowClient]) -> dict[int, str]:
    return {member: text(client.cluster_role()) for member, client in clients.items()}


def exercise(members: list[Member]) -> None:
    clients = {member.member: direct(member) for member in members}
    topology = topology_client([member.url for member in members])
    try:
        assert roles(clients) == {1: "voter", 2: "voter", 3: "replica"}, roles(clients)
        key = f"{{roles:{uuid.uuid4().hex}}}:p"
        flow_type = f"roles-{uuid.uuid4().hex}"
        params = {"partition": key, "type": flow_type, "state": "ready"}
        topology.create(
            f"{key}:a", type=flow_type, state="ready", partition_key=key, idempotent=True
        )

        def replica_sees() -> bool:
            result = clients[3].query(QUERY, params)
            found = {text(r.get("id", r.get(b"id"))) for r in (result.records or [])}
            return f"{key}:a" in found

        wait("the replica reads the acknowledged write", replica_sees, timeout=30)
        try:
            clients[3].create(
                f"{key}:refused", type=flow_type, state="ready", partition_key=key
            )
        except FerricStoreError:
            pass
        else:
            raise AssertionError("a replica took a write")

        # Asked of the replica itself, which asks the leader.
        assert clients[3].cluster_promote("ferricstore2-3") is True
        wait("member 3 votes", lambda: roles(clients)[3] == "voter", timeout=30)

        leader = text(topology.route(key)["leader_node"])
        led = int(leader.rsplit("-", 1)[1])
        asker = next(member for member in clients if member != led)
        assert clients[asker].cluster_demote(leader) is True

        def moved() -> bool:
            topology.refresh_topology()
            return text(topology.route(key)["leader_node"]) != leader

        wait("leadership moves off the demoted member", moved, timeout=30)
        wait("the demoted member is a replica", lambda: roles(clients)[led] == "replica")
        topology.create(
            f"{key}:b", type=flow_type, state="ready", partition_key=key, idempotent=True
        )

        voters = [member for member, role in roles(clients).items() if role == "voter"]
        assert clients[voters[0]].cluster_demote(f"ferricstore2-{voters[0]}") is True
        last = voters[1]
        wait(
            "one voter left",
            lambda: list(roles(clients).values()).count("voter") == 1
            and roles(clients)[last] == "voter",
        )
        try:
            clients[led].cluster_demote(f"ferricstore2-{last}")
        except FerricStoreError as error:
            assert "at least one voter" in str(error), error
        else:
            raise AssertionError("the last voter was demoted")
        try:
            clients[last].cluster_promote("k")
        except FerricStoreError as error:
            assert "unknown node k" in str(error), error
        else:
            raise AssertionError("a name that is no member was taken")
    finally:
        for client in clients.values():
            client.close()
        topology.close()


def main() -> None:
    binary = Path(sys.argv[1]) if len(sys.argv) > 1 else ROOT / "target/release/ferricstore2"
    ports = free_ports(9)
    nodes = ",".join(f"127.0.0.1:{ports[i]}:{ports[3 + i]}" for i in range(3))
    root = Path(tempfile.mkdtemp(prefix="ferricstore2-roles-"))
    members = [
        Member(
            binary,
            i + 1,
            nodes,
            (ports[i], ports[3 + i], ports[6 + i]),
            root / f"n{i + 1}",
            role="replica" if i == 2 else "voter",
        )
        for i in range(3)
    ]
    try:
        for member in members:
            member.start()
        for member in members:
            wait(f"member {member.member} knows its leader", member.ready, timeout=90)
        exercise(members)
        print("ROLES-OK")
    finally:
        for member in members:
            member.stop()
        if os.environ.get("KEEP_ROLES_DATA") != "1":
            shutil.rmtree(root, ignore_errors=True)
        else:
            print(f"data kept in {root}")


if __name__ == "__main__":
    main()
