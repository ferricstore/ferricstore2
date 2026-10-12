#!/usr/bin/env python3
"""Independent private FWCP1 vectors; uses no Rust code or fixtures as input."""
import hashlib
import pathlib
import struct


def u8(value):
    return struct.pack(">B", value)


def u16(value):
    return struct.pack(">H", value)


def u32(value):
    return struct.pack(">I", value)


def u64(value):
    return struct.pack(">Q", value)


def raw(value):
    return u32(len(value)) + value


def option(value, encode):
    return b"\x00" if value is None else b"\x01" + encode(value)


def guard(fence=7):
    return raw(b"tok") + u64(fence)


def claim(state):
    return raw(state) + raw(b"w\xff") + raw(b"tok") + u64(30)


def item(tag, fields, identity=b"r1", priority=0):
    return raw(b"p\x00") + raw(identity) + u8(priority) + u64(11) + u8(tag) + fields


def frame(items):
    header = b"FWCP" + u16(1) + u8(1)
    header += b"".join(raw(value) for value in [b"\x00\xff", b"G", b"R", b"Q"])
    header += u16(1) + bytes(range(32)) + u64(11) + u16(2) + u64(7)
    return header + u32(len(items)) + b"".join(items)


def vectors():
    absent = item(1, raw(b"t") + raw(b"ready") + b"\x00\x00", priority=2)
    empty = item(1, raw(b"t") + raw(b"ready") + option(0, u64) + option(b"", raw), priority=2)
    cases = [
        ("create_absent", [absent]),
        ("create_empty", [empty]),
        ("claim", [item(2, claim(b"ready"))]),
        ("reclaim", [item(3, claim(b"running"))]),
        ("transition", [item(4, raw(b"running") + raw(b"next") + u64(2**64 - 1)
                             + option(b"tok", raw) + option(0, u64) + option(b"\x00\xff", raw))]),
        ("complete", [item(5, guard() + b"\x00" + option(b"", raw))]),
        ("fail", [item(6, guard() + option(b"x", raw) + option(b"\xff", raw))]),
        ("retry_explicit", [item(7, guard() + u8(0) + u64(0) + b"\x00\x00")]),
        ("retry_resolved", [item(7, guard() + u8(1) + u64(11)
                                + option(b"", raw) + option(b"err", raw))]),
        ("retry_policy", [item(8, guard() + b"\x00" + option(0, u16)
                              + b"\x01" + u8(1) + u64(0) + u64(7) + u8(100)
                              + option(b"failed", raw) + u32(2**32 - 1)
                              + option(b"", raw) + option(b"", raw))]),
        ("extend_lease", [item(9, guard() + u64(2**64 - 1))]),
        ("cancel_absent", [item(10, u64(0) + b"\x00\x00\x00\x00")]),
        ("cancel_full", [item(10, u64(7) + option(b"tok", raw) + option(b"ready", raw)
                             + option(0, u64) + option(b"", raw))]),
        ("batch_forward", [absent, item(2, claim(b"ready"), identity=b"r2")]),
        ("batch_reverse", [item(2, claim(b"ready"), identity=b"r2"), absent]),
    ]
    return [(name, frame(items)) for name, items in cases]


def patch(tag=0, value=b""):
    return u8(tag) + value


def backoff(value):
    kind, base, maximum, jitter = value
    return u8(kind) + u64(base) + u64(maximum) + u8(jitter)


def backoff_patch(value):
    return b"".join(option(field, encode) for field, encode in zip(value, (u8, u64, u64, u8)))


def retry(maximum=None, timing=None, target=None, partial=False):
    return (option(maximum, u16) + option(timing, backoff_patch if partial else backoff)
            + option(target, raw))


def retention(ttl=None, history=None):
    return option(ttl, u64) + option(history, u32)


def states(values):
    return u32(len(values)) + b"".join(raw(name) + body for name, body in sorted(values))


