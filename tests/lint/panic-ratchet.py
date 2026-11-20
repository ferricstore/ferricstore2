#!/usr/bin/env python3
"""ADR-010 production panic-lint ratchet.

Counts Clippy's panic-prone lints in production code (library and binary
targets, without test configuration) per file and compares them with the
committed baseline. A count may never rise, and a new file starts at zero, so
new code is held to the lints while existing code is paid down. A count that
falls fails too, until the baseline is rewritten with --update, so the baseline
always states the real debt.
"""

import json
import os
from pathlib import Path
import subprocess
import sys

LINTS = (
    "clippy::arithmetic_side_effects",
    "clippy::indexing_slicing",
    "clippy::panic_in_result_fn",
)
SCHEMA = 1
ROOT = Path(__file__).resolve().parents[2]
BASELINE = ROOT / "architecture/panic-baseline.json"
UPDATE_HINT = "run tests/lint/panic-ratchet.py --update"


def clippy_command(environment):
    command = ["cargo", "clippy", "--workspace", "--lib", "--bins", "--locked"]
    command += ["--message-format=json", "--"]
    for lint in LINTS:
        command += ["-W", lint]
    # The ratchet reads the warnings; the warning-denied gate runs separately.
    return command, {**environment, "CARGO_BUILD_WARNINGS": "warn"}


def count(lines):
    seen = set()
    counts = {}
    for line in lines:
        try:
            record = json.loads(line)
        except json.JSONDecodeError:
            continue
        if record.get("reason") != "compiler-message":
            continue
        diagnostic = record["message"]
        lint = (diagnostic.get("code") or {}).get("code")
        if lint not in LINTS or diagnostic.get("level") != "warning":
            continue
        for span in diagnostic.get("spans", []):
            if not span.get("is_primary"):
                continue
            key = (span["file_name"], span["line_start"], span["column_start"], lint)
            if key in seen:
                continue
            seen.add(key)
            per_file = counts.setdefault(span["file_name"], {})
            per_file[lint] = per_file.get(lint, 0) + 1
    return counts


def compare(baseline, current):
    problems = []
    for file in sorted(set(baseline) | set(current)):
        for lint in LINTS:
            before = baseline.get(file, {}).get(lint, 0)
            now = current.get(file, {}).get(lint, 0)
            if now > before:
                problems.append(f"{file}: {lint} rose from {before} to {now}")
            elif now < before:
                problems.append(f"{file}: {lint} fell from {before} to {now}; {UPDATE_HINT}")
    return problems


def write_baseline(path, counts):
    document = {
        "schema": SCHEMA,
        "lints": list(LINTS),
        "files": {file: dict(sorted(counts[file].items())) for file in sorted(counts)},
    }
    path.write_text(json.dumps(document, indent=2) + "\n")


def read_baseline(path):
    document = json.loads(path.read_text())
    if document.get("schema") != SCHEMA or document.get("lints") != list(LINTS):
        raise ValueError(f"{path}: baseline schema or lint list differs; {UPDATE_HINT}")
    files = document.get("files")
    if not isinstance(files, dict):
        raise ValueError(f"{path}: baseline files must be an object")
    for file, lints in files.items():
        for lint, value in lints.items():
            if lint not in LINTS or not isinstance(value, int) or value < 1:
                raise ValueError(f"{path}: {file} has invalid entry {lint}={value!r}")
    return files


def run_clippy():
    command, environment = clippy_command(dict(os.environ))
    completed = subprocess.run(
        command, cwd=ROOT, env=environment, capture_output=True, text=True, check=False
    )
    if completed.returncode != 0:
        sys.stderr.write(completed.stderr)
        raise SystemExit("panic ratchet: Clippy failed")
    return count(completed.stdout.splitlines())


def main(arguments):
    if arguments not in ([], ["--update"]):
        raise SystemExit("usage: tests/lint/panic-ratchet.py [--update]")
    current = run_clippy()
    if arguments == ["--update"]:
        write_baseline(BASELINE, current)
        total = sum(sum(lints.values()) for lints in current.values())
        print(f"panic ratchet: baseline rewritten with {total} sites in {len(current)} files")
        return 0
    problems = compare(read_baseline(BASELINE), current)
    for problem in problems:
        print(f"panic ratchet: {problem}", file=sys.stderr)
    if problems:
        print(
            "panic ratchet: new production code must not index, slice, use unchecked "
            "arithmetic, or panic inside Result functions (ADR-010)",
            file=sys.stderr,
        )
        return 1
    print("panic ratchet: no production file gained panic-prone sites")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
