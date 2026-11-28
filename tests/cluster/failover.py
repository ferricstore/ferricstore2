"""Three-member failover through the Python SDK (ADR-018 phase 4).

Starts three ferricstore2 members on this machine, writes a Flow through
the SDK's topology clients, stops the member that leads the Flow's range
as `docker compose stop -t 1` does (SIGTERM, SIGKILL a second later), and
checks what the SDK's own cluster failover test checks, less its KV steps
(the node has no KV commands): writes on the old route fail, the sync and
async clients reroute to a new leader, the Flow and a query over it
survive, new writes commit on the new leader, and the stopped member
rejoins and catches up.

Usage, with a Python that imports the `ferricstore` SDK:
    <sdk>/.venv/bin/python tests/cluster/failover.py [path/to/ferricstore2]
The binary defaults to target/release/ferricstore2. Prints FAILOVER-OK.
"""

from __future__ import annotations

import asyncio
import os
import shutil
import signal
import socket
import subprocess
import sys
import tempfile
import time
import urllib.request
import uuid
from pathlib import Path
from typing import Any

from ferricstore import AsyncFlowClient, FerricStoreError, FlowClient, JsonCodec

ROOT = Path(__file__).resolve().parents[2]
STALE = (FerricStoreError, OSError, TimeoutError, ConnectionError)
QUERY = (
    "FROM runs WHERE partition_key = @partition AND type = @type AND state = @state "
    "ORDER BY updated_at_ms ASC LIMIT 10 RETURN RECORDS"
)


def free_ports(count: int) -> list[int]:
    sockets = [socket.socket() for _ in range(count)]
    for sock in sockets:
        sock.bind(("127.0.0.1", 0))
    ports = [sock.getsockname()[1] for sock in sockets]
    for sock in sockets:
        sock.close()
    return ports


class Member:
    """One member process; `start` again reopens its data directory."""

    def __init__(
        self,
        binary: Path,
        member: int,
        nodes: str,
        ports: tuple[int, int, int],
        data: Path,
        role: str = "voter",
    ):
        self.binary, self.member, self.nodes = binary, member, nodes
        self.native, self.peer, self.health = ports
        self.data, self.role = data, role
        self.process: subprocess.Popen[bytes] | None = None

    @property
    def url(self) -> str:
        return f"ferric://127.0.0.1:{self.native}"

    def start(self) -> None:
        env = dict(
            os.environ,
            FERRICSTORE_DISCOVERY="static",
            FERRICSTORE_CLUSTER_NODES=self.nodes,
            FERRICSTORE_NODE_ID=str(self.member),
            FERRICSTORE_NATIVE_PORT=str(self.native),
            FERRICSTORE_PEER_PORT=str(self.peer),
            FERRICSTORE_HEALTH_PORT=str(self.health),
            FERRICSTORE_CLUSTER_ROLE=self.role,
        )
        with open(self.data.with_suffix(".log"), "ab") as log:
            self.process = subprocess.Popen(
                [str(self.binary), "--data-dir", str(self.data)],
                env=env,
                stdout=log,
                stderr=log,
            )

    def stop(self) -> None:
        """SIGTERM, then SIGKILL after a second, as `docker compose stop -t 1`."""
        if self.process is None or self.process.poll() is not None:
            return
        self.process.send_signal(signal.SIGTERM)
        try:
            self.process.wait(timeout=1)
        except subprocess.TimeoutExpired:
            self.process.kill()
            self.process.wait()

    def ready(self) -> bool:
        """Whether the member knows its leader (`/health/ready`)."""
        try:
            with urllib.request.urlopen(
                f"http://127.0.0.1:{self.health}/health/ready", timeout=1
            ) as reply:
                return reply.status == 200
        except OSError:
            return False


def wait(what: str, check: Any, timeout: float = 60.0) -> Any:
    deadline = time.monotonic() + timeout
    last: BaseException | None = None
    while time.monotonic() < deadline:
        try:
            result = check()
            if result:
                return result
        except Exception as error:  # noqa: BLE001 - kept to report the last one
            last = error
        time.sleep(0.2)
    raise AssertionError(f"{what} within {timeout} s; last error: {last!r}")


async def await_until(what: str, check: Any, timeout: float = 60.0) -> Any:
    deadline = time.monotonic() + timeout
    last: BaseException | None = None
    while time.monotonic() < deadline:
        try:
            result = await check()
            if result:
                return result
        except Exception as error:  # noqa: BLE001 - kept to report the last one
            last = error
        await asyncio.sleep(0.2)
    raise AssertionError(f"{what} within {timeout} s; last error: {last!r}")


def topology_client(urls: list[str]) -> FlowClient:
    return wait(
        "a topology client opened",
        lambda: FlowClient.from_urls(
            urls, codec=JsonCodec(), endpoint_policy="any", max_connections=2, timeout=1.0
        ),
    )


