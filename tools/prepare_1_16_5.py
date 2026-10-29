#!/usr/bin/env python3
"""Validate a player's own Minecraft Java 1.16.5 files and stage version-specific inputs.

This deliberately does NOT create horizon-resources.json: the Rust client still needs
1.16.5-specific registry/state exporters and worldgen adapters before this data is
safe to load as a playable world.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import shutil
import sys
import tempfile
import zipfile
from pathlib import Path, PurePosixPath

VERSION = "1.16.5"
DATA_VERSION = 2586
PROTOCOL_VERSION = 754
CLIENT_SHA1 = "37fd3c903861eeff3bc24b71eed48f828b5269c8"
SERVER_SHA1 = "1b557e7b033b583cd9f66746b7a9ab1ec1673ced"
VERSION_JSON_SHA1 = "436877ffaef948954053e1a78a366b8b7c204a91"
CLIENT_URL = f"https://piston-data.mojang.com/v1/objects/{CLIENT_SHA1}/client.jar"
SERVER_URL = f"https://piston-data.mojang.com/v1/objects/{SERVER_SHA1}/server.jar"
VERSION_JSON_URL = f"https://piston-meta.mojang.com/v1/packages/{VERSION_JSON_SHA1}/{VERSION}.json"


def sha1(path: Path) -> str:
    digest = hashlib.sha1()
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def validate_manifest(path: Path) -> dict:
    actual = sha1(path)
    if actual != VERSION_JSON_SHA1:
        raise ValueError(
            f"{path} is not the pinned official {VERSION} manifest: "
            f"expected SHA-1 {VERSION_JSON_SHA1}, got {actual}"
        )
    manifest = json.loads(path.read_text(encoding="utf-8"))
    if manifest.get("id") != VERSION:
        raise ValueError(f"Manifest id must be {VERSION!r}")
    downloads = manifest.get("downloads", {})
    for name, expected in (("client", CLIENT_SHA1), ("server", SERVER_SHA1)):
        actual_hash = downloads.get(name, {}).get("sha1")
        if actual_hash != expected:
            raise ValueError(f"Manifest {name} SHA-1 mismatch: {actual_hash!r}")
    if manifest.get("javaVersion", {}).get("majorVersion") != 8:
        raise ValueError("Pinned 1.16.5 manifest does not declare Java 8")
    return manifest


def verify_jar(path: Path, expected: str, kind: str) -> None:
    if not path.is_file():
        raise FileNotFoundError(
            f"Missing {kind} jar: {path}\n"
            f"Install and run Minecraft Java {VERSION} in the official launcher, "
            f"or supply --server-jar. Official download: "
            f"{CLIENT_URL if kind == 'client' else SERVER_URL}"
        )
    actual = sha1(path)
    if actual != expected:
        raise ValueError(
            f"{path} is not the pinned {VERSION} {kind} jar: "
            f"expected SHA-1 {expected}, got {actual}"
        )
    with zipfile.ZipFile(path) as archive:
        bad = archive.testzip()
        if bad:
            raise ValueError(f"Corrupt {kind} jar entry: {bad}")


def safe_extract(archive: zipfile.ZipFile, members: list[str], destination: Path) -> int:
    """Extract only selected, normal relative paths; reject traversal and symlinks."""
    count = 0
    root = destination.resolve()
    for name in members:
        member = PurePosixPath(name)
        if member.is_absolute() or ".." in member.parts:
            raise ValueError(f"Unsafe jar entry: {name}")
        target = (destination / Path(*member.parts)).resolve()
        if target != root and root not in target.parents:
            raise ValueError(f"Jar entry escapes output directory: {name}")
        info = archive.getinfo(name)
        if (info.external_attr >> 16) & 0o170000 == 0o120000:
            raise ValueError(f"Refusing symbolic link in jar: {name}")
        target.parent.mkdir(parents=True, exist_ok=True)
        with archive.open(info) as source, target.open("wb") as output:
            shutil.copyfileobj(source, output)
        count += 1
    return count


def stage(client: Path, server: Path, manifest_path: Path, output: Path) -> dict:
    manifest = validate_manifest(manifest_path)
    verify_jar(client, CLIENT_SHA1, "client")
    verify_jar(server, SERVER_SHA1, "server")
    if output.exists() and any(output.iterdir()):
        raise FileExistsError(f"Output directory must be empty: {output}")
    output.mkdir(parents=True, exist_ok=True)
    assets_dir = output / "client-assets"
    data_dir = output / "server-data"
    with zipfile.ZipFile(client) as archive:
        asset_members = [n for n in archive.namelist()
                         if n.startswith("assets/minecraft/") and not n.endswith("/")]
        asset_count = safe_extract(archive, asset_members, assets_dir)
    with zipfile.ZipFile(server) as archive:
        # Vanilla worldgen JSON, tags, dimensions, loot tables and other data-driven inputs.
        data_members = [n for n in archive.namelist()
                        if n.startswith("data/minecraft/") and not n.endswith("/")]
        if not data_members:
            raise ValueError(
                "The 1.16.5 server jar contains no data/minecraft resources; "
                "the bundled-jar layout may have changed and must be handled explicitly."
            )
        data_count = safe_extract(archive, data_members, data_dir)
    result = {
        "format": 1,
        "minecraft_version": VERSION,
        "data_version": DATA_VERSION,
        "protocol_version": PROTOCOL_VERSION,
        "java_major": 8,
        "source": "official-launcher-install",
        "client_sha1": CLIENT_SHA1,
        "server_sha1": SERVER_SHA1,
        "version_json_sha1": VERSION_JSON_SHA1,
        "asset_index": manifest.get("assetIndex", {}).get("id"),
        "extracted_client_asset_files": asset_count,
        "extracted_server_data_files": data_count,
        "status": "inputs-staged-not-playable-resource-bundle",
        "missing_work": [
            "1.16.5 block-state and registry facts exporter",
            "1.16.5 biome registry and biome-source adapter",
            "1.16.5 noise settings, density functions, carvers and feature placement adapter",
            "1.16.5 chunk section layout, heightmaps, lighting and spawn integration",
            "1.16.5 save/load NBT and data-fixer compatibility",
        ],
    }
    (output / "horizon-1.16.5-inputs.json").write_text(
        json.dumps(result, indent=2) + "\n", encoding="utf-8"
    )
    return result


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--minecraft-dir", type=Path,
                        help="Minecraft game directory (default: ~/.minecraft or platform equivalent)")
    parser.add_argument("--client-jar", type=Path,
                        help="override the official launcher's 1.16.5 client jar")
    parser.add_argument("--server-jar", type=Path, required=True,
                        help="the official 1.16.5 server jar downloaded by the user")
    parser.add_argument("--manifest", type=Path,
                        help="override versions/1.16.5/1.16.5.json")
    parser.add_argument("--out", type=Path, required=True,
                        help="new/empty directory for staged input data")
    args = parser.parse_args()
    if args.minecraft_dir:
        game_dir = args.minecraft_dir.expanduser()
    elif sys.platform == "win32":
        import os
        game_dir = Path(os.environ.get("APPDATA", Path.home())) / ".minecraft"
    elif sys.platform == "darwin":
        game_dir = Path.home() / "Library" / "Application Support" / "minecraft"
    else:
        game_dir = Path.home() / ".minecraft"
    manifest = args.manifest or game_dir / "versions" / VERSION / f"{VERSION}.json"
    client = args.client_jar or game_dir / "versions" / VERSION / f"{VERSION}.jar"
    try:
        result = stage(client, args.server_jar.expanduser(), manifest.expanduser(), args.out.expanduser())
    except (OSError, ValueError, zipfile.BadZipFile, json.JSONDecodeError) as exc:
        print(f"1.16.5 resource staging failed: {exc}", file=sys.stderr)
        return 1
    print(json.dumps(result, indent=2))
    print("\nStaging complete. This is not yet a playable Horizon resource bundle.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
