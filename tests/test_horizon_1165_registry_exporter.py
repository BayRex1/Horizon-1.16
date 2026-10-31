from __future__ import annotations

import importlib.util
import unittest
from pathlib import Path

SCRIPT = Path(__file__).resolve().parents[1] / "tools" / "export_1_16_5_registry.py"
SPEC = importlib.util.spec_from_file_location("horizon_1165_registry_exporter", SCRIPT)
assert SPEC is not None and SPEC.loader is not None
EXPORTER = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(EXPORTER)


def fixture() -> dict:
    return {
        "minecraft:air": {"states": [{"id": 0, "properties": {}, "default": True}]},
        "minecraft:stone": {"states": [{"id": 1, "properties": {}, "default": True}]},
        "minecraft:oak_slab": {"states": [
            {"id": 2, "properties": {"type": "bottom", "waterlogged": "false"}, "default": True},
            {"id": 3, "properties": {"type": "bottom", "waterlogged": "true"}},
        ]},
        "minecraft:water": {"states": [
            {"id": 4, "properties": {"level": "0"}, "default": True},
            {"id": 5, "properties": {"level": "1"}},
        ]},
    }


class Minecraft1165RegistryExporterTests(unittest.TestCase):
    def test_version_state_ids_and_defaults(self) -> None:
        result = EXPORTER.export_report(fixture())
        self.assertEqual((result["minecraft_version"], result["data_version"]), ("1.16.5", 2586))
        self.assertEqual(len(result["states"]), 6)
        self.assertEqual(result["defaults"], {
            "minecraft:air": 0, "minecraft:oak_slab": 2,
            "minecraft:stone": 1, "minecraft:water": 4,
        })

    def test_air_and_fluid_flags(self) -> None:
        rows = EXPORTER.export_report(fixture())["states"]
        self.assertTrue(rows[0][1])
        self.assertFalse(rows[1][1])
        self.assertFalse(rows[2][2])
        self.assertTrue(rows[3][2])
        self.assertTrue(rows[4][2])
        self.assertTrue(rows[5][2])

    def test_state_properties_are_preserved(self) -> None:
        rows = EXPORTER.export_report(fixture())["states"]
        self.assertEqual(rows[2][4], {
            "id": "minecraft:oak_slab",
            "properties": {"type": "bottom", "waterlogged": "false"},
        })

    def test_duplicate_and_non_contiguous_ids_are_rejected(self) -> None:
        duplicate = fixture()
        duplicate["minecraft:stone"]["states"][0]["id"] = 0
        with self.assertRaisesRegex(ValueError, "duplicate global state id"):
            EXPORTER.export_report(duplicate)
        gap = fixture()
        gap["minecraft:water"]["states"][1]["id"] = 7
        with self.assertRaisesRegex(ValueError, "contiguous"):
            EXPORTER.export_report(gap)

    def test_exactly_one_default_is_required(self) -> None:
        report = fixture()
        report["minecraft:stone"]["states"][0]["default"] = False
        with self.assertRaisesRegex(ValueError, "exactly one default"):
            EXPORTER.export_report(report)

    def test_empty_and_malformed_reports_are_rejected(self) -> None:
        with self.assertRaisesRegex(ValueError, "non-empty"):
            EXPORTER.export_report({})
        with self.assertRaisesRegex(ValueError, "missing states array"):
            EXPORTER.export_report({"minecraft:stone": {}})
        with self.assertRaisesRegex(ValueError, "empty states array"):
            EXPORTER.export_report({"minecraft:stone": {"states": []}})


if __name__ == "__main__":
    unittest.main()
