#!/usr/bin/env python3
"""Fetch version-pinned public reference data for the Rust Minecraft 1.16.5 port.

This is a developer-time importer, not a game launcher. It downloads structured
worldgen JSON from mcmeta's 1.16.5-data branch and PC protocol/biome reference
files from minecraft-data's 1.16 folder (the 1.16.5 protocol version is pinned
separately). It never downloads or runs Java/JAR files.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import urllib.error
import urllib.request
from pathlib import Path

MC_META_ROOT = "https://raw.githubusercontent.com/misode/mcmeta/1.16.5-data"
MCDATA_ROOT = "https://raw.githubusercontent.com/PrismarineJS/minecraft-data/master/data/pc/1.16"
VERSION = "1.16.5"
DATA_VERSION = 2586
PROTOCOL_VERSION = 754

# Worldgen JSON files are from the exact 1.16.5 generated-data branch.
WORLDGEN_DIRS = (
    "worldgen/biome",
    "worldgen/configured_carver",
    "worldgen/configured_feature",
    "worldgen/configured_structure_feature",
    "worldgen/configured_surface_builder",
    "worldgen/noise_settings",
    "worldgen/processor_list",
    "worldgen/template_pool",
)
REFERENCE_FILES = (
    "biomes.json",
    "protocol.json",
    "version.json",
)


def fetch(url: str, timeout: int = 30) -> bytes:
    request = urllib.request.Request(
        url, headers={"User-Agent": "Horizon-1.16.5-reference-importer/1.0"}
    )
    with urllib.request.urlopen(request, timeout=timeout) as response:
        if response.status != 200:
            raise ValueError(f"HTTP {response.status} for {url}")
        return response.read()


def digest(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def json_bytes(data: bytes, source: str) -> object:
    try:
        return json.loads(data)
    except (UnicodeDecodeError, json.JSONDecodeError) as exc:
        raise ValueError(f"Invalid JSON from {source}: {exc}") from exc


def list_directory(path: str) -> list[str]:
    url = f"https://api.github.com/repos/misode/mcmeta/contents/data/minecraft/{path}?ref=1.16.5-data"
    payload = json_bytes(fetch(url), url)
    if not isinstance(payload, list):
        raise ValueError(f"Expected GitHub directory listing for {path}")
    return sorted(
        item["name"] for item in payload
        if isinstance(item, dict)
        and item.get("type") == "file"
        and item.get("name", "").endswith(".json")
    )


def write_json_file(url: str, destination: Path) -> dict:
    data = fetch(url)
    parsed = json_bytes(data, url)
    destination.parent.mkdir(parents=True, exist_ok=True)
    destination.write_bytes(data)
    return {
        "source": url,
        "sha256": digest(data),
        "bytes": len(data),
        "json_type": type(parsed).__name__,
    }


def import_data(output: Path) -> dict:
    if output.exists() and any(output.iterdir()):
        raise FileExistsError(f"Output directory must be empty: {output}")
    output.mkdir(parents=True, exist_ok=True)
    files: dict[str, dict] = {}
    counts: dict[str, int] = {}

    for relative_dir in WORLDGEN_DIRS:
        names = list_directory(relative_dir)
        if not names:
            raise ValueError(f"No JSON files found in pinned source: {relative_dir}")
        counts[relative_dir] = len(names)
        for name in names:
            relative = f"data/minecraft/{relative_dir}/{name}"
            url = f"{MC_META_ROOT}/{relative}"
            files[relative] = write_json_file(url, output / relative)

    for name in REFERENCE_FILES:
        url = f"{MCDATA_ROOT}/{name}"
        relative = f"reference/minecraft-data-1.16/{name}"
        files[relative] = write_json_file(url, output / relative)

    version_info = json_bytes((output / "reference/minecraft-data-1.16/version.json").read_bytes(),
                              "minecraft-data 1.16 version.json")
    if not isinstance(version_info, dict):
        raise ValueError("Unexpected minecraft-data version metadata")
    if int(version_info.get("version", -1)) not in (735, 736, 751, 753, 754):
        raise ValueError(f"Unexpected protocol version metadata: {version_info!r}")

    biome_dir = output / "data/minecraft/worldgen/biome"
    biome_names = sorted(p.stem for p in biome_dir.glob("*.json"))
    report = {
        "target": VERSION,
        "data_version": DATA_VERSION,
        "protocol_version": PROTOCOL_VERSION,
        "source_revisions": {
            "worldgen": "misode/mcmeta@1.16.5-data",
            "reference": "PrismarineJS/minecraft-data@master/data/pc/1.16",
        },
        "counts": counts,
        "biome_count": len(biome_names),
        "biomes": biome_names,
        "files": files,
        "status": "reference-data-imported-generator-not-yet-ported",
        "remaining_work": [
            "derive exact 1.16.5 block-state registry and defaults",
            "adapt Rust biome source and noise/density functions to 1.16.5",
            "implement carvers, surface builders, placed features and structures",
            "compare fixed-seed output against a trusted 1.16.5 reference",
        ],
    }
    (output / "horizon-1.16.5-reference-manifest.json").write_text(
        json.dumps(report, indent=2, sort_keys=True) + "\n", encoding="utf-8"
    )
    return report


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--out", required=True, type=Path, help="empty output directory")
    args = parser.parse_args()
    try:
        report = import_data(args.out)
    except (OSError, ValueError, urllib.error.URLError, TimeoutError) as exc:
        parser.exit(2, f"reference import failed: {exc}\n")
    print(f"Imported {len(report['files'])} JSON files for Minecraft {VERSION}.")
    print(f"Biome definitions: {report['biome_count']}")
    print(f"Manifest: {args.out / 'horizon-1.16.5-reference-manifest.json'}")
    print("This is reference data only; it does not yet make the client playable.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
