import importlib.util
import unittest
from pathlib import Path

MODULE_PATH = Path(__file__).resolve().parents[1] / "tools" / "export_1_16_5_registry.py"
SPEC = importlib.util.spec_from_file_location("export_1_16_5_registry", MODULE_PATH)
exporter = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(exporter)


class RegistryExporter1165Tests(unittest.TestCase):
    def test_exports_states_in_global_id_order_and_defaults(self):
        report = {
            "minecraft:stone": {"states": [{"id": 1, "default": True}]},
            "minecraft:air": {"states": [{"id": 0, "default": True}]},
            "minecraft:oak_slab": {"states": [
                {"id": 2, "default": True, "properties": {"type": "bottom"}},
                {"id": 3, "properties": {"type": "top"}},
            ]},
        }
        result = exporter.export_report(report)
        self.assertEqual(result["data_version"], 2586)
        self.assertEqual([row[0] for row in result["states"]],
                         ["minecraft:air", "minecraft:stone", "minecraft:oak_slab", "minecraft:oak_slab"])
        self.assertEqual(result["defaults"]["minecraft:oak_slab"], 2)
        self.assertEqual(result["states"][0][1:3], [True, False])
        self.assertEqual(result["states"][3][4]["properties"], {"type": "top"})

    def test_water_and_waterlogged_are_marked_fluid(self):
        report = {
            "minecraft:water": {"states": [{"id": 0, "default": True}]},
            "minecraft:oak_log": {"states": [
                {"id": 1, "default": True, "properties": {"waterlogged": "true"}}
            ]},
        }
        result = exporter.export_report(report)
        self.assertTrue(result["states"][0][2])
        self.assertTrue(result["states"][1][2])

    def test_rejects_non_contiguous_ids(self):
        report = {"minecraft:stone": {"states": [{"id": 4, "default": True}]}}
        with self.assertRaisesRegex(ValueError, "contiguous"):
            exporter.export_report(report)

    def test_rejects_missing_or_multiple_defaults(self):
        for states in (
            [{"id": 0}],
            [{"id": 0, "default": True}, {"id": 1, "default": True}],
        ):
            with self.subTest(states=states):
                with self.assertRaisesRegex(ValueError, "exactly one default"):
                    exporter.export_report({"minecraft:stone": {"states": states}})

    def test_rejects_duplicate_ids(self):
        report = {
            "minecraft:stone": {"states": [{"id": 0, "default": True}]},
            "minecraft:dirt": {"states": [{"id": 0, "default": True}]},
        }
        with self.assertRaisesRegex(ValueError, "duplicate"):
            exporter.export_report(report)


if __name__ == "__main__":
    unittest.main()
