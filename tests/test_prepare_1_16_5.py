import importlib.util
import json
import tempfile
import unittest
from unittest.mock import patch
import zipfile
from pathlib import Path

MODULE_PATH = Path(__file__).resolve().parents[1] / "tools" / "prepare_1_16_5.py"
SPEC = importlib.util.spec_from_file_location("prepare_1_16_5", MODULE_PATH)
prepare = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(prepare)


class Prepare1165Tests(unittest.TestCase):
    def test_profile_is_the_expected_legacy_target(self):
        self.assertEqual(prepare.VERSION, "1.16.5")
        self.assertEqual(prepare.DATA_VERSION, 2586)
        self.assertEqual(prepare.PROTOCOL_VERSION, 754)
        self.assertEqual(prepare.CLIENT_SHA1, "37fd3c903861eeff3bc24b71eed48f828b5269c8")
        self.assertEqual(prepare.SERVER_SHA1, "1b557e7b033b583cd9f66746b7a9ab1ec1673ced")

    def test_manifest_rejects_wrong_hash_before_parsing(self):
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "1.16.5.json"
            path.write_text(json.dumps({"id": "1.16.5"}), encoding="utf-8")
            with self.assertRaisesRegex(ValueError, "pinned official"):
                prepare.validate_manifest(path)

    def test_runs_vanilla_datagen_and_finds_the_block_report(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            server = root / "server.jar"
            server.write_bytes(b"test")
            def fake_run(command, cwd, stdout, stderr, check):
                self.assertEqual(command[0], "java-test")
                self.assertIn("-DbundlerMainClass=net.minecraft.data.Main", command)
                self.assertEqual(command[-3:], ["--output", "out"][-3:])
                self.assertEqual(cwd, root / "out" / "datagen")
                report = cwd / "out" / "reports" / "blocks.json"
                report.parent.mkdir(parents=True)
                report.write_text("{}", encoding="utf-8")
            with patch.object(prepare.subprocess, "run", side_effect=fake_run) as run:
                report = prepare.run_blocks_datagen(server, root / "out", "java-test")
            self.assertEqual(report, root / "out" / "datagen" / "out" / "reports" / "blocks.json")
            run.assert_called_once()

    def test_safe_extract_rejects_path_traversal(self):
        with tempfile.TemporaryDirectory() as tmp:
            archive_path = Path(tmp) / "sample.zip"
            with zipfile.ZipFile(archive_path, "w") as archive:
                archive.writestr("../escape.txt", "no")
            with zipfile.ZipFile(archive_path) as archive:
                with self.assertRaisesRegex(ValueError, "Unsafe jar entry"):
                    prepare.safe_extract(archive, ["../escape.txt"], Path(tmp) / "out")

    def test_exports_required_block_tags_from_server_jar(self):
        with tempfile.TemporaryDirectory() as tmp:
            jar = Path(tmp) / "server.jar"
            expected = {
                "data/minecraft/tags/block/blocks_motion_in_heightmap.json": '{"values":["minecraft:stone"]}',
                "data/minecraft/tags/block/blocks_motion_in_heightmap_no_leaves.json": '{"values":["minecraft:stone"]}',
                "data/minecraft/tags/block/blocks_fluid_flow.json": '{"values":["minecraft:water"]}',
                "data/minecraft/tags/block/replaceable.json": '{"values":["minecraft:grass"]}',
            }
            with zipfile.ZipFile(jar, "w") as archive:
                for name, value in expected.items():
                    archive.writestr(name, value)
                archive.writestr("data/minecraft/loot_table/ignored.json", "{}")
            tags = prepare.export_block_tags(jar)
            self.assertEqual(
                set(tags),
                {
                    "minecraft:blocks_motion_in_heightmap",
                    "minecraft:blocks_motion_in_heightmap_no_leaves",
                    "minecraft:blocks_fluid_flow",
                    "minecraft:replaceable",
                },
            )
            self.assertEqual(
                tags["minecraft:blocks_fluid_flow"],
                '{"values":["minecraft:water"]}',
            )

    def test_legacy_jar_gets_prototype_heightmap_tag_fallbacks(self):
        with tempfile.TemporaryDirectory() as tmp:
            jar = Path(tmp) / "server.jar"
            with zipfile.ZipFile(jar, "w") as archive:
                archive.writestr("data/minecraft/tags/block/planks.json", '{"values":["minecraft:oak_planks"]}')
            tags = prepare.export_block_tags(jar)
            self.assertIn("minecraft:stone", json.loads(tags["minecraft:blocks_motion_in_heightmap"])["values"])
            self.assertEqual(
                json.loads(tags["minecraft:blocks_motion_in_heightmap_no_leaves"]),
                json.loads(tags["minecraft:blocks_motion_in_heightmap"]),
            )
            self.assertEqual(
                json.loads(tags["minecraft:blocks_fluid_flow"]),
                {"values": []},
            )

    def test_safe_extract_writes_selected_resources_only(self):
        with tempfile.TemporaryDirectory() as tmp:
            archive_path = Path(tmp) / "sample.zip"
            output = Path(tmp) / "out"
            with zipfile.ZipFile(archive_path, "w") as archive:
                archive.writestr("data/minecraft/worldgen/noise_settings.json", "{}")
                archive.writestr("unrelated.txt", "ignored")
            with zipfile.ZipFile(archive_path) as archive:
                count = prepare.safe_extract(
                    archive,
                    ["data/minecraft/worldgen/noise_settings.json"],
                    output,
                )
            self.assertEqual(count, 1)
            self.assertEqual(
                (output / "data/minecraft/worldgen/noise_settings.json").read_text(),
                "{}",
            )
            self.assertFalse((output / "unrelated.txt").exists())


if __name__ == "__main__":
    unittest.main()
