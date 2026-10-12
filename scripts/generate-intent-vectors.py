#!/usr/bin/env python3
"""Independent version-1 comparison vectors; no Rust implementation imports."""

import hashlib
import math
from pathlib import Path
import struct


PREFIX = b"WF-INTENT\0V1\0"


def encode(value, depth=1):
    tag, data = value
    nodes, deepest = 1, depth
    if tag in (0, 1, 2):
        payload = b""
    elif tag == 3:
        payload = struct.pack(">Q", data)
    elif tag == 4:
        payload = struct.pack(">q", data)
    elif tag == 5:
        payload = data.encode("utf-8")
    elif tag == 6:
        payload = data
    elif tag == 9:
        if not math.isfinite(data):
            raise ValueError("finite binary64 required")
        payload = struct.pack(">d", data)
    elif tag in (7, 8):
        if tag == 8:
            if len({key for key, _ in data}) != len(data):
                raise ValueError("unique keys required")
            children = []
            for key, item in sorted(data, key=lambda pair: pair[0].encode("utf-8")):
                children.extend([(5, key), item])
        else:
            children = data
        parts = [struct.pack(">Q", len(data))]
        for child in children:
            encoded, child_nodes, child_depth = encode(child, depth + 1)
            parts.append(encoded)
            nodes += child_nodes
            deepest = max(deepest, child_depth)
        payload = b"".join(parts)
    else:
        raise ValueError("unknown tag")
    return bytes([tag]) + struct.pack(">Q", len(payload)) + payload, nodes, deepest


def document(tenant, operation, arguments):
    return (8, [
        ("normalization_version", (5, "1")),
        ("schema_version", (5, "1")),
        ("tenant_id", (6, tenant)),
        ("operation", (5, operation)),
        ("args", (8, arguments)),
    ])


def policy_backoff(kind, base, maximum, jitter):
    return (8, [
        ("kind", (5, kind)), ("base_ms", (3, base)),
        ("max_ms", (3, maximum)), ("jitter_pct", (3, jitter)),
    ])


def policy_states(rows):
    return (7, [
        (8, [("state", (6, state)), ("change", change)])
        for state, change in sorted(rows, key=lambda row: row[0])
    ])


def policy_vectors():
    rich = [
        ("version", (6, b"\0\xff")),
        ("max_active_ms", (3, 31_536_000_000)),
        ("retry", (8, [
            ("max_retries", (3, 0)), ("backoff", policy_backoff("fixed", 0, 7, 100)),
            ("exhausted_to", (6, b"\xff")),
        ])),
        ("retention", (8, [("ttl_ms", (3, 1)), ("history_max_events", (3, 1))])),
        ("states", policy_states([
            (b"ready", (8, [
                ("mode", (5, "parallel")),
                ("retry", (8, [
                    ("max_retries", (3, 1_000)),
                    ("backoff", policy_backoff("none", 0, 0, 0)),
                    ("exhausted_to", (6, b"done")),
                ])),
                ("retention", (8, [
                    ("ttl_ms", (3, 31_536_000_000)),
                    ("history_max_events", (3, 1_000_000)),
                ])),
            ])),
            (b"\0\xff", (8, [("mode", (5, "fifo")), ("retention", (8, []))])),
        ])),
    ]
    clear = [(key, (0, None)) for key in ["version", "max_active_ms", "retry", "retention"]]
    clear.append(("states", policy_states([
        (b"ready", (0, None)),
        (b"\0\xff", (8, [
            ("mode", (0, None)), ("retry", (8, [("backoff", (8, []))])),
            ("retention", (0, None)),
        ])),
    ])))
    rows = []
    for name in ["policy_empty", "policy_number_zero", "policy_number_max", "policy_replace",
                 "policy_clear", "policy_linear", "policy_exponential"]:
        patch, mode, expected = [("states", (7, []))], "patch", None
        if name in ("policy_number_zero", "policy_number_max"):
            patch.append(("version", (3, 0 if name == "policy_number_zero" else 2**64 - 1)))
            mode, expected = "replace", 0
        elif name == "policy_replace":
            patch, mode, expected = rich, "replace", 0
        elif name == "policy_clear":
            patch, expected = clear, 1
        elif name in ("policy_linear", "policy_exponential"):
            patch.append(("retry", (8, [
                ("backoff", policy_backoff(name.removeprefix("policy_"), 11, 100, 0)),
            ])))
        arguments = [("workflow_type", (6, b"t\xff")), ("mode", (5, mode)), ("patch", (8, patch))]
        if expected is not None:
            arguments.append(("expected_generation", (3, expected)))
        rows.append((name, b"\0\xff", "workflow.policy_set", arguments))
    return rows