async def exercise(members: list[Member]) -> None:
    urls = [member.url for member in members]
    sync_client = topology_client(urls)
    async_client = AsyncFlowClient.from_urls(
        urls, codec=JsonCodec(), endpoint_policy="any", max_connections=2, timeout=1.0
    )
    key = f"{{failover:{uuid.uuid4().hex}}}:p"
    flow_type = f"failover-{uuid.uuid4().hex}"
    params = {"partition": key, "type": flow_type, "state": "ready"}

    def create(client: FlowClient, suffix: str) -> Any:
        return client.create(
            f"{key}:{suffix}",
            type=flow_type,
            state="ready",
            partition_key=key,
            now_ms=int(time.time() * 1000),
            idempotent=True,
        )

    async def ids() -> set[str]:
        result = await async_client.query(QUERY, params)
        return {_text(record.get("id", record.get(b"id"))) for record in (result.records or [])}

    stopped: Member | None = None
    try:
        await async_client.refresh_topology()
        before = sync_client.route(key)
        old_leader = str(before["leader_node"])
        port = int(before["endpoint"]["native_port"])
        stopped = next(member for member in members if member.native == port)
        create(sync_client, "before")
        await await_until("the first Flow is visible", lambda: _has(ids, {f"{key}:before"}))

        stopped.stop()
        try:
            create(sync_client, "stale-sync")
        except STALE:
            pass
        else:
            raise AssertionError("a sync write on the old route succeeded")
        try:
            await async_client.create(
                f"{key}:stale-async",
                type=flow_type,
                state="ready",
                partition_key=key,
                now_ms=int(time.time() * 1000),
                idempotent=True,
            )
        except STALE:
            pass
        else:
            raise AssertionError("an async write on the old route succeeded")

        async def rerouted() -> bool:
            sync_client.refresh_topology()
            await async_client.refresh_topology()
            sync_route = sync_client.route(key)
            async_route = await async_client.route(key)
            return (
                str(sync_route["leader_node"]) != old_leader
                and str(async_route["leader_node"]) != old_leader
            )

        await await_until("both clients reroute to a new leader", rerouted, timeout=90)
        create(sync_client, "after-sync")
        await async_client.create(
            f"{key}:after-async",
            type=flow_type,
            state="ready",
            partition_key=key,
            now_ms=int(time.time() * 1000),
            idempotent=True,
        )
        expected = {f"{key}:before", f"{key}:after-sync", f"{key}:after-async"}
        try:
            await await_until("every acknowledged Flow is visible", lambda: _has(ids, expected))
        except AssertionError:
            for member in members:
                if member is stopped:
                    continue
                direct = AsyncFlowClient.from_url(member.url, codec=JsonCodec(), timeout=2.0)
                try:
                    result = await direct.query(QUERY, params)
                    found = sorted(_text(r.get("id", r.get(b"id"))) for r in (result.records or []))
                    print(f"member {member.member}: {found}", flush=True)
                except Exception as error:  # noqa: BLE001 - reported
                    print(f"member {member.member}: {error!r}", flush=True)
                finally:
                    await direct.close()
            print(f"expected {sorted(expected)}; leader was {old_leader}", flush=True)
            raise

        stopped.start()
        wait("the stopped member rejoins and knows its leader", stopped.ready, timeout=90)
        stopped = None
        create(sync_client, "rejoined")
        expected.add(f"{key}:rejoined")
        await await_until("writes commit with the member back", lambda: _has(ids, expected))
    finally:
        if stopped is not None:
            stopped.start()
        # A client kept a connection to the member that was killed; closing
        # it may report the reset.
        try:
            sync_client.close()
        finally:
            try:
                await async_client.close()
            except OSError:
                pass


def _text(value: Any) -> str:
    return value.decode() if isinstance(value, bytes) else str(value)


async def _has(ids: Any, expected: set[str]) -> bool:
    found = await ids()
    return expected <= found


def main() -> None:
    binary = Path(sys.argv[1]) if len(sys.argv) > 1 else ROOT / "target/release/ferricstore2"
    ports = free_ports(9)
    nodes = ",".join(f"127.0.0.1:{ports[i]}:{ports[3 + i]}" for i in range(3))
    root = Path(tempfile.mkdtemp(prefix="ferricstore2-failover-"))
    members = [
        Member(binary, i + 1, nodes, (ports[i], ports[3 + i], ports[6 + i]), root / f"n{i + 1}")
        for i in range(3)
    ]
    try:
        for member in members:
            member.start()
        for member in members:
            wait(f"member {member.member} knows its leader", member.ready, timeout=90)
        asyncio.run(exercise(members))
        print("FAILOVER-OK")
    finally:
        for member in members:
            member.stop()
        if os.environ.get("KEEP_FAILOVER_DATA") != "1":
            shutil.rmtree(root, ignore_errors=True)
        else:
            print(f"data kept in {root}")


if __name__ == "__main__":
    main()
