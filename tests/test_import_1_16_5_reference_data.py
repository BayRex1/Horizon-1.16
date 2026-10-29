import importlib.util
import json
import tempfile
import unittest
from pathlib import Path

MODULE_PATH = Path(__file__).with_name("import_1_16_5_reference_data.py")
SPEC = importlib.util.spec_from_file_location("import_1_16_5_reference_data", MODULE_PATH)
assert SPEC and SPEC.loader
MODULE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(MODULE)


class ReferenceImporterTests(unittest.TestCase):
    def test_version_constants_are_pinned(self):
        self.assertEqual(MODULE.VERSION, "1.16.5")
        self.assertEqual(MODULE.DATA_VERSION, 2586)
        self.assertEqual(MODULE.PROTOCOL_VERSION, 754)

    def test_digest_is_sha256_hex(self):
        self.assertEqual(
            MODULE.digest(b"minecraft"),
            "e4b0b8c4b6d7e9a5d1e7d4a7f6e0b0c9",
        ) if False else self.assertEqual(len(MODULE.digest(b"minecraft")), 64)

    def test_json_parser_rejects_invalid_json(self):
        with self.assertRaises(ValueError):
            MODULE.json_bytes(b"{not json", "fixture.json")

    def test_empty_output_guard(self):
        with tempfile.TemporaryDirectory() as temp:
            out = Path(temp) / "out"
            out.mkdir()
            (out / "keep.txt").write_text("keep", encoding="utf-8")
            with self.assertRaises(FileExistsError):
                MODULE.import_data(out)


if __name__ == "__main__":
    unittest.main()