def claim_due_vectors():
    rows = []
    for name in ["defaults", "explicit_fixed_defaults", "auto", "any", "global",
                 "state_keep_last", "partition_keep_first", "values_omitted",
                 "values_empty", "opaque_governance_zero", "requested_now_zero",
                 "invalid_scalars"]:
        states = (8, [("any", (2, None)), ("names", (7, []))])
        partitions = (8, [("mode", (5, "auto")), ("keys", (7, []))])
        values = (8, [("mode", (5, "omitted")), ("names", (7, []))])
        lease, limit, ratio = 30_000, 1, 25
        options = []
        if name in ("any", "global"):
            partitions = (8, [("mode", (5, name)), ("keys", (7, []))])
        elif name == "state_keep_last":
            states = (8, [
                ("any", (1, None)),
                ("names", (7, [(6, b"ready"), (6, b"queued")])),
            ])
        elif name == "partition_keep_first":
            partitions = (8, [
                ("mode", (5, "only")),
                ("keys", (7, [(6, b"p2"), (6, b"p1")])),
            ])
        elif name == "values_empty":
            values = (8, [("mode", (5, "only")), ("names", (7, []))])
        elif name == "opaque_governance_zero":
            options = [
                ("governance_limit_scope", (6, b"\0\xff")),
                ("governance_shard_id", (3, 0)),
            ]
        elif name == "requested_now_zero":
            options = [("now_ms", (3, 0))]
        elif name == "invalid_scalars":
            lease, limit, ratio = 0, 0, 2**64 - 1
            options = [
                ("priority", (3, 2**64 - 1)),
                ("payload_max_bytes", (3, 2**64 - 1)),
            ]
        arguments = [
            ("workflow_type", (6, b"t\xff")), ("worker", (6, b"w\xff")),
            ("states", states), ("partitions", partitions), ("values", values),
            ("lease_ms", (3, lease)), ("limit", (3, limit)),
            ("reclaim_expired", (2, None)), ("reclaim_ratio", (3, ratio)),
            ("reply_mode", (5, "records")), ("payload", (2, None)),
        ] + options
        canonical, nodes, _ = encode(document(b"\0\xff", "workflow.claim_due", arguments))
        name_count = sum(len(value[1]) for key, value in [
            *states[1], *partitions[1], *values[1],
        ] if key in ("names", "keys"))
        assert nodes == 45 + 2 * len(options) + name_count
        assert canonical
        rows.append((name, b"\0\xff", "workflow.claim_due", arguments))
    return rows


def write_vectors(path, vectors):
    rows = ["# id\tcanonical_hex\tsha256\tvalue_count\tdepth"]
    for name, tenant, operation, arguments in vectors:
        encoded, nodes, depth = encode(document(tenant, operation, arguments))
        canonical = PREFIX + encoded
        rows.append("\t".join([
            name, canonical.hex(), hashlib.sha256(canonical).hexdigest(),
            str(nodes), str(depth),
        ]))
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text("\n".join(rows) + "\n", encoding="utf-8")
    print(f"Wrote {len(vectors)} independent vectors to {path}")


def main():
    mixed = [
        ("z", (0, None)), ("a", (6, b"\0\xff")), ("flag", (2, None)),
        ("neg", (4, -1)), ("u", (3, 2**64 - 1)), ("empty", (7, [])),
        ("text", (5, "é")), ("zero", (9, -0.0)), ("false", (1, None)),
    ]
    nested = [("items", (7, [
        (8, [("b", (6, b"")), ("a", (5, ""))]), (0, None), (3, 0),
    ]))]
    vectors = [
        ("empty", b"tenant", "create", []),
        ("mixed", b"t\0\xff", "create", mixed),
        ("reordered", b"t\0\xff", "create", list(reversed(mixed))),
        ("nested", "租户".encode(), "atomic_group_batch", nested),
        ("absent", b"tenant", "create", []),
        ("null", b"tenant", "create", [("payload", (0, None))]),
        ("empty_bytes", b"tenant", "create", [("payload", (6, b""))]),
        ("zero", b"tenant", "create", [("payload", (3, 0))]),
        ("positive_zero", b"tenant", "create", [("n", (9, 0.0))]),
        ("negative_zero", b"tenant", "create", [("n", (9, -0.0))]),
        ("signed_zero", b"tenant", "create", [("n", (4, 0))]),
        ("unsigned_zero", b"tenant", "create", [("n", (3, 0))]),
        ("string_zero", b"tenant", "create", [("n", (5, "0"))]),
        ("tenant_change", b"other", "create", []),
        ("operation_change", b"tenant", "complete", []),
        ("list_forward", b"tenant", "create", [("items", (7, [(3, 1), (3, 2)]))]),
        ("list_reverse", b"tenant", "create", [("items", (7, [(3, 2), (3, 1)]))]),
        ("unicode_keys", b"tenant", "create", [
            ("é", (0, None)), ("e\u0301", (0, None)), ("a", (0, None)),
            ("aa", (0, None)), ("", (0, None)),
        ]),
    ]
    directory = Path(__file__).resolve().parents[1] / "tests/intent"
    write_vectors(directory / "v1-vectors.tsv", vectors)
    write_vectors(directory / "policy-set-v1-vectors.tsv", policy_vectors())
    write_vectors(directory / "claim-due-v1-vectors.tsv", claim_due_vectors())


if __name__ == "__main__":
    main()