def snapshot(version=None, active=None, retry_fields=b"\x00\x00\x00",
             retention_fields=b"\x00\x00", state_fields=()):
    return (b"FWPS" + u16(1) + raw(b"t\xff") + option(version, lambda value: value)
            + option(active, u64) + retry_fields + retention_fields + states(state_fields))


def install(generation, mode, expected, fields, final):
    envelope = b"FWPV" + u16(1) + u64(generation) + raw(final)
    header = b"FWCP" + u16(1) + u8(2)
    header += b"".join(raw(value) for value in [b"\x00\xff", b"G", b"R", b"Q"])
    header += u16(1) + bytes(range(32)) + u64(11) + u16(2) + u64(7)
    command = raw(b"t\xff") + option(expected, u64) + u8(mode) + fields
    return header + command + raw(envelope), envelope


def policy_vectors():
    empty = patch() * 4 + states(())
    cases = [("policy_empty", *install(1, 0, None, empty, snapshot()))]
    for name, number, generation in [("policy_number_zero", 0, 1),
                                      ("policy_number_max", 2**64 - 1, 2**53 - 1)]:
        version = u8(1) + u64(number)
        fields = patch(1, version) + patch() * 3 + states(())
        cases.append((name, *install(generation, 1, 0, fields, snapshot(version=version))))
    rich_retry = retry(0, (1, 0, 7, 100), b"\xff")
    ready_retry = retry(1000, (0, 0, 0, 0), b"done")
    retained = retention(31_536_000_000, 1_000_000)
    rich_states = [
        (b"ready", u8(1) + patch(1, u8(0))
         + patch(1, retry(1000, (0, 0, 0, 0), b"done", partial=True)) + patch(1, retained)),
        (b"\x00\xff", u8(1) + patch(1, u8(1)) + patch() + patch(1, retention())),
    ]
    fields = (patch(1, u8(0) + raw(b"\x00\xff")) + patch(1, u64(31_536_000_000))
              + patch(1, retry(0, (1, 0, 7, 100), b"\xff", partial=True))
              + patch(1, retention(1, 1)) + states(rich_states))
    final = snapshot(version=u8(0) + raw(b"\x00\xff"), active=31_536_000_000,
                     retry_fields=rich_retry, retention_fields=retention(1, 1), state_fields=[
                         (b"ready", option(0, u8) + ready_retry + retained),
                         (b"\x00\xff", option(1, u8) + retry() + retention()),
                     ])
    cases.append(("policy_replace", *install(1, 1, 0, fields, final)))
    cleared_states = [
        (b"ready", u8(0)),
        (b"\x00\xff", u8(1) + patch(2)
         + patch(1, retry(timing=(None, None, None, None), partial=True)) + patch(2)),
    ]
    fields = patch(2) * 4 + states(cleared_states)
    final = snapshot(state_fields=[
        (b"\x00\xff", b"\x00" + retry(timing=(3, 1000, 30_000, 20)) + retention()),
    ])
    cases.append(("policy_clear", *install(2, 0, 1, fields, final)))
    for name, kind in [("policy_linear", 2), ("policy_exponential", 3)]:
        timing = (kind, 11, 100, 0)
        fields = patch() * 2 + patch(1, retry(timing=timing, partial=True)) + patch() + states(())
        cases.append((name, *install(1, 0, None, fields, snapshot(retry_fields=retry(timing=timing)))))
    return cases


def components(values):
    return u32(len(values)) + b"".join(raw(value) for value in values)


