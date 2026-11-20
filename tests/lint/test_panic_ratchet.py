"""Controls for the ADR-010 production panic-lint ratchet."""

import importlib.util
import json
from pathlib import Path
import tempfile
import unittest


def message(lint, file, line, column=1, level="warning", primary=True):
    return json.dumps(
        {
            "reason": "compiler-message",
            "message": {
                "level": level,
                "code": {"code": lint},
                "spans": [
                    {
                        "file_name": file,
                        "line_start": line,
                        "column_start": column,
                        "is_primary": primary,
                    }
                ],
            },
        }
    )


class PanicRatchetTests(unittest.TestCase):
    def setUp(self):
        path = Path(__file__).with_name("panic-ratchet.py")
        spec = importlib.util.spec_from_file_location("panic_ratchet", path)
        self.ratchet = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(self.ratchet)

    def test_counts_unique_primary_spans_of_ratcheted_lints_only(self):
        lines = [
            message("clippy::indexing_slicing", "crates/a/src/x.rs", 3),
            # The same span reported by two compilation units counts once.
            message("clippy::indexing_slicing", "crates/a/src/x.rs", 3),
            message("clippy::indexing_slicing", "crates/a/src/x.rs", 3, column=9),
            message("clippy::arithmetic_side_effects", "crates/a/src/x.rs", 4),
            message("clippy::panic_in_result_fn", "crates/b/src/y.rs", 1),
            message("clippy::needless_return", "crates/a/src/x.rs", 5),
            message("clippy::indexing_slicing", "crates/a/src/x.rs", 6, primary=False),
            message("clippy::indexing_slicing", "crates/a/src/x.rs", 7, level="note"),
            '{"reason": "build-finished", "success": true}',
            "not json",
        ]
        self.assertEqual(
            self.ratchet.count(lines),
            {
                "crates/a/src/x.rs": {
                    "clippy::arithmetic_side_effects": 1,
                    "clippy::indexing_slicing": 2,
                },
                "crates/b/src/y.rs": {"clippy::panic_in_result_fn": 1},
            },
        )

    def test_rises_and_new_files_fail_with_their_counts(self):
        baseline = {"crates/a/src/x.rs": {"clippy::indexing_slicing": 2}}
        current = {
            "crates/a/src/x.rs": {"clippy::indexing_slicing": 3},
            "crates/c/src/new.rs": {"clippy::arithmetic_side_effects": 1},
        }
        problems = self.ratchet.compare(baseline, current)
        self.assertEqual(len(problems), 2)
        self.assertIn("crates/a/src/x.rs: clippy::indexing_slicing rose from 2 to 3", problems)
        self.assertIn(
            "crates/c/src/new.rs: clippy::arithmetic_side_effects rose from 0 to 1", problems
        )

    def test_falls_and_removed_files_require_a_tighter_baseline(self):
        baseline = {
            "crates/a/src/x.rs": {"clippy::indexing_slicing": 2},
            "crates/a/src/gone.rs": {"clippy::panic_in_result_fn": 1},
        }
        current = {"crates/a/src/x.rs": {"clippy::indexing_slicing": 1}}
        problems = self.ratchet.compare(baseline, current)
        self.assertEqual(
            sorted(problems),
            [
                "crates/a/src/gone.rs: clippy::panic_in_result_fn fell from 1 to 0; run tests/lint/panic-ratchet.py --update",
                "crates/a/src/x.rs: clippy::indexing_slicing fell from 2 to 1; run tests/lint/panic-ratchet.py --update",
            ],
        )

    def test_equal_counts_pass(self):
        counts = {"crates/a/src/x.rs": {"clippy::indexing_slicing": 2}}
        self.assertEqual(self.ratchet.compare(counts, counts), [])

    def test_baseline_round_trips_and_rejects_unknown_schema_or_lints(self):
        counts = {"crates/a/src/x.rs": {"clippy::indexing_slicing": 2}}
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "baseline.json"
            self.ratchet.write_baseline(path, counts)
            self.assertEqual(self.ratchet.read_baseline(path), counts)
            document = json.loads(path.read_text())
            self.assertEqual(document["lints"], list(self.ratchet.LINTS))
            for broken in (
                {**document, "schema": 2},
                {**document, "lints": ["clippy::indexing_slicing"]},
                {**document, "files": {"x.rs": {"clippy::unknown": 1}}},
                {**document, "files": {"x.rs": {"clippy::indexing_slicing": 0}}},
            ):
                path.write_text(json.dumps(broken))
                with self.assertRaises(ValueError):
                    self.ratchet.read_baseline(path)

    def test_clippy_command_checks_production_targets_with_warnings_allowed(self):
        command, environment = self.ratchet.clippy_command({"CARGO_BUILD_WARNINGS": "deny"})
        self.assertEqual(command[:6], ["cargo", "clippy", "--workspace", "--lib", "--bins", "--locked"])
        self.assertNotIn("--all-targets", command)
        self.assertNotIn("--tests", command)
        for lint in self.ratchet.LINTS:
            self.assertIn(lint, command)
        self.assertEqual(environment["CARGO_BUILD_WARNINGS"], "warn")


if __name__ == "__main__":
    unittest.main()
