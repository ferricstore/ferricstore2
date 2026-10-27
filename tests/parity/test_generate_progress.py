"""Regression controls for lossless reviewed-link bookkeeping, not parity."""

import copy
import hashlib
import importlib.util
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch


class ProgressGeneratorTests(unittest.TestCase):
    def setUp(self):
        path = Path(__file__).with_name("generate-progress.py")
        spec = importlib.util.spec_from_file_location("progress_generator", path)
        self.generator = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(self.generator)
        self.directory = tempfile.TemporaryDirectory()
        self.addCleanup(self.directory.cleanup)
        self.root = Path(self.directory.name)
        manifest = {
            "source_commit": self.generator.SOURCE_COMMIT,
            "declarations": [{"id": "first"}, {"id": "second"}],
        }
        raw = json.dumps(manifest).encode()
        manifest_path = self.root / "tests/reference/workflow-test-manifest.json"
        manifest_path.parent.mkdir(parents=True)
        manifest_path.write_bytes(raw)
        self.path = self.root / "tests/parity/workflow-progress.json"
        self.path.parent.mkdir(parents=True)
        self.manifest_hash = hashlib.sha256(raw).hexdigest()
        self.manual = {"scenario_id": "manual", "review": "preserve verbatim"}
        self.generated = {"scenario_id": "generated", "review": "fresh anchors"}
        self.ledger = {
            "schema_version": 1,
            "source_commit": self.generator.SOURCE_COMMIT,
            "reference_manifest_sha256": self.manifest_hash,
            "runtime_collection": None,
            "declarations": [
                {"source_declaration_id": "first", "runtime_case_ids": [],
                 "rust_assertions": [self.manual]},
                {"source_declaration_id": "second", "runtime_case_ids": [],
                 "rust_assertions": [{"scenario_id": "generated", "review": "old anchors"}]},
            ],
        }
        for name, value in [
            ("ROOT", self.root),
            ("MANIFEST_SHA256", self.manifest_hash),
        ]:
            replacement = patch.object(self.generator, name, value)
            replacement.start()
            self.addCleanup(replacement.stop)
        replacement = patch.object(
            self.generator, "counterparts", return_value={"second": [self.generated]}
        )
        replacement.start()
        self.addCleanup(replacement.stop)

    def write_ledger(self, ledger=None):
        self.path.write_text(json.dumps(self.ledger if ledger is None else ledger))

    def assert_refused_without_write(self, ledger, collection=None):
        self.write_ledger(ledger)
        before = self.path.read_bytes()
        with self.assertRaises(ValueError):
            self.generator.generate(collection)
        self.assertEqual(self.path.read_bytes(), before)

    def test_preserves_manual_and_refreshes_generated_links(self):
        self.write_ledger()
        self.generator.generate(None)
        generated = json.loads(self.path.read_bytes())
        self.assertEqual(generated["declarations"][0]["rust_assertions"], [self.manual])
        self.assertEqual(generated["declarations"][1]["rust_assertions"], [self.generated])
        before = self.path.read_bytes()
        self.generator.generate(None)
        self.assertEqual(self.path.read_bytes(), before)

    def test_rejects_bad_provenance(self):
        for field, value in [
            ("schema_version", 2),
            ("source_commit", "different baseline"),
            ("reference_manifest_sha256", "0" * 64),
        ]:
            with self.subTest(field=field):
                ledger = copy.deepcopy(self.ledger)
                ledger[field] = value
                self.assert_refused_without_write(ledger)

    def test_rejects_bad_declaration_accounting(self):
        for mode in ["missing", "duplicate", "unknown"]:
            with self.subTest(mode=mode):
                ledger = copy.deepcopy(self.ledger)
                if mode == "missing":
                    ledger["declarations"].pop()
                elif mode == "duplicate":
                    ledger["declarations"].append(copy.deepcopy(ledger["declarations"][0]))
                else:
                    ledger["declarations"][0]["source_declaration_id"] = "unknown"
                self.assert_refused_without_write(ledger)

    def test_rejects_duplicate_scenarios(self):
        ledger = copy.deepcopy(self.ledger)
        ledger["declarations"][0]["rust_assertions"].append(copy.deepcopy(self.manual))
        self.assert_refused_without_write(ledger)
        ledger = copy.deepcopy(self.ledger)
        ledger["declarations"][0]["rust_assertions"] = [copy.deepcopy(self.generated)]
        self.assert_refused_without_write(ledger)

    def test_preserves_existing_runtime_bindings(self):
        collection = {
            "source_commit": self.generator.SOURCE_COMMIT,
            "reference_manifest_sha256": self.manifest_hash,
            "cases": [{"source_declaration_id": "first", "case_id": "registered"}],
        }
        collection_path = self.root / "tests/parity/registration.json"
        collection_path.write_text(json.dumps(collection))
        ledger = copy.deepcopy(self.ledger)
        ledger["runtime_collection"] = {
            "path": "tests/parity/registration.json",
            "sha256": hashlib.sha256(collection_path.read_bytes()).hexdigest(),
        }
        ledger["declarations"][0]["runtime_case_ids"] = ["registered"]
        self.assert_refused_without_write(ledger)
        self.generator.generate("tests/parity/registration.json")
        result = json.loads(self.path.read_bytes())
        self.assertEqual(result["runtime_collection"], ledger["runtime_collection"])
        self.assertEqual(result["declarations"][0]["runtime_case_ids"], ["registered"])
        self.assertEqual(result["declarations"][0]["rust_assertions"], [self.manual])


if __name__ == "__main__":
    unittest.main()