def claim_phase(workflow_type=b"t\xff", states=None, exclude_states=(),
                partitions=None, priority=None, worker=b"w\xff", lease_ms=50,
                limit=1, scan_budget=3, head_budget=4, token_version=1,
                first_ordinal=2**32 - 1):
    header = b"FWCP" + u16(1) + u8(3)
    header += b"".join(raw(value) for value in [b"\x00\xff", b"G", b"R", b"Q"])
    header += u16(1) + bytes(range(32)) + u64(11) + u16(2) + u64(7)
    return (header + raw(workflow_type) + option(states, components)
            + components(exclude_states) + option(partitions, components)
            + option(priority, u8) + raw(worker) + u64(lease_ms) + u32(limit)
            + u32(scan_budget) + u32(head_budget) + u16(token_version)
            + u32(first_ordinal))


def claim_phase_vectors():
    cases = [
        ("claim_any", {}),
        ("claim_states_empty", {"states": []}),
        ("claim_states", {"states": [b"waiting", b"running"]}),
        ("claim_states_reverse", {"states": [b"running", b"waiting"]}),
        ("claim_excluded", {"exclude_states": [b"waiting", b"signal_wait"]}),
        ("claim_excluded_reverse", {"exclude_states": [b"signal_wait", b"waiting"]}),
        ("claim_partitions", {"partitions": [b"\x00\xff", b"p"]}),
        ("claim_partitions_reverse", {"partitions": [b"p", b"\x00\xff"]}),
        ("claim_partitions_empty", {"partitions": []}),
        ("claim_priority_zero", {"priority": 0}),
        ("claim_priority_max", {"priority": 255}),
        ("claim_empty_type", {"workflow_type": b""}),
        ("claim_empty_worker", {"worker": b""}),
        ("claim_lease_zero", {"lease_ms": 0}),
        ("claim_lease_max", {"lease_ms": 2**64 - 1}),
        ("claim_budgets_zero", {"limit": 0, "scan_budget": 0, "head_budget": 0}),
        ("claim_budgets_max", {"limit": 2**32 - 1, "scan_budget": 2**32 - 1,
                               "head_budget": 2**32 - 1}),
        ("claim_token_zero", {"token_version": 0}),
        ("claim_token_max", {"token_version": 2**16 - 1}),
        ("claim_ordinal_zero", {"first_ordinal": 0}),
        ("claim_rich", {"states": [b"waiting", b"running"], "exclude_states": [b"waiting"],
                        "partitions": [b"\x00\xff", b"p"], "priority": 2, "lease_ms": 7,
                        "limit": 5, "scan_budget": 9, "head_budget": 11, "first_ordinal": 7}),
    ]
    return [(name, claim_phase(**fields)) for name, fields in cases]


def main():
    destination = pathlib.Path(__file__).resolve().parents[1] / "tests/prepared/v1-vectors.tsv"
    destination.parent.mkdir(parents=True, exist_ok=True)
    lines = ["# name\tcanonical_hex\tsha256"]
    lines += [f"{name}\t{data.hex()}\t{hashlib.sha256(data).hexdigest()}" for name, data in vectors()]
    destination.write_text("\n".join(lines) + "\n", encoding="utf-8")
    print(f"wrote {len(lines) - 1} independent vectors to {destination}")
    destination = destination.with_name("policy-v1-vectors.tsv")
    lines = ["# name\tcanonical_hex\tsha256\tpolicy_envelope_hex"]
    lines += [f"{name}\t{data.hex()}\t{hashlib.sha256(data).hexdigest()}\t{envelope.hex()}"
              for name, data, envelope in policy_vectors()]
    destination.write_text("\n".join(lines) + "\n", encoding="utf-8")
    print(f"wrote {len(lines) - 1} independent policy vectors to {destination}")
    destination = destination.with_name("claim-phase-v1-vectors.tsv")
    lines = ["# name\tcanonical_hex\tsha256"]
    lines += [f"{name}\t{data.hex()}\t{hashlib.sha256(data).hexdigest()}"
              for name, data in claim_phase_vectors()]
    destination.write_text("\n".join(lines) + "\n", encoding="utf-8")
    print(f"wrote {len(lines) - 1} independent claim vectors to {destination}")


if __name__ == "__main__":
    main()
